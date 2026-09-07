# Two-player Littleroot conformance

This is a local, operator-observed test of stock mGBA 0.10.5, the Lua bridge,
two production launcher lifecycles, and the authenticated in-memory server.
Passing Rust or Lua unit tests alone does not certify visible multiplayer.
Read the [stock mGBA testing lessons](../solutions/workflow-issues/stock-mgba-testing.md)
before another operator-driven run, especially for input timing and build pitfalls.

The final Character/Online campaign passed on 2026-09-07 in two stock emulators,
including actual lifecycle cleanup. See the [final evidence matrix](evidence/character-20260906/final-campaign/README.md)
and the final-validation section below. Earlier failed attempts are preserved
as diagnostic history, not outstanding failures of the final build.

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

Rebuild the sidecar executable after any protocol change, even when
`cargo test` has already rebuilt its library. The manual harness launches the
separate `target/debug/coop-sidecar.exe`; a passing test binary does not prove
that executable is current.

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
observation deadline between 30 and 7200 seconds; the default is 900. Use a
longer bounded window when manually navigating two fresh-game introductions.

Set `COOP_REAL_SCENARIO=online` to select the invitation/recovery checklist.
The default `walking` checklist remains limited to reciprocal movement.

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

## Online invitation and recovery scenario

Use the `online` scenario with `COOP_REAL_DURATION_SECONDS=7200` for two fresh
introductions and the full Character/Online checklist. This extends only the
operator's observation window; gameplay and protocol deadlines stay unchanged.
The harness routes local traffic through a test proxy, which can interrupt
WebSockets without dropping the HTTP connections used for the lease and groups.
No external server, account, or network configuration change is involved.

1. Reach Littleroot in both games and verify reciprocal walking first.
2. Keep player two in the overworld. Player one opens **Online > Nearby players**,
   selects player two, and sends an invitation. Player two then opens
   **Online > Invitations**, accepts, and both inspect their group status.
   A player with a menu open is hidden and is not an eligible invitation target.
3. Leave the group from player one, refresh both views, and verify both are
   ungrouped. Close the menus and verify nearby avatars remain. Repeat the
   invitation/acceptance/leave sequence with the roles reversed.
4. Send and decline another invitation. Also leave an invitation displayed for
   more than 30 seconds, then attempt acceptance. Neither stale action may
   create a group; Refresh must show the authoritative state.
5. Close both menus. Create the printed `interrupt-websockets.txt` marker once.
   The proxy logs each actual disconnection. Observe old avatars clear and
   reciprocal movement return. The Online acceptance marker is rejected unless
   at least two WebSockets were actually interrupted.
6. Enter a house and return to Littleroot with each player, observing reciprocal
   presence after each return. Neither emulator should restart.
7. Check Back during loading/unavailable states, long names, and a fully unlocked
   pause menu. Record any debug-only layout fixture separately from the real
   player evidence; never fabricate poses or mark an unobserved flow as passed.
8. Write only the exact Online checklist printed for this run, after every item
   has been observed. Both lifecycle drains and lease releases must also pass.

After repeated transport failures, presence pauses while gameplay and heartbeat
continue. An explicit Online Refresh can retry. Protocol/authentication failure,
unsolicited ROM reset, or uncertain checkpoint recovery still ends the session.

## Evidence and limits

Record the Git revision/diff, ROM and emulator hashes, command outcome, and
observations for each player. Record an unexecuted or failed scenario as such.
Keep ROMs, saves, session scripts, credentials, and compatible states private.

Each run certifies only its selected checklist and automatic lifecycle cleanup
results. The earlier walking pass does not certify the new Online scenario.
Checkpoint upload, resume/fallback, group travel, and Save and exit still require
separate live scenarios.

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

## Online implementation checks on 2026-09-06

The first Online observation used ROM SHA-256
`6d0ac5db6669ca20193e156419aebe4cce290aa9e75c8d2f0dcc1cd14ec96cae`
with the local U1–U4 implementation and manual Online harness. Both bridges
authenticated, both mints/upgrades succeeded, and reciprocal movement appeared.
Opening Online returned snapshot HTTP 200 but blanked the game display. The
operator aborted without acceptance after 1778.35 seconds. Both lifecycles
drained and both leases were released. This was not an Online conformance pass.

The [failed screen](evidence/online-20260906/online-vram-failure.jpg) exposed
overlapping window graphics and field tilemaps. Online now reuses the removed
pause menu's base tile 8. An independent review checked border, map, popup, and
message-window restoration behavior. A regression against the actual window
template failed with old base `0x220`; all 10 Online ARM tests passed with base 8.

Code review also corrected transient mint recovery, non-JSON HTTP 5xx handling,
empty terminal invitation pages, failed-snapshot detail text, and contract test
fixtures. Post-fix checks passed: 141 launcher library tests with one test worker
(one fixture ignored), a separate eight-page/32-invitation cap test, checkpoint
reconciliation before mint recovery, and Clippy with warnings denied. The tight
three-second existing realtime lifecycle test timed out under concurrent load
and passed unchanged alone and in the serial suite; see the
[testing lessons](../solutions/workflow-issues/stock-mgba-testing.md).

## Character selector campaign

The user extended the campaign to include an in-game Character appearance menu.
Use Start → Character, browse the animated preview with Left/Right, apply with A,
or cancel with B. The roster is Brendan, May, Red, Leaf, Wally, Steven, Norman,
Youngster, Lass, Prof. Birch, Hiker, and Sailor, plus Original appearance.
Selection changes walking appearance; bike, surf, fishing, story identity, and
progress retain their native behavior. Saving the game persists the selection.
No save-layout expansion is needed: a tagged choice occupies unused var 0x40FF.

For the campaign, select different appearances through this menu on both games.
Observe each selected sprite locally and remotely, all movement directions,
cancel without changing the applied choice, and appearance after house return.
Cycle the roster to inspect previews, then return to a known pair before the
invitation scenarios. Check a normal save/reload for the cosmetic choice
separately from claiming full cloud save/resume certification.

The preceding Online attempt using ROM
`d49d9e20ee2c511621c4a086065aeac417f2129bcc5c2a44bd3cee804bdf7c2a`
timed out after 3607.27 seconds without acceptance. Both lifecycles drained and
leases released. Its invitation/group/leave screenshots are partial historical
evidence; they do not certify the expanded campaign or the new character build.

The first Character attempt (`14c7ab457e43b8dfdad77bcf707564f036877f6a9703a524f27e6a7a33a42ce7`)
authenticated both bridges and joined both players. Original and Wally previews
rendered, but the pause label clipped. A regression against the actual window
allocation reproduced it; widening the pause window from seven to eight tiles
passed while retaining its right edge and safe graphics bounds. This attempt
failed after 976.37 seconds with a sidecar control-protocol error and no
acceptance. The launched sidecar executable was stale (built before the expanded
avatar protocol); it was explicitly rebuilt before retrying. The stale binary
is a suspected cause, pending verification on the corrected artifacts.

The next attempt used ROM
`8d599e9742e14c4418e754f9a105299d99058c584f59a7a1b5ae35eb8b336eb0`
and a freshly built sidecar. The label fitted; Wally and Leaf applied locally
and appeared on both screens with reciprocal movement. Two complete roster
cycles followed by cancellation retained Wally. Delayed Online allowed Back;
injected HTTP 503 displayed Unavailable, Back restored the field, and Refresh
recovered. Both players accepted and displayed their group; player one left.
The run then failed at 1596.78 seconds with player two's generic realtime
lifecycle error. Player one drained and released its lease. No acceptance was
written. Captures in `evidence/character-20260906` document this partial run.

Investigation found a separately reproducible coordinator ordering defect:
late input could report a fatal closed-channel error before the driver result
was collected. A real-socket regression held task completion after channel
closure and failed on the late pose. The fix defers closed/not-ready input to
terminal classification and joins the driver when its event channel closes.
The regression also checks that malformed protocol input remains fatal. The
native failure's exact terminal cause was not retained, so the connection to
this defect remains a hypothesis until the final campaign is rerun.

Independent review found that waiting for that result also had to retain the
driver's `JoinHandle` across cancellation by the session's select loop. The
extended regression failed on lost ownership, then passed after awaiting the
borrowed handle and clearing it only on completion. All seven coordinator
checks passed; a follow-up review found no remaining scoped issues. The full
workspace suite passed before this ownership correction, and workspace Clippy
with warnings denied passed afterward. Final native and workspace retests
remain required before acceptance.

The recovery retest (same ROM, corrected coordinator) ran for 2621.43 seconds.
Both characters, cancellation, reciprocal movement, accept, leave from each
side, decline/stale Accept, real 30-second expiry, visible Connecting/Back,
HTTP 503/Back/Refresh, two actual socket interruptions with fresh connections,
both house returns, Bag, and sign dialogue passed. Screenshots are in
`evidence/character-20260906/recovery-retest`. Late-response ordering remains
covered by the deterministic reopened-view regression, rather than claimed
from native timing. The first clock transition briefly rendered black, then
restored without inputs or restart.

The run failed immediately after confirming the ordinary Save on player two,
while player one was preparing the fully unlocked menu fixture. No acceptance
was written. The generic player-one control error did not retain the underlying
cause or report the other player's failure. The retained player-two canonical
save still had its initial timestamp, so persistence and full-menu layout did
not pass. The harness now reports each lifecycle failure and payload-free
control-pump cause before cleanup. A short save-first reproduction is required;
the three-second post-grant completion timeout is a hypothesis, not a diagnosis.

A short diagnostic attempt reproduced the Save failure before character changes,
debug unlocks, or Online activity. It ended after 531.61 seconds. Fixed-label
tracing in the private staged Lua script (installed before first load) recorded
the grant arriving, but no ROM completion frame, savedataUpdated callback, or
capture. Both channels reported ReaderClosed; canonical save timestamp remained
unchanged. This isolates a general checkpoint handoff problem from the new
appearance menu. No acceptance or completed-save claim was made.

The next save-only attempt ended after 961.70 seconds. Read-only ROM tracing
in the staged script showed the grant consumed, checkpoint state Saving,
generation 1, save counter 1, and successful preparation. For the following
175 emulated frames, PC samples remained in flash timer/programming routines
while the synchronous save held the main loop. The sidecar then closed both
control channels at its three-second post-grant deadline. This establishes
premature expiry during flash work, rather than a character-selection defect.

The sidecar now allows ten seconds for post-grant flash completion; the
launcher's enclosing checkpoint deadline is twenty seconds. Decision and
transport limits remain three seconds. A socket regression completing after
four seconds failed with the old deadline, then passed in the full sidecar
suite (104 tests). Independent scoped review found no issues with absolute
expiry, correlation, lease heartbeats, or shutdown. Native timing remains
pending. The missing-completion test still exercises finite expiry.

The rebuilt run then displayed the ordinary saved-game message and returned
to the field, but failed after 826.14 seconds overall with a launcher
checkpoint deadline. No acceptance was written. The retained 131088-byte
save includes the supported 16-byte RTC trailer and a complete generation-1
slot. Comparing its checksums against linked `sSaveSlotLayout` exposed stale
Rust lengths at sectors 0 (3884 vs 3892), 4 (3664 vs 3968), and 5 (0 vs 264).
The launcher had been retrying that rejected file until expiry. The genuine
file is retained as a disposable-game parser regression fixture; a linked-ROM
layout check now guards manifest generation. The parser fixture failed with
the old lengths, then all 23 save tests and 27 Python tool tests passed. The
complete Rust workspace, formatting, Clippy, and Lua suites also passed before
the next native attempt.

The layout retest ended after 3157.47 seconds with player one's generic realtime
lifecycle error during a second, outdoor Save. The first indoor Save displayed
the saved message and the supervised session remained healthy beyond the
twenty-second checkpoint deadline. Both players then selected Wally/Leaf,
cancelled a changed preview, moved reciprocally, inspected empty Online lists,
formed two groups, left once from each member, and declined an invitation.
Stale Accept after decline remained ungrouped. Screenshots are in
`evidence/character-20260906/layout-retest` (ROM SHA-256 unchanged: `8d599e9742e14c4418e754f9a105299d99058c584f59a7a1b5ae35eb8b336eb0`).

Player two drained and released its lease; no acceptance was written. The
second Save's completion message was not observed and its workspace was not
retained, so appearance persistence is still unproven. Expiry, injected faults,
house returns, fully unlocked pause layout, and final clean acceptance remain
pending on the corrected runtime. The failure's original cause may be obscured
by realtime cleanup, which currently replaces an earlier error. A separate
32-event checkpoint-buffer overflow was independently reproduced with 33 paced
peer updates while cloud finalization was held: the old implementation returned
`Realtime` after completing the checkpoint. Coalescing the newest update per
handle within structural-event boundaries fixes that regression; all 15 focused
checkpoint tests pass. Spawn/despawn order and the 32-event structural cap remain
intact. The other player was hidden in Online during the native failure; do not
attribute that failure to this buffer without additional evidence.

The next attempt retains the genuine SAV after child teardown, logs checkpoint
prepare/finalize status and payload-free driver outcomes, and preserves an
existing fatal error if coordinator cleanup also fails. Failed cleanup still
turns a recoverable outcome into a fatal one. The operator observation maximum
is now two hours (default fifteen minutes); production deadlines are unchanged.

The checkpoint-liveness diagnostic failed after 1547.25 seconds. Its first indoor
Save visibly completed and both checkpoint HTTP phases returned 200. After both
players joined and selected Wally/Leaf, player two stayed in Online while player
one overwrote the ordinary save outdoors. Logs recorded an active checkpoint,
`ProtocolViolation` from the realtime driver, successful prepare/finalize, then
rejection because the driver was terminal (no reset or generation mismatch).
Both leases were released and the harness retained the genuine SAV files after
child teardown. No acceptance was written. Screenshots are in
`evidence/character-20260906/checkpoint-liveness-retest`.

The ROM intentionally suppresses PlayerState publication while awaiting a save
grant and writing flash. That exceeds the server's 1500 ms presence-staleness
limit during a normal save; the resulting policy close is correctly fatal.
Forwarding cached poses cannot solve this. The correction deliberately retires
the presence transport before granting flash, completes the noncancellable
checkpoint, and resumes through acknowledged rearm with a fresh pose. Native
verification of that correction remains pending.

An untouched copy of the retained player-one SAV (SHA-256
`8c5befe82de8015cc717a3d24e3c091142534ba1031e72e79efa82fdd4126222`)
was paired with the same ROM and opened offline in stock mGBA. Continue restored
Wally in Littleroot; normal movement worked and reopening Character showed Wally
as the selected choice. The reload screenshots and provenance are in the same
checkpoint-liveness evidence directory. Appearance persistence is now observed;
this does not certify multiplayer checkpoint recovery.

The suspension correction passed real-socket regressions for active presence,
Ready-but-inactive presence, and an unanswered HTTP upgrade. The old driver kept
the active socket open at flash grant; the old connecting path missed the bounded
stop check. The corrected paths close before grant and require acknowledged
rearm, without charging planned saves to the transport-fault retry budget.
Serial launcher tests passed (147 plus one ignored), sidecar tests passed (104),
and both libraries passed Clippy with warnings denied. Independent review found
no remaining issues; all temporary production diagnostic prints were removed
before rebuilding the final observation runtime.

## Final Character/Online native acceptance — 2026-09-07

The final harness passed in **4221.48 seconds**, one test with no failures.
It ran on base `598de40eb16cd59793bf43a4a5c3d6028812be09` plus the reviewed local
diff, ROM SHA-256 `8d599e9742e14c4418e754f9a105299d99058c584f59a7a1b5ae35eb8b336eb0`.
The [evidence matrix](evidence/character-20260906/final-campaign/README.md) pins
the emulator, sidecar and harness hashes and maps every gameplay row to directly
observed screenshots. No runtime rebuild or bridge reload occurred in the run.

Both players authenticated, selected Wally/Leaf, saw reciprocal motion, and
retained the applied choice after cycling and cancelling a different preview.
Online names, empty lists, accepted groups, each member's leave, decline/stale
Accept, and a separately observed live invitation followed by expiry all passed.
Both members independently refreshed to ungrouped states. A timed-out decline
attempt was repeated successfully rather than counted as decline evidence.

The delayed response showed Connecting and allowed Back; the injected HTTP 503
showed Unavailable and allowed Back, then a subsequent Refresh succeeded.
Late-response ordering remains a deterministic reopened-view regression; native
timing did not establish overlap before the held reply auto-released. The proxy
disconnected two actual sockets, observed zero joined players, then two fresh
ticket mints/upgrades and two joined players. Reciprocal movement was observed
after recovery. Both house transitions restored the selected local/remote
sprites, with subsequent movement. Bag and sign dialogue remained readable.

The normal debug UI enabled Pokédex/PokéNav and supplied one Pokémon solely as
a layout fixture. All ten configured pause entries were visible through bounded
scrolling; top/bottom wrap, Character, Online, and Exit worked without clipping.
This is not a claim of earned story progress.

An ordinary active multiplayer Save also completed prepare/finalize (HTTP 200),
rejoined with a fresh connection, and restored reciprocal movement. This verifies
the save/presence suspension fix in the native runtime. The transient saved-message
text was not captured in this attempt. Same-ROM appearance Save/Continue is
separately proven by the untouched offline reload described above; full cloud
resume and live group travel still need their own acceptance scenarios.

Acceptance was written after all printed gameplay assertions had been observed.
Both lifecycles then drained, both lease releases were confirmed, both emulator
and sidecar processes exited, and the temporary fault directory was removed.
The harness retained private genuine SAV files after child teardown (paths and
limits in the evidence README). Neither acceptance nor final cleanup is inferred
from an earlier failed run.

## Final Validation

Code review: skipped (ce-code-review unavailable)

The formal review workflow exhausted its retries. A manual diff scan covered
the final Character/Online, shared protocol, save-layout, and lifecycle changes;
separate scoped reviewers checked the behavior-bearing fixes and their follow-up
corrections, with no unresolved findings. These checks do not claim a successful
formal ce-code-review receipt.

Post-cleanup ARM tests passed all 110 Cloud Coop cases. Python tool tests passed
27 cases; all three Lua protocol/memory/main-loop suites passed. Formatting and
`git diff --check` passed. Regeneration to temporary files matched the tracked
manifest and Lua addresses exactly, including the linked save checksum layout.
The final serial Rust workspace run (`cargo test --workspace --locked --
--test-threads=1`) passed (544 tests; three explicit environment/subprocess
fixtures ignored), including 147 launcher library tests, 104 sidecar
library tests, 23 save tests, and nine harness infrastructure tests. The explicit
native harness was run separately as described above. Workspace all-target
Clippy passed with warnings denied. No final gate failed or required a deadline
change. Local operator logs are `hoenn-character-final-workspace.log` and
`hoenn-character-final-clippy.log` in Temp; WSL logs are
`/tmp/hoenn-character-final-arm.log` and `/tmp/hoenn-character-final-python.log`.
An independent final documentation review found no inconsistencies and verified
that all referenced final-campaign screenshots exist.

Local implementation commits: `4f5c6c8293` (saved character selection and readable
menus) and `75f0c36cb1` (native-save and connection recovery fixes). Documentation
and observed evidence are delivered in the following local U5 commit. No remote
publication is part of this result.
