//! The Android client re-implements the bridge frame rules in Java. When this
//! codec gained realtime message types and protocol 2, the Java copy kept the
//! old limits and silently dropped the bridge on the first companion update.
//! This test keeps the two in lockstep.

use coop_sidecar::{BRIDGE_ABI_VERSION, Direction, GAME_PROTOCOL_VERSION, MessageType};

fn java_constant(source: &str, name: &str) -> u16 {
    let marker = format!("static final int {name}=");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("BridgeFrame.java does not declare {name}"))
        + marker.len();
    let literal = &source[start..start + source[start..].find(';').expect("terminated constant")];
    match literal.strip_prefix("0x") {
        Some(hex) => u16::from_str_radix(hex, 16),
        None => literal.parse(),
    }
    .unwrap_or_else(|_| panic!("{name} is not a plain integer literal: {literal}"))
}

fn last_type(direction: Direction) -> u16 {
    (0..=u16::MAX)
        .filter(|value| {
            MessageType::try_from(*value).is_ok_and(|message| message.direction() == direction)
        })
        .max()
        .expect("each direction has message types")
}

#[test]
fn android_bridge_frame_matches_the_sidecar_codec() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../android/app/src/main/java/io/hoenn/sessions/BridgeFrame.java"
    );
    let source = std::fs::read_to_string(path).expect("Android BridgeFrame.java");
    assert_eq!(java_constant(&source, "BRIDGE_ABI"), BRIDGE_ABI_VERSION);
    assert_eq!(
        java_constant(&source, "PROTOCOL_VERSION"),
        GAME_PROTOCOL_VERSION
    );
    assert_eq!(
        java_constant(&source, "LAST_OUTBOUND_TYPE"),
        last_type(Direction::RomToSidecar)
    );
    assert_eq!(
        java_constant(&source, "LAST_INBOUND_TYPE"),
        last_type(Direction::SidecarToRom)
    );
    // The Java ranges are contiguous, so every codec type must fall inside them.
    for value in 0..=u16::MAX {
        if let Ok(message) = MessageType::try_from(value) {
            match message.direction() {
                Direction::RomToSidecar => assert!((1..0x100).contains(&value), "{value:#x}"),
                Direction::SidecarToRom => assert!(value >= 0x100, "{value:#x}"),
            }
        }
    }
}
