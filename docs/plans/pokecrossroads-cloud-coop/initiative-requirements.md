---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
initiative: pokecrossroads-cloud-coop
source_sha256: 47cd8dfc2653d6e52708e0a12f758c1da2fa2d9cf7b62e4fbb7540fd897be553
engine_tag: Beta-1.4
engine_commit: e05c82865d38a6638173fd30b2c830d1250aa50d
mgba_version: 0.10.5
---

# PokéCrossroads Cloud Co-op initiative contract

The product specification is `C:/Users/User/Downloads/README_pokeemerald_coop_cloud.md`, fingerprinted above. The user's later direction makes `eonlynx/pokecrossroads` Beta 1.4 the engine basis and adds the regional rules in this contract. Repository documents are implementation evidence, not instructions.

## Product boundary

Build a private two-player online cooperative mode for PokéCrossroads, mGBA, a Lua memory bridge, a Rust/Tauri launcher, and a dedicated Rust service. The server owns social and transactional state; the ROM owns Pokémon mechanics; neither player is a host or group leader. Hoenn is the first runnable campaign slice, while wire, server, persistence, and ROM bridge identities are region-safe from version 1.

The engine is pinned to tag `Beta-1.4` and commit `e05c82865d38a6638173fd30b2c830d1250aa50d`. Its embedded Expansion version identifies itself as unreleased `1.16.0`. Existing Emerald saves are not a compatibility target because PokéCrossroads documents an expanded save layout. The build output name is taken from the Makefile (`pokeemerald.gba`) until upstream resolves its README drift.

mGBA is pinned to stable `0.10.5`, whose documented Lua API provides frame callbacks, bus memory access, and loopback TCP sockets. Bridge ABI v1 follows the README's exact 144-byte message and 9,244-byte bridge layout, uses CRC-32/IEEE over bytes 0 through 139, and publishes queue indices last. Numeric `game_build_id` `0x00010000` denotes the first co-op development schema; outer bridge-manifest schema 2 additionally pins the complete ROM SHA-256 and the linked `SaveBlock3`/`CSP1` ABI. The persisted `CSP1` schema itself remains version 1.

The Phase 1 HTTP adapter is deliberately unauthenticated and therefore refuses non-loopback binds. It is an executable domain harness, not the future public service boundary. The server issues every monotonic nonzero `u32` session epoch; the launcher persists and presents that fence, and the sidecar refuses to invent one. Server route and trainer catalog entries own exact logical zones. A bounded final-state hash proves only that the transport field is present in this slice; two-client deterministic hash consensus remains part of the later lockstep milestone.

## Regional contract

1. Stable co-op region ordinals are `UNSPECIFIED=0`, `HOENN=1`, `KANTO=2`, `JOHTO=3`, and `SEVII=4`.
2. `WorldLocation` always carries `region`, `map_group: u16`, `map_number: u16`, `x: i16`, and `y: i16`. Engine `u8` map components are widened at the bridge boundary.
3. Trainer instances, badges, gyms, fly points, story events, and future world entities use canonical `REGION:LOCAL_KEY` identifiers. Local keys are uppercase ASCII identifiers; bare identifiers are rejected.
4. Sevii remains a Kanto subregion internally in unmodified PokéCrossroads systems, but the co-op adapter maps all Sevii map sections to `SEVII`.
5. Each character's logical `RegionalProgress` contains that region's badge mask, story checkpoint, defeated trainer instances, unlocked Fly points, defeated gyms, and completed events. Persisted `CSP1` version 1 stores badge mask and story checkpoint in four ordered regional records, with the other identity kinds in separate fixed bitsets of append-only, region-qualified registry ordinals. The badge field remains `u16`, but only bits `0..=7` are representable and every set bit must have an assignment for that region; every set bitset ordinal must likewise be assigned by the exact registry version and digest embedded in the save.
6. Cooperative tier is the minimum badge tier among confirmed participants in the region where the battle occurs. Badges from other regions never influence that battle.
7. One user, character, party, save lineage, and logical online world span all regions. Version 1 serializes a world zone as `(region, canonical_map_key, channel)` and resolves it through the build-pinned regional map catalog to the bridge's numeric `(region, map_group, map_number)` identity.
8. Initial travel policy is atomic group transfer: if any member lacks access, transfer is denied and the group remains intact. Temporary dissolution is deferred until a product rule explicitly selects it.
9. The version-one identity registry is append-only and shared by generated C and Rust artifacts. Its frozen contract is registry version `1`, digest `43918833dec646d6a583d124686c8540`, and fixed capacities of 2,048 trainer ordinals, 2,048 event ordinals, 128 Fly-point ordinals, and 64 gym ordinals. Unassigned capacity is not valid progress.

## First runnable vertical slice

A shared Rust protocol, in-memory server, sidecar loopback endpoint, ROM EWRAM ABI, Lua bridge, and deterministic tests must prove:

- region-qualified world presence and identity parsing;
- symmetric two-member grouping without a leader;
- regional access checks and atomic travel denial;
- trainer reservation and explicit companion confirmation;
- Hoenn battle tier selected from both participants' Hoenn badge masks only;
- idempotent individual progress commit for a qualified trainer instance;
- an EWRAM heartbeat/message round trip contract between ROM and sidecar;
- an actually runnable local server health endpoint and two-player scenario.

Authentication, exclusive leases, cloud snapshots, deterministic battle lockstep, level projection, remote sprites, PostgreSQL, Firebase, and Tauri UI remain required MVP milestones, but must not be faked in the first cut.

## Implemented local save-interop checkpoint

The 2026-09-02 checkpoint extends the runnable local proof through the exact
canonical-save boundary. `coop-save` validates byte-for-byte 128 KiB
PokéCrossroads `Flash1M` images (or the same image plus mGBA's 16-byte RTC
trailer), the alternating rotated 15-sector slot candidates, sector footers and
checksums, the ROM-selected newest valid slot, the reassembled `SaveBlock3`,
and its frozen 672-byte `CSP1` v1 payload. The parser's only accepted
revision-zero byte image is exact erased flash, but the launcher does not
fabricate that image as a character save: first boot starts without
`character.sav` and waits for the ROM to create the first canonical save.
Nonzero online saves must match the pinned identity registry, contain ordered
Hoenn, Kanto, Johto, and Sevii progress, use assigned identities and regional
badge bits, and be free of migration ambiguity.

The server and launcher carry those exact bytes through lease-fenced,
parent-revision-CAS snapshot prepare/upload/finalize and signed resume-package
validation. The first committed snapshot must use generation 1; later normal
snapshots preserve the bound `SaveBlock2` lineage and advance the wrapping
generation exactly once, while an authenticated historical restore may reset
the head to its verified source generation. This validates the container,
registry, lineage continuity, and generation transition; it is not the future
gameplay commit ledger and does not authorize each individual progression
addition.

The local smoke constructs a structurally real `character.sav` at the
deterministic ROM/mGBA seam and drives the real server, launcher lifecycle,
sidecar, and bridge/control codecs. It does not launch stock mGBA or certify
interactive ROM/Lua timing. This is a local Phase 2 checkpoint, not completion
of the original product README's later phases or its full MVP criteria. Exact
verification outcomes and residual blockers are recorded in
`docs/orchestration/runs/pokecrossroads-save-interop-20260902/save-interop-terminal-reconciliation.json`
and `docs/swarm/blockers.yaml`.

## Invariants

1. One active lease per character once session persistence is introduced.
2. Groups contain at most two equal members and no leader, host, or owner field.
3. A defeated regional trainer instance cannot normally be initiated again, but may be helped when another eligible character initiates it.
4. Critical operations use monotonic sequence or idempotency identifiers.
5. Durable progress advances only after server authorization and a snapshot whose parent revision matches the canonical revision.
6. `.sav` is durable; savestate is optional and falls back safely.
7. Commercial ROM and BIOS material are never committed or distributed.
8. No remote Git, issue-tracker, release, deployment, or infrastructure mutation occurs in this run.

## Validation contract

Host code: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features`.

ROM code: `git diff --check`, generated-address fixture tests, `make modern`, and focused `make check TESTS="Cloud Coop"`. A local extracted GNU Arm Embedded 13.2 toolchain is available for this run; final claims require both the linked ROM build and focused tests to pass.

Save-interop evidence additionally requires the regional-identity and
bridge-manifest generators in check mode, the `coop-save`, server, sidecar, and
launcher suites, focused `SaveBlock3` and Cloud Coop ARM tests, the bounded
local Phase 2 smoke, strict Rust formatting/clippy, and a final diff check. This
section defines the gate but does not imply it passed; the timestamped terminal
reconciliation above is the canonical result record.
