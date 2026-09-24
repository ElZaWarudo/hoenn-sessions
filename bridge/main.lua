local source = debug.getinfo(1, "S").source
if string.sub(source, 1, 1) == "@" then source = string.sub(source, 2) end
local script_directory = string.match(source, "^(.*[\\/])") or "./"
package.path = script_directory .. "?.lua;" .. package.path

local protocol = require("protocol")
local memory_module = require("memory")
local manifest = dofile(script_directory .. "generated_addresses.lua")
local session = dofile(script_directory .. "session.lua")

local SAVESTATE_WITHOUT_SAVEDATA = 29
local EWRAM_START = 0x02000000
local EWRAM_END = 0x02040000

local function is_integer(value)
  return type(value) == "number" and math.type(value) == "integer"
end

local function validate_save_manifest(value)
  if manifest.schema_version ~= 4 then
    return nil, "generated bridge address projection schema is incompatible"
  end
  if type(value) ~= "table"
    or not is_integer(value.block3_address)
    or value.block3_address < EWRAM_START
    or value.block3_address >= EWRAM_END
    or value.block3_address % 4 ~= 0
    or value.coop_offset ~= 4
    or value.generation_offset ~= 28
    or value.crc_offset ~= 668
    or value.schema_version ~= 1
    or value.struct_size ~= 672
    or value.registry_version ~= 1
    or type(value.registry_digest) ~= "string"
    or #value.registry_digest ~= 32
    or value.registry_digest:match("^[0-9a-f]+$") == nil then
    return nil, "generated bridge manifest has no compatible co-op save schema"
  end

  local generation_address = value.block3_address
    + value.coop_offset
    + value.generation_offset
  if not is_integer(value.generation_address)
    or value.generation_address ~= generation_address
    or generation_address < EWRAM_START
    or generation_address + 4 > EWRAM_END
    or value.block3_address + value.coop_offset + value.struct_size > EWRAM_END then
    return nil, "generated bridge manifest has an invalid save generation address"
  end
  return value
end

local function valid_absolute_path(value)
  if type(value) ~= "string"
    or #value == 0
    or #value > 32767
    or value:find("\0", 1, true)
    or value:find("\r", 1, true)
    or value:find("\n", 1, true) then
    return false
  end
  return value:sub(1, 1) == "/"
    or value:match("^[A-Za-z]:[\\/]") ~= nil
    or value:match("^[\\/][\\/]") ~= nil
end

local function valid_session(value)
  return type(value) == "table"
    and value.host == "127.0.0.1"
    and math.type(value.port) == "integer"
    and value.port > 0 and value.port <= 65535
    and type(value.secret) == "string"
    and #value.secret == 32
    and value.secret:match("^[0-9a-f]+$") ~= nil
    and valid_absolute_path(value.character_save)
    and valid_absolute_path(value.resume_input)
    and valid_absolute_path(value.resume_output)
    and value.character_save ~= value.resume_input
    and value.character_save ~= value.resume_output
    and value.resume_input ~= value.resume_output
end

if not valid_session(session) then
  error("bridge/session.lua must contain a launcher-generated loopback session")
end

local save_manifest, save_manifest_error = validate_save_manifest(manifest.save)
if not save_manifest then error(save_manifest_error) end

local character_save_path = session.character_save
local resume_input_path = session.resume_input
local resume_output_path = session.resume_output
-- The capture publishes through a temporary sibling so a crash mid-write
-- can never leave a partial resume state behind. The temporary path is
-- derived, so it stays distinct from the three launcher-owned paths above.
local resume_output_tmp_path = resume_output_path .. ".tmp"

local save_bound, save_bind_result = pcall(
  emu.loadSaveFile, emu, character_save_path, false)
if not save_bound or save_bind_result ~= true then
  error("mGBA could not bind the canonical character.sav")
end

local function readable_file_exists(path)
  local file = io.open(path, "rb")
  if not file then return false end
  file:close()
  return true
end

local restored_state = false
if readable_file_exists(resume_input_path) then
  local state_called, state_loaded = pcall(
    emu.loadStateFile, emu, resume_input_path, SAVESTATE_WITHOUT_SAVEDATA)
  restored_state = state_called and state_loaded == true
  if not restored_state then
    console:warn("compatible resume state was rejected; restarting from character.sav")
  end
end
if not restored_state then
  -- The SAV mapping survives reset. Resetting discards any game RAM that may
  -- have been populated from mGBA's default sibling save before manual load.
  local reset_called = pcall(emu.reset, emu)
  if not reset_called then error("mGBA could not reset for canonical SAV fallback") end
end

-- reset() returns before AgbMain initializes the RAM ABI. Validate on frame
-- callbacks, before any queue access, with a finite startup allowance.
local bridge
local initialization_frames = 0
local MAX_INITIALIZATION_FRAMES = 300

local function is_newer_u32(candidate, baseline)
  local distance = (candidate - baseline) & 0xFFFFFFFF
  return distance ~= 0 and distance < 0x80000000
end

local REMOVE_ATTEMPTS = 3

local function remove_readable_file(path)
  local file = io.open(path, "rb")
  if not file then return true end
  file:close()
  local remove_error
  for _ = 1, REMOVE_ATTEMPTS do
    local removed, operation_error = os.remove(path)
    if removed then return true end
    remove_error = operation_error
    local probe = io.open(path, "rb")
    if not probe then return true end
    probe:close()
  end
  return nil, "could not remove stale compatible state: " .. tostring(remove_error)
end

-- A previous crash may have left a partial atomic capture behind. The next
-- capture removes it again before writing, so this boot cleanup warns and
-- continues instead of failing the bridge.
local stale_tmp_removed, stale_tmp_error = remove_readable_file(resume_output_tmp_path)
if not stale_tmp_removed then
  console:warn("could not remove stale temporary state: " .. tostring(stale_tmp_error))
end

local checkpoint = {
  active_epoch = nil,
  callback_serial = 0,
  grant = nil,
  frame = nil,
  ready = false,
}

-- The ROM queue remains the owner of a critical completion frame until all
-- three proofs agree: a current grant, a later mGBA flush callback, and the
-- exact live/payload generation. Only then may TCP transmission begin.

function checkpoint:read_generation()
  local generation = emu:read32(save_manifest.generation_address)
  if not is_integer(generation) or generation < 0 or generation > 0xFFFFFFFF then
    return nil, "mGBA returned an invalid co-op save generation"
  end
  return generation
end

function checkpoint:observe_inbound(message)
  if message.type == protocol.types.SESSION_READY then
    if message.session_epoch == 0 then
      return nil, "SESSION_READY cannot install epoch zero"
    end
    if self.grant then
      return nil, "session epoch changed during an authorized checkpoint"
    end
    if self.active_epoch
      and message.session_epoch ~= self.active_epoch
      and message.session_epoch < self.active_epoch then
      return nil, "SESSION_READY attempted to move the bridge epoch backwards"
    end
    self.active_epoch = message.session_epoch
    return true
  end

  if message.type ~= protocol.types.CHECKPOINT_GRANTED then return true end
  if message.payload ~= "" then
    return nil, "CHECKPOINT_GRANTED must have an empty payload"
  end
  if not self.active_epoch or message.session_epoch ~= self.active_epoch then
    return nil, "CHECKPOINT_GRANTED does not match the active session epoch"
  end
  if self.grant then
    return nil, "a second checkpoint grant arrived before completion"
  end
  local generation, generation_error = self:read_generation()
  if not generation then return nil, generation_error end
  self.grant = {
    epoch = message.session_epoch,
    sequence = message.sequence,
    baseline_generation = generation,
    callback_serial = self.callback_serial,
    callback_generation = nil,
  }
  self.frame = nil
  self.ready = false
  return true
end

function checkpoint:observe_savedata_updated()
  self.callback_serial = self.callback_serial + 1
  if not self.grant then return true end
  local generation, generation_error = self:read_generation()
  if not generation then return nil, generation_error end
  self.grant.callback_serial_seen = self.callback_serial
  self.grant.callback_generation = generation
  return self:try_capture()
end

function checkpoint:try_capture()
  if self.ready or not self.grant or not self.frame then return true end
  if not self.grant.callback_serial_seen
    or self.grant.callback_serial_seen <= self.grant.callback_serial then
    return true
  end

  local target = self.frame.generation
  local callback_generation = self.grant.callback_generation
  if callback_generation ~= target then
    if is_newer_u32(callback_generation, target) then
      return nil, "save generation advanced past the checkpoint before state capture"
    end
    return true
  end

  local live_generation, generation_error = self:read_generation()
  if not live_generation then return nil, generation_error end
  if live_generation ~= target then
    if is_newer_u32(live_generation, target) then
      return nil, "live save generation advanced past the checkpoint before forwarding"
    end
    return true
  end

  local removed, remove_error = remove_readable_file(resume_output_path)
  if not removed then
    console:warn("could not remove previous compatible state: " .. tostring(remove_error)
      .. "; checkpoint will use character.sav only")
    self.ready = true
    return true
  end
  local temporary_removed, temporary_error = remove_readable_file(resume_output_tmp_path)
  if not temporary_removed then
    console:warn("could not remove temporary compatible state: " .. tostring(temporary_error)
      .. "; checkpoint will use character.sav only")
    self.ready = true
    return true
  end
  local capture_called, capture_result = pcall(
    emu.saveStateFile, emu, resume_output_tmp_path, SAVESTATE_WITHOUT_SAVEDATA)
  if not capture_called or capture_result ~= true then
    local cleaned, cleanup_error = remove_readable_file(resume_output_tmp_path)
    if not cleaned then
      console:warn("could not remove partial compatible state: " .. tostring(cleanup_error)
        .. "; checkpoint will use character.sav only")
    else
      console:warn("compatible state capture failed; checkpoint will use character.sav only")
    end
  else
    -- Publish atomically: the resume path is either absent or a complete
    -- state, never a partial write. The previous state was already removed
    -- above because os.rename cannot replace an existing file on Windows.
    local renamed, rename_error = os.rename(resume_output_tmp_path, resume_output_path)
    if not renamed then
      local cleaned, cleanup_error = remove_readable_file(resume_output_tmp_path)
      if not cleaned then
        console:warn("could not remove partial compatible state: " .. tostring(cleanup_error)
          .. "; checkpoint will use character.sav only")
      else
        console:warn("compatible state publish failed; checkpoint will use character.sav only")
      end
    end
  end
  self.ready = true
  return true
end

function checkpoint:allow_outbound(message)
  if message.type ~= protocol.types.SAVE_DATA_UPDATED then return true end
  if not self.grant then
    return nil, "SAVE_DATA_UPDATED arrived without a current checkpoint grant"
  end
  if message.session_epoch ~= self.grant.epoch then
    return nil, "SAVE_DATA_UPDATED does not match the granted session epoch"
  end
  if #message.payload ~= 4 then
    return nil, "SAVE_DATA_UPDATED must carry one little-endian u32 generation"
  end
  local generation = string.unpack("<I4", message.payload)
  if not is_newer_u32(generation, self.grant.baseline_generation) then
    return nil, "SAVE_DATA_UPDATED did not advance the granted save generation"
  end

  if self.frame then
    if self.frame.sequence ~= message.sequence or self.frame.generation ~= generation then
      return nil, "the pending SAVE_DATA_UPDATED frame changed before forwarding"
    end
  else
    self.frame = { sequence = message.sequence, generation = generation }
  end
  local captured, capture_error = self:try_capture()
  if not captured then return nil, capture_error end
  return self.ready
end

function checkpoint:validate_pending(message)
  if message.type ~= protocol.types.SAVE_DATA_UPDATED then return true end
  if not self.ready or not self.frame or self.frame.sequence ~= message.sequence then
    return nil, "SAVE_DATA_UPDATED lost its completed state-capture proof"
  end
  local live_generation, generation_error = self:read_generation()
  if not live_generation then return nil, generation_error end
  if live_generation ~= self.frame.generation then
    return nil, "save generation changed while forwarding SAVE_DATA_UPDATED"
  end
  return true
end

function checkpoint:commit_outbound(message)
  if message.type ~= protocol.types.SAVE_DATA_UPDATED then return true end
  local valid, validation_error = self:validate_pending(message)
  if not valid then return nil, validation_error end
  self.grant = nil
  self.frame = nil
  self.ready = false
  return true
end

local handshake = string.format(
  '{"secret":"%s","bridge_abi":%d,"protocol_version":%d}\n',
  session.secret, protocol.ABI_VERSION, protocol.PROTOCOL_VERSION)
if #handshake > 256 then error("internal handshake exceeds its protocol bound") end

local client = nil
local receive_buffer = ""
local authenticated = false
local pending_handshake = { bytes = handshake, offset = 1 }
local pending_outbound = nil
local frame_counter = 0
local reconnect_frames = 0
local reconnect_attempts = 0
local handshake_frames = 0
local MAX_RECEIVE_BUFFER_BYTES = protocol.MESSAGE_SIZE * manifest.queue.capacity

local function schedule_reconnect(message)
  if authenticated and ((pending_outbound and pending_outbound.offset > 1)
    or #receive_buffer > 0) then
    error("sidecar connection lost with a frame in flight")
  end
  console:warn(message)
  local previous = client
  client = nil
  if previous then pcall(previous.close, previous) end
  receive_buffer = ""
  authenticated = false
  pending_handshake = { bytes = handshake, offset = 1 }
  handshake_frames = 0
  -- The sidecar retains its checkpoint grant across an idle bridge socket
  -- replacement. An unsent ROM frame remains owned by its queue.
  if pending_outbound then pending_outbound.offset = 1 end
  reconnect_frames = math.min(60 * (2 ^ math.min(reconnect_attempts, 4)), 600)
  reconnect_attempts = reconnect_attempts + 1
end

local function connect_sidecar()
  local connection, connect_error = socket.connect(session.host, session.port)
  if not connection then
    schedule_reconnect("sidecar connection failed: " .. tostring(connect_error))
    return
  end
  client = connection
  handshake_frames = 0
  connection:add("received", function()
    if client ~= connection then return end
    while connection:hasdata() do
      local buffer_limit = authenticated and MAX_RECEIVE_BUFFER_BYTES or 256
      local available = buffer_limit - #receive_buffer
      if available <= 0 then return end
      local bytes, receive_error = connection:receive(math.min(4096, available))
      if not bytes then
        schedule_reconnect("sidecar receive failed: " .. tostring(receive_error))
        return
      end
      receive_buffer = receive_buffer .. bytes
    end
  end)
  connection:add("error", function(error_value)
    if client == connection then
      schedule_reconnect("sidecar socket error: " .. tostring(error_value))
    end
  end)
end

connect_sidecar()

local function process_handshake_response()
  local newline = string.find(receive_buffer, "\n", 1, true)
  if not newline then
    if #receive_buffer >= 256 then error("sidecar response exceeds handshake bound") end
    return
  end
  local response = string.sub(receive_buffer, 1, newline)
  receive_buffer = string.sub(receive_buffer, newline + 1)
  if response ~= '{"ok":true}\n' then error("sidecar rejected the bridge handshake") end
  authenticated = true
  reconnect_attempts = 0
  console:log("PokéCrossroads co-op bridge authenticated with the local sidecar")
end

local function push_inbound_frames()
  while #receive_buffer >= protocol.MESSAGE_SIZE do
    local bytes = string.sub(receive_buffer, 1, protocol.MESSAGE_SIZE)
    local decoded, decode_error = protocol.decode(bytes, "inbound")
    if not decoded then error("invalid sidecar bridge frame: " .. tostring(decode_error)) end
    local pushed, push_error, queue_recovered = bridge:push_inbound(bytes)
    if not pushed then
      if push_error == "ROM inbound queue is full" then return end
      if queue_recovered then
        console:warn("reset corrupt ROM inbound queue: " .. tostring(push_error))
        return
      end
      error("invalid sidecar bridge frame: " .. tostring(push_error))
    end
    local observed, observe_error = checkpoint:observe_inbound(decoded)
    if not observed then error(observe_error) end
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
    schedule_reconnect("sidecar handshake send failed: " .. tostring(send_error))
    return
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
    local allowed, gate_error = checkpoint:allow_outbound(message.decoded)
    if allowed == nil then error(gate_error) end
    if not allowed then return end
    pending_outbound = {
      bytes = message.bytes,
      decoded = message.decoded,
      read_index = message.read_index,
      offset = 1,
    }
  end

  local valid, validation_error = checkpoint:validate_pending(pending_outbound.decoded)
  if not valid then error(validation_error) end
  local complete, send_error = advance_pending_send(pending_outbound)
  if complete == nil then
    schedule_reconnect("sidecar send failed: " .. tostring(send_error))
    return
  end
  if complete then
    local committed, commit_error = bridge:commit_outbound(pending_outbound.read_index)
    if not committed then error(commit_error) end
    local checkpoint_committed, checkpoint_error = checkpoint:commit_outbound(
      pending_outbound.decoded)
    if not checkpoint_committed then error(checkpoint_error) end
    pending_outbound = nil
  end
end

callbacks:add("savedataUpdated", function()
  local observed, observe_error = checkpoint:observe_savedata_updated()
  if not observed then error(observe_error) end
end)

callbacks:add("frame", function()
  if initialization_frames >= MAX_INITIALIZATION_FRAMES then return end
  if not bridge then
    local bridge_error
    bridge, bridge_error = memory_module.new(emu, manifest)
    if not bridge then
      initialization_frames = initialization_frames + 1
      if initialization_frames >= MAX_INITIALIZATION_FRAMES then
        client:close()
        error("ROM bridge initialization timed out: " .. tostring(bridge_error))
      end
      return
    end
  end
  frame_counter = (frame_counter + 1) & 0xFFFFFFFF
  if not client then
    if reconnect_frames > 0 then
      reconnect_frames = reconnect_frames - 1
      return
    end
    connect_sidecar()
    if not client then return end
  end
  client:poll()
  if not client then return end
  if pending_handshake then
    send_handshake()
    return
  end
  if not authenticated then
    handshake_frames = handshake_frames + 1
    if handshake_frames >= 300 then
      schedule_reconnect("sidecar handshake timed out")
      return
    end
    process_handshake_response()
    return
  end
  push_inbound_frames()
  send_outbound_frame()
  if frame_counter % 60 == 0 then bridge:heartbeat() end
end)

console:log("PokéCrossroads co-op bridge waiting for ROM initialization and authentication")
