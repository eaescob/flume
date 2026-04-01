# greet.py — Simple greeter bot script
#
# Responds to "!greet" in channels and registers a /pyinfo command.

import flume

def on_message(e):
    text = e.get("text", "")
    if text.startswith("!greet"):
        nick = e.get("nick", "someone")
        channel = e.get("channel", "")
        server = e.get("server", "")
        if channel:
            flume.channel.say(server, channel, f"Hello, {nick}! Welcome to {channel}.")

flume.event.on("message", on_message)

def pyinfo(args):
    flume.buffer.print("", "", "Python scripting is active!")
    flume.buffer.print("", "", "  Use /script list to see loaded scripts")
    flume.buffer.print("", "", "  Python scripts have full import access")

flume.command.register("pyinfo", pyinfo, "Show Python script info")
