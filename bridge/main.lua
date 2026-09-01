local source = debug.getinfo(1, "S").source
if string.sub(source, 1, 1) == "@" then source = string.sub(source, 2) end
local script_directory = string.match(source, "^(.*[\\/])") or "./"
package.path = script_directory .. "?.lua;" .. package.path

local protocol = require("protocol")
local memory_module = require("memory")
local manifest = dofile(script_directory .. "generated_addresses.lua")
local session = dofile(script_directory .. "session.lua")

local function valid_session(value)
  return type(value) == "table"
    and value.host == "127.0.0.1"
    and math.type(value.port) == "integer"
    and value.port > 0 and value.port <= 65535
    and type(value.secret) == "string"
    and #value.secret == 32
    and value.secret:match("^[0-9a-f]+$") ~= nil
end

if not valid_session(session) then
  error("bridge/session.lua must contain a launcher-generated loopback session")
end

local bridge, bridge_error = memory_module.new(emu, manifest)
if not bridge then error(bridge_error) end

local client, connect_error = socket.connect(session.host, session.port)
if not client then error("sidecar connection failed: " .. tostring(connect_error)) end

local handshake = string.format(
  '{"secret":"%s","bridge_abi":%d,"protocol_version":%d}\n',
  session.secret, protocol.ABI_VERSION, protocol.PROTOCOL_VERSION)
if #handshake > 256 then error("internal handshake exceeds its protocol bound") end

local receive_buffer = ""
local authenticated = false
local pending_handshake = { bytes = handshake, offset = 1 }
local pending_outbound = nil
local frame_counter = 0

client:add("received", function()
  while client:hasdata() do
    local bytes, receive_error = client:receive(4096)
    if not bytes then
      console:error("sidecar receive failed: " .. tostring(receive_error))
      return
    end
    receive_buffer = receive_buffer .. bytes
  end
end)

client:add("error", function(error_value)
  console:error("sidecar socket error: " .. tostring(error_value))
end)

local function process_handshake_response()
  local newline = string.find(receive_buffer, "\n", 1, true)
  if not newline then
    if #receive_buffer > 256 then error("sidecar response exceeds handshake bound") end
    return
  end
  local response = string.sub(receive_buffer, 1, newline)
  receive_buffer = string.sub(receive_buffer, newline + 1)
  if response ~= '{"ok":true}\n' then error("sidecar rejected the bridge handshake") end
  authenticated = true
end

local function push_inbound_frames()
  while #receive_buffer >= protocol.MESSAGE_SIZE do
    local bytes = string.sub(receive_buffer, 1, protocol.MESSAGE_SIZE)
    local pushed, push_error, queue_recovered = bridge:push_inbound(bytes)
    if not pushed then
      if push_error == "ROM inbound queue is full" then return end
      if queue_recovered then
        console:warn("reset corrupt ROM inbound queue: " .. tostring(push_error))
        return
      end
      error("invalid sidecar bridge frame: " .. tostring(push_error))
    end
    receive_buffer = string.sub(receive_buffer, protocol.MESSAGE_SIZE + 1)
  end
end

local function advance_pending_send(pending)
  local last_sent, send_error = client:send(pending.bytes, pending.offset, #pending.bytes)
  if not last_sent then
    if send_error == socket.ERRORS.AGAIN then return false end
    return nil, send_error
  end
  if math.type(last_sent) ~= "integer"
    or last_sent < pending.offset
    or last_sent > #pending.bytes then
    return nil, "socket reported invalid send progress"
  end
  pending.offset = last_sent + 1
  return pending.offset > #pending.bytes
end

local function send_handshake()
  local complete, send_error = advance_pending_send(pending_handshake)
  if complete == nil then
    error("sidecar handshake send failed: " .. tostring(send_error))
  end
  if complete then pending_handshake = nil end
end

local function send_outbound_frame()
  if not pending_outbound then
    local message, peek_error = bridge:peek_outbound()
    if not message then
      if peek_error then console:warn("discarded ROM bridge frame: " .. peek_error) end
      return
    end
    pending_outbound = {
      bytes = message.bytes,
      read_index = message.read_index,
      offset = 1,
    }
  end

  local complete, send_error = advance_pending_send(pending_outbound)
  if complete == nil then
    console:error("sidecar send failed: " .. tostring(send_error))
    return
  end
  if complete then
    local committed, commit_error = bridge:commit_outbound(pending_outbound.read_index)
    if not committed then error(commit_error) end
    pending_outbound = nil
  end
end

callbacks:add("frame", function()
  frame_counter = (frame_counter + 1) & 0xFFFFFFFF
  client:poll()
  if pending_handshake then
    send_handshake()
    return
  end
  if not authenticated then
    process_handshake_response()
    return
  end
  push_inbound_frames()
  send_outbound_frame()
  if frame_counter % 60 == 0 then bridge:heartbeat() end
end)

console:log("PokéCrossroads co-op bridge connected to the local sidecar")
