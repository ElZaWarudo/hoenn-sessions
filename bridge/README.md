# mGBA bridge

`main.lua` targets pinned mGBA 0.10.5's Lua scripting API with memory access, frame callbacks,
and TCP sockets. The script only connects to the loopback sidecar, authenticates
with a launcher-generated 32-hex-character ephemeral secret, then forwards
fixed 144-byte bridge frames. Lua runs while the emulated CPU is paused; queue
entries are copied before the producer publishes its `u16` write counter.

After linking a legal local ROM build, generate the address module and manifest:

```text
python tools/generate_bridge_manifest.py --elf pokeemerald.elf --rom pokeemerald.gba
```

Before writing `bridge/session.lua` or starting the sidecar, the launcher MUST
compute the SHA-256 of the exact built ROM and compare it with
`game_build.rom_sha256` in `dist/bridge_manifest.json`. It must fail closed on a
mismatch. Lua validates the in-memory bridge ABI header, but it cannot hash the
whole ROM; the hash embedded in the generated Lua address module is therefore
metadata, not runtime proof of the loaded ROM.

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
the sidecar, and then load `bridge/main.lua` in the matching mGBA build. Never
commit the generated session file, ROM, save, savestate, or BIOS.
