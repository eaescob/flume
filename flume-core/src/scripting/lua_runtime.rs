use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mlua::{Function, Lua, Result as LuaResult};

use super::sandbox;
use super::{ScriptAction, ScriptEvent};

/// Shared mutable state accessible from Lua callbacks.
pub(crate) struct SharedState {
    /// Event handlers: event_name → Vec<(script_name, handler)>
    pub(crate) event_handlers: HashMap<String, Vec<(String, Function)>>,
    /// Custom commands: command_name → (script_name, handler, help_text)
    pub(crate) custom_commands: HashMap<String, (String, Function, String)>,
    /// Pending actions queued by scripts.
    pub(crate) actions: Vec<ScriptAction>,
    /// The currently executing script name (set during exec_script).
    pub(crate) current_script: String,
}

/// Wraps the Lua VM and manages script state.
pub struct LuaRuntime {
    lua: Lua,
    state: Arc<Mutex<SharedState>>,
}

impl LuaRuntime {
    pub fn new() -> LuaResult<Self> {
        let lua = Lua::new();

        let state = Arc::new(Mutex::new(SharedState {
            event_handlers: HashMap::new(),
            custom_commands: HashMap::new(),
            actions: Vec::new(),
            current_script: String::new(),
        }));

        // Apply sandbox restrictions
        sandbox::apply_sandbox(&lua)?;

        // Register the flume API
        super::api::register_all(&lua, Arc::clone(&state))?;

        Ok(Self { lua, state })
    }

    /// Execute a script's source code in the Lua VM.
    pub fn exec_script(&self, name: &str, source: &str) -> LuaResult<()> {
        self.state.lock().unwrap().current_script = name.to_string();
        self.lua.load(source).set_name(name).exec()?;
        self.state.lock().unwrap().current_script.clear();
        Ok(())
    }

    /// Remove all event handlers and commands registered by a script.
    pub fn remove_script_handlers(&self, script_name: &str) {
        let mut state = self.state.lock().unwrap();
        for handlers in state.event_handlers.values_mut() {
            handlers.retain(|(name, _)| name != script_name);
        }
        state
            .custom_commands
            .retain(|_, (name, _, _)| name != script_name);
    }

    /// Dispatch an event to all registered handlers.
    pub fn dispatch_event(&self, mut event: ScriptEvent) -> ScriptEvent {
        let handlers: Vec<(String, Function)> = {
            let state = self.state.lock().unwrap();
            state
                .event_handlers
                .get(&event.name)
                .cloned()
                .unwrap_or_default()
        };

        for (_script_name, handler) in &handlers {
            if event.cancelled {
                break;
            }

            let result: LuaResult<()> = (|| {
                let tbl = self.lua.create_table()?;
                tbl.set("name", event.name.as_str())?;
                tbl.set("server", event.server.as_str())?;
                tbl.set("cancelled", event.cancelled)?;
                for (k, v) in &event.fields {
                    tbl.set(k.as_str(), v.as_str())?;
                }

                // Add cancel() method
                let cancelled_flag = Arc::new(Mutex::new(false));
                let flag_clone = Arc::clone(&cancelled_flag);
                let cancel_fn = self.lua.create_function(move |_, ()| {
                    *flag_clone.lock().unwrap() = true;
                    Ok(())
                })?;
                tbl.set("cancel", cancel_fn)?;

                handler.call::<()>(tbl)?;

                if *cancelled_flag.lock().unwrap() {
                    event.cancelled = true;
                }

                Ok(())
            })();

            if let Err(e) = result {
                tracing::warn!("Script event handler error: {}", e);
            }
        }

        event
    }

    /// Drain all pending actions.
    pub fn drain_actions(&self) -> Vec<ScriptAction> {
        let mut state = self.state.lock().unwrap();
        std::mem::take(&mut state.actions)
    }

    /// Execute a custom command. Returns true if the command was found.
    pub fn execute_command(&self, name: &str, args: &str) -> bool {
        let handler = {
            let state = self.state.lock().unwrap();
            state
                .custom_commands
                .get(name)
                .map(|(_, f, _)| f.clone())
        };

        if let Some(handler) = handler {
            if let Err(e) = handler.call::<()>(args.to_string()) {
                tracing::warn!("Script command '{}' error: {}", name, e);
            }
            true
        } else {
            false
        }
    }

    /// Check if a custom command is registered.
    pub fn has_command(&self, name: &str) -> bool {
        self.state.lock().unwrap().custom_commands.contains_key(name)
    }

    /// Get all custom command names.
    pub fn custom_command_names(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .custom_commands
            .keys()
            .cloned()
            .collect()
    }
}

/// Expose State type for the API module.
pub(crate) type State = Arc<Mutex<SharedState>>;

/// Helper to push an action onto the shared state.
pub(crate) fn push_action(state: &State, action: ScriptAction) {
    state.lock().unwrap().actions.push(action);
}

/// Helper to register an event handler.
pub(crate) fn add_event_handler(state: &State, event_name: &str, handler: Function) {
    let script_name = state.lock().unwrap().current_script.clone();
    state
        .lock()
        .unwrap()
        .event_handlers
        .entry(event_name.to_string())
        .or_default()
        .push((script_name, handler));
}

/// Helper to register a custom command.
pub(crate) fn add_custom_command(
    state: &State,
    name: &str,
    handler: Function,
    help_text: &str,
) {
    let script_name = state.lock().unwrap().current_script.clone();
    state.lock().unwrap().custom_commands.insert(
        name.to_string(),
        (script_name, handler, help_text.to_string()),
    );
}

/// Helper to remove a custom command.
pub(crate) fn remove_custom_command(state: &State, name: &str) -> bool {
    state.lock().unwrap().custom_commands.remove(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_runtime_creates() {
        let rt = LuaRuntime::new();
        assert!(rt.is_ok());
    }

    #[test]
    fn exec_simple_script() {
        let rt = LuaRuntime::new().unwrap();
        let result = rt.exec_script("test", "local x = 1 + 1");
        assert!(result.is_ok());
    }

    #[test]
    fn exec_script_syntax_error() {
        let rt = LuaRuntime::new().unwrap();
        let result = rt.exec_script("bad", "local x = !!!!");
        assert!(result.is_err());
    }

    #[test]
    fn event_handler_registration_and_dispatch() {
        let rt = LuaRuntime::new().unwrap();
        rt.exec_script(
            "test",
            r#"
            flume.event.on("message", function(e)
                flume.buffer.print("", "", "got: " .. e.text)
            end)
            "#,
        )
        .unwrap();

        let event = ScriptEvent::new("message", "libera").field("text", "hello");
        let result = rt.dispatch_event(event);
        assert!(!result.cancelled);

        let actions = rt.drain_actions();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ScriptAction::PrintToBuffer { text, .. } => {
                assert_eq!(text, "got: hello");
            }
            _ => panic!("Expected PrintToBuffer action"),
        }
    }

    #[test]
    fn event_cancellation() {
        let rt = LuaRuntime::new().unwrap();
        rt.exec_script(
            "test",
            r#"
            flume.event.on("message", function(e)
                e:cancel()
            end)
            "#,
        )
        .unwrap();

        let event = ScriptEvent::new("message", "test");
        let result = rt.dispatch_event(event);
        assert!(result.cancelled);
    }

    #[test]
    fn custom_command_registration() {
        let rt = LuaRuntime::new().unwrap();
        rt.exec_script(
            "test",
            r#"
            flume.command.register("greet", function(args)
                flume.buffer.print("", "", "Hello " .. args)
            end, "Greet someone")
            "#,
        )
        .unwrap();

        assert!(rt.has_command("greet"));
        assert!(rt.execute_command("greet", "world"));

        let actions = rt.drain_actions();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ScriptAction::PrintToBuffer { text, .. } => {
                assert_eq!(text, "Hello world");
            }
            _ => panic!("Expected PrintToBuffer"),
        }
    }

    #[test]
    fn remove_script_handlers() {
        let rt = LuaRuntime::new().unwrap();
        rt.exec_script(
            "myscript",
            r#"
            flume.event.on("message", function(e) end)
            flume.command.register("mycmd", function(args) end, "test")
            "#,
        )
        .unwrap();

        assert!(rt.has_command("mycmd"));
        rt.remove_script_handlers("myscript");
        assert!(!rt.has_command("mycmd"));
    }
}
