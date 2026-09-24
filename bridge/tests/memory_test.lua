package.path = "bridge/?.lua;" .. package.path

local memory_module = require("memory")
local protocol = require("protocol")

local Core = {}
Core.__index = Core

function Core.new()
  return setmetatable({ bytes = {} }, Core)
end

function Core:read8(address)
  return self.bytes[address] or 0
end

function Core:write8(address, value)
  self.bytes[address] = value & 0xFF
end

function Core:read16(address)
  return self:read8(address) | (self:read8(address + 1) << 8)
end

function Core:write16(address, value)
  self:write8(address, value)
  self:write8(address + 1, value >> 8)
end

function Core:read32(address)
  return self:read16(address) | (self:read16(address + 2) << 16)
end

function Core:write32(address, value)
  self:write16(address, value)
  self:write16(address + 2, value >> 16)
end

function Core:readRange(address, length)
  local result = {}
  for offset = 0, length - 1 do
    result[#result + 1] = string.char(self:read8(address + offset))
  end
  return table.concat(result)
end

local manifest = {
  address = 0x02001000,
  size = 9244,
  magic = 0x504B434F,
  abi_version = 1,
  protocol_version = 1,
  game_build_id = 0x00010000,
  offsets = {
    last_sidecar_heartbeat = 16,
    game_to_network = 20,
    network_to_game = 4632,
  },
  queue = { capacity = 32, read_index = 0, write_index = 2, entries = 4 },
  message = { size = 144 },
}

local core = Core.new()
core:write32(manifest.address, manifest.magic)
core:write16(manifest.address + 4, manifest.abi_version)
core:write16(manifest.address + 6, manifest.protocol_version)
core:write32(manifest.address + 8, manifest.game_build_id)

local bridge, create_error = memory_module.new(core, manifest)
assert(bridge, create_error)

local outbound_queue = manifest.address + manifest.offsets.game_to_network
local ready = assert(protocol.encode({
  type = protocol.types.ROM_READY,
  sequence = 1,
  session_epoch = 0,
}))
for index = 1, #ready do
  core:write8(outbound_queue + manifest.queue.entries + index - 1, string.byte(ready, index))
end
core:write16(outbound_queue + manifest.queue.write_index, 1)

local pending, peek_error = bridge:peek_outbound()
assert(pending, peek_error)
assert(pending.bytes == ready)
assert(core:read16(outbound_queue + manifest.queue.read_index) == 0)
assert(bridge:commit_outbound(pending.read_index))
assert(core:read16(outbound_queue + manifest.queue.read_index) == 1)

core:write16(outbound_queue + manifest.queue.read_index, 2)
core:write16(outbound_queue + manifest.queue.write_index, 1)
local corrupt_outbound, corrupt_outbound_error, outbound_recovered = bridge:peek_outbound()
assert(not corrupt_outbound)
assert(corrupt_outbound_error:match("impossible occupancy"))
assert(outbound_recovered)
assert(core:read16(outbound_queue + manifest.queue.read_index) == 1)
local empty_outbound, empty_outbound_error = bridge:peek_outbound()
assert(not empty_outbound)
assert(not empty_outbound_error)

core:write16(outbound_queue + manifest.queue.read_index, 0xFFFF)
core:write16(outbound_queue + manifest.queue.write_index, 0)
local outbound_wrap_slot = outbound_queue + manifest.queue.entries
  + 31 * manifest.message.size
for index = 1, #ready do
  core:write8(outbound_wrap_slot + index - 1, string.byte(ready, index))
end
local wrapped_outbound, wrapped_outbound_error = bridge:peek_outbound()
assert(wrapped_outbound, wrapped_outbound_error)
assert(wrapped_outbound.bytes == ready)
assert(bridge:commit_outbound(wrapped_outbound.read_index))
assert(core:read16(outbound_queue + manifest.queue.read_index) == 0)

local inbound_queue = manifest.address + manifest.offsets.network_to_game
core:write16(inbound_queue + manifest.queue.read_index, 0xFFFF)
core:write16(inbound_queue + manifest.queue.write_index, 0xFFFF)
local session_ready = assert(protocol.encode({
  type = protocol.types.SESSION_READY,
  sequence = 1,
  session_epoch = 1,
}))
assert(bridge:push_inbound(session_ready))
assert(core:read16(inbound_queue + manifest.queue.write_index) == 0)
local slot_address = inbound_queue + manifest.queue.entries + 31 * manifest.message.size
assert(core:readRange(slot_address, manifest.message.size) == session_ready)

core:write16(inbound_queue + manifest.queue.read_index, 1)
core:write16(inbound_queue + manifest.queue.write_index, 0)
local pushed, corrupt_error, inbound_recovered = bridge:push_inbound(session_ready)
assert(not pushed)
assert(corrupt_error:match("impossible occupancy"))
assert(inbound_recovered)
assert(core:read16(inbound_queue + manifest.queue.write_index) == 1)
assert(bridge:push_inbound(session_ready))
assert(core:read16(inbound_queue + manifest.queue.write_index) == 2)

local heartbeat_address = manifest.address + manifest.offsets.last_sidecar_heartbeat
bridge:heartbeat()
bridge:heartbeat()
assert(core:read32(heartbeat_address) == 2)

-- Queue occupancy validation uses the manifest capacity, not a hardcoded 32.
local alt_manifest = {
  address = 0x02002000,
  size = 2332,
  magic = 0x504B434F,
  abi_version = 1,
  protocol_version = 1,
  game_build_id = 0x00010000,
  offsets = {
    last_sidecar_heartbeat = 16,
    game_to_network = 20,
    network_to_game = 1176,
  },
  queue = { capacity = 8, read_index = 0, write_index = 2, entries = 4 },
  message = { size = 144 },
}
local alt_core = Core.new()
alt_core:write32(alt_manifest.address, alt_manifest.magic)
alt_core:write16(alt_manifest.address + 4, alt_manifest.abi_version)
alt_core:write16(alt_manifest.address + 6, alt_manifest.protocol_version)
alt_core:write32(alt_manifest.address + 8, alt_manifest.game_build_id)
local alt_bridge = assert(memory_module.new(alt_core, alt_manifest))
local alt_outbound = alt_manifest.address + alt_manifest.offsets.game_to_network
alt_core:write16(alt_outbound + alt_manifest.queue.read_index, 0)
alt_core:write16(alt_outbound + alt_manifest.queue.write_index, 9)
local alt_bad, alt_bad_error, alt_bad_recovered = alt_bridge:peek_outbound()
assert(not alt_bad)
assert(alt_bad_error:match("impossible occupancy"))
assert(alt_bad_recovered)
assert(alt_core:read16(alt_outbound + alt_manifest.queue.read_index) == 9)
local alt_slot0 = alt_outbound + alt_manifest.queue.entries
for index = 1, #ready do
  alt_core:write8(alt_slot0 + index - 1, string.byte(ready, index))
end
alt_core:write16(alt_outbound + alt_manifest.queue.read_index, 0)
alt_core:write16(alt_outbound + alt_manifest.queue.write_index, 8)
local alt_full, alt_full_error = alt_bridge:peek_outbound()
assert(alt_full, alt_full_error)
assert(alt_full.bytes == ready)
assert(alt_bridge:commit_outbound(alt_full.read_index))
assert(alt_core:read16(alt_outbound + alt_manifest.queue.read_index) == 1)
local alt_inbound = alt_manifest.address + alt_manifest.offsets.network_to_game
alt_core:write16(alt_inbound + alt_manifest.queue.read_index, 0)
alt_core:write16(alt_inbound + alt_manifest.queue.write_index, 8)
local alt_push_full, alt_push_full_error = alt_bridge:push_inbound(session_ready)
assert(not alt_push_full)
assert(alt_push_full_error:match("full"))
alt_core:write16(alt_inbound + alt_manifest.queue.read_index, 0)
alt_core:write16(alt_inbound + alt_manifest.queue.write_index, 9)
local alt_push_bad, alt_push_bad_error, alt_push_recovered = alt_bridge:push_inbound(session_ready)
assert(not alt_push_bad)
assert(alt_push_bad_error:match("impossible occupancy"))
assert(alt_push_recovered)
assert(alt_core:read16(alt_inbound + alt_manifest.queue.write_index) == 0)

-- Stock mGBA exposes emu as userdata, rather than our table test double.
local native_core = assert(io.tmpfile())
local native_metatable = debug.getmetatable(native_core)
debug.setmetatable(native_core, { __index = function(_, name)
  return function(_, ...) return core[name](core, ...) end
end })
local native_bridge, native_error = memory_module.new(native_core, manifest)
debug.setmetatable(native_core, native_metatable)
native_core:close()
assert(native_bridge, native_error)

print("bridge memory tests passed")
