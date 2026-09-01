local protocol = require("protocol")

local memory = {}
memory.__index = memory

local function queue_depth(read_index, write_index)
  local depth = (write_index - read_index) & 0xFFFF
  if depth > 32 then
    return nil, "bridge queue counters describe an impossible occupancy"
  end
  return depth
end

function memory.new(core, manifest)
  if type(core) ~= "table" or type(manifest) ~= "table" then
    return nil, "core and generated manifest are required"
  end
  if manifest.address < 0x02000000
    or manifest.address + manifest.size > 0x02040000
    or core:read32(manifest.address) ~= manifest.magic
    or core:read16(manifest.address + 4) ~= manifest.abi_version
    or core:read16(manifest.address + 6) ~= manifest.protocol_version
    or core:read32(manifest.address + 8) ~= manifest.game_build_id then
    return nil, "loaded ROM does not match the generated bridge manifest"
  end
  return setmetatable({ core = core, manifest = manifest }, memory)
end

function memory:_queue_address(direction)
  if direction == "outbound" then
    return self.manifest.address + self.manifest.offsets.game_to_network
  end
  if direction == "inbound" then
    return self.manifest.address + self.manifest.offsets.network_to_game
  end
  error("unknown bridge queue direction")
end

function memory:peek_outbound()
  local queue_address = self:_queue_address("outbound")
  local read_index = self.core:read16(queue_address + self.manifest.queue.read_index)
  local write_index = self.core:read16(queue_address + self.manifest.queue.write_index)
  local depth, err = queue_depth(read_index, write_index)
  if not depth then
    -- Lua owns the outbound consumer index. Discard every ambiguous slot and
    -- converge on the producer's published counter instead of retrying forever.
    self.core:write16(queue_address + self.manifest.queue.read_index, write_index)
    return nil, err, true
  end
  if depth == 0 then return nil end

  local slot = read_index & (self.manifest.queue.capacity - 1)
  local message_address = queue_address + self.manifest.queue.entries
    + slot * self.manifest.message.size
  local bytes = self.core:readRange(message_address, self.manifest.message.size)
  local decoded, decode_error = protocol.decode(bytes, "outbound")
  if not decoded then
    -- A published corrupt slot cannot repair itself. Discard it so one bad
    -- host boundary record cannot wedge all later critical traffic.
    self.core:write16(queue_address + self.manifest.queue.read_index, (read_index + 1) & 0xFFFF)
    return nil, decode_error
  end
  return { bytes = bytes, decoded = decoded, read_index = read_index }
end

function memory:commit_outbound(expected_read_index)
  local queue_address = self:_queue_address("outbound")
  local current = self.core:read16(queue_address + self.manifest.queue.read_index)
  if current ~= expected_read_index then
    return nil, "outbound queue consumer index changed unexpectedly"
  end
  self.core:write16(queue_address + self.manifest.queue.read_index, (current + 1) & 0xFFFF)
  return true
end

function memory:push_inbound(bytes)
  local decoded, err = protocol.decode(bytes, "inbound")
  if not decoded then return nil, err end

  local queue_address = self:_queue_address("inbound")
  local read_index = self.core:read16(queue_address + self.manifest.queue.read_index)
  local write_index = self.core:read16(queue_address + self.manifest.queue.write_index)
  local depth
  depth, err = queue_depth(read_index, write_index)
  if not depth then
    -- Lua owns the inbound producer index. Rewind it to the ROM consumer's
    -- counter so the rejected frame can be retried against an empty queue.
    self.core:write16(queue_address + self.manifest.queue.write_index, read_index)
    return nil, err, true
  end
  if depth == self.manifest.queue.capacity then
    return nil, "ROM inbound queue is full"
  end

  local slot = write_index & (self.manifest.queue.capacity - 1)
  local message_address = queue_address + self.manifest.queue.entries
    + slot * self.manifest.message.size
  for index = 1, #bytes do
    self.core:write8(message_address + index - 1, string.byte(bytes, index))
  end
  -- Publish the producer counter last. mGBA invokes this script while the
  -- emulated CPU is paused, matching the ROM-side compiler-barrier contract.
  self.core:write16(queue_address + self.manifest.queue.write_index, (write_index + 1) & 0xFFFF)
  return true
end

function memory:heartbeat()
  local address = self.manifest.address + self.manifest.offsets.last_sidecar_heartbeat
  self.core:write32(address, (self.core:read32(address) + 1) & 0xFFFFFFFF)
end

return memory
