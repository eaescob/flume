use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::{ScriptAction, ScriptEvent};

/// Shared state for the Python runtime.
struct PySharedState {
    event_handlers: HashMap<String, Vec<(String, PyObject)>>,
    custom_commands: HashMap<String, (String, PyObject, String)>,
    actions: Vec<ScriptAction>,
    current_script: String,
    vault_secrets: HashMap<String, String>,
}

/// The bridge object exposed to Python as `_flume_bridge`.
/// Python scripts call methods on this to register handlers and queue actions.
#[pyclass]
struct FlumeBridge {
    state: Arc<Mutex<PySharedState>>,
}

#[pymethods]
impl FlumeBridge {
    fn event_on(&self, event_name: String, callback: PyObject) {
        let mut s = self.state.lock().unwrap();
        let script = s.current_script.clone();
        s.event_handlers
            .entry(event_name)
            .or_default()
            .push((script, callback));
    }

    fn event_off(&self, event_name: String) {
        let mut s = self.state.lock().unwrap();
        let script = s.current_script.clone();
        if let Some(handlers) = s.event_handlers.get_mut(&event_name) {
            handlers.retain(|(name, _)| *name != script);
        }
    }

    fn command_register(&self, name: String, callback: PyObject, help_text: String) {
        let mut s = self.state.lock().unwrap();
        let script = s.current_script.clone();
        s.custom_commands
            .insert(name, (script, callback, help_text));
    }

    fn command_unregister(&self, name: String) {
        self.state.lock().unwrap().custom_commands.remove(&name);
    }

    fn buffer_print(&self, server: String, buffer: String, text: String) {
        self.state
            .lock()
            .unwrap()
            .actions
            .push(ScriptAction::PrintToBuffer {
                server,
                buffer,
                text,
            });
    }

    fn buffer_switch(&self, buffer: String) {
        self.state
            .lock()
            .unwrap()
            .actions
            .push(ScriptAction::SwitchBuffer { buffer });
    }

    fn channel_say(&self, server: String, target: String, text: String) {
        self.state
            .lock()
            .unwrap()
            .actions
            .push(ScriptAction::SendMessage {
                server,
                target,
                text,
            });
    }

    #[pyo3(signature = (server, channel, key=None))]
    fn channel_join(&self, server: String, channel: String, key: Option<String>) {
        self.state
            .lock()
            .unwrap()
            .actions
            .push(ScriptAction::JoinChannel {
                server,
                channel,
                key,
            });
    }

    #[pyo3(signature = (server, channel, message=None))]
    fn channel_part(&self, server: String, channel: String, message: Option<String>) {
        self.state
            .lock()
            .unwrap()
            .actions
            .push(ScriptAction::PartChannel {
                server,
                channel,
                message,
            });
    }

    fn server_send_raw(&self, server: String, line: String) {
        self.state
            .lock()
            .unwrap()
            .actions
            .push(ScriptAction::SendRaw { server, line });
    }

    #[pyo3(signature = (message, level=None))]
    fn ui_notify(&self, message: String, level: Option<String>) {
        self.state
            .lock()
            .unwrap()
            .actions
            .push(ScriptAction::Notify {
                message,
                level: level.unwrap_or_else(|| "info".to_string()),
            });
    }

    fn vault_get(&self, py: Python<'_>, name: String) -> PyObject {
        let s = self.state.lock().unwrap();
        match s.vault_secrets.get(&name) {
            Some(val) => val.into_pyobject(py).unwrap().into_any().unbind(),
            None => py.None(),
        }
    }

    fn config_get(&self, py: Python<'_>, key: String) -> PyObject {
        let script_name = self.state.lock().unwrap().current_script.clone();
        if script_name.is_empty() {
            return py.None();
        }
        let path = super::script_data_dir(&script_name).join("config.toml");
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        let table: toml::Table = toml::from_str(&contents).unwrap_or_default();
        match table.get(&key) {
            Some(toml::Value::String(s)) => s.into_pyobject(py).unwrap().into_any().unbind(),
            Some(toml::Value::Integer(n)) => n.into_pyobject(py).unwrap().into_any().unbind(),
            Some(toml::Value::Float(f)) => f.into_pyobject(py).unwrap().into_any().unbind(),
            Some(toml::Value::Boolean(b)) => {
                let py_bool = (*b).into_pyobject(py).unwrap();
                py_bool.to_owned().into_any().unbind()
            }
            _ => py.None(),
        }
    }

    fn config_set(&self, key: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let script_name = self.state.lock().unwrap().current_script.clone();
        if script_name.is_empty() {
            return Ok(());
        }
        let path = super::script_data_dir(&script_name).join("config.toml");
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        let mut table: toml::Table = toml::from_str(&contents).unwrap_or_default();

        if let Ok(s) = value.extract::<String>() {
            table.insert(key, toml::Value::String(s));
        } else if let Ok(n) = value.extract::<i64>() {
            table.insert(key, toml::Value::Integer(n));
        } else if let Ok(f) = value.extract::<f64>() {
            table.insert(key, toml::Value::Float(f));
        } else if let Ok(b) = value.extract::<bool>() {
            table.insert(key, toml::Value::Boolean(b));
        }

        let toml_str = toml::to_string_pretty(&table).unwrap_or_default();
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(&path, toml_str);
        Ok(())
    }
}

/// Python bootstrap code that creates the `flume` module facade.
const BOOTSTRAP: &str = r#"
import sys, types

_b = _flume_bridge

class _Event:
    @staticmethod
    def on(event_name, callback):
        _flume_bridge.event_on(event_name, callback)
    @staticmethod
    def off(event_name):
        _flume_bridge.event_off(event_name)

class _Server:
    @staticmethod
    def send_raw(server, line):
        _flume_bridge.server_send_raw(server, line)

class _Channel:
    @staticmethod
    def say(server, target, text):
        _flume_bridge.channel_say(server, target, text)
    @staticmethod
    def join(server, channel, key=None):
        _flume_bridge.channel_join(server, channel, key)
    @staticmethod
    def part(server, channel, message=None):
        _flume_bridge.channel_part(server, channel, message)

class _Buffer:
    @staticmethod
    def print(server, buffer, text):
        _flume_bridge.buffer_print(server, buffer, text)
    @staticmethod
    def switch(buffer):
        _flume_bridge.buffer_switch(buffer)

class _Command:
    @staticmethod
    def register(name, callback, help_text=""):
        _flume_bridge.command_register(name, callback, help_text)
    @staticmethod
    def unregister(name):
        _flume_bridge.command_unregister(name)

class _Config:
    @staticmethod
    def get(key):
        return _flume_bridge.config_get(key)
    @staticmethod
    def set(key, value):
        _flume_bridge.config_set(key, value)

class _Ui:
    @staticmethod
    def notify(message, level=None):
        _flume_bridge.ui_notify(message, level)

class _Vault:
    @staticmethod
    def get(name):
        return _flume_bridge.vault_get(name)

flume_mod = types.ModuleType("flume")
flume_mod.version = __FLUME_VERSION__
flume_mod.event = _Event()
flume_mod.server = _Server()
flume_mod.channel = _Channel()
flume_mod.buffer = _Buffer()
flume_mod.command = _Command()
flume_mod.config = _Config()
flume_mod.ui = _Ui()
flume_mod.vault = _Vault()

sys.modules["flume"] = flume_mod
sys.modules["flume.event"] = flume_mod.event
sys.modules["flume.server"] = flume_mod.server
sys.modules["flume.channel"] = flume_mod.channel
sys.modules["flume.buffer"] = flume_mod.buffer
sys.modules["flume.command"] = flume_mod.command
sys.modules["flume.vault"] = flume_mod.vault
sys.modules["flume.config"] = flume_mod.config
sys.modules["flume.ui"] = flume_mod.ui
"#;

/// Python runtime wrapping PyO3.
pub struct PyRuntime {
    state: Arc<Mutex<PySharedState>>,
}

impl PyRuntime {
    pub fn new() -> Result<Self, PyErr> {
        let state = Arc::new(Mutex::new(PySharedState {
            event_handlers: HashMap::new(),
            custom_commands: HashMap::new(),
            actions: Vec::new(),
            current_script: String::new(),
            vault_secrets: HashMap::new(),
        }));

        Python::with_gil(|py| {
            let bridge = Py::new(
                py,
                FlumeBridge {
                    state: Arc::clone(&state),
                },
            )?;
            // Set the bridge as a global so the bootstrap can find it
            let globals = py.import("builtins")?.dict();
            globals.set_item("_flume_bridge", bridge)?;

            // Run bootstrap to set up the flume module (substitute version)
            let bootstrap = BOOTSTRAP.replace(
                "__FLUME_VERSION__",
                &format!("\"{}\"", env!("CARGO_PKG_VERSION")),
            );
            py.run(
                std::ffi::CString::new(bootstrap).unwrap().as_c_str(),
                None,
                None,
            )?;

            // Keep _flume_bridge in builtins — the flume module facade references it
            Ok::<(), PyErr>(())
        })?;

        Ok(Self { state })
    }

    pub fn exec_script(&self, name: &str, source: &str) -> Result<(), PyErr> {
        self.state.lock().unwrap().current_script = name.to_string();
        Python::with_gil(|py| -> Result<(), PyErr> {
            py.run(
                std::ffi::CString::new(source).map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
                })?.as_c_str(),
                None,
                None,
            )?;
            Ok(())
        })?;
        self.state.lock().unwrap().current_script.clear();
        Ok(())
    }

    pub fn remove_script_handlers(&self, script_name: &str) {
        let mut state = self.state.lock().unwrap();
        for handlers in state.event_handlers.values_mut() {
            handlers.retain(|(name, _)| name != script_name);
        }
        state
            .custom_commands
            .retain(|_, (name, _, _)| name != script_name);
    }

    pub fn dispatch_event(&self, mut event: ScriptEvent) -> ScriptEvent {
        Python::with_gil(|py| {
            let handlers: Vec<(String, PyObject)> = {
                let state = self.state.lock().unwrap();
                state
                    .event_handlers
                    .get(&event.name)
                    .map(|h| {
                        h.iter()
                            .map(|(name, obj)| (name.clone(), obj.clone_ref(py)))
                            .collect()
                    })
                    .unwrap_or_default()
            };

            if handlers.is_empty() {
                return;
            }

            for (_script_name, handler) in &handlers {
                if event.cancelled {
                    break;
                }

                let result: PyResult<()> = (|| {
                    let dict = PyDict::new(py);
                    dict.set_item("name", &event.name)?;
                    dict.set_item("server", &event.server)?;
                    dict.set_item("_cancel", false)?;
                    for (k, v) in &event.fields {
                        dict.set_item(k, v)?;
                    }

                    handler.call1(py, (&dict,))?;

                    if let Ok(Some(val)) = dict.get_item("_cancel") {
                        if let Ok(b) = val.extract::<bool>() {
                            if b {
                                event.cancelled = true;
                            }
                        }
                    }

                    Ok(())
                })();

                if let Err(e) = result {
                    tracing::warn!("Python event handler error: {}", e);
                }
            }
        });

        event
    }

    pub fn drain_actions(&self) -> Vec<ScriptAction> {
        std::mem::take(&mut self.state.lock().unwrap().actions)
    }

    pub fn execute_command(&self, name: &str, args: &str) -> bool {
        self.execute_command_with_error(name, args).is_some()
    }



    /// Execute a command and return Some(Result) if the command exists.
    /// Returns Some(Ok) on success, Some(Err(msg)) on Python error, None if not found.
    pub fn execute_command_with_error(&self, name: &str, args: &str) -> Option<Result<(), String>> {
        Python::with_gil(|py| {
            let handler = {
                self.state
                    .lock()
                    .unwrap()
                    .custom_commands
                    .get(name)
                    .map(|(_, f, _)| f.clone_ref(py))
            };

            if let Some(handler) = handler {
                match handler.call1(py, (args,)) {
                    Ok(_) => Some(Ok(())),
                    Err(e) => {
                        // Format the Python error with traceback
                        let msg = format_py_error(py, &e);
                        tracing::warn!("Python command '{}' error: {}", name, msg);
                        Some(Err(msg))
                    }
                }
            } else {
                None
            }
        })
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.state.lock().unwrap().custom_commands.contains_key(name)
    }

    pub fn command_help(&self, name: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .custom_commands
            .get(name)
            .map(|(_, _, help)| help.clone())
            .filter(|h| !h.is_empty())
    }

    pub fn set_vault_secrets(&self, secrets: HashMap<String, String>) {
        self.state.lock().unwrap().vault_secrets = secrets;
    }

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

/// Format a Python error with traceback for user display.
fn format_py_error(py: Python<'_>, err: &PyErr) -> String {
    // Try to get the traceback
    let tb = err.traceback(py);
    let value_str = err.value(py).to_string();
    let type_name = err.get_type(py).name().map(|s| s.to_string()).unwrap_or_else(|_| "Error".to_string());

    let mut msg = format!("{}: {}", type_name, value_str);
    if let Some(tb) = tb {
        if let Ok(formatted) = tb.format() {
            msg.push('\n');
            msg.push_str(&formatted);
        }
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn py_runtime_creates() {
        let rt = PyRuntime::new();
        assert!(rt.is_ok());
    }

    #[test]
    fn py_exec_simple() {
        let rt = PyRuntime::new().unwrap();
        assert!(rt.exec_script("test", "x = 1 + 1").is_ok());
    }

    #[test]
    fn py_exec_syntax_error() {
        let rt = PyRuntime::new().unwrap();
        assert!(rt.exec_script("bad", "def !!!!").is_err());
    }

    #[test]
    fn py_event_handler() {
        let rt = PyRuntime::new().unwrap();
        rt.exec_script(
            "test",
            "import flume\ndef on_msg(e):\n    flume.buffer.print('', '', 'got: ' + e['text'])\nflume.event.on('message', on_msg)",
        )
        .unwrap();

        let event = ScriptEvent::new("message", "libera").field("text", "hello");
        let result = rt.dispatch_event(event);
        assert!(!result.cancelled);

        let actions = rt.drain_actions();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ScriptAction::PrintToBuffer { text, .. } => assert_eq!(text, "got: hello"),
            _ => panic!("Expected PrintToBuffer"),
        }
    }

    #[test]
    fn py_custom_command() {
        let rt = PyRuntime::new().unwrap();
        rt.exec_script(
            "test",
            "import flume\ndef greet(args):\n    flume.buffer.print('', '', 'Hello ' + args)\nflume.command.register('greet', greet, 'Greet')",
        )
        .unwrap();

        assert!(rt.has_command("greet"));
        assert!(rt.execute_command("greet", "world"));

        let actions = rt.drain_actions();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ScriptAction::PrintToBuffer { text, .. } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected PrintToBuffer"),
        }
    }

    #[test]
    fn py_full_import_access() {
        let rt = PyRuntime::new().unwrap();
        assert!(rt.exec_script("test", "import json\nimport os.path\nimport re").is_ok());
    }

    #[test]
    fn py_remove_handlers() {
        let rt = PyRuntime::new().unwrap();
        rt.exec_script(
            "myscript",
            "import flume\nflume.event.on('message', lambda e: None)\nflume.command.register('mycmd', lambda a: None, 'test')",
        )
        .unwrap();

        assert!(rt.has_command("mycmd"));
        rt.remove_script_handlers("myscript");
        assert!(!rt.has_command("mycmd"));
    }
}
