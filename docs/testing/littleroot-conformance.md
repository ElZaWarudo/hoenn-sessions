# Two-player Littleroot conformance

This is a local, operator-observed test of stock mGBA 0.10.5, the Lua bridge,
two production launcher lifecycles, and the authenticated in-memory server.
Passing Rust or Lua unit tests alone does not certify visible multiplayer.

## Prepare

Build the ROM with GNU Arm Embedded 13.2.Rel1 and generate its manifest before
compiling the Rust server: the server embeds that manifest at compile time.
On Windows, use WSL for the ROM build. A native WSL filesystem build avoids
the cost of scanning the asset tree through `/mnt/c`.

```text
make -j4 modern
python tools/generate_bridge_manifest.py --elf pokeemerald.elf --rom pokeemerald.gba
cargo build -p coop-sidecar --locked
```

Use the official portable Windows x64 Qt mGBA 0.10.5 artifact. The harness
checks the exact executable hash and the ROM/manifest match; it does not accept
an arbitrary emulator or an upstream ROM without this fork's bridge.

In PowerShell, supply absolute paths to your local artifacts:

```powershell
$env:COOP_REAL_ROM = 'C:\path\to\pokeemerald.gba'
$env:COOP_REAL_MGBA = 'C:\path\to\mGBA.exe'
cargo test -p coop-launcher --test real_mgba_presence --locked -- --ignored --nocapture
```

`COOP_REAL_SIDECAR` optionally selects the built sidecar executable (the default
is `target/debug/coop-sidecar.exe`). `COOP_REAL_DURATION_SECONDS` sets the
observation deadline between 30 and 3600 seconds; the default is 900. Use a
longer bounded window when manually navigating two fresh-game introductions.

## Observe

The harness creates two disposable accounts, separate server-issued leases,
separate epoch records, and separate private ROM/save/script workspaces. No
password entry, external account, cloud credentials, or public listener is
needed. It prints each player's script path and this run's acceptance path.

1. In each emulator choose **Tools > Scripting > File > Load script** and load
   that player's printed `main.lua`. Do not load the repository script or the
   other player's script. The bridge binds the canonical save and resets into
   it; it also owns optional compatible-state restoration.
2. Start a new game in each emulator and reach the Littleroot outdoor map.
   The launcher waits through intro/house poses before activating presence.
3. Confirm both scripts authenticate and both players see a remote avatar.
4. Walk with player one and observe the motion in player two's game. Repeat
   in the other direction. Confirm the remote avatar cannot block movement.
5. Only after observing all four assertions, write the exact LF checklist
   printed by the harness to its `accepted.txt` path. The harness then drains
   both lifecycles, stops their children, and releases their leases.

Creating the adjacent `abort.txt` file stops the test without accepting it.
A deadline, an early lifecycle exit, or uncertain cleanup fails the run even
if an acceptance file exists. Do not create the acceptance file to bypass an
unobserved or failing step.

## Evidence and limits

Record the Git revision/diff, ROM and emulator hashes, command outcome, and
observations for each player. Record an unexecuted or failed scenario as such.
Keep ROMs, saves, session scripts, credentials, and compatible states private.

This harness certifies only the walking observations actually made and the
automatic lifecycle cleanup results. Cross-map transitions, automatic
rejoining, checkpoint upload, resume/fallback, and Save and exit require
separate scenarios. The current realtime lifecycle makes one attempt and
terminates on incompatible partition changes. It does not automatically
rejoin when a player returns to Littleroot.

## Runtime checks on 2026-09-06

The local ROM was built with GNU Arm 13.2.Rel1 from revision `7bdf0ddb36` and
its manifest regenerated before compiling the server. Its SHA-256 is
`55d0831781645a28a97c8c86b48b4d48585eaa813f21b117708a8358fd88d4e2`.
The official Windows Qt mGBA executable SHA-256 is
`5a3c98c2984dd04bd0d7c9378cdfae937ae0d73a196c880bb2eecf3b254af247`.

The guarded real-emulator startup and cleanup test passed. A fresh title-screen
boot opens an empty implicit save; allocating 128 KiB is not a startup oracle.
Canonical save validation still requires the full valid save format.

Real startup exposed a missing Windows `SystemRoot` after environment
isolation in both the sidecar and guarded emulator paths. The launcher now
supplies the fixed kernel system-directory alias. Subprocess regressions verify
loopback binding and credential exclusion for both launch paths and the
emulator version probe.
Both real players subsequently launched; an intentional observation abort
drained both lifecycles and released both leases. With both environment fixes,
both guarded games rendered at approximately 60 fps and both private Lua bridges
logged successful authentication. That run was intentionally aborted during
fresh-game navigation and is not a walking conformance pass.

A subsequent 3600-second observation reached free movement in Littleroot in
both games. Neither remote avatar appeared. The run timed out without an
acceptance marker; both lifecycles drained and both leases were released.
Player one's public bridge status was `0x8f` (initialized, ROM/session ready,
player state sent, heartbeat seen), with both queues fully consumed. This
established bridge health but did not establish realtime admission or rendering.

Investigation found that `IsOverworldPoseAllowed` required
`PLAYER_AVATAR_FLAG_CONTROLLABLE`. The engine clears this bit during ordinary
walking in `PlayerAllowForcedMovementIfMovingSameDirection`; it controls
forced-tile detection rather than field-input permission. The existing ROM
fixture incorrectly treated that cleared bit as a hidden player.

The corrected ROM retains the existing field-lock, fade, callback, and player
binding guards and requires ordinary on-foot state. Its SHA-256 is
`6eaf019bfe087f671c71190a43d9fcacdf9d1ca854afabb483e30ff3e1ae88f3`.
The harness logs fixed-label realtime admission HTTP outcomes and changes in
the server's joined-player count, without logging tickets or credentials.

The corrected-ROM observation passed in 1503.61 seconds. Both bridges
authenticated, both ticket mints returned HTTP 200, both WebSocket upgrades
returned HTTP 101, and the server reported two joined players. Both games
displayed the remote avatar. Moving each player changed its position on the
other screen; each player also crossed the other's tile without being blocked.
The acceptance marker was written only after those observations. Both
lifecycles then drained and both leases were released; the harness exited 0.

Final native captures: [player one](evidence/littleroot-20260906/player-one.jpg)
and [player two](evidence/littleroot-20260906/player-two.jpg).

The new ARM regression failed against the old predicate at the first ordinary
walking pose. With the fix, all 34 `Cloud Coop presence` tests passed, including
moving/idle rendering and actual script-lock hiding. Launcher validation also
passed: 164 non-GUI tests, formatting, and Clippy with warnings denied. The
guarded stock-emulator startup test passed separately. These results do not
extend the scope to travel, saving, resume, or automatic rejoining.
