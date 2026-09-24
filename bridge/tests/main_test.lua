package.path = "bridge/?.lua;" .. package.path

local protocol = require("protocol")
local original_dofile = dofile
local original_io_open = io.open
local original_memory_module = package.loaded.memory

local frame_callback
local savedata_callback
local receive_callback
local error_callback
local send_calls = {}
local incoming_chunks = {}
local warning_messages = {}
local error_messages = {}
local push_attempts = 0
local outbound_message
local outbound_commits = 0
local generation = 9
local state_capture_count = 0
local skipped_capture_count = 0
local temporary_base = os.tmpname()
os.remove(temporary_base)
local character_save_path = temporary_base .. ".character.sav"
local resume_input_path = temporary_base .. ".resume.input.ss1"
local resume_output_path = temporary_base .. ".resume.ss1"
local use_valid_session = false
local manifest_schema = 2
local save_schema = 1
local initialization_attempts = 0
local rom_initialized = false
local socket_connections = 0
local connect_failures = 0

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
    initialization_attempts = initialization_attempts + 1
    if not rom_initialized then return nil, "ROM has not initialized its ABI yet" end
    return bridge
  end,
}

local client = {}

function client:add(event, callback)
  if event == "received" then
    receive_callback = callback
  elseif event == "error" then
    error_callback = callback
  end
end

function client:poll()
end

local closed = false
function client:close()
  closed = true
end

function client:hasdata()
  return #incoming_chunks > 0
end

function client:receive(max_bytes)
  local chunk = table.remove(incoming_chunks, 1)
  if #chunk > max_bytes then
    table.insert(incoming_chunks, 1, string.sub(chunk, max_bytes + 1))
    return string.sub(chunk, 1, max_bytes)
  end
  return chunk
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
      assert(state_capture_count == outbound_commits - 1)
    end
  end
  return last
end

socket = {
  ERRORS = { AGAIN = "again" },
  connect = function(host, port)
    assert(host == "127.0.0.1")
    assert(port == 12345)
    socket_connections = socket_connections + 1
    if connect_failures > 0 then
      connect_failures = connect_failures - 1
      return nil, "temporarily unavailable"
    end
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
    assert(path == resume_output_path .. ".tmp")
    assert(flags == 29)
    if state_capture_count == 1 then
      local complete = assert(original_io_open(path, "wb"))
      complete:write("complete-state")
      complete:close()
      return true
    end
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
      queue = { capacity = 32 },
      save = {
        block3_address = 0x02001000,
        coop_offset = 4,
        generation_offset = 28,
        generation_address = 0x02001020,
        crc_offset = 668,
        schema_version = save_schema,
        struct_size = 672,
        registry_version = 2,
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
assert(tostring(stale_manifest_error):match("address projection schema"))

manifest_schema = 4
save_schema = 1
local stale_save_ok, stale_save_error = pcall(original_dofile, "bridge/main.lua")
assert(not stale_save_ok)
assert(tostring(stale_save_error):match("compatible co%-op save schema"))

save_schema = 2
-- A permanently missing/mismatched ABI fails once, closes the socket, and
-- leaves the ROM queues untouched, rather than retrying forever.
assert(pcall(original_dofile, "bridge/main.lua"))
for _ = 1, 299 do frame_callback() end
local startup_ok, startup_error = pcall(frame_callback)
assert(not startup_ok)
assert(tostring(startup_error):match("initialization timed out"))
assert(closed)
assert(#send_calls == 0)
frame_callback()
assert(initialization_attempts == 300)
initialization_attempts = 0
reset_count = 0
warning_messages = {}
closed = false
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

-- Reset returns before the ROM executes AgbMain. Script load must not reject
-- that fresh RAM or touch queues before a later frame initializes the ABI.
assert(initialization_attempts == 0)
frame_callback()
assert(initialization_attempts == 1)
assert(#send_calls == 0)
rom_initialized = true
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

-- The portal ID is forwarded in the authenticated epoch before the ordinary
-- checkpoint request; neither frame by itself authorizes a ROM switch.
local portal_request = assert(protocol.encode({
  type = protocol.types.PORTAL_TRAVEL_REQUEST,
  sequence = 8,
  session_epoch = 7,
  payload = "to_cormoria",
}))
outbound_message = {
  bytes = portal_request,
  decoded = assert(protocol.decode(portal_request, "outbound")),
  read_index = 2,
}
frame_callback()
assert(#send_calls == 4)
assert(send_calls[4].bytes == portal_request)
assert(outbound_commits == 1)
local portal_checkpoint_ready = assert(protocol.encode({
  type = protocol.types.CHECKPOINT_READY,
  sequence = 9,
  session_epoch = 7,
}))
outbound_message = {
  bytes = portal_checkpoint_ready,
  decoded = assert(protocol.decode(portal_checkpoint_ready, "outbound")),
  read_index = 3,
}
frame_callback()
assert(#send_calls == 5)
assert(send_calls[5].bytes == portal_checkpoint_ready)
assert(outbound_commits == 2)

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
assert(#send_calls == 5)
assert(outbound_commits == 2)

savedata_callback()
assert(state_capture_count == 1)
assert(#send_calls == 5)

frame_callback()
assert(#send_calls == 6)
assert(send_calls[6].bytes == save_data_updated)
assert(outbound_commits == 3)
-- The capture publishes atomically: the complete state lands at the resume
-- path and no temporary sibling lingers behind.
local published = assert(original_io_open(resume_output_path, "rb"))
assert(published:read("*a") == "complete-state")
published:close()
assert(original_io_open(resume_output_path .. ".tmp", "rb") == nil)

-- A later non-wrapping generation forwards only after optional state capture
-- has been attempted. The ROM now rejects generation overflow.
local wrap_grant = assert(protocol.encode({
  type = protocol.types.CHECKPOINT_GRANTED,
  sequence = 3,
  session_epoch = 7,
}))
incoming_chunks[1] = wrap_grant
receive_callback()
frame_callback()
generation = 12
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
assert(#send_calls == 6)
savedata_callback()
assert(state_capture_count == 2)
assert(#warning_messages == 3)
assert(warning_messages[3]:match("state capture failed"))
assert(original_io_open(resume_output_path, "rb") == nil)
frame_callback()
assert(#send_calls == 7)
assert(send_calls[7].bytes == wrapped_update)
assert(outbound_commits == 4)

-- Legacy empty completion payloads fail closed and are never sent.
local next_grant = assert(protocol.encode({
  type = protocol.types.CHECKPOINT_GRANTED,
  sequence = 4,
  session_epoch = 7,
}))
incoming_chunks[1] = next_grant
receive_callback()
frame_callback()
generation = 13
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
assert(#send_calls == 7)
assert(outbound_commits == 4)

-- A locked previous resume capture (Windows file lock) must not terminate
-- the bridge: the optional capture is skipped and the canonical SAV
-- completion still forwards. Transient remove failures are retried.
local original_os_remove = os.remove
local remove_calls = 0
local locked_remove = false
local transient_failures_remaining = 0
os.remove = function(path)
  if path == resume_output_path or path == resume_output_path .. ".tmp" then
    local probe = original_io_open(path, "rb")
    if probe then
      probe:close()
      remove_calls = remove_calls + 1
      if locked_remove then
        return nil, "Permission denied (locked)"
      end
      if transient_failures_remaining > 0 then
        transient_failures_remaining = transient_failures_remaining - 1
        return nil, "Permission denied (locked)"
      end
    end
  end
  return original_os_remove(path)
end

local stale_capture = assert(original_io_open(resume_output_path, "wb"))
stale_capture:write("stale-state")
stale_capture:close()
assert(original_io_open(resume_output_path .. ".tmp", "rb") == nil)

-- Complete the pending sequence-4 grant with a valid generation while the
-- previous capture is locked. The bridge must warn, skip the optional
-- capture, and still forward the SAV completion.
locked_remove = true
remove_calls = 0
local warnings_before_lock = #warning_messages
local sends_before_lock = #send_calls
local commits_before_lock = outbound_commits
local captures_before_lock = state_capture_count
generation = 12
local locked_update = assert(protocol.encode({
  type = protocol.types.SAVE_DATA_UPDATED,
  sequence = 4,
  session_epoch = 7,
  payload = string.pack("<I4", generation),
}))
outbound_message = {
  bytes = locked_update,
  decoded = assert(protocol.decode(locked_update, "outbound")),
  read_index = 6,
}
savedata_callback()
skipped_capture_count = skipped_capture_count + 1
local lock_frame_ok, lock_frame_error = pcall(frame_callback)
assert(lock_frame_ok, lock_frame_error)
assert(state_capture_count == captures_before_lock)
assert(#send_calls == sends_before_lock + 1)
assert(send_calls[#send_calls].bytes == locked_update)
assert(outbound_commits == commits_before_lock + 1)
assert(#warning_messages == warnings_before_lock + 1)
assert(warning_messages[#warning_messages]:match("could not remove previous compatible state"))
assert(warning_messages[#warning_messages]:match("character%.sav only"))
assert(remove_calls >= 1 and remove_calls <= 3)
local surviving = assert(original_io_open(resume_output_path, "rb"))
assert(surviving:read("*a") == "stale-state")
surviving:close()
locked_remove = false

-- A transient lock that clears on retry still permits the optional capture
-- attempt and forwards the canonical completion.
transient_failures_remaining = 2
remove_calls = 0
local transient_grant = assert(protocol.encode({
  type = protocol.types.CHECKPOINT_GRANTED,
  sequence = 5,
  session_epoch = 7,
}))
incoming_chunks[1] = transient_grant
receive_callback()
frame_callback()
generation = 13
local transient_update = assert(protocol.encode({
  type = protocol.types.SAVE_DATA_UPDATED,
  sequence = 5,
  session_epoch = 7,
  payload = string.pack("<I4", generation),
}))
outbound_message = {
  bytes = transient_update,
  decoded = assert(protocol.decode(transient_update, "outbound")),
  read_index = 7,
}
local transient_captures_before = state_capture_count
local transient_sends_before = #send_calls
local transient_commits_before = outbound_commits
savedata_callback()
frame_callback()
assert(remove_calls >= 3)
assert(state_capture_count == transient_captures_before + 1)
assert(#send_calls == transient_sends_before + 1)
assert(send_calls[#send_calls].bytes == transient_update)
assert(outbound_commits == transient_commits_before + 1)
os.remove = original_os_remove
original_os_remove(resume_output_path)
original_os_remove(resume_output_path .. ".tmp")

-- Backpressure bounds Lua memory while the ROM cannot drain its inbound queue,
-- as happens transiently during map loads and is amplified by fast-forward.
incoming_chunks[1] = string.rep(session_ready, 33)
receive_callback()
assert(#incoming_chunks == 1)
assert(#incoming_chunks[1] == protocol.MESSAGE_SIZE)
frame_callback()
receive_callback()
frame_callback()

-- A socket error retries with bounded frame backoff and authenticates the
-- replacement bridge without losing an uncommitted ROM queue frame.
incoming_chunks = {}
local connects_before = socket_connections
local sends_before = #send_calls
error_callback("connection reset")
assert(closed)
for _ = 1, 60 do frame_callback() end
assert(socket_connections == connects_before)
frame_callback()
assert(socket_connections == connects_before + 1)
assert(#send_calls == sends_before + 1)
assert(send_calls[#send_calls].bytes:match('^%{"secret"'))
incoming_chunks[1] = '{"ok":true}\n'
receive_callback()
frame_callback()
assert(#error_messages == 0)

-- Failed replacement attempts also back off and leave the frame loop alive.
connect_failures = 1
error_callback("connection reset again")
local attempted_before = socket_connections
for _ = 1, 60 do frame_callback() end
frame_callback()
assert(socket_connections == attempted_before + 1)
for _ = 1, 120 do frame_callback() end
assert(socket_connections == attempted_before + 1)
frame_callback()
assert(socket_connections == attempted_before + 2)
assert(send_calls[#send_calls].bytes:match('^%{"secret"'))
closed = false
local warnings_before_timeout = #warning_messages
for _ = 1, 300 do frame_callback() end
assert(closed)
assert(#warning_messages == warnings_before_timeout + 1)
assert(warning_messages[#warning_messages]:match("handshake timed out"))

-- The sidecar treats partial frame loss as fatal. The bridge must not replay
-- an ambiguous fragment after reconnecting.
for _ = 1, 240 do frame_callback() end
frame_callback()
incoming_chunks[1] = '{"ok":true}\n'
receive_callback()
frame_callback()
incoming_chunks[1] = string.sub(session_ready, 1, 5)
receive_callback()
local partial_ok, partial_error = pcall(error_callback, "connection reset mid-frame")
assert(not partial_ok)
assert(tostring(partial_error):match("frame in flight"))

print("bridge main-loop tests passed")
