mod app;
mod input;
pub mod keybindings;
pub mod split;
pub mod theme;
mod ui;
pub mod url;

use std::time::{Duration, Instant};

use crossterm::event::EventStream;
use futures::StreamExt;
use tokio::sync::mpsc;

use flume_core::config;
use flume_core::config::vault::Vault;
use flume_core::connection::ServerConnection;
use flume_core::event::{IrcEvent, UserCommand};
use flume_core::irc::command::Command;
use flume_core::logging::Logger;
use flume_core::dcc::{self, DccEvent, DccTransfer, DccTransferState};
use flume_core::scripting::{ScriptAction, ScriptEvent, ScriptManager};

use app::{GenerationKind, InputMode, PendingGeneration};

/// Parsed command-line arguments.
struct CliArgs {
    server: Option<String>,
    nick: Option<String>,
    no_autoconnect: bool,
    no_autoload_scripts: bool,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs {
        server: None,
        nick: None,
        no_autoconnect: false,
        no_autoload_scripts: false,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-v" | "--version" => {
                println!("Flume {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-h" | "--help" => {
                println!("Flume {} — modern terminal IRC client", env!("CARGO_PKG_VERSION"));
                println!();
                println!("Usage: flume [OPTIONS]");
                println!();
                println!("Options:");
                println!("  -v, --version              Print version and exit");
                println!("  -h, --help                 Show this help and exit");
                println!("  -s, --server <name>        Connect to a specific server on startup");
                println!("  -n, --nick <nick>          Override the default nickname");
                println!("  -x, --no-autoconnect       Don't auto-connect to servers");
                println!("      --no-autoload-scripts  Don't auto-load scripts on startup");
                println!();
                println!("Config: ~/.config/flume/config.toml");
                println!("Docs:   https://docs.flumeirc.io");
                std::process::exit(0);
            }
            "-s" | "--server" => {
                i += 1;
                cli.server = args.get(i).cloned();
            }
            "-n" | "--nick" => {
                i += 1;
                cli.nick = args.get(i).cloned();
            }
            "-x" | "--no-autoconnect" => {
                cli.no_autoconnect = true;
            }
            "--no-autoload-scripts" => {
                cli.no_autoload_scripts = true;
            }
            other => {
                eprintln!("Unknown option: {}", other);
                eprintln!("Try 'flume --help' for usage");
                std::process::exit(1);
            }
        }
        i += 1;
    }
    cli
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args();

    // Set up logging to file
    let log_dir = config::data_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_file = std::fs::File::create(log_dir.join("flume.log"))?;
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_env_filter("flume=debug")
        .with_ansi(false)
        .init();

    tracing::info!("Flume starting");

    // Load config
    let flume_config = config::load_config().unwrap_or_default();
    let irc_config = config::load_irc_config().unwrap_or_default();

    // Try to unlock vault from env var
    let mut vault: Option<Vault> = std::env::var("FLUME_VAULT_PASS").ok().and_then(|pass| {
        let path = config::vault_path();
        match Vault::load(path.clone(), pass.clone()) {
            Ok(v) => Some(v),
            Err(flume_core::config::vault::VaultError::NotFound) => {
                let v = Vault::new(path, pass);
                let _ = v.save();
                Some(v)
            }
            Err(e) => {
                tracing::error!("Failed to load vault via FLUME_VAULT_PASS: {}", e);
                None
            }
        }
    });

    // Load theme
    let mut theme = theme::Theme::load(&flume_config.ui.theme);
    tracing::info!("Loaded theme: {}", theme.name);

    // Set up IRC message logger
    let mut logger = Logger::new(flume_config.logging.clone());

    // Set up app state
    let mut app = app::App::new(
        flume_config.general.scrollback_lines,
        &flume_config.general.timestamp_format,
        flume_config.notifications.clone(),
        flume_config.general.url_open_command.clone(),
        flume_config.ui.keybindings.mode,
        flume_config.ui.show_join_part,
        flume_config.ui.show_hostmask_on_join,
        flume_config.formats.clone(),
        {
            let mut combos = flume_core::config::combos::default_combos();
            combos.extend(flume_config.combos.combos.clone());
            combos
        },
        flume_config.aliases.clone(),
        flume_config.ui.mouse,
        {
            use crate::split::SplitDirection;
            flume_config.groups.iter().map(|(name, cfg)| {
                let channels = if cfg.channels.len() >= 2 {
                    [cfg.channels[0].clone(), cfg.channels[1].clone()]
                } else {
                    [cfg.channels.first().cloned().unwrap_or_default(), String::new()]
                };
                let ratio_parts: Vec<&str> = cfg.ratio.split(':').collect();
                let ratio = ratio_parts.first()
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(50);
                let direction = match cfg.direction.as_str() {
                    "horizontal" | "h" => SplitDirection::Horizontal,
                    _ => SplitDirection::Vertical,
                };
                (name.clone(), app::BufferGroup { channels, ratio, direction })
            }).collect()
        },
    );
    app.irc_config = irc_config;
    app.active_theme = flume_config.ui.theme.clone();

    // Set up scripting engine
    let mut script_manager = match ScriptManager::new() {
        Ok(mgr) => {
            tracing::info!("Script engine initialized");
            Some(mgr)
        }
        Err(e) => {
            tracing::error!("Failed to initialize script engine: {}", e);
            None
        }
    };

    // Pass vault secrets to script engine
    if let Some(ref mgr) = script_manager {
        if let Some(ref v) = vault {
            let secrets: std::collections::HashMap<String, String> = v
                .list()
                .into_iter()
                .filter_map(|name| v.get(name).map(|val| (name.to_string(), val.to_string())))
                .collect();
            mgr.set_vault_secrets(secrets);
        }
    }

    // Load autoload scripts (unless --no-autoload-scripts)
    if !cli.no_autoload_scripts {
        if let Some(ref mut mgr) = script_manager {
            let results = mgr.load_autoload();
            for (name, result) in &results {
                match result {
                    Ok(()) => tracing::info!("Loaded script: {}", name),
                    Err(e) => tracing::error!("Failed to load script {}: {}", name, e),
                }
            }
        }
    }

    // LLM client — lazily initialized on first /generate use
    let mut llm_client: Option<std::sync::Arc<flume_core::llm::LlmClient>> = None;

    // Channel for receiving LLM generation results
    // (kind, language, code, description, user_name)
    let (gen_tx, mut gen_rx) = mpsc::channel::<Result<(GenerationKind, Option<String>, String, String, Option<String>), String>>(1);

    // Channel for DCC events (progress, completion, chat messages)
    let (dcc_tx, mut dcc_rx) = mpsc::channel::<DccEvent>(256);

    // Apply CLI nick override
    if let Some(ref nick) = cli.nick {
        app.irc_config.networks.iter_mut().for_each(|n| {
            n.nick = Some(nick.clone());
        });
    }

    // Determine which servers to connect on startup
    let servers_to_connect: Vec<String> = if let Some(ref name) = cli.server {
        vec![name.clone()]
    } else if cli.no_autoconnect {
        Vec::new()
    } else {
        // Connect all autoconnect servers
        app.irc_config
            .networks
            .iter()
            .filter(|n| n.autoconnect)
            .map(|n| n.name.clone())
            .collect()
    };

    let has_servers = !servers_to_connect.is_empty();

    // Vault passphrase prompt
    let vault_file_exists = config::vault_path().exists();
    let need_passphrase_prompt = vault.is_none() && vault_file_exists;

    if need_passphrase_prompt {
        app.input_mode = InputMode::Passphrase("Vault passphrase (Enter to skip)".to_string());
        app.show_splash = false;
        app.system_message("Vault found. Enter passphrase to unlock (or press Enter to skip):");
    } else {
        app.vault_unlocked = true;
    }

    // Show splash only on truly fresh installs (no config, no vault, no servers)
    if has_servers || need_passphrase_prompt || flume_core::config::config_dir().join("config.toml").exists() {
        app.show_splash = false;
    }

    if !has_servers && !need_passphrase_prompt {
        app.system_message("No networks to connect. Add one with:");
        app.system_message("  /server add <name> <address> [port] [-tls|-notls] [-autoconnect]");
        app.system_message("  /save");
        app.system_message("  /connect <name>");
    }

    // Set up panic hook
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        ratatui::restore();
        original_hook(panic_info);
    }));

    let mut terminal = ratatui::init();

    // Enable mouse capture if configured
    if app.mouse_enabled {
        crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture).ok();
    }

    // Spawn crossterm event reader
    let (term_tx, mut term_rx) = mpsc::channel(100);
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(event)) = reader.next().await {
            if term_tx.send(event).await.is_err() {
                break;
            }
        }
    });

    // Server event receivers — polled directly, no bridge tasks.
    let mut server_event_rxs: Vec<mpsc::UnboundedReceiver<IrcEvent>> = Vec::new();
    // Also keep a collector for compatibility with spawn_connection
    let (event_collector_tx, mut event_collector_rx) = mpsc::unbounded_channel::<IrcEvent>();

    let mut autojoin_sent: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut initial_connections_spawned = false;

    if app.vault_unlocked && has_servers {
        for name in &servers_to_connect {
            spawn_connection(name, &flume_config, &vault, &event_collector_tx, &mut app);
        }
        initial_connections_spawned = true;
    }

    let tick_rate = Duration::from_millis(1000 / flume_config.ui.tick_rate_fps.max(1) as u64);
    let mut last_render = Instant::now();

    loop {
        // ── Step 1: select! waits for any event ──
        tokio::select! {
            Some(event) = event_collector_rx.recv() => {
                let server_name = match &event {
                    IrcEvent::Connected { server_name, .. } => server_name.clone(),
                    IrcEvent::Disconnected { server_name, .. } => server_name.clone(),
                    IrcEvent::MessageReceived { server_name, .. } => server_name.clone(),
                    IrcEvent::StateChanged { server_name, .. } => server_name.clone(),
                    IrcEvent::Error { server_name, .. } => server_name.clone(),
                };

                // Auto-join channels on connect
                if let IrcEvent::Connected { .. } = &event {
                    if !autojoin_sent.contains(&server_name) {
                        autojoin_sent.insert(server_name.clone());
                        let autojoin = app.irc_config.find(&server_name)
                            .map(|e| e.autojoin.clone())
                            .unwrap_or_default();
                        if let Some(ss) = app.servers.get(&server_name) {
                            if let Some(tx) = &ss.command_tx {
                                for channel in &autojoin {
                                    let _ = tx.send(UserCommand::Join {
                                        channel: channel.clone(),
                                        key: None,
                                    }).await;
                                }
                            }
                        }
                    }
                }

                // Dispatch to script engine before TUI processing
                if let Some(ref mgr) = script_manager {
                    let script_event = irc_event_to_script_event(&event);
                    if let Some(se) = script_event {
                        let result = mgr.dispatch_event(se);
                        if result.cancelled {
                            // Script cancelled this event — skip TUI processing
                            // but still process any script actions
                            process_script_actions(mgr, &mut app);
                            continue;
                        }
                    }
                }

                // Check for DCC offers in CTCP messages
                if let IrcEvent::MessageReceived { ref server_name, ref message } = event {
                    if let Command::Privmsg { ref text, .. } = message.command {
                        if text.starts_with('\x01') && text.ends_with('\x01') {
                            let inner = &text[1..text.len()-1];
                            if inner.starts_with("DCC ") {
                                let nick = message.prefix_nick().unwrap_or("");
                                if let Some(dcc_msg) = dcc::parse_dcc_ctcp(&inner[4..], nick, server_name) {
                                    match dcc_msg {
                                        dcc::DccCtcpMessage::Offer(offer) => {
                                            let kind = match offer.dcc_type {
                                                dcc::DccType::Send => "SEND",
                                                dcc::DccType::Chat => "CHAT",
                                            };
                                            let name = offer.filename.as_deref().unwrap_or("(chat)");
                                            let size_str = if offer.size > 0 {
                                                format!(" ({})", dcc::format_size(offer.size))
                                            } else {
                                                String::new()
                                            };
                                            let id = offer.id;
                                            app.system_message(&format!(
                                                "DCC {} offer from {} — {}{}  [/dcc accept {}]",
                                                kind, nick, name, size_str, id
                                            ));
                                            app.dcc_transfers.push(DccTransfer::from_offer(offer));
                                        }
                                        _ => {} // Resume/Accept handled at protocol level
                                    }
                                }
                            }
                        }
                    }
                }

                let notifications = app.handle_irc_event(&event);

                // Process script actions from event handlers
                if let Some(ref mgr) = script_manager {
                    process_script_actions(mgr, &mut app);
                }

                // Fire notifications (bell + desktop)
                for notif in &notifications {
                    // Terminal bell
                    if flume_config.notifications.highlight_bell {
                        print!("\x07");
                    }
                    // Desktop notification (macOS)
                    match notif {
                        app::NotificationEvent::Highlight { nick, text, .. } => {
                            if flume_config.notifications.notify_highlight {
                                send_desktop_notification(
                                    &format!("Highlight from {}", nick),
                                    text,
                                );
                            }
                        }
                        app::NotificationEvent::PrivateMessage { nick, text, .. } => {
                            if flume_config.notifications.notify_private {
                                send_desktop_notification(
                                    &format!("PM from {}", nick),
                                    text,
                                );
                            }
                        }
                    }
                }

                // Log IRC messages to disk
                if let IrcEvent::MessageReceived { ref server_name, ref message } = event {
                    let ts = message.server_time.unwrap_or_else(chrono::Utc::now);
                    match &message.command {
                        Command::Privmsg { ref target, ref text } => {
                            if let Some(nick) = message.prefix_nick() {
                                // Check for CTCP ACTION (/me)
                                if text.starts_with('\x01') && text.ends_with('\x01') {
                                    let inner = &text[1..text.len()-1];
                                    if let Some(action_text) = inner.strip_prefix("ACTION ") {
                                        logger.log_action(server_name, target, ts, nick, action_text);
                                    }
                                } else {
                                    logger.log_message(server_name, target, ts, nick, text);
                                }
                            }
                        }
                        Command::Join { ref channels } => {
                            if let Some(nick) = message.prefix_nick() {
                                for (channel, _) in channels {
                                    logger.log_event(server_name, channel, ts,
                                        &format!("{} joined {}", nick, channel));
                                }
                            }
                        }
                        Command::Part { ref channels, message: ref part_msg } => {
                            if let Some(nick) = message.prefix_nick() {
                                let reason = part_msg.as_deref().unwrap_or("");
                                for channel in channels {
                                    if reason.is_empty() {
                                        logger.log_event(server_name, channel, ts,
                                            &format!("{} left {}", nick, channel));
                                    } else {
                                        logger.log_event(server_name, channel, ts,
                                            &format!("{} left {} ({})", nick, channel, reason));
                                    }
                                }
                            }
                        }
                        Command::Quit { message: ref quit_msg } => {
                            if let Some(nick) = message.prefix_nick() {
                                let reason = quit_msg.as_deref().unwrap_or("");
                                // Quit is logged to all channels the user was in,
                                // but we only have access to the server-level here.
                                // The TUI handles per-channel cleanup.
                                if reason.is_empty() {
                                    logger.log_event(server_name, "", ts,
                                        &format!("{} quit", nick));
                                } else {
                                    logger.log_event(server_name, "", ts,
                                        &format!("{} quit ({})", nick, reason));
                                }
                            }
                        }
                        Command::Kick { ref channel, ref user, ref reason } => {
                            if let Some(nick) = message.prefix_nick() {
                                let reason = reason.as_deref().unwrap_or("");
                                if reason.is_empty() {
                                    logger.log_event(server_name, channel, ts,
                                        &format!("{} kicked {} from {}", nick, user, channel));
                                } else {
                                    logger.log_event(server_name, channel, ts,
                                        &format!("{} kicked {} from {} ({})", nick, user, channel, reason));
                                }
                            }
                        }
                        Command::Topic { ref channel, ref topic } => {
                            if let Some(nick) = message.prefix_nick() {
                                if let Some(ref topic) = topic {
                                    logger.log_event(server_name, channel, ts,
                                        &format!("{} changed topic to: {}", nick, topic));
                                }
                            }
                        }
                        Command::Nick { ref nickname } => {
                            if let Some(old_nick) = message.prefix_nick() {
                                logger.log_event(server_name, "", ts,
                                    &format!("{} is now known as {}", old_nick, nickname));
                            }
                        }
                        _ => {}
                    }
                }

                // Drain remaining queued events without blocking.
                // Prevents TUI from falling behind during bursts (ZNC playback, floods).
                while let Ok(event) = event_collector_rx.try_recv() {
                // Inline: extract server name and process
                let server_name = match &event {
                    IrcEvent::Connected { server_name, .. }
                    | IrcEvent::Disconnected { server_name, .. }
                    | IrcEvent::MessageReceived { server_name, .. }
                    | IrcEvent::StateChanged { server_name, .. }
                    | IrcEvent::Error { server_name, .. } => server_name.clone(),
                };
                if let Some(ref mgr) = script_manager {
                    let script_event = irc_event_to_script_event(&event);
                    if let Some(se) = script_event {
                        let result = mgr.dispatch_event(se);
                        if result.cancelled {
                            process_script_actions(mgr, &mut app);
                            continue;
                        }
                    }
                }
                // Check for DCC offers
                if let IrcEvent::MessageReceived { ref server_name, ref message } = event {
                    if let Command::Privmsg { ref text, .. } = message.command {
                        if text.starts_with('\x01') && text.ends_with('\x01') {
                            let inner = &text[1..text.len()-1];
                            if inner.starts_with("DCC ") {
                                let nick = message.prefix_nick().unwrap_or("");
                                if let Some(dcc_msg) = dcc::parse_dcc_ctcp(&inner[4..], nick, server_name) {
                                    if let dcc::DccCtcpMessage::Offer(offer) = dcc_msg {
                                        let kind = match offer.dcc_type {
                                            dcc::DccType::Send => "SEND",
                                            dcc::DccType::Chat => "CHAT",
                                        };
                                        let name = offer.filename.as_deref().unwrap_or("(chat)");
                                        let id = offer.id;
                                        app.system_message(&format!(
                                            "DCC {} offer from {} — {}  [/dcc accept {}]",
                                            kind, nick, name, id
                                        ));
                                        app.dcc_transfers.push(DccTransfer::from_offer(offer));
                                    }
                                }
                            }
                        }
                    }
                }
                let notifications = app.handle_irc_event(&event);
                if let Some(ref mgr) = script_manager {
                    process_script_actions(mgr, &mut app);
                }
                for notif in &notifications {
                    if flume_config.notifications.highlight_bell {
                        print!("\x07");
                    }
                    match notif {
                        app::NotificationEvent::Highlight { nick, text, .. } => {
                            if flume_config.notifications.notify_highlight {
                                send_desktop_notification(&format!("Highlight from {}", nick), text);
                            }
                        }
                        app::NotificationEvent::PrivateMessage { nick, text, .. } => {
                            if flume_config.notifications.notify_private {
                                send_desktop_notification(&format!("PM from {}", nick), text);
                            }
                        }
                    }
                }
                }
            }
            Some(event) = term_rx.recv() => {
                input::handle_input(&mut app, event, &mut vault).await;

                // Apply mouse capture state change
                if app.mouse_changed {
                    if app.mouse_enabled {
                        crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture).ok();
                    } else {
                        crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture).ok();
                    }
                    app.mouse_changed = false;
                }

                // Check if vault was just unlocked — spawn initial connections
                if app.vault_unlocked && !initial_connections_spawned && has_servers {
                    for name in &servers_to_connect {
                        spawn_connection(name, &flume_config, &vault, &event_collector_tx, &mut app);
                    }
                    initial_connections_spawned = true;
                }

                // Check if /theme was requested
                if let Some(name) = app.theme_switch.take() {
                    if name == "__reload__" {
                        if theme.has_file() {
                            let old_name = theme.name.clone();
                            if theme.force_reload() {
                                app.system_message(&format!("Theme '{}' reloaded", old_name));
                            } else {
                                let path_str = theme.file_path()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|| "unknown".to_string());
                                app.system_message(&format!("Failed to reload theme (file: {})", path_str));
                            }
                        } else {
                            app.system_message("No theme file to reload (using default)");
                        }
                    } else {
                        let path = flume_core::config::themes_dir().join(format!("{}.toml", name));
                        if path.exists() {
                            theme.switch_to(&name);
                            app.active_theme = name.clone();
                            app.system_message(&format!("Theme switched to '{}'", name));
                        } else {
                            app.system_message(&format!(
                                "Theme '{}' not found at {}", name, path.display()
                            ));
                            app.system_message(&format!(
                                "Copy theme files to: {}", flume_core::config::themes_dir().display()
                            ));
                        }
                    }
                }

                // Check if /script was requested
                if let Some(args) = app.script_command.take() {
                    if let Some(ref mut mgr) = script_manager {
                        handle_script_command(&args, mgr, &mut app, &mut vault);
                    } else {
                        app.system_message("Script engine not available");
                    }
                }

                // Check if /generate was requested
                if let Some(req) = app.generate_request.take() {
                    // Lazy-init LLM client: re-read config and vault each time
                    if llm_client.is_none() {
                        let llm_config = config::load_config()
                            .map(|c| c.llm)
                            .unwrap_or_default();
                        let secret_name = &llm_config.api_key_secret;
                        let api_key = vault
                            .as_ref()
                            .and_then(|v| v.get(secret_name).map(|s| s.to_string()))
                            .or_else(|| std::env::var(
                                secret_name.to_uppercase().replace(' ', "_")
                            ).ok());

                        if let Some(key) = api_key {
                            tracing::info!("LLM client initialized (provider: {:?})", llm_config.provider);
                            llm_client = Some(std::sync::Arc::new(
                                flume_core::llm::LlmClient::new(llm_config, key),
                            ));
                        }
                    }

                    if let Some(ref client) = llm_client {
                        app.generating = true;
                        let client = std::sync::Arc::clone(client);
                        let tx = gen_tx.clone();
                        tokio::spawn(async move {
                            let system_prompt = match req.kind {
                                GenerationKind::Script => {
                                    flume_core::llm::prompts::script_system_prompt(
                                        req.language.as_deref().unwrap_or("lua"),
                                    )
                                }
                                GenerationKind::Theme => {
                                    flume_core::llm::prompts::theme_system_prompt()
                                }
                                GenerationKind::Layout => {
                                    flume_core::llm::prompts::layout_system_prompt()
                                }
                            };

                            let llm_req = flume_core::llm::LlmRequest {
                                system: system_prompt,
                                user: req.description.clone(),
                            };

                            match client.generate(llm_req).await {
                                Ok(resp) => {
                                    let code = flume_core::llm::extract_code(&resp.content);
                                    let _ = tx.send(Ok((
                                        req.kind,
                                        req.language,
                                        code,
                                        req.description,
                                        req.name,
                                    ))).await;
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(e.to_string())).await;
                                }
                            }
                        });
                    } else {
                        app.system_message("LLM not configured. Run /generate init for setup instructions.");
                    }
                }

                // Check if /dcc command was requested
                if let Some(dcc_cmd) = app.dcc_command.take() {
                    handle_dcc_tui_command(&dcc_cmd, &mut app, &flume_config, &dcc_tx);
                }

                // Check if /connect was requested
                if let Some(name) = app.connect_to.take() {
                    spawn_connection(&name, &flume_config, &vault, &event_collector_tx, &mut app);
                    // Switch to the new server
                    app.switch_server(&name);
                }

                if app.should_quit {
                    break;
                }
            }

            // DCC events (progress, completion, chat messages)
            Some(event) = dcc_rx.recv() => {
                match event {
                    DccEvent::Progress { id, bytes, total } => {
                        if let Some(t) = app.dcc_transfers.iter_mut().find(|t| t.id == id) {
                            t.state = DccTransferState::Active { bytes_transferred: bytes, total };
                        }
                    }
                    DccEvent::Complete { id } => {
                        let name = app.dcc_transfers.iter()
                            .find(|t| t.id == id)
                            .and_then(|t| t.offer.filename.clone())
                            .unwrap_or_else(|| "transfer".to_string());
                        if let Some(t) = app.dcc_transfers.iter_mut().find(|t| t.id == id) {
                            t.state = DccTransferState::Complete;
                        }
                        app.system_message(&format!("DCC #{} ({}) complete", id, name));
                    }
                    DccEvent::Failed { id, error } => {
                        let name = app.dcc_transfers.iter()
                            .find(|t| t.id == id)
                            .and_then(|t| t.offer.filename.clone())
                            .unwrap_or_else(|| "transfer".to_string());
                        if let Some(t) = app.dcc_transfers.iter_mut().find(|t| t.id == id) {
                            t.state = DccTransferState::Failed(error.clone());
                        }
                        app.system_message(&format!("DCC #{} ({}) failed: {}", id, name, error));
                    }
                    DccEvent::ChatMessage { id, text } => {
                        app.system_message(&format!("[DCC CHAT #{}] {}", id, text));
                    }
                    DccEvent::ChatDisconnected { id } => {
                        app.dcc_chat_senders.remove(&id);
                        if let Some(t) = app.dcc_transfers.iter_mut().find(|t| t.id == id) {
                            t.state = DccTransferState::Complete;
                        }
                        app.system_message(&format!("DCC CHAT #{} disconnected", id));
                    }
                }
            }

            // LLM generation result
            Some(result) = gen_rx.recv() => {
                app.generating = false;
                match result {
                    Ok((kind, language, content, description, user_name)) => {
                        let ext = match kind {
                            GenerationKind::Script => match language.as_deref() {
                                Some("python") | Some("py") => "py",
                                _ => "lua",
                            },
                            GenerationKind::Theme => "toml",
                            GenerationKind::Layout => "toml",
                        };
                        let name = match user_name {
                            Some(n) => format!("{}.{}", n, ext),
                            None => slugify_name(&description, ext),
                        };
                        app.pending_generation = Some(PendingGeneration {
                            kind,
                            language,
                            content,
                            name,
                            description,
                        });
                        app.system_message("Generation complete — review in split pane");
                        app.system_message("  /generate accept  — save and load");
                        app.system_message("  /generate reject  — discard");
                    }
                    Err(e) => {
                        app.system_message(&format!("Generation failed: {}", e));
                    }
                }
            }

            // Render tick (theme hot-reload and time-based updates)
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(last_render + tick_rate)) => {
                theme.check_reload();
                if let Some(ref mgr) = script_manager {
                    process_script_actions(mgr, &mut app);
                }
            }
        }

        // ── Final drain: process any IRC events that arrived during select! ──
        while let Ok(event) = event_collector_rx.try_recv() {
            let notifications = app.handle_irc_event(&event);
            if let Some(ref mgr) = script_manager {
                process_script_actions(mgr, &mut app);
            }
            for notif in &notifications {
                if flume_config.notifications.highlight_bell { print!("\x07"); }
                match notif {
                    app::NotificationEvent::Highlight { nick, text, .. } => {
                        if flume_config.notifications.notify_highlight {
                            send_desktop_notification(&format!("Highlight from {}", nick), text);
                        }
                    }
                    app::NotificationEvent::PrivateMessage { nick, text, .. } => {
                        if flume_config.notifications.notify_private {
                            send_desktop_notification(&format!("PM from {}", nick), text);
                        }
                    }
                }
            }
        }

        // Render after every event — ensures display is always current
        terminal.draw(|frame| ui::render(frame, &mut app, &theme))?;
        last_render = Instant::now();
    }

    logger.flush();
    crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture).ok();
    ratatui::restore();
    tracing::info!("Flume exiting");
    Ok(())
}

/// Spawn a connection for a server and register it in the app.
fn spawn_connection(
    name: &str,
    flume_config: &flume_core::config::general::FlumeConfig,
    vault: &Option<Vault>,
    event_collector_tx: &mpsc::UnboundedSender<IrcEvent>,
    app: &mut app::App,
) {
    let server_config = match config::load_server_config(name) {
        Ok(sc) => sc,
        Err(e) => {
            app.system_message(&format!("Failed to load config for '{}': {}", name, e));
            return;
        }
    };

    let display_name = server_config.server.name.clone();
    let nick = server_config
        .identity
        .nick
        .as_deref()
        .unwrap_or(&flume_config.general.default_nick);

    // Create server state in app
    app.add_server(&display_name, nick);

    let (conn, handle) =
        ServerConnection::new(server_config, flume_config.general.clone(), vault.clone(), flume_config.ctcp.clone());
    tokio::spawn(conn.run());

    // Store command_tx in server state
    if let Some(ss) = app.servers.get_mut(&display_name) {
        ss.command_tx = Some(handle.command_tx);
    }

    // Store the event receiver — polled directly in the main loop
    // Store receiver — polled directly in main loop, no bridge task
    event_collector_tx.send(IrcEvent::StateChanged {
        server_name: display_name.clone(),
        state: flume_core::event::ConnectionState::Connecting,
    }).ok();
    // Push receiver for direct polling
    let collector_tx = event_collector_tx.clone();
    let mut rx = handle.event_rx;
    // Forwarding task — but uses recv_many for batch processing
    tokio::spawn(async move {
        let mut buf = Vec::with_capacity(256);
        loop {
            // recv_many: waits for at least 1, returns up to 256 at once
            let n = rx.recv_many(&mut buf, 256).await;
            if n == 0 { break; } // channel closed
            for event in buf.drain(..) {
                if collector_tx.send(event).is_err() { return; }
            }
        }
    });

    app.system_message_to(&display_name, &format!("Connecting to {}...", display_name));
}

/// Send a desktop notification using platform-native tools.
fn send_desktop_notification(title: &str, body: &str) {
    // Truncate body for notification display
    let short_body: String = body.chars().take(100).collect();
    let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_body = short_body.replace('\\', "\\\\").replace('"', "\\\"");

    if cfg!(target_os = "macos") {
        let _ = std::process::Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "display notification \"{}\" with title \"{}\"",
                    escaped_body, escaped_title,
                ),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    } else {
        // Linux / FreeBSD fallback
        let _ = std::process::Command::new("notify-send")
            .args([title, &short_body])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Handle /script subcommands.
fn handle_script_command(args: &str, mgr: &mut ScriptManager, app: &mut app::App, vault: &mut Option<Vault>) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let subcmd = parts.first().copied().unwrap_or("");
    let rest = parts.get(1).copied().unwrap_or("").trim();

    match subcmd {
        // Accept a pending LLM generation
        "_accept_generation" => {
            if let Some(gen) = app.pending_generation.take() {
                match gen.kind {
                    app::GenerationKind::Script => {
                        let dir = flume_core::scripting::scripts_generated_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        let path = dir.join(&gen.name);
                        match std::fs::write(&path, &gen.content) {
                            Ok(()) => {
                                app.system_message(&format!("Script saved to {}", path.display()));
                                match mgr.load_script(&path) {
                                    Ok(()) => app.system_message("Script loaded successfully"),
                                    Err(e) => app.system_message(&format!("Failed to load: {}", e)),
                                }
                            }
                            Err(e) => app.system_message(&format!("Failed to save: {}", e)),
                        }
                    }
                    app::GenerationKind::Theme => {
                        let dir = flume_core::config::themes_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        let name = gen.name.trim_end_matches(".toml");
                        let path = dir.join(&gen.name);
                        match std::fs::write(&path, &gen.content) {
                            Ok(()) => {
                                app.theme_switch = Some(name.to_string());
                                app.system_message(&format!("Theme saved and applied: {}", name));
                            }
                            Err(e) => app.system_message(&format!("Failed to save theme: {}", e)),
                        }
                    }
                    app::GenerationKind::Layout => {
                        let name = gen.name.trim_end_matches(".toml");
                        match toml::from_str::<crate::split::LayoutProfile>(&gen.content) {
                            Ok(profile) => {
                                match crate::split::save_layout(name, &profile) {
                                    Ok(()) => app.system_message(&format!("Layout '{}' saved. Use /layout load {}", name, name)),
                                    Err(e) => app.system_message(&format!("Failed to save layout: {}", e)),
                                }
                            }
                            Err(e) => app.system_message(&format!("Generated layout has invalid format: {}", e)),
                        }
                    }
                }
            }
            return;
        }
        // Internal: store LLM API key in vault during /generate init
        "_init_llm_key" => {
            let key = rest.trim();
            if key.is_empty() {
                app.system_message("No API key provided");
                return;
            }
            // Ensure vault exists
            if vault.is_none() {
                app.system_message("Creating vault...");
                let path = flume_core::config::vault_path();
                let v = flume_core::config::vault::Vault::new(path, "flume".to_string());
                let _ = v.save();
                *vault = Some(v);
                app.vault_unlocked = true;
            }
            // Refresh script engine vault secrets
            if let Some(ref v) = vault {
                let secrets: std::collections::HashMap<String, String> = v
                    .list()
                    .into_iter()
                    .filter_map(|name| v.get(name).map(|val| (name.to_string(), val.to_string())))
                    .collect();
                mgr.set_vault_secrets(secrets);
            }
            if let Some(ref mut v) = vault {
                v.set("flume_llm_key".to_string(), key.to_string());
                if let Err(e) = v.save() {
                    app.system_message(&format!("Failed to save vault: {}", e));
                    return;
                }
                app.system_message("API key stored in vault as 'flume_llm_key'");
                app.system_message("");
                app.system_message("Setup complete! Try it out:");
                app.system_message("  /generate script greet users who join my channel");
                app.system_message("  /generate theme dark mode with blue accents");
                app.system_message("");
                app.system_message("Note: restart Flume to load the new LLM config.");
            }
            return;
        }
        // Internal: show help for a script command
        "_help" => {
            let cmd_name = rest.trim();
            if let Some(help) = mgr.command_help(cmd_name) {
                app.system_message(&format!("/{} — {}", cmd_name, help));
            } else if mgr.has_command(cmd_name) {
                app.system_message(&format!("/{} — script command (no help text)", cmd_name));
            }
        }
        // Internal: try executing as a script-registered custom command
        "_exec" => {
            let exec_parts: Vec<&str> = rest.splitn(2, ' ').collect();
            let cmd_name = exec_parts.first().copied().unwrap_or("");
            let cmd_args = exec_parts.get(1).copied().unwrap_or("");
            if !mgr.execute_command(cmd_name, cmd_args) {
                app.system_message(&format!("Unknown command: /{}", cmd_name));
            }
            // Process any actions the command queued
            process_script_actions(mgr, app);
        }
        "load" => {
            if rest.is_empty() {
                app.system_message("Usage: /script load <path or name>");
                return;
            }
            let path = if rest.contains('/') || rest.contains('.') {
                std::path::PathBuf::from(rest)
            } else {
                // Search: lua/autoload, python/autoload, available, generated
                let candidates = [
                    flume_core::scripting::lua_autoload_dir().join(format!("{}.lua", rest)),
                    flume_core::scripting::python_autoload_dir().join(format!("{}.py", rest)),
                    flume_core::scripting::scripts_available_dir().join(format!("{}.lua", rest)),
                    flume_core::scripting::scripts_available_dir().join(format!("{}.py", rest)),
                    flume_core::scripting::scripts_generated_dir().join(format!("{}.lua", rest)),
                    flume_core::scripting::scripts_generated_dir().join(format!("{}.py", rest)),
                ];
                candidates.into_iter().find(|p| p.exists()).unwrap_or_else(|| {
                    // Default to lua autoload path (will error on load)
                    flume_core::scripting::lua_autoload_dir().join(format!("{}.lua", rest))
                })
            };
            match mgr.load_script(&path) {
                Ok(()) => app.system_message(&format!("Script '{}' loaded", rest)),
                Err(e) => app.system_message(&format!("Failed to load script: {}", e)),
            }
        }
        "unload" => {
            if rest.is_empty() {
                app.system_message("Usage: /script unload <name>");
                return;
            }
            if mgr.unload_script(rest) {
                app.system_message(&format!("Script '{}' unloaded", rest));
            } else {
                app.system_message(&format!("Script '{}' not found", rest));
            }
        }
        "reload" => {
            if rest.is_empty() {
                app.system_message("Usage: /script reload <name>");
                return;
            }
            match mgr.reload_script(rest) {
                Ok(true) => app.system_message(&format!("Script '{}' reloaded", rest)),
                Ok(false) => app.system_message(&format!("Script '{}' not found", rest)),
                Err(e) => app.system_message(&format!("Failed to reload script: {}", e)),
            }
        }
        "autoload" => {
            if rest.is_empty() {
                app.system_message("Usage: /script autoload <name or path>");
                return;
            }
            // Find the script — check loaded scripts first, then search dirs
            let script_path = mgr.list_scripts().iter()
                .find(|s| s.name == rest)
                .map(|s| s.path.clone())
                .or_else(|| {
                    let candidates = [
                        flume_core::scripting::scripts_available_dir().join(format!("{}.lua", rest)),
                        flume_core::scripting::scripts_available_dir().join(format!("{}.py", rest)),
                        flume_core::scripting::scripts_generated_dir().join(format!("{}.lua", rest)),
                        flume_core::scripting::scripts_generated_dir().join(format!("{}.py", rest)),
                    ];
                    candidates.into_iter().find(|p| p.exists())
                })
                .or_else(|| {
                    let p = std::path::PathBuf::from(rest);
                    if p.exists() { Some(p) } else { None }
                });

            let Some(source) = script_path else {
                app.system_message(&format!("Script '{}' not found", rest));
                return;
            };

            let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("lua");
            let autoload_dir = match ext {
                "py" => flume_core::scripting::python_autoload_dir(),
                _ => flume_core::scripting::lua_autoload_dir(),
            };
            let _ = std::fs::create_dir_all(&autoload_dir);
            let link_name = source.file_name().unwrap_or_default();
            let link_path = autoload_dir.join(link_name);

            if link_path.exists() {
                app.system_message(&format!("Already in autoload: {}", link_path.display()));
                return;
            }

            // Create symlink
            #[cfg(unix)]
            match std::os::unix::fs::symlink(&source, &link_path) {
                Ok(()) => app.system_message(&format!(
                    "Autoload enabled: {} -> {}", link_path.display(), source.display()
                )),
                Err(e) => app.system_message(&format!("Failed to create symlink: {}", e)),
            }
            #[cfg(not(unix))]
            match std::fs::copy(&source, &link_path) {
                Ok(_) => app.system_message(&format!("Autoload enabled: copied to {}", link_path.display())),
                Err(e) => app.system_message(&format!("Failed to copy: {}", e)),
            }
        }
        "noautoload" => {
            if rest.is_empty() {
                app.system_message("Usage: /script noautoload <name>");
                return;
            }
            // Check both lua and python autoload dirs
            let candidates = [
                flume_core::scripting::lua_autoload_dir().join(format!("{}.lua", rest)),
                flume_core::scripting::python_autoload_dir().join(format!("{}.py", rest)),
            ];
            let removed = candidates.iter().find(|p| p.exists());
            if let Some(path) = removed {
                match std::fs::remove_file(path) {
                    Ok(()) => app.system_message(&format!("Autoload disabled: removed {}", path.display())),
                    Err(e) => app.system_message(&format!("Failed to remove: {}", e)),
                }
            } else {
                app.system_message(&format!("'{}' not found in autoload directories", rest));
            }
        }
        "list" | "ls" | "" => {
            let scripts = mgr.list_scripts();
            if scripts.is_empty() {
                app.system_message("No scripts loaded");
            } else {
                app.system_message("Loaded scripts:");
                for info in scripts {
                    let auto = if info.is_autoload { " (autoload)" } else { "" };
                    app.system_message(&format!("  {}{}", info.name, auto));
                }
            }
            let cmds = mgr.custom_command_names();
            if !cmds.is_empty() {
                app.system_message("Script commands:");
                for name in &cmds {
                    app.system_message(&format!("  /{}", name));
                }
            }
        }
        _ => {
            app.system_message("Usage: /script load|unload|reload|list [name]");
        }
    }
}

/// Handle DCC commands that need the main loop context (async tasks, config access).
fn handle_dcc_tui_command(
    cmd: &str,
    app: &mut app::App,
    config: &flume_core::config::general::FlumeConfig,
    dcc_tx: &mpsc::Sender<DccEvent>,
) {
    if !config.dcc.enabled {
        app.system_message("DCC is disabled. Enable in config.toml: [dcc] enabled = true");
        return;
    }

    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    let subcmd = parts[0];
    let rest = parts.get(1).copied().unwrap_or("");

    match subcmd {
        "accept" => {
            let id: u64 = match rest.parse() {
                Ok(n) => n,
                Err(_) => return,
            };

            // Clone offer and update state before borrowing app for messages
            let offer = app.dcc_transfers.iter().find(|t| t.id == id).map(|t| t.offer.clone());
            let Some(offer) = offer else { return };

            if let Some(t) = app.dcc_transfers.iter_mut().find(|t| t.id == id) {
                t.state = DccTransferState::Connecting;
            }

            match offer.dcc_type {
                dcc::DccType::Send => {
                    let dir = dcc::transfer::expand_download_dir(&config.dcc.download_directory);
                    let _ = std::fs::create_dir_all(&dir);
                    let filename = offer.filename.clone().unwrap_or_else(|| "download".to_string());
                    let path = dir.join(&filename);

                    if let Some(t) = app.dcc_transfers.iter_mut().find(|t| t.id == id) {
                        t.path = Some(path.clone());
                    }

                    let tx = dcc_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = dcc::transfer::receive_file(id, &offer, &path, 0, tx.clone()).await {
                            let _ = tx.send(DccEvent::Failed { id, error: e }).await;
                        }
                    });
                    app.system_message(&format!("DCC #{}: downloading {} to {}", id, filename, dir.display()));
                }
                dcc::DccType::Chat => {
                    let tx = dcc_tx.clone();
                    let (chat_tx, chat_rx) = mpsc::channel::<String>(100);
                    app.dcc_chat_senders.insert(id, chat_tx);
                    let from = offer.from.clone();

                    tokio::spawn(async move {
                        match dcc::chat::connect_chat(offer.ip, offer.port).await {
                            Ok(stream) => {
                                dcc::chat::run_chat(id, stream, tx, chat_rx).await;
                            }
                            Err(e) => {
                                let _ = tx.send(DccEvent::Failed { id, error: e }).await;
                            }
                        }
                    });
                    app.system_message(&format!("DCC CHAT #{} connecting to {}", id, from));
                }
            }
        }
        "send" => {
            let send_parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if send_parts.len() < 2 {
                return;
            }
            let _nick = send_parts[0];
            let _file = send_parts[1];
            // TODO: implement outgoing DCC SEND
            // 1. Bind listener on port range
            // 2. Send CTCP DCC SEND to nick with our IP/port
            // 3. Wait for connection
            // 4. Stream file
            app.system_message("DCC SEND outgoing: not yet fully implemented (file listening)");
        }
        "chat" => {
            let _nick = rest;
            // TODO: implement outgoing DCC CHAT
            app.system_message("DCC CHAT outgoing: not yet fully implemented");
        }
        _ => {}
    }
}

/// Generate a filename from a description.
fn slugify_name(description: &str, ext: &str) -> String {
    let slug: String = description
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    let truncated = if slug.len() > 30 { &slug[..30] } else { &slug };
    let trimmed = truncated.trim_end_matches('_');
    format!("{}.{}", trimmed, ext)
}

/// Convert an IrcEvent to a ScriptEvent for the scripting engine.
fn irc_event_to_script_event(event: &IrcEvent) -> Option<ScriptEvent> {
    match event {
        IrcEvent::Connected { server_name, our_nick, .. } => {
            Some(ScriptEvent::new("connect", server_name).field("nick", our_nick))
        }
        IrcEvent::Disconnected { server_name, reason } => {
            let reason_str = format!("{:?}", reason);
            Some(ScriptEvent::new("disconnect", server_name).field("reason", &reason_str))
        }
        IrcEvent::MessageReceived { server_name, message } => {
            use flume_core::irc::command::Command;
            let nick = message.prefix_nick().unwrap_or("");
            match &message.command {
                Command::Privmsg { target, text } => {
                    if target.starts_with('#') {
                        // Check for CTCP
                        if text.starts_with('\x01') && text.ends_with('\x01') {
                            let inner = &text[1..text.len()-1];
                            Some(ScriptEvent::new("ctcp_request", server_name)
                                .field("nick", nick)
                                .field("command", inner.split(' ').next().unwrap_or(""))
                                .field("params", inner.split(' ').skip(1).collect::<Vec<_>>().join(" ").as_str()))
                        } else {
                            Some(ScriptEvent::new("message", server_name)
                                .field("nick", nick)
                                .field("channel", target)
                                .field("text", text))
                        }
                    } else {
                        Some(ScriptEvent::new("private_message", server_name)
                            .field("nick", nick)
                            .field("text", text))
                    }
                }
                Command::Join { channels } => {
                    for (channel, _) in channels {
                        return Some(ScriptEvent::new("join", server_name)
                            .field("nick", nick)
                            .field("channel", channel));
                    }
                    None
                }
                Command::Part { channels, message: part_msg } => {
                    for channel in channels {
                        return Some(ScriptEvent::new("part", server_name)
                            .field("nick", nick)
                            .field("channel", channel)
                            .field("message", part_msg.as_deref().unwrap_or("")));
                    }
                    None
                }
                Command::Quit { message: quit_msg } => {
                    Some(ScriptEvent::new("quit", server_name)
                        .field("nick", nick)
                        .field("message", quit_msg.as_deref().unwrap_or("")))
                }
                Command::Kick { channel, user, reason } => {
                    Some(ScriptEvent::new("kick", server_name)
                        .field("nick", nick)
                        .field("channel", channel)
                        .field("target", user)
                        .field("reason", reason.as_deref().unwrap_or("")))
                }
                Command::Nick { nickname } => {
                    Some(ScriptEvent::new("nick_change", server_name)
                        .field("old_nick", nick)
                        .field("new_nick", nickname))
                }
                Command::Topic { channel, topic } => {
                    Some(ScriptEvent::new("topic_change", server_name)
                        .field("nick", nick)
                        .field("channel", channel)
                        .field("topic", topic.as_deref().unwrap_or("")))
                }
                Command::Mode { target, modes, params } => {
                    Some(ScriptEvent::new("mode_change", server_name)
                        .field("target", target)
                        .field("modes", modes.as_deref().unwrap_or(""))
                        .field("params", &params.join(" ")))
                }
                Command::Notice { target, text } => {
                    Some(ScriptEvent::new("notice", server_name)
                        .field("nick", nick)
                        .field("target", target)
                        .field("text", text))
                }
                _ => {
                    // Raw event for all messages
                    Some(ScriptEvent::new("raw", server_name)
                        .field("line", &format!("{:?}", message.command)))
                }
            }
        }
        _ => None,
    }
}

/// Process pending ScriptActions from the scripting engine.
fn process_script_actions(mgr: &ScriptManager, app: &mut app::App) {
    let actions = mgr.drain_actions();
    for action in actions {
        match action {
            ScriptAction::PrintToBuffer { server, buffer, text } => {
                let target_server = if server.is_empty() {
                    app.active_server.clone()
                } else {
                    Some(server)
                };
                if let Some(srv) = target_server {
                    let target_buffer = if buffer.is_empty() {
                        app.servers.get(&srv).map(|ss| ss.active_buffer.clone()).unwrap_or_default()
                    } else {
                        buffer
                    };
                    let msg = app::DisplayMessage {
                        timestamp: chrono::Utc::now(),
                        source: app::MessageSource::System,
                        text,
                        highlight: false,
                    };
                    let scrollback = app.scrollback_limit;
                    if let Some(ss) = app.servers.get_mut(&srv) {
                        ss.add_message(&target_buffer, msg, scrollback);
                    }
                }
            }
            ScriptAction::SendMessage { server, target, text } => {
                let srv = if server.is_empty() {
                    app.active_server.clone().unwrap_or_default()
                } else {
                    server
                };
                if let Some(ss) = app.servers.get(&srv) {
                    if let Some(tx) = &ss.command_tx {
                        let _ = tx.try_send(UserCommand::SendMessage { target, text });
                    }
                }
            }
            ScriptAction::SendRaw { server, line } => {
                let srv = if server.is_empty() {
                    app.active_server.clone().unwrap_or_default()
                } else {
                    server
                };
                if let Some(ss) = app.servers.get(&srv) {
                    if let Some(tx) = &ss.command_tx {
                        let _ = tx.try_send(UserCommand::RawLine(line));
                    }
                }
            }
            ScriptAction::JoinChannel { server, channel, key } => {
                let srv = if server.is_empty() {
                    app.active_server.clone().unwrap_or_default()
                } else {
                    server
                };
                if let Some(ss) = app.servers.get(&srv) {
                    if let Some(tx) = &ss.command_tx {
                        let _ = tx.try_send(UserCommand::Join { channel, key });
                    }
                }
            }
            ScriptAction::PartChannel { server, channel, message } => {
                let srv = if server.is_empty() {
                    app.active_server.clone().unwrap_or_default()
                } else {
                    server
                };
                if let Some(ss) = app.servers.get(&srv) {
                    if let Some(tx) = &ss.command_tx {
                        let _ = tx.try_send(UserCommand::Part { channel, message });
                    }
                }
            }
            ScriptAction::Notify { message, .. } => {
                send_desktop_notification("Flume Script", &message);
            }
            ScriptAction::SetStatusItem { name, text } => {
                // Status items could be stored in app for rendering
                tracing::debug!("Script status item: {} = {}", name, text);
            }
            ScriptAction::SwitchBuffer { buffer } => {
                if let Some(ss) = app.active_server_state_mut() {
                    if ss.buffers.contains_key(&buffer) {
                        ss.switch_buffer(&buffer);
                    }
                }
            }
        }
    }
}
