use std::sync::Arc;

use mlua::{Function, Lua, Result as LuaResult, Table, Value};

use super::lua_runtime::{self, State};
use super::ScriptAction;

/// Register all flume.* API namespaces.
pub(crate) fn register_all(lua: &Lua, state: State) -> LuaResult<()> {
    let flume = lua.create_table()?;

    // Expose Flume version as a string
    flume.set("version", env!("CARGO_PKG_VERSION"))?;

    register_event_api(lua, &flume, Arc::clone(&state))?;
    register_server_api(lua, &flume, Arc::clone(&state))?;
    register_channel_api(lua, &flume, Arc::clone(&state))?;
    register_buffer_api(lua, &flume, Arc::clone(&state))?;
    register_command_api(lua, &flume, Arc::clone(&state))?;
    register_config_api(lua, &flume, Arc::clone(&state))?;
    register_ui_api(lua, &flume, Arc::clone(&state))?;
    register_vault_api(lua, &flume, Arc::clone(&state))?;

    lua.globals().set("flume", flume)?;
    Ok(())
}

/// flume.vault — read-only access to vault secrets.
fn register_vault_api(lua: &Lua, flume: &Table, state: State) -> LuaResult<()> {
    let vault_tbl = lua.create_table()?;

    // flume.vault.get(name) — read a vault secret
    let state_get = Arc::clone(&state);
    vault_tbl.set(
        "get",
        lua.create_function(move |lua, name: String| -> LuaResult<Value> {
            let s = state_get.lock().unwrap();
            match s.vault_secrets.get(&name) {
                Some(val) => Ok(Value::String(lua.create_string(val)?)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;

    flume.set("vault", vault_tbl)?;
    Ok(())
}

/// flume.event — subscribe to and emit events.
fn register_event_api(lua: &Lua, flume: &Table, state: State) -> LuaResult<()> {
    let event_tbl = lua.create_table()?;

    // flume.event.on(event_name, callback)
    let state_on = Arc::clone(&state);
    event_tbl.set(
        "on",
        lua.create_function(move |_, (event_name, callback): (String, Function)| {
            lua_runtime::add_event_handler(&state_on, &event_name, callback);
            Ok(())
        })?,
    )?;

    // flume.event.off(event_name) — removes all handlers for this event from current script
    let state_off = Arc::clone(&state);
    event_tbl.set(
        "off",
        lua.create_function(move |_, event_name: String| {
            let mut s = state_off.lock().unwrap();
            let current = s.current_script.clone();
            if let Some(handlers) = s.event_handlers.get_mut(&event_name) {
                handlers.retain(|(name, _)| *name != current);
            }
            Ok(())
        })?,
    )?;

    // flume.event.emit(event_name, data_table) — emit a custom event
    // Note: custom events are dispatched immediately through the handler chain.
    // This is a no-op in terms of ScriptAction — it just calls other handlers.
    let state_emit = Arc::clone(&state);
    event_tbl.set(
        "emit",
        lua.create_function(move |_, (_event_name, _data): (String, Value)| {
            // Custom event emission — for now, just logs.
            // Full implementation would re-dispatch through the runtime.
            let _ = &state_emit;
            Ok(())
        })?,
    )?;

    flume.set("event", event_tbl)?;
    Ok(())
}

/// flume.server — server connection operations.
fn register_server_api(lua: &Lua, flume: &Table, state: State) -> LuaResult<()> {
    let server_tbl = lua.create_table()?;

    // flume.server.send_raw(server, raw_line)
    let state_raw = Arc::clone(&state);
    server_tbl.set(
        "send_raw",
        lua.create_function(move |_, (server, line): (String, String)| {
            lua_runtime::push_action(
                &state_raw,
                ScriptAction::SendRaw { server, line },
            );
            Ok(())
        })?,
    )?;

    // flume.server.list() — returns list of server names (stub: returns empty)
    server_tbl.set(
        "list",
        lua.create_function(|lua, ()| {
            let tbl = lua.create_table()?;
            // Server list is populated at dispatch time; scripts get it from events.
            Ok(tbl)
        })?,
    )?;

    // flume.server.connect(name) — queued as action
    let state_connect = Arc::clone(&state);
    server_tbl.set(
        "connect",
        lua.create_function(move |_, _name: String| {
            // Connection is managed by the TUI; scripts request via PrintToBuffer for now
            let _ = &state_connect;
            Ok(())
        })?,
    )?;

    // flume.server.disconnect(name) — queued as action
    let state_dc = Arc::clone(&state);
    server_tbl.set(
        "disconnect",
        lua.create_function(move |_, _name: String| {
            let _ = &state_dc;
            Ok(())
        })?,
    )?;

    flume.set("server", server_tbl)?;
    Ok(())
}

/// flume.channel — channel operations.
fn register_channel_api(lua: &Lua, flume: &Table, state: State) -> LuaResult<()> {
    let chan_tbl = lua.create_table()?;

    // flume.channel.join(server, channel, key?)
    let state_join = Arc::clone(&state);
    chan_tbl.set(
        "join",
        lua.create_function(move |_, (server, channel, key): (String, String, Option<String>)| {
            lua_runtime::push_action(
                &state_join,
                ScriptAction::JoinChannel { server, channel, key },
            );
            Ok(())
        })?,
    )?;

    // flume.channel.part(server, channel, message?)
    let state_part = Arc::clone(&state);
    chan_tbl.set(
        "part",
        lua.create_function(
            move |_, (server, channel, message): (String, String, Option<String>)| {
                lua_runtime::push_action(
                    &state_part,
                    ScriptAction::PartChannel {
                        server,
                        channel,
                        message,
                    },
                );
                Ok(())
            },
        )?,
    )?;

    // flume.channel.say(server, target, message)
    let state_say = Arc::clone(&state);
    chan_tbl.set(
        "say",
        lua.create_function(move |_, (server, target, text): (String, String, String)| {
            lua_runtime::push_action(
                &state_say,
                ScriptAction::SendMessage {
                    server,
                    target,
                    text,
                },
            );
            Ok(())
        })?,
    )?;

    // flume.channel.action(server, target, message)
    let state_action = Arc::clone(&state);
    chan_tbl.set(
        "action",
        lua.create_function(move |_, (server, target, text): (String, String, String)| {
            lua_runtime::push_action(
                &state_action,
                ScriptAction::SendRaw {
                    server,
                    line: format!("PRIVMSG {} :\x01ACTION {}\x01", target, text),
                },
            );
            Ok(())
        })?,
    )?;

    // flume.channel.topic(server, channel) — returns empty string (state not accessible from Lua)
    chan_tbl.set(
        "topic",
        lua.create_function(|_, (_server, _channel): (String, String)| {
            Ok(String::new())
        })?,
    )?;

    // flume.channel.names(server, channel) — returns empty table
    chan_tbl.set(
        "names",
        lua.create_function(|lua, (_server, _channel): (String, String)| {
            lua.create_table()
        })?,
    )?;

    flume.set("channel", chan_tbl)?;
    Ok(())
}

/// flume.buffer — buffer/window operations.
fn register_buffer_api(lua: &Lua, flume: &Table, state: State) -> LuaResult<()> {
    let buf_tbl = lua.create_table()?;

    // flume.buffer.print(server, buffer, text)
    let state_print = Arc::clone(&state);
    buf_tbl.set(
        "print",
        lua.create_function(move |_, (server, buffer, text): (String, String, String)| {
            lua_runtime::push_action(
                &state_print,
                ScriptAction::PrintToBuffer {
                    server,
                    buffer,
                    text,
                },
            );
            Ok(())
        })?,
    )?;

    // flume.buffer.current() — returns empty string (populated at dispatch time)
    buf_tbl.set(
        "current",
        lua.create_function(|_, ()| Ok(String::new()))?,
    )?;

    // flume.buffer.switch(buffer_name)
    let state_switch = Arc::clone(&state);
    buf_tbl.set(
        "switch",
        lua.create_function(move |_, buffer: String| {
            lua_runtime::push_action(
                &state_switch,
                ScriptAction::SwitchBuffer { buffer },
            );
            Ok(())
        })?,
    )?;

    // flume.buffer.scroll(direction, amount) — no-op for now
    buf_tbl.set(
        "scroll",
        lua.create_function(|_, (_direction, _amount): (String, i32)| Ok(()))?,
    )?;

    // flume.buffer.search(pattern) — no-op for now
    buf_tbl.set(
        "search",
        lua.create_function(|_, _pattern: String| Ok(()))?,
    )?;

    flume.set("buffer", buf_tbl)?;
    Ok(())
}

/// flume.command — register custom slash commands.
fn register_command_api(lua: &Lua, flume: &Table, state: State) -> LuaResult<()> {
    let cmd_tbl = lua.create_table()?;

    // flume.command.register(name, callback, help_text)
    let state_reg = Arc::clone(&state);
    cmd_tbl.set(
        "register",
        lua.create_function(
            move |_, (name, callback, help_text): (String, Function, String)| {
                lua_runtime::add_custom_command(&state_reg, &name, callback, &help_text);
                Ok(())
            },
        )?,
    )?;

    // flume.command.unregister(name)
    let state_unreg = Arc::clone(&state);
    cmd_tbl.set(
        "unregister",
        lua.create_function(move |_, name: String| {
            lua_runtime::remove_custom_command(&state_unreg, &name);
            Ok(())
        })?,
    )?;

    flume.set("command", cmd_tbl)?;
    Ok(())
}

/// flume.config — read/write script-specific config.
fn register_config_api(lua: &Lua, flume: &Table, state: State) -> LuaResult<()> {
    let cfg_tbl = lua.create_table()?;

    // flume.config.get(key) — reads from script's config file
    let state_get = Arc::clone(&state);
    cfg_tbl.set(
        "get",
        lua.create_function(move |lua, key: String| -> LuaResult<Value> {
            let script_name = state_get.lock().unwrap().current_script.clone();
            if script_name.is_empty() {
                return Ok(Value::Nil);
            }
            let path = super::script_data_dir(&script_name).join("config.toml");
            let contents = std::fs::read_to_string(&path).unwrap_or_default();
            let table: toml::Table = toml::from_str(&contents).unwrap_or_default();
            match table.get(&key) {
                Some(toml::Value::String(s)) => {
                    Ok(Value::String(lua.create_string(s)?))
                }
                Some(toml::Value::Integer(n)) => Ok(Value::Integer(*n)),
                Some(toml::Value::Float(f)) => Ok(Value::Number(*f)),
                Some(toml::Value::Boolean(b)) => Ok(Value::Boolean(*b)),
                _ => Ok(Value::Nil),
            }
        })?,
    )?;

    // flume.config.set(key, value)
    let _state_set = Arc::clone(&state);
    cfg_tbl.set(
        "set",
        lua.create_function(move |_, (key, value): (String, Value)| -> LuaResult<()> {
            let script_name = _state_set.lock().unwrap().current_script.clone();
            if script_name.is_empty() {
                return Ok(());
            }
            let path = super::script_data_dir(&script_name).join("config.toml");
            let contents = std::fs::read_to_string(&path).unwrap_or_default();
            let mut table: toml::Table = toml::from_str(&contents).unwrap_or_default();

            let toml_val = match value {
                Value::String(s) => toml::Value::String(s.to_str()?.to_string()),
                Value::Integer(n) => toml::Value::Integer(n),
                Value::Number(f) => toml::Value::Float(f),
                Value::Boolean(b) => toml::Value::Boolean(b),
                _ => return Ok(()),
            };
            table.insert(key, toml_val);

            let toml_str = toml::to_string_pretty(&table).unwrap_or_default();
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let _ = std::fs::write(&path, toml_str);
            Ok(())
        })?,
    )?;

    flume.set("config", cfg_tbl)?;
    Ok(())
}

/// flume.ui — UI manipulation.
fn register_ui_api(lua: &Lua, flume: &Table, state: State) -> LuaResult<()> {
    let ui_tbl = lua.create_table()?;

    // flume.ui.notify(message, level?)
    let state_notify = Arc::clone(&state);
    ui_tbl.set(
        "notify",
        lua.create_function(move |_, (message, level): (String, Option<String>)| {
            lua_runtime::push_action(
                &state_notify,
                ScriptAction::Notify {
                    message,
                    level: level.unwrap_or_else(|| "info".to_string()),
                },
            );
            Ok(())
        })?,
    )?;

    // flume.ui.status_item(name, text)
    let state_status = Arc::clone(&state);
    ui_tbl.set(
        "status_item",
        lua.create_function(move |_, (name, text): (String, String)| {
            lua_runtime::push_action(
                &state_status,
                ScriptAction::SetStatusItem { name, text },
            );
            Ok(())
        })?,
    )?;

    // flume.ui.input_text() — returns empty string (state not directly accessible)
    ui_tbl.set(
        "input_text",
        lua.create_function(|_, ()| Ok(String::new()))?,
    )?;

    // flume.ui.set_input_text(text) — no-op for now
    ui_tbl.set(
        "set_input_text",
        lua.create_function(|_, _text: String| Ok(()))?,
    )?;

    flume.set("ui", ui_tbl)?;
    Ok(())
}
