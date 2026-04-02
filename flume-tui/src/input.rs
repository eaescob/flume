use crossterm::event::{Event, KeyCode, KeyModifiers};

use flume_core::config::keybindings::KeybindingMode;
use flume_core::config::vault::Vault;
use flume_core::event::UserCommand;

use crate::app::{App, DisplayMessage, GenerateRequest, GenerationKind, InputMode, MessageSource, TabCompletionState, ViMode};
use crate::keybindings::{self, InputAction, KeyCombo, Keymap};
use crate::split::{self, LayoutProfile, SplitDirection, SplitState};

use std::collections::HashMap;
use std::sync::OnceLock;

/// Lazily-initialized keymaps. Built once per mode on first use.
struct KeymapSet {
    emacs: Keymap,
    vi: Keymap,
    vi_normal: HashMap<KeyCombo, InputAction>,
}

fn keymap_set() -> &'static KeymapSet {
    static SET: OnceLock<KeymapSet> = OnceLock::new();
    SET.get_or_init(|| KeymapSet {
        emacs: keybindings::build_keymap(KeybindingMode::Emacs),
        vi: keybindings::build_keymap(KeybindingMode::Vi),
        vi_normal: keybindings::build_vi_normal_keymap(),
    })
}

/// Handle a crossterm input event.
pub async fn handle_input(
    app: &mut App,
    event: Event,
    vault: &mut Option<Vault>,
) {
    let Event::Key(key_event) = event else {
        return;
    };

    // In passphrase mode, handle input specially
    if matches!(app.input_mode, InputMode::Passphrase(_)) {
        handle_passphrase_input(app, key_event.code, key_event.modifiers, vault);
        return;
    }

    let maps = keymap_set();
    let is_vi = app.keybinding_mode == KeybindingMode::Vi;
    let is_vi_normal = is_vi && app.vi_mode == ViMode::Normal;

    let keymap = if is_vi { &maps.vi } else { &maps.emacs };
    let vi_normal_map = if is_vi { Some(&maps.vi_normal) } else { None };

    // Try to resolve the key to an action
    if let Some(action) = keybindings::resolve(&key_event, keymap, vi_normal_map, is_vi_normal) {
        execute_action(app, action, vault).await;
        return;
    }

    // Handle vi operator-pending (dd, cc)
    if is_vi_normal {
        if let KeyCode::Char(c) = key_event.code {
            if let Some(pending) = app.vi_pending_op.take() {
                match (pending, c) {
                    ('d', 'd') => {
                        app.input.clear();
                        app.cursor_pos = 0;
                    }
                    ('c', 'c') => {
                        app.input.clear();
                        app.cursor_pos = 0;
                        app.vi_mode = ViMode::Insert;
                    }
                    _ => {} // Invalid combo, just drop it
                }
                app.tab_state = None;
                return;
            }

            // Start operator-pending for 'd' and 'c'
            if c == 'd' || c == 'c' {
                app.vi_pending_op = Some(c);
                return;
            }
        }
        // In vi normal mode, don't insert characters
        app.vi_pending_op = None;
        app.tab_state = None;
        return;
    }

    // Unbound key — if printable char, insert it (emacs or vi-insert mode)
    if let KeyCode::Char(c) = key_event.code {
        let mods = key_event.modifiers & !KeyModifiers::SHIFT;
        if mods.is_empty() || mods == KeyModifiers::NONE {
            app.input.insert(app.cursor_pos, c);
            app.cursor_pos += 1;
            app.tab_state = None;
        }
    }
}

/// Execute a resolved input action.
async fn execute_action(
    app: &mut App,
    action: InputAction,
    vault: &mut Option<Vault>,
) {
    // Tab completion: only TabComplete preserves tab_state
    let is_tab = matches!(action, InputAction::TabComplete);

    match action {
        // Submission
        InputAction::Submit => {
            if let Some(text) = app.submit_input() {
                process_input(&text, app, vault).await;
            }
        }
        InputAction::TabComplete => {
            handle_tab_completion(app);
        }

        // Cursor movement
        InputAction::CursorLeft => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
            }
        }
        InputAction::CursorRight => {
            if app.cursor_pos < app.input.len() {
                app.cursor_pos += 1;
            }
        }
        InputAction::CursorHome => {
            app.cursor_pos = 0;
        }
        InputAction::CursorEnd => {
            app.cursor_pos = app.input.len();
        }
        InputAction::CursorWordLeft => {
            app.cursor_pos = word_boundary_left(&app.input, app.cursor_pos);
        }
        InputAction::CursorWordRight => {
            app.cursor_pos = word_boundary_right(&app.input, app.cursor_pos);
        }

        // Deletion
        InputAction::DeleteCharBack => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                app.input.remove(app.cursor_pos);
            }
        }
        InputAction::DeleteCharForward => {
            if app.cursor_pos < app.input.len() {
                app.input.remove(app.cursor_pos);
            }
        }
        InputAction::DeleteWordBack => {
            let target = word_boundary_left(&app.input, app.cursor_pos);
            app.input.drain(target..app.cursor_pos);
            app.cursor_pos = target;
        }
        InputAction::DeleteToLineStart => {
            app.input.drain(..app.cursor_pos);
            app.cursor_pos = 0;
        }
        InputAction::DeleteToLineEnd => {
            app.input.truncate(app.cursor_pos);
        }
        InputAction::TransposeChars => {
            if app.cursor_pos > 0 && app.input.len() >= 2 {
                // If at end, transpose last two; otherwise transpose around cursor
                let pos = if app.cursor_pos >= app.input.len() {
                    app.cursor_pos - 1
                } else {
                    app.cursor_pos
                };
                if pos > 0 {
                    let bytes = unsafe { app.input.as_bytes_mut() };
                    bytes.swap(pos - 1, pos);
                    app.cursor_pos = pos + 1;
                }
            }
        }

        // History
        InputAction::HistoryPrev => {
            app.history_up();
        }
        InputAction::HistoryNext => {
            app.history_down();
        }

        // Buffer navigation
        InputAction::ScrollUp => {
            app.scroll_up(10);
        }
        InputAction::ScrollDown => {
            app.scroll_down(10);
        }
        InputAction::BufferNext => {
            if let Some(ss) = app.active_server_state_mut() {
                ss.cycle_buffer(true);
            }
        }
        InputAction::BufferPrev => {
            if let Some(ss) = app.active_server_state_mut() {
                ss.cycle_buffer(false);
            }
        }
        InputAction::BufferJump(n) => {
            let idx = (n as usize) - 1;
            if let Some(ss) = app.active_server_state_mut() {
                let sorted = ss.sorted_buffers();
                if let Some(name) = sorted.get(idx).cloned() {
                    ss.switch_buffer(&name);
                }
            }
        }
        InputAction::ServerCycle => {
            app.cycle_server();
        }

        // App control
        InputAction::Quit => {
            app.should_quit = true;
        }
        InputAction::SwapSplitFocus => {
            app.swap_split_focus();
        }

        // Vi mode switching
        InputAction::ViEnterNormal => {
            app.vi_mode = ViMode::Normal;
            app.vi_pending_op = None;
            // Move cursor back one (vi convention)
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
            }
        }
        InputAction::ViEnterInsert => {
            app.vi_mode = ViMode::Insert;
            app.vi_pending_op = None;
        }
        InputAction::ViEnterInsertAfter => {
            app.vi_mode = ViMode::Insert;
            app.vi_pending_op = None;
            if app.cursor_pos < app.input.len() {
                app.cursor_pos += 1;
            }
        }
        InputAction::ViEnterInsertEnd => {
            app.vi_mode = ViMode::Insert;
            app.vi_pending_op = None;
            app.cursor_pos = app.input.len();
        }
        InputAction::ViEnterInsertStart => {
            app.vi_mode = ViMode::Insert;
            app.vi_pending_op = None;
            app.cursor_pos = 0;
        }
        InputAction::ViDeleteChar => {
            if app.cursor_pos < app.input.len() {
                app.input.remove(app.cursor_pos);
                // Keep cursor in bounds
                if app.cursor_pos >= app.input.len() && app.cursor_pos > 0 {
                    app.cursor_pos -= 1;
                }
            }
        }
        InputAction::ViDeleteCharBack => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                app.input.remove(app.cursor_pos);
            }
        }
        InputAction::ViDeleteLine => {
            app.input.clear();
            app.cursor_pos = 0;
        }
        InputAction::ViChangeLine => {
            app.input.clear();
            app.cursor_pos = 0;
            app.vi_mode = ViMode::Insert;
            app.vi_pending_op = None;
        }
        InputAction::ViChangeToEnd => {
            app.input.truncate(app.cursor_pos);
            app.vi_mode = ViMode::Insert;
            app.vi_pending_op = None;
        }
    }

    if !is_tab {
        app.tab_state = None;
    }
}

/// Find the start of the previous word (for Ctrl+W, Alt+B).
fn word_boundary_left(input: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let bytes = input.as_bytes();
    let mut i = pos;
    // Skip whitespace backwards
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    // Skip word characters backwards
    while i > 0 && bytes[i - 1] != b' ' {
        i -= 1;
    }
    i
}

/// Find the start of the next word (for Alt+F, w).
fn word_boundary_right(input: &str, pos: usize) -> usize {
    let len = input.len();
    if pos >= len {
        return len;
    }
    let bytes = input.as_bytes();
    let mut i = pos;
    // Skip current word characters
    while i < len && bytes[i] != b' ' {
        i += 1;
    }
    // Skip whitespace
    while i < len && bytes[i] == b' ' {
        i += 1;
    }
    i
}

fn handle_passphrase_input(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    vault: &mut Option<Vault>,
) {
    match code {
        KeyCode::Enter => {
            let passphrase = std::mem::take(&mut app.input);
            app.cursor_pos = 0;
            let label = match &app.input_mode {
                InputMode::Passphrase(l) => l.clone(),
                _ => String::new(),
            };

            if passphrase.is_empty() {
                app.system_message("Vault skipped (empty passphrase)");
                app.input_mode = InputMode::Normal;
                app.vault_unlocked = true;
                return;
            }

            if label.starts_with("New vault") {
                if let Some(ref mut v) = vault {
                    v.change_passphrase(passphrase);
                    if let Err(e) = v.save() {
                        app.system_message(&format!("Failed to save vault: {}", e));
                    } else {
                        app.system_message("Vault passphrase changed");
                    }
                } else {
                    let path = flume_core::config::vault_path();
                    let v = Vault::new(path, passphrase);
                    if let Err(e) = v.save() {
                        app.system_message(&format!("Failed to create vault: {}", e));
                    } else {
                        app.system_message("New vault created");
                    }
                    *vault = Some(v);
                }
                app.input_mode = InputMode::Normal;
                app.vault_unlocked = true;
            } else {
                let path = flume_core::config::vault_path();
                match Vault::load(path, passphrase) {
                    Ok(v) => {
                        app.system_message("Vault unlocked");
                        *vault = Some(v);
                        app.input_mode = InputMode::Normal;
                        app.vault_unlocked = true;
                    }
                    Err(flume_core::config::vault::VaultError::Decryption) => {
                        app.system_message("Wrong passphrase. Try again (or press Enter to skip)");
                    }
                    Err(e) => {
                        app.system_message(&format!("Vault error: {}. Skipping vault.", e));
                        app.input_mode = InputMode::Normal;
                        app.vault_unlocked = true;
                    }
                }
            }
        }
        KeyCode::Char(c) => {
            if modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                app.should_quit = true;
                return;
            }
            app.input.insert(app.cursor_pos, c);
            app.cursor_pos += 1;
        }
        KeyCode::Backspace => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                app.input.remove(app.cursor_pos);
            }
        }
        KeyCode::Left => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
            }
        }
        KeyCode::Right => {
            if app.cursor_pos < app.input.len() {
                app.cursor_pos += 1;
            }
        }
        KeyCode::Esc => {
            app.input.clear();
            app.cursor_pos = 0;
            app.system_message("Vault skipped");
            app.input_mode = InputMode::Normal;
            app.vault_unlocked = true;
        }
        _ => {}
    }
}

fn handle_tab_completion(app: &mut App) {
    if let Some(ref mut state) = app.tab_state {
        // Cycling through existing matches
        if state.matches.is_empty() {
            return;
        }
        state.index = (state.index + 1) % state.matches.len();
        let completed = &state.matches[state.index];
        let suffix = if state.word_start == 0 { ": " } else { " " };
        app.input.truncate(state.word_start);
        app.input.push_str(completed);
        app.input.push_str(suffix);
        app.cursor_pos = app.input.len();
    } else {
        // Start new tab completion
        if app.input.is_empty() {
            return;
        }

        // Find the word being typed (from last space or start)
        let word_start = app.input[..app.cursor_pos]
            .rfind(' ')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = app.input[word_start..app.cursor_pos].to_string();
        if prefix.is_empty() {
            return;
        }

        // Check if completing an emoji shortcode (:prefix)
        if prefix.starts_with(':') && prefix.len() > 1 {
            let emoji_prefix = &prefix[1..]; // strip leading ':'
            let emoji_matches: Vec<String> = flume_core::emoji::complete_shortcode(emoji_prefix)
                .into_iter()
                .map(|(code, emoji)| format!(":{}:{}", code, emoji))
                .collect();

            if !emoji_matches.is_empty() {
                // Show first match: replace with the emoji directly
                let first = &emoji_matches[0];
                // Extract just the emoji (after the last ':' + space)
                let emoji_char = first.rsplit(':').next().unwrap_or("");
                app.input.truncate(word_start);
                app.input.push_str(emoji_char);
                app.input.push(' ');
                app.cursor_pos = app.input.len();

                // Store full codes for cycling
                let display_matches: Vec<String> = flume_core::emoji::complete_shortcode(emoji_prefix)
                    .into_iter()
                    .map(|(_, emoji)| emoji.to_string())
                    .collect();

                app.tab_state = Some(TabCompletionState {
                    prefix,
                    word_start,
                    matches: display_matches,
                    index: 0,
                });
                return;
            }
        }

        // Get nicks from active channel buffer
        let prefix_lower = prefix.to_lowercase();
        let nicks: Vec<String> = app
            .active_server_state()
            .and_then(|ss| ss.buffers.get(&ss.active_buffer))
            .map(|buf| {
                buf.nicks
                    .iter()
                    .filter(|cn| cn.nick.to_lowercase().starts_with(&prefix_lower))
                    .map(|cn| cn.nick.clone())
                    .collect()
            })
            .unwrap_or_default();

        if nicks.is_empty() {
            return;
        }

        let completed = &nicks[0];
        let suffix = if word_start == 0 { ": " } else { " " };
        app.input.truncate(word_start);
        app.input.push_str(completed);
        app.input.push_str(suffix);
        app.cursor_pos = app.input.len();

        app.tab_state = Some(TabCompletionState {
            prefix,
            word_start,
            matches: nicks,
            index: 0,
        });
    }
}

/// Send a command to the active server. Returns false if no connection.
async fn send_cmd(app: &App, cmd: UserCommand) -> bool {
    if let Some(tx) = app.active_command_tx() {
        let _ = tx.send(cmd).await;
        true
    } else {
        false
    }
}

async fn process_input(
    text: &str,
    app: &mut App,
    vault: &mut Option<Vault>,
) {
    // Handle interactive /generate init flow
    if let Some(step) = app.generate_init_step {
        handle_generate_init_input(text.trim(), step, app);
        return;
    }

    if text.starts_with('/') {
        let rest = &text[1..];
        let (cmd, args) = match rest.find(' ') {
            Some(pos) => (&rest[..pos], rest[pos + 1..].trim()),
            None => (rest, ""),
        };

        match cmd.to_lowercase().as_str() {
            "join" | "j" => {
                if args.is_empty() {
                    app.system_message("Usage: /join <channel> [key]");
                } else {
                    let parts: Vec<&str> = args.splitn(2, ' ').collect();
                    let channel = parts[0].to_string();
                    let key = parts.get(1).map(|s| s.to_string());
                    send_cmd(app, UserCommand::Join { channel, key }).await;
                }
            }
            "part" | "leave" => {
                let target = app.active_target().map(|s| s.to_string());
                let (channel, message) = if args.is_empty() {
                    (target, None)
                } else {
                    let parts: Vec<&str> = args.splitn(2, ' ').collect();
                    if parts[0].starts_with('#') {
                        (Some(parts[0].to_string()), parts.get(1).map(|s| s.to_string()))
                    } else {
                        (target, Some(args.to_string()))
                    }
                };
                if let Some(ch) = channel {
                    send_cmd(app, UserCommand::Part { channel: ch, message }).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "nick" => {
                if args.is_empty() {
                    app.system_message("Usage: /nick <nickname>");
                } else {
                    send_cmd(app, UserCommand::ChangeNick(args.to_string())).await;
                }
            }
            "quit" | "q" => {
                let msg = if args.is_empty() { None } else { Some(args.to_string()) };
                send_cmd(app, UserCommand::Quit(msg)).await;
                app.should_quit = true;
            }
            "umode" => {
                if args.is_empty() {
                    // Show current user modes
                    let modes = app.active_server_state()
                        .map(|ss| ss.user_modes.clone())
                        .unwrap_or_default();
                    if modes.is_empty() {
                        app.system_message("No user modes set");
                    } else {
                        app.system_message(&format!("User modes: {}", modes));
                    }
                } else {
                    // Set user modes: /umode +i or /umode -w
                    let nick = app.active_nick().to_string();
                    send_cmd(app, UserCommand::RawLine(format!("MODE {} {}", nick, args))).await;
                }
            }
            "disconnect" => {
                let server_name = if args.is_empty() {
                    app.active_server.clone()
                } else {
                    Some(args.to_string())
                };
                if let Some(ref name) = server_name {
                    // Send QUIT to server
                    if let Some(ss) = app.servers.get(name) {
                        if let Some(tx) = &ss.command_tx {
                            let _ = tx.send(UserCommand::Quit(None)).await;
                        }
                    }
                    let msg = format!("Disconnected from {}", name);
                    // Remove server and its buffers
                    app.servers.remove(name);
                    app.server_order.retain(|s| s != name);
                    // Switch to next server if available
                    if app.active_server.as_deref() == Some(name) {
                        app.active_server = app.server_order.first().cloned();
                    }
                    app.system_message(&msg);
                } else {
                    app.system_message("No active server");
                }
            }
            "msg" | "query" => {
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    app.system_message("Usage: /msg <target> <message>");
                } else {
                    let target = parts[0].to_string();
                    let msg_text = parts[1].to_string();
                    send_cmd(app, UserCommand::SendMessage {
                        target: target.clone(),
                        text: msg_text.clone(),
                    }).await;
                    // Track for echo dedup and show locally
                    if let Some(ss) = app.active_server_state_mut() {
                        ss.recent_own_messages.push_back((msg_text.clone(), chrono::Utc::now()));
                        while ss.recent_own_messages.len() > 20 {
                            ss.recent_own_messages.pop_front();
                        }
                    }
                    let nick = app.active_nick().to_string();
                    let scrollback = app.scrollback_limit;
                    if let Some(ss) = app.active_server_state_mut() {
                        ss.ensure_buffer(&target);
                        ss.add_message(
                            &target,
                            DisplayMessage {
                                timestamp: chrono::Utc::now(),
                                source: MessageSource::Own(nick),
                                text: msg_text,
                                highlight: false,
                            },
                            scrollback,
                        );
                    }
                }
            }
            "me" => {
                let target = app.active_target().map(|s| s.to_string());
                if let Some(ref target) = target {
                    let action = format!("\x01ACTION {}\x01", args);
                    send_cmd(app, UserCommand::SendMessage {
                        target: target.clone(),
                        text: action,
                    }).await;
                    let nick = app.active_nick().to_string();
                    let scrollback = app.scrollback_limit;
                    if let Some(ss) = app.active_server_state_mut() {
                        ss.add_message(
                            target,
                            DisplayMessage {
                                timestamp: chrono::Utc::now(),
                                source: MessageSource::Action(nick),
                                text: args.to_string(),
                                highlight: false,
                            },
                            scrollback,
                        );
                    }
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "quote" | "raw" => {
                if args.is_empty() {
                    app.system_message("Usage: /quote <raw IRC line>");
                } else {
                    send_cmd(app, UserCommand::RawLine(args.to_string())).await;
                }
            }
            // --- Core IRC commands ---
            "whois" | "wi" => {
                if args.is_empty() {
                    app.system_message("Usage: /whois <nick>");
                } else {
                    send_cmd(app, UserCommand::RawLine(format!("WHOIS {}", args))).await;
                }
            }
            "who" => {
                if args.is_empty() {
                    app.system_message("Usage: /who <mask>");
                } else {
                    send_cmd(app, UserCommand::RawLine(format!("WHO {}", args))).await;
                }
            }
            "mode" | "m" => {
                if args.is_empty() {
                    app.system_message("Usage: /mode <target> [modes] [params]");
                } else {
                    send_cmd(app, UserCommand::RawLine(format!("MODE {}", args))).await;
                }
            }
            "topic" | "t" => {
                let target = app.active_target().map(|s| s.to_string());
                if args.is_empty() {
                    // Show topic for current channel
                    if let Some(ch) = target {
                        send_cmd(app, UserCommand::RawLine(format!("TOPIC {}", ch))).await;
                    } else {
                        app.system_message("Usage: /topic [channel] [text]");
                    }
                } else if args.starts_with('#') {
                    // /topic #channel [text]
                    let parts: Vec<&str> = args.splitn(2, ' ').collect();
                    if parts.len() == 1 {
                        send_cmd(app, UserCommand::RawLine(format!("TOPIC {}", parts[0]))).await;
                    } else {
                        send_cmd(app, UserCommand::RawLine(format!("TOPIC {} :{}", parts[0], parts[1]))).await;
                    }
                } else if let Some(ch) = target {
                    // /topic some text (set topic on current channel)
                    send_cmd(app, UserCommand::RawLine(format!("TOPIC {} :{}", ch, args))).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "kick" | "k" => {
                let target = app.active_target().map(|s| s.to_string());
                if args.is_empty() {
                    app.system_message("Usage: /kick <nick> [reason]");
                } else if let Some(ch) = target {
                    let parts: Vec<&str> = args.splitn(2, ' ').collect();
                    let nick = parts[0];
                    let reason = parts.get(1).unwrap_or(&nick);
                    send_cmd(app, UserCommand::RawLine(format!("KICK {} {} :{}", ch, nick, reason))).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "ban" => {
                let target = app.active_target().map(|s| s.to_string());
                if args.is_empty() {
                    // Show ban list
                    if let Some(ch) = target {
                        send_cmd(app, UserCommand::RawLine(format!("MODE {} +b", ch))).await;
                    } else {
                        app.system_message("Usage: /ban [mask]");
                    }
                } else if let Some(ch) = target {
                    send_cmd(app, UserCommand::RawLine(format!("MODE {} +b {}", ch, args))).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "unban" => {
                let target = app.active_target().map(|s| s.to_string());
                if args.is_empty() {
                    app.system_message("Usage: /unban <mask>");
                } else if let Some(ch) = target {
                    send_cmd(app, UserCommand::RawLine(format!("MODE {} -b {}", ch, args))).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "invite" => {
                if args.is_empty() {
                    app.system_message("Usage: /invite <nick> [channel]");
                } else {
                    let parts: Vec<&str> = args.splitn(2, ' ').collect();
                    let nick = parts[0];
                    let channel = parts
                        .get(1)
                        .map(|s| s.to_string())
                        .or_else(|| app.active_target().map(|s| s.to_string()));
                    if let Some(ch) = channel {
                        send_cmd(app, UserCommand::RawLine(format!("INVITE {} {}", nick, ch))).await;
                    } else {
                        app.system_message("Not in a channel");
                    }
                }
            }
            "names" => {
                let channel = if args.is_empty() {
                    app.active_target().map(|s| s.to_string())
                } else {
                    Some(args.to_string())
                };
                if let Some(ch) = channel {
                    send_cmd(app, UserCommand::RawLine(format!("NAMES {}", ch))).await;
                } else {
                    app.system_message("Usage: /names [channel]");
                }
            }
            "list" => {
                if args.is_empty() {
                    send_cmd(app, UserCommand::RawLine("LIST".to_string())).await;
                } else {
                    send_cmd(app, UserCommand::RawLine(format!("LIST {}", args))).await;
                }
            }
            "away" => {
                if args.is_empty() {
                    // Unset away
                    send_cmd(app, UserCommand::RawLine("AWAY".to_string())).await;
                    app.system_message("Away status cleared");
                } else {
                    send_cmd(app, UserCommand::RawLine(format!("AWAY :{}", args))).await;
                    app.system_message(&format!("Set away: {}", args));
                }
            }
            "back" => {
                send_cmd(app, UserCommand::RawLine("AWAY".to_string())).await;
                app.system_message("Away status cleared");
            }
            "notice" | "n" => {
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    app.system_message("Usage: /notice <target> <text>");
                } else {
                    send_cmd(app, UserCommand::RawLine(format!("NOTICE {} :{}", parts[0], parts[1]))).await;
                }
            }
            "ctcp" => {
                let parts: Vec<&str> = args.splitn(3, ' ').collect();
                if parts.len() < 2 {
                    app.system_message("Usage: /ctcp <nick> <command> [args]");
                } else {
                    let target = parts[0];
                    let command = parts[1].to_uppercase();
                    let ctcp_args = parts.get(2).unwrap_or(&"");
                    let ctcp_msg = if ctcp_args.is_empty() {
                        format!("\x01{}\x01", command)
                    } else {
                        format!("\x01{} {}\x01", command, ctcp_args)
                    };
                    send_cmd(app, UserCommand::SendMessage {
                        target: target.to_string(),
                        text: ctcp_msg,
                    }).await;
                    app.system_message(&format!("CTCP {} sent to {}", command, target));
                }
            }
            "ping" => {
                if args.is_empty() {
                    app.system_message("Usage: /ping <nick>");
                } else {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    send_cmd(app, UserCommand::SendMessage {
                        target: args.to_string(),
                        text: format!("\x01PING {}\x01", ts),
                    }).await;
                    app.system_message(&format!("CTCP PING sent to {}", args));
                }
            }
            "motd" => {
                send_cmd(app, UserCommand::RawLine("MOTD".to_string())).await;
            }
            "lusers" => {
                send_cmd(app, UserCommand::RawLine("LUSERS".to_string())).await;
            }
            "version" => {
                if args.is_empty() {
                    send_cmd(app, UserCommand::RawLine("VERSION".to_string())).await;
                } else {
                    send_cmd(app, UserCommand::RawLine(format!("VERSION {}", args))).await;
                }
            }
            "info" => {
                send_cmd(app, UserCommand::RawLine("INFO".to_string())).await;
            }
            "op" => {
                let target = app.active_target().map(|s| s.to_string());
                if args.is_empty() {
                    app.system_message("Usage: /op <nick>");
                } else if let Some(ch) = target {
                    send_cmd(app, UserCommand::RawLine(format!("MODE {} +o {}", ch, args))).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "deop" => {
                let target = app.active_target().map(|s| s.to_string());
                if args.is_empty() {
                    app.system_message("Usage: /deop <nick>");
                } else if let Some(ch) = target {
                    send_cmd(app, UserCommand::RawLine(format!("MODE {} -o {}", ch, args))).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "voice" => {
                let target = app.active_target().map(|s| s.to_string());
                if args.is_empty() {
                    app.system_message("Usage: /voice <nick>");
                } else if let Some(ch) = target {
                    send_cmd(app, UserCommand::RawLine(format!("MODE {} +v {}", ch, args))).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "devoice" => {
                let target = app.active_target().map(|s| s.to_string());
                if args.is_empty() {
                    app.system_message("Usage: /devoice <nick>");
                } else if let Some(ch) = target {
                    send_cmd(app, UserCommand::RawLine(format!("MODE {} -v {}", ch, args))).await;
                } else {
                    app.system_message("Not in a channel");
                }
            }
            "close" => {
                // Close active buffer (remove from buffer list)
                if let Some(ss) = app.active_server_state_mut() {
                    let buf = ss.active_buffer.clone();
                    if buf.is_empty() {
                        app.system_message("Cannot close server buffer");
                    } else {
                        ss.buffers.remove(&buf);
                        ss.buffer_order.retain(|b| *b != buf);
                        // Switch to server buffer
                        ss.active_buffer = String::new();
                    }
                }
            }
            "clear" => {
                if let Some(ss) = app.active_server_state_mut() {
                    ss.active_buf_mut().messages.clear();
                    ss.active_buf_mut().scroll_offset = 0;
                }
            }
            "search" | "grep" | "find" => {
                if args.is_empty() {
                    // Clear search
                    if let Some(ss) = app.active_server_state_mut() {
                        ss.active_buf_mut().search = None;
                    }
                    app.system_message("Search cleared");
                } else {
                    let pattern = args.to_lowercase();
                    // Find first matching line and scroll to it
                    let scroll_to = app.active_messages().iter().enumerate().rev()
                        .find(|(_, msg)| msg.text.to_lowercase().contains(&pattern))
                        .map(|(i, _)| i);

                    if let Some(idx) = scroll_to {
                        let total = app.active_messages().len();
                        let offset = total.saturating_sub(idx + 1);
                        if let Some(ss) = app.active_server_state_mut() {
                            ss.active_buf_mut().search = Some(pattern);
                            ss.active_buf_mut().scroll_offset = offset;
                        }
                    } else {
                        app.system_message(&format!("No matches for '{}'", args));
                    }
                }
            }
            "help" | "h" => {
                if args.is_empty() {
                    show_help(app);
                } else {
                    show_help_topic(args.trim_start_matches('/'), app);
                }
            }
            "keys" | "keybindings" => {
                show_keybindings(app);
            }
            "buffer" | "buf" | "b" => {
                if args.is_empty() {
                    // List buffers for active server (alphabetical, matching buffer list panel)
                    if let Some(ss) = app.active_server_state() {
                        let mut sorted: Vec<&String> = ss.buffer_order.iter().collect();
                        sorted.sort_by(|a, b| {
                            if a.is_empty() { return std::cmp::Ordering::Less; }
                            if b.is_empty() { return std::cmp::Ordering::Greater; }
                            a.to_lowercase().cmp(&b.to_lowercase())
                        });
                        let lines: Vec<String> = sorted.iter().enumerate().map(|(i, name)| {
                            let display = if name.is_empty() { "server" } else { name.as_str() };
                            let active = if **name == ss.active_buffer { " *" } else { "" };
                            let unread = ss.buffers.get(name.as_str()).map(|b| b.unread_count).unwrap_or(0);
                            if unread > 0 {
                                format!("  {}: {} ({} unread){}", i + 1, display, unread, active)
                            } else {
                                format!("  {}: {}{}", i + 1, display, active)
                            }
                        }).collect();
                        app.system_message("Buffers:");
                        for line in &lines {
                            app.system_message(line);
                        }
                    }
                } else {
                    if let Some(ss) = app.active_server_state_mut() {
                        if ss.buffers.contains_key(args) {
                            ss.switch_buffer(args);
                        } else {
                            // Try as "server" alias for ""
                            if args == "server" {
                                ss.switch_buffer("");
                            }
                        }
                    }
                }
            }
            "go" => {
                if args.is_empty() {
                    app.system_message("Usage: /go <name or number> (or /go flume for global buffer)");
                } else if args == "flume" {
                    app.viewing_global = true;
                } else if let Ok(num) = args.parse::<usize>() {
                    app.viewing_global = false;
                    // Jump by window number (1-indexed, alphabetical order)
                    if num == 0 {
                        app.system_message("Window numbers start at 1");
                    } else if let Some(ss) = app.active_server_state_mut() {
                        let sorted = ss.sorted_buffers();
                        let idx = num - 1;
                        if let Some(name) = sorted.get(idx).cloned() {
                            ss.switch_buffer(&name);
                        } else {
                            app.system_message(&format!("No window #{}", num));
                        }
                    }
                } else {
                    app.viewing_global = false;
                    // Jump by name — try exact match first, then substring
                    let target = args.to_lowercase();
                    if let Some(ss) = app.active_server_state_mut() {
                        // Exact match
                        if ss.buffers.contains_key(args) {
                            ss.switch_buffer(args);
                        } else if args == "server" {
                            ss.switch_buffer("");
                        } else {
                            // Substring match
                            let found = ss.buffer_order.iter()
                                .find(|b| b.to_lowercase().contains(&target))
                                .cloned();
                            if let Some(name) = found {
                                ss.switch_buffer(&name);
                            } else {
                                // Try as server name
                                if app.servers.contains_key(args) {
                                    app.switch_server(args);
                                } else {
                                    app.system_message(&format!("No buffer or server matching '{}'", args));
                                }
                            }
                        }
                    }
                }
            }
            "switch" => {
                if args.is_empty() {
                    app.system_message("Usage: /switch <server name>");
                } else if app.servers.contains_key(args) {
                    app.switch_server(args);
                } else {
                    app.system_message(&format!("Server '{}' not connected", args));
                }
            }
            "connect" => {
                if args.is_empty() {
                    app.system_message("Usage: /connect <network name>");
                    app.system_message("Use /server list to see available networks.");
                } else {
                    let name = args.split_whitespace().next().unwrap_or("");
                    if app.irc_config.find(name).is_some()
                        || flume_core::config::load_server_config(name).is_ok()
                    {
                        app.connect_to = Some(name.to_string());
                    } else {
                        app.system_message(&format!("Network '{}' not found. Use /server add to create it.", name));
                    }
                }
            }
            "secure" => {
                handle_secure_command(args, app, vault);
            }
            "server" => {
                handle_server_command(args, app);
            }
            "set" => {
                handle_set_command(args, app);
            }
            "save" => {
                // Save irc.toml
                match flume_core::config::save_irc_config(&app.irc_config) {
                    Ok(()) => app.system_message(&format!(
                        "Saved {} network(s) to {}",
                        app.irc_config.networks.len(),
                        flume_core::config::irc_config_path().display()
                    )),
                    Err(e) => app.system_message(&format!("Failed to save irc.toml: {}", e)),
                }

                // Save config.toml (update runtime state)
                let config_path = flume_core::config::config_dir().join("config.toml");
                let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
                let mut config: toml::Table = toml::from_str(&existing).unwrap_or_default();

                // Update ui section
                let ui = config
                    .entry("ui")
                    .or_insert_with(|| toml::Value::Table(toml::Table::new()));
                if let toml::Value::Table(ref mut t) = ui {
                    t.insert("theme".to_string(), toml::Value::String(app.active_theme.clone()));
                    t.insert("show_join_part".to_string(), toml::Value::Boolean(app.show_join_part));
                    t.insert("show_hostmask_on_join".to_string(), toml::Value::Boolean(app.show_hostmask_on_join));
                }

                // Update combos section
                let combos_config = flume_core::config::combos::CombosConfig {
                    combos: app.combos.clone(),
                };
                if let Ok(combos_value) = toml::Value::try_from(&combos_config) {
                    config.insert("combos".to_string(), combos_value);
                }

                let _ = std::fs::create_dir_all(flume_core::config::config_dir());
                match toml::to_string_pretty(&config) {
                    Ok(toml_str) => {
                        match std::fs::write(&config_path, &toml_str) {
                            Ok(()) => app.system_message(&format!(
                                "Saved config to {}", config_path.display()
                            )),
                            Err(e) => app.system_message(&format!("Failed to save config.toml: {}", e)),
                        }
                    }
                    Err(e) => app.system_message(&format!("Failed to serialize config: {}", e)),
                }
            }
            "url" | "urls" => {
                let messages = app.active_messages();
                let mut all_urls: Vec<(String, String)> = Vec::new();
                for msg in messages.iter() {
                    let nick = match &msg.source {
                        MessageSource::User(n)
                        | MessageSource::Own(n)
                        | MessageSource::Action(n) => n.clone(),
                        _ => String::new(),
                    };
                    for u in crate::url::extract_urls(&msg.text) {
                        all_urls.push((u, nick.clone()));
                    }
                }

                if all_urls.is_empty() {
                    app.system_message("No URLs found in this buffer");
                } else if args.is_empty() {
                    // List recent URLs (last 10)
                    app.system_message(&format!("URLs in buffer ({} total):", all_urls.len()));
                    for (i, (u, nick)) in all_urls.iter().rev().take(10).enumerate() {
                        let label = if nick.is_empty() {
                            format!("  {}: {}", i + 1, u)
                        } else {
                            format!("  {}: {} ({})", i + 1, u, nick)
                        };
                        app.system_message(&label);
                    }
                    app.system_message("Use /url <number> to open");
                } else {
                    let num_str = args.trim_start_matches("open").trim();
                    if let Ok(n) = num_str.parse::<usize>() {
                        if n >= 1 && n <= all_urls.len() {
                            let idx = all_urls.len() - n;
                            let u = &all_urls[idx].0;
                            app.system_message(&format!("Opening: {}", u));
                            let _ = std::process::Command::new(&app.url_open_command)
                                .arg(u)
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .spawn();
                        } else {
                            app.system_message(&format!("Invalid URL number (1-{})", all_urls.len()));
                        }
                    } else {
                        app.system_message("Usage: /url [number]");
                    }
                }
            }
            "theme" => {
                if args.is_empty() {
                    let themes = crate::theme::Theme::list_available();
                    app.system_message("Available themes:");
                    for name in &themes {
                        if name == &app.active_theme {
                            app.system_message(&format!("  {} *", name));
                        } else {
                            app.system_message(&format!("  {}", name));
                        }
                    }
                    app.system_message("Usage: /theme <name> | /theme reload");
                } else if args == "reload" {
                    // Signal forced reload
                    app.theme_switch = Some("__reload__".to_string());
                } else if args == "path" {
                    // Show where themes are loaded from
                    let dir = flume_core::config::themes_dir();
                    app.system_message(&format!("Themes directory: {}", dir.display()));
                } else {
                    app.theme_switch = Some(args.to_string());
                }
            }
            "split" => {
                handle_split_command(args, app);
            }
            "unsplit" => {
                if app.split.is_some() {
                    app.unsplit();
                    app.system_message("Split closed");
                } else {
                    app.system_message("No active split");
                }
            }
            "focus" => {
                if app.split.is_some() {
                    app.swap_split_focus();
                } else {
                    app.system_message("No active split to swap focus");
                }
            }
            "layout" => {
                handle_layout_command(args, app);
            }
            "script" => {
                // Delegate to main loop which owns the ScriptManager
                app.script_command = Some(args.to_string());
            }
            "color" | "colour" => {
                // /color combo ... — manage color combos
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.first().copied() == Some("combo") {
                    handle_color_combo_command(parts.get(1).copied().unwrap_or(""), app);
                    return;
                }
                // /color <name> <text> — send text in a color
                if parts.len() < 2 {
                    app.system_message("Usage: /color <name> <text>");
                    app.system_message("  /color red Hello world!");
                    app.system_message("  /color blue,white Blue on white");
                    app.system_message("  See /colors for available color names");
                    app.system_message("  /color combo list|add|remove|test — manage combos");
                    return;
                }
                let color_spec = parts[0];
                let text = parts[1];
                // Parse fg[,bg]
                let color_parts: Vec<&str> = color_spec.splitn(2, ',').collect();
                let fg = flume_core::irc_format::color_name_to_code(color_parts[0])
                    .or_else(|| color_parts[0].parse::<u8>().ok());
                let bg = color_parts.get(1)
                    .and_then(|b| flume_core::irc_format::color_name_to_code(b)
                        .or_else(|| b.parse::<u8>().ok()));

                if let Some(fg_code) = fg {
                    let colored = if let Some(bg_code) = bg {
                        format!("\x03{},{}{}\x0f", fg_code, bg_code, text)
                    } else {
                        format!("\x03{}{}\x0f", fg_code, text)
                    };
                    // Send as if the user typed it
                    let target = app.active_target().map(|s| s.to_string());
                    if let Some(ref target) = target {
                        send_cmd(app, UserCommand::SendMessage {
                            target: target.clone(),
                            text: colored.clone(),
                        }).await;
                        if let Some(ss) = app.active_server_state_mut() {
                            ss.recent_own_messages.push_back((colored.clone(), chrono::Utc::now()));
                        }
                        let nick = app.active_nick().to_string();
                        let scrollback = app.scrollback_limit;
                        if let Some(ss) = app.active_server_state_mut() {
                            ss.add_message(target, DisplayMessage {
                                timestamp: chrono::Utc::now(),
                                source: MessageSource::Own(nick),
                                text: colored,
                                highlight: false,
                            }, scrollback);
                        }
                    }
                } else {
                    app.system_message(&format!("Unknown color: {}. See /colors", color_parts[0]));
                }
            }
            "colors" | "colours" => {
                app.system_message("Available colors:");
                let colors = flume_core::irc_format::color_names();
                let line: String = colors.iter()
                    .map(|(name, code)| format!("  \x03{}{}\x0f({})", code, name, code))
                    .collect::<Vec<_>>()
                    .join("  ");
                app.system_message(&line);
                app.system_message("");
                app.system_message("Usage:");
                app.system_message("  /color red Hello!          — send in red");
                app.system_message("  /color blue,white text     — blue on white");
                app.system_message("  %Cred inline %O normal     — inline formatting");
                app.system_message("  %B bold %I italic %U underline %O reset");
            }
            "emoji" => {
                if args.is_empty() {
                    app.system_message(&format!(
                        "Emoji shortcodes: {} available. Type :name: to use, or :prefix then Tab to complete.",
                        flume_core::emoji::shortcode_count()
                    ));
                    app.system_message("Examples: :thumbsup: :wave: :fire: :heart: :rocket: :100:");
                    app.system_message("Search: /emoji <term>");
                } else {
                    let matches = flume_core::emoji::complete_shortcode(args);
                    if matches.is_empty() {
                        app.system_message(&format!("No emoji matching '{}'", args));
                    } else {
                        let lines: Vec<String> = matches.iter()
                            .map(|(code, emoji)| format!("  {} :{}:", emoji, code))
                            .collect();
                        app.system_message(&format!("Emoji matching '{}':", args));
                        for line in &lines {
                            app.system_message(line);
                        }
                    }
                }
            }
            "snotice" => {
                handle_snotice_command(args, app);
            }
            "generate" | "gen" => {
                handle_generate_command(args, app);
            }
            "dcc" => {
                handle_dcc_command(args, app);
            }
            "xdcc" => {
                handle_xdcc_command(args, app).await;
            }
            _ => {
                // Try as script command (processed in main loop)
                app.script_command = Some(format!("_exec {} {}", cmd, args));
            }
        }
    } else {
        // Replace emoji shortcodes and IRC format shortcuts before sending
        let text = &flume_core::emoji::replace_shortcodes(text);
        let text = &flume_core::irc_format::apply_input_shortcuts(text, &app.combos);

        // Send as PRIVMSG to current target
        let target = app.active_target().map(|s| s.to_string());
        if let Some(ref target) = target {
            send_cmd(app, UserCommand::SendMessage {
                target: target.clone(),
                text: text.to_string(),
            }).await;
            // Track for echo deduplication and show locally
            if let Some(ss) = app.active_server_state_mut() {
                ss.recent_own_messages.push_back((text.to_string(), chrono::Utc::now()));
                // Keep only last 20 entries
                while ss.recent_own_messages.len() > 20 {
                    ss.recent_own_messages.pop_front();
                }
            }
            let nick = app.active_nick().to_string();
            let scrollback = app.scrollback_limit;
            if let Some(ss) = app.active_server_state_mut() {
                ss.add_message(
                    target,
                    DisplayMessage {
                        timestamp: chrono::Utc::now(),
                        source: MessageSource::Own(nick),
                        text: text.to_string(),
                        highlight: false,
                    },
                    scrollback,
                );
            }
        } else {
            app.system_message("No target set. Use /join <channel> or /buffer <name>");
        }
    }
}

fn handle_secure_command(args: &str, app: &mut App, vault: &mut Option<Vault>) {
    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    let subcmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

    match subcmd.as_str() {
        "set" => {
            if parts.len() < 3 {
                app.system_message("Usage: /secure set <name> <value>");
                return;
            }
            let name = parts[1];
            let value = parts[2];
            match vault {
                Some(v) => {
                    v.set(name.to_string(), value.to_string());
                    if let Err(e) = v.save() {
                        app.system_message(&format!("Failed to save vault: {}", e));
                    } else {
                        app.system_message(&format!("Secret '{}' saved", name));
                    }
                }
                None => {
                    app.system_message("Vault not initialized. Use /secure init");
                }
            }
        }
        "del" | "delete" => {
            if parts.len() < 2 {
                app.system_message("Usage: /secure del <name>");
                return;
            }
            let name = parts[1];
            match vault {
                Some(v) => {
                    if v.delete(name) {
                        if let Err(e) = v.save() {
                            app.system_message(&format!("Failed to save vault: {}", e));
                        } else {
                            app.system_message(&format!("Secret '{}' deleted", name));
                        }
                    } else {
                        app.system_message(&format!("Secret '{}' not found", name));
                    }
                }
                None => app.system_message("Vault not initialized"),
            }
        }
        "list" => match vault {
            Some(v) => {
                let names = v.list();
                if names.is_empty() {
                    app.system_message("Vault is empty");
                } else {
                    app.system_message(&format!("Vault secrets: {}", names.join(", ")));
                }
            }
            None => app.system_message("Vault not initialized"),
        },
        "init" => {
            if vault.is_some() {
                app.system_message("Vault already exists. Use /secure passphrase to change it.");
                return;
            }
            app.input_mode = InputMode::Passphrase("New vault passphrase".to_string());
            app.system_message("Enter a passphrase for the new vault:");
        }
        "passphrase" => {
            if vault.is_none() {
                app.system_message("No vault loaded. Use /secure init to create one.");
                return;
            }
            app.input_mode = InputMode::Passphrase("New vault passphrase".to_string());
            app.system_message("Enter new passphrase for the vault:");
        }
        "unlock" => {
            if vault.is_some() {
                app.system_message("Vault is already unlocked");
                return;
            }
            if !flume_core::config::vault_path().exists() {
                app.system_message("No vault file found. Use /secure init to create one.");
                return;
            }
            app.input_mode = InputMode::Passphrase("Vault passphrase".to_string());
            app.system_message("Enter vault passphrase:");
        }
        _ => app.system_message("Usage: /secure <set|del|list|init|unlock|passphrase>"),
    }
}

fn handle_server_command(args: &str, app: &mut App) {
    use flume_core::config::NetworkEntry;

    let all_args: Vec<&str> = args.split_whitespace().collect();
    let subcmd = all_args.first().map(|s| s.to_lowercase()).unwrap_or_default();

    match subcmd.as_str() {
        "add" => {
            // /server add <name> <address> [port] [flags...]
            // Flags: -tls, -notls, -autoconnect, -username <user>, -password <pass>, -nick <nick>
            if all_args.len() < 3 {
                app.system_message("Usage: /server add <name> <address> [port] [options]");
                app.system_message("  Options: -tls -notls -autoconnect");
                app.system_message("           -username <user> -password <pass> -nick <nick>");
                return;
            }
            let name = all_args[1];
            let address = all_args[2];

            let mut port: Option<u16> = None;
            let mut force_tls: Option<bool> = None;
            let mut autoconnect = false;
            let mut username: Option<String> = None;
            let mut password: Option<String> = None;
            let mut nick: Option<String> = None;

            let mut i = 3;
            while i < all_args.len() {
                match all_args[i] {
                    "-tls" => force_tls = Some(true),
                    "-notls" => force_tls = Some(false),
                    "-autoconnect" => autoconnect = true,
                    "-username" | "-user" => {
                        i += 1;
                        if i < all_args.len() {
                            username = Some(all_args[i].to_string());
                        }
                    }
                    "-password" | "-pass" => {
                        i += 1;
                        if i < all_args.len() {
                            password = Some(all_args[i].to_string());
                        }
                    }
                    "-nick" => {
                        i += 1;
                        if i < all_args.len() {
                            nick = Some(all_args[i].to_string());
                        }
                    }
                    _ => {
                        if port.is_none() {
                            if let Ok(p) = all_args[i].parse::<u16>() {
                                port = Some(p);
                            }
                        }
                    }
                }
                i += 1;
            }

            let port = port.unwrap_or(6697);
            let mut entry = NetworkEntry::new(name.to_string(), address.to_string(), port);
            if let Some(tls) = force_tls {
                entry.tls = tls;
            }
            entry.autoconnect = autoconnect;
            entry.username = username;
            entry.password = password;
            entry.nick = nick;

            let tls_str = if entry.tls { "TLS" } else { "plain" };
            let auto_str = if autoconnect { ", autoconnect" } else { "" };
            match app.irc_config.add(entry) {
                Ok(()) => app.system_message(&format!(
                    "Added network '{}' ({}:{}, {}{}). Use /save to persist.",
                    name, address, port, tls_str, auto_str
                )),
                Err(e) => app.system_message(&format!("Error: {}", e)),
            }
        }
        "remove" | "rm" | "del" => {
            if all_args.len() < 2 {
                app.system_message("Usage: /server remove <name>");
                return;
            }
            let name = all_args[1];
            if app.irc_config.remove(name) {
                app.system_message(&format!("Removed network '{}'. Use /save to persist.", name));
            } else {
                app.system_message(&format!("Network '{}' not found", name));
            }
        }
        "list" | "ls" => {
            if app.irc_config.networks.is_empty() {
                app.system_message("No networks configured. Use /server add <name> <address> [port]");
            } else {
                let lines: Vec<String> = app.irc_config.networks.iter().map(|entry| {
                    let tls_str = if entry.tls { "TLS" } else { "plain" };
                    let auth_str = match entry.auth_method {
                        flume_core::config::server::AuthMethod::Sasl => "SASL",
                        flume_core::config::server::AuthMethod::Nickserv => "NickServ",
                        flume_core::config::server::AuthMethod::None => "none",
                    };
                    let auto_str = if entry.autoconnect { " [auto]" } else { "" };
                    let channels = if entry.autojoin.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", entry.autojoin.join(", "))
                    };
                    format!(
                        "  {} — {}:{} ({}, auth: {}){}{}",
                        entry.name, entry.address, entry.port, tls_str, auth_str, auto_str, channels
                    )
                }).collect();

                app.system_message("Configured networks:");
                for line in &lines {
                    app.system_message(line);
                }
            }
        }
        "set" => {
            if all_args.len() < 4 {
                app.system_message("Usage: /server set <name> <key> <value>");
                return;
            }
            let name = all_args[1];
            let after_name = args.splitn(3, ' ').nth(2).unwrap_or("");
            let (key, value) = match after_name.find(' ') {
                Some(pos) => (&after_name[..pos], after_name[pos + 1..].trim()),
                None => {
                    app.system_message("Usage: /server set <name> <key> <value>");
                    return;
                }
            };

            match app.irc_config.find_mut(name) {
                Some(entry) => match entry.set_field(key, value) {
                    Ok(()) => app.system_message(&format!(
                        "Set {}.{} = {}. Use /save to persist.",
                        name, key, value
                    )),
                    Err(e) => app.system_message(&format!("Error: {}", e)),
                },
                None => app.system_message(&format!("Network '{}' not found", name)),
            }
        }
        "connect" => {
            if all_args.len() < 2 {
                app.system_message("Usage: /server connect <name>");
                return;
            }
            let name = all_args[1];
            if app.irc_config.find(name).is_some()
                || flume_core::config::load_server_config(name).is_ok()
            {
                app.connect_to = Some(name.to_string());
            } else {
                app.system_message(&format!("Network '{}' not found", name));
            }
        }
        "switch" => {
            if all_args.len() < 2 {
                app.system_message("Usage: /server switch <name>");
                return;
            }
            let name = all_args[1];
            if app.servers.contains_key(name) {
                app.switch_server(name);
            } else {
                app.system_message(&format!("Server '{}' not connected", name));
            }
        }
        _ => {
            app.system_message("Usage: /server <add|remove|list|set|connect|switch>");
        }
    }
}

fn show_help(app: &mut App) {
    app.system_message("Flume commands:");
    app.system_message("  Chat:");
    app.system_message("    /join <channel> [key]    — Join a channel");
    app.system_message("    /part [channel] [msg]    — Leave a channel");
    app.system_message("    /msg <target> <text>     — Send a private message");
    app.system_message("    /me <text>               — Send an action");
    app.system_message("    /notice <target> <text>  — Send a notice");
    app.system_message("    /topic [channel] [text]  — View or set topic");
    app.system_message("    /nick <nick>             — Change nickname");
    app.system_message("    /away [message]          — Set away (no args = clear)");
    app.system_message("    /back                    — Clear away status");
    app.system_message("  Channel management:");
    app.system_message("    /kick <nick> [reason]    — Kick a user");
    app.system_message("    /ban [mask]              — Ban (no args = list bans)");
    app.system_message("    /unban <mask>            — Remove a ban");
    app.system_message("    /op <nick>               — Give operator status");
    app.system_message("    /deop <nick>             — Remove operator status");
    app.system_message("    /voice <nick>            — Give voice");
    app.system_message("    /devoice <nick>          — Remove voice");
    app.system_message("    /invite <nick> [channel] — Invite a user");
    app.system_message("    /mode <target> [modes]   — Set/view modes");
    app.system_message("    /names [channel]         — List channel members");
    app.system_message("  Queries:");
    app.system_message("    /whois <nick>            — Query user info");
    app.system_message("    /who <mask>              — Search users");
    app.system_message("    /list [pattern]          — List channels");
    app.system_message("    /motd                    — Show message of the day");
    app.system_message("    /lusers                  — Show network statistics");
    app.system_message("    /version [server]        — Show server version");
    app.system_message("    /ctcp <nick> <cmd>       — Send CTCP request");
    app.system_message("    /ping <nick>             — CTCP ping a user");
    app.system_message("  Navigation:");
    app.system_message("    /buffer <name>           — Switch buffer (or list if no args)");
    app.system_message("    /switch <server>         — Switch active server");
    app.system_message("    /close                   — Close active buffer");
    app.system_message("    /clear                   — Clear active buffer");
    app.system_message("    /search <pattern>        — Search buffer (no args = clear)");
    app.system_message("    Ctrl+X                   — Cycle servers");
    app.system_message("    Alt+Left/Right           — Cycle buffers");
    app.system_message("    Alt+1-9                  — Jump to buffer by number");
    app.system_message("  Connection:");
    app.system_message("    /connect <name>          — Connect to a network");
    app.system_message("    /disconnect              — Disconnect active server");
    app.system_message("    /quit [message]          — Quit Flume");
    app.system_message("  Server management:");
    app.system_message("    /server add <name> <addr> [port] [-tls] [-autoconnect] [-username <u>] [-password <p>]");
    app.system_message("    /server remove <name>    — Remove a network");
    app.system_message("    /server list             — List configured networks");
    app.system_message("    /server set <n> <k> <v>  — Set a network field");
    app.system_message("    /save                    — Save config to disk");
    app.system_message("  Vault:");
    app.system_message("    /secure init             — Create a new vault");
    app.system_message("    /secure set <n> <v>      — Store a secret");
    app.system_message("    /secure del <name>       — Delete a secret");
    app.system_message("    /secure list             — List secret names");
    app.system_message("  Splits:");
    app.system_message("    /split v|h <buffer>      — Split view with a buffer");
    app.system_message("    /unsplit                 — Close split");
    app.system_message("    /focus                   — Swap focus between panes");
    app.system_message("    /layout save <name>      — Save current split layout");
    app.system_message("    /layout load <name>      — Load a saved layout");
    app.system_message("    /layout list             — List saved layouts");
    app.system_message("    /layout delete <name>    — Delete a saved layout");
    app.system_message("  Scripts:");
    app.system_message("    /script load <name|path> — Load a script");
    app.system_message("    /script unload <name>    — Unload a script");
    app.system_message("    /script reload <name>    — Reload a script");
    app.system_message("    /script list             — List loaded scripts");
    app.system_message("  DCC:");
    app.system_message("    /dcc list                — Show DCC transfers");
    app.system_message("    /dcc accept [id]         — Accept pending DCC");
    app.system_message("    /dcc reject [id]         — Reject pending DCC");
    app.system_message("    /dcc send <nick> <file>  — Send a file");
    app.system_message("    /dcc chat <nick>         — Start DCC CHAT");
    app.system_message("    /dcc close <id>          — Close transfer/chat");
    app.system_message("    /xdcc <bot> <pack#>      — Request XDCC pack");
    app.system_message("    /xdcc <bot> list         — Request bot pack list");
    app.system_message("    /xdcc <bot> cancel       — Cancel XDCC");
    app.system_message("  Generate (LLM):");
    app.system_message("    /generate script <desc>  — Generate a script from description");
    app.system_message("    /generate theme <desc>   — Generate a theme");
    app.system_message("    /generate layout <desc>  — Generate a layout");
    app.system_message("    /generate accept/reject  — Save or discard generation");
    app.system_message("  Other:");
    app.system_message("    /snotice add|suppress|list|rm|save|test|last");
    app.system_message("                             — Manage server notice rules");
    app.system_message("    /color <name> <text>     — Send colored text");
    app.system_message("    /color combo list|add|rm — Manage color combos");
    app.system_message("    /colors                  — Show available colors");
    app.system_message("    /set [key] [value]       — View or change settings");
    app.system_message("    /quote <raw line>        — Send raw IRC line");
    app.system_message("    /go <name or number>     — Jump to buffer/server");
    app.system_message("    /keys                    — Show keybinding info");
    app.system_message("    /help [command]          — Show help (or help on a command)");
}

fn show_help_topic(topic: &str, app: &mut App) {
    match topic {
        "join" => {
            app.system_message("/join <channel> [key]");
            app.system_message("  Join an IRC channel. Optionally provide a key for +k channels.");
            app.system_message("  Example: /join #rust");
            app.system_message("  Example: /join #secret mykey");
        }
        "part" | "leave" => {
            app.system_message("/part [channel] [message]");
            app.system_message("  Leave a channel. Defaults to the active channel.");
            app.system_message("  The buffer is automatically closed when you part.");
            app.system_message("  Example: /part #rust Goodbye!");
        }
        "msg" | "query" => {
            app.system_message("/msg <target> <text>");
            app.system_message("  Send a private message to a user or channel.");
            app.system_message("  Example: /msg alice Hello there!");
        }
        "me" => {
            app.system_message("/me <text>");
            app.system_message("  Send an action message (/me dances).");
        }
        "nick" => {
            app.system_message("/nick <newnick>");
            app.system_message("  Change your nickname on the current server.");
        }
        "topic" => {
            app.system_message("/topic [channel] [text]");
            app.system_message("  View or set the channel topic.");
            app.system_message("  With no args: shows current topic.");
            app.system_message("  With text: sets the topic (requires permissions).");
        }
        "buffer" | "buf" | "b" => {
            app.system_message("/buffer [name]");
            app.system_message("  With no args: list all buffers with their numbers.");
            app.system_message("  With name: switch to that buffer.");
            app.system_message("  Use 'server' to switch to the server buffer.");
        }
        "go" => {
            app.system_message("/go <name or number>");
            app.system_message("  Jump to a buffer by number (1-indexed) or name.");
            app.system_message("  Supports partial/substring matching on names.");
            app.system_message("  Also matches server names for cross-server switching.");
            app.system_message("  Example: /go 3        — jump to window 3");
            app.system_message("  Example: /go #rust    — jump to #rust");
            app.system_message("  Example: /go rust     — fuzzy match #rust");
        }
        "switch" => {
            app.system_message("/switch <server>");
            app.system_message("  Switch to a different connected server.");
        }
        "split" => {
            app.system_message("/split v|h <buffer>");
            app.system_message("  Split the view vertically (v) or horizontally (h).");
            app.system_message("  Shows two buffers side-by-side or top-bottom.");
            app.system_message("  Use server/buffer for cross-server splits.");
            app.system_message("  Example: /split v #linux");
        }
        "unsplit" => {
            app.system_message("/unsplit");
            app.system_message("  Close the split view and return to single buffer.");
        }
        "focus" => {
            app.system_message("/focus");
            app.system_message("  Swap keyboard focus between split panes. Also: Alt+Tab.");
        }
        "layout" => {
            app.system_message("/layout save|load|list|delete <name>");
            app.system_message("  Manage saved split layouts.");
            app.system_message("  save <name>   — save current split as a named layout");
            app.system_message("  load <name>   — restore a saved layout");
            app.system_message("  list          — list saved layouts");
            app.system_message("  delete <name> — delete a saved layout");
        }
        "script" => {
            app.system_message("/script load|unload|reload|autoload|noautoload|list [name]");
            app.system_message("  Manage Lua and Python scripts.");
            app.system_message("  load <name|path>     — load a script");
            app.system_message("  unload <name>        — unload a loaded script");
            app.system_message("  reload <name>        — reload a script from disk");
            app.system_message("  autoload <name>      — symlink script into autoload dir");
            app.system_message("  noautoload <name>    — remove from autoload dir");
            app.system_message("  list                 — list loaded scripts and commands");
            app.system_message("");
            app.system_message("  Autoload directories:");
            app.system_message(&format!("    Lua:    {}", flume_core::scripting::lua_autoload_dir().display()));
            app.system_message(&format!("    Python: {}", flume_core::scripting::python_autoload_dir().display()));
        }
        "generate" | "gen" => {
            app.system_message("/generate init|script|theme|layout|accept|reject");
            app.system_message("  Use an LLM to generate content from a description.");
            app.system_message("");
            app.system_message("  /generate init                 — interactive setup (provider + API key)");
            app.system_message("  /generate script [options] <desc>");
            app.system_message("    --name <name>                — set the filename");
            app.system_message("    --lua / --python             — choose language (default: lua)");
            app.system_message("  /generate theme [--name <name>] <desc>");
            app.system_message("  /generate layout [--name <name>] <desc>");
            app.system_message("  /generate accept               — save and load result");
            app.system_message("  /generate reject               — discard result");
            app.system_message("");
            app.system_message("  Examples:");
            app.system_message("    /generate script --name greeter greet users who join");
            app.system_message("    /generate script --python --name urlbot log URLs");
            app.system_message("    /generate theme --name midnight dark blue with orange");
            app.system_message("    /generate layout --name monitor #ops left #alerts right");
        }
        "dcc" => {
            app.system_message("/dcc list|accept|reject|send|chat|close [args]");
            app.system_message("  DCC file transfer and chat commands.");
            app.system_message("  list          — show all DCC transfers");
            app.system_message("  accept [id]   — accept pending DCC offer");
            app.system_message("  reject [id]   — reject pending DCC offer");
            app.system_message("  send <nick> <file> — send a file to a user");
            app.system_message("  chat <nick>   — start a DCC chat session");
            app.system_message("  close <id>    — close a transfer or chat");
        }
        "xdcc" => {
            app.system_message("/xdcc <bot> <pack#|list|cancel>");
            app.system_message("  Request files from XDCC bots.");
            app.system_message("  /xdcc bot 42     — request pack #42");
            app.system_message("  /xdcc bot list   — request pack list");
            app.system_message("  /xdcc bot cancel — cancel transfer");
        }
        "server" => {
            app.system_message("/server add|remove|list|set|connect|switch [args]");
            app.system_message("  Manage IRC network configurations.");
            app.system_message("  add <name> <addr> [port] [-tls|-notls] [-autoconnect] [-username <u>] [-password <p>] [-nick <n>]");
            app.system_message("  remove <name>    — remove a network");
            app.system_message("  list             — list configured networks");
            app.system_message("  set <n> <k> <v>  — set a network field");
            app.system_message("  connect <name>   — connect to a network");
            app.system_message("");
            app.system_message("  Authentication examples:");
            app.system_message("    /secure set libera_pass my-password");
            app.system_message("    /server set libera auth_method sasl");
            app.system_message("    /server set libera sasl_username mynick");
            app.system_message("    /server set libera sasl_password ${libera_pass}");
            app.system_message("");
            app.system_message("  NickServ example:");
            app.system_message("    /server set libera auth_method nickserv");
            app.system_message("    /server set libera nickserv_password ${libera_pass}");
            app.system_message("");
            app.system_message("  The ${name} syntax references vault secrets.");
        }
        "secure" | "vault" => {
            app.system_message("/secure init|set|del|list|unlock|passphrase");
            app.system_message("  Manage the encrypted secrets vault.");
            app.system_message("  init             — create a new vault");
            app.system_message("  set <name> <val> — store a secret");
            app.system_message("  del <name>       — delete a secret");
            app.system_message("  list             — list secret names");
            app.system_message("  unlock           — unlock the vault");
            app.system_message("  passphrase       — change vault passphrase");
        }
        "whois" => {
            app.system_message("/whois <nick>");
            app.system_message("  Query detailed information about a user.");
            app.system_message("  Shows nick, user@host, realname, channels, server, idle time.");
        }
        "umode" => {
            app.system_message("/umode [modes]");
            app.system_message("  View or set your user modes.");
            app.system_message("  /umode           — show current modes");
            app.system_message("  /umode +i        — set invisible");
            app.system_message("  /umode -w        — unset wallops");
            app.system_message("  /umode +iwx      — set multiple modes");
        }
        "disconnect" => {
            app.system_message("/disconnect [server]");
            app.system_message("  Disconnect from a server and remove it from the session.");
            app.system_message("  With no args: disconnects the active server.");
            app.system_message("  Switches to the next connected server if available.");
        }
        "keys" | "keybindings" => {
            app.system_message("/keys");
            app.system_message("  Show all keybindings for the active mode (Emacs or Vi).");
            app.system_message("  Set mode in config.toml: [ui.keybindings] mode = \"vi\"");
        }
        "theme" => {
            app.system_message("/theme [name|reload|path]");
            app.system_message("  Switch themes or reload the current theme.");
            app.system_message("  /theme             — list available themes");
            app.system_message("  /theme <name>      — switch to a theme");
            app.system_message("  /theme reload      — force-reload current theme");
            app.system_message("  /theme path        — show themes directory");
            app.system_message("");
            app.system_message("  Themes are TOML files in ~/.local/share/flume/themes/");
            app.system_message("  Copy example themes: cp examples/themes/*.toml ~/.local/share/flume/themes/");
        }
        "search" | "grep" | "find" => {
            app.system_message("/search <pattern>");
            app.system_message("  Search the active buffer for a text pattern.");
            app.system_message("  Matching lines are highlighted. /search with no args clears.");
        }
        "snotice" => {
            app.system_message("/snotice add|suppress|list|remove|save|test|last");
            app.system_message("  Manage regex-based server notice routing rules.");
            app.system_message("");
            app.system_message("  /snotice list             — show all rules");
            app.system_message("  /snotice add --match <regex> [options]");
            app.system_message("    --format <fmt>          — format with ${1} ${2} capture groups");
            app.system_message("    --buffer <name>         — route to a named buffer");
            app.system_message("    --suppress              — drop the notice entirely");
            app.system_message("  /snotice suppress <text>  — suppress notices containing literal text");
            app.system_message("  /snotice remove <number>  — remove a rule by number");
            app.system_message("  /snotice save             — save rules to snotice.toml");
            app.system_message("  /snotice test <text>      — test rules against sample text");
            app.system_message("  /snotice last             — show last notice, suppress/route it");
            app.system_message("    /snotice last suppress  — suppress the last notice");
            app.system_message("    /snotice last route <buffer> [--format <fmt>]");
            app.system_message("                            — route last notice to a buffer");
            app.system_message("");
            app.system_message("  Example:");
            app.system_message("    /snotice add --match \"Client connecting: (\\S+)\" --format \"[connect] ${1}\" --buffer snotice-connections");
            app.system_message("    /snotice suppress Oper-up notice");
        }
        "color" | "colour" => {
            app.system_message("/color <color> <text>");
            app.system_message("  Send a message with colored text.");
            app.system_message("");
            app.system_message("  Color can be a name or mIRC number (0-15):");
            app.system_message("  /color red Watch out!");
            app.system_message("  /color 4 This is also red");
            app.system_message("  /color lightblue Hello world");
            app.system_message("");
            app.system_message("  Use /colors to see all available color names.");
            app.system_message("");
            app.system_message("  Inline formatting shortcuts:");
            app.system_message("  %B bold  %I italic  %U underline  %R reverse  %O reset");
            app.system_message("  %C<color> or %C<fg>,<bg> for inline colors");
            app.system_message("  Example: %Cred hello %O world");
            app.system_message("");
            app.system_message("  Color combos (reusable shortcuts):");
            app.system_message("  %rainbow%text%O — apply a combo to text");
            app.system_message("  /color combo list             — list all combos");
            app.system_message("  /color combo add <name> <fmt> — add static combo");
            app.system_message("  /color combo add <name> cycle <colors...>");
            app.system_message("                                — add cycling combo");
            app.system_message("  /color combo remove <name>    — remove a combo");
            app.system_message("  /color combo test <name> <text>");
            app.system_message("                                — preview a combo");
        }
        "colors" | "colours" => {
            app.system_message("/colors");
            app.system_message("  Show all available color names with previews.");
        }
        "set" => {
            app.system_message("/set [section.key] [value]");
            app.system_message("  View or change settings (saved to config.toml).");
            app.system_message("");
            app.system_message("  /set                           — list all settings");
            app.system_message("  /set ui                        — list ui section");
            app.system_message("  /set ui.theme                  — show current value");
            app.system_message("  /set ui.theme solarized-dark   — set and save");
            app.system_message("");
            app.system_message("  Sections: general, ui, logging, notifications, ctcp, llm, dcc");
            app.system_message("");
            app.system_message("  Common settings:");
            app.system_message("    general.default_nick        — default nickname");
            app.system_message("    general.quit_message         — quit message");
            app.system_message("    ui.theme                     — active theme");
            app.system_message("    ui.show_join_part            — show join/part messages (true/false)");
            app.system_message("    ui.show_hostmask_on_join     — show user@host on joins");
            app.system_message("    ui.keybindings.mode          — emacs or vi");
            app.system_message("    notifications.highlight_bell — terminal bell on highlight");
            app.system_message("    llm.provider                 — anthropic or openai");
            app.system_message("    dcc.enabled                  — enable DCC (true/false)");
        }
        _ => {
            // Check if it's a script-registered command
            app.script_command = Some(format!("_help {}", topic));
            app.system_message(&format!("No help for '{}'. Try /help for the full list.", topic));
        }
    }
}

fn show_keybindings(app: &mut App) {
    let mode_name = match app.keybinding_mode {
        KeybindingMode::Emacs => "Emacs",
        KeybindingMode::Vi => "Vi",
        KeybindingMode::Custom => "Custom",
    };
    app.system_message(&format!("Keybinding mode: {}", mode_name));
    app.system_message("  Global (all modes):");
    app.system_message("    Ctrl+C             — Quit");
    app.system_message("    Ctrl+X             — Cycle servers");
    app.system_message("    Alt+1-9            — Jump to buffer");
    app.system_message("    Alt+Left/Right     — Cycle buffers");
    app.system_message("    PageUp/Down        — Scroll");
    app.system_message("    Tab                — Nick completion");
    app.system_message("    Alt+Tab            — Swap split focus");
    app.system_message("    Enter              — Submit");

    match app.keybinding_mode {
        KeybindingMode::Emacs | KeybindingMode::Custom => {
            app.system_message("  Emacs bindings:");
            app.system_message("    Ctrl+A / Home      — Start of line");
            app.system_message("    Ctrl+E / End       — End of line");
            app.system_message("    Ctrl+B / Left      — Cursor left");
            app.system_message("    Ctrl+F / Right     — Cursor right");
            app.system_message("    Alt+B              — Word left");
            app.system_message("    Alt+F              — Word right");
            app.system_message("    Ctrl+D / Delete    — Delete forward");
            app.system_message("    Ctrl+K             — Kill to end of line");
            app.system_message("    Ctrl+U             — Kill to start of line");
            app.system_message("    Ctrl+W / Alt+Bksp  — Delete word back");
            app.system_message("    Ctrl+T             — Transpose chars");
            app.system_message("    Ctrl+P / Up        — History previous");
            app.system_message("    Ctrl+N / Down      — History next");
            app.system_message("    Esc                — Quit");
        }
        KeybindingMode::Vi => {
            app.system_message("  Vi Insert mode:");
            app.system_message("    Esc                — Enter Normal mode");
            app.system_message("    Ctrl+W             — Delete word back");
            app.system_message("    Ctrl+U             — Kill to start of line");
            app.system_message("    Ctrl+K             — Kill to end of line");
            app.system_message("  Vi Normal mode:");
            app.system_message("    i                  — Insert at cursor");
            app.system_message("    a                  — Insert after cursor");
            app.system_message("    A                  — Insert at end of line");
            app.system_message("    I                  — Insert at start of line");
            app.system_message("    h/l                — Cursor left/right");
            app.system_message("    w/b                — Word forward/backward");
            app.system_message("    0/^/$              — Start/end of line");
            app.system_message("    x/X                — Delete char forward/back");
            app.system_message("    dd                 — Delete entire line");
            app.system_message("    cc                 — Change entire line");
            app.system_message("    C                  — Change to end of line");
            app.system_message("    j/k                — History next/previous");
        }
    }
    app.system_message("  Set mode in config.toml: [ui.keybindings] mode = \"emacs\"|\"vi\"");
}

fn handle_split_command(args: &str, app: &mut App) {
    if args.is_empty() {
        if let Some(ref s) = app.split {
            let dir = match s.direction {
                SplitDirection::Vertical => "vertical",
                SplitDirection::Horizontal => "horizontal",
            };
            app.system_message(&format!(
                "Active split: {} — {}/{}",
                dir, s.secondary_server, s.secondary_buffer
            ));
        } else {
            app.system_message("No active split");
            app.system_message("Usage: /split v|h <buffer> or /split v|h <server>/<buffer>");
        }
        return;
    }

    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        app.system_message("Usage: /split v|h <buffer> or /split v|h <server>/<buffer>");
        return;
    }

    let direction = match parts[0] {
        "v" | "vertical" => SplitDirection::Vertical,
        "h" | "horizontal" => SplitDirection::Horizontal,
        _ => {
            app.system_message("Direction must be 'v' (vertical) or 'h' (horizontal)");
            return;
        }
    };

    let target = parts[1].trim();
    let (server, buffer) = if let Some(slash) = target.find('/') {
        // Cross-server split: server/buffer
        (target[..slash].to_string(), target[slash + 1..].to_string())
    } else {
        // Same server
        let server = app.active_server.clone().unwrap_or_default();
        (server, target.to_string())
    };

    // Verify the target exists
    if let Some(ss) = app.servers.get(&server) {
        if ss.buffers.contains_key(&buffer) {
            app.split = Some(SplitState::new(direction, server.clone(), buffer.clone()));
            let dir_name = match direction {
                SplitDirection::Vertical => "vertical",
                SplitDirection::Horizontal => "horizontal",
            };
            app.system_message(&format!("Split {} with {}/{}", dir_name, server, buffer));
        } else {
            app.system_message(&format!("Buffer '{}' not found on server '{}'", buffer, server));
        }
    } else {
        app.system_message(&format!("Server '{}' not found", server));
    }
}

fn handle_layout_command(args: &str, app: &mut App) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let subcmd = parts.first().copied().unwrap_or("");
    let name = parts.get(1).copied().unwrap_or("").trim();

    match subcmd {
        "save" => {
            if name.is_empty() {
                app.system_message("Usage: /layout save <name>");
                return;
            }
            let Some(ref s) = app.split else {
                app.system_message("No active split to save");
                return;
            };
            let primary = app
                .active_server_state()
                .map(|ss| ss.active_buffer.clone())
                .unwrap_or_default();
            let profile = LayoutProfile {
                direction: s.direction,
                primary,
                secondary: s.secondary_buffer.clone(),
                ratio: s.ratio,
            };
            match split::save_layout(name, &profile) {
                Ok(()) => app.system_message(&format!("Layout '{}' saved", name)),
                Err(e) => app.system_message(&format!("Failed to save layout: {}", e)),
            }
        }
        "load" => {
            if name.is_empty() {
                app.system_message("Usage: /layout load <name>");
                return;
            }
            match split::load_layout(name) {
                Some(profile) => {
                    let server = app.active_server.clone().unwrap_or_default();
                    // Switch active buffer to the primary
                    if let Some(ss) = app.active_server_state_mut() {
                        if ss.buffers.contains_key(&profile.primary) {
                            ss.switch_buffer(&profile.primary);
                        }
                    }
                    // Set up split with the secondary
                    if app
                        .servers
                        .get(&server)
                        .is_some_and(|ss| ss.buffers.contains_key(&profile.secondary))
                    {
                        app.split = Some(SplitState::new(
                            profile.direction,
                            server,
                            profile.secondary.clone(),
                        ));
                        app.system_message(&format!("Layout '{}' loaded", name));
                    } else {
                        app.system_message(&format!(
                            "Buffer '{}' not found on active server",
                            profile.secondary
                        ));
                    }
                }
                None => app.system_message(&format!("Layout '{}' not found", name)),
            }
        }
        "list" | "ls" => {
            let layouts = split::list_layouts();
            if layouts.is_empty() {
                app.system_message("No saved layouts");
            } else {
                app.system_message("Saved layouts:");
                for name in &layouts {
                    app.system_message(&format!("  {}", name));
                }
            }
        }
        "delete" | "del" | "rm" => {
            if name.is_empty() {
                app.system_message("Usage: /layout delete <name>");
                return;
            }
            if split::delete_layout(name) {
                app.system_message(&format!("Layout '{}' deleted", name));
            } else {
                app.system_message(&format!("Layout '{}' not found", name));
            }
        }
        _ => {
            app.system_message("Usage: /layout save|load|list|delete <name>");
        }
    }
}

fn handle_dcc_command(args: &str, app: &mut App) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let subcmd = parts.first().copied().unwrap_or("");
    let rest = parts.get(1).copied().unwrap_or("").trim();

    match subcmd {
        "list" | "ls" | "" => {
            if app.dcc_transfers.is_empty() {
                app.system_message("No DCC transfers");
            } else {
                let lines: Vec<String> = app.dcc_transfers.iter().map(|t| {
                    let kind = match t.offer.dcc_type {
                        flume_core::dcc::DccType::Send => "SEND",
                        flume_core::dcc::DccType::Chat => "CHAT",
                    };
                    let name = t.offer.filename.as_deref().unwrap_or("(chat)");
                    let status = match &t.state {
                        flume_core::dcc::DccTransferState::Pending => "pending".to_string(),
                        flume_core::dcc::DccTransferState::Connecting => "connecting".to_string(),
                        flume_core::dcc::DccTransferState::Active { bytes_transferred, total } => {
                            if *total > 0 {
                                format!("{}%", (*bytes_transferred * 100) / total)
                            } else {
                                flume_core::dcc::format_size(*bytes_transferred)
                            }
                        }
                        flume_core::dcc::DccTransferState::Complete => "complete".to_string(),
                        flume_core::dcc::DccTransferState::Failed(e) => format!("failed: {}", e),
                        flume_core::dcc::DccTransferState::Cancelled => "cancelled".to_string(),
                    };
                    let dir = if t.outgoing { ">>>" } else { "<<<" };
                    format!("  [{}] {} {} {} {} — {}", t.id, kind, dir, t.offer.from, name, status)
                }).collect();
                app.system_message("DCC transfers:");
                for line in &lines {
                    app.system_message(line);
                }
            }
        }
        "accept" => {
            // Accept most recent pending, or by ID
            let id: Option<u64> = if rest.is_empty() {
                app.dcc_transfers
                    .iter()
                    .rev()
                    .find(|t| matches!(t.state, flume_core::dcc::DccTransferState::Pending))
                    .map(|t| t.id)
            } else {
                rest.parse().ok()
            };
            match id {
                Some(id) => {
                    app.dcc_command = Some(format!("accept {}", id));
                }
                None => {
                    app.system_message("No pending DCC transfer to accept");
                }
            }
        }
        "reject" => {
            let id: Option<u64> = if rest.is_empty() {
                app.dcc_transfers
                    .iter()
                    .rev()
                    .find(|t| matches!(t.state, flume_core::dcc::DccTransferState::Pending))
                    .map(|t| t.id)
            } else {
                rest.parse().ok()
            };
            if let Some(id) = id {
                if let Some(t) = app.dcc_transfers.iter_mut().find(|t| t.id == id) {
                    t.state = flume_core::dcc::DccTransferState::Cancelled;
                    app.system_message(&format!("DCC #{} rejected", id));
                }
            } else {
                app.system_message("No pending DCC transfer to reject");
            }
        }
        "send" => {
            // /dcc send <nick> <file>
            let send_parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if send_parts.len() < 2 {
                app.system_message("Usage: /dcc send <nick> <file>");
                return;
            }
            app.dcc_command = Some(format!("send {} {}", send_parts[0], send_parts[1]));
        }
        "chat" => {
            if rest.is_empty() {
                app.system_message("Usage: /dcc chat <nick>");
                return;
            }
            app.dcc_command = Some(format!("chat {}", rest));
        }
        "close" => {
            let id: Option<u64> = rest.parse().ok();
            if let Some(id) = id {
                if let Some(t) = app.dcc_transfers.iter_mut().find(|t| t.id == id) {
                    t.state = flume_core::dcc::DccTransferState::Cancelled;
                    app.dcc_chat_senders.remove(&id);
                    app.system_message(&format!("DCC #{} closed", id));
                }
            } else {
                app.system_message("Usage: /dcc close <id>");
            }
        }
        _ => {
            app.system_message("Usage: /dcc list|accept|reject|send|chat|close [args]");
        }
    }
}

async fn handle_xdcc_command(args: &str, app: &mut App) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        app.system_message("Usage: /xdcc <bot> <pack#|list|cancel>");
        return;
    }
    let bot = parts[0];
    let subcmd = parts[1].trim();

    let message = if subcmd == "list" {
        flume_core::dcc::xdcc::request_list()
    } else if subcmd == "cancel" {
        flume_core::dcc::xdcc::request_cancel()
    } else {
        // Try to parse as pack number (with or without #)
        let num_str = subcmd.trim_start_matches('#');
        match num_str.parse::<u32>() {
            Ok(n) => flume_core::dcc::xdcc::request_pack(n),
            Err(_) => {
                app.system_message("Usage: /xdcc <bot> <pack#|list|cancel>");
                return;
            }
        }
    };

    send_cmd(
        app,
        flume_core::event::UserCommand::SendMessage {
            target: bot.to_string(),
            text: message,
        },
    )
    .await;
    app.system_message(&format!("XDCC request sent to {}", bot));
}

fn handle_generate_init_input(text: &str, step: u8, app: &mut App) {
    match step {
        // Step 1: Choose provider
        1 => {
            let (provider, default_model) = match text {
                "1" | "anthropic" => ("anthropic", "claude-sonnet-4-20250514"),
                "2" | "openai" => ("openai", "gpt-4o"),
                _ => {
                    app.system_message("Please type 1 (Anthropic) or 2 (OpenAI):");
                    return;
                }
            };
            // Save provider choice to config
            let config_dir = flume_core::config::config_dir();
            let _ = std::fs::create_dir_all(&config_dir);
            let config_path = config_dir.join("config.toml");
            let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
            let mut config: toml::Table = toml::from_str(&existing).unwrap_or_default();

            let llm = config.entry("llm").or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if let toml::Value::Table(ref mut t) = llm {
                t.insert("provider".to_string(), toml::Value::String(provider.to_string()));
                t.insert("model".to_string(), toml::Value::String(default_model.to_string()));
            }

            if let Ok(toml_str) = toml::to_string_pretty(&config) {
                let _ = std::fs::write(&config_path, toml_str);
            }

            app.system_message(&format!("Provider set to {} (model: {})", provider, default_model));
            app.system_message("");
            app.system_message("Now paste your API key (it will be stored in the encrypted vault):");
            app.generate_init_step = Some(2);
        }
        // Step 2: Store API key
        2 => {
            if text.is_empty() || text.starts_with('/') {
                app.system_message("Please paste your API key:");
                return;
            }
            // Store in vault via script_command (processed in main loop with vault access)
            app.script_command = Some(format!("_init_llm_key {}", text));
            app.generate_init_step = None;
        }
        _ => {
            app.generate_init_step = None;
        }
    }
}

fn handle_color_combo_command(args: &str, app: &mut App) {
    use flume_core::config::combos::{ComboDefinition, DynamicCombo};

    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let subcmd = parts.first().copied().unwrap_or("");
    match subcmd {
        "list" | "ls" | "" => {
            if app.combos.is_empty() {
                app.system_message("No color combos defined");
                app.system_message("Usage: /color combo add <name> <format>");
                app.system_message("       /color combo add <name> cycle <color1> <color2> ...");
                return;
            }
            let mut lines = vec!["Color combos:".to_string()];
            let mut names: Vec<_> = app.combos.keys().cloned().collect();
            names.sort();
            for name in &names {
                let desc = match &app.combos[name] {
                    ComboDefinition::Static(fmt) => format!("  %{}% = {}", name, fmt),
                    ComboDefinition::Dynamic(d) => {
                        format!("  %{}% = {} [{}]", name, d.combo_type, d.colors.join(", "))
                    }
                };
                lines.push(desc);
            }
            lines.push("Use %<name>%text%O in messages".to_string());
            for line in &lines {
                app.system_message(line);
            }
        }
        "add" => {
            let rest = parts.get(1).copied().unwrap_or("").trim();
            let add_parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if add_parts.len() < 2 {
                app.system_message("Usage: /color combo add <name> <format string>");
                app.system_message("       /color combo add <name> cycle <color1> <color2> ...");
                app.system_message("  Example: /color combo add alert %B%Cred,white");
                app.system_message("  Example: /color combo add pride cycle red orange yellow green blue purple");
                return;
            }
            let name = add_parts[0].to_lowercase();
            let def = add_parts[1].trim();

            if def.starts_with("cycle ") || def.starts_with("cycle\t") {
                // Dynamic cycle combo
                let colors: Vec<String> = def[6..]
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                if colors.is_empty() {
                    app.system_message("Cycle combo needs at least one color");
                    return;
                }
                // Validate color names
                for c in &colors {
                    if flume_core::irc_format::color_name_to_code(c).is_none()
                        && c.parse::<u8>().is_err()
                    {
                        app.system_message(&format!("Unknown color: {}. See /colors", c));
                        return;
                    }
                }
                app.combos.insert(
                    name.clone(),
                    ComboDefinition::Dynamic(DynamicCombo {
                        combo_type: "cycle".to_string(),
                        colors,
                    }),
                );
                app.system_message(&format!("Added cycle combo: %{}%", name));
            } else {
                // Static combo
                app.combos.insert(name.clone(), ComboDefinition::Static(def.to_string()));
                app.system_message(&format!("Added combo: %{}% = {}", name, def));
            }
            app.system_message("Use /save to persist");
        }
        "remove" | "rm" | "del" => {
            let name = parts.get(1).copied().unwrap_or("").trim().to_lowercase();
            if name.is_empty() {
                app.system_message("Usage: /color combo remove <name>");
                return;
            }
            if app.combos.remove(&name).is_some() {
                app.system_message(&format!("Removed combo: {}", name));
                app.system_message("Use /save to persist");
            } else {
                app.system_message(&format!("No combo named '{}'. See /color combo list", name));
            }
        }
        "test" => {
            let rest = parts.get(1).copied().unwrap_or("").trim();
            let test_parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if test_parts.len() < 2 {
                app.system_message("Usage: /color combo test <name> <text>");
                app.system_message("  Example: /color combo test rainbow Hello world!");
                return;
            }
            let name = test_parts[0].to_lowercase();
            let text = test_parts[1];
            if !app.combos.contains_key(&name) {
                app.system_message(&format!("No combo named '{}'. See /color combo list", name));
                return;
            }
            // Build the formatted string and display it as a system message
            let input = format!("%{}%{}%O", name, text);
            let formatted = flume_core::irc_format::apply_input_shortcuts(&input, &app.combos);
            app.system_message(&format!("Preview: {}", formatted));
        }
        _ => {
            app.system_message("Usage: /color combo list|add|remove|test");
        }
    }
}

fn handle_snotice_command(args: &str, app: &mut App) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let subcmd = parts.first().copied().unwrap_or("");

    match subcmd {
        "list" | "ls" | "" => {
            if app.snotice_configs.is_empty() {
                app.system_message("No snotice rules configured");
                app.system_message("Usage: /snotice add --match <regex> [--format <fmt>] [--buffer <name>] [--suppress]");
            } else {
                app.system_message("Snotice rules:");
                let lines: Vec<String> = app.snotice_configs.iter().enumerate().map(|(i, rule)| {
                    let mut desc = format!("  {}: match=\"{}\"", i + 1, rule.pattern);
                    if let Some(ref fmt) = rule.format {
                        desc.push_str(&format!(" format=\"{}\"", fmt));
                    }
                    if let Some(ref buf) = rule.buffer {
                        desc.push_str(&format!(" buffer=\"{}\"", buf));
                    }
                    if rule.suppress {
                        desc.push_str(" suppress");
                    }
                    desc
                }).collect();
                for line in &lines {
                    app.system_message(line);
                }
            }
        }
        "add" => {
            let rest = parts.get(1).copied().unwrap_or("");
            let words: Vec<&str> = rest.split_whitespace().collect();

            let mut pattern: Option<String> = None;
            let mut format: Option<String> = None;
            let mut buffer: Option<String> = None;
            let mut suppress = false;

            let mut i = 0;
            while i < words.len() {
                match words[i] {
                    "--match" | "-m" => {
                        i += 1;
                        if i < words.len() {
                            // Collect until next flag
                            let mut p = words[i].to_string();
                            while i + 1 < words.len() && !words[i + 1].starts_with("--") {
                                i += 1;
                                p.push(' ');
                                p.push_str(words[i]);
                            }
                            pattern = Some(p);
                        }
                    }
                    "--format" | "-f" => {
                        i += 1;
                        if i < words.len() {
                            let mut f = words[i].to_string();
                            while i + 1 < words.len() && !words[i + 1].starts_with("--") {
                                i += 1;
                                f.push(' ');
                                f.push_str(words[i]);
                            }
                            format = Some(f);
                        }
                    }
                    "--buffer" | "-b" => {
                        i += 1;
                        if i < words.len() {
                            buffer = Some(words[i].to_string());
                        }
                    }
                    "--suppress" | "-s" => {
                        suppress = true;
                    }
                    _ => {}
                }
                i += 1;
            }

            let Some(pat) = pattern else {
                app.system_message("Usage: /snotice add --match <regex> [--format <fmt>] [--buffer <name>] [--suppress]");
                return;
            };

            // Validate regex
            if regex::Regex::new(&pat).is_err() {
                app.system_message(&format!("Invalid regex: {}", pat));
                return;
            }

            let rule = flume_core::config::formats::SnoticeRuleConfig {
                pattern: pat.clone(),
                format,
                buffer,
                suppress,
            };
            app.snotice_configs.push(rule);
            app.snotice_rules = crate::app::compile_snotice_rules(&app.snotice_configs);
            app.system_message(&format!("Snotice rule added: match=\"{}\"", pat));
            app.system_message("Use /snotice save to persist");
        }
        "remove" | "rm" | "del" => {
            let rest = parts.get(1).copied().unwrap_or("").trim();
            if let Ok(idx) = rest.parse::<usize>() {
                if idx >= 1 && idx <= app.snotice_configs.len() {
                    let removed = app.snotice_configs.remove(idx - 1);
                    app.snotice_rules = crate::app::compile_snotice_rules(&app.snotice_configs);
                    app.system_message(&format!("Removed rule {}: match=\"{}\"", idx, removed.pattern));
                } else {
                    app.system_message(&format!("Invalid rule number. Use /snotice list (1-{})", app.snotice_configs.len()));
                }
            } else {
                app.system_message("Usage: /snotice remove <number>");
            }
        }
        "save" => {
            match flume_core::config::save_snotice_rules(&app.snotice_configs) {
                Ok(()) => app.system_message(&format!(
                    "Saved {} rule(s) to {}",
                    app.snotice_configs.len(),
                    flume_core::config::snotice_config_path().display()
                )),
                Err(e) => app.system_message(&format!("Failed to save: {}", e)),
            }
        }
        "suppress" => {
            // /snotice suppress <literal text> — suppress notices containing this text
            let text = parts.get(1).copied().unwrap_or("").trim();
            if text.is_empty() {
                app.system_message("Usage: /snotice suppress <text to match>");
                app.system_message("  Matches notices containing this text (literal, not regex)");
                app.system_message("  Example: /snotice suppress popm2!bopm");
                return;
            }
            let escaped = regex::escape(text);
            if regex::Regex::new(&escaped).is_ok() {
                let rule = flume_core::config::formats::SnoticeRuleConfig {
                    pattern: escaped,
                    format: None,
                    buffer: None,
                    suppress: true,
                };
                app.snotice_configs.push(rule);
                app.snotice_rules = crate::app::compile_snotice_rules(&app.snotice_configs);
                app.system_message(&format!("Suppressing notices containing: {}", text));
                app.system_message("Use /snotice save to persist");
            }
        }
        "test" => {
            // Test snotice rules against a sample text
            let test_text = parts.get(1).copied().unwrap_or("").trim();
            if test_text.is_empty() {
                app.system_message("Usage: /snotice test <notice text to test against>");
                app.system_message("Paste the exact notice text (including *** Notice -- if present)");
                return;
            }
            let mut matched = false;
            for (i, rule) in app.snotice_rules.iter().enumerate() {
                if let Some(caps) = rule.regex.captures(test_text) {
                    let action = if rule.suppress {
                        "SUPPRESS".to_string()
                    } else {
                        let formatted = match &rule.format {
                            Some(fmt) => flume_core::format::format_regex_captures(fmt, &caps),
                            None => test_text.to_string(),
                        };
                        let buf = rule.buffer.as_deref().unwrap_or("(server)");
                        format!("MATCH → buffer=\"{}\" text=\"{}\"", buf, formatted)
                    };
                    app.system_message(&format!("Rule {}: {}", i + 1, action));
                    matched = true;
                    break;
                }
            }
            if !matched {
                app.system_message("No rules matched. The text would use default server_notice format.");
                app.system_message(&format!("Text tested: \"{}\"", test_text));
            }
        }
        "last" => {
            // Show and suppress the last server/global notice.
            // Falls back to searching the active buffer for server messages.
            let rest = parts.get(1).copied().unwrap_or("").trim();
            let last_notice = app.last_raw_snotice.clone().or_else(|| {
                app.active_messages().iter().rev()
                    .find(|m| matches!(m.source, MessageSource::Server))
                    .map(|m| {
                        let t = &m.text;
                        if let Some(r) = t.strip_prefix("[notice] ") {
                            r.to_string()
                        } else if let Some(pos) = t.find("] ") {
                            t[pos + 2..].to_string()
                        } else {
                            t.clone()
                        }
                    })
            });
            if let Some(text) = last_notice {
                let raw = &text;
                if rest.is_empty() || rest == "suppress" {
                    let escaped = regex::escape(raw);
                    let rule = flume_core::config::formats::SnoticeRuleConfig {
                        pattern: escaped,
                        format: None,
                        buffer: None,
                        suppress: true,
                    };
                    app.snotice_configs.push(rule);
                    app.snotice_rules = crate::app::compile_snotice_rules(&app.snotice_configs);
                    app.system_message(&format!("Suppressed: {}", &raw[..raw.len().min(80)]));
                    app.system_message("Use /snotice save to persist");
                } else if rest.starts_with("route ") {
                    let route_args = rest.strip_prefix("route ").unwrap().trim();
                    // Parse: <buffer> [--format <fmt>]
                    let route_parts: Vec<&str> = route_args.splitn(2, " --format ").collect();
                    let buf_name = route_parts[0].trim();
                    let fmt = route_parts.get(1).map(|s| s.trim().to_string());
                    let escaped = regex::escape(raw);
                    let rule = flume_core::config::formats::SnoticeRuleConfig {
                        pattern: escaped,
                        format: fmt.clone(),
                        buffer: Some(buf_name.to_string()),
                        suppress: false,
                    };
                    app.snotice_configs.push(rule);
                    app.snotice_rules = crate::app::compile_snotice_rules(&app.snotice_configs);
                    if let Some(ref f) = fmt {
                        app.system_message(&format!("Routing to '{}' with format '{}'. /snotice save to persist", buf_name, f));
                    } else {
                        app.system_message(&format!("Routing to '{}'. /snotice save to persist", buf_name));
                    }
                } else if rest == "show" {
                    app.system_message(&format!("Last notice text:"));
                    app.system_message(raw);
                    app.system_message("");
                    app.system_message("  /snotice last                          — suppress it");
                    app.system_message("  /snotice last route <buf>              — route to buffer");
                    app.system_message("  /snotice last route <buf> --format <f> — route with format");
                    app.system_message("  /snotice last show                     — show raw text");
                } else {
                    app.system_message("Usage: /snotice last [suppress|route <buffer>|show]");
                }
                // Keep raw text for follow-up commands
                app.last_raw_snotice = Some(text);
            } else {
                app.system_message("No recent server notice found");
                app.system_message("Try switching to the server buffer first");
            }
        }
        _ => {
            app.system_message("Usage: /snotice add|suppress|list|remove|save|test|last");
        }
    }
}

fn handle_set_command(args: &str, app: &mut App) {
    let config_path = flume_core::config::config_dir().join("config.toml");
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    let mut config: toml::Table = toml::from_str(&existing).unwrap_or_default();

    if args.is_empty() {
        // List all settings
        app.system_message("Current settings (config.toml):");
        list_toml_table(&config, "", app);
        app.system_message(&format!("  Config file: {}", config_path.display()));
        app.system_message("  Usage: /set <key> <value> | /set <section>");
        return;
    }

    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let key = parts[0];
    let value = parts.get(1).map(|s| s.trim());

    if value.is_none() {
        // Show a section or single key
        let dot_parts: Vec<&str> = key.split('.').collect();
        if dot_parts.len() == 1 {
            // Show a section
            if let Some(toml::Value::Table(t)) = config.get(key) {
                app.system_message(&format!("[{}]:", key));
                for (k, v) in t {
                    app.system_message(&format!("  {}.{} = {}", key, k, format_toml_value(v)));
                }
            } else if let Some(defaults) = section_defaults(key) {
                // Section exists but has no custom settings — show defaults
                app.system_message(&format!("[{}]: (all defaults)", key));
                for (k, v) in defaults {
                    app.system_message(&format!("  {}.{} = {} (default)", key, k, v));
                }
            } else {
                app.system_message(&format!("Unknown section '{}'. Sections: general, ui, logging, notifications, ctcp, llm, dcc", key));
            }
        } else {
            // Show a single key
            let section = dot_parts[0];
            let field = dot_parts[1];
            if let Some(toml::Value::Table(t)) = config.get(section) {
                if let Some(v) = t.get(field) {
                    app.system_message(&format!("{} = {}", key, format_toml_value(v)));
                } else {
                    // Check if it's a known default
                    if let Some(defaults) = section_defaults(section) {
                        if let Some((_, dv)) = defaults.iter().find(|(k, _)| *k == field) {
                            app.system_message(&format!("{} = {} (default)", key, dv));
                        } else {
                            app.system_message(&format!("'{}' is not a known setting", key));
                        }
                    } else {
                        app.system_message(&format!("'{}' not set (using default)", key));
                    }
                }
            } else if let Some(defaults) = section_defaults(section) {
                if let Some((_, dv)) = defaults.iter().find(|(k, _)| *k == field) {
                    app.system_message(&format!("{} = {} (default)", key, dv));
                } else {
                    app.system_message(&format!("'{}' is not a known setting", key));
                }
            } else {
                app.system_message(&format!("Unknown section '{}'", section));
            }
        }
        return;
    }

    let value_str = value.unwrap();

    // Parse dotted key: section.field
    let dot_parts: Vec<&str> = key.split('.').collect();
    if dot_parts.len() != 2 {
        app.system_message("Key must be section.field (e.g., ui.theme, general.default_nick)");
        return;
    }

    let section = dot_parts[0];
    let field = dot_parts[1];

    // Parse value type
    let toml_value = if value_str == "true" {
        toml::Value::Boolean(true)
    } else if value_str == "false" {
        toml::Value::Boolean(false)
    } else if let Ok(n) = value_str.parse::<i64>() {
        toml::Value::Integer(n)
    } else if let Ok(f) = value_str.parse::<f64>() {
        toml::Value::Float(f)
    } else {
        toml::Value::String(value_str.to_string())
    };

    // Insert into config
    let table = config
        .entry(section)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let toml::Value::Table(ref mut t) = table {
        t.insert(field.to_string(), toml_value.clone());
    }

    // Save to disk
    let _ = std::fs::create_dir_all(flume_core::config::config_dir());
    match toml::to_string_pretty(&config) {
        Ok(toml_str) => {
            match std::fs::write(&config_path, &toml_str) {
                Ok(()) => {
                    app.system_message(&format!("{} = {} (saved)", key, format_toml_value(&toml_value)));

                    // Apply some settings immediately
                    match key {
                        "ui.theme" => {
                            if let toml::Value::String(ref name) = toml_value {
                                app.theme_switch = Some(name.clone());
                            }
                        }
                        "ui.show_join_part" => {
                            if let toml::Value::Boolean(b) = toml_value {
                                app.show_join_part = b;
                            }
                        }
                        "ui.show_hostmask_on_join" => {
                            if let toml::Value::Boolean(b) = toml_value {
                                app.show_hostmask_on_join = b;
                            }
                        }
                        _ => {
                            app.system_message("  (some settings require restart to take effect)");
                        }
                    }
                }
                Err(e) => app.system_message(&format!("Failed to save: {}", e)),
            }
        }
        Err(e) => app.system_message(&format!("Failed to serialize config: {}", e)),
    }
}

fn list_toml_table(table: &toml::Table, prefix: &str, app: &mut App) {
    for (k, v) in table {
        let full_key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{}.{}", prefix, k)
        };
        match v {
            toml::Value::Table(t) => {
                list_toml_table(t, &full_key, app);
            }
            _ => {
                app.system_message(&format!("  {} = {}", full_key, format_toml_value(v)));
            }
        }
    }
}

fn section_defaults(section: &str) -> Option<Vec<(&'static str, &'static str)>> {
    match section {
        "general" => Some(vec![
            ("default_nick", "\"flume_user\""),
            ("realname", "\"Flume User\""),
            ("username", "\"flume\""),
            ("quit_message", "\"Flume IRC\""),
            ("timestamp_format", "\"%H:%M:%S\""),
            ("scrollback_lines", "10000"),
            ("url_open_command", "\"open\" (macOS) / \"xdg-open\" (Linux)"),
        ]),
        "ui" => Some(vec![
            ("theme", "\"default\""),
            ("show_server_tree", "true"),
            ("show_nick_list", "true"),
            ("server_tree_width", "20"),
            ("nick_list_width", "18"),
            ("input_history_size", "500"),
            ("tick_rate_fps", "30"),
            ("show_join_part", "true"),
            ("show_hostmask_on_join", "true"),
        ]),
        "logging" => Some(vec![
            ("enabled", "true"),
            ("format", "\"plain\""),
            ("rotate", "\"daily\""),
        ]),
        "notifications" => Some(vec![
            ("highlight_bell", "true"),
            ("highlight_words", "[]"),
            ("notify_private", "true"),
            ("notify_highlight", "true"),
        ]),
        "ctcp" => Some(vec![
            ("version_reply", "\"Flume <version>\""),
            ("respond_to_version", "true"),
            ("respond_to_ping", "true"),
            ("respond_to_time", "true"),
            ("rate_limit", "3"),
        ]),
        "llm" => Some(vec![
            ("provider", "\"anthropic\""),
            ("api_key_secret", "\"flume_llm_key\""),
            ("model", "\"claude-sonnet-4-20250514\""),
            ("temperature", "0.3"),
            ("max_tokens", "4096"),
        ]),
        "dcc" => Some(vec![
            ("enabled", "false"),
            ("auto_accept", "false"),
            ("download_directory", "\"~/Downloads/flume\""),
            ("port_range", "[1024, 65535]"),
            ("passive", "true"),
            ("max_transfer_size", "0"),
        ]),
        _ => None,
    }
}

fn format_toml_value(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => format!("\"{}\"", s),
        toml::Value::Integer(n) => n.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Array(a) => {
            let items: Vec<String> = a.iter().map(format_toml_value).collect();
            format!("[{}]", items.join(", "))
        }
        toml::Value::Table(_) => "{...}".to_string(),
        _ => format!("{}", v),
    }
}

/// Parse --name <name> from generate args, returning (name, remaining description).
fn parse_generate_name(args: &str) -> (Option<String>, String) {
    let words: Vec<&str> = args.split_whitespace().collect();
    let mut name: Option<String> = None;
    let mut desc_words: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if words[i] == "--name" {
            i += 1;
            if i < words.len() {
                name = Some(words[i].to_string());
            }
        } else {
            desc_words.push(words[i]);
        }
        i += 1;
    }
    (name, desc_words.join(" "))
}

fn handle_generate_command(args: &str, app: &mut App) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let subcmd = parts.first().copied().unwrap_or("");

    match subcmd {
        "init" | "setup" => {
            app.system_message("LLM Generation Setup");
            app.system_message("Choose your provider:");
            app.system_message("  1) Anthropic (Claude)");
            app.system_message("  2) OpenAI (GPT)");
            app.system_message("Type 1 or 2:");
            app.generate_init_step = Some(1);
        }
        "accept" => {
            if app.pending_generation.is_some() {
                // Signal main loop to save and load the generation
                app.script_command = Some("_accept_generation".to_string());
            } else {
                app.system_message("No pending generation to accept");
            }
        }
        "reject" => {
            if app.pending_generation.take().is_some() {
                app.system_message("Generation discarded");
            } else {
                app.system_message("No pending generation to reject");
            }
        }
        "script" => {
            if app.generating {
                app.system_message("Generation already in progress...");
                return;
            }
            let rest = parts.get(1).copied().unwrap_or("").trim();

            // Parse flags: --lua, --python, --name <name>
            let mut language = Some("lua".to_string());
            let mut gen_name: Option<String> = None;
            let mut desc_words: Vec<&str> = Vec::new();

            let words: Vec<&str> = rest.split_whitespace().collect();
            let mut i = 0;
            while i < words.len() {
                match words[i] {
                    "--python" | "--py" => language = Some("python".to_string()),
                    "--lua" => language = Some("lua".to_string()),
                    "--name" => {
                        i += 1;
                        if i < words.len() {
                            gen_name = Some(words[i].to_string());
                        }
                    }
                    _ => desc_words.push(words[i]),
                }
                i += 1;
            }
            let description = desc_words.join(" ");

            if description.is_empty() {
                app.system_message("Usage: /generate script [--lua|--python] [--name <name>] <description>");
                return;
            }

            app.generate_request = Some(GenerateRequest {
                kind: GenerationKind::Script,
                language,
                description,
                name: gen_name,
            });
            app.system_message("Generating script...");
        }
        "theme" => {
            if app.generating {
                app.system_message("Generation already in progress...");
                return;
            }
            let rest = parts.get(1).copied().unwrap_or("").trim();
            let (gen_name, description) = parse_generate_name(rest);
            if description.is_empty() {
                app.system_message("Usage: /generate theme [--name <name>] <description>");
                return;
            }
            app.generate_request = Some(GenerateRequest {
                kind: GenerationKind::Theme,
                language: None,
                description,
                name: gen_name,
            });
            app.system_message("Generating theme...");
        }
        "layout" => {
            if app.generating {
                app.system_message("Generation already in progress...");
                return;
            }
            let rest = parts.get(1).copied().unwrap_or("").trim();
            let (gen_name, description) = parse_generate_name(rest);
            if description.is_empty() {
                app.system_message("Usage: /generate layout [--name <name>] <description>");
                return;
            }
            app.generate_request = Some(GenerateRequest {
                kind: GenerationKind::Layout,
                language: None,
                description,
                name: gen_name,
            });
            app.system_message("Generating layout...");
        }
        "" => {
            app.system_message("Usage: /generate init|script|theme|layout <description>");
            app.system_message("  /generate init    — setup instructions for LLM");
            app.system_message("  /generate script [--lua|--python] <what you want>");
            app.system_message("  /generate theme <describe the look>");
            app.system_message("  /generate layout <describe the split>");
            app.system_message("  /generate accept  — save pending generation");
            app.system_message("  /generate reject  — discard pending generation");
        }
        _ => {
            app.system_message("Usage: /generate init|script|theme|layout <description>");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_boundary_left_basic() {
        assert_eq!(word_boundary_left("hello world", 11), 6);
        assert_eq!(word_boundary_left("hello world", 6), 0);
        assert_eq!(word_boundary_left("hello world", 5), 0);
        assert_eq!(word_boundary_left("hello world", 0), 0);
    }

    #[test]
    fn word_boundary_left_multiple_spaces() {
        assert_eq!(word_boundary_left("foo  bar  baz", 13), 10);
        assert_eq!(word_boundary_left("foo  bar  baz", 9), 5);
    }

    #[test]
    fn word_boundary_right_basic() {
        assert_eq!(word_boundary_right("hello world", 0), 6);
        assert_eq!(word_boundary_right("hello world", 6), 11);
        assert_eq!(word_boundary_right("hello world", 11), 11);
    }

    #[test]
    fn word_boundary_right_multiple_spaces() {
        assert_eq!(word_boundary_right("foo  bar  baz", 0), 5);
        assert_eq!(word_boundary_right("foo  bar  baz", 5), 10);
    }

    #[test]
    fn word_boundary_empty_string() {
        assert_eq!(word_boundary_left("", 0), 0);
        assert_eq!(word_boundary_right("", 0), 0);
    }

    #[test]
    fn word_boundary_single_word() {
        assert_eq!(word_boundary_left("hello", 5), 0);
        assert_eq!(word_boundary_right("hello", 0), 5);
    }
}
