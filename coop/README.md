# Local co-op vertical slice

This directory contains the host-side Phase 1 slice: the region-safe shared
protocol, a loopback-only in-memory server, and the local ROM sidecar. It is a
development boundary, not the authenticated public service described by the
full product specification.

## Validate

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python -m unittest discover -s tools/tests -p "test_*.py"
```

Run the internal server and its real HTTP two-player smoke scenario:

```text
cargo run -p coop-server -- --bind 127.0.0.1:3000
python tools/smoke_coop_server.py
```

The binary rejects non-loopback addresses until authentication and exclusive
character leases exist.

## Run the sidecar and mGBA bridge

The future launcher owns a persisted `u32` session epoch. Increment it for each
new sidecar process with wrapping arithmetic and skip zero:

```text
cargo run -p coop-sidecar -- --session-epoch 42
```

The sidecar prints one JSON descriptor containing its OS-selected loopback port
and ephemeral secret. Use it to create ignored `bridge/session.lua` from the
example. After a legal local ROM build, generate the address artifacts:

```text
python tools/generate_bridge_manifest.py --elf pokeemerald.elf --rom pokeemerald.gba
```

Before creating `session.lua` or starting the sidecar, the launcher must hash
the selected ROM and require an exact match with `game_build.rom_sha256` in
`dist/bridge_manifest.json`. The Lua API does not replace this whole-file
launcher check.

Open the matching ROM in pinned mGBA 0.10.5, choose **Tools → Scripting**, and
load `bridge/main.lua`. Generated addresses, session secrets, ROMs, saves,
savestates, and BIOS files must remain uncommitted.

## Deliberately deferred

Authentication, leases, PostgreSQL/Firebase persistence, authoritative
two-client battle-hash validation, deterministic lockstep, presence and remote
sprites, cloud snapshots, and the Tauri launcher belong to later milestones.
The current server's final-state hash is bounded transport evidence only; it is
not presented as an anti-cheat or deterministic-consensus proof.
