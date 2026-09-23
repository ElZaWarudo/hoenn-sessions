# User-requested pause — 2026-09-22; resumed 2026-09-23

The user requested “pause when you can,” then explicitly said “Continue” on 2026-09-23. The pause ended. This file preserves the earlier checkpoint; current verification is appended to `baseline.md`. Later on 2026-09-23 the user removed legacy-save migration from scope while keeping lossless player-data transfer on every trip between ROM regions. The active plan supersedes older save-preservation instructions below.

## Settled direction

Automatic travel between ROMs, sharing Pokémon, items, menus and co-op. The user chose to combine the best parts of both games. The implementation direction is a shared Hoenn Sessions engine/roster/progression/co-op, plus Dreamstone's expanded bag, quest journal and Cormoria content, with separate world-content builds. Legacy saves need not migrate; new V2 characters must retain all shared player data when traveling in either direction. The active brief is `docs/plans/2026-09-22-cormoria-two-roms.md`; the older combined-ROM plan is superseded.

Canonical worktree: `C:\Users\Mayor\.codex\worktrees\dreamstone-region-assessment\hoenn-sessions`, branch `codex/dreamstone-region`. HEAD remains `7a95b80b18` (the original plan-only commit); subsequent work is uncommitted. Leave the unrelated original checkout untouched.

## Preserved work and evidence

- The original host and original pinned Dreamstone ROMs both build successfully. Exact metrics, hashes and artifact paths are in `baseline.md`. The donor ROM is a reference, not the integrated second ROM.
- U1 tooling and canonical inventory are under `tools/cormoria/`, `tools/tests/test_cormoria_manifest.py` and `data/cormoria/`. They describe 165 maps/layouts, 51 sections, 59 tilesets and planned dependencies. No runtime maps or assets have been imported.
- A previous U1 bundle passed 13 tests plus CLI `--check`. Independent U4 dependency review then found missing fixed native calls/constants inside selected macros and missing audio inside native minigame callbacks. The worker added regression tests, observed failures, implemented fixes and reported 12 focused tests passing. The regenerated bundle still requires the final post-fix corpus/check verification and independent follow-up review; do not treat the earlier 13-test result as verification of the final files.
- Root prototyped world selection in an isolated scratch source copy: `C:\Users\Mayor\AppData\Local\Temp\dreamstone-assessment-bfce03f1\world-build-probe`. Only its `tools/mapjson/mapjson.cpp` and `tools/tests/test_rom_worlds.py` contain the new behavior. Five tests passed after observed failures. They assemble actual generated tables for main/Cormoria/shared membership, preserve map/layout slots and constants, omit other-world data, and reject invalid world metadata. This prototype has NOT been copied to the canonical checkout, connected to Makefile profiles, independently reviewed, or tested in a linked ROM.
- The reference Dreamstone build is at `/home/teb/codex-builds/dreamstone-f7997186` in WSL; its retained Windows artifacts are in the task scratch directory's `dreamstone-artifacts`. Host baseline and feature caches are `/home/teb/codex-builds/cormoria-main-ea4bbc06` and `/home/teb/codex-builds/cormoria-build`.
- Root scratch scripts `snapshot-changes.py` and `build-feature.sh` can synchronize the task's delta into the native-filesystem build cache. Use unique phase names. Avoid `/tmp`, which WSL previously cleared on restart.

## Resume boundary

First inspect status and any final worker checkpoint. Finish post-fix U1 validation, inspect its actual files and integrate a path-limited commit. Then finish and integrate the split-world prototype, including Makefile selection, required shared maps, unavailable-map handling and a default-world regression build. Do not publish a Cormoria build with an empty world as a finished feature.

The shared bag/save/section changes, full campaign adaptation, launcher handoff, common menus and observed co-op round trip are still outstanding. Earlier U2/U3/U4 readiness notes exist in task history; update them for the agreed shared-engine, new-V2-save design before implementation. The final save layout must account for the expanded bag before freezing offsets.

Final worker checkpoints: U1 completed regeneration, then stopped its active verification; no process remains. The latest inventory has 122 native bindings, 172 audio identities and 195 allocated trainers (one added through macro discovery). Final corpus/check, allocation review and independent follow-up remain pending. U3 stopped its read-only shell without edits: expanded bag adds 540 bytes, provisionally leaving 2,792 bytes in the current final sector for Cormoria state. U3 also found a stale mGBA 0.10.5 pin in the server save admission code and a donor eight-array/seven-pocket-count inconsistency. These are investigation findings, not implemented fixes.

No automatic ROM switching, integrated Cormoria gameplay, player transfer or two-client acceptance has been implemented or claimed. No PR, push, merge or deployment has occurred.

## Resumed work

The final donor inventory verification passed (15 tests and pinned-source `--check`) and was committed at `ae19f1933e`. The split-world map/layout generator passed five assembled-output tests; the default world linked with unchanged resource usage and was committed at `bc15ca2fc0`. The full campaign, lossless travel for new V2 characters, and co-op integration remain open.
