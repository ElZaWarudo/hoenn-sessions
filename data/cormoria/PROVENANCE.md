# Cormoria content inventory

The source is [Dreamstone Mysteries](https://github.com/dsmyst/dreamstone-mysteries), pinned to `f7997186345885bfa23a170e5f573851fc034b9b`. The first six campaign groups supply all 165 required maps. Their order is retained as 28, 32, 40, 31, 30 and 4 maps.

This package inventories content for two world-content ROM builds using the shared Hoenn Sessions engine. It does not register maps, implement runtime adapters or copy an entire donor engine. Pokémon species constructors bind explicitly to the shared host roster and overworld art. Ordinary donor NPC graphics, objects, tilesets, music and effects retain their source dependencies.

`region_manifest.json` records maps, layouts, sections, tilesets, external destinations, gameplay data and resource evidence. `symbol_ledger.json` records namespaced identities, source references, assembly labels, macros and native adapter obligations. `source_manifest.json` records SHA-256 hashes, owners and asset conversion inputs. `DONOR_CREDITS.md` preserves the pinned upstream credits; the pinned upstream README is also retained in the source hash inventory.

Run from the repository root:

```powershell
python -m tools.cormoria.region_manifest --donor C:/path/to/dreamstone-mysteries
python -m tools.cormoria.region_manifest --donor C:/path/to/dreamstone-mysteries --check
$env:CORMORIA_DONOR = 'C:/path/to/dreamstone-mysteries'
python -m unittest tools.tests.test_cormoria_manifest
```

The donor must be at the exact revision with a clean worktree. Output contains no machine-specific donor path or timestamp. Each input must be tracked by the donor. Compiled `scripts.inc` supplies executable script evidence; `.pory` is retained only as review evidence.

Map identities append groups 79–84 to the pinned 79-group, 1,344-map host baseline. The checker also accepts the exact planned Cormoria tail after registration. Sections reserve 250–300; flags start at `0x8000`, persistent variables at `0x9100`, trainers at `0x5000` and heals at `0x0300`. Numeric aliases retain one identity. These logical allocations require the corresponding runtime dispatch and save representation before use.

Native implementation files and their include graph are evidence at explicit semantic adapter boundaries. Their inclusion in the source manifest is not a request to import the engine files. Planned native bindings, quest-command translation, script-controlled graphics slots and external service bindings remain visibly unimplemented. `runtime_ready` therefore remains `false`.

Asset byte counts measure source files. Exact source-byte matches identify potential host storage reuse; they do not establish converted or linked ROM size. Each final world binary still requires its own ROM, EWRAM and IWRAM measurements.

The cable-car escape override copied from Hoenn is explicitly mapped to Cormoria Route7 `(11,7)`, below its station entrance at `(11,6)`. `CableCarWarp` preserves the donor's Pelluca/Mirroh station destinations and requires a Cormoria adapter because the host service uses Hoenn destinations. These mappings require observed travel and escape verification before runtime acceptance.
