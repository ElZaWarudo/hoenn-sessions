---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
execution: code
initiative: johto-region
---

# Complete Johto in the existing world

The requested outcome is a playable Johto adventure connected to the existing
Hoenn/Kanto game. Importing scenery, compiling a ROM, or enabling a debug warp
does not satisfy it. The player must be able to complete the adventure and
travel between Johto and Kanto with the same character and Pokemon.

## Authority and baseline

The user requested the whole Johto region, permitted reuse of other projects,
required the Gen II-style connection back to Kanto, and requested Seneschal.
On 2026-09-08 the user accepted this work as a fully autonomous, subagentic
goal, permitting necessary local commits and requiring one release phase at
the end. This accepts the proposed existing-Kanto connection. Root records
the documentation approval against this packet before dispatch.
Remote Git, Jira, deployment and publication are outside this request.

Current source: `a5ace3186f38e147b31a9cd3cb4a660ae71f0afa`, branch
`codex/johto-new-bark`. The existing New Bark preview is preserved. Existing
uncommitted gameplay, launcher, server and Android changes belong to other
work and must not be overwritten or silently included in Johto commits.

## Scope and acceptance

- **R1 World:** all Johto towns, Routes 29-46, connecting gates, houses, shops,
  Pokemon Centers, gyms and campaign dungeons are reachable through ordinary
  play. Include the donor's Routes 47-48/Safari extension, Ruins of Alph,
  Whirl Islands, Tin/Bell Tower, Mt. Mortar, Dark Cave, Union Cave, Slowpoke
  Well, Ilex Forest, Ice Path, Dragon's Den, Lake of Rage, lighthouse, Radio
  Tower and Rocket hideouts. A reviewed content manifest must enumerate every
  selected map and explicitly classify every donor map not selected.
- **R2 Campaign:** Elm's opening, starter gift, rival encounters, eight Johto
  gyms, Slowpoke Well, Sudowoodo, medicine for Amphy, Lake of Rage, Rocket HQ,
  Radio Tower, Dragon's Den, legendary events and the Johto League form a
  completable progression. Required field moves and items have obtainable
  sources. HM gates use Johto progress when in Johto.
- **R3 Encounters:** import and adapt trainer parties, wild tables, gifts,
  trades and static encounters. Species, items, moves and abilities resolve
  by semantic identity, never by copying numeric donor IDs. Rewards and
  one-time encounters cannot be duplicated by re-entry or save/reload.
- **R4 Kanto continuity:** connect Johto to Kanto through Route 27/Tohjo Falls,
  Routes 26/22 and the shared Indigo area, plus S.S. Aqua between Olivine and
  Vermilion and the Magnet Train between Goldenrod and Saffron. Add access to
  Route 28/Mt. Silver and Red after the intended regional achievements.
  Every transport supports return travel and recoverable decline/denial.
- **R5 Regional progress:** Johto badges, story variables, defeated trainers,
  item pickups and champion status are independent of Hoenn/Kanto. A Johto
  victory must not award an existing-region badge, change another region's
  cap, or mark an unrelated trainer defeated. Pokemon identity and earned
  progression survive all regional transfers.
- **R6 Save compatibility:** existing saves load with the same party, PC,
  inventory, money, trainer flags and regional achievements. Initialize new
  Johto state once. Never enlarge or reorder saved structures without a
  reviewed migration and old-save fixtures. Preserve all existing map IDs.
- **R7 Presentation and recovery:** Johto has correct map names, regional Town
  Map/Fly destinations, healing and blackout return points. Menus, field moves,
  interiors, puzzles and music operate correctly after crossing regions.
  No mandatory route depends on a preview-only guide or debug facility.
- **R8 Co-op:** all selected maps have stable qualified identities; Johto
  progress and entitlement checks match the ROM. Validate admission and
  presence in Johto and cross-region travel against the existing group
  contract. Prevent partial group transfers on denial or disconnect.
- **R9 Proof:** deliver a built ROM and observed emulator evidence for entry,
  traversal, each gym/story gate, League completion, transport in both
  directions, save/reload, blackout and two-player regional behavior. Unit
  tests and static map renders supplement this; they do not substitute for it.

## Accepted product decisions

1. **One Kanto:** connect to the existing Kanto and adapt only locations/events
   needed for Johto travel. Do not install a second conflicting Kanto. Whether
   to recreate the full later Gen II Kanto is outside this accepted proposal.
2. **Entry:** support beginning a Johto adventure without resetting an existing
   character. Elm's introduction and gift must tolerate an existing party and
   full party/PC. A fresh Johto origin is desirable but requires inspection of
   the existing origin selector before its exact UI is specified. Neither
   approach may replace the Hoenn/Kanto opening or require a fresh save.
3. **Campaign source:** use HnS's GSC/HGSS adaptation as the content basis,
   adapting engine-dependent puzzles and services explicitly. No promise of
   byte-for-byte GSC/HGSS fidelity, a new battle engine, or donor-wide options.
4. **Shared Indigo:** reuse geographic Kanto identity while keeping the Johto
   League challenge and champion outcome distinct from existing Kanto events.
   Do not silently replace the Kanto Elite Four teams or Hall of Fame logic.
5. **Existing mechanics:** preserve the host's battle rules, mastery/caps and
   Pokemon systems; use region-qualified progression. Scaling or level resets
   are not part of this request. Inspect current level-projection rules first.

## Evidence and known integration risks

Pinned donor: `PokemonHnS-Development/pokemonHnS` at
`751823abaf677020bcd72c45fe3e7cb2b8a576e4`, read-only checkout `../johto-hns`.
Its README describes a completed Johto story and Kanto postgame and invites
reuse. Retain its credits and per-asset provenance. The expansion port remains
a reference only: its three-layer metatiles do not match the host renderer.

Root inspection on 2026-09-08 found 954 donor map registrations, 878 layouts,
176 referenced tilesets and 518 map-name collisions with this checkout. These
are donor-wide counts, not the Johto import manifest. The donor includes Kanto
and unused Emerald maps: blind recursive import is invalid.

The host has 210 registered map sections and an 8-bit map-header section field.
Its current trainer allocation is 1,478 with a maximum of 1,622; the donor uses
864 trainer slots. The selected Johto trainer count is still unknown. Numeric
ID copying and unmeasured save-array growth are therefore prohibited.
The New Bark ROM uses 85.37% of the 32 MiB ROM budget; every import must report
ROM, EWRAM and IWRAM deltas before adding further assets.

The donor Falkner script sets generic `FLAG_BADGE01_GET` and
`VAR_NUM_BADGES`, and its Magnet Train depends on a Kanto machine-part event.
Those dependencies require explicit adaptation, not search-and-replace alone.

## Completion boundary and deferred work

All R1-R9 are required for the requested complete region. Intermediate maps,
tests and ROMs are milestones only. Do not label a connected-map import or a
shortened custom campaign as the whole Johto region.

Deferred unless separately requested: a wholesale Gen II Kanto replacement,
additional regions, new battle mechanics, donor challenge-mode menus, a
Pokewalker/Pokeathlon recreation, public distribution and production hosting.
Donor limitations must be listed individually; do not conceal them as completed
features. Core Johto story and Kanto travel cannot be deferred to a later scope.

## Review and escalation

Root composition review checks requirements against the inspected map, save,
trainer and transport code. Implementation additionally requires independent
compatibility/save review and independent emulator validation. Escalate a
required save-format break, ROM-budget overflow, unresolvable donor behavior,
or a material Kanto timeline change before implementing that decision. Routine
symbol translation and path allocation remain implementation choices.

See [roadmap](roadmap.md), [Seneschal startup](../../swarm/johto-region/swarm-startup.md),
and [composition review](composition-review.md).
