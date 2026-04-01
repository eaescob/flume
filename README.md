# Flume

A modern, fast, terminal-based IRC client built in Rust.

Flume supports multi-server connections, a rich TUI with theming and split views, vi/emacs keybinding modes, a dual scripting engine (Lua and Python), LLM-powered script and theme generation, DCC file transfers, and XDCC support.

## Features

- **Multi-server** — connect to multiple IRC networks simultaneously
- **Full IRCv3** — message-tags, SASL, server-time, away-notify, STS, with graceful degradation to RFC 1459/2812
- **Theming** — hot-reloadable theme engine with hash-based nick coloring
- **Split views** — side-by-side or top-bottom buffer splits with saveable layouts
- **Vi and Emacs modes** — configurable keybinding modes with full readline support
- **Lua + Python scripting** — dual runtime with unified API, event system, and custom commands
- **LLM generation** — describe what you want in plain English and Flume writes the script, theme, or layout for you (bring your own API key)
- **DCC** — file transfers (send/receive/resume), DCC CHAT, XDCC bot support, passive DCC for NAT
- **Bouncer support** — ZNC and Soju with buffer playback
- **Secure vault** — encrypted secret storage for passwords and API keys

## Install

### From source (all platforms)

```sh
git clone https://github.com/emilio/flume.git
cd flume
cargo install --path flume-tui
```

The binary installs as `flume-tui`. You can alias it:

```sh
alias flume=flume-tui
```

### With Python scripting support

```sh
cargo install --path flume-tui --features python
```

Requires Python 3.10+ development headers.

### Homebrew (macOS/Linux)

```sh
brew install emilio/tap/flume
```

### Arch Linux (AUR)

```sh
yay -S flume
```

### FreeBSD

```sh
pkg install flume
```

### Pre-built binaries

Download from the [Releases](https://github.com/emilio/flume/releases) page.

## Quick Start

```sh
# Start Flume
flume-tui

# Add a network
/server add libera irc.libera.chat 6697 -tls -autoconnect

# Store your password securely
/secure init
/secure set libera_pass your-password-here

# Save and connect
/save
/connect libera

# Join a channel
/join #flume
```

## Configuration

Config files live in `~/.config/flume/`:

| File | Purpose |
|------|---------|
| `config.toml` | Main settings (UI, keybindings, notifications, LLM, DCC) |
| `irc.toml` | Network/server definitions |
| `themes/` | Theme files (TOML) |
| `scripts/autoload/` | Scripts loaded on startup |
| `scripts/available/` | Installed scripts (load manually) |

### Example config.toml

```toml
[general]
default_nick = "mynick"
quit_message = "Flume IRC"

[ui]
theme = "solarized-dark"

[ui.keybindings]
mode = "vi"  # or "emacs"

[notifications]
highlight_words = ["mynick", "flume"]

[llm]
provider = "anthropic"  # or "openai"
model = "claude-sonnet-4-20250514"

[dcc]
enabled = true
download_directory = "~/Downloads/flume"
passive = true
```

### LLM Setup

Store your API key in the vault:

```
/secure set flume_llm_key sk-your-api-key
```

Then generate scripts, themes, or layouts:

```
/generate script auto-respond when someone says hello in #general
/generate theme dark blue with warm orange accents
/generate layout monitoring setup with #ops on the left and #alerts on the right
```

## Scripting

Flume supports both Lua and Python scripts with the same API:

**Lua:**
```lua
flume.event.on("message", function(e)
    if e.text:find("hello") then
        flume.channel.say(e.server, e.channel, "Hello, " .. e.nick .. "!")
    end
end)
```

**Python:**
```python
import flume

def on_message(e):
    if "hello" in e.get("text", ""):
        flume.channel.say(e["server"], e["channel"], f"Hello, {e['nick']}!")

flume.event.on("message", on_message)
```

See `examples/scripts/` for more examples.

## Keybindings

| Key | Action |
|-----|--------|
| `Ctrl+C` | Quit |
| `Ctrl+X` | Cycle servers |
| `Alt+1-9` | Jump to buffer |
| `Alt+Left/Right` | Cycle buffers |
| `Alt+Tab` | Swap split focus |
| `PageUp/Down` | Scroll |
| `Tab` | Nick completion |

Emacs mode adds `Ctrl+A/E/B/F/D/K/U/W/P/N` and `Alt+B/F`. Vi mode adds normal/insert modes with `h/j/k/l/w/b/i/a/A` and more.

## License

BSD-3-Clause
