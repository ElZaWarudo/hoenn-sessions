# Mastery and badge battle caps

Pokémon keep their ordinary level, up to 100. After that, every 25,000 EXP earns
one mastery level, in this order:

| Mastery levels | Display |
| --- | --- |
| 1–100 | Alpha 1–100 |
| 101–200 | Beta 1–100 |
| 201–300 | Omega 1–100 |

Mastery does not increase the level used by stats, damage, evolution, or move
learning. The summary shows the full rank; party and PC lists use A, B, or O.
The EXP bar and next-level amount continue through mastery. Battle EXP, Exp.
Share, EXP Candies, Rare Candies, and daycare can advance it. Omega 100 is final.
Rare Candies advance to the next mastery level, including across rank boundaries.
They also retain the existing evolution check for eligible level-100 Pokémon.

## Temporary battle strength

Ordinary wild and trainer battles lower over-cap player Pokémon to the current
region's cap. Level-dependent evolution stages follow that cap too: a level-70
Charizard fights as a level-15 Charmander before the first Hoenn Gym victory,
then as a level-19 Charmeleon after it. Moves, held items, IVs, EVs, and nature
are retained. Types and species abilities follow the temporary species.

| Regional Gym victories | Battle cap |
| --- | --- |
| 0 | 15 |
| 1 | 19 |
| 2 | 24 |
| 3 | 29 |
| 4 | 31 |
| 5 | 33 |
| 6 | 42 |
| 7 | 46 |
| 8, before the regional championship | 58 |
| 8, after the regional championship | 100 |

The ROM uses region-specific Gym victories because Crossroads shares its generic
badge flags. Hoenn uses its Gym trainer victories, and Kanto uses its defeated
Gym Leader flags. Sevii follows Kanto's campaign progression. Kanto championship
completion has a persistent flag; Oak's existing post-League scene also recognizes
older champion saves. An older save that unlocked the National Pokédex before
winning Kanto and never ran that scene acquires the flag on its next League win.

Evolution edges without a numeric level requirement (such as stones or trading)
have no invented minimum level. An earlier numeric requirement in the same
lineage still applies.

The projection exists only during battle. EXP and move learning use the real
species and level, then the battle projection is reapplied before combat resumes.
On exit the real progression returns, including earned EXP. HP is converted
proportionally; fainted Pokémon stay fainted. Status, PP, and normal held-item
changes carry over. An untouched projection round trip preserves HP exactly.

Link, recorded, Frontier, Trainer Hill, e-Reader, raid, Safari, and tutorial
battles retain their own rules. NPC partners are not nerfed. This changes ROM
campaign battles; it does not activate the service's future cooperative battle
protocol or change its tier table.

## Save layout and configuration

`MAX_LEVEL` remains 100. Mastery EXP uses the existing low 21 experience bits
plus three formerly unused high bits in `PokemonSubstruct0`; the nickname bits
and all record sizes/offsets remain unchanged. Existing newly-created Pokémon
have zero in those spare bits. Keep mastery saves on a mastery-aware ROM: older
ROMs only understand the low experience bits.

`include/mastery.h` defines the mastery count and EXP cost.
`B_BADGE_BATTLE_CAP` in `include/config/caps.h` enables the temporary nerf.
The pre-existing EXP-cap settings remain disabled, so badge progression does
not limit permanent level or mastery growth.

## Verification

Build and run with the repository's ARM toolchain and mGBA test runner:

```sh
make -j6 check TESTS=Mastery
make -j6 check TESTS=Badge
make -j6 check TESTS=test/pokemon.c
make -j6 check TESTS=test/battle/exp.c
make -j6 check TESTS=test/battle/gimmick/dynamax.c
make -j6 modern
```

Tests cover save round trips and nickname preservation, all growth curves,
rank boundaries and overflow saturation, candy behavior, label width, regional
caps, evolution thresholds, HP/status/PP/item preservation, ordinary battle
integration, switching, real level-ups, catching, defeat, Dynamax HP restoration,
HP EV growth, daycare overflow, and mastery gained while capped.

The September 7, 2026 verification used GNU Arm 13.2 and the repository's
headless mGBA runner. Mastery passed 6 tests; Badge passed 20 tests (including
the existing badge-boost checks); ordinary EXP passed 6 tests. Pokémon data
passed 25 tests, with its existing learnset-size test marked `KNOWN_FAILING`.
The Dynamax suite passed 78 tests, with 3 existing `TO_DO` placeholders.
Counts exclude the additional parameter combinations within each test.

A preliminary Dynamax form-change failure was traced to a cached test object
using the old configuration enum. Forced recompilation resolved it; the same
check passed on the base revision as well. Regression validation uses refreshed
objects for the changed configuration layout.

The final combined run linked the test-runner sources and the five suites listed
above: 143 passed, 6 intentional `EXPECT_FAILING` cases, 1 existing
`KNOWN_FAILING` case, and 3 existing `TO_DO` placeholders (153 total). It exited
successfully with no unexpected failures.

Independent correctness, adversarial, testing, and maintainability review
completed. Follow-up review confirmed the HP-rounding, Dynamax HP, HP EV-growth,
Rare Candy evolution, and daycare overflow corrections with no remaining
actionable findings.

`make -j6 modern` completed successfully. The playable artifact is
`pokeemerald-mastery.gba` (32 MiB), SHA-256
`0969dc0971720fc20d1d12551925bed2927e69cff33fcb01c7e41d001a406af5`.
