package.path = "bridge/?.lua;" .. package.path

local protocol = require("protocol")

assert(protocol.crc32("123456789") == 0xCBF43926)

local ready, error_value = protocol.encode({
  type = protocol.types.ROM_READY,
  sequence = 1,
  session_epoch = 0,
  payload = "",
})
assert(ready, error_value)
assert(#ready == 144)
assert(string.byte(ready, 141) == 0x3D)
assert(string.byte(ready, 142) == 0x37)
assert(string.byte(ready, 143) == 0xEE)
assert(string.byte(ready, 144) == 0x9C)

local decoded, decode_error = protocol.decode(ready, "outbound")
assert(decoded, decode_error)
assert(decoded.type == protocol.types.ROM_READY)
assert(decoded.sequence == 1)
assert(decoded.session_epoch == 0)
assert(decoded.payload == "")

local corrupted = string.char((string.byte(ready, 1) ~ 1) & 0xFF) .. string.sub(ready, 2)
assert(protocol.decode(corrupted, "outbound") == nil)

local session_ready = assert(protocol.encode({
  type = protocol.types.SESSION_READY,
  sequence = 1,
  session_epoch = 1,
}))
assert(protocol.decode(session_ready, "outbound") == nil)
assert(protocol.decode(session_ready, "inbound") ~= nil)

local proposal = string.rep("\xA5", 16)
for route = 1, 6 do
  local departure = route <= 2 and 1 or (route <= 4 and 2 or 4)
  local request = assert(protocol.encode_group_travel({
    kind = 1, route = route, request_id = route, proposal_id = string.rep("\0", 16),
    departure = departure, result = 0, reason = 0,
  }, "outbound"))
  assert(#request == 32)
  assert(assert(protocol.decode_group_travel(request, "outbound")).route == route)
  assert(assert(protocol.decode_group_travel(request, "outbound")).departure == departure)
end
for route = 3, 4 do
  local maiden = assert(protocol.encode_group_travel({
    kind = 1, route = route, request_id = route + 10, proposal_id = string.rep("\0", 16),
    departure = 3, result = 0, reason = 0,
  }, "outbound"))
  local decoded_maiden = assert(protocol.decode_group_travel(maiden, "outbound"))
  assert(decoded_maiden.route == route)
  assert(decoded_maiden.departure == 3)
end
for _, route in ipairs({1, 2, 5, 6}) do
  local rejected = protocol.encode_group_travel({
    kind = 1, route = route, request_id = route + 20, proposal_id = string.rep("\0", 16),
    departure = 3, result = 0, reason = 0,
  }, "outbound")
  assert(rejected == nil)
end
local commit = assert(protocol.encode_group_travel({
  kind = 3, route = 6, request_id = 9, proposal_id = proposal, result = 0, reason = 0,
  departure = 4,
}, "inbound"))
assert(protocol.decode_group_travel(commit, "outbound") == nil)
assert(protocol.decode_group_travel(commit:sub(1, 28) .. "\1\0\0\0", "inbound") == nil)
assert(protocol.decode_group_travel(commit:sub(1, 7) .. "\1" .. commit:sub(9), "inbound") == nil)
assert(protocol.decode_group_travel(commit:sub(1, 6) .. "\0" .. commit:sub(8), "inbound") == nil)
local travel_frame = assert(protocol.encode({
  type = protocol.types.GROUP_TRAVEL_SERVER, sequence = 2, session_epoch = 1,
  payload = commit,
}))
assert(protocol.decode(travel_frame, "inbound"))
assert(protocol.decode(travel_frame, "outbound") == nil)

print("bridge protocol tests passed")
