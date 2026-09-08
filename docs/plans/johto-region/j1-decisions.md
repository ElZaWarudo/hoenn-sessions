# Johto foundation decisions — root integration record

Status: implementation constraints; independent feasibility review pending.
These refine the approved full-region scope without changing its acceptance
criteria. No campaign implementation or migration is certified by this record.

## Map identities

The selected donor predicates reproduce 239 maps and 57 distinct source section
symbols. Preserve every existing map registration, including New Bark at 75:0.
Split new registrations into appended groups with at most 128 maps per group:
`WarpData.mapGroup` and `mapNum` are signed bytes, so 239 registrations in one
group is invalid. Reject any registration or resolved warp above 127 in either
component. Do not renumber existing groups to fit the donor.

Preserve section IDs 0–209, reserve the current no-location value 210, and keep
253–255 reserved for special Pokémon met locations. Allocate only 211–252 for
new locations. Use explicit aliases for sub-areas to their enclosing town or
route when needed; do not recycle old locations. The final allocation ledger
must name every source section and prove the bound before runtime import.
New locations above 210 require replacing existing `id < MAPSEC_NONE` validity
checks with a bounded validity predicate that still excludes 210. The popup,
Pokédex area display, region map names, and Pokémon summary are consumers.

## Persistent state

Keep existing SaveBlock1 fields, trainer flag ranges, and cooperative ordinals
unchanged. A separate Johto trainer-table adapter is preferable to increasing
`MAX_TRAINERS_COUNT`, which would move system flags and collide with partner
trainer IDs. Audit all direct trainer-table consumers and offline fallback;
unrecognized Johto IDs must never reach legacy flag-pointer arithmetic.

For Johto variables and offline campaign state, investigate an append-only
SaveBlock1 extension before reusing unrelated storage. The current production
ELF reports 0x3f88 bytes for `SaveBlock1ASLR`, including 128 ASLR bytes, hence
0x3f08 bytes of actual SaveBlock1 against five 3968-byte sectors. This is a
capacity observation, not permission to change old checksums blindly. The
writer clears sector padding, which suggests an appended zero-initialized
extension can retain old-save checksums. The implementation must verify that
with old-save fixtures and static offset assertions, and must not reinterpret
corrupt or incompatible saves as fresh Johto progress. No SaveBlock1 field
may move and no existing party, PC, money, item, flag, or variable may reset.

Registry history is already append-only and versioned. Generate compatibility
metadata from its immutable lock snapshots; accept only known historical
version/digest pairs with valid CRC and historically assigned bits. Preserve
all previous assignments and values. Unknown metadata and corrupt bodies must
still fail closed. Resealing an old valid record with the new metadata must not
erase progress or silently enable online play for an ambiguous legacy save.

## Delivery boundary

First freeze a reproducible map/section/dependency ledger and its checks.
Then implement and independently validate the save/identity seams. Only after
that foundation passes can map, trainer, encounter, and campaign import use it.
Local checkpoint commits are authorized. There is one final local release
phase after full campaign, Kanto-link, navigation, and co-op acceptance.
