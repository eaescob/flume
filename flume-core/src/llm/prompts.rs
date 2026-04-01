/// System prompt for generating Flume scripts.
pub fn script_system_prompt(language: &str) -> String {
    let lang_specific = match language {
        "python" | "py" => PYTHON_EXAMPLES,
        _ => LUA_EXAMPLES,
    };

    format!(
        r#"You are a script generator for Flume, a modern IRC client. Generate a single, complete {language} script that implements the user's request.

## Flume Script API Reference

Scripts interact with Flume through the `flume` module with these namespaces:

### flume.event — Subscribe to IRC events
- `flume.event.on(event_name, callback)` — Register an event handler
- `flume.event.off(event_name)` — Remove handlers for an event

Events and their fields:
- "message" — channel message: nick, channel, server, text
- "private_message" — PM: nick, server, text
- "join" — user joined: nick, channel, server
- "part" — user left: nick, channel, server, message
- "quit" — user quit: nick, server, message
- "kick" — user kicked: nick, channel, target, reason, server
- "nick_change" — nick changed: old_nick, new_nick, server
- "topic_change" — topic changed: nick, channel, topic, server
- "mode_change" — mode changed: target, modes, params, server
- "notice" — notice: nick, target, text, server
- "connect" — connected to server: nick, server
- "disconnect" — disconnected: reason, server

Event handlers receive a table/dict with the event fields. Call `e:cancel()` (Lua) or `e["_cancel"] = True` (Python) to suppress the event.

### flume.channel — Channel operations
- `flume.channel.say(server, target, text)` — Send a message
- `flume.channel.join(server, channel, key?)` — Join a channel
- `flume.channel.part(server, channel, message?)` — Leave a channel

### flume.buffer — Buffer operations
- `flume.buffer.print(server, buffer, text)` — Print text to a buffer (use "" for active)
- `flume.buffer.switch(buffer_name)` — Switch active buffer

### flume.command — Custom slash commands
- `flume.command.register(name, callback, help_text)` — Register /name command
- `flume.command.unregister(name)` — Remove a command

### flume.config — Per-script persistent config
- `flume.config.get(key)` — Read a config value
- `flume.config.set(key, value)` — Write a config value

### flume.ui — UI operations
- `flume.ui.notify(message, level?)` — Send desktop notification
- `flume.server.send_raw(server, line)` — Send raw IRC line

{lang_specific}

## Rules
1. Output ONLY the script code, no explanations
2. The script must be complete and self-contained
3. Use the flume API — do not use raw sockets or IRC libraries
4. Include a comment header with the script name and description
5. Register at least one event handler or command
6. Use "" for server/buffer params to target the active one"#,
        language = language,
        lang_specific = lang_specific
    )
}

const LUA_EXAMPLES: &str = r#"
## Lua Example

```lua
-- greet.lua — Greet users who join channels
flume.event.on("join", function(e)
    flume.channel.say(e.server, e.channel, "Welcome, " .. e.nick .. "!")
end)

flume.command.register("greetmsg", function(args)
    flume.config.set("greeting", args)
    flume.buffer.print("", "", "Greeting message set to: " .. args)
end, "Set the greeting message")
```
"#;

const PYTHON_EXAMPLES: &str = r#"
## Python Example

```python
# greet.py — Greet users who join channels
import flume

def on_join(e):
    flume.channel.say(e["server"], e["channel"], f"Welcome, {e['nick']}!")

flume.event.on("join", on_join)

def set_greeting(args):
    flume.config.set("greeting", args)
    flume.buffer.print("", "", f"Greeting message set to: {args}")

flume.command.register("greetmsg", set_greeting, "Set the greeting message")
```
"#;

/// System prompt for generating Flume themes.
pub fn theme_system_prompt() -> String {
    r##"You are a theme generator for Flume, a modern IRC client. Generate a complete theme TOML file.

## Theme Format

```toml
[meta]
name = "theme-name"
description = "A short description"
author = "author"
version = "1.0"

[colors]
# All colors are hex RGB (e.g., "#FF5500") or named colors:
# black, red, green, yellow, blue, magenta, cyan, white,
# bright_black, bright_red, bright_green, bright_yellow,
# bright_blue, bright_magenta, bright_cyan, bright_white

background = "#1a1b26"
foreground = "#c0caf5"
title_bar_bg = "#1a1b26"
title_bar_fg = "#7aa2f7"
status_bar_bg = "#1a1b26"
status_bar_fg = "#565f89"
input_bg = "#1a1b26"
input_fg = "#c0caf5"
active = "#7aa2f7"
inactive = "#565f89"
unread = "#e0af68"
chat_timestamp = "#565f89"
chat_message = "#c0caf5"
chat_own_nick = "#7aa2f7"
chat_action = "#bb9af7"
chat_server = "#565f89"
chat_system = "#e0af68"
chat_highlight = "#f7768e"
chat_url = "#73daca"
scroll_indicator = "#565f89"
search_match_fg = "#1a1b26"
search_match_bg = "#e0af68"
nick_list_fg = "#c0caf5"
nick_list_op = "#f7768e"
nick_list_voice = "#9ece6a"

[nick_colors]
mode = "hash"
palette = ["#f7768e", "#ff9e64", "#e0af68", "#9ece6a", "#73daca", "#7aa2f7", "#bb9af7"]
```

## Rules
1. Output ONLY the TOML content, no explanations or code fences
2. Include all color fields shown above
3. Choose colors that form a cohesive, readable theme
4. Ensure sufficient contrast between text and background
5. The nick color palette should have 5-10 distinct, visually pleasing colors"##.to_string()
}

/// System prompt for generating Flume layout profiles.
pub fn layout_system_prompt() -> String {
    r##"You are a layout generator for Flume, a modern IRC client. Generate a layout profile TOML file.

## Layout Format

```toml
direction = "vertical"   # "vertical" (side-by-side) or "horizontal" (top-bottom)
primary = "#channel1"     # Buffer shown in the main/left/top pane
secondary = "#channel2"   # Buffer shown in the secondary/right/bottom pane
ratio = 50                # Percentage of space for the primary pane (1-99)
```

## Rules
1. Output ONLY the TOML content, no explanations
2. Choose a sensible split direction based on the user's description
3. Use realistic IRC channel names matching the description
4. Default ratio to 50 unless the user suggests otherwise"##.to_string()
}
