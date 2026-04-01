-- away_logger.lua — Log private messages and highlights while away
--
-- When you set yourself away (e.g., /away brb), this script collects
-- private messages and highlights. When you come back (/back), it
-- replays them as a summary in the active buffer.

local away = false
local missed = {}

flume.event.on("private_message", function(e)
    if away then
        table.insert(missed, {
            nick = e.nick,
            text = e.text,
            server = e.server,
            time = os.date("%H:%M"),
        })
    end
end)

flume.event.on("message", function(e)
    -- Simple highlight detection: check if our nick is mentioned
    -- (Scripts don't have direct access to our nick, so this uses config)
    if not away then return end

    local my_nick = flume.config.get("nick") or ""
    if my_nick ~= "" and e.text:lower():find(my_nick:lower(), 1, true) then
        table.insert(missed, {
            nick = e.nick,
            text = e.text,
            channel = e.channel,
            server = e.server,
            time = os.date("%H:%M"),
        })
    end
end)

flume.command.register("awayon", function(args)
    away = true
    missed = {}
    flume.buffer.print("", "", "Away logger: now logging missed messages")
end, "Start logging missed messages")

flume.command.register("awayoff", function(args)
    away = false
    if #missed == 0 then
        flume.buffer.print("", "", "Away logger: no missed messages")
    else
        flume.buffer.print("", "", "Away logger: " .. #missed .. " missed messages:")
        for _, m in ipairs(missed) do
            local loc = m.channel or "PM"
            flume.buffer.print("", "",
                string.format("  [%s] <%s> (%s) %s", m.time, m.nick, loc, m.text))
        end
        missed = {}
    end
end, "Stop logging and show missed messages")
