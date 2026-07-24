tedii.log("event_logger plugin loading")

tedii.events.on_open(function()
  tedii.log("event_logger: OpenFile event fired")
end)

tedii.events.on_save(function()
  tedii.log("event_logger: SaveFile event fired")
end)

tedii.events.on_cursor_moved(function()
  local line, col = tedii.cursor.get_pos()
  tedii.log(string.format("event_logger: cursor moved to line %d, col %d", line, col))
end)

tedii.events.on_mode_changed(function()
  tedii.log("event_logger: mode changed")
end)

tedii.events.on_buffer_changed(function()
  local version = tedii.buf.get_version()
  tedii.log(string.format("event_logger: buffer changed (version %d)", version))
end)

tedii.events.on_diagnostics_updated(function()
  tedii.log("event_logger: diagnostics updated")
end)

tedii.log("event_logger plugin loaded")
