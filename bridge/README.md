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

The launcher owns the persisted session-epoch counter. Increment it by one for
each new sidecar process using wrapping `u32` arithmetic, skip zero, then start:

```text
coop-sidecar --session-epoch <nonzero-u32>
```

The sidecar refuses to start without this value and does not invent monotonic
state on the launcher's behalf.

The launcher must create ignored `bridge/session.lua` from the example, start
the sidecar, and then load `bridge/main.lua` in the matching mGBA build. Never
commit the generated session file, ROM, save, savestate, or BIOS.
