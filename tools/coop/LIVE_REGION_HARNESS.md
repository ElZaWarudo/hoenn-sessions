# Test-only multi-ROM journey harness

`live_region_harness.py` keeps one signed release and two isolated character
profiles pinned across retries. Its plan names world IDs and portal IDs from the
signed catalog; the Python code contains no Cormoria map or quest special case.
The `run_dir` must be on a volume other than C:. Before any live action,
`preflight` verifies the Ed25519 release envelope and all signed artifact
digests, the server catalog and arrival templates, each installed ROM, the two
character IDs, and distinct ROM-written source saves. It refuses to run below
2 GiB free on C:.

Use the same plan for each attempt while its envelope, catalog, ROMs, profile
installations, and source heads are unchanged:

```powershell
py -3 tools/coop/live_region_harness.py $Plan preflight
py -3 tools/coop/live_region_harness.py $Plan server-check
py -3 tools/coop/live_region_harness.py $Plan launch
py -3 tools/coop/live_region_harness.py $Plan start --pids $DesktopPidsJson
py -3 tools/coop/live_region_harness.py $Plan drive --leg $Leg --pids $GamePidsJson
py -3 tools/coop/live_group_evidence.py $Plan --leg $Leg --group-id $GroupId --output-dir $ServerEvidenceDir
py -3 tools/coop/live_region_harness.py $Plan discover --leg $Leg --output $EvidenceJson
py -3 tools/coop/live_region_harness.py $Plan verify --leg $Leg --evidence $EvidenceJson
py -3 tools/coop/live_region_harness.py $Plan journey
```

`launch` uses the already installed signed fixture, and `start` identifies
each mGBA process by its isolated profile. `drive` accepts a bounded list of
`player`, `key`, `hold_ms`, `release_ms`, `wait_ms`, and optional `screenshot`
actions. It refuses an empty itinerary. `start` captures the signed desktop
status if mGBA does not open within 45 seconds. Every successful or failed
boundary appends to `checkpoint.json` on the spare volume. Avoid putting
credentials or bearer tokens in the plan, evidence, screenshots, or checkpoints.

`server-check` performs a bounded, read-only `GET /health/ready`. Set
`server_url` in the plan or `COOP_HARNESS_SERVER_URL`; credentials, query
strings, and fragments are rejected, and remote hosts require the explicit
`allow_remote_server_checks` plan flag. Transport failures retry at most four
times and never send lease, snapshot, or group requests.

`discover` scans only local profile, run, and release save files plus travel
journals for the selected leg. It writes paths and the leg name while leaving
authenticated `group` evidence as `null` unless the plan supplies a
`group_evidence` JSON path. Journal contents include session and nonce material,
so they are never copied into checkpoints or printed. The discovered staged
save must match the journal digest and a valid Flash1M image before `verify`.

After the signed game clients exit, `live_group_evidence.py` can authenticate
both test accounts and collect the server's current save, latest older save in
the destination world, and group view. Set `COOP_HARNESS_USERNAME_A`,
`COOP_HARNESS_USERNAME_B`, and `COOP_HARNESS_PASSWORD` in the process environment;
do not put credentials in a plan. The probe verifies each downloaded save
against its authenticated snapshot manifest and releases each temporary lease
even on failure. Keep its output directory under `run_dir` on the spare volume.
The clients' leases must be released or expired before this probe acquires its
own; HTTP 409 is a stopped boundary, not an invitation to overwrite a live
session.

For each leg, the evidence JSON has `players.a` and `players.b`, each with
`source`, `staged`, `template`, and `journal` paths, plus an authenticated
`group` response. `verify` checks the exact character, source world, portal,
destination world, journal stage digest, Flash1M sectors and V2 CRC, every
descriptor-owned shared and world-local field, and group membership/region.
For a first arrival, the destination world-local oracle is the signed arrival
template. For a return visit, set each player's `destination_base_save` and
`destination_base_sha256` to the authenticated dormant-world head. The server
projects the shared player fields into that parked world; comparing a return
against a fresh template would incorrectly reject preserved local progress.
If normal gameplay between source and ferry changes a field, name its numeric
descriptor ID in that leg's `runtime_field_exceptions` and retain both saves.
Set `expected_generation_delta` to the number of ROM writes between the source
and the staged arrival. Do not exempt Pokémon, Bag, PC, or co-op fields.

When the exact journal source checkpoint is available, `verify` requires it to
match the journal digest. When the launcher has already retired that file,
`source_is_baseline: true` allows a pinned earlier ROM-written source save and
reports `exact_journal_source_inspected: false`. This checks end-to-end field
preservation over the stated generation delta but does not prove every byte of
the intervening source checkpoint. Capture that checkpoint during the next
replay to close this evidence gap.

`drive` now watches each player's live session directory throughout input and
receipt waits. It retains validated, content-addressed images on the spare
volume from a single immutable read, with a limit of 256 images per player/leg.
It requires BOTH players' fresh journals and their exact source and stage
images; an earlier baseline cannot satisfy this live boundary. Invalid partial
writes are retried, capture errors stop the leg, and existing evidence is never
overwritten with different bytes. Discovery prefers retained exact copies and
accepts multiple identical copies. Standalone `verify` revalidates all fixture
pins before reading its descriptor.

For a return leg, `destination_base_leg` can name the outbound leg whose source
world is now the destination. Its persisted discovery evidence supplies the
exact parked source save, checked against the preceding successful oracle
receipt's source digest, actor ID, world pair, portal and fixture identity.
Discovery retains a private hash-pinned journal copy before the client can
retire its live journal. Never publish journal copies: they contain session
and nonce material. This works with catalog world IDs and named legs rather than a
Cormoria-specific rule. An authenticated explicit dormant save remains usable
for older offline evidence.

`journey` launches the installed signed clients once and runs every leg in plan
order. Each leg needs `inputs` for the ferry and `arrival_inputs` for cold
Continue covering both players, using the same bounded key-action format.
It checks the replacement emulator's actual ROM argument against the catalog
hash, sends the arrival inputs, retains both destination screenshots, closes
the runtime through the signed desktop's Stop control, collects authenticated group/save evidence, and
verifies preservation before starting the next leg. Screenshots are witnesses
for inspection; the tool does not recognize harbor imagery automatically.
Lease collection waits up to 15 seconds for acquire-world HTTP 409 with the
same idempotency key, without takeover; other errors stop immediately. Normal
standalone collection has no retry. An individual HTTP request can add up to
15 seconds beyond the between-request deadline. Cleanup attempts desktop close
even when runtime drain fails, waits for exit, and preserves the primary error.
No forced process kill or save repair is performed. C: space is checked during
inputs and receipt waits, with a 2 GiB stop floor.

`live_seed_players.py $SeedPlan [--register]` provisions real ROM-written
lineages through loopback snapshot admission, prepare, upload and finalize.
The seed plan adds `seed_world_id` and each player's ordered `seed_lineage`
array of `{ "path": "...", "sha256": "..." }` entries, starting at generation
one and ending at the pinned `source_sha256`. It validates both lineages and
distinct decoded money sentinels before making requests. Use the same account
environment variables as the group collector; fresh registration additionally
requires `COOP_HARNESS_INVITATION`. The resulting character IDs must match the
signed clients' authenticated profiles before journey preflight can pass.
Matching partially seeded heads resume at the next generation; complete heads
are reused without another snapshot. A changed or advanced head stops instead
of being replaced. Credentials are never copied into the plan or receipt,
redirects are refused, and leases are released in cleanup. This tool copies
existing ROM writes; it does not fabricate harbor saves or migrate legacy data.

On 2026-10-02 the retained signed v7l catalog and ROM pair admitted two fresh
local accounts with coherent harbor generations A 1–6 and B 1–7. A repeated
seed invocation returned identical revision/hash pairs with no reinstall or
ROM rebuild. The first retry exposed an admission-order 401; admission now
precedes the existing-head read and the focused live retry passes. Its temporary
server is closed. The generic journey command and exact capture path have 40
passing focused tests and independent review, but still need the complete
signed-client live replay. The newly fixed sidecar reconnect race also requires
one updated signed runtime fixture before live validation; retain the ROM pair
and reuse that fixture while its relevant inputs remain unchanged.

The current v7e Main→Cormoria live evidence and v7l Cormoria→Main live evidence
both pass the descriptor and group oracles for two distinct characters. The
v7l fixture reuses the exact Main and Cormoria ROMs and harbor saves; only the
signed desktop changed to prevent restoring a pre-ack emulator state. Both
clients cold-Continued at the Main harbor after the return. The return oracle
uses the authenticated dormant Main heads and reports that its earlier
Cormoria baselines, not the exact ferry checkpoints, were inspected. The
individual `drive` commands and evidence collection have been exercised, but
one unattended invocation of the complete two-leg itinerary is not yet proven.

The current v7m fixture includes the updated reconnect sidecar and desktop,
while retaining that exact ROM pair. Both profiles installed it once. Startup
now waits for the owned desktop window and retries Play under the controller's
duplicate-start guard. The full replay launched both games but its cold input
sequence remained in menus and produced no ferry journal; runtime cleanup
finished. Retry diagnosis also found expired persisted sign-in after runtime
closure. These initial failures are superseded by the proof below.

The v7m live run subsequently completed both ferry crossings for both players
and cold-Continued them into Cormoria and back to Lilycove. The outbound oracle
passed before the return. Return discovery then stopped because the client
retired the outbound journals. The harness now retains journal copies and
uses actor/world/fixture-bound successful receipts for parked-world selection.
The focused return check reused captured bytes and authenticated group evidence,
explicitly pinning each parked Main save to its already verified outbound
source after checking the original plan and preceding preflight bindings.
Both legs pass 27 shared fields, 16 world-local fields, exact ferry source
digests, generation increments and the same two-member group, with zero
runtime exceptions. The final harness has 52 passing checks and independent
evidence review; a fresh single-command replay of the final code remains
unverified. Empty party/PC witnesses do not prove populated Pokémon handling;
campaign, reward, service and story-aware co-op acceptance remain separate.

The proved cold sequence is B(wait20s), Start(wait10s), Start(wait4s), A(wait5s),
after the owned mGBA window displays the expected ROM header title. Desktop
startup accepts `start_timeout_seconds` from 1–180 (default45); this run uses120
after observing game startup at48s and96s. Desktop stdout/stderr and settled
screenshots stay under the spare-volume run root. Stop preserves cached sign-in;
closing the emulator first had caused ReaderClosed and credential revocation.
No ROM rebuild, fixture regeneration or reinstall was needed for these fixes.

An input may request `"expect_presence_published": true` after its settle wait.
This records one documented bridge with a ready session, published player
state, and no specified bridge error flags. It rejects a cold session which
has never established presence. Historical presence can survive as a hidden
pose in menus, so this checkpoint does not prove current controls, expected
harbor location, or successful Continue. Inspect destination screenshots as
separate witnesses. Screenshots now record the settled action state.

Each player may supply `shared_witnesses`: a list of objects containing integer
`field_id`, lowercase full-logical-field `sha256`, integer `offset` and `size`
within that field, and positive `min_nonzero_bytes` within the selected span.
Seed validation and preflight check these before requests or client launch.
Travel verification requires the exact journal source and repeats the checks on
both source and staged destination, including logically rekeyed inventory.
Receipts retain hashes and span counts; no save bytes are printed.

Choose the span from the fixture's actual ABI. Party field `0x0101` starts with
its count byte: offset0/size1 catches an empty party even when stale nonzero
bytes remain elsewhere. A whole-field nonzero check would miss that failure.
Use PC-mon, inventory and custody spans backed by their actual layout and ROM
interaction evidence. A hash and nonzero span alone do not prove Pokémon
validity, species, occupied slots or usable items. An omitted/empty list provides
no populated-data evidence. Existing empty v7m fixtures remain historical
round-trip proof; they do not become populated proof through this option.

`live_author_fixtures.py` creates test-only populated sources through ROM menu
inputs, production gift/item functions, normal Save and cold Continue. It never
patches saves or contacts the server. Add `population_recipe` to each player:

```json
{"abi":"hoenn-box80-v1","party_species":[1,2,3,4,5,6],"party_level":5,
 "pc_species":[7],"bag_items":[[2,3]]}
```

Also set `authoring_menu_profile` to `hoenn-debug-v1`. The first driver supports
empty party/PC sources, six party gifts and one PC gift, species1–32, levels1–20
and Great Balls1–20. These are fixture itinerary limits, not game content limits.
Choose different species/levels/quantities for the second player. This ABI can
serve further regions sharing the same Pokémon storage; other families need an
explicit ABI and menu driver. The source retains its existing lineage ancestry.
This does not create proof of a fresh new game or migrate legacy saves.

```powershell
python tools/coop/live_author_fixtures.py --plan S:\cormoria-build\authoring-plan.json --output-root S:\cormoria-build\authored-fixtures --mgba-config "$env:APPDATA\mGBA\config.ini" --world-id 1
```

Preflight verifies the signed family, source hashes, recipe and tested keyboard
mapping. Each sandbox lives on the spare volume with its own appdata and logs.
Cache identity includes ROM, emulator, descriptor, seed, configuration, recipe
and relevant driver/validator/oracle/control code hashes. A complete unchanged
cache revalidates and reuses its retained save and screenshot receipts. Partial
or changed caches fail rather than overwrite evidence; no rebuild or reinstall
occurs. The driver checks C: before each player and closes only its owned emulator.

Seed hash, identity and sandbox bytes come from one read. Population hash,
validation and immutable output also use one read, with trainer-lineage and
next-generation checks. The published receipt follows cold reload of those
exact retained bytes. Inspect `cold-party.png` separately: byte checks and a
captured image do not recognize successful Continue. Append the retained save
to that player's ordered lineage and use its `shared_witnesses` in a fresh-account
travel plan. Never reseed an advanced account or replace its played head.

Current evidence: the versioned driver authored A's generation7 and B's
generation8, with distinct six-mon parties, one boxed mon and Great Balls3/7.
Both inspected cold screenshots show valid named parties at levels5/7. A second
unchanged invocation reused both validated caches without launching an emulator.
These versioned saves have SHA256 270b0358d243152cb374d0e13b918d512568467d9bf46041b195aaf050f88d9e
and 26b0374679b31cfecabcee3fe9339d0ebd6345ca5deea2c167c11e6b36af2280.
Historical temporary-script observations remain separate and do not identify
these saves. The first versioned attempt's focus failure is retained; the
owned-target-thread focus correction subsequently passed live authoring.

The fresh-account v7n `journey` passed Main→Cormoria→Main in one invocation,
reusing the unchanged signed v7m ROM family. Each player passed27 shared and16
world-local field checks, three populated witnesses and exact ferry-source
inspection on each leg, with zero runtime exceptions. The same co-op group
returned to Hoenn. Both destination harbor screenshot pairs were inspected.
Results, private captures and logs are under
`S:\cormoria-build\harness-v7n-populated`. Source semantic validation and logical
field equality prove preservation of these party/PC/Bag fixtures; destination
party-menu usability, populated daycare/mail and campaign acceptance remain
unverified. Do not rerun source seeding or the original journey against these
now-advanced player heads.

Signed cache readiness requires actual client-written acceptance heads and
generation markers as well as verified artifact bytes. Every retained accepted
generation must have its matching completion marker. Newer conflicting or
malformed orphan markers fail before launch. Executable cache paths must be
plain, canonically resolvable directories; Windows reparse paths and the current
S: alias cannot serve this signed client's executable cache. This fixture uses
regular C: profiles and hardlinks to existing immutable artifacts, while large
generated outputs stay on the spare volume. The real client accepted these
artifacts once and requested its normal restart; acceptance records were never
fabricated. Latest gate hardening passed focused checks and a final real-cache
preflight, separately from the completed live journey.

Aggregate harness checks73/73 pass. Independent review closed the seed race,
validator, focus and signed-cache findings. Remaining shortest path: populated
daycare/mail fixtures and focused campaign/reward save-reload checks, then
services/minigames/story-local co-op acceptance. Batch the next ROM family once
the new starter/reward fixes and their acceptance inputs are ready; the proved
signed v7m family does not contain those later script changes. No legacy-save
migration is included.

`live_fixture_custody.check_custody` supplements population checks with the
explicit `hoenn-mail-daycare-v1` fixture ABI. A recipe must name written
party-held mail, PC mailbox mail and a deposited mail-bearing Pokémon, with
exact sender/word/carrier metadata. It validates encrypted Pokémon checksums,
identity and held-item/mail-index relationships, and exports full logical
party, Mail and Daycare field witnesses. Merge witnesses by field ID and
require identical hashes when population and custody both name the party.
Those party bytes prevent stale Mail records from hiding a lost attachment
between fixture authoring and the exact ferry capture. Ordinary species are
supported; Unown's encoded mail species are explicitly excluded.

Set a player's optional `custody_recipe` to require this semantic validation
on its pinned final source. Both signed-client preflight and lineage seeding
run the same read-only admission helper before launching or calling seed APIs.
Include all three exact `shared_witnesses` returned by `check_custody` in the
plan, merging an identical population Party witness once. Missing, partial,
stale or duplicate fingerprints and a present null/malformed recipe stop the
run. Tests also require rejection of player B after A passes to leave both
client launches and seed API calls untouched. Omitting the recipe preserves
existing population-only plans; this does
not confer custody coverage on them.

Daycare steps advance during ordinary movement. An authored full-field witness
can therefore become stale while approaching the ferry. Author at the intended
interaction point or explicitly validate the actual departure state before a
custody-bearing journey; retain exact source-to-destination equality. Initial
fixture admission and synthetic tests do not prove that live itinerary ready.

The signed descriptor proves Mail576bytes and Daycare288bytes. Internal member
offsets follow source/APCS-GNU layouts, not extracted DWARF. Padding is ignored
for semantic interpretation and included in full-field hashes. Canonical unit
bytes are never admitted as ROM-written player saves. The initial normal-mail
probe stopped before Save because foreground activation failed. A later
standalone scripted-button probe produced a normal ROM Save with one held
Harbor Mail, then cold-loaded it and displayed APOLOGIZE from the actual sender.
This proves one player's first held Mail persistence and usability. PC mailbox,
daycare and custody-bearing travel remain unverified.

`live_scripted_input.py` controls standalone fixture authoring through mGBA Lua
button and screenshot APIs. It does not write emulated memory or patch saves.
It bounds each atomic numbered request and closes only its owned emulator.
Never add this script to signed-client bridge files. The signed ROM pair can
remain unchanged while authoring controls change; the authoring cache includes
the relevant helper and validator hashes.

`live_author_mail.py` versions the first held-Mail itinerary and validates the
normal Save generation, original carrier identity, other party/PC records,
sender, words and exact Bag quantities before retaining an immutable checkpoint.
Its first unattended runs exposed a delivery-message timing boundary: later
inputs reached the Bag instead of Save, and the disk saves remained the original
generation. The repaired sequence separately finishes and dismisses the gift
message, closes the Bag, and waits for both Save prompts. Both players passed
normal Save and cold Continue on the unchanged v7m family: A generation7→8,
B generation8→9, original carrier identities and other party/PC data preserved,
original Bag quantities plus two remaining Harbor Mail. Both unchanged-input
cache retries returned identical receipts. Retained evidence is under
`S:\cormoria-build\authored-mail-20261003`; rejected partial outputs remain
available for diagnosis. This is first held-Mail checkpoint coverage, not PC
mailbox, daycare or custody-bearing travel. A semantic save rejection is not
successful fixture creation.

The two sources have distinct account/character IDs, Pokémon and inventory
sentinels, but currently share their in-game trainer ID and sender name. They
must not be described as distinct ROM trainers. A future fixture pair should
use separately ROM-authored trainer identities before relying on Mail sender
data to detect cross-player swaps. Changing a trainer ID midway through an
existing lineage is not valid ancestry for the current seeding validator.

`live_author_harbor.py` starts without a save and authors a generation-one
harbor lineage through New Game and normal ROM Save. Automated A/B runs passed
on the unchanged v7m family, with different names and trainer IDs, empty party
and PC, and exact cold-reloaded bytes. A has money3000; B uses the existing ROM
debug menu before its first Save to set money999999 and a new trainer ID. Both
save harbor13/10/warp0 at(9,12), which is not the proved ferry interaction point.
Their unchanged-input retries return verified caches without opening an emulator.

The author advances dialogue only until the pinned default Boy/Girl menu is
visible, then requires a blank naming field with A selected before typing.
Three small screenshot crops, the source-backed cursor animation handling,
Pillow version and helper hashes bind these gates to the cache. Initial failures
exposed fixed-count dialogue drift and animated-cursor pixel differences; retain
their screenshots and logs as diagnostic evidence. Later menus still require
inspected screenshots and semantic Save acceptance. Fresh harbor saves must
subsequently gain population and custody through ROM-written generations before
seeding new travel actors. These empty saves do not extend the prior populated
travel result to distinct ROM trainers or establish custody travel.

```powershell
python tools/coop/live_author_harbor.py --plan S:\cormoria-build\harness-v7n-populated\plan.json --name A --output-root S:\cormoria-build\authored-harbor-20261003 --mgba-config <pinned-config.ini>
```

Repeat with `--name B`. Reuse the verified receipt and generation-one save as
immutable lineage roots; do not change trainer identity after that first Save.

`live_author_population.py` consumes these verified roots without reauthoring
them. Both players passed normal generation-two Save and exact cold reload:
A species1–6 at level5, PC7 and Great Balls3; B species10–15 at level7, PC16
and Great Balls7. Generated Pokémon checksums and OT identities match each
player's original ROM trainer; money and harbor location are preserved.
An independent cache review caught unchecked exported ancestry. Reuse now
requires exactly the canonical generation-one and generation-two paths/hashes.
The corrected validator passed A's retained earlier-producer capture offline;
B's current CLI cache retry passed. Do not describe A as a current-CLI cache hit.

```powershell
python tools/coop/live_author_population.py --plan S:\cormoria-build\harness-v7n-populated\plan.json --player b --harbor-root <verified-B-harbor-root> --output-root S:\cormoria-build\authored-population-20261003 --mgba-config <pinned-config.ini>
```

Both new sources now have validated generation-three held-Mail and generation-four
PC-Mail checkpoints. `live_author_mail.py` accepts the proved harbor warp0 or255
and preserves the exact source location. `live_author_pc_mail.py` moves the first
letter to PC slot6 and gives replacement Mail to the second carrier, leaving one
Mail in the Bag. Each checkpoint passed exact cold-byte, semantic, ancestry and
independent artifact validation; unchanged-input CLI retries reused the cache.
B's Daycare custody and cross-region travel for these new sources remain unproved.
A new journey
must use fresh test accounts and the actual ROM-written ancestry, leaving the
previously travelled accounts intact.

A's later standalone probe now proves ROM-written Daycare custody and a cold
departure checkpoint at Main harbor8,11 (generation7). The third carrier's
letter survives inside Daycare; the second carrier's held letter, PC mailbox
letter, surviving Party, Bag and PC passed exact semantic comparisons. A position
gate first rejected generation6 at9,12; only the final movement was repaired
before saving again. Daycare steps advanced4 to6 and the final exact value must
be pinned in departure witnesses. Cold Continue displayed five Party Pokémon
and retained identical save bytes. B's Daycare authoring and travel with these
custody-bearing sources remain unproved. This probe is retained evidence, not a
current automated Daycare cache receipt.

`live_author_third_mail.py` now caches the normal generation-four to-five
checkpoint separately. It verifies both earlier Mail parents, gives the last
Bag letter to the third carrier, and validates exact identity, existing letters,
PC, Bag and harbor location before retaining the save. B's automated author and
cold load passed; the unchanged-input cache retry returned the same receipt in
about1.24seconds. A's earlier manual checkpoint passed the new semantic oracle
offline; it is not a current-driver CLI cache receipt.

```powershell
python tools/coop/live_author_third_mail.py --plan <authoring-plan.json> --player b --pc-mail-root <verified-B-PC-Mail-root> --held-mail-root <verified-B-first-Mail-root> --output-root S:\cormoria-build\authored-third-mail-v7o-20261003 --mgba-config <pinned-config.ini>
```

Standalone Lua inputs cannot be substituted for signed-client travel inputs:
the authenticated runtime binds its managed Lua scripts and has no gameplay
input API. For the unchanged fixture, first check only both signed clients'
cold Continue, owned-window targeting and published presence. Stop on any focus,
Continue or presence failure before attempting departure. Then check one travel
boundary with exact journal source and destination ROM evidence before the
return leg. Do not modify signed scripts or replay a whole journey to diagnose
a focus failure.

For standalone interactive probe controls, use `live_probe_control.py` instead
of writing a watched `control-NNNN.json` directly. The live consumer once read
an empty file during a PowerShell write; it stopped and closed its owned emulator
before depositing in Daycare. The producer validates bounded inputs, writes an
exclusive temporary file, flushes/fsyncs, then publishes the complete JSON with
a non-overwriting rename on Windows. Invalid, duplicate or partial controls
must never become visible as ready input. The normal scripted Lua driver already
uses atomic publication and is unchanged.

```powershell
python tools/coop/live_probe_control.py --root <owned-probe-directory> --sequence 1 --label daycare-page --actions-json '[{"mask":1,"hold":8,"wait":300}]'
python tools/coop/live_probe_control.py --root <owned-probe-directory> --sequence 2 --done
```

Run the standalone checkpoint for each existing populated source, without
rebuilding or reseeding the already-travelled accounts:

```powershell
python tools/coop/live_author_mail.py --plan S:\cormoria-build\harness-v7n-populated\plan.json --player a --output-root S:\cormoria-build\authored-mail-20261003 --mgba-config <pinned-config.ini>
```

Repeat with `--player b`. An unchanged-input retry revalidates and returns the
retained cache. A rejected partial cache is diagnostic evidence, not a reusable
fixture. The gift text ends with `PAUSE_UNTIL_PRESS`: finish printing with
released keys, dismiss it once, then close the Bag with a separate B press
before selecting Save. These menu boundaries require inspected screenshots;
the final semantic Save gate alone does not identify the earliest UI mismatch.

Windows game focus first uses the normal foreground request and owned-thread
attachment. A timeout can attempt a click only on the verified owned title bar:
target raising is asynchronous, WindowFromPoint/GetAncestor must identify that
target, and a bounded WM_NCHITTEST must return HTCAPTION before and after cursor
movement. Occlusion, client pixels, hit-test timeout or failed foreground
verification stop before gameplay input. No global ALT input is used. A real
caption-only check passed with zero gameplay inputs/unchanged save, while the
target was already foreground; recovery from the observed denial still needs
live proof. The later delayed probe failed specifically at `cursor_move_error_0`.
A same-position cursor check outside the emulator reproduced that failure,
although cursor reads succeeded and both desktop handles reported `Default`.
That does not establish the cause or successful recovery. Do not rerun the
whole authoring itinerary for this failure. Focus errors now include the exact
caption boundary; failed movement sends no mouse click, and an unsuccessful
activation releases the mouse and stops before gameplay input. The focused
Windows suite passes24/24; the nine-suite harness aggregate passes90/90.

Current custody authoring checkpoint (2026-10-03): both A and B now have current automated generation-five third-Mail and generation-six Daycare cache receipts. Each final save cold-loads at Main ferry8,11 with distinct ROM trainer identity, five Party Pokémon, PC Pokémon, Bag, a held letter, PC mailbox letter and a Daycare Pokémon carrying the third letter. Exact retained/cold bytes, canonical six-generation ancestry and full custody witnesses passed independent validation. Unchanged Daycare cache retries returned the same receipts in about1.36s/1.62s. Historical manual A generation-seven proof remains separate. These standalone checkpoints do not prove signed presence, co-op membership or custody-bearing regional travel. The plan publisher is under independent review; real seed-receipt character IDs must replace draft IDs before profiles and journey.

`live_custody_plan.py` derives an immutable seed draft from the accepted current
custody caches. The parents JSON maps exactly `a` and `b`, each to
`held_mail_root`, `pc_mail_root`, `third_mail_root` and `daycare_root`.
It revalidates current signed inputs/dependencies, six ROM-written generations,
full custody witnesses and distinct actor identity before writing. Other
region/portal/leg settings stay in the base plan so another region can retain
its own itinerary. The current authoring adapter is Hoenn Main menu v1.

```powershell
python tools/coop/live_custody_plan.py --plan <authoring-plan.json> --parents-json <parents.json> --mgba-config <pinned-config.ini> --output <spare-volume-seed-plan.json>
```

An identical publication retry reuses the immutable output. A different output
at that path fails. The draft's character IDs are placeholders and its safety
marker is advisory; existing journey commands do not enforce that marker.
Seed only fresh local test accounts through `live_seed_players.py`, validate
that each successful receipt matches the final source hash and lineage
revision, then apply the two distinct actual character IDs to a separate
journey plan before profile preparation or travel. Never reseed travelled
accounts. The v7o custody accounts are already seeded at revision6 and both
regular signed profiles are prepared; reuse them and their unchanged signed
fixture rather than registering or installing again.

`live_signed_presence.py` isolates the first signed runtime boundary without
reinstalling clients or sending departure/group inputs. It validates the whole
bounded cold-Continue prefix first, requires two distinct running game PIDs,
hashes each actual ROM and emulator against the unchanged fixture, captures
each step, then requires published presence for both actors. Any input, focus,
screenshot or presence failure stops subsequent inputs and closes owned
clients while preserving the first error. Its success proves transport
presence; inspect the retained screens for gameplay state and use the journey
oracles for travel and membership.

```powershell
python tools/coop/live_signed_presence.py --plan <verified-seeded-journey-plan.json>
```

The command defaults to the first leg. `--leg <name>` can select another
recorded source boundary; its actual running ROM must match that source world.
It never registers, seeds, prepares or builds. Mocked tests do not establish
live Windows focus or cold Continue; retain the live result separately.

The first v7o focused signed-presence execution passed live (2026-10-03): both actual Main ROM/emulator bindings, eight recorded Continue inputs, retained screenshots and published transport presence. Primary inspected both final harbor screens. Owned clients closed cleanly. This establishes live input/focus/Continue/presence for these unchanged inputs; the custody-bearing journey is a separate check. `S:\cormoria-build\harness-v7o-custody\presence-result-01.json` is its receipt.

The first custody-bearing v7o live leg reached Cormoria for both actors, including destination cold Continue/presence and retained group membership. The ordinary witness gate then stopped: Main boot normalized only the unused sixth Party slot hpLost lowbyte (A23/B27 to0), so the full authored Party witness was stale. Independent exact departure→stage projection passed27shared/16world-local fields without exceptions, and Mail/Daycare/Bag/PC stayed exact. A narrow test-only source attestation is under implementation; do not exempt Party or replace hashes without that proof. Both accounts are now advanced in Cormoria. Preserve their state and resume the return boundary only after focused verification; do not reseed or replay the Main departure. The return trip and full campaign remain unverified.

`live_attest_departure.py` handles the observed Hoenn loader normalization as a
separate test-only source attestation. It requires the five occupied Party
slots and exact unused sixth-slot sentinel, permits only hpLost lowbyte534 to
become0, pins each actual journal source, and preserves full Party/Bag/PC/Mail/
Daycare witnesses. No save or production helper changes. Exact source→staged
projection still allows no exceptions.

```powershell
python tools/coop/live_attest_departure.py --plan <original-seeded-plan.json> --leg main-to-cormoria --output <spare-volume-attested-plan.json>
```

It validates retained committed journals, identity, canonical gen6→7→8 ancestry,
all custody recipes, and existing strict verification before atomically
publishing an immutable evidence plan. Both original legs remain configured;
the new verified checkpoint binds the derived source fixture key for the
parked Main world. The v7o focused rerun passed for both actors:27shared fields,
16world-local fields, no exceptions, and both group members in CORMORIA.
This accepts the captured outbound boundary; it does not complete the return.
Existing `journey` always executes every leg, including when CLI `--leg` is
provided. Do not replay it against advanced accounts. A selected-return runner
must first fence actual current Cormoria heads, reject an already committed
return and retain both configured legs/parked Main proof. After any failure,
inspect journals/current heads before retrying. Return and final-code campaign
acceptance remain separate checks.


The guarded v7o return passed live on2026-10-03 at02:26:03Z.
`live_run_leg.py --plan <attested-plan.json> --leg cormoria-to-main`
fenced both exact current heads, released read leases, checked loaded source
bytes before input, and restored dormant Main state. Both players passed27
shared and16 world-local fields without exceptions, including full Party,
Bag, PC, Mail and Daycare witnesses; both co-op members returned to HOENN.
The receipt is `S:\cormoria-build\harness-v7o-custody\return-result-01.json`.
These accounts have completed the round trip: do not reseed or replay them.
Owned clients closed; the in-memory server remains necessary to preserve heads.
This proves the retained signed v7m pair, which predates later production fixes.
A fresh single-command roundtrip and final-code campaign remain unverified.


`live_roundtrip.py` composes the accepted factories, seed admission, signed
client preparation, public group setup, outbound attestation and guarded return.
Its fresh live orchestration is still under acceptance; completed v7o evidence
above remains separate. Supply two distinct environment usernames, the local
test password and one unused registration invitation when using `--register`;
the seed helper obtains B's separate invitation from A.

```powershell
python -B tools/coop/live_roundtrip.py --authoring-plan <plan.json> --mgba-config <pinned-config.ini> --cache-roots-json <factory-config.json> --output-root <existing-spare-directory> --register
```

The factory configuration declares `adapter: hoenn-debug-v1`,
`source_world_id: 1`, `group_mode: api-preformed`, explicit
`preformed_group_inputs`, and six `outputs` cache directories: harbor, population,
mail, pc_mail, third_mail and daycare. The configured legs select destination
world and portals; another destination ROM can reuse this orchestration. Other
authoring ABIs require an adapter. Do not reuse invitation-menu controls after
API group formation.

Optional `signed_cache_source` validates an existing signed generation and
hardlinks only its immutable signed artifacts and completion/envelope records
on the same volume. It never copies accounts, sessions or acceptance markers;
the signed client writes its own authentication and acceptance.

Input digests select an immutable run directory. Completed retries recertify
both retained crossings offline and perform no factory, seed, API or launch.
They require the exact existing `inputs.json`, plan hash, cleanup proof and
both leg reports; missing or changed inputs fail without being repaired.
Retained verification can run below the C: capacity floor using the internal
boolean `require_live_space=False`. Signature, artifact, catalog, lineage,
custody and projection checks remain mandatory. This option is not exposed in
the plan or CLI for live actions. Fresh or incomplete runs check capacity before
creating files or starting work. Offline recertification may append verification
logs on the spare volume.
Interrupted mutation phases stop for focused recovery. Preserve failed attempt
markers and advanced heads; never clear them to force a replay. Cleanup receipts
record graceful stop and observed desktop absence; verify detached-runtime
absence separately during live acceptance.


Windows capture paths use the extended namespace for the complete digest-named
SAV and its readers. The v7p run exposed a261-character path during the initial
source capture, before gameplay input. Native regressions now cover long-path
publication, serialized consumer references and ordinary/extended aliases in
departure output protection. Original failed attempt markers remain intact.

A helper fix changes the top-level input key. Do not start a newly keyed run
against already seeded actors: use a separately recorded focused recovery with
the retained signed plan, actor IDs and group. Existing `outbound` checks exact
seeded heads/group and loaded bytes before input; use a separate receipt root
so immutable cleanup receipts cannot be overwritten. Follow with guarded return
and both-leg recertification. This recovery proves its explicit scope; it does
not turn an interrupted top-level run into a completed cache receipt.


The focused v7p recovery committed both outbound crossings and cold-continued
both Cormoria ROMs. The collector also normalizes resolved containment paths
into the same namespace while enforcing C: exclusion. Its `group` return value
is a JSON file reference. The exact guarded return has not launched: C: dropped
below the2GiB floor after clients closed. Preserve the server, advanced actors,
all phase markers and saved boundary images. Recover storage before exact-head
verification and the selected return; never replay the outbound leg.

Current offline validation accepts the retained v7p outbound for both players
with 27 shared and 16 world-local fields, full custody and no exceptions:
`S:/cormoria-build/v7p-recovery-01/offline-outbound-receipt.json`. It proves no
current server head or return. The retained v7o completed journey also passes
both actual validators with current helpers, without API or launch:
`S:/cormoria-build/offline-preflight-20261003/v7o-both-legs-result.json`.
Neither receipt proves fresh single-command completion. Main has since advanced
to `17153772bc77c9abac15600a13930ac37e280040`; synchronize the preserved source
candidate before the next paired build, keeping the existing fixture unchanged.
