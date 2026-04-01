# Phase 3: Full TUI and Theming

## Sub-phase A: Foundation (COMPLETE)

- [x] **A1: Theme engine — core config types** (`flume-core/src/config/theme.rs`)
  - ThemeConfig, ThemeMeta, ThemeColors, NickColorConfig, ElementColors structs
  - parse_color_rgb() and named_color() parsing functions
  - 6 tests passing
- [x] **A1: Theme engine — TUI Theme struct** (`flume-tui/src/theme.rs`)
  - Pre-resolved ratatui Color values from ThemeConfig
  - Theme::load(), default_theme(), nick_color() (hash-based), switch_to()
  - check_reload() for hot-reload via mtime polling
  - list_available() for theme discovery
- [x] **A1: Theme engine — replace hardcoded colors** (all `ui/*.rs` files)
  - All render functions accept `&Theme` parameter
  - Every Color::* replaced with theme field reference
  - Default theme matches original hardcoded colors exactly
- [x] **A1: Theme hot-reload and /theme command**
  - Mtime polling in main loop tick branch
  - `/theme` lists available themes
  - `/theme <name>` switches theme via app.theme_switch signal
  - `/theme reload` triggers hot-reload
- [x] **A2: CTCP handling** (`flume-core/src/connection/mod.rs`)
  - CtcpConfig threaded into ServerConnection::new()
  - Auto-respond to VERSION, PING, TIME, CLIENTINFO
  - Rate limiting per source nick (HashMap<String, Instant>)
  - Still broadcasts CTCP events to TUI for display
- [x] **A3: Logging to disk** (`flume-core/src/logging.rs`)
  - Logger struct with BufWriter file handles
  - Plain text and JSON log formats
  - Daily rotation (date change detection)
  - Logs PRIVMSG, ACTION, JOIN, PART, QUIT, KICK, TOPIC, NICK events
  - Files at ~/.local/share/flume/logs/<server>/<target>/YYYY-MM-DD.log

## Sub-phase B: Notifications + URL Detection (COMPLETE)

- [x] **B1: Notification system** (`flume-tui/src/app.rs`, `flume-tui/src/main.rs`)
  - `DisplayMessage.highlight: bool` field — detected at message insertion time
  - `Buffer.highlight_count: u32` — incremented for non-active buffers
  - `NotificationEvent` enum (Highlight, PrivateMessage) returned from `handle_irc_event()`
  - `is_highlight()` free function: checks own nick + configurable highlight_words
  - PMs from other users always treated as highlights
  - Terminal bell (`\x07`) on any highlight when `highlight_bell` enabled
  - Desktop notifications via `osascript` (macOS) / `notify-send` (Linux) for highlights and PMs
  - 6 new URL tests passing (81 total)

- [x] **B2: URL detection and opening** (`flume-tui/src/url.rs`, `flume-tui/src/ui/chat_buffer.rs`)
  - `url.rs` module with `OnceLock<Regex>` for URL pattern matching
  - `find_urls()` returns byte-offset spans, `extract_urls()` returns URL strings
  - `styled_text_spans()` in chat_buffer.rs splits text into normal/URL segments
  - URLs rendered in `theme.chat_url` color with underline modifier
  - Highlighted messages rendered in `theme.chat_highlight` color
  - Search highlighting still takes precedence (bg/fg swap)

- [x] **B3: /url command** (`flume-tui/src/input.rs`)
  - `/url` — lists last 10 URLs in active buffer with nick attribution
  - `/url <N>` — opens Nth most recent URL via `url_open_command`
  - `/url open <N>` — same as above

- [x] **B4: Title bar + status bar highlight indicators** (`flume-tui/src/ui/title_bar.rs`, `flume-tui/src/ui/status_bar.rs`)
  - Buffers with `highlight_count > 0` shown in `theme.chat_highlight` color with `(N!)` format
  - Other servers with highlights shown similarly in status bar
  - `ServerState::total_highlights()` aggregates across non-active buffers

## Sub-phase C: Keybinding Modes (COMPLETE)

- [x] **C1: Config types** (`flume-core/src/config/keybindings.rs`)
  - KeybindingMode enum (Emacs, Vi, Custom) with serde lowercase rename
  - KeybindingsConfig struct with `mode` field, default Emacs
  - Added `keybindings` field to UiConfig
  - 4 config tests passing

- [x] **C2: Keybinding engine** (`flume-tui/src/keybindings.rs`)
  - InputAction enum — 30+ bindable actions (editing, navigation, vi-specific)
  - KeyCombo hashable wrapper with helper constructors (ctrl, alt, plain)
  - Global keymap (Ctrl+C, Ctrl+X, Alt+1-9, PageUp/Down, Tab, Enter)
  - Emacs keymap — readline bindings (Ctrl+A/E/B/F/D/K/U/W/T/P/N, Alt+B/F)
  - Vi insert keymap — Esc to normal, basic editing, Ctrl+W/U/K
  - Vi normal keymap — h/l/w/b/e/0/$, i/a/A/I, x/X/C, j/k
  - `resolve()` function: global → vi-normal → mode-specific priority
  - 15 keymap tests passing

- [x] **C3: App state** (`flume-tui/src/app.rs`)
  - ViMode enum (Normal/Insert)
  - `keybinding_mode`, `vi_mode`, `vi_pending_op` fields on App
  - Vi mode defaults to Insert on startup
  - Threaded KeybindingMode from config through App::new()

- [x] **C4: Action-based dispatch** (`flume-tui/src/input.rs`)
  - Refactored handle_input() to use keymap resolution instead of hardcoded matches
  - `execute_action()` dispatches all InputAction variants
  - Word movement helpers (word_boundary_left/right)
  - Vi operator-pending state for dd/cc
  - Vi mode switching (Esc → normal, cursor back; i/a/A/I → insert)
  - Emacs: Ctrl+K kill-to-end, Ctrl+U kill-to-start, Ctrl+W delete-word, Ctrl+T transpose
  - Char insertion only in emacs/vi-insert mode (blocked in vi-normal)
  - 6 word boundary tests passing

- [x] **C5: Vi mode indicator** (`flume-tui/src/ui/input_line.rs`)
  - `[N]` prefix with status bar colors + bold in vi Normal mode
  - `[I]` prefix with input colors + bold in vi Insert mode
  - No indicator shown in Emacs mode

- [x] **C6: Commands** (`flume-tui/src/input.rs`)
  - `/keys` (or `/keybindings`) — shows active mode and all bindings
  - Updated `/help` to list `/keys` command

## Sub-phase D: Split Windows + Layout Profiles (COMPLETE)

- [x] **D1: Split types** (`flume-tui/src/split.rs`)
  - SplitDirection enum (Vertical/Horizontal) with serde
  - SplitState struct (direction, secondary server/buffer, ratio)
  - LayoutProfile struct for save/load (TOML files)
  - save_layout, load_layout, list_layouts, delete_layout file I/O
  - 5 split tests passing

- [x] **D2: App split state** (`flume-tui/src/app.rs`)
  - `split: Option<SplitState>` field on App
  - `split_buffer()`, `unsplit()`, `swap_split_focus()` methods
  - `split_messages()`, `split_scroll_offset()`, `split_search()` accessors
  - Focus swap exchanges active/secondary buffers (supports cross-server)

- [x] **D3: Chat buffer refactor** (`flume-tui/src/ui/chat_buffer.rs`)
  - Extracted `render_buffer()` that takes explicit messages/scroll/search
  - `render()` is now a thin wrapper calling `render_buffer` with active buffer data
  - Enables rendering any buffer into any Rect (used by split panes)

- [x] **D4: Split layout rendering** (`flume-tui/src/ui/mod.rs`)
  - Splits main area into primary + separator + secondary panes
  - Vertical split uses horizontal layout, horizontal split uses vertical layout
  - 1-char separator line (│ or ─) using box-drawing chars
  - Nick list hidden when split is active

- [x] **D5: Commands** (`flume-tui/src/input.rs`)
  - `/split v|h <buffer>` — create split (same server or cross-server via server/buffer)
  - `/unsplit` — close split
  - `/focus` — swap focus between panes
  - `/layout save|load|list|delete <name>` — layout profile management
  - Alt+Tab keybinding for SwapSplitFocus (global)

- [x] **D6: Title bar indicator** (`flume-tui/src/ui/title_bar.rs`)
  - Shows `[│ #channel]` or `[─ #channel]` for active split
  - Updated `/help` and `/keys` with split commands

## Phase 4: Scripting Engine — Sub-phase A (COMPLETE)

- [x] **A1: Lua runtime** (`flume-core/src/scripting/lua_runtime.rs`)
  - LuaRuntime wrapping mlua Lua 5.4 (vendored build)
  - Arc<Mutex<SharedState>> for thread-safe shared state
  - Event handler registry (event_name → Vec<(script, handler)>)
  - Custom command registry (name → handler)
  - Action queue pattern — scripts queue ScriptActions, TUI processes them
  - 6 runtime tests passing

- [x] **A2: Full Script API** (`flume-core/src/scripting/api.rs`)
  - 7 namespaces: flume.event, flume.server, flume.channel, flume.buffer, flume.command, flume.config, flume.ui
  - flume.event.on/off/emit — subscribe/unsubscribe/emit events
  - flume.server.send_raw/connect/disconnect/list
  - flume.channel.join/part/say/action/topic/names
  - flume.buffer.print/current/switch/scroll/search
  - flume.command.register/unregister — custom slash commands
  - flume.config.get/set — per-script TOML config
  - flume.ui.notify/status_item/input_text/set_input_text

- [x] **A3: ScriptManager** (`flume-core/src/scripting/mod.rs`)
  - ScriptAction enum (8 variants: PrintToBuffer, SendMessage, SendRaw, JoinChannel, etc.)
  - ScriptEvent with builder pattern, cancellation support
  - ScriptInfo metadata (name, path, is_autoload)
  - load_script/unload_script/reload_script/list_scripts/load_autoload
  - Script directory helpers (scripts_dir, autoload_dir, available_dir, data_dir)
  - 4 manager tests passing

- [x] **A4: Sandbox** (`flume-core/src/scripting/sandbox.rs`)
  - Strips os.execute, os.remove, os.rename, os.getenv, os.exit
  - Strips io.popen, io.open, io.input, io.output
  - Strips dofile, loadfile, debug library
  - Preserves os.time, os.date, os.clock, string, table, math
  - 8 sandbox tests passing

- [x] **A5: TUI integration** (`flume-tui/src/main.rs`, `flume-tui/src/input.rs`)
  - ScriptManager created on startup, autoload scripts loaded
  - IRC events dispatched to scripts before TUI processing (cancellable)
  - ScriptActions processed each tick (PrintToBuffer, SendMessage, Notify, etc.)
  - irc_event_to_script_event() maps all 18 event types
  - /script load|unload|reload|list commands
  - Unknown commands routed to script custom commands
  - Updated /help with script commands

- [x] **A6: Example scripts** (`examples/scripts/`)
  - highlight.lua — configurable keyword notifications via flume.config
  - away_logger.lua — logs missed PMs/highlights with /awayon, /awayoff commands
  - url_title.lua — announces URLs posted in channels

## Phase 4B: Python Runtime (COMPLETE)

- [x] **B1: PyO3 dependency** (`flume-core/Cargo.toml`)
  - Optional `python` feature flag: `pyo3 = { version = "0.23", features = ["auto-initialize"], optional = true }`
  - Default build has no Python dependency

- [x] **B2: Python runtime** (`flume-core/src/scripting/py_runtime.rs`)
  - `PyRuntime` struct wrapping PyO3 with `FlumeBridge` pyclass
  - Bootstrap Python code creates `flume` module facade with 7 sub-namespaces
  - All methods route through `_flume_bridge` builtins global
  - Same interface as LuaRuntime (exec_script, dispatch_event, drain_actions, etc.)
  - Full import access — no sandbox (scripts are trusted user extensions)
  - 7 Python runtime tests passing

- [x] **B3: Dual-dispatch ScriptManager** (`flume-core/src/scripting/mod.rs`)
  - Routes .lua → Lua, .py → Python by file extension
  - dispatch_event() runs through Lua first, then Python (respects cancellation)
  - drain_actions() merges from both runtimes
  - execute_command() checks Lua first, falls back to Python
  - load_autoload() picks up both .lua and .py files
  - Warns on load from `generated/` directory
  - All feature-gated with `#[cfg(feature = "python")]`

- [x] **B4: Example Python scripts** (`examples/scripts/`)
  - greet.py — responds to !greet, registers /pyinfo command
  - totp.py — TOTP code generator using pyotp (demonstrates full import access)

## Phase 5: LLM-Powered Generation (COMPLETE)

- [x] **5A: LLM config** (`flume-core/src/config/llm.rs`)
  - LlmProvider enum (Anthropic, OpenAi), LlmConfig struct
  - API key stored in vault via `/secure set flume_llm_key <key>`
  - Configurable model, temperature, max_tokens
  - Added `[llm]` section to FlumeConfig

- [x] **5B: LLM client** (`flume-core/src/llm/mod.rs`)
  - LlmClient with Anthropic and OpenAI HTTP implementations via reqwest
  - Anthropic: POST /v1/messages with x-api-key + anthropic-version headers
  - OpenAI: POST /v1/chat/completions with Bearer auth
  - `extract_code()` strips markdown code fences from LLM responses
  - 3 extract_code tests + 3 config tests

- [x] **5C: System prompts** (`flume-core/src/llm/prompts.rs`)
  - `script_system_prompt(language)` — full Flume Script API reference, event list, examples
  - `theme_system_prompt()` — ThemeConfig TOML schema with all color fields
  - `layout_system_prompt()` — LayoutProfile TOML schema

- [x] **5D: /generate command** (`flume-tui/src/input.rs`, `flume-tui/src/main.rs`)
  - `/generate script [--lua|--python] <description>` — generate script from description
  - `/generate theme <description>` — generate theme TOML
  - `/generate layout <description>` — generate layout TOML
  - Async LLM task spawned via tokio, results received via mpsc channel
  - "generating..." indicator in status bar while in flight

- [x] **5E: Split pane preview** (`flume-tui/src/ui/mod.rs`)
  - Generated content shown in right split pane with line numbers
  - Header shows generation kind and filename
  - Footer shows accept/reject instructions

- [x] **5F: Accept/reject flow** (`flume-tui/src/main.rs`)
  - `/generate accept` — saves script to scripts/generated/, auto-loads via ScriptManager
  - `/generate accept` — saves theme to themes dir, auto-applies via theme_switch
  - `/generate accept` — saves layout to layouts dir
  - `/generate reject` — discards pending generation, closes preview
  - Updated /help with /generate commands

## Phase 6: DCC (COMPLETE)

- [x] **6A: DCC config** (`flume-core/src/config/dcc.rs`)
  - DccConfig struct: enabled, auto_accept, download_directory, port_range, passive, max_transfer_size
  - Disabled by default for security
  - 3 config tests passing

- [x] **6B: Core DCC types** (`flume-core/src/dcc/mod.rs`)
  - DccType (Send/Chat), DccOffer, DccTransfer, DccTransferState, DccEvent
  - DccCtcpMessage parser: SEND, CHAT, RESUME, ACCEPT with IP/port/token parsing
  - Integer and dotted IP address parsing, DCC IP encoding
  - format_size() for human-readable file sizes
  - 11 DCC parsing tests passing

- [x] **6C: File transfer** (`flume-core/src/dcc/transfer.rs`)
  - `receive_file()` — async connect, download, write to disk, progress reporting via mpsc
  - `send_file()` — async listen, stream file, progress reporting
  - Resume support (append mode with seek)
  - `bind_listener()` for port range allocation
  - `expand_download_dir()` for ~/path expansion

- [x] **6D: DCC CHAT** (`flume-core/src/dcc/chat.rs`)
  - `run_chat()` — async bidirectional text over TCP (no IRC protocol)
  - `connect_chat()` — connect to peer for outgoing chat
  - `accept_chat()` — accept incoming chat connection
  - Chat messages displayed as system messages in active buffer

- [x] **6E: XDCC** (`flume-core/src/dcc/xdcc.rs`)
  - `request_pack(n)` — sends `xdcc send #N` PRIVMSG
  - `request_list()` / `request_cancel()` — XDCC bot commands
  - Bot responses arrive as normal DCC SEND offers
  - 3 XDCC tests passing

- [x] **6F: TUI integration** (`flume-tui/src/input.rs`, `flume-tui/src/main.rs`)
  - `/dcc list|accept|reject|send|chat|close` commands
  - `/xdcc <bot> <pack#|list|cancel>` commands
  - DCC transfers tracked in App state
  - Incoming DCC offers parsed from CTCP in main loop
  - DCC events (progress, complete, failed, chat) processed via mpsc channel
  - Transfer progress shown in status bar
  - Updated /help with DCC and XDCC commands

## Review

- 154 tests (122 core + 32 TUI)
- 155 tests with python feature (123 core + 32 TUI)
- No new clippy warnings
- 3 example themes, 5 example scripts (3 Lua + 2 Python)
- LLM providers: Anthropic (Claude), OpenAI (GPT)
- API key via vault: `/secure set flume_llm_key <key>`
