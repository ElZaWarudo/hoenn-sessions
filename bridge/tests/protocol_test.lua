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

local nonce = string.char(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16)
local arrival_challenge = assert(protocol.encode({
  type = protocol.types.ARRIVAL_CHALLENGE,
  sequence = 2,
  session_epoch = 0,
  payload = nonce,
}))
assert(assert(protocol.decode(arrival_challenge, "inbound")).payload == nonce)
assert(protocol.decode(arrival_challenge, "outbound") == nil)
assert(protocol.encode({type = protocol.types.ARRIVAL_CHALLENGE,
  sequence = 2, session_epoch = 1, payload = nonce}) == nil)
assert(protocol.encode({type = protocol.types.ARRIVAL_CHALLENGE,
  sequence = 2, session_epoch = 0, payload = string.rep("\0", 16)}) == nil)

local arrival_payload = nonce .. string.rep("\xA5", 32)
  .. string.pack("<I4I4I1I1", 2, 7, 3, 4) .. string.rep("\0", 6)
local arrival = assert(protocol.decode_arrival_proof(arrival_payload))
assert(arrival.nonce == nonce and arrival.world_id == 2 and arrival.save_generation == 7)
assert(arrival.map_group == 3 and arrival.map_num == 4)
local arrival_proof = assert(protocol.encode({type = protocol.types.ARRIVAL_PROOF,
  sequence = 3, session_epoch = 0, payload = arrival_payload}))
assert(assert(protocol.decode(arrival_proof, "outbound")).payload == arrival_payload)
assert(protocol.decode(arrival_proof, "inbound") == nil)
assert(protocol.encode({type = protocol.types.ARRIVAL_PROOF,
  sequence = 3, session_epoch = 1, payload = arrival_payload}) == nil)
assert(protocol.encode({type = protocol.types.ARRIVAL_PROOF,
  sequence = 3, session_epoch = 0, payload = arrival_payload:sub(1, 63) .. "\1"}) == nil)
assert(protocol.decode_arrival_proof(string.rep("\0", 64)) == nil)

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

assert(protocol.is_outbound(protocol.types.COMPANION_STATE))
assert(protocol.is_outbound(protocol.types.SOCIAL_SIGNAL))
assert(protocol.is_outbound(protocol.types.PORTAL_TRAVEL_REQUEST))
assert(not protocol.is_outbound(protocol.types.REMOTE_COMPANION))
assert(not protocol.is_inbound(protocol.types.SOCIAL_SIGNAL))
assert(protocol.is_inbound(protocol.types.REMOTE_COMPANION))
assert(protocol.is_inbound(protocol.types.REMOTE_SOCIAL_SIGNAL))

local companion_frame = assert(protocol.encode({
  type = protocol.types.COMPANION_STATE, sequence = 3, session_epoch = 1,
  payload = string.rep("\0", 8),
}))
assert(protocol.decode(companion_frame, "outbound"))
assert(protocol.decode(companion_frame, "inbound") == nil)

local remote_signal_frame = assert(protocol.encode({
  type = protocol.types.REMOTE_SOCIAL_SIGNAL, sequence = 4, session_epoch = 1,
  payload = string.rep("\0", 20),
}))
assert(protocol.decode(remote_signal_frame, "inbound"))
assert(protocol.decode(remote_signal_frame, "outbound") == nil)

local portal_frame = assert(protocol.encode({
  type = protocol.types.PORTAL_TRAVEL_REQUEST, sequence = 5, session_epoch = 1,
  payload = "to_cormoria",
}))
assert(assert(protocol.decode(portal_frame, "outbound")).payload == "to_cormoria")
assert(protocol.decode(portal_frame, "inbound") == nil)
assert(protocol.encode({ type = protocol.types.PORTAL_TRAVEL_REQUEST,
  sequence = 6, session_epoch = 0, payload = "to_cormoria" }) == nil)
for _, invalid in ipairs({"", "To_cormoria", "to-cormoria", "to/cormoria",
  "to_cormoria\0", string.rep("a", 97)}) do
  assert(protocol.encode({ type = protocol.types.PORTAL_TRAVEL_REQUEST,
    sequence = 6, session_epoch = 1, payload = invalid }) == nil)
end
assert(protocol.encode({ type = protocol.types.PORTAL_TRAVEL_REQUEST,
  sequence = 6, session_epoch = 1, payload = string.rep("a", 96) }) ~= nil)

-- The decoder must also reject malformed but checksummed frames from a ROM.
local invalid_payload = "To_cormoria"
local prefix = string.pack("<I2I2I4I4", protocol.types.PORTAL_TRAVEL_REQUEST,
  #invalid_payload, 7, 1) .. invalid_payload
  .. string.rep("\0", protocol.PAYLOAD_SIZE - #invalid_payload)
assert(protocol.decode(prefix .. string.pack("<I4", protocol.crc32(prefix)), "outbound") == nil)

print("bridge protocol tests passed")
