pub mod api;
pub mod lua_runtime;
#[cfg(feature = "python")]
pub mod py_runtime;
pub mod sandbox;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Actions that scripts can trigger, processed by the TUI on the next tick.
#[derive(Debug, Clone)]
pub enum ScriptAction {
    /// Print text to a specific buffer.
    PrintToBuffer {
        server: String,
        buffer: String,
        text: String,
    },
    /// Send a PRIVMSG.
    SendMessage {
        server: String,
        target: String,
        text: String,
    },
    /// Send a raw IRC line.
    SendRaw {
        server: String,
        line: String,
    },
    /// Join a channel.
    JoinChannel {
        server: String,
        channel: String,
        key: Option<String>,
    },
    /// Part a channel.
    PartChannel {
        server: String,
        channel: String,
        message: Option<String>,
    },
    /// Show a desktop notification.
    Notify {
        message: String,
        level: String,
    },
    /// Set a custom status bar item.
    SetStatusItem {
        name: String,
        text: String,
    },
    /// Switch the active buffer.
    SwitchBuffer {
        buffer: String,
    },
}

/// An event passed to script handlers.
#[derive(Debug, Clone)]
pub struct ScriptEvent {
    /// Event name (e.g., "message", "join", "connect").
    pub name: String,
    /// Server this event relates to.
    pub server: String,
    /// Arbitrary key-value fields (nick, channel, text, etc.).
    pub fields: HashMap<String, String>,
    /// If true, the TUI should skip normal processing for this event.
    pub cancelled: bool,
}

impl ScriptEvent {
    pub fn new(name: &str, server: &str) -> Self {
        Self {
            name: name.to_string(),
            server: server.to_string(),
            fields: HashMap::new(),
            cancelled: false,
        }
    }

    pub fn field(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }
}

/// Metadata about a loaded script.
#[derive(Debug, Clone)]
pub struct ScriptInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_autoload: bool,
}

/// The script manager owns both Lua and Python runtimes and manages script lifecycle.
pub struct ScriptManager {
    lua: lua_runtime::LuaRuntime,
    #[cfg(feature = "python")]
    py: Option<py_runtime::PyRuntime>,
    scripts: Vec<ScriptInfo>,
}

impl ScriptManager {
    pub fn new() -> Result<Self, mlua::Error> {
        let lua = lua_runtime::LuaRuntime::new()?;

        #[cfg(feature = "python")]
        let py = match py_runtime::PyRuntime::new() {
            Ok(rt) => {
                tracing::info!("Python scripting engine initialized");
                Some(rt)
            }
            Err(e) => {
                tracing::warn!("Python scripting not available: {}", e);
                None
            }
        };

        Ok(Self {
            lua,
            #[cfg(feature = "python")]
            py,
            scripts: Vec::new(),
        })
    }

    /// Load a script from a file path. Routes by extension: .lua → Lua, .py → Python.
    pub fn load_script(&mut self, path: &Path) -> Result<(), mlua::Error> {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        if self.scripts.iter().any(|s| s.name == name) {
            return Ok(());
        }

        let source = std::fs::read_to_string(path)
            .map_err(|e| mlua::Error::runtime(e.to_string()))?;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("lua");

        match ext {
            #[cfg(feature = "python")]
            "py" => {
                if let Some(ref py) = self.py {
                    py.exec_script(&name, &source)
                        .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                } else {
                    return Err(mlua::Error::runtime(
                        "Python scripting not available",
                    ));
                }
            }
            _ => {
                self.lua.exec_script(&name, &source)?;
            }
        }

        // Warn about generated scripts
        if path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "generated") {
            tracing::warn!(
                "Script '{}' loaded from generated/ — review before trusting",
                name
            );
        }

        let is_autoload = path
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == "autoload");

        self.scripts.push(ScriptInfo {
            name,
            path: path.to_path_buf(),
            is_autoload,
        });

        Ok(())
    }

    /// Unload a script by name.
    pub fn unload_script(&mut self, name: &str) -> bool {
        if let Some(pos) = self.scripts.iter().position(|s| s.name == name) {
            let ext = self.scripts[pos]
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("lua");

            match ext {
                #[cfg(feature = "python")]
                "py" => {
                    if let Some(ref py) = self.py {
                        py.remove_script_handlers(name);
                    }
                }
                _ => {
                    self.lua.remove_script_handlers(name);
                }
            }
            self.scripts.remove(pos);
            true
        } else {
            false
        }
    }

    /// Reload a script by name.
    pub fn reload_script(&mut self, name: &str) -> Result<bool, mlua::Error> {
        if let Some(info) = self.scripts.iter().find(|s| s.name == name).cloned() {
            let source = std::fs::read_to_string(&info.path)
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;
            let ext = info.path.extension().and_then(|e| e.to_str()).unwrap_or("lua");

            match ext {
                #[cfg(feature = "python")]
                "py" => {
                    if let Some(ref py) = self.py {
                        py.remove_script_handlers(name);
                        py.exec_script(name, &source)
                            .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                    }
                }
                _ => {
                    self.lua.remove_script_handlers(name);
                    self.lua.exec_script(name, &source)?;
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn list_scripts(&self) -> &[ScriptInfo] {
        &self.scripts
    }

    /// Load all scripts from the autoload directory (.lua and .py).
    pub fn load_autoload(&mut self) -> Vec<(String, Result<(), mlua::Error>)> {
        let dir = scripts_autoload_dir();
        let mut results = Vec::new();

        let Ok(entries) = std::fs::read_dir(&dir) else {
            return results;
        };

        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let ext = e.path().extension().and_then(|x| x.to_str()).unwrap_or("").to_string();
                ext == "lua" || ext == "py"
            })
            .map(|e| e.path())
            .collect();
        paths.sort();

        for path in paths {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let result = self.load_script(&path);
            results.push((name, result));
        }

        results
    }

    /// Dispatch an event to all runtimes.
    pub fn dispatch_event(&self, event: ScriptEvent) -> ScriptEvent {
        let event = self.lua.dispatch_event(event);
        if event.cancelled {
            return event;
        }
        #[cfg(feature = "python")]
        if let Some(ref py) = self.py {
            return py.dispatch_event(event);
        }
        event
    }

    /// Drain actions from all runtimes.
    pub fn drain_actions(&self) -> Vec<ScriptAction> {
        let mut actions = self.lua.drain_actions();
        #[cfg(feature = "python")]
        if let Some(ref py) = self.py {
            actions.extend(py.drain_actions());
        }
        actions
    }

    /// Execute a custom command (checks Lua first, then Python).
    pub fn execute_command(&self, name: &str, args: &str) -> bool {
        if self.lua.execute_command(name, args) {
            return true;
        }
        #[cfg(feature = "python")]
        if let Some(ref py) = self.py {
            if py.execute_command(name, args) {
                return true;
            }
        }
        false
    }

    pub fn has_command(&self, name: &str) -> bool {
        if self.lua.has_command(name) {
            return true;
        }
        #[cfg(feature = "python")]
        if let Some(ref py) = self.py {
            if py.has_command(name) {
                return true;
            }
        }
        false
    }

    pub fn custom_command_names(&self) -> Vec<String> {
        let mut names = self.lua.custom_command_names();
        #[cfg(feature = "python")]
        if let Some(ref py) = self.py {
            names.extend(py.custom_command_names());
        }
        names
    }
}

/// Scripts directory.
pub fn scripts_dir() -> PathBuf {
    crate::config::config_dir().join("scripts")
}

/// Autoload scripts directory.
pub fn scripts_autoload_dir() -> PathBuf {
    scripts_dir().join("autoload")
}

/// Available (not auto-loaded) scripts directory.
pub fn scripts_available_dir() -> PathBuf {
    scripts_dir().join("available")
}

/// Script data directory (for persistent storage).
pub fn script_data_dir(script_name: &str) -> PathBuf {
    crate::config::data_dir().join("scripts").join(script_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_event_builder() {
        let event = ScriptEvent::new("message", "libera")
            .field("nick", "alice")
            .field("channel", "#rust")
            .field("text", "hello world");

        assert_eq!(event.name, "message");
        assert_eq!(event.server, "libera");
        assert_eq!(event.fields.get("nick"), Some(&"alice".to_string()));
        assert_eq!(event.fields.get("channel"), Some(&"#rust".to_string()));
        assert!(!event.cancelled);
    }

    #[test]
    fn script_manager_creates() {
        let mgr = ScriptManager::new();
        assert!(mgr.is_ok());
    }

    #[test]
    fn script_manager_load_nonexistent() {
        let mut mgr = ScriptManager::new().unwrap();
        let result = mgr.load_script(Path::new("/nonexistent/script.lua"));
        assert!(result.is_err());
    }

    #[test]
    fn script_manager_load_and_unload() {
        let mut mgr = ScriptManager::new().unwrap();
        let dir = std::env::temp_dir().join("flume_test_scripts");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.lua");
        std::fs::write(&path, "-- test script").unwrap();

        mgr.load_script(&path).unwrap();
        assert_eq!(mgr.list_scripts().len(), 1);
        assert_eq!(mgr.list_scripts()[0].name, "test");

        assert!(mgr.unload_script("test"));
        assert!(mgr.list_scripts().is_empty());
        assert!(!mgr.unload_script("test")); // already unloaded
    }

    #[test]
    fn script_manager_no_duplicate_load() {
        let mut mgr = ScriptManager::new().unwrap();
        let dir = std::env::temp_dir().join("flume_test_scripts_dup");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("dup.lua");
        std::fs::write(&path, "-- dup script").unwrap();

        mgr.load_script(&path).unwrap();
        mgr.load_script(&path).unwrap(); // no-op
        assert_eq!(mgr.list_scripts().len(), 1);
    }
}
