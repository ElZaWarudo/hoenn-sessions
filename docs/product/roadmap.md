# Cloud co-op delivery roadmap

## Current local checkpoint

Phase 5 has a certified local implementation of authenticated realtime presence,
launcher/sidecar activation, and safe ROM remote avatars with interpolation.
The earlier symmetric two-member group and region-entitled travel backend is
also implemented. See the [Phase 5 closeout](../orchestration/runs/pokecrossroads-phase5-presence-activation-20260902/phase5-presence-resume-final-closeout.json).

End-to-end authenticated Littleroot presence passed in two stock mGBA sessions
on 2026-09-06: reciprocal avatars, movement, nonblocking collision behavior,
and clean lifecycle/lease shutdown were observed. The
[conformance procedure and evidence](../testing/littleroot-conformance.md)
record the pinned artifacts and the limits of that result.
Realtime invitation UI, reconnect/leave lifecycle, live group warps, and
production PostgreSQL/Firebase adapters remain subsequent work.

## Milestone 0 — pinned regional foundation (local checkpoint complete)

Root the project on PokéCrossroads Beta 1.4, define stable region-qualified protocol/domain types, reserve and test the ROM NetBridge ABI, generate its symbol manifest, add the Lua/sidecar loop, and run an in-memory two-player Hoenn scenario.

Exit criteria: host workspace formats, lints, and tests; ROM sources pass focused tests when the ARM toolchain is available; all world/progress identities include a region; Kanto badges cannot raise a Hoenn tier or vice versa; atomic cross-region travel denial preserves the group.

## Milestone 1 — cloud session and resume

Implement invite-only registration, password authentication, refresh rotation, one exclusive lease per character, revisioned `.sav` storage, compatible optional savestates, Firebase storage adapter, PostgreSQL repositories, and a headless launcher core.

## Milestone 2 — Hoenn presence and groups (next player-visible milestone)

The local two-player walking slice is verified. Next, expose the certified symmetric group backend through an Online pause-menu invitation flow and add reconnect/leave lifecycle. Cross-map travel and save/resume still need separate live acceptance scenarios.

## Milestone 3 — cooperative battle laboratory

Implement a fixed Hoenn trainer battle with two human battlers, isolated deterministic RNG, action bundles, normalized hashes, divergence aborts, and a 100-run replay test.

## Milestone 4 — regional progress and real parties

Export real parties, apply regional minimum-tier level projection, restore canonical Pokémon state, reserve qualified trainer instances, commit per-character rewards idempotently, and enforce helper/rematch rules.

## Milestone 5 — multi-region world travel

Deliver live client warps and world presentation on top of the certified backend routes, then add regional fly points and canonical Kanto/Sevii progress ingestion. Add Johto when its PokéCrossroads content is stable enough to map and test.
