use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::config::general::LoggingConfig;

/// IRC message logger that writes to per-channel log files.
///
/// Log files are organized as:
///   `<base_dir>/<server>/<target>/YYYY-MM-DD.log`
///
/// Supports daily rotation (closes old file, opens new on date change).
pub struct Logger {
    config: LoggingConfig,
    base_dir: PathBuf,
    /// Open file handles keyed by "server/target".
    /// Value is (writer, current date string "YYYY-MM-DD").
    open_files: HashMap<String, (BufWriter<File>, String)>,
}

impl Logger {
    /// Create a new Logger. Does not create directories until first write.
    pub fn new(config: LoggingConfig) -> Self {
        let base_dir = crate::config::data_dir().join("logs");
        Logger {
            config,
            base_dir,
            open_files: HashMap::new(),
        }
    }

    /// Log a regular message (PRIVMSG).
    pub fn log_message(
        &mut self,
        server: &str,
        target: &str,
        timestamp: DateTime<Utc>,
        nick: &str,
        text: &str,
    ) {
        if !self.config.enabled {
            return;
        }
        let ts = timestamp.format("%H:%M:%S").to_string();
        match self.config.format.as_str() {
            "json" => {
                let line = format!(
                    r#"{{"ts":"{}","type":"msg","nick":"{}","text":"{}"}}"#,
                    timestamp.to_rfc3339(),
                    escape_json(nick),
                    escape_json(text),
                );
                self.write_line(server, target, &timestamp, &line);
            }
            _ => {
                let line = format!("[{}] <{}> {}", ts, nick, text);
                self.write_line(server, target, &timestamp, &line);
            }
        }
    }

    /// Log an action (/me).
    pub fn log_action(
        &mut self,
        server: &str,
        target: &str,
        timestamp: DateTime<Utc>,
        nick: &str,
        text: &str,
    ) {
        if !self.config.enabled {
            return;
        }
        let ts = timestamp.format("%H:%M:%S").to_string();
        match self.config.format.as_str() {
            "json" => {
                let line = format!(
                    r#"{{"ts":"{}","type":"action","nick":"{}","text":"{}"}}"#,
                    timestamp.to_rfc3339(),
                    escape_json(nick),
                    escape_json(text),
                );
                self.write_line(server, target, &timestamp, &line);
            }
            _ => {
                let line = format!("[{}] * {} {}", ts, nick, text);
                self.write_line(server, target, &timestamp, &line);
            }
        }
    }

    /// Log a channel event (join, part, quit, kick, mode, topic, etc.).
    pub fn log_event(
        &mut self,
        server: &str,
        target: &str,
        timestamp: DateTime<Utc>,
        text: &str,
    ) {
        if !self.config.enabled {
            return;
        }
        let ts = timestamp.format("%H:%M:%S").to_string();
        match self.config.format.as_str() {
            "json" => {
                let line = format!(
                    r#"{{"ts":"{}","type":"event","text":"{}"}}"#,
                    timestamp.to_rfc3339(),
                    escape_json(text),
                );
                self.write_line(server, target, &timestamp, &line);
            }
            _ => {
                let line = format!("[{}] -- {}", ts, text);
                self.write_line(server, target, &timestamp, &line);
            }
        }
    }

    /// Flush all open file handles.
    pub fn flush(&mut self) {
        for (writer, _) in self.open_files.values_mut() {
            let _ = writer.flush();
        }
    }

    fn write_line(
        &mut self,
        server: &str,
        target: &str,
        timestamp: &DateTime<Utc>,
        line: &str,
    ) {
        let date_str = timestamp.format("%Y-%m-%d").to_string();
        let key = format!("{}/{}", server, sanitize_target(target));

        // Check if we need to rotate (date changed) or open a new file
        let needs_open = match self.open_files.get(&key) {
            Some((_, current_date)) => *current_date != date_str,
            None => true,
        };

        if needs_open {
            // Close existing file if rotating
            if let Some((mut old_writer, _)) = self.open_files.remove(&key) {
                let _ = old_writer.flush();
            }

            // Create directory and open file
            let dir = self
                .base_dir
                .join(server)
                .join(sanitize_target(target));
            if let Err(e) = fs::create_dir_all(&dir) {
                tracing::error!("Failed to create log dir {}: {}", dir.display(), e);
                return;
            }

            let file_path = dir.join(format!("{}.log", date_str));
            match OpenOptions::new().create(true).append(true).open(&file_path) {
                Ok(file) => {
                    self.open_files
                        .insert(key.clone(), (BufWriter::new(file), date_str));
                }
                Err(e) => {
                    tracing::error!("Failed to open log file {}: {}", file_path.display(), e);
                    return;
                }
            }
        }

        if let Some((writer, _)) = self.open_files.get_mut(&key) {
            let _ = writeln!(writer, "{}", line);
        }
    }
}

/// Sanitize a channel/target name for use as a directory name.
/// Replaces characters that are problematic in file paths.
fn sanitize_target(target: &str) -> String {
    if target.is_empty() {
        return "server".to_string();
    }
    target
        .replace(['/', '\\'], "_")
        .replace('\0', "")
}

/// Simple JSON string escaping (quotes and backslashes).
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_target_names() {
        assert_eq!(sanitize_target("#rust"), "#rust");
        assert_eq!(sanitize_target(""), "server");
        assert_eq!(sanitize_target("nick/name"), "nick_name");
    }

    #[test]
    fn escape_json_strings() {
        assert_eq!(escape_json(r#"hello "world""#), r#"hello \"world\""#);
        assert_eq!(escape_json("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn logger_disabled() {
        let config = LoggingConfig {
            enabled: false,
            ..LoggingConfig::default()
        };
        let mut logger = Logger::new(config);
        // Should not panic or create files
        logger.log_message("test", "#test", Utc::now(), "nick", "hello");
        assert!(logger.open_files.is_empty());
    }
}
