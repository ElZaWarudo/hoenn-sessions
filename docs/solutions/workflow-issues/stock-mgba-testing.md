---
title: Reliable stock mGBA multiplayer testing
date: 2026-09-06
category: workflow-issues
module: Cloud co-op conformance
problem_type: workflow_issue
component: development_workflow
severity: medium
applies_when:
  - Running two real Windows mGBA sessions through the launcher and Lua bridge
  - Investigating differences between bridge fixtures and visible gameplay
tags: [mgba, windows, wsl, lua, conformance, testing]
---

# Reliable stock mGBA multiplayer testing

## Context

The first stock-emulator walking test took substantially longer than unit tests
suggested. Healthy bridge queues did not imply visible avatars, brief UI key
presses were unreliable, and Windows process isolation and build paths introduced
failures outside the mocked boundaries. These lessons come from the live runs
recorded in the [conformance report](../../testing/littleroot-conformance.md),
completed locally on 2026-09-06 and extended by the accepted Character/Online
campaign on 2026-09-07. No remote publication is implied.

## Guidance

### Observe each boundary separately

Record Lua authentication, ROM readiness, eligible outdoor pose, ticket mint,
WebSocket upgrade, joined-player count, visible remote avatar, reciprocal motion,
nonblocking collision, and lifecycle cleanup separately. None substitutes for
the next. The failed run had bridge flags `0x8f` and consumed queues but no avatars.
Fixed-label HTTP outcomes and joined counts made the later run diagnosable
without exposing credentials or tickets.

When a fixture disagrees with gameplay, trace the engine transition behind the
flag. `PLAYER_AVATAR_FLAG_CONTROLLABLE` is cleared during ordinary walking; its
name does not establish field-input permission. The old fixture repeated the
same incorrect assumption as production. The replacement regression exercises
ordinary on-foot movement and actual field-control locking.

### Use bounded game inputs and inspect the result

Brief native key presses were sometimes missed by emulator polling. For operator
navigation, a temporary Lua helper that applies keys in both `frame` and
`keysRead` callbacks worked reliably with stock mGBA 0.10.5. Release keys for
two frames before each press, hold for a bounded frame count, then release and
remove callbacks. Keep this helper outside versioned artifacts and session data.
It must only provide normal button input, never write poses or save memory.
The [mGBA scripting API](https://mgba.io/docs/scripting.html) documents `setKeys`,
the frame/input callbacks, and removal by callback ID.

Observed key indexes were A=0, B=1, Select=2, Start=3, Right=4, Left=5, Up=6,
Down=7. Twelve frames worked for menu confirmation; approximately sixteen frames
per walking tile worked in the tested outdoor state. These are starting points,
not universal timings. Inspect screenshots between stages, especially naming,
clock setting, dialogs, and map changes. The clock confirmation initially selected
No; moving Up before A was necessary. Male and female starts use different houses.

Replay a short, previously observed sequence only from its known starting state.
Large blind input batches can choose a different option or walk against a wall.
The successful second attempt used bounded batches with screenshots between
stages, reducing fresh-game navigation substantially.

For Online acceptance/decline, keep the recipient in the field while the sender
loads a fresh nearby view. Stage the recipient's known navigation before sending,
confirm the visible "Invitation sent" result, and execute promptly: invitations
expire after 30 seconds. Screenshot preparation and long input gaps can consume
that lifetime. An HTTP 200 alone does not prove acceptance; inspect both group
views. A decline that returns the invitation-domain HTTP 401 expiry result must
be repeated as a decline case. Test expiry separately from an observed live
invitation, and refresh the authoritative group afterward.

The final full-menu fixture used the existing debug UI to toggle Pokédex and
PokéNav and give one basic Pokémon. Ten configured entries then exercised the
eight-row pause window: scroll to Option, wrap between Pokédex and Exit, and
open Character/Online before Exit. Do not claim story progression from these
debug unlocks. Normal Bag exit needs enough fade time before dismissing the
restored pause menu; inspect field/dialogue rendering afterward.

The scripting console accepted `dofile` for the private per-player bridge script
and the input helper. Return executed the command while retaining console focus;
the Run button changed focus. Console `emu:runFrame()` was unavailable in that
context. Do not reload an already running bridge: startup binds the canonical
save and resets the emulator. Rediscover window IDs each run, refresh UI state
after switching windows, and activate the game before capturing evidence so the
scripting window does not obscure it.

### Keep build and runtime evidence aligned

On Windows, do not pass a Bash script containing `$variables` through nested
`wsl ... bash -lc` quoting without verifying the exact argument. One such call
expanded its variables before Bash received the assignments, turning both copy
paths into `/` and the build directory into an empty string. The copies failed;
the unintended build was stopped. Use literal paths for short calls or a script
file for commands that need shell variables. Preserve the build command's exit
status instead of accidentally returning the status of a later `tail` command.

Build the ROM on a native WSL filesystem; asset scans through `/mnt/c` were slow.
The first test build compiles many fixtures even when the execution filter names
only presence tests. Preserve the old source/build separately when proving a
regression fails before the fix.

Quote a PATH prepend or use an explicit Linux toolchain PATH. Expanding inherited
Windows PATH entries unquoted broke on spaces in Program Files. Generate the
bridge manifest with WSL Python and the explicit ARM `nm` path when Windows
cannot find that executable. Regenerate the manifest before rebuilding the Rust
server, which embeds it at compile time. Record the final ROM hash, not an earlier
artifact's hash. The distribution manifest is tracked despite ignore rules.

Codex's Windows AppData virtualization can make a logical download path differ
from its physical package LocalCache path. Resolve the physical path before
accessing such downloads from WSL; do not assume the logical `/mnt/c` equivalent
exists. Avoid relinking a Rust test executable while that same executable runs
the live harness: Windows rejected this with a linker file-lock error. Run
unrelated explicit test targets or defer that relink until cleanup.

### Separate startup, acceptance, and shutdown

A fresh title-screen boot can leave a zero-byte implicit save. This is compatible
with startup liveness; it is not a valid canonical checkpoint. Keep canonical
save validation strict and never fabricate a save to claim fresh-game evidence.

The harness gives each lifecycle an independently spawned Tokio task. Running
both futures in one task allowed synchronous artifact hashing/probing to starve
the other player's network work. Preserve coordinated stop handling and drain
both tasks; dropping a join handle is not cleanup evidence.

Choose the bounded observation duration before launch. The default 900 seconds
was short for two manually navigated fresh introductions; the harness supports
up to 7200 for the expanded Character/Online campaign. This is operator
observation time; game and protocol deadlines are unchanged. Create
`accepted.txt` only after every printed assertion was actually
observed. Use the adjacent `abort.txt` to stop without passing. A timeout, early
exit, or uncertain cleanup remains a failure even if an acceptance file exists.
Keep scripts, saves, passwords, tickets, and compatible states private.

After confirmed child teardown the harness retains a separate, genuine
`character.sav` copy and prints its private temporary path before releasing the
session workspace. This is diagnostic evidence, not a successful checkpoint or
an accepted run. It contains no bridge script or compatible state. Use it for
read-only checksum investigation or an offline Continue check; never patch it
to advance the campaign. Evidence-copy failure must still release the lease.

Exercise a normal Save both before presence starts and after a peer is visible.
The former active checkpoint path buffered peer lifecycle events while the save
and cloud transaction finished. A 32-event list overflowed with 33 paced updates
behind held finalization; coalescing per handle between structural events fixed
that isolated defect. Native retesting with a hidden peer exposed another cause:
the ROM intentionally emits no PlayerState during flash, exceeding the server's
1500 ms stale-presence limit. The save finalized successfully after the driver
had already reported ProtocolViolation. Cached poses cannot renew freshness.
Coordinator-backed checkpoints therefore need deliberate transport suspension
before flash grant and acknowledged rearm with a fresh pose afterward. Test
active, preactivation, and stalled-connection cases; stopping a pending upgrade
must not consume the three-second checkpoint decision window. Preserve fatal
policy-close outcomes instead of treating every 1008 close as recoverable.

## Why this matters

Unit tests can validate a mistaken engine assumption, and bridge status can hide
a later admission or rendering failure. Boundary-specific observations plus real
movement and cleanup evidence prevent false passes and make another operator's
investigation reproducible.

## When to apply

Use this guidance for stock-emulator presence checks and as preparation for
separate invitation, reconnect, travel, save, and resume scenarios. The walking
pass does not certify those flows.

## Example evidence record

For each run, record the revision and local diff, ROM and emulator hashes,
duration, exact test command and exit result, each player's observed behavior,
and both lifecycle/lease cleanup results. Distinguish failed or aborted runs from
passes. Link native screenshots and state explicitly which flows were untested.

## Online testing lessons

The first Online run authenticated both bridges, received ticket HTTP 200 and
WebSocket HTTP 101 for both players, and showed reciprocal movement. Opening
Online then blanked the display despite a successful snapshot response. The
run was aborted without acceptance after 1778.35 seconds; both lifecycles
drained and both leases were released.

Window tile counts alone do not establish safe graphics allocation. With the
field's character base 2, Online's former base tile `0x220` and 26×18 dimensions
wrote VRAM `0x0600C400..0x0600FE80`, overlapping background tilemaps. Base tile 8
reuses the removed pause menu's graphics and ends at `0x0600BB80`, below dialogue
borders and tilemaps. Check the actual window template against those boundaries.
After any allocation change, inspect Online, Back, movement, another menu, and
an NPC/sign dialogue; inactive windows can share graphics that must be redrawn.

Contract rejection tests need valid positive controls. Numeric presence handles
were malformed and made size/generation tests pass before their intended checks
ran. Use canonical hexadecimal string handles and prove that four peers and a
generation-1 invitation decode before rejecting oversized or ambiguous input.

Invitation pages can expire or be consumed between requests. An empty terminal
page is valid; an empty page claiming another cursor is not. Also exercise the
launcher's multi-page aggregation and 32-entry cap. Empty or HTML HTTP 5xx bodies
must remain service-unavailable outcomes rather than JSON protocol failures.

Record machine load with tight timing failures. The existing three-second
`realtime_activation_runs_ready_lifecycle_interaction_and_joined_teardown` test
timed out during a busy parallel run, then passed unchanged alone in 2.77 seconds
and in the complete launcher suite with one test worker (141 passed, one fixture
ignored). Separate contention from a logic regression before changing deadlines.

For normal-key intro navigation, 45-frame A holds advanced dialogue more reliably
than 12-frame holds in this run. A completed input batch does not prove dialogue
completion: inspect the game before the next state-dependent movement. Keep these
helpers private and never replace native movement with injected poses.

On Windows, do not rebuild a binary while the native harness is running that
same executable. A final workspace run could not replace `coop-sidecar.exe`
because Windows held it open. Finish the native lifecycle and confirm cleanup,
then rerun the workspace checks; this is an executable lock, not a test failure.

## Character testing additions

The native harness launches a standalone sidecar executable. Rebuilding Rust
test libraries does not refresh that executable: explicitly build
`coop-sidecar` after changing avatar ordinals, before starting mGBA. A stale
sidecar was found after a Character run ended with a control-protocol error.
Keep this distinction in preflight rather than diagnosing only the ROM.

Check label width as well as menu height. `CHARACTER` clipped in the original
seven-tile pause window even though all rows fitted vertically. The regression
uses the actual font width, window allocation, right edge, and VRAM end tile;
the eight-tile window passes those bounds.

Existing sprites are not interchangeable animation tables. Some named NPCs
have only standing frames. The selected roster has actual walking frames, and
its standard animation table now covers the player run/spin indexes. Spin
fallbacks must use spin sequences: substituting short standing sequences can
leave the player's animation-command index outside the sequence. Keep cosmetic
graphics separate from saved gender and rival graphics selection.

## Distinguish flash completion from checkpoint completion

A ROM saved-game message proves its flash routine returned successfully. It
does not prove the launcher validated and uploaded the resulting file. Test
both milestones and retain the actual lifecycle exit.

The native campaign found two successive failures. First, the sidecar's
three-second post-grant deadline expired while the ROM was still executing
flash programming routines. A read-only trace showed generation and save
counter advancing, with the main loop held by the synchronous write. The
post-grant budget is now ten seconds, inside a twenty-second launcher budget;
transport and grant-decision deadlines remain separate.

After that correction, the ROM saved successfully but Rust rejected the file
because its checksum-length table had drifted from linked `sSaveSlotLayout`.
Synthetic fixtures constructed with the same stale table could not detect it.
Keep the genuine disposable-game save regression and run manifest generation's
linked-layout check before native testing. The optional 16-byte mGBA RTC trailer
is supported and was not the cause. Obtain structure offsets from the current
linked build; old header offset comments may also be stale.

## Related

- [Runnable procedure and dated evidence](../../testing/littleroot-conformance.md)
- [Player-visible roadmap](../../product/roadmap.md)
- [Manual harness](../../../coop/crates/coop-launcher/tests/real_mgba_presence.rs)
- [ROM visibility predicate](../../../src/coop/presence_runtime.c)
