# Cloud co-op delivery roadmap

## Current local checkpoint

The authenticated backend now has a certified server-side foundation for strict, symmetric two-member group invitations, acceptance, inspection, and atomic region-entitled travel across the designated Hoenn, Sevii, and Kanto ferry routes. Route thresholds use each member's source-region progress, while a destination-region progress record is the travel entitlement; a denial leaves both members and the group unchanged. The frozen implementation has independent correctness and security approvals under `docs/orchestration/runs/pokecrossroads-phase4-group-travel-20260902/`.

This checkpoint does not include live sidecar warp delivery, remote avatar rendering, a realtime invitation UI, reconnect or leave lifecycle, or production PostgreSQL/Firebase adapters. The next player-visible milestone is end-to-end authenticated Littleroot presence: two live sessions must see each other walking through the sidecar, ROM remote-object, and interpolation path.

## Milestone 0 — pinned regional foundation (active)

Root the project on PokéCrossroads Beta 1.4, define stable region-qualified protocol/domain types, reserve and test the ROM NetBridge ABI, generate its symbol manifest, add the Lua/sidecar loop, and run an in-memory two-player Hoenn scenario.

Exit criteria: host workspace formats, lints, and tests; ROM sources pass focused tests when the ARM toolchain is available; all world/progress identities include a region; Kanto badges cannot raise a Hoenn tier or vice versa; atomic cross-region travel denial preserves the group.

## Milestone 1 — cloud session and resume

Implement invite-only registration, password authentication, refresh rotation, one exclusive lease per character, revisioned `.sav` storage, compatible optional savestates, Firebase storage adapter, PostgreSQL repositories, and a headless launcher core.

## Milestone 2 — Hoenn presence and groups (next player-visible milestone)

Connect authenticated WebSocket presence to ROM remote object events, interpolation, and avatar interaction so two authenticated sessions can see each other walking in Littleroot. Then expose the certified symmetric group backend through an Online pause-menu invitation flow and add reconnect/leave lifecycle.

## Milestone 3 — cooperative battle laboratory

Implement a fixed Hoenn trainer battle with two human battlers, isolated deterministic RNG, action bundles, normalized hashes, divergence aborts, and a 100-run replay test.

## Milestone 4 — regional progress and real parties

Export real parties, apply regional minimum-tier level projection, restore canonical Pokémon state, reserve qualified trainer instances, commit per-character rewards idempotently, and enforce helper/rematch rules.

## Milestone 5 — multi-region world travel

Deliver live client warps and world presentation on top of the certified backend routes, then add regional fly points and canonical Kanto/Sevii progress ingestion. Add Johto when its PokéCrossroads content is stable enough to map and test.
