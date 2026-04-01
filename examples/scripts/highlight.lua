-- highlight.lua — Custom highlight word notifications
--
-- Watches channel messages for configurable keywords and sends
-- desktop notifications when they appear.
--
-- Config (~/.config/flume/scripts/highlight.toml):
--   words = "keyword1 keyword2 keyword3"

flume.event.on("message", function(e)
    local words = flume.config.get("words") or ""
    if words == "" then return end

    local text_lower = e.text:lower()
    for word in words:gmatch("%S+") do
        if text_lower:find(word:lower(), 1, true) then
            flume.ui.notify(
                e.nick .. " mentioned '" .. word .. "' in " .. e.channel
            )
            flume.buffer.print(e.server, e.channel,
                "[highlight] " .. e.nick .. " said '" .. word .. "': " .. e.text)
            break
        end
    end
end)

flume.command.register("highlight", function(args)
    if args == "" then
        local words = flume.config.get("words") or "(none)"
        flume.buffer.print("", "", "Highlight words: " .. words)
    else
        flume.config.set("words", args)
        flume.buffer.print("", "", "Highlight words set to: " .. args)
    end
end, "View or set highlight words")
