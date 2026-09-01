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

## Phase 2 local checkpoint smoke

The authenticated Phase 2 adapter is an explicit local mode. It binds only to
the literal loopback address and accepts an OS-selected port, so no service is
published to a LAN or the Internet. Run the bounded integration runner from the
repository root:

```text
python tools/smoke_phase2.py --local
```

The runner invokes these exact targets without a shell and gives each a finite
five-minute timeout:

```text
cargo test -p coop-server --test phase2_http --all-features --locked
cargo test -p coop-launcher --test phase2_smoke --all-features --locked
```

The server binary smoke requires the following local-only environment values;
the test supplies deterministic values itself. An operator starting the binary
must provide their own values and must never paste them into logs or tickets:

```text
COOP_PHASE2_STORAGE_MODE=phase2-local
COOP_PHASE2_INVITE_PEPPER=<server-only random pepper>
COOP_PHASE2_SIGNING_KEY_HEX=<64 lowercase hex characters for the private key>
COOP_PHASE2_SIGNING_KEY_ID=<pinned signing-key identifier>
COOP_PHASE2_BOOTSTRAP_INVITATION=<one-use invitation>
cargo run -p coop-server -- --phase2-local --bind 127.0.0.1:0
```

Never use a wildcard bind, commit an invitation, or print the pepper, signing
key, access/refresh token, bridge secret, or control secret. The bridge and
control channels have independent ephemeral secrets. Only the bridge endpoint
and bridge secret belong in the private generated `session.lua`; the control
secret remains launcher-memory-only. Generated session files, ROMs, saves,
savestates, and BIOS files remain ignored and local.

The smoke exercises invite registration, case-insensitive login, lease fencing,
snapshot prepare/upload/finalize, signed resume verification, reconnect, stale
fence rejection, release, and revision-preserving reacquire. Its synthetic SAV
is an opaque serialized `CharacterCloudState` fixture used to test regional
serialization and CAS behavior. It is not a production `.sav` block and does
not demonstrate interoperability with a real ROM save.

If a smoke fails, first stop the server/launcher processes and rerun the
command; private temporary session directories are cleaned up by the test. A
stale lease must be recovered through the server reconnect/release path; do not
invent an epoch or edit the epoch file. A missing or malformed Phase 2
environment must fail closed. The local adapter is process-memory only and
does not claim Firebase, PostgreSQL, production token issuance, object storage,
or deployment readiness.

This hardened Phase 2 launcher process-supervision slice is supported on
Windows; it fails closed before probing or spawning child processes on other
operating systems. The local integration runner supports Windows and Linux
listener ownership checks and likewise fails before launching any target on
other operating systems. Cross-platform secure process binding is a later
milestone; the server and integration test code remains portable to compile.

## Run the sidecar and mGBA bridge

The cloud server owns the durable, nonzero session epoch. The launcher persists
the greatest accepted server-issued epoch and never increments or invents it:

Use the nonzero `session_epoch` returned by the server's acquire/reconnect lease
response when launching the sidecar; never substitute a hand-written epoch:

```text
cargo run -p coop-sidecar -- --session-epoch <server-issued-session-epoch>
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
manually load `bridge/main.lua`. Stock mGBA 0.10.5 requires this UI action;
there is no supported script-autoload command-line flag. Generated addresses,
session secrets, ROMs, saves, savestates, and BIOS files must remain
uncommitted.

## Deliberately deferred

PostgreSQL/Firebase persistence, authoritative two-client battle-hash
validation, deterministic lockstep, presence and remote sprites, and the Tauri
launcher belong to later milestones. The Phase 2 local smoke is an executable
contract test, not production cloud readiness.
The current server's final-state hash is bounded transport evidence only; it is
not presented as an anti-cheat or deterministic-consensus proof.
