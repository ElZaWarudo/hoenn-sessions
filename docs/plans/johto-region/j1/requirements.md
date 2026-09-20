> Revision note: the original broad proposal below is retained as research. Its single-group and section-count assumptions are superseded by `../j1-decisions.md` and `manifest-work-package.md`. Only the manifest tooling slice is execution-ready; save, trainer, runtime registration and campaign work remain gated.

# J1 — Johto import, save, and identity foundation

Status: implementation-ready proposal pending the root independent review.

## Contract and baseline

- Run: `johto-region-j1`; orchestrator `seneschal`; mode `artifacts`; interaction `brokered`; Jira `skip`.
- Contract: `C:/Users/Mayor/Documents/Caribbean/hoenn-sessions/docs/orchestration/runs/johto-region-20260908/j1-planning-contract.json`.
- Contract SHA-256: `f996d3fdd1501e13ed197136228be1eae003f149f122ba6209e8328c96accc61`.
- Source revision: `121ee4c192d476e57ca033a1e420e16c4dcec865`.
- Sealed baseline tree: `5e4c97f0cc2a129d51ea98cd9099695853ffc261`.
- Donor: `C:/Users/Mayor/Documents/Caribbean/johto-hns`, pinned at `751823abaf677020bcd72c45fe3e7cb2b8a576e4`.

## Outcome required

Import the complete Johto campaign into the existing Kanto-capable host. The closure includes New Bark through Blackthorn, Routes 29–48, gates and buildings, all campaign dungeons and special areas, Johto story progression, and Kanto continuity through Route 27/Tohjo Falls, Route 26 and Route 28, the Indigo border, S.S. Aqua, Magnet Train, Route 28 and Mt. Silver. A New Bark scenery preview is insufficient.

Existing saves must retain party, PC, inventory, money, trainer flags, event flags, fly points, regional achievements, and old map IDs. Johto state initializes once for a legacy save and is then persisted through the shared cooperative save schema. No donor numeric ID may be copied as an identity.

## Selected content closure

The manifest selects 239 donor map registrations in deterministic donor group order:

| Content | Count | Selection rule |
|---|---:|---|
| Towns/routes | 35 | New Bark, Cherrygrove, Violet, Azalea, Goldenrod, Ecruteak, Olivine, Cianwood, Safari Gate, Mahogany, Blackthorn, Routes 26–48 |
| Town interiors | 87 | All `IndoorNewBark`, `IndoorCherrygrove`, `IndoorViolet`, `IndoorAzalea`, `IndoorGoldenrod`, `IndoorEcruteak`, `IndoorOlivine`, `IndoorCianwood`, `IndoorMahogany`, and `IndoorBlackthorn` registrations |
| Johto route interiors | 25 | All `IndoorJohtoRoutes` except the two Trainer Hill courtyard registrations |
| Dungeons | 72 | All donor dungeons except Kanto Victory Road, Viridian Forest, Mt. Moon, Rock Tunnel, Cerulean Cave, Diglett’s Cave, Seafoam Islands, and Route 19 Cave; add `CliffEdgeCave` for Route 47 closure |
| Special areas | 18 | All 11 `SSAqua_*` registrations plus the seven connected Safari extension registrations |
| Route 26 houses | 2 | `Route26_House1`, `Route26_House2`, required by Route 26 warps |

The seven Safari registrations are `SafariZone_Top_Left`, `SafariZone_Low_Mid`, `SafariZone_Enterance`, `SafariZone_Low_Left`, `SafariZone_Low_Right`, `SafariZone_Top_Mid`, and `SafariZone_Top_Right`. `SafariZone1`, `SafariZone2`, `SafariZone3`, and `SafariZoneIndoor` are donor debug duplicates and are excluded. The two excluded `IndoorJohtoRoutes` maps are `Gate_Route40_TrainerHill_Courtyard` and `TrainerHill_Courtyard`.

Every other donor registration is classified as Kanto, Emerald, donor debug, or duplicate content by its source group in `inventory.json`; partial groups have explicit exclusions there. The import must fail closed if a registration is neither selected nor explicitly excluded.

## Numeric and identity invariants

1. Retain host `gMapGroup_Johto` at group index 75 and its existing `NewBarkTown` map at map index 0. Append the other 238 selected registrations in donor group order at indices 1–238, leaving 239–255 as reserve. Do not reorder or renumber a host map.
2. The host map-header section field is u8. The host section table has 210 entries and only 11 unused IDs. The selected donor closure has about 55 unique section names before collision normalization. Therefore the implementation must first measure actual host section references, then reuse only deliberately compatible existing IDs or compact/alias Johto section names. It must never append blindly past 255. `MAPSEC_NEW_BARK_TOWN` is preserved; donor `MAPSEC_ROCKET_HIDEOUT` and `MAPSEC_SAFARI_ZONE` require Johto-qualified semantics; `ReceptionGate` retains the Kanto border semantics of the existing Victory Road/Route 22 connection.
3. Host trainer storage has 1,478 current records, a 1,622 hard maximum, and 144 free standard slots. Selected Johto scripts contain 284 unique trainer identities (245 trainer event records and 243 distinct scripts), including two battle-set placeholders. Numeric append at 1,478–1,761 is feasible only with a reviewed SaveBlock1 trainer-flag expansion and compatibility migration. The preferred foundation is a qualified Johto identity adapter backed by the 2,048-bit cooperative regional trainer set, with engine trainer records allocated only after the flag seam is proven. Never treat same-named host and donor trainers as identical without party/behavior evidence.
4. The identity registry currently contains 912 entries and Johto seed entries already occupy trainer ordinal 855, gym ordinals 16–23, badge bits 0–7, fly ordinal 2, and event ordinal 2. Preserve existing order and append qualified Johto identities. The registry digest/version change requires a save migration that accepts the current v1 digest, copies all old bitsets, initializes new bits to zero, and writes the current digest. A valid current save must not become incompatible merely because Johto identities were appended.
5. Donor scripts contain 354 flag symbols and 72 variable symbols; only 48 flags and 16 variables are known to the host. Allocate/translate Johto-qualified flags and vars, and route progression through regional APIs. Do not reuse global `FLAG_BADGE01_GET`, `VAR_NUM_BADGES`, `VAR_TRAIN`, or similarly generic donor symbols for Johto state.
6. Selected scripts contain 149 wild encounter records across 93 maps, covering 3,436 species slots. The 146 selected maps without tables are intentionally classified as non-encounter maps. Every selected encounter map receives a verified host-format table and every non-encounter map is checked for an explicit empty classification.
7. Shared host scripts (PC, Nurse, generic trees/rocks/boulders, and standard service labels) are adapted through host equivalents. Donor contamination labels from Lilycove, Mauville, Lavaridge, Fallarbor, Oldale, Route 104, and Route 121 are removed or replaced. `0`/`0x0` script references normalize to null. New Bark preview/WorldHub and Tin Tower roof warps are replaced by the Elm opening and real campaign links. Route 40 Trainer Hill warps are removed. Kanto border warps are mapped to the existing host maps.

## Acceptance evidence required from implementation

- A generated 239-map manifest with no duplicate map IDs, no changed pre-existing map IDs, and a connected internal Johto/Kanto border graph.
- A section allocation report proving every selected map’s section is `<256`, resolves to JOHTO except intentional Kanto border maps, and does not change host map section behavior.
- A trainer identity report proving all 284 selected identities have qualified mappings, party data, defeated-state storage, and no flag overlap.
- A registry migration fixture loading the current v1 digest and preserving every old bitset while adding Johto zero state.
- Script, flag, variable, special-command, graphics, layout, and wild-encounter closure reports; no unresolved donor contamination.
- Save/load tests for legacy, current, incompatible, full-bitset, and CRC-failure cases, plus real-input campaign traversal through the Johto opening and at least one Kanto continuity route.
