# Cormoria campaign cases

The case registry is `data/cormoria/campaign_checks.json`, backed by donor
revision `f7997186345885bfa23a170e5f573851fc034b9b`. This first unit covers the
starter capacity boundary and four entry gates. It does not certify the
complete campaign. Registered tests are not execution receipts; consult the
run record for their observed results.

| Case | Executed boundary | Expected result |
| --- | --- | --- |
| Party space, full PC | Production `givemon` | Party delivery; PC bytes unchanged |
| Full party, PC space | Production `givemon` | PC delivery; party bytes unchanged |
| Full party and PC, then free one slot | Production `givemon` | Failure preserves data and caught status; retry delivers to the freed slot |

Each native case covers Gothita, Timburr and Galarian Zigzagoon with both forced
shiny modes. `test/cormoria/campaign.c` uses the existing embedded script DSL
and actual gift/storage code. Run it in an isolated test corpus:

```sh
make -s -j2 check TEST_SRCS='test/test_runner.c test/test_runner_args.c test/test_runner_battle.c test/test_test_runner.c test/cormoria/campaign.c' TESTS='Cormoria starter'
```

`TEST_SRCS` limits compilation/linking of test files, whereas `TESTS` alone
only filters execution. The isolated corpus avoids unrelated test EWRAM
pressure. Keep the four runner sources in the selected corpus. Native checks do not need the Cormoria map profile: gift behavior is
shared by both ROMs. Record the source inputs, profile and output in the
orchestration run record before treating a run as evidence.

The Python regeneration test separately checks each lab entry: already-claimed
choices retain their original branch; capacity is checked before presentation;
both shiny branches reject `MON_CANT_GIVE` before flags or object removal;
failure releases controls and retains the choice. Selected objects are 4/7/6,
and removing them is scoped to their respective starter branches. Party result0
and PC result1 both count as successful delivery.

Visible interaction, failure followed by save/reload and retry, and the full
lab cutscene remain unverified. The immediate script DSL does not support the
entry's object events, waits and presentation.

`test/cormoria/finale.c` uses exported production scripts in the Cormoria
profile. It executes predicates and text selection until the first declared
save/hardware effect, checking the actual text pointer and stopped command.
It permits no UI effect. Cases cover Championship registration, match entry,
the post-finale lab scientist and Ceram's second-badge exit gate, including
regional versus host flag isolation and branch precedence. Championship
registration has no eight-badge predicate: its donor gates use the Kohla-room,
signup and regional game-clear flags. The text prefixes include distinguishing
control codes where two dialogues share an opening.

```sh
make -s -j2 check ROM_WORLD=cormoria TEST_SRCS='test/test_runner.c test/test_runner_args.c test/test_runner_battle.c test/test_test_runner.c test/cormoria/finale.c test/cormoria/post_battle.c' TESTS='Cormoria'
```

These cases do not execute Championship battles, Somber's ending, Hall of Fame,
post-finale state writes or visible movement. Next cases must cover those
campaign transitions, followed by rewards/services, minigames and
player-local story behavior during co-op. Travel evidence remains in
`tools/coop/LIVE_REGION_HARNESS.md`.

The lab Gardevoir's Starf Berry now checks delivery before its thanks and
claimed flag. Regeneration checks cover rejection ordering and a control-release
exit; live full-pocket, retry and save/reload checks remain pending.

`test/cormoria/rewards.c` executes the compiled Gardevoir entry and real
`STD_OBTAIN_ITEM` call, including `additem`, result handling and regional claim
flag. A bounded test-local command table skips only named presentation effects
and rejects unexpected opcodes. It covers a saturated Starf stack, successful
retry after removing one Berry, and a third interaction with no gift attempt;
Bag bytes and unrelated flags/items must stay intact. This is native transaction
proof, not dialogue, field locking or save/reload proof. The generic free-space
query counts empty Berry slots even when actual insertion rejects a second
stack; the fixture establishes rejection through `AddBagItem` itself.

```sh
make -s -j2 check ROM_WORLD=cormoria TEST_SRCS='test/test_runner.c test/test_runner_args.c test/test_runner_battle.c test/test_test_runner.c test/cormoria/quest_state.c test/cormoria/rewards.c' TESTS='Cormoria'
```

The quest-state boundary uses the donor table's twenty subquests: index19 is
valid, index20 is rejected, and neighboring event flags are unchanged. The
former twenty-one-bit native assertion was stale. Quest palette provenance
requires exact donor bytes; `.gitattributes` keeps its textual palette in LF
on Windows without weakening the pinned digest.

`post_battle.c` additionally executes first regional completion and repeat
completion with both host-clear states and absent/present shared first-HOF time.
It checks normal-mon healing and one ribbon, egg ribbon exclusion, identity and
held-item preservation, sealed regional state, callback and continue warp.
It does not execute the Hall-of-Fame UI or write/reload flash.

`story_transitions.c` executes both answers to the imported Dreamstone return
script across all host/regional completion combinations. No preserves the
regional record and warps; Yes advances island stage3→4, swaps Tenebris/finale
visibility and commits Uncharted Island(78,24), warp255, preserving party/Bag
and completion state. The bounded adapter runs real state commands and parses
warp operands using native `setwarp`, followed by `ApplyCurrentWarp`; it replaces
warp scheduling, so rendered map loading, avatar reset and disk persistence
remain unverified. Named question/lock/release presentation is skipped only.

```sh
make -s -j2 check ROM_WORLD=cormoria TEST_SRCS='test/test_runner.c test/test_runner_args.c test/test_runner_battle.c test/test_test_runner.c test/cormoria/post_battle.c test/cormoria/story_transitions.c' TESTS='Cormoria'
```

`somber_ending.c` executes the complete imported Somber ending and the normal
finale lab entry across four host/regional championship states. Native state,
quest, healing, permanent actor coordinates and respawn commands remain active;
named presentation adapters are bounded and counted. With Waterfall already
owned, it checks stage5, finale flags, completed Somber quest, retained party/Bag
and championship flags, finale lab(5,4), and the native Carabrue respawn. Visible
cutscenes, map loading, missing-gift/full-pocket behavior and flash persistence
remain unverified.

The baseline exposed imported heal token0x0303 truncating to host location3.
`ResolveScriptHealLocation` translates the seventeen ledger tokens explicitly
before the native u8 boundary. Unsupported full-width values return NONE and
leave the saved respawn unchanged; native lookup indices and ledger IDs retain
their existing meanings. `heal_commands.c` runs actual setrespawn bytecode for
all seventeen mappings, all sixty-two host IDs, and invalid values in both ROM
profiles. Native results: Cormoria5/5 definitions, Main2/2; retained input hashes,
ELFs and logs at S:\cormoria-build\native-somber-20261003. The existing signed
v7m ROM pair predates this fix; its live results do not prove the repaired
campaign respawn until the next deliberately batched signed family is built.
