package.path = "bridge/?.lua;" .. package.path

local protocol = require("protocol")
local original_dofile = dofile
local original_memory_module = package.loaded.memory

local frame_callback
local receive_callback
local send_calls = {}
local incoming_chunks = {}
local warning_messages = {}
local error_messages = {}
local push_attempts = 0

local bridge = {}

function bridge:push_inbound(bytes)
  push_attempts = push_attempts + 1
  if push_attempts == 1 then
    return nil, "bridge queue counters describe an impossible occupancy", true
  end
  assert(bytes == assert(protocol.encode({
    type = protocol.types.SESSION_READY,
    sequence = 1,
    session_epoch = 7,
  })))
  return true
end

function bridge:peek_outbound()
  return nil
end

function bridge:heartbeat()
end

package.loaded.memory = {
  new = function()
    return bridge
  end,
}

local client = {}

function client:add(event, callback)
  if event == "received" then
    receive_callback = callback
  end
end

function client:poll()
end

function client:hasdata()
  return #incoming_chunks > 0
end

function client:receive()
  return table.remove(incoming_chunks, 1)
end

function client:send(bytes, first, last)
  first = first or 1
  last = last or #bytes
  send_calls[#send_calls + 1] = { bytes = bytes, first = first, last = last }
  if #send_calls == 1 then
    return nil, socket.ERRORS.AGAIN
  end
  if #send_calls == 2 then
    return 7
  end
  return last
end

socket = {
  ERRORS = { AGAIN = "again" },
  connect = function(host, port)
    assert(host == "127.0.0.1")
    assert(port == 12345)
    return client
  end,
}

callbacks = {
  add = function(_, event, callback)
    assert(event == "frame")
    frame_callback = callback
  end,
}

console = {
  log = function()
  end,
  warn = function(_, message)
    warning_messages[#warning_messages + 1] = message
  end,
  error = function(_, message)
    error_messages[#error_messages + 1] = message
  end,
}

emu = {}

dofile = function(path)
  if path:match("generated_addresses%.lua$") then
    return {}
  end
  if path:match("session%.lua$") then
    return {
      host = "127.0.0.1",
      port = 12345,
      secret = "0123456789abcdef0123456789abcdef",
    }
  end
  return original_dofile(path)
end

local loaded, load_error = pcall(original_dofile, "bridge/main.lua")
dofile = original_dofile
package.loaded.memory = original_memory_module
assert(loaded, load_error)
assert(frame_callback)
assert(receive_callback)

frame_callback()
frame_callback()
frame_callback()

assert(#send_calls == 3)
assert(send_calls[1].first == 1)
assert(send_calls[2].first == 1)
assert(send_calls[3].first == 8)
assert(send_calls[1].bytes == send_calls[2].bytes)
assert(send_calls[2].bytes == send_calls[3].bytes)
assert(send_calls[3].last == #send_calls[3].bytes)

local session_ready = assert(protocol.encode({
  type = protocol.types.SESSION_READY,
  sequence = 1,
  session_epoch = 7,
}))
incoming_chunks[1] = '{"ok":true}\n' .. session_ready
receive_callback()

frame_callback()
frame_callback()
assert(push_attempts == 1)
assert(#warning_messages == 1)
assert(warning_messages[1]:match("impossible occupancy"))

frame_callback()
assert(push_attempts == 2)
assert(#error_messages == 0)

print("bridge main-loop tests passed")
