-- url_title.lua — Announce URLs detected in channel messages
--
-- When someone posts a URL in a channel, this script prints a notice
-- showing who posted it. (Actual title fetching would require HTTP
-- support which isn't available in the sandbox — this is a placeholder
-- that demonstrates the event API.)

flume.event.on("message", function(e)
    -- Simple URL detection pattern
    local url = e.text:match("https?://[%w%-%.]+%.[%w%-]+[/%w%-%.~:/?#%[%]@!$&'()*+,;=%%]*")
    if url then
        flume.buffer.print(e.server, e.channel,
            "[url] " .. e.nick .. " shared: " .. url)
    end
end)

flume.event.on("private_message", function(e)
    local url = e.text:match("https?://[%w%-%.]+%.[%w%-]+[/%w%-%.~:/?#%[%]@!$&'()*+,;=%%]*")
    if url then
        flume.buffer.print(e.server, "",
            "[url] " .. e.nick .. " (PM) shared: " .. url)
    end
end)
