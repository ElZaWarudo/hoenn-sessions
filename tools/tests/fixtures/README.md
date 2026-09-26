# Arrival save fixture

`arrival-v2.sav` is a 131,088-byte Flash1M save written through the Cormoria ROM's in-game Save menu and cold-reloaded by libmGBA. Its selected save slot resumes in Carabrue Town Home 1F (map 79:1, warp 255); SHA-256 is `b6493f464b5d114090ad9c6672be94ff852582f49ee3adaef1c4a09760ebc2ed`.

The fixture carries identity registry version 2 and remains a historical save sample.

`arrival-v3-cormoria-carabrue.sav` is a 131,088-byte registry-v3 Flash1M save written through the current Cormoria ROM's in-game Save menu and cold-reloaded by pinned mGBA 0.11. It was seeded from a synthetic registry-v3 adaptation of the older fixture, then overwritten by the ROM; it is not a fresh-new-game save. The source ROM is the clean two-world build at `f59cc59416`, Cormoria ROM SHA-256 `11c510cec9e03ce84a713aa982eea70da08619d30dd7da5c199cff4836a9326c`. The save SHA-256 is `c17f39cd98342281ab0de668a6b9daa83f264f058ac642a55d5b5543f90e6765`. `verify_arrival_save` accepts registry v3 and map 79:1, warp 255. Catalog contract tests use this immutable image instead of modifying a save's registry bytes in Python.

This Carabrue save is a test fixture, not a release arrival template for the Rivetshore harbor portal. Generate distribution arrival images from the exact release ROM at each portal's intended arrival tile.
