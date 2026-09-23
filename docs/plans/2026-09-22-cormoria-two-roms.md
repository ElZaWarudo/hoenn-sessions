---
title: Cormoria travel between two ROMs
type: feat
date: 2026-09-22
execution: code
---

# Cormoria travel between two ROMs

## User contract

Entering Cormoria automatically switches to the Cormoria ROM. Leaving switches back to the existing-world ROM. Share the player's identity, Pokémon (party and PC), items, common menus/settings, and co-op identity. Preserve each world's quests, maps, puzzles, trainers and return position independently. The user's latest instruction requires preserving existing save files and progress, superseding the earlier permission to require new saves. Keep the whole Cormoria campaign in its own ROM; do not put both worlds into one binary or require bank-switching hardware.

This replaces the single-binary strategy in [the combined-ROM plan](2026-09-22-1124-feat-cormoria-region-plan.md). Its pinned source inventory and shared-engine compatibility findings remain useful. Work remains in the existing `codex/dreamstone-region` worktree. No deployment, merge or publication is authorized by this plan.

## Implementation decisions

- The user chose to combine the best parts of both games. Build both ROMs from a shared Hoenn Sessions engine: retain its co-op, Pokémon roster and progression; adapt Dreamstone's expanded bag, quest journal and regional adventure. Menus, item definitions, Pokémon definitions and save structures must agree across both builds. World content is selected at build time.
- Keep Dreamstone's campaign source pinned at `f7997186345885bfa23a170e5f573851fc034b9b`, with explicit import transformations and reproducible build inputs. Preserve its campaign and source attribution. The original donor build is a behavioral reference, not a compatible drop-in second ROM.
- Treat each ROM and its matching save/bridge descriptor as a separate verified artifact. A shared release identity must not pretend that the ROM hashes or memory addresses are identical.
- Keep one active player authority and one regional save per ROM. Switching is a checkpointed transaction, not copying one game's entire save over the other.
- Preserve original saves untouched. Any format migration operates on a separate copy, with explicit version detection, integrity checks and verified preservation of existing player and regional progress. Never initialize over an incompatible save. Migration failure leaves the original usable by its original ROM; support is not established until tested with existing-save fixtures.
- Translate donor identifiers into the shared engine at import time. The original host and donor have different save-sector layouts and item-number collisions; the two released ROMs must instead share a verified player-data contract. Transfer only the common player fields between their regional saves; never import source-engine raw saves or silently discard unsupported data.
- Common menus and gameplay definitions must be reconciled explicitly. Persist regional quest/key-item effects with their owning campaign. Shared inventory must retain the origin and meaning of every item.
- Stop the old emulator before activating the destination. Commit arrival only after destination import, save and bridge verification succeed. Failed or interrupted transitions retain the departure save and a recoverable transaction.
- Preserve co-op character/group identity while replacing the ROM-specific bridge and presence location. Server compatibility must distinguish a compatible release family from exact ROM identity.

## Execution units

1. **Verify sources and split world builds.** Build the pinned donor as a reference and finish the campaign dependency inventory. Add separate world build profiles with stable map/layout identities and explicit absent-map handling. Both released profiles share the same engine and player-data definitions; each must fit the 32 MiB limit independently.
2. **Integrate the common systems and Cormoria.** Adapt the full campaign, quest runtime, native features and items into the Cormoria build. Share the expanded bag, menus, Pokémon systems and five-region save/co-op contract. Implement authenticated save-layout descriptors, player-field transfer validation and a recoverable transfer record. Prove inventory, party, PC, fusion storage and mail round trips without changing either world's story bytes. Test mismatched identities, unsupported values, corrupt saves and interrupted writes.
3. **Connect travel and co-op.** Route a portal request through checkpoint, shutdown, destination save preparation, ROM launch and arrival acknowledgement. Add the Cormoria bridge/catalog and compatible release-family checks. Connect common menu entry points and settings; preserve regional quest menus.
4. **Verify the player flow.** Exercise entering, catching a Pokémon, acquiring an item, saving and returning. Observe both ROMs and two-client presence/reconnection. Inject a failed launch and confirm recovery. Report actual observations separately from fixture tests.

## Completion evidence

Both ROMs build from pinned inputs. A real round trip retains shared player data and independent campaign progress. Co-op reconnects to the correct world without duplicating a live player. Failure recovery loses neither Pokémon nor items. Changes have independent code review and focused regression coverage. Merely generating a manifest, writing a converter, or passing mocked orchestration tests does not establish completion.
