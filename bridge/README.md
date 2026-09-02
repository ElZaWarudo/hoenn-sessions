# mGBA bridge

`main.lua` targets pinned mGBA 0.10.5's Lua scripting API with memory access, frame callbacks,
and TCP sockets. The script only connects to the loopback sidecar, authenticates
with a launcher-generated 32-hex-character ephemeral secret, then forwards
fixed 144-byte bridge frames. Lua runs while the emulated CPU is paused; queue
entries are copied before the producer publishes its `u16` write counter.

## Local Phase 2 checkpoint smoke

From the repository root, run the bounded shell-free local proof:

```text
python tools/smoke_phase2.py --local
```

The runner supports Windows and Linux for this child-boundary smoke and fails
before launching a target on other operating systems. It starts no remote
service and requires no Docker, Firebase, PostgreSQL, or credentials. The real
coop-server binary is started by its Rust integration
target on a literal `127.0.0.1` ephemeral port with explicit `phase2-local`
configuration. The launcher target uses the real `LocalSidecar`, real
bridge/control codecs, a deterministic fake cloud, and a temporary private
workspace while directly driving the launcher lifecycle. It does not start
mGBA or claim to exercise real-ROM timing.

The launcher/sidecar target writes and uploads a byte-accurate 128 KiB
PokéCrossroads `Flash1M` `character.sav`, not an opaque application DTO. Its
fixture contains the two rotated 15-sector slots and their real footer
signatures, counters, logical-sector IDs, and checksums; the selected slot
contains the frozen 672-byte `CSP1` v1 extension in `SaveBlock3`. The same
production parser validates the registry binding, four ordered regional
records, CRC, and `save_generation`, and extracts the selected slot's lineage
bytes before the checkpoint can finalize. The fixture is constructed at the
deterministic ROM/mGBA seam, so this proves exact container and lifecycle
interoperability without claiming that stock mGBA produced the bytes
interactively.

The bridge and control listeners use distinct per-process ephemeral secrets.
The launcher authenticates control first, then writes only the bridge address
and bridge secret to its private generated `session.lua`; never copy the
control secret into Lua or an environment variable. Do not print any secret or
token. A requested launcher shutdown drains at most one already-ready
authenticated checkpoint and never fabricates a new checkpoint. It then sends
one epoch-bound `shutdown_request` over the authenticated control channel and
accepts only a correlated `Applied` or `Replayed` result without a reason. The
sidecar records an accepted command before acknowledging it and can replay that
result across control reconnects within the same process. After acknowledgement,
the launcher gives mGBA one best-effort non-forced close opportunity, then
attempts Job termination and root reaping. Uncertain evidence retains recovery
material.

If interrupted, let the launcher perform child cleanup. Do not manually remove
a retained session directory or ownership marker when shutdown evidence
requires recovery. Reconnect must use the server-issued epoch and persisted
monotonic epoch record; never hand-edit or wrap it.

After linking a legal local ROM build, generate the address module and manifest:

```text
python tools/generate_bridge_manifest.py --elf pokeemerald.elf --rom pokeemerald.gba
```

The generated `dist/bridge_manifest.json` uses outer schema 3. In addition to
the NetBridge ABI and whole-ROM SHA-256, it binds the linked `SaveBlock3`
address, `CSP1` offset and size, generation and CRC offsets, the exact regional
identity-registry version and digest, and the official mGBA 0.10.5 Windows x64
Qt artifact. Its archive SHA-256 is
`b497a57c7d9093834dadc64f33a90f7c411439c21fdb8a0143255a45ea37563a`, and its
executable SHA-256 is
`5a3c98c2984dd04bd0d7c9378cdfae937ae0d73a196c880bb2eecf3b254af247`. The launcher
owns validation of that executable identity before probe/gameplay. Lua
receives only the generated address projection; it does not receive or
validate host executable digests. The embedded save schema remains `CSP1`
version 1, and manifest schema 3 does not rename or migrate the persisted
format.

## Canonical save and compatible-state lifecycle

The launcher writes the three distinct normalized absolute paths into private
`session.lua`. The bridge workspace uses these fixed, non-interchangeable artifacts:
`character.sav` is the canonical battery save, optional `resume.input.ss1` is
the previously verified state, and `resume.ss1` is reserved for a new capture.
At revision-zero bootstrap the launcher leaves `character.sav` absent; it does
not fabricate an erased save on the ROM's behalf. The first canonical file is
created through the ROM/emulator save path.
On script load, the bridge binds `character.sav` with writeback enabled, then
loads `resume.input.ss1` with mGBA state flags `29` (all compatible state except
savedata). A missing or rejected input state resets into the bound SAV instead.
The restored input is never used as the next upload output.

Every ROM `SAVE_DATA_UPDATED` payload is exactly one little-endian `u32`
`save_generation`. Lua withholds that frame until a `savedataUpdated` callback
after the matching checkpoint grant observes the same live generation through
the generated save manifest. It then completes one `resume.ss1` capture attempt
with flags `29` before forwarding the frame. A failed optional state capture is
removed and reported, but the canonical SAV completion can still proceed;
missing manifest metadata, legacy empty payloads, epoch mismatches, and stale or
skipped generations fail closed.

Before writing `bridge/session.lua` or starting the sidecar, the launcher MUST
compute the SHA-256 of the exact built ROM and compare it with
`game_build.rom_sha256` in `dist/bridge_manifest.json`. It must also validate
the selected mGBA executable against the manifest's official Windows x64 Qt
identity before probing or spawning it, and fail closed on either mismatch.
Lua validates the in-memory bridge ABI header, but it cannot hash the whole ROM
or host executable. The ROM hash embedded in the generated Lua address module
is therefore metadata, not runtime proof of the loaded ROM; emulator digests
are intentionally omitted from Lua and remain launcher-owned.

`game_build.id` is the canonical textual cloud build identity. The launcher
MUST read it from the generated manifest, and the server MUST sign resume
packages from a supported-build configuration generated from the same release
manifest. A client-supplied value is never a server trust root, and neither
process may offer a command-line override for this identity.
`game_build.numeric_id` remains the compact ROM bridge ABI value and is not a
substitute for the cloud identity.

The cloud server owns the durable, nonzero session epoch. The launcher persists
the greatest accepted server-issued epoch under a cross-process lock and starts
a new sidecar only after acquire or reconnect returns a strictly greater value.
Epoch exhaustion fails closed; it never wraps.

```text
coop-sidecar --session-epoch <nonzero-u32>
```

The sidecar refuses to start without this value and does not invent monotonic
state on the launcher's behalf.

The launcher must create ignored `bridge/session.lua` from the example, start
the sidecar, and then manually choose **Tools → Scripting → Load Script** in
stock mGBA 0.10.5 to load `bridge/main.lua` in the matching build. Do not rely
on an unsupported script-autoload flag. Never commit the generated session
file, ROM, save, savestate, or BIOS.

The `CSP1` payload keeps four ordered regional records for Hoenn, Kanto, Johto,
and Sevii. Each record owns its regional badge mask and story checkpoint;
trainer, gym, Fly-point, and event completion use fixed bitsets of append-only,
region-qualified registry ordinals. Readers reject unassigned ordinal bits,
badges not registered for that region, unknown status/reserved bits, registry
drift, and migration-ambiguous saves at online boundaries. The selected
`SaveBlock2` player bytes bind later revisions to one character lineage, but
they are not a secret and do not prevent a clone from becoming the first
accepted snapshot.

## Evidence boundary

The exact command outcomes for the 2026-09-02 local save-interop checkpoint are
recorded in
`docs/orchestration/runs/pokecrossroads-save-interop-20260902/save-interop-terminal-reconciliation.json`.
That checkpoint does not claim completion of the later product phases or full
MVP acceptance. The mGBA executable digest, creation-time Job containment, and
authenticated shutdown-to-forced-cleanup path are implemented. Still open are
interactive stock-mGBA ROM/Lua conformance, same-user-resistant atomic
staged-file cleanup, proof that a terminated Job has no active descendants,
production storage, gameplay-commit authority, provenance, and first-snapshot
anti-cloning. Process shutdown does not request a fresh checkpoint, so it is
not yet the complete product-level “Save and exit” workflow. Canonical details
are in `docs/swarm/blockers.yaml`.
