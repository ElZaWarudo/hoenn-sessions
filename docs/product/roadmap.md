# Cloud co-op delivery roadmap

## Milestone 0 — pinned regional foundation (active)

Root the project on PokéCrossroads Beta 1.4, define stable region-qualified protocol/domain types, reserve and test the ROM NetBridge ABI, generate its symbol manifest, add the Lua/sidecar loop, and run an in-memory two-player Hoenn scenario.

Exit criteria: host workspace formats, lints, and tests; ROM sources pass focused tests when the ARM toolchain is available; all world/progress identities include a region; Kanto badges cannot raise a Hoenn tier or vice versa; atomic cross-region travel denial preserves the group.

## Milestone 1 — cloud session and resume

Implement invite-only registration, password authentication, refresh rotation, one exclusive lease per character, revisioned `.sav` storage, compatible optional savestates, Firebase storage adapter, PostgreSQL repositories, and a headless launcher core.

## Milestone 2 — Hoenn presence and groups

Connect authenticated WebSocket presence to ROM remote object events, interpolation, avatar interaction, an Online pause menu, symmetric group invitations, and reconnect grace.

## Milestone 3 — cooperative battle laboratory

Implement a fixed Hoenn trainer battle with two human battlers, isolated deterministic RNG, action bundles, normalized hashes, divergence aborts, and a 100-run replay test.

## Milestone 4 — regional progress and real parties

Export real parties, apply regional minimum-tier level projection, restore canonical Pokémon state, reserve qualified trainer instances, commit per-character rewards idempotently, and enforce helper/rematch rules.

## Milestone 5 — multi-region world travel

Expose Kanto and Sevii zones, travel entitlements, atomic group transfer, regional fly points, and separate Kanto/Sevii progress. Add Johto when its PokéCrossroads content is stable enough to map and test.
