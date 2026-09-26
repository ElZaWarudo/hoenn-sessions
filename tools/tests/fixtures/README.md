# Arrival save fixture

`arrival-v2.sav` is a 131,088-byte Flash1M save written through the Cormoria ROM's in-game Save menu and cold-reloaded by libmGBA. Its selected save slot resumes in Carabrue Town Home 1F (map 79:1, warp 255); SHA-256 is `b6493f464b5d114090ad9c6672be94ff852582f49ee3adaef1c4a09760ebc2ed`.

The fixture carries identity registry version 2. `test_rom_release_catalog.py` updates a copy's registry fields and CRC to the current version for synthetic catalog-contract tests; that copy is not a ROM-written arrival proof. Regenerate and pin distribution arrival images from the exact release ROM at each portal's intended arrival tile.
