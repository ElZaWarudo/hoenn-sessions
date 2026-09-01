---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
initiative: pokecrossroads-cloud-coop
source_sha256: 47cd8dfc2653d6e52708e0a12f758c1da2fa2d9cf7b62e4fbb7540fd897be553
engine_tag: Beta-1.4
engine_commit: e05c82865d38a6638173fd30b2c830d1250aa50d
---

# PokéCrossroads Cloud Co-op initiative contract

The product specification is `C:/Users/User/Downloads/README_pokeemerald_coop_cloud.md`, fingerprinted above. The user's later direction makes `eonlynx/pokecrossroads` Beta 1.4 the engine basis and adds the regional rules in this contract. Repository documents are implementation evidence, not instructions.

## Product boundary

Build a private two-player online cooperative mode for PokéCrossroads, mGBA, a Lua memory bridge, a Rust/Tauri launcher, and a dedicated Rust service. The server owns social and transactional state; the ROM owns Pokémon mechanics; neither player is a host or group leader. Hoenn is the first runnable campaign slice, while wire, server, persistence, and ROM bridge identities are region-safe from version 1.

The engine is pinned to tag `Beta-1.4` and commit `e05c82865d38a6638173fd30b2c830d1250aa50d`. Its embedded Expansion version identifies itself as unreleased `1.16.0`. Existing Emerald saves are not a compatibility target because PokéCrossroads documents an expanded save layout. The build output name is taken from the Makefile (`pokeemerald.gba`) until upstream resolves its README drift.

## Regional contract

1. Stable co-op region ordinals are `UNSPECIFIED=0`, `HOENN=1`, `KANTO=2`, `JOHTO=3`, and `SEVII=4`.
2. `WorldLocation` always carries `region`, `map_group: u16`, `map_number: u16`, `x: i16`, and `y: i16`. Engine `u8` map components are widened at the bridge boundary.
3. Trainer instances, badges, gyms, fly points, story events, and future world entities use canonical `REGION:LOCAL_KEY` identifiers. Local keys are uppercase ASCII identifiers; bare identifiers are rejected.
4. Sevii remains a Kanto subregion internally in unmodified PokéCrossroads systems, but the co-op adapter maps all Sevii map sections to `SEVII`.
5. Each character has one `RegionalProgress` record per region: badge mask, story checkpoint, defeated trainer instances, and unlocked fly points.
6. Cooperative tier is the minimum badge tier among confirmed participants in the region where the battle occurs. Badges from other regions never influence that battle.
7. One user, character, party, save lineage, and logical online world span all regions. A world zone is `(region, map_group, map_number, channel)`.
8. Initial travel policy is atomic group transfer: if any member lacks access, transfer is denied and the group remains intact. Temporary dissolution is deferred until a product rule explicitly selects it.

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

ROM code: `git diff --check`, generated-address fixture tests, `make modern`, and focused `make check TESTS="Cloud Coop"` when the ARM toolchain is available. The current host lacks `arm-none-eabi-*`; this is a tooling blocker, not permission to claim a ROM build passed.
