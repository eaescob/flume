# Flume — Modern IRC Client Architecture Design

**Version:** 0.1.0-draft
**Date:** March 30, 2026
**Author:** Emilio Escobar

---

## 1. Vision

Flume is a modern, fast, terminal-based IRC client built in Rust. It supports multi-server/multi-network connections, a rich customizable TUI with theming and layout control, a dual scripting engine (Python and Lua), and an LLM-powered script generation system where users provide their own API keys. Flume targets macOS, Linux, and FreeBSD, with a future iPadOS version planned.

Flume draws inspiration from the best ideas of mIRC, BitchX, epic, irssi, and weechat — while rethinking the experience for 2026 and beyond.

---

## 2. Design Principles

- **Speed over everything.** Sub-millisecond rendering. Async I/O across all connections. No blocking the UI thread, ever.
- **User owns the layout.** Every panel, status bar, title bar, and window split is configurable. Themes are first-class.
- **Scripts are citizens, not hacks.** Python and Lua scripts share the same event system and API surface. Scripts can be generated from natural language via LLM.
- **Protocol correctness.** Full IRCv3 with graceful degradation to RFC 1459/2812. Bouncer-aware. DCC and CTCP support.
- **Small footprint.** Target binary size under 5MB (excluding Python runtime). Minimal dependencies. Fast startup.

---

## 3. Technology Stack

| Component | Choice | Rationale |
|---|---|---|
| Language | Rust (2021 edition) | Speed, safety, small binaries, excellent async story |
| Async runtime | Tokio | Industry standard, lightweight tasks, work-stealing scheduler |
| TUI framework | Ratatui 0.30+ | Modular architecture, constraint-based layouts, custom widgets |
| Terminal backend | Crossterm | Pure Rust, macOS/Linux/FreeBSD support, mouse + color |
| Lua scripting | mlua | Supports Lua 5.4 + LuaJIT, async-capable, clean FFI |
| Python scripting | PyO3 | Dynamic linking against system Python, bidirectional type conversion |
| Configuration | serde + toml | Human-readable config, derive-based (de)serialization |
| TLS | rustls + webpki | Pure Rust TLS, no OpenSSL dependency, SNI support |
| IRC parsing | Custom parser | Full IRCv3 message-tags, CTCP, DCC; existing crates lack complete coverage |
| Logging | tracing | Structured, async-aware, filterable |

### Why a Custom IRC Parser

The existing `irc` crate provides RFC 2812 and partial IRCv3 support, but lacks complete message-tag parsing, DCC handling, and modern IRCv3 extensions (chathistory, read-marker, labeled-responses). Building a purpose-built parser ensures full protocol coverage, tighter integration with Flume's event system, and no dead code from unused features.

---

## 4. High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Flume Binary                         │
├─────────────┬──────────────┬──────────────┬─────────────────┤
│   TUI Layer │  Script Eng. │  LLM Engine  │  Config/State   │
│  (ratatui)  │ (mlua/pyo3)  │ (HTTP client)│  (serde/toml)   │
├─────────────┴──────┬───────┴──────────────┴─────────────────┤
│                    │  Event Bus (tokio mpsc channels)        │
├────────────────────┴────────────────────────────────────────┤
│              Core Engine (async, multi-connection)           │
├──────────┬──────────┬───────────┬───────────┬───────────────┤
│ IRC Proto│ IRCv3 CAP│ DCC Engine│ CTCP      │ TLS (rustls)  │
│ Parser   │ Negotiator│          │ Handler   │               │
├──────────┴──────────┴───────────┴───────────┴───────────────┤
│                   Tokio Async Runtime                       │
└─────────────────────────────────────────────────────────────┘
```

### Module Breakdown

**Core Engine** — Manages connection lifecycle, server state, channel state, user tracking, and message routing. One `ServerConnection` task per network, each running independently via `tokio::spawn`.

**IRC Protocol Parser** — Zero-copy parser for IRC messages including IRCv3 message tags. Handles encoding/decoding, message splitting, and flood protection.

**IRCv3 Capability Negotiator** — Manages CAP LS/REQ/ACK/END handshake. Maintains per-server capability state. Enables graceful degradation when capabilities are unavailable.

**DCC Engine** — Handles DCC SEND, DCC CHAT, passive DCC, and reverse DCC. Manages direct TCP connections for file transfers. Validates IP/port ranges for security.

**CTCP Handler** — Parses and responds to CTCP queries (VERSION, PING, TIME, ACTION, CLIENTINFO). Rate-limits automatic responses to prevent floods.

**TUI Layer** — Renders the interface via ratatui. Owns layout state, theme state, and input handling. Receives events from the core engine via channels.

**Script Engine** — Embeds Lua (via mlua) and Python (via PyO3) interpreters. Exposes Flume's event API to scripts. Manages script lifecycle (load, unload, reload).

**LLM Engine** — HTTP client for calling LLM provider APIs. Takes natural language prompts, generates script code, presents it for user review before installation.

**Config/State** — Reads/writes TOML configuration. Manages persistent state (server list, channel keys, script registry, window layouts, themes).

---

## 5. IRC Protocol Support

### 5.1 IRCv3 Capabilities (Priority Order)

**Must-Have (v1.0):**

- `cap-notify` — Dynamic capability updates
- `sasl` — SASL authentication (PLAIN, SCRAM-SHA-256, EXTERNAL)
- `message-tags` — Per-message metadata
- `server-time` — Accurate message timestamps
- `echo-message` — Message acknowledgment from server
- `away-notify` — Real-time away status updates
- `extended-join` — Account and realname on JOIN
- `account-tag` — Account name in message tags
- `multi-prefix` — All user prefixes in NAMES/WHO replies
- `batch` — Grouped message handling (netsplits, history)
- `labeled-response` — Correlate requests with responses
- `monitor` — Efficient user online/offline tracking
- `sts` — Strict Transport Security (upgrade to TLS)

**Should-Have (v1.1):**

- `chathistory` — Request historical messages from server
- `read-marker` — Sync read state across clients
- `extended-monitor` — Extended user tracking
- `setname` — Change realname without reconnecting

**Bouncer-Specific:**

- `znc.in/playback` — ZNC buffer playback with time ranges
- `znc.in/self-message` — See own messages from other clients
- `soju.im/bouncer-networks` — Soju multi-network management

### 5.2 Legacy Support

Flume connects to any RFC 1459 or RFC 2812 server. When CAP negotiation fails or returns no capabilities, Flume falls back to traditional NICK/USER registration and NickServ-based authentication. Numeric reply parsing is defensive — no assumptions about parameter counts or ordering.

### 5.3 CTCP

Automatic responses for: VERSION, PING, TIME, CLIENTINFO, SOURCE. The ACTION command (`/me`) is always supported. All automatic CTCP responses are rate-limited (default: 3 per 10 seconds per source). Users can disable individual CTCP responses via config.

### 5.4 DCC

DCC SEND and DCC CHAT are supported with configurable accept/reject policies. Passive DCC and reverse DCC are supported for NAT traversal. IP and port validation prevents abuse. File transfers show progress in a dedicated TUI panel. DCC is disabled by default and must be explicitly enabled per-server in config.

### 5.5 TLS and Security

All connections use TLS by default via rustls. Certificate validation uses webpki roots. SNI is sent on every TLS handshake. STS policies are cached locally and enforced on reconnect. PLAIN SASL is rejected unless the connection is TLS-encrypted. SCRAM-SHA-256 is preferred when available.

---

## 6. TUI Architecture

### 6.1 Layout System

Flume's UI is built on ratatui's constraint-based layout system with a user-configurable tree of splits and panels.

```
┌──────────────────────────────────────────────────────────┐
│ [Title Bar: network/channel/topic]                       │
├────────┬─────────────────────────────────────┬───────────┤
│        │                                     │           │
│ Server │          Chat Buffer                │  Nick     │
│  Tree  │                                     │  List     │
│        │                                     │           │
│        │                                     │           │
│        │                                     │           │
│        │                                     │           │
├────────┴─────────────────────────────────────┴───────────┤
│ [Input Line]                                             │
├──────────────────────────────────────────────────────────┤
│ [Status Bar: nick | lag | away | modes | clock]          │
└──────────────────────────────────────────────────────────┘
```

**Configurable Elements:**

- **Title bar** — Shows current network, channel name, topic. Position and content are configurable.
- **Server tree** — Hierarchical view of networks → servers → channels → queries. Can be toggled, moved to left/right, resized.
- **Chat buffer** — The main message area. Supports scrollback, search, URL detection, nick highlighting. Can be split horizontally or vertically for multiple buffers side-by-side.
- **Nick list** — Channel member list with mode indicators. Toggleable, resizable.
- **Input line** — Multi-line input via `tui-textarea`. History, tab completion, vi/emacs keybindings.
- **Status bar** — Customizable segments showing nick, lag, away status, channel modes, clock, script indicators.

### 6.2 Window Management

Flume supports multiple window management models, configurable per-user:

- **Tabbed** — One buffer per tab, switch with Alt+number or Ctrl+n/p (similar to irssi)
- **Split** — Horizontal and vertical splits showing multiple buffers simultaneously (similar to weechat)
- **Hybrid** — Tabs at the top level, splits within each tab

Window layouts are defined in the config file and can be saved/loaded as named profiles.

### 6.3 Theming

Themes are TOML files that define colors for every UI element:

```toml
# ~/.config/flume/themes/solarized-dark.toml
[meta]
name = "Solarized Dark"
author = "Emilio"

[colors]
background = "#002b36"
foreground = "#839496"
highlight = "#268bd2"
error = "#dc322f"
warning = "#b58900"
success = "#859900"

[nick_colors]
palette = ["#dc322f", "#cb4b16", "#b58900", "#859900", "#2aa198", "#268bd2", "#6c71c4", "#d33682"]

[elements]
title_bar_bg = "#073642"
title_bar_fg = "#93a1a1"
status_bar_bg = "#073642"
status_bar_fg = "#586e75"
input_bg = "#002b36"
input_fg = "#839496"
server_tree_bg = "#002b36"
server_tree_fg = "#586e75"
server_tree_active = "#268bd2"
nick_list_bg = "#002b36"
nick_list_fg = "#586e75"
nick_list_op = "#859900"
nick_list_voice = "#2aa198"
chat_timestamp = "#586e75"
chat_nick = "#93a1a1"
chat_message = "#839496"
chat_action = "#cb4b16"
chat_notice = "#6c71c4"
chat_highlight = "#d33682"
chat_url = "#268bd2"
```

Nick colors are assigned deterministically via a hash of the nickname, using the configured palette. Users can override specific nicks.

---

## 7. Scripting Engine

### 7.1 Dual Runtime Architecture

Flume embeds both a Lua interpreter (via mlua, Lua 5.4 or LuaJIT) and a Python interpreter (via PyO3, dynamically linked). Both runtimes share the same event-driven API surface — a script written in Lua should be trivially portable to Python and vice versa.

```
┌─────────────────────────────────────┐
│         Script Manager              │
│  (load, unload, reload, lifecycle)  │
├────────────────┬────────────────────┤
│   Lua Runtime  │  Python Runtime    │
│   (mlua)       │  (PyO3)            │
├────────────────┴────────────────────┤
│        Flume Script API             │
│  (events, commands, buffers, UI)    │
└─────────────────────────────────────┘
```

### 7.2 Script API Surface

The Flume Script API exposes the following namespaces:

**`flume.event`** — Subscribe to and emit events.
- `on(event_name, callback)` — Register a handler
- `off(event_name, callback)` — Unregister a handler
- `emit(event_name, data)` — Emit a custom event

**`flume.server`** — Server connection operations.
- `connect(name)` — Connect to a named server from config
- `disconnect(name)` — Disconnect from a server
- `send_raw(server, raw_line)` — Send a raw IRC line
- `list()` — List active connections

**`flume.channel`** — Channel operations.
- `join(server, channel, key?)` — Join a channel
- `part(server, channel, message?)` — Leave a channel
- `say(server, target, message)` — Send a message
- `action(server, target, message)` — Send an action (/me)
- `topic(server, channel)` — Get channel topic
- `names(server, channel)` — Get nick list

**`flume.buffer`** — Buffer/window operations.
- `current()` — Get the active buffer
- `switch(buffer_id)` — Switch to a buffer
- `scroll(direction, amount)` — Scroll the buffer
- `search(pattern)` — Search buffer contents
- `print(buffer_id, text, style?)` — Print styled text to a buffer

**`flume.command`** — Register custom commands.
- `register(name, callback, help_text)` — Register `/command`
- `unregister(name)` — Remove a custom command

**`flume.config`** — Read/write script-specific config.
- `get(key)` — Read a config value
- `set(key, value)` — Write a config value

**`flume.ui`** — UI manipulation.
- `notify(message, level?)` — Show a notification
- `status_item(name, text)` — Set a status bar item
- `input_text()` — Get current input line text
- `set_input_text(text)` — Set the input line

### 7.3 Event System

All IRC events, user actions, and internal state changes flow through a unified event bus. Scripts subscribe to events by name. Events include:

| Event | Payload |
|---|---|
| `message` | server, channel, nick, text, tags |
| `private_message` | server, nick, text, tags |
| `join` | server, channel, nick, account, realname |
| `part` | server, channel, nick, message |
| `quit` | server, nick, message, channels |
| `kick` | server, channel, nick, target, message |
| `nick_change` | server, old_nick, new_nick |
| `topic_change` | server, channel, nick, new_topic |
| `mode_change` | server, target, mode_string, params |
| `notice` | server, target, nick, text |
| `ctcp_request` | server, nick, command, params |
| `ctcp_response` | server, nick, command, params |
| `connect` | server |
| `disconnect` | server, reason |
| `raw` | server, raw_line (for advanced scripts) |
| `input` | buffer, text (before sending, cancelable) |
| `timer` | timer_id |

Events pass through scripts in priority order. Scripts can modify event data or cancel propagation (e.g., a spam filter script can cancel a `message` event to suppress it).

### 7.4 Script Sandboxing

Scripts run in restricted environments:

- **Lua:** Custom environment with no `os.execute`, `io.popen`, or `loadfile` from arbitrary paths. File I/O is restricted to the script's data directory. Network access is only available through the Flume API.
- **Python:** Restricted imports list. No `subprocess`, `os.system`, `shutil.rmtree`. File access limited to script data directory. Users can opt into "trusted mode" per-script to lift restrictions.

### 7.5 Script Directory Structure

```
~/.config/flume/scripts/
├── autoload/           # Scripts loaded automatically on startup
│   ├── highlight.lua
│   └── away_logger.py
├── available/          # Installed but not auto-loaded
│   ├── trivia_bot.lua
│   └── url_preview.py
└── generated/          # LLM-generated scripts (pending review)
    └── auto_respond.lua
```

---

## 8. LLM Script Generation Engine

### 8.1 Overview

Flume includes an in-client prompt UI for generating scripts from natural language descriptions. Users provide their own LLM API key (OpenAI, Anthropic, or other compatible providers) via config.

### 8.2 Configuration

```toml
# ~/.config/flume/config.toml
[llm]
provider = "anthropic"            # "openai", "anthropic", "ollama", "custom"
api_key_env = "FLUME_LLM_KEY"    # Environment variable containing the key
model = "claude-sonnet-4-20250514"
base_url = ""                     # Override for custom/local endpoints
max_tokens = 4096
temperature = 0.3                 # Low temp for code generation
```

API keys are never stored in the config file directly — only environment variable names or system keychain references.

### 8.3 Generation Flow

```
User types: /generate lua "auto-respond with away message when mentioned while away"
                │
                ▼
┌───────────────────────────────────┐
│  1. Build system prompt           │
│     - Flume Script API reference  │
│     - Target language (Lua/Python)│
│     - Available events list       │
│     - Sandboxing constraints      │
└───────────────┬───────────────────┘
                │
                ▼
┌───────────────────────────────────┐
│  2. Call LLM API                  │
│     - Stream response to buffer   │
│     - Show generation progress    │
└───────────────┬───────────────────┘
                │
                ▼
┌───────────────────────────────────┐
│  3. Present script for review     │
│     - Syntax-highlighted preview  │
│     - Diff view if modifying      │
│     - [Accept] [Edit] [Reject]    │
└───────────────┬───────────────────┘
                │
                ▼
┌───────────────────────────────────┐
│  4. Install to generated/ dir     │
│     - Run sandbox validation      │
│     - Optionally move to autoload │
└───────────────────────────────────┘
```

### 8.4 System Prompt Design

The LLM receives a system prompt containing:

- The complete Flume Script API reference (from section 7.2)
- The list of available events (from section 7.3)
- Sandboxing rules (what is/isn't allowed)
- Example scripts demonstrating common patterns
- The user's installed scripts (for context on what already exists)

This ensures generated scripts are immediately compatible with Flume's API without manual adaptation.

### 8.5 Safety

- Generated scripts are always placed in `generated/` and never auto-loaded
- Users must explicitly review and accept before a script is activated
- The sandbox validator runs static analysis on generated code before offering to install
- Network-accessing code is flagged for extra review
- Scripts that attempt restricted operations are rejected with an explanation

---

## 9. Configuration

### 9.1 Config File Location

Flume follows the XDG Base Directory specification:

- Config: `$XDG_CONFIG_HOME/flume/` (default: `~/.config/flume/`)
- Data: `$XDG_DATA_HOME/flume/` (default: `~/.local/share/flume/`)
- Cache: `$XDG_CACHE_HOME/flume/` (default: `~/.cache/flume/`)
- State: `$XDG_STATE_HOME/flume/` (default: `~/.local/state/flume/`)

### 9.2 Main Config Structure

```toml
# ~/.config/flume/config.toml

[general]
default_nick = "emilio"
alt_nicks = ["emilio_", "emilio__"]
realname = "Emilio"
username = "emilio"
quit_message = "Flume — https://github.com/emilio/flume"
timestamp_format = "%H:%M:%S"
scrollback_lines = 10000
url_open_command = "open"        # macOS: "open", Linux: "xdg-open"

[ui]
theme = "solarized-dark"
layout = "default"               # Named layout profile
show_server_tree = true
show_nick_list = true
server_tree_width = 20
nick_list_width = 18
input_history_size = 500

[ui.keybindings]
mode = "emacs"                   # "emacs", "vi", or "custom"

[logging]
enabled = true
directory = "$XDG_DATA_HOME/flume/logs"
format = "plain"                 # "plain" or "json"
rotate = "daily"

[notifications]
highlight_bell = true
highlight_words = ["emilio", "flume"]
notify_private = true
notify_highlight = true

[ctcp]
version_reply = "Flume 0.1.0"
respond_to_version = true
respond_to_ping = true
respond_to_time = true
rate_limit = 3                   # Max responses per 10 seconds

[dcc]
enabled = false
auto_accept = false
download_directory = "~/Downloads/flume"
port_range = [1024, 65535]
passive = true                   # Prefer passive DCC for NAT

[llm]
provider = "anthropic"
api_key_env = "FLUME_LLM_KEY"
model = "claude-sonnet-4-20250514"
temperature = 0.3
```

### 9.3 Server Config

```toml
# ~/.config/flume/servers/libera.toml

[server]
name = "Libera Chat"
address = "irc.libera.chat"
port = 6697
tls = true
password = ""                    # Server password (not NickServ)

[auth]
method = "sasl"                  # "sasl", "nickserv", "none"
sasl_mechanism = "PLAIN"         # "PLAIN", "SCRAM-SHA-256", "EXTERNAL"
sasl_username = "emilio"
sasl_password_env = "FLUME_LIBERA_PASS"
client_cert = ""                 # Path for EXTERNAL auth

[identity]
nick = ""                        # Override global default
alt_nicks = []
realname = ""
username = ""

[channels]
autojoin = ["#rust", "#linux", "#flume"]
# Per-channel keys
keys = { "#secret" = "password123" }

[bouncer]
type = "none"                    # "none", "znc", "soju"
# ZNC-specific settings
playback = true

[advanced]
encoding = "utf-8"               # Fallback: "latin1"
flood_delay_ms = 500
reconnect_attempts = 10
reconnect_delay_ms = 5000
```

---

## 10. Data Model

### 10.1 Core Types

```
ServerConnection
├── id: Uuid
├── name: String
├── config: ServerConfig
├── state: ConnectionState (Disconnected | Connecting | Registering | Connected)
├── capabilities: HashSet<String>
├── channels: HashMap<String, Channel>
├── queries: HashMap<String, Query>
├── nick: String
├── user_modes: HashSet<char>
├── lag_ms: u64
└── last_activity: Instant

Channel
├── name: String
├── topic: Option<Topic>
├── modes: ChannelModes
├── members: HashMap<String, MemberStatus>
├── buffer: Buffer
├── joined: bool
└── key: Option<String>

Buffer
├── id: Uuid
├── lines: VecDeque<BufferLine>
├── scroll_position: usize
├── unread_count: u32
├── highlight_count: u32
├── last_read: Option<Instant>
└── search_state: Option<SearchState>

BufferLine
├── timestamp: DateTime<Utc>
├── source: LineSource (Server | Nick(String) | System | Action(String))
├── text: String
├── tags: HashMap<String, String>
├── highlight: bool
└── style: Option<LineStyle>
```

### 10.2 Persistent State

Stored in `$XDG_STATE_HOME/flume/`:

- `window_state.toml` — Window layout, split positions, active buffer
- `scrollback/` — Compressed scrollback history per buffer (optional)
- `sts_policies.toml` — Cached STS policies per server
- `monitor_lists.toml` — MONITOR lists per server

---

## 11. Async Architecture

### 11.1 Task Model

```
Main Thread (TUI render loop)
│
├── tokio::spawn ── ServerConnection("libera")
│   ├── TCP read loop → parse → event bus
│   ├── TCP write loop ← command queue
│   └── Reconnect timer
│
├── tokio::spawn ── ServerConnection("efnet")
│   ├── TCP read loop → parse → event bus
│   ├── TCP write loop ← command queue
│   └── Reconnect timer
│
├── tokio::spawn ── DCC Transfer (file.zip)
│   └── TCP stream → progress events
│
├── tokio::spawn ── Script Engine
│   └── Event handler dispatch loop
│
└── tokio::spawn ── LLM Request (if active)
    └── HTTP stream → generation progress events
```

### 11.2 Channel Architecture

```
ServerConnection ──(mpsc)──► Event Bus ──(broadcast)──► TUI Layer
                                │                       Script Engine
                                │                       Logger
                                │
User Input ──(mpsc)──► Command Router ──(mpsc)──► ServerConnection
                           │
                           └──► Script Engine (for /commands)
```

The event bus uses `tokio::sync::broadcast` so multiple consumers (TUI, scripts, logger) all receive every event. Commands flow in the opposite direction via `tokio::sync::mpsc` channels.

### 11.3 Render Loop

The TUI render loop runs at a configurable tick rate (default: 30fps). Each tick:

1. Drain all pending events from the broadcast channel
2. Update internal state (buffers, nick lists, connection status)
3. Check for user input via crossterm's async event stream
4. Render the full frame via ratatui

This ensures the UI stays responsive regardless of IRC traffic volume.

---

## 12. Build and Distribution

### 12.1 Feature Flags

```toml
# Cargo.toml
[features]
default = ["lua", "tls"]
lua = ["mlua"]
python = ["pyo3"]
tls = ["rustls", "webpki-roots"]
dcc = []                         # DCC support, off by default
llm = ["reqwest"]                # LLM script generation
full = ["lua", "python", "tls", "dcc", "llm"]
```

Users who don't want Python or LLM features can build a minimal binary. The default build includes Lua, TLS, and core IRC functionality.

### 12.2 Binary Size Targets

| Build | Estimated Size |
|---|---|
| Minimal (no scripting, no LLM) | ~2 MB |
| Default (Lua + TLS) | ~3-4 MB |
| Full (Lua + Python + LLM + DCC) | ~5-6 MB |

Python adds minimal overhead to the binary since it links dynamically — the Python runtime must be installed on the target system. A future stretch goal is optional static Python embedding for self-contained distribution.

### 12.3 Platform Support

| Platform | Status | Notes |
|---|---|---|
| macOS (aarch64) | Primary | Apple Silicon native |
| macOS (x86_64) | Primary | Intel Mac support |
| Linux (x86_64) | Primary | glibc and musl targets |
| Linux (aarch64) | Primary | ARM64 (Raspberry Pi, etc.) |
| FreeBSD (x86_64) | Primary | Tier 1 target |
| FreeBSD (aarch64) | Secondary | Less tested |
| iPadOS | Future | Separate SwiftUI project, shared Rust core via FFI |

### 12.4 Installation

```bash
# From source
cargo install flume-irc

# macOS (Homebrew)
brew install flume-irc

# FreeBSD
pkg install flume-irc

# Arch Linux (AUR)
yay -S flume-irc
```

---

## 13. iPadOS Strategy

The iPadOS version will share Flume's core engine (IRC parser, protocol handler, connection manager, script engine) compiled as a static library via `cargo-lipo` or `uniffi`. The UI will be native SwiftUI.

### Shared Rust Core

The `flume-core` crate will be extracted as a standalone library with no TUI dependencies. It exposes a C-compatible FFI (or Swift bindings via `uniffi-rs`) for:

- Connection management
- IRC protocol handling
- Event subscription
- Script engine execution
- Configuration parsing

### Native iPadOS UI

The iPadOS app will use SwiftUI with:

- Split view for server tree + chat buffer (leveraging iPad multitasking)
- Slide-over nick list
- Keyboard shortcut support (for external keyboards)
- System notification integration
- iCloud sync for configuration and themes

### Crate Structure

```
flume/
├── flume-core/          # Protocol, connections, events, scripting (no UI)
├── flume-tui/           # Terminal UI (ratatui) — desktop binary
├── flume-ffi/           # C/Swift FFI bindings for flume-core
└── flume-ios/           # Xcode project consuming flume-ffi
```

---

## 14. Project Phases

### Phase 1: Foundation (Weeks 1-4)

- Project scaffolding and crate structure
- IRC message parser with full IRCv3 message-tag support
- Single-server connection with CAP negotiation and SASL
- Basic TUI: single buffer, input line, status bar
- TOML configuration loading
- Connect to Libera Chat and hold a conversation

### Phase 2: Multi-Server and Core TUI (Weeks 5-8)

- Multi-server connection manager
- Server tree panel
- Tab/window management (switch buffers)
- Nick list panel
- Channel join/part, topic display
- Nick completion (tab)
- Scrollback and search
- Basic theme loading

### Phase 3: Full TUI and Theming (Weeks 9-12)

- Split window support (horizontal/vertical)
- Full theme engine with hot-reload
- Custom layout profiles
- Vi and Emacs keybinding modes
- URL detection and opening
- Notification system (highlights, PMs)
- CTCP handling
- Logging to disk

### Phase 4: Scripting Engine (Weeks 13-16)

- Lua runtime integration
- Flume Script API (events, commands, buffers, UI)
- Script loading/unloading
- Script sandboxing
- Python runtime integration
- Example scripts (highlight, away logger, URL preview)

### Phase 5: LLM-Powered Generation (Weeks 17-20)

- **LLM provider configuration** — BYOK (bring your own key) for Anthropic, OpenAI
- **Script generation** — `/generate script <description>` creates a working Lua or Python script from natural language
  - User describes desired functionality ("auto-respond when someone mentions my nick")
  - LLM generates a complete script using the Flume Script API
  - Script displayed for review before saving
  - User chooses language: `/generate script --lua` or `/generate script --python`
  - Saved to `scripts/generated/` directory
- **Theme generation** — `/generate theme <description>` creates a theme TOML
- **Layout generation** — `/generate layout <description>` creates a layout profile
- System prompt includes full Flume Script API reference, theme schema, layout schema

### Phase 6: DCC (Weeks 21-22)

- DCC SEND/CHAT support
- DCC progress display
- Passive/reverse DCC

### Phase 7: Polish and Release (Weeks 23-26)

- Bouncer support (ZNC, Soju)
- STS policy caching
- Performance profiling and optimization
- Comprehensive error handling
- Man page and documentation
- Package builds (Homebrew, AUR, FreeBSD pkg)
- v1.0 release

### Phase 8: iPadOS (Post v1.0)

- Extract `flume-core` crate
- Build FFI bindings
- SwiftUI app scaffold
- Core feature parity
- TestFlight beta
- App Store submission

---

## 15. Open Questions

1. **Script package manager?** Should Flume have a built-in package manager for community scripts (like weechat's script repository), or is a Git-based approach sufficient for v1.0?

2. **Plugin binary extensions?** Should the scripting engine eventually support compiled Rust plugins (via dynamic loading) for performance-critical extensions?

3. **Proxy support?** Should v1.0 include SOCKS5/HTTP proxy support for Tor and corporate network users?

4. **Accessibility?** What level of screen reader compatibility should the TUI target?

5. **IRCv3 extensions pace?** The IRCv3 spec continues evolving. How aggressively should Flume track draft specifications?

---

## Appendix A: Key Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
ratatui = { version = "0.30", features = ["crossterm"] }
crossterm = "0.28"
mlua = { version = "0.10", features = ["lua54", "async"], optional = true }
pyo3 = { version = "0.23", features = ["auto-initialize"], optional = true }
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
rustls = { version = "0.23", optional = true }
webpki-roots = { version = "0.26", optional = true }
reqwest = { version = "0.12", features = ["rustls-tls", "stream"], optional = true }
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
directories = "5"
unicode-width = "0.2"
```

## Appendix B: Reference Implementations

These existing projects are worth studying for architectural patterns:

- **halloy** — Modern IRC client in Rust with iced GUI. Good reference for IRC protocol handling and IRCv3 implementation.
- **weechat** — C-based, the gold standard for plugin architecture and extensibility.
- **irssi** — C-based, excellent keybinding and window management model.
- **tiny** — Minimal IRC client in Rust with a TUI. Good reference for ratatui + tokio integration patterns.
