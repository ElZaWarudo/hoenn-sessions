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
completed locally on 2026-09-06. No remote publication is implied.

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

The scripting console accepted `dofile` for the private per-player bridge script
and the input helper. Return executed the command while retaining console focus;
the Run button changed focus. Console `emu:runFrame()` was unavailable in that
context. Do not reload an already running bridge: startup binds the canonical
save and resets the emulator. Rediscover window IDs each run, refresh UI state
after switching windows, and activate the game before capturing evidence so the
scripting window does not obscure it.

### Keep build and runtime evidence aligned

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
up to 3600. Create `accepted.txt` only after every printed assertion was actually
observed. Use the adjacent `abort.txt` to stop without passing. A timeout, early
exit, or uncertain cleanup remains a failure even if an acceptance file exists.
Keep scripts, saves, passwords, tickets, and compatible states private.

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

## Related

- [Runnable procedure and dated evidence](../../testing/littleroot-conformance.md)
- [Player-visible roadmap](../../product/roadmap.md)
- [Manual harness](../../../coop/crates/coop-launcher/tests/real_mgba_presence.rs)
- [ROM visibility predicate](../../../src/coop/presence_runtime.c)
