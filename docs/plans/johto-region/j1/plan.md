> Revision note: the original broad proposal below is retained as research. Its single-group and section-count assumptions are superseded by `../j1-decisions.md` and `manifest-work-package.md`. Only the manifest tooling slice is execution-ready; save, trainer, runtime registration and campaign work remain gated.

# J1 implementation plan — Johto import/save/identity foundation

This is the concrete proposal for root review. It is intentionally limited to foundation work; it does not claim that the campaign is implemented or reviewed-ready.

## 1. Freeze the manifest and ID ledger

Read the pinned donor `data/maps/map_groups.json`, each selected `map.json`, layouts, scripts, graphics references, wild tables, and trainer parties. Generate a checked-in manifest from the selection in `requirements.md` and `inventory.json`. Require exactly 239 registrations and reject an unclassified donor registration. Preserve host map group 75/index 0 (`NewBarkTown`), append selected registrations in donor group order at indices 1–238, and reserve 239–255. Keep the donor map-to-layout and map-to-tileset references symbolic until the host asset ledger resolves them.

Required edit paths:

- `data/maps/map_groups.json` and the generated map group output: append into the existing Johto group, never copy donor group numbers.
- `data/maps/*/map.json`, `data/layouts/*`, and the host map/connection registries: import only the manifest closure.
- `src/data/maps/` generated maps and any generated headers: regenerate after the symbolic ledger is reviewed.

## 2. Resolve the u8 section constraint before touching map data

Extract the 210 host section IDs and all actual host map references. Allocate Johto sections in a deterministic ledger. Preserve `MAPSEC_NEW_BARK_TOWN`; qualify Rocket Hideout and Safari Zone for Johto; keep `ReceptionGate` on the host Kanto border semantics. The donor closure has approximately 55 unique sections and the host has only 11 currently unused slots, so a simple append is invalid. The implementation unit must choose and document one of these measurable options:

1. alias Johto interiors and sub-areas to their town/route section where the region map contract permits it;
2. recycle an existing section ID only where a complete host-reference audit proves no existing map behavior or save identity depends on the old semantic; or
3. compact the section table through a generated compatibility map while preserving every old map’s externally visible section behavior.

The output ledger must prove every selected map resolves to a byte below 256 and `CoopRegion_FromSectionId` returns JOHTO for Johto sections. Do not proceed to content implementation with an unresolved section ledger.

Required edit paths: `src/data/region_map/region_map_sections.json`, generated `include/constants/region_map_sections.h`, `src/coop/region.c`, `include/coop/region.h` only if the reviewed mapping requires it, and map headers/JSON that carry the section symbol.

## 3. Establish qualified Johto identity mappings

Start from the existing cooperative registry, retaining all existing ordinals. Append qualified Johto identities for the 284 unique trainer symbols selected by script closure, Johto campaign events, gym/badge progression, fly points, and any one-time state. Keep the already-seeded Johto identities stable. Generate the registry and lock artifacts only from the reviewed ledger; never copy donor numeric values.

Trainer handling is a gating seam. The host has 1,478 records and only 144 free standard slots, while Johto needs 284 unique records. Implement an adapter that gives each Johto trainer a qualified identity and regional defeated bit. If standard engine records are required for battle setup, allocate a measured contiguous range only after expanding/migrating the trainer-flag storage safely; preserving host flags 0–1,477 is mandatory. The two donor battle-set placeholders must resolve to their concrete qualified trainer identities (`ARIANA_1` and `GRUNT_23`) rather than remaining dynamic donor placeholders.

Required edit paths: `data/coop/regional_identities.json`, `data/coop/regional_identities.lock.json`, `include/coop/identity.h`, the trainer identity adapter, and the engine trainer/flag seam identified by the host implementation. Any SaveBlock1 layout change needs a separate migration fixture in the same implementation review.

## 4. Adapt scripts, flags, vars, specials, and graphics

Build a script closure report before import. Map generic host labels through host services (PC, Nurse, tree, rock smash, strength, whirlpool, bookshelf, and service tutors). Remove donor contamination labels from Hoenn shops/contests and replace them with Johto services. Normalize null script references. Replace New Bark’s preview/WorldHub and Tin Tower roof links with Elm opening, starter, rival, and ordinary campaign entrances. Remove Route 40 Trainer Hill warps. Adapt Goldenrod elevator to the host dynamic map facility and adapt `ReceptionGate` Kanto border warps to host Route 22 and Kanto Victory Road maps.

Create qualified Johto flags and vars from a collision report. Route badge and campaign state through regional progression. `FLAG_BADGE01_GET`/`VAR_NUM_BADGES` are donor globals and cannot be used as Johto state. Audit every `special`, `setvar`, `setflag`, `clearflag`, `applymovement`, and `setmetatile` command against the host command set and graphics constants. Missing command or graphics dependencies become explicit work items in the asset ledger, not silent no-ops.

Required edit paths: selected `data/scripts/*`, `data/maps/*/scripts.inc`, `include/constants/flags.h`, `include/constants/vars.h`, special command registries, and the graphics/tileset manifest generated by the host.

## 5. Import encounter and battle data

Import the 149 selected wild records on 93 maps after species and encounter-format validation. Mark the remaining 146 selected maps explicitly as non-encounter maps. Verify land, water, fishing, and rock-smash tables independently. Add party data and battle setup for every qualified trainer. The output report must show 284/284 trainer identities, 245/245 trainer event records, and both battle-set placeholders resolved.

## 6. Migrate cooperative saves and registry digest

The existing save struct is fixed at 672 bytes with 2,048 trainer bits, 2,048 event bits, 128 fly bits, and 64 gym bits. Preserve field offsets and CRC behavior. Add a compatibility path for the current v1 registry version/digest: load the old header, copy all existing bitsets and progress, zero only newly appended Johto identity bits, then reseal with the current registry metadata. Keep legacy pre-schema handling explicit and preserve party/PC/inventory/money and map state in the engine save migration. A current valid save must remain loadable after registry append.

Required edit paths: `include/coop/save.h`, `src/coop/save.c`, registry digest generation, engine save migration code, and focused save migration fixtures. No migration may silently discard an old bit or reinterpret an old ordinal.

## 7. Verification gates and exact next slice

Run focused checks only after root accepts this plan: manifest/ID ledger checker; section byte and region mapping checker; script/asset closure checker; trainer/party/flag checker; wild-table checker; registry migration tests; save CRC/version tests; and a real-input libGBA traversal from New Bark through the first Johto transition. Broader builds and end-to-end campaign traversal belong to the root integration wave.

The next implementation unit is **J1-RU1: ledger and compatibility seams**: freeze the 239-map manifest, produce the section allocation ledger with a measured u8-safe choice, append qualified identity records without changing existing ordinals, and implement/read-test current-v1 registry migration before importing scripts or graphics. J1-RU1 must stop if the section ledger or trainer SaveBlock1 strategy cannot be proven without changing old map/save identity.
