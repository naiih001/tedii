tedii.log("keybindings plugin loading")

tedii.commands.register("say_hello", function()
  print("Hello from plugin keybinding!")
end)

tedii.keymap.set("normal", "<C-h>", function()
  tedii.commands.run("say_hello")
end)

tedii.keymap.set("normal", "Q", function()
  local line, col = tedii.cursor.get_pos()
  print(string.format("Cursor: line %d, col %d", line + 1, col + 1))
end)

tedii.keymap.set("insert", "<C-d>", function()
  local line, col = tedii.cursor.get_pos()
  local text = tedii.buf.get_line(line)
  tedii.log(string.format("insert mode at line %d, col %d: %s", line, col, text))
end)

tedii.log("keybindings plugin loaded")
