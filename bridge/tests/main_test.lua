package.path = "bridge/?.lua;" .. package.path

local protocol = require("protocol")
local original_dofile = dofile
local original_io_open = io.open
local original_memory_module = package.loaded.memory

local frame_callback
local savedata_callback
local receive_callback
local send_calls = {}
local incoming_chunks = {}
local warning_messages = {}
local error_messages = {}
local push_attempts = 0
local outbound_message
local outbound_commits = 0
local generation = 9
local state_capture_count = 0
local temporary_base = os.tmpname()
os.remove(temporary_base)
local character_save_path = temporary_base .. ".character.sav"
local resume_input_path = temporary_base .. ".resume.input.ss1"
local resume_output_path = temporary_base .. ".resume.ss1"
local use_valid_session = false
local manifest_schema = 1

local bridge = {}

function bridge:push_inbound(bytes)
  push_attempts = push_attempts + 1
  if push_attempts == 1 then
    return nil, "bridge queue counters describe an impossible occupancy", true
  end
  assert(protocol.decode(bytes, "inbound"))
  return true
end

function bridge:peek_outbound()
  return outbound_message
end

function bridge:commit_outbound(expected_read_index)
  assert(outbound_message)
  assert(expected_read_index == outbound_message.read_index)
  outbound_message = nil
  outbound_commits = outbound_commits + 1
  return true
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
  if #bytes == protocol.MESSAGE_SIZE then
    local decoded = assert(protocol.decode(bytes, "outbound"))
    if decoded.type == protocol.types.SAVE_DATA_UPDATED then
      assert(state_capture_count == outbound_commits + 1)
    end
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
    if event == "frame" then
      frame_callback = callback
    elseif event == "savedataUpdated" then
      savedata_callback = callback
    else
      error("unexpected callback " .. tostring(event))
    end
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

local bound_save
local loaded_state
local reset_count = 0
emu = {
  loadSaveFile = function(_, path, temporary)
    bound_save = path
    assert(temporary == false)
    return true
  end,
  reset = function()
    reset_count = reset_count + 1
  end,
  loadStateFile = function(_, path, flags)
    loaded_state = { path = path, flags = flags }
    return false
  end,
  saveStateFile = function(_, path, flags)
    state_capture_count = state_capture_count + 1
    assert(path == resume_output_path)
    assert(flags == 29)
    if state_capture_count == 1 then return true end
    local partial = assert(original_io_open(path, "wb"))
    partial:write("partial-state")
    partial:close()
    return false
  end,
  read32 = function()
    return generation
  end,
}

io.open = function(path, mode)
  if path == resume_input_path then
    assert(mode == "rb")
    return { close = function() end }
  end
  return original_io_open(path, mode)
end

dofile = function(path)
  if path:match("generated_addresses%.lua$") then
    return {
      schema_version = manifest_schema,
      save = {
        block3_address = 0x02001000,
        coop_offset = 4,
        generation_offset = 28,
        generation_address = 0x02001020,
        crc_offset = 668,
        schema_version = 1,
        struct_size = 672,
        registry_version = 1,
        registry_digest = "0123456789abcdef0123456789abcdef",
      },
    }
  end
  if path:match("session%.lua$") then
    local value = {
      host = "127.0.0.1",
      port = 12345,
      secret = "0123456789abcdef0123456789abcdef",
    }
    if use_valid_session then
      value.character_save = character_save_path
      value.resume_input = resume_input_path
      value.resume_output = resume_output_path
    end
    return value
  end
  return original_dofile(path)
end

local rejected, rejection_error = pcall(original_dofile, "bridge/main.lua")
assert(not rejected)
assert(tostring(rejection_error):match("launcher%-generated loopback session"))

use_valid_session = true
local stale_manifest_ok, stale_manifest_error = pcall(original_dofile, "bridge/main.lua")
assert(not stale_manifest_ok)
assert(tostring(stale_manifest_error):match("compatible co%-op save schema"))

manifest_schema = 2
local loaded, load_error = pcall(original_dofile, "bridge/main.lua")
dofile = original_dofile
io.open = original_io_open
package.loaded.memory = original_memory_module
assert(loaded, load_error)
assert(frame_callback)
assert(savedata_callback)
assert(receive_callback)
assert(bound_save == character_save_path)
assert(loaded_state.path == resume_input_path)
assert(loaded_state.flags == 29)
assert(reset_count == 1)
assert(#warning_messages == 1)
assert(warning_messages[1]:match("resume state was rejected"))

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
assert(#warning_messages == 2)
assert(warning_messages[2]:match("impossible occupancy"))

frame_callback()
assert(push_attempts == 2)
assert(#error_messages == 0)

-- A savedata callback before a grant cannot authorize a completion frame.
savedata_callback()
assert(state_capture_count == 0)

generation = 10
local checkpoint_granted = assert(protocol.encode({
  type = protocol.types.CHECKPOINT_GRANTED,
  sequence = 2,
  session_epoch = 7,
}))
incoming_chunks[1] = checkpoint_granted
receive_callback()
frame_callback()
assert(push_attempts == 3)

-- A post-grant callback at the baseline generation is still stale.
savedata_callback()
assert(state_capture_count == 0)

generation = 11
local save_data_updated = assert(protocol.encode({
  type = protocol.types.SAVE_DATA_UPDATED,
  sequence = 2,
  session_epoch = 7,
  payload = string.pack("<I4", generation),
}))
outbound_message = {
  bytes = save_data_updated,
  decoded = assert(protocol.decode(save_data_updated, "outbound")),
  read_index = 4,
}

frame_callback()
assert(state_capture_count == 0)
assert(#send_calls == 3)
assert(outbound_commits == 0)

savedata_callback()
assert(state_capture_count == 1)
assert(#send_calls == 3)

frame_callback()
assert(#send_calls == 4)
assert(send_calls[4].bytes == save_data_updated)
assert(outbound_commits == 1)

-- A wrapping u32 generation is newer, and failed optional state capture still
-- forwards the canonical SAV completion only after the attempt returns.
generation = 0xFFFFFFFF
local wrap_grant = assert(protocol.encode({
  type = protocol.types.CHECKPOINT_GRANTED,
  sequence = 3,
  session_epoch = 7,
}))
incoming_chunks[1] = wrap_grant
receive_callback()
frame_callback()
generation = 0
local wrapped_update = assert(protocol.encode({
  type = protocol.types.SAVE_DATA_UPDATED,
  sequence = 3,
  session_epoch = 7,
  payload = string.pack("<I4", generation),
}))
outbound_message = {
  bytes = wrapped_update,
  decoded = assert(protocol.decode(wrapped_update, "outbound")),
  read_index = 5,
}
frame_callback()
assert(#send_calls == 4)
savedata_callback()
assert(state_capture_count == 2)
assert(#warning_messages == 3)
assert(warning_messages[3]:match("state capture failed"))
assert(original_io_open(resume_output_path, "rb") == nil)
frame_callback()
assert(#send_calls == 5)
assert(send_calls[5].bytes == wrapped_update)
assert(outbound_commits == 2)

-- Legacy empty completion payloads fail closed and are never sent.
local next_grant = assert(protocol.encode({
  type = protocol.types.CHECKPOINT_GRANTED,
  sequence = 4,
  session_epoch = 7,
}))
incoming_chunks[1] = next_grant
receive_callback()
frame_callback()
generation = 12
local malformed_update = assert(protocol.encode({
  type = protocol.types.SAVE_DATA_UPDATED,
  sequence = 4,
  session_epoch = 7,
}))
outbound_message = {
  bytes = malformed_update,
  decoded = assert(protocol.decode(malformed_update, "outbound")),
  read_index = 6,
}
local malformed_ok, malformed_error = pcall(frame_callback)
assert(not malformed_ok)
assert(tostring(malformed_error):match("must carry one little%-endian u32 generation"))
assert(#send_calls == 5)
assert(outbound_commits == 2)

print("bridge main-loop tests passed")
