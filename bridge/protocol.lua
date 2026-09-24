local protocol = {}

protocol.MESSAGE_SIZE = 144
protocol.PAYLOAD_SIZE = 128
protocol.ABI_VERSION = 1
protocol.PROTOCOL_VERSION = 1

protocol.types = {
  ROM_READY = 0x0001,
  PLAYER_STATE = 0x0002,
  INTERACT_REMOTE_PLAYER = 0x0003,
  GROUP_INVITE_REQUEST = 0x0004,
  TRAINER_BATTLE_RESERVE = 0x0005,
  BATTLE_JOIN_RESPONSE = 0x0006,
  PARTY_SNAPSHOT = 0x0007,
  ACTION_INTENT = 0x0008,
  TURN_RESULT_HASH = 0x0009,
  BATTLE_FINISHED = 0x000A,
  COMMIT_APPLIED = 0x000B,
  CHECKPOINT_READY = 0x000C,
  SAVE_DATA_UPDATED = 0x000D,
  ONLINE_REQUEST = 0x000E,
  GROUP_TRAVEL_CLIENT = 0x000F,
  COMPANION_STATE = 0x0010,
  SOCIAL_SIGNAL = 0x0011,
  PORTAL_TRAVEL_REQUEST = 0x0012,
  ARRIVAL_PROOF = 0x0013,
  SESSION_READY = 0x0100,
  REMOTE_PLAYER_SPAWN = 0x0101,
  REMOTE_PLAYER_UPDATE = 0x0102,
  REMOTE_PLAYER_DESPAWN = 0x0103,
  GROUP_INVITE_RECEIVED = 0x0104,
  GROUP_STATE_CHANGED = 0x0105,
  BATTLE_JOIN_OFFER = 0x0106,
  BATTLE_MANIFEST = 0x0107,
  TURN_BUNDLE = 0x0108,
  PAUSE_FOR_RECONNECT = 0x0109,
  BATTLE_COMMIT = 0x010A,
  ABORT_BATTLE = 0x010B,
  CHECKPOINT_GRANTED = 0x010C,
  ONLINE_STATUS = 0x010D,
  GROUP_TRAVEL_SERVER = 0x010E,
  REMOTE_COMPANION = 0x010F,
  REMOTE_SOCIAL_SIGNAL = 0x0110,
  ARRIVAL_CHALLENGE = 0x0111,
}

local function is_integer(value)
  return type(value) == "number" and math.type(value) == "integer"
end

function protocol.is_outbound(message_type)
  return is_integer(message_type)
    and message_type >= protocol.types.ROM_READY
    and message_type <= protocol.types.ARRIVAL_PROOF
end

function protocol.is_inbound(message_type)
  return is_integer(message_type)
    and message_type >= protocol.types.SESSION_READY
    and message_type <= protocol.types.ARRIVAL_CHALLENGE
end

protocol.GROUP_TRAVEL_RECORD_SIZE = 32

local function all_zero(bytes)
  return bytes:find("[^\0]") == nil
end

local function route_era_destination(route)
  local values = {
    [1] = {1, 3}, [2] = {2, 4}, [3] = {1, 1},
    [4] = {2, 2}, [5] = {1, 5}, [6] = {2, 6},
  }
  local value = values[route]
  if not value then return nil end
  return value[1], value[2]
end

local function departure_matches_route(route, departure)
  if route == 1 or route == 2 then return departure == 1 end
  if route == 3 or route == 4 then return departure == 2 or departure == 3 end
  if route == 5 or route == 6 then return departure == 4 end
  return false
end

function protocol.decode_group_travel(payload, direction)
  if type(payload) ~= "string" or #payload ~= protocol.GROUP_TRAVEL_RECORD_SIZE then
    return nil, "group-travel record must contain exactly 32 bytes"
  end
  local kind, route, era, destination, result, reason, departure, reserved, request_id, proposal_id,
    tail = string.unpack("<I1I1I1I1I1I1I1I1I4c16c4", payload)
  if reserved ~= 0 or not all_zero(tail) then
    return nil, "group-travel reserved bytes must be zero"
  end
  local expected_era, expected_destination = route_era_destination(route)
  if not expected_era or era ~= expected_era or destination ~= expected_destination then
    return nil, "group-travel route, era, and destination disagree"
  end
  if not departure_matches_route(route, departure) then
    return nil, "group-travel departure does not match route"
  end
  if request_id == 0 or result > 3 or reason > 4 then
    return nil, "group-travel scalar is outside its wire range"
  end
  local proposal_zero = all_zero(proposal_id)
  local valid
  if direction == "outbound" then
    valid = (kind == 1 and proposal_zero and result == 0 and reason == 0)
      or (kind == 2 and not proposal_zero and (result == 1 or result == 2) and reason == 0)
      or (kind == 3 and result == 0 and reason == 2)
      or (kind == 4 and not proposal_zero and result == 3 and reason == 0)
  elseif direction == "inbound" then
    valid = (kind == 1 and proposal_zero and result == 0 and reason == 0)
      or ((kind == 2 or kind == 3) and not proposal_zero and result == 0 and reason == 0)
      or (kind == 4 and result == 0 and reason > 0
        and (not proposal_zero or reason == 3 or reason == 4))
      or (kind == 5 and not proposal_zero and result == 3 and reason == 0)
  else
    return nil, "group-travel direction is required"
  end
  if not valid then return nil, "invalid group-travel phase fields" end
  return { kind = kind, route = route, era = era, destination = destination,
    departure = departure,
    result = result, reason = reason, request_id = request_id, proposal_id = proposal_id }
end

function protocol.encode_group_travel(record, direction)
  if type(record) ~= "table" or type(record.proposal_id) ~= "string"
    or #record.proposal_id ~= 16 then
    return nil, "invalid group-travel record"
  end
  local era, destination = route_era_destination(record.route)
  if not era then return nil, "invalid group-travel route" end
  local payload = string.pack("<I1I1I1I1I1I1I1I1I4c16I4", record.kind, record.route,
    era, destination, record.result or 0, record.reason or 0, record.departure or 0, 0, record.request_id,
    record.proposal_id, 0)
  local decoded, err = protocol.decode_group_travel(payload, direction)
  if not decoded then return nil, err end
  return payload
end

function protocol.is_known(message_type)
  return protocol.is_outbound(message_type) or protocol.is_inbound(message_type)
end

function protocol.valid_portal_id(payload)
  return type(payload) == "string"
    and #payload >= 1 and #payload <= 96
    and payload:match("^[a-z][a-z0-9_]*$") ~= nil
end

function protocol.decode_arrival_proof(payload)
  if type(payload) ~= "string" or #payload ~= 64 then
    return nil, "arrival proof must contain exactly 64 bytes"
  end
  local nonce, digest, world_id, generation, map_group, map_num, reserved =
    string.unpack("<c16c32I4I4I1I1c6", payload)
  if all_zero(nonce) or all_zero(digest) or world_id == 0 or generation == 0
    or not all_zero(reserved) then
    return nil, "arrival proof contains invalid or reserved fields"
  end
  return {nonce = nonce, save_sha256 = digest, world_id = world_id,
    save_generation = generation, map_group = map_group, map_num = map_num}
end

function protocol.crc32(bytes)
  local crc = 0xFFFFFFFF
  for index = 1, #bytes do
    crc = (crc ~ string.byte(bytes, index)) & 0xFFFFFFFF
    for _ = 1, 8 do
      if (crc & 1) ~= 0 then
        crc = ((crc >> 1) ~ 0xEDB88320) & 0xFFFFFFFF
      else
        crc = (crc >> 1) & 0xFFFFFFFF
      end
    end
  end
  return (~crc) & 0xFFFFFFFF
end

local function validate_scalar(value, maximum, label)
  if not is_integer(value) or value < 0 or value > maximum then
    return nil, label .. " is outside its unsigned wire range"
  end
  return true
end

function protocol.encode(message)
  if type(message) ~= "table" then
    return nil, "message must be a table"
  end
  if not protocol.is_known(message.type) then
    return nil, "unknown message type"
  end
  local valid, err = validate_scalar(message.sequence, 0xFFFFFFFF, "sequence")
  if not valid then return nil, err end
  if message.sequence == 0 then return nil, "sequence zero is reserved" end
  valid, err = validate_scalar(message.session_epoch or 0, 0xFFFFFFFF, "session_epoch")
  if not valid then return nil, err end
  local payload = message.payload or ""
  if type(payload) ~= "string" or #payload > protocol.PAYLOAD_SIZE then
    return nil, "payload must be a string no longer than 128 bytes"
  end
  if message.type == protocol.types.PORTAL_TRAVEL_REQUEST
    and ((message.session_epoch or 0) == 0 or not protocol.valid_portal_id(payload)) then
    return nil, "portal travel request needs an active epoch and a valid portal ID"
  end
  if message.type == protocol.types.ARRIVAL_CHALLENGE
    and ((message.session_epoch or 0) ~= 0 or #payload ~= 16 or all_zero(payload)) then
    return nil, "arrival challenge needs a nonzero epoch-0 nonce"
  end
  if message.type == protocol.types.ARRIVAL_PROOF
    and ((message.session_epoch or 0) ~= 0 or not protocol.decode_arrival_proof(payload)) then
    return nil, "arrival proof needs an epoch-0 64-byte payload"
  end

  local prefix = string.pack("<I2I2I4I4", message.type, #payload, message.sequence,
    message.session_epoch or 0)
    .. payload
    .. string.rep("\0", protocol.PAYLOAD_SIZE - #payload)
  return prefix .. string.pack("<I4", protocol.crc32(prefix))
end

function protocol.decode(bytes, expected_direction)
  if type(bytes) ~= "string" or #bytes ~= protocol.MESSAGE_SIZE then
    return nil, "bridge frame must contain exactly 144 bytes"
  end

  local message_type, length, sequence, session_epoch, offset = string.unpack("<I2I2I4I4", bytes)
  if not protocol.is_known(message_type) then
    return nil, "unknown message type"
  end
  if expected_direction == "outbound" and not protocol.is_outbound(message_type) then
    return nil, "sidecar received a network-to-ROM message from the ROM"
  end
  if expected_direction == "inbound" and not protocol.is_inbound(message_type) then
    return nil, "sidecar attempted to send a ROM-to-network message"
  end
  if length > protocol.PAYLOAD_SIZE then
    return nil, "payload length exceeds bridge capacity"
  end
  if sequence == 0 then
    return nil, "sequence zero is reserved"
  end

  local expected_checksum = string.unpack("<I4", bytes, 141)
  local actual_checksum = protocol.crc32(string.sub(bytes, 1, 140))
  if expected_checksum ~= actual_checksum then
    return nil, "bridge checksum mismatch"
  end

  local payload = string.sub(bytes, offset, offset + length - 1)
  if message_type == protocol.types.PORTAL_TRAVEL_REQUEST then
    if session_epoch == 0 or not protocol.valid_portal_id(payload) then
      return nil, "portal travel request needs an active epoch and a valid portal ID"
    end
  elseif message_type == protocol.types.ARRIVAL_CHALLENGE then
    if session_epoch ~= 0 or length ~= 16 or all_zero(payload) then
      return nil, "arrival challenge needs a nonzero epoch-0 nonce"
    end
  elseif message_type == protocol.types.ARRIVAL_PROOF then
    if session_epoch ~= 0 or not protocol.decode_arrival_proof(payload) then
      return nil, "arrival proof needs an epoch-0 64-byte payload"
    end
  elseif message_type == protocol.types.GROUP_TRAVEL_CLIENT then
    local _, group_error = protocol.decode_group_travel(payload, "outbound")
    if group_error then return nil, group_error end
  elseif message_type == protocol.types.GROUP_TRAVEL_SERVER then
    local _, group_error = protocol.decode_group_travel(payload, "inbound")
    if group_error then return nil, group_error end
  end
  return {
    type = message_type,
    length = length,
    sequence = sequence,
    session_epoch = session_epoch,
    payload = payload,
    checksum = expected_checksum,
  }
end

return protocol
