tedii.log("hello_world plugin loading")

tedii.commands.register("hello", function()
  print("Hello from tedii plugin!")
end)

tedii.ui.add_status_item("hello_world", function()
  return "hello"
end, { priority = 50 })

tedii.events.on_open(function()
  tedii.log("hello_world: file opened")
end)

tedii.events.on_save(function()
  tedii.log("hello_world: file saved")
end)

tedii.log("hello_world plugin loaded")
