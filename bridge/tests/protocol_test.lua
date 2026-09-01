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

print("bridge protocol tests passed")
