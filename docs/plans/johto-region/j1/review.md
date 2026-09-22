# J1 proposal review packet

Review status: **pending independent root review**.

This nested role prepared an implementation-ready proposal from the sealed host baseline and pinned Johto donor. It has not reviewed or certified its own implementation, because no implementation was performed and the parent owns independent reviews.

## Evidence available to the reviewer

- Donor inventory: 954 map registrations, 878 layouts, 176 referenced tilesets, and 518 map-name collisions.
- Selected closure: 239 registrations — 35 towns/routes, 87 town interiors, 25 Johto route interiors, 72 dungeons, 18 special areas, and two Route 26 houses.
- Host identity/capacity: 210 region sections with a u8 map-header field; 1,478 trainer records with max 1,622; 912 cooperative identities; fixed 672-byte cooperative save with 2,048 trainer and event bits.
- Selected script closure: 284 unique trainer symbols, 245 trainer event records, 243 distinct scripts, two battle-set placeholders, 354 flag symbols, 72 vars, 149 wild records over 93 maps, and 146 intentionally no-encounter maps.
- Host map group 75 already contains `NewBarkTown`; preserving index 0 and appending 238 registrations leaves 17 byte-range reserve slots.
- Host section reference audit found only 11 currently unused IDs, making blind append of approximately 55 new Johto section names unsafe.

## Independent review questions

1. Does the map manifest classify every one of the 954 donor registrations, select exactly the 239 required registrations, and preserve the Kanto continuity graph?
2. Is the chosen section alias/compaction ledger semantically safe for old Kanto/Hoenn maps and `CoopRegion_FromSectionId`, with every selected map below 256?
3. Does the trainer identity adapter provide battle records and defeated-state storage for all 284 identities without shifting old trainer flags or corrupting SaveBlock1?
4. Does registry migration accept the current v1 digest, preserve every old cooperative and engine save field, initialize new bits deterministically, and reseal CRC?
5. Are generic donor badge/var symbols, contamination scripts, Trainer Hill warps, New Bark preview links, and Kanto donor maps all removed or intentionally adapted?
6. Do focused tests and a real-input traversal demonstrate New Bark opening, at least one Johto route transition, and the Kanto border continuity?

## Decisions requiring root disposition

- Section byte capacity must be resolved as technical work before content import. The proposal recommends audited aliasing/compaction over changing the header width.
- Trainer standard-slot overflow must be resolved through the qualified regional identity adapter or a reviewed SaveBlock1 migration. A constant-only increase is rejected.
- Registry metadata changes require compatibility migration; leaving `CoopSave_Load` as exact-digest-only is incompatible with existing valid preview saves.

## Review outcome

No pass, readiness, release, or implementation-success claim is made. Root should review the actual six artifacts and return a decision through the bounded nested workflow.
