# Johto integration roadmap

This is the delivery decomposition for the complete scope in
[the initiative contract](initiative-requirements.md). It does not reduce the
request to an MVP. Each unit needs an implementation-ready, reviewed plan and
a bounded worker contract before dispatch. All use the current Johto preview
commit as source, advanced by accepted dependency patches. Production status
is unknown; all work is local and compatibility-preserving.

| Unit | Outcome and owned surface | Dependencies | Verification and admission |
| --- | --- | --- | --- |
| J1 | Import manifest, dependency closure, reserved map/section/trainer/flag/variable/item identities, measured memory/save design. Own `tools/johto/`, `data/johto/` and the explicitly reviewed constant/save seams. | Packet approval | Inventory every selected script reference; reject unresolved and colliding symbols. Compare existing numeric IDs/save layout. Build a representative map, trainer, story flag and round-trip save. Deep/high: persistent data and compatibility; executable contract, save specialist and independent validator. |
| J2 | Full Johto scenery, connected maps, interiors, collision and encounter terrain. Own only manifest-listed layouts, tilesets and map geometry; root owns shared registration generation. | J1 accepted | All exits resolve to registered maps, landings are valid, sections fit the header, tiles fit allocations. Walk representative exterior/interior/cave/puzzle seams in emulator. Standard/medium where frozen importer suffices; deep/high if renderer contracts change. |
| J3 | Johto trainers, wild encounters, gifts, trades and static encounters. Own Johto-specific encounter/party tables and reward data; no shared save schema edits. | J1 accepted | Complete symbolic references, valid species/moves/items, expected encounter distributions and parties; real battle/capture/gift including full party/bag and reload. Standard/medium after IDs are frozen; one independent reviewer. |
| J4 | Full Johto campaign, gym/badge progression, field-move gates and regional state. Own Johto scripts and explicitly assigned progression helpers. | J2, J3 accepted | Every required event and eight badges in sequence; losses do not award wins; repeats do not duplicate gifts; reload and blackout at each major gate. Deep/high: regional save integrity; executable contract, specialist and independent validator. |
| J5 | Kanto integration: overland route, S.S. Aqua, Magnet Train, distinct Johto League and Mt. Silver/Red access. Own explicitly selected border/station/port/Indigo maps and transport code. | J4 accepted; Kanto decision settled | Both directions, pass/ticket acquisition, absent ticket, decline, interrupted travel, existing Kanto story in early/late saves, distinct champion outcome. Deep/high: shared progression and travel contracts. |
| J6 | Regional Town Map/Fly, healing, blackout, origin/entry and player-facing polish. Own Johto-specific UI/heal/origin adapters; shared menu seams serialized. | J5 accepted | Correct region after transfer, no unavailable Fly destinations, save inside/outside, blackout and hospital respawn; full menu/input observations. Deep/high for save/respawn contracts; medium for isolated presentation. |
| J7 | Co-op catalog/progress/entitlement integration and full acceptance. Own explicitly assigned protocol generators, ROM exports and server travel registration; respect dirty server files. | J6 accepted | Protocol identity and progress tests, live two-client Johto presence, transfer/denial/disconnect, old-save regressions and final playthrough. Deep/high: public contracts and group atomicity. |

## Wave plan

Begin with J1 alone. J2 and J3 may run concurrently only after J1's identities,
generated interfaces and dependency snapshot are accepted and immutable.
They receive separate detached worktrees. Root serializes integration and
shared generated files. J4-J7 remain serial until narrower reviewed contracts
prove a safe split. At most two mutable implementers; reserve capacity for
independent review and validation. No wave has been dispatched.

## Start and finish criteria

Start requires the documentation packet's approval, product decisions settled
for the affected unit, exact writable paths, a reproducible source snapshot
including any explicitly needed user WIP, and an executable verification
contract. Existing dirty files cannot be imported into snapshots or commits
without preserving their provenance and the user's scope.

Finish requires all R1-R9, no unresolved mandatory content, content-bound
build/test evidence, independent review findings resolved, and observed
player-flow acceptance. Optional presentation polishing can follow; required
campaign, save compatibility and Kanto travel cannot.

## Reusable verification

Existing commands to retain:

```text
python -m unittest tools.tests.test_new_bark_import tools.tests.test_johto_region
python tools/coop/generate_regional_catalog.py --check
python tools/coop/generate_regional_identities.py --check
cargo test -p coop-protocol --lib --locked
make modern
make check -j6 TESTS=Cloud "TEST_SRCS_IN=test/test_runner.c test/test_runner_args.c test/test_runner_battle.c test/test_test_runner.c test/coop/region.c test/coop/presence.c"
```

These currently validate the preview. Expand their assertions rather than
freezing preview-only map counts or one-section Johto assumptions. New content
graph, progression and migration commands must be named by J1/J4 plans before
implementation; unimplemented test commands are not passing evidence.

Runtime acceptance uses real emulator input and ordinary saves, with fixture
saves clearly distinguished from a continuous new-game playthrough. Record
ROM/source hashes, screenshots, input steps and outcomes; do not count static
renders as emulator evidence. Run fresh Johto and pre-existing Hoenn/Kanto
save scenarios. Existing two-client co-op conformance procedures provide the
base for J7; no existing result proves the new region works.
