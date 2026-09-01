//! Local, authenticated transport between the mGBA Lua bridge and the cloud sidecar.
//!
//! The wire format intentionally mirrors `struct CoopBridgeMessage` in the ROM:
//! a fixed 144-byte little-endian frame with no transport length prefix.

mod codec;
pub mod control;
mod server;

pub use codec::{
    BRIDGE_ABI_VERSION, BRIDGE_FRAME_SIZE, BRIDGE_PAYLOAD_SIZE, BridgeFrame, Direction,
    FrameCodecError, GAME_PROTOCOL_VERSION, MessageType,
};
pub use server::{
    BridgeDescriptor, ControlDescriptor, HANDSHAKE_ACCEPTED_LINE, LocalSidecar,
    MAX_DESCRIPTOR_BYTES, MAX_HANDSHAKE_BYTES, SessionDescriptor, SidecarError,
};
