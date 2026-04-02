# Flume

A modern, fast, terminal-based IRC client built in Rust.

Flume supports multi-server connections, a rich TUI with theming and split views, vi/emacs keybinding modes, a dual scripting engine (Lua and Python), LLM-powered script and theme generation, DCC file transfers, XDCC, and emoji shortcodes.

**[Documentation](https://docs.flumeirc.io)** | **[GitHub](https://github.com/FlumeIRC/flume)** | **[#flume on Libera.Chat](ircs://irc.libera.chat/#flume)**

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
- **Emoji shortcodes** — type `:thumbsup:` to send :thumbsup:, tab-complete with `:thu` + Tab
- **Secure vault** — encrypted secret storage for passwords and API keys

## Install

### From source (all platforms)

```sh
git clone https://github.com/FlumeIRC/flume.git
cd flume
cargo install --path flume-tui
```

### With Python scripting support

```sh
cargo install --path flume-tui --features python
```

Requires Python 3.10+ development headers.

### Homebrew (macOS/Linux)

```sh
brew install FlumeIRC/tap/flume
```

### Arch Linux (AUR)

```sh
yay -S flume
```

### Debian / Ubuntu

```sh
curl -fsSL https://pkg.flumeirc.io/gpg/flume-signing-key.asc | sudo gpg --dearmor -o /usr/share/keyrings/flume.gpg
echo "deb [signed-by=/usr/share/keyrings/flume.gpg] https://pkg.flumeirc.io/apt stable main" | sudo tee /etc/apt/sources.list.d/flume.list
sudo apt update && sudo apt install flume
```

### Fedora / RHEL / CentOS

```sh
sudo dnf config-manager --add-repo https://pkg.flumeirc.io/rpm/flume.repo
sudo dnf install flume
```

### FreeBSD

```sh
pkg install flume
```

### Pre-built binaries

Download from the [Releases](https://github.com/FlumeIRC/flume/releases) page. Available for macOS (Apple Silicon, Intel), Linux (x86_64, ARM64), and Debian (.deb).

## Quick Start

```sh
# Start Flume
flume

# Add a network with credentials
/secure init
/secure set libera_pass your-sasl-password
/server add libera irc.libera.chat 6697 -tls -autoconnect -username mynick -password ${libera_pass}

# Or configure SASL auth
/server set libera auth_method sasl
/server set libera sasl_username mynick
/server set libera sasl_password ${libera_pass}

# Save and connect
/save
/connect libera

# Join a channel
/join #flume
```

## File Locations

```
~/.config/flume/               # Configuration
  config.toml                  # Main settings
  irc.toml                     # Network definitions

~/.local/share/flume/          # Data
  themes/                      # Theme files
  layouts/                     # Saved split layouts
  scripts/
    lua/autoload/              # Lua scripts loaded on startup
    python/autoload/           # Python scripts loaded on startup
    available/                 # Installed but not auto-loaded
    generated/                 # Created by /generate
  vault.toml                   # Encrypted secrets
  logs/                        # IRC message logs
```

### Example config.toml

```toml
[general]
default_nick = "mynick"
quit_message = "Flume IRC"

[ui]
theme = "solarized-dark"
show_join_part = true
show_hostmask_on_join = true

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

## LLM Setup

Interactive setup:

```
/generate init
```

This walks you through choosing a provider and storing your API key. Then:

```
/generate script --name greeter auto-respond when someone says hello
/generate theme --name midnight dark blue with warm orange accents
/generate layout --name monitoring #ops on the left and #alerts on the right
```

## Scripting

Flume supports both Lua and Python scripts with the same API:

**Lua** (`~/.local/share/flume/scripts/lua/autoload/hello.lua`):
```lua
flume.event.on("message", function(e)
    if e.text:find("hello") then
        flume.channel.say(e.server, e.channel, "Hello, " .. e.nick .. "!")
    end
end)
```

**Python** (`~/.local/share/flume/scripts/python/autoload/hello.py`):
```python
import flume

def on_message(e):
    if "hello" in e.get("text", ""):
        flume.channel.say(e["server"], e["channel"], f"Hello, {e['nick']}!")

flume.event.on("message", on_message)
```

Scripts can register custom commands with help text:

```lua
flume.command.register("greet", function(args)
    flume.buffer.print("", "", "Hello " .. args)
end, "Greet someone by name")
```

Then `/help greet` shows the help text.

See `examples/scripts/` for more examples.

## Emoji

Type `:shortcode:` in messages — they're replaced with emoji on send:

```
:thumbsup: → 👍    :fire: → 🔥    :wave: → 👋    :heart: → ❤️
:rocket: → 🚀     :100: → 💯     :tada: → 🎉    :coffee: → ☕
```

Tab-complete: type `:thu` then Tab to cycle through matches. Search with `/emoji fire`.

## Keybindings

| Key | Action |
|-----|--------|
| `Ctrl+C` | Quit |
| `Ctrl+X` | Cycle servers |
| `Alt+1-9` | Jump to buffer |
| `Alt+Left/Right` | Cycle buffers |
| `Alt+Tab` | Swap split focus |
| `PageUp/Down` | Scroll |
| `Tab` | Nick / emoji completion |

Emacs mode adds `Ctrl+A/E/B/F/D/K/U/W/P/N` and `Alt+B/F`. Vi mode adds normal/insert modes with `h/j/k/l/w/b/i/a/A` and more.

## Commands

Use `/help` for the full list, or `/help <command>` for details on any command.

| Command | Description |
|---------|-------------|
| `/set <key> <value>` | View or change settings |
| `/go <name or #>` | Jump to buffer by name or number |
| `/split v\|h <buf>` | Split view |
| `/script load <name>` | Load a script |
| `/generate script <desc>` | AI-generate a script |
| `/dcc list` | Show DCC transfers |
| `/xdcc <bot> <pack#>` | Request XDCC pack |
| `/snotice add ...` | Add server notice routing rule |
| `/emoji <search>` | Search emoji shortcodes |

## Custom Formats

Every message format is configurable in `config.toml`:

```toml
[formats]
message = "<${nick}> ${text}"
join = "--> ${nick} (${userhost}) has joined ${channel}"
part = "<-- ${nick} has left ${channel}${?message| (${message})}"
quit = "<-- ${nick} has quit${?message| (${message})}"
nick_change = "*** ${old_nick} is now known as ${new_nick}"
```

## Server Notice Routing (IRC Operators)

Parse and route raw server notices with regex:

```
/snotice add --match "Client connecting: (\S+)" --format "[connect] ${1}" --buffer snotice-connections
/snotice add --match "Oper-up" --suppress
/snotice save
```

## License

Apache-2.0
