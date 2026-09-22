> Revision note: the original broad proposal below is retained as research. Its single-group and section-count assumptions are superseded by `../j1-decisions.md` and `manifest-work-package.md`. Only the manifest tooling slice is execution-ready; save, trainer, runtime registration and campaign work remain gated.

# Work package J1-RU1 — Johto foundation ledger and save compatibility

## Contract

- Run ID: `johto-region-j1`
- Review unit: `J1-RU1`
- Owner: Seneschal nested planning role; root owns independent review and acceptance.
- Mode: artifact-first proposal; no implementation performed in this package.
- Source/base: `121ee4c192d476e57ca033a1e420e16c4dcec865` / sealed tree `5e4c97f0cc2a129d51ea98cd9099695853ffc261`.
- Donor: `C:/Users/Mayor/Documents/Caribbean/johto-hns@751823abaf677020bcd72c45fe3e7cb2b8a576e4`.

## Objective

Make the full Johto import mechanically safe to implement by freezing its 239-map closure, proving a map-header-byte-safe section ledger, defining qualified Johto trainer/event/gym/badge/fly identities, and specifying a current-v1 cooperative-save migration that preserves old state when the registry digest changes.

## Scope

1. Materialize the 239-registration manifest: 35 towns/routes, 87 town interiors, 25 Johto route interiors, 72 dungeons, 18 special areas, and two Route 26 houses. Classify every donor registration either selected or excluded by the inventory rules.
2. Preserve host Johto map group 75 and `NewBarkTown` index 0; append selected registrations at 1–238 and reserve 239–255.
3. Resolve the 210-entry host section table against the approximately 55 unique selected Johto section names and the u8 map-header field. Preserve New Bark, qualify Johto Rocket Hideout/Safari, and preserve Kanto border semantics.
4. Append qualified Johto registry identities without reordering existing entries. Resolve 284 trainer symbols, 245 trainer event records, 243 scripts, and two battle-set placeholders; keep party and defeated-state mapping explicit.
5. Define current registry v1/digest migration across the fixed 672-byte cooperative save, including preservation of all existing bitsets and engine save state.

## Technical acceptance criteria

- Manifest count is exactly 239; no duplicate registration or unclassified donor map remains.
- Existing host map IDs, group IDs, registry ordinals, and save field offsets remain stable.
- Every imported map section is byte-safe (`0..255`) and has a reviewed region mapping. The implementation must provide a reference report for all 239 maps.
- All 284 trainer identities have a qualified mapping, party data, battle setup, defeated-state key, and collision result. No donor numeric trainer ID is reused.
- Existing v1 registry saves load after identity append; old trainer/event/fly/gym bits, progress, and engine save fields are preserved; new Johto bits initialize zero; CRC is resealed.
- Script closure contains no donor debug/Hoenn service reference, unresolved non-null label, or generic global Johto badge state.
- Wild closure reports 149 records/93 maps and explicitly classifies 146 no-encounter maps.

## Required implementation outputs

- A generated map/section/identity ledger under the exact J1 namespace.
- Focused save migration and identity tests, including current v1 digest fixture and full-bitset preservation.
- Script, trainer, wild, graphics, and layout closure reports.
- A real-input opening traversal and a root integration handoff with known risks.

## Known blockers to resolve as technical work

- Section capacity: 210 host entries, only 11 currently unreferenced, and approximately 55 new selected section names. Resolve through audited aliasing, safe recycling, or generated compaction before import.
- Standard trainer capacity: 1,478 current records plus 284 needed identities exceeds the 1,622 standard maximum. Prove the regional identity adapter or a SaveBlock1 trainer-flag expansion/migration; do not silently raise the constant.
- Registry digest compatibility: `CoopSave_Load` currently rejects a valid old digest. Add an explicit accepted-old-metadata migration path.
- Donor script closure: 52 event-script references are shared-host labels, nulls, or contamination; all require explicit adapter mapping.

## Suggested edit order

1. Generate the manifest and enforce map ID/group allocation.
2. Generate and review the u8 section ledger and `CoopRegion` mapping.
3. Append qualified registry identities and establish trainer defeated-state adapter.
4. Add save migration fixture and preserve fixed save offsets/CRC.
5. Import encounters and trainer parties.
6. Adapt scripts/flags/vars/specials/graphics and remove donor contamination.
7. Run focused gates, then hand the concrete results to root for independent review.

## Review boundary

This package is proposed and requires root independent review. It does not grant implementation approval, release readiness, merge authority, or campaign-complete status. No reviewer identity or pass result is asserted here.
