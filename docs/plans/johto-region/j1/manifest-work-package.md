# J1-RU1a — reproducible import ledger

Readiness: execution-ready for tooling only. Root narrowed this unit after
the independent J1 feasibility review confirmed the selection and rejected
the original single-group allocation. This does not accept the broader J1
runtime plan. User authority is the approved autonomous full-Johto mandate.

Own exactly `tools/johto/region_manifest.py`,
`tools/tests/test_johto_manifest.py`, and `data/johto/region_manifest.json`.
Implement a deterministic, pinned-donor manifest generator with `--donor`,
`--check`, and a normal generation mode. Read the corrected inventory and the
root decisions. Do not change runtime maps, scripts, graphics, constants,
registry, save data, or any existing importer.

Select the 239 donor maps using the reviewed inventory predicates, preserving
donor group order. Classify every one of the 954 donor registrations, including
each exclusion reason. New Bark remains group75/map0. Propose maps1..127 in
group75 and the remaining111 maps at group76/maps0..110. Reject duplicates,
missing source maps, unresolved layout references, out-of-range signed map
components, or any change to the existing host map identities. These are
proposed registrations; the tool must not install them.

Record all57 source section symbols. Preserve NewBark209, reserve210 and
253..255, and retain existing Kanto semantics for ReceptionGate by mapping its
donor MAPSEC_VICTORY_ROAD to MAPSEC_KANTO_VICTORY_ROAD. For other sections use
Johto-qualified symbols and assign new numeric IDs in first-selected-map order
within211..252. Use these explicit sub-area aliases (source suffix → target
suffix, both within Johto):

| Source | Enclosing location |
|---|---|
| OLIVINE_LIGHTHOUSE | OLIVINE_CITY |
| SPROUT_TOWER | VIOLET_CITY |
| BURNED_TOWER | ECRUTEAK_CITY |
| TIN_TOWER | ECRUTEAK_CITY |
| DRAGONS_DEN | BLACKTHORN_CITY |
| SLOWPOKE_WELL | AZALEA_TOWN |
| ROCKET_HIDEOUT | MAHOGANY_TOWN |
| SAFARI_ZONE | SAFARI_ZONE_GATE |
| CLIFF_CAVE | ROUTE_47 |
| EMBEDDED_TOWER | ROUTE_47 |
| TOHJO_FALLS | ROUTE_27 |
| UNION_CAVE | ROUTE_32 |
| DARK_CAVE | ROUTE_31 |
| MT_MORTAR | ROUTE_42 |
| ICE_PATH | ROUTE_44 |

Record aliases explicitly; no old section ID may be recycled. The resulting
ledger should contain41 distinct Johto sections including New Bark, plus the
existing Kanto section. Verify this result instead of silently forcing it.
Map aliases are a limited identifier allocation decision; later presentation
work remains responsible for clear cave/tower names.

For every selected map retain source map symbol, source group/index, proposed
host group/index, layout and tileset references, source and resolved section,
engine region, source map/script SHA-256 hashes, connections, and warp targets.
Classify each external edge as a required host adapter or excluded/debug edge;
never omit unresolved edges or replace them with a silent no-op. Preserve the
pinned repository and revision in the manifest. Missing dependencies must
fail with actionable errors.

Tests must cover deterministic output, total/excluded/selected counts, stable
New Bark and existing host identities, boundary indices127/128, all section
reservations, missing/duplicate references, donor-pin rejection, and `--check`
detecting a stale manifest. Use synthetic fixtures for failure cases. Run the
focused unittest module and the generator's `--check` against the pinned donor.
One focused independent tool review follows. No broad ROM build is needed for
this unit, because it changes no runtime content.
