tedii.log("buffer_info plugin loading")

tedii.ui.add_status_item("buf_lang", function()
  local lang = tedii.buf.get_language()
  return lang
end, { priority = 90 })

tedii.ui.add_status_item("buf_lines", function()
  local lines = tedii.buf.line_count()
  return "L" .. lines
end, { priority = 80 })

tedii.ui.add_status_item("buf_name", function()
  local name = tedii.buf.get_name()
  if name ~= "Untitled" then
    local parts = {}
    for part in name:gmatch("[^/]+") do
      parts[#parts + 1] = part
    end
    return parts[#parts] or name
  end
  return name
end, { priority = 70 })

tedii.commands.register("bufstats", function()
  local lines = tedii.buf.line_count()
  local name = tedii.buf.get_name()
  local lang = tedii.buf.get_language()
  local version = tedii.buf.get_version()
  print(string.format("Buffer: %s | Lang: %s | Lines: %d | Version: %d", name, lang, lines, version))
end)

tedii.log("buffer_info plugin loaded")
