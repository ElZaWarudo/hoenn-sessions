//! Local, authenticated transport between the mGBA Lua bridge and the cloud sidecar.
//!
//! The wire format intentionally mirrors `struct CoopBridgeMessage` in the ROM:
//! a fixed 144-byte little-endian frame with no transport length prefix.

mod codec;
pub mod control;
pub mod realtime;
mod server;

pub use codec::{
    BRIDGE_ABI_VERSION, BRIDGE_FRAME_SIZE, BRIDGE_PAYLOAD_SIZE, BridgeFrame, Direction,
    FrameCodecError, GAME_PROTOCOL_VERSION, MessageType,
};
pub use realtime::{
    CONNECT_TIMEOUT, MAX_INTERACTION_QUEUE, MAX_OWNER_EVENT_QUEUE, MAX_REALTIME_ENDPOINT_BYTES,
    MAX_REMOTE_PLAYERS, PRESENCE_TICK, READY_TIMEOUT, RealtimeDriver, RealtimeEndpoint,
    RealtimeError, RealtimeGrant, RealtimeInputError, RealtimeOutcome, RealtimeOwner,
    RealtimeOwnerEvent, WRITE_TIMEOUT, realtime_channel, run_realtime,
};
pub use server::{
    BridgeDescriptor, ControlDescriptor, HANDSHAKE_ACCEPTED_LINE, LocalSidecar,
    MAX_DESCRIPTOR_BYTES, MAX_HANDSHAKE_BYTES, SessionDescriptor, SidecarError,
};
