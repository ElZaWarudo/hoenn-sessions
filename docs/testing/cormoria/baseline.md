# Cormoria integration baseline

Measured on 2026-09-22 before Cormoria engine or content changes.

## Source and build

- Host revision: `ea4bbc060b74b6418ac3fcaa0008cd62b1f83dce` (`main`).
- Host tree: `bfdf34e57ae4241fb9f8697d779a7519875988cd`.
- Donor revision: `f7997186345885bfa23a170e5f573851fc034b9b`.
- Canonical branch: `codex/dreamstone-region`, in the managed `dreamstone-region-assessment` worktree.
- Build input: `git archive` of the host revision, extracted on the native WSL filesystem. No Cormoria files were in this snapshot.
- Archive SHA-256: `091ad5ecc957da60ace7594c1f3488966957c2feb2f74f0c7072966789f69297`.
- Environment: Ubuntu 24.04.3 under WSL2; ARM GCC `13.2.1 20231009` (`15:13.2.rel1-2`), GNU Make 4.3, Python 3.12.3.
- Commands: initial `make -j4 modern`; independent persistent rebuild `make -j8 modern`.
- Result: successful compilation, link, binary extraction, and ROM padding.

The initial build used `/tmp/cormoria-main-ea4bbc06` in WSL Ubuntu. WSL restarted after the build and cleared `/tmp`, so subsequent builds use `/home/teb/codex-builds/` and copy evidence into the Windows task scratch directory before returning. Build snapshots are not additional Git worktrees. A second build in `/home/teb/codex-builds/cormoria-main-ea4bbc06` reproduced the exact ROM and ELF hashes and resource measurements below. Its ROM, ELF, map, full build log, and hash report are retained in `C:\Users\Mayor\AppData\Local\Temp\dreamstone-assessment-bfce03f1\baseline-artifacts`.

## Linked resource usage

| Resource | Used bytes | Limit bytes | Remaining bytes | Used |
|---|---:|---:|---:|---:|
| ROM | 31,597,600 | 33,554,432 | 1,956,832 | 94.17% |
| EWRAM | 243,352 | 262,144 | 18,792 | 92.83% |
| IWRAM | 28,648 | 32,768 | 4,120 | 87.43% |

The ROM file is padded to 33,554,432 bytes. Its file size therefore does not measure occupied ROM space; use the linker table for import deltas. The ELF is 42,567,952 bytes including its non-ROM sections and symbols.

| Artifact | SHA-256 |
|---|---|
| `pokeemerald.gba` | `095a07644f31a4f802c8e617666717504eb1c47a5eb27a77708bdcad4e1ab0ca` |
| `pokeemerald.elf` | `f1d6af5a18135514e7ce3791202a0b1a7172e97ab0f2010ed37b4b4c17348c39` |

This leaves a narrow ROM budget. Inventory raw assets and exact-byte host duplicates, then measure compressed/linker deltas before live registration. Neither the donor's source size nor the older Johto preview build establishes Cormoria fit.

## Initial regression and runtime capabilities

The assessment exercised 41 Python test methods across regional catalog/identity and Johto region/manifest suites. Two map-header methods initially lacked the generated `map_groups.h` header in the fresh checkout. After generating the ignored map headers from a scratch copy of the host map corpus, all six Johto region methods passed; the other methods passed in the initial run. No tracked host content was changed to obtain the baseline.

`cargo test -p coop-protocol -p coop-save --locked` passed before Cormoria runtime changes: 30 protocol tests and 23 canonical-save tests, with no failures or ignored tests. This characterizes region ordinals, registered identities, travel contracts, rotated save sectors, CRCs, strict parsing, and existing real-save fixtures before the five-region/schema changes.

The bundled Linux `tools/mgba/mgba-rom-test` executes under WSL and exposes the ROM test runner used by `make check`. This capability probe is not a gameplay test result.

The Computer Use skill's `@oai/sky` runtime initializes and lists Windows applications, including installed mGBA. Native emulator observation is available through this API; actual rendering and input checks remain integration work.

The source contracts in `tools/generate_bridge_manifest.py` and `coop/crates/coop-launcher/src/compat.rs` pin **mGBA 0.11.0**, build `0.11-9139-3a5bc2462`, executable SHA-256 `743157a16a1cb478a2b45e6e20e9a482ea397c3820d7e8e27b1e048e85bd5546`. Its official archive was downloaded into task scratch space and verified against archive SHA-256 `ea7cc0e8632cd80d28bdb55e37aacc58b2b018f564209f790e8cc3caed8c002b` and the executable hash. `--version` reports the expected build and `--help` confirms `--script` startup support. The installed `Program Files` emulator is 0.10.5. The checked-in bridge manifest and older conformance instructions also still name 0.10.5; they are not the authority for the current source's runtime. Regenerate the manifest against the integrated ELF/ROM and use the verified source-pinned runtime for two-client checks.

No Cormoria ROM, campaign traversal, save round trip, or two-client acceptance is claimed by this baseline.

After the user selected automatic switching between two ROMs, the combined-ROM import was stopped. A baseline `make -j8 check TESTS='Johto section*'` run was interrupted during dependency generation; it supplies no ROM-test pass or failure evidence. The completed baseline builds and Python/Rust results above remain valid.

## Original Dreamstone reference build

After the two-ROM decision, `git archive` of donor revision `f7997186345885bfa23a170e5f573851fc034b9b` was built separately in `/home/teb/codex-builds/dreamstone-f7997186` with the same ARM GCC toolchain and `make -j8 modern`. Compilation, linking, binary extraction and padding succeeded. The donor emitted existing compiler warnings, including unused variables/functions in its menu/summary code; the build was not warning-free.

| Resource | Used bytes | Limit bytes | Remaining bytes |
|---|---:|---:|---:|
| ROM | 29,157,380 | 33,554,432 | 4,397,052 |
| EWRAM | 229,648 | 262,144 | 32,496 |
| IWRAM | 28,357 | 32,768 | 4,411 |

| Artifact | SHA-256 |
|---|---|
| Original donor `pokeemerald.gba` | `955e76b5aa503f5167f19d4d57f8b526f9338d58ecaee988976246dd79f921c2` |
| Original donor `pokeemerald.elf` | `966a2afe961444a3a1c8021b6e4f67f2d993fd1338bd03b36a03bbbdb85502b3` |

Artifacts and the full log are retained in `C:\Users\Mayor\AppData\Local\Temp\dreamstone-assessment-bfce03f1\dreamstone-artifacts`. This is the unchanged donor reference, not the integrated Cormoria ROM. Its successful build does not establish save compatibility, co-op compatibility, shared menus, or cross-ROM travel. The agreed implementation combines common engine systems and builds separate world binaries.

## Split-world generator checkpoint — 2026-09-23

The pinned donor inventory passed all 15 tests and a fresh `--check`: 165 maps/layouts, 51 sections and 59 tilesets. Its `runtime_ready` field remains false. The inventory is committed at `ae19f1933e`.

The split-world generator is committed at `bc15ca2fc0`. Five tests compiled the generator and assembled its output for both world selections. Map and layout IDs retain their slots; main, Cormoria and shared content produce the expected table entries. Invalid world metadata is rejected. The default world build with `ROM_WORLD=1` completed in `/home/teb/codex-builds/cormoria-build` using `make -j8 modern`; linked usage remained ROM 31,597,600, EWRAM 243,352 and IWRAM 28,648 bytes. Its padded ROM SHA-256 is `8bd4ca588f1a4e4fa15ab350ee37b4072900e3e1444108080771c0af2b1ea266` and ELF SHA-256 is `4f79d6f9f2870aece60185263eee4f040f2d6f269601c1c8f66927d11ec32ee3`. Those hashes differ from the unchanged baseline. The emitted map/layout content for world 1 was text-equivalent after stripping the new assembly guards; linked symbol differences begin in ARM veneer ordering. This is a resource regression check, not byte identity.

The Cormoria build profile selects its own output name and ROM header identity, but the region has not yet been imported. A bootable Cormoria campaign, safe foreign-map handoff, new traveling V2 saves and co-op travel are still pending; do not distribute a world-2 build from this checkpoint.

## Registry-backed Cormoria profile baseline — 2026-09-23

The native WSL snapshot at `/home/teb/codex-builds/cormoria-wide-header-734a` contains the verified wide-section and world-registry patches, the region-map predicate correction, and the Makefile recursion correction. It predates the later account/download portal merge, which did not touch ROM sources. A first `make -s -j8 modern ROM_WORLD=cormoria` attempt exposed that the top-level generation recursion omitted `ROM_WORLD`; the parent and child repeatedly rewrote `.map_version` as worlds 2 and 1. Passing the resolved world bit into that recursion removed the loop. The rerun compiled and linked `pokeemerald-cormoria.gba` successfully.

The linked Cormoria-profile ROM uses 29,249,476 of 33,554,432 bytes (87.17%), leaving 4,304,956 bytes before donor content. EWRAM uses 243,444 of 262,144 bytes and IWRAM uses 28,648 of 32,768 bytes. The ROM header title is `CORMORIA`, game code `BPCO`; padded ROM SHA-256 is `db6b517eeb4a9dee6157019482f8c28deb29c9988b973f5caecfc50912203908`, and ELF SHA-256 is `2b90dbd3f81b83b42b0361759f01e4213aba3c7de015548100942f9f16533ac2`.

This is a build-profile and resource baseline. The donor's 165 maps and quest runtime remain unregistered, and no cross-ROM travel has been observed.

## Main synchronization — 2026-09-23

The Cormoria branch was rebased onto `origin/main` at `4fb7f5060e` (40 upstream commits beyond the original base), then local `main` was fast-forwarded to the same commit. The donor inventory was regenerated because upstream changed `include/constants/event_objects.h`; its 135 affected shared-roster binding records now carry the new host header hash. The map, layout, section and tileset counts did not change, and the pinned-source `--check` passed. No donor source revision changed.

On the synced tree, the five split-world generator tests pass, all 36 `coop-protocol` tests pass, and `cargo check --workspace --locked` passes. The default-world `make -j8 modern` build links with ROM 31,602,808/33,554,432, EWRAM 243,440/262,144, and IWRAM 28,648/32,768 bytes. This is 5,208 ROM bytes and 88 EWRAM bytes above the earlier feature build, after upstream changes and the reserved Cormoria region label. The padded ROM SHA-256 is `5ff03a58954e25788f3163efa7b7e49939951038315a8bb96001bce605d51abd`; ELF SHA-256 is `c590a878665808183017ba952d1bdddb5e333679d67979403ec278386d0043a4`. Build artifacts are in `C:\Users\Mayor\AppData\Local\Temp\dreamstone-assessment-bfce03f1\feature-artifacts\synced-main-default`.
