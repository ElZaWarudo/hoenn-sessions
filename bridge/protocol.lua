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
}

local function is_integer(value)
  return type(value) == "number" and math.type(value) == "integer"
end

function protocol.is_outbound(message_type)
  return is_integer(message_type)
    and message_type >= protocol.types.ROM_READY
    and message_type <= protocol.types.SAVE_DATA_UPDATED
end

function protocol.is_inbound(message_type)
  return is_integer(message_type)
    and message_type >= protocol.types.SESSION_READY
    and message_type <= protocol.types.CHECKPOINT_GRANTED
end

function protocol.is_known(message_type)
  return protocol.is_outbound(message_type) or protocol.is_inbound(message_type)
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
