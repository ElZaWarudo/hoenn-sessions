# New Bark Town compatibility import

This first Johto slice imports New Bark Town's outdoor scenery. Talk to the
gentleman at the right of Oldale's Pokémon Center counter to visit. The guide
beside New Bark's town sign returns you to the same Pokémon Center. Declining
either trip leaves you in place. Save/load uses the existing character and save
format; no badge, story, starter, or respawn flags are changed.

Buildings and onward routes are closed. There are no Johto encounters, gyms,
story events, Fly destinations, or server group-travel routes in this slice.
The Town Map still uses the existing Hoenn display; a Johto regional map is a
later integration task. This is a compatibility preview, not a Johto campaign.

## Source and attribution

Scenery comes from [Pokémon Heart & Soul](https://github.com/PokemonHnS-Development/pokemonHnS)
at commit `751823abaf677020bcd72c45fe3e7cb2b8a576e4`. Its README invites developers
to modify and expand the project. Preserve its upstream credits when reusing or
distributing this work; that invitation does not independently settle every
third-party asset's redistribution terms.

Credit Lil Dill and the Heart & Soul development team; pret and Resetes12's
Modern Emerald; and the HnS-listed scenery contributors: Crystal Advance
(Kertra), Ekat99, TheDeadHeroAlistair, the Johto Redrawn Team, Fire Gold
(blackfragrant), and SkidMarc25. These are HnS's collective tileset/mapping
credits, not a claim that each authored every imported file.

The [expansion port](https://github.com/smithk200/Gold-And-Silver-Gen-3-Decomp)
was evaluated at `4df7d5a50ca6296201a4c17831c8cef730660fb7`. Its three-layer
metatiles require renderer changes. The original's two-layer scenery is the
smaller compatible donor; no expansion-port assets are included here.

## Adaptation

- Append the map group, layout, and section so all existing playable IDs stay
  unchanged. Johto map-header byte is explicitly 2; protocol region remains 3.
- Use Crossroads' FRLG layout format for HnS's 640 primary tiles/metatiles and
  seven primary palettes. This is a layout format choice, not a Kanto region.
- Convert Emerald 16-bit metatile attributes into FRLG 32-bit attributes,
  preserving behavior and layer placement. Map HnS's Headbutt tree behavior
  `0xA1` to inert terrain: that number means an Indigo Plateau sign here.
- Keep original map, border, tiles, metatiles, and palettes. Use static scenery
  and existing Littleroot music. New guide scripts replace donor story scripts.
- Normalize New Bark explicitly in both ROM presence and the protocol catalog;
  reject contradictory region/section pairs. No other Johto location is enabled.

`python tools/johto/import_new_bark_assets.py` reproduces the scenery import from
the pinned source. `data/johto/new_bark_source.json` records source and converted
SHA-256 hashes. The script overwrites only the listed imported assets and manifest.

## Verification commands

```text
python -m unittest tools.tests.test_new_bark_import tools.tests.test_johto_region
python tools/coop/generate_regional_catalog.py --check
cargo test -p coop-protocol --lib --locked
make modern
```

## Verified result (2026-09-08)

- Production ROM compiled successfully with ARM GCC 13.2.1: 28,644,460 bytes
  used (85.37% of ROM capacity).
- All 10 Python import/map tests and 27 Rust protocol tests passed. Regional
  catalog and identity generation checks passed.
- All 33 focused region/presence engine tests passed under the headless mGBA
  test runner, including the new Johto identity cases.
- Independent review findings were addressed. A separate check confirmed all
  209 existing map-section entries retain their original values and encoding.
- A render from the imported tiles, palettes, and map was visually inspected.
  This was an asset render, not an emulator playthrough.

The local build is `pokeemerald-johto-preview.gba` (32 MiB padded), SHA-256
`ce038956729cb026e97e43393f6d346fb9a54778e50a26d333179811b9d1f964`.
It was built from the current workspace, including the existing gameplay work.
Manual travel, save/reload, and live multiplayer acceptance remain unverified.

Focused engine test command:

```sh
make check -j6 TESTS=Cloud "TEST_SRCS_IN=test/test_runner.c test/test_runner_args.c test/test_runner_battle.c test/test_test_runner.c test/coop/region.c test/coop/presence.c"
```

Runtime acceptance: enter via Oldale, decline and accept the return trip, walk
around the town, inspect the closed doors, save/reload in New Bark, then return.
Before claiming online readiness, also validate two-player presence and server
admission with the new catalog and rebuilt ROM compatibility manifest.
