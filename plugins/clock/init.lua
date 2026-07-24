tedii.log("clock plugin loading")

local function tick()
  -- no-op: status items are polled on each render
end

tedii.ui.add_status_item("clock", function()
  return os.date("%H:%M:%S")
end, { priority = 0 })

tedii.events.on_open(function()
  tedii.schedule(1.0, function()
    tick()
    tedii.schedule(1.0, function()
      tick()
      tedii.schedule(1.0, function()
        tick()
        tedii.schedule(1.0, function()
          tick()
          tedii.schedule(1.0, function()
            tick()
            tedii.schedule(1.0, function()
              tick()
            end)
          end)
        end)
      end)
    end)
  end)
end)

tedii.log("clock plugin loaded")
