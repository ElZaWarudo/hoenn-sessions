# Co-op battles for every trainer, and the multiplayer outcome ledger

Decisions (2026-09-29): every trainer encounter can become a co-op battle;
until the ledger covers battles, each ROM applies vanilla rewards locally
(option A). The server ledger covers multiplayer outcomes only (Level 1):
trades and co-op battle results. Solo progress stays trusted.

Co-op battle rules chosen:
- Single-Pokémon trainers field a second Pokémon at the same level, from
  their own pool or the same species with a seeded personality.
- Gym leaders keep their full team in co-op (no three-Pokémon cap).
- A gym win gives the badge, TM and story flags to each player who has not
  earned them and is at the same story point; otherwise the partner helps.
- A player needs one usable Pokémon to join; each side brings one to three.

## A. Co-op battles for every trainer

Findings that shape the work:

- EXP is suppressed in co-op (`src/battle_util.c`, the engine-active branch)
  because level-ups during battle would desynchronize the per-turn digest,
  which hashes both parties. EXP must be applied after the party restore.
- The server anchors a member's party to the last finalized cloud save; on a
  route the live party (HP/EXP) rarely matches, so most reservations would
  fail. Local-reward mode anchors to the party records the ROM uploads.
- A trainer win currently becomes `CommitPending` and waits for a Wally-only
  save validator. Local mode completes the battle instead.
- Wally/Brock are hard-coded in three ROM places and in `trainer_encounter`;
  the server also requires both players on the trainer's exact map.
- `PreparePartnerParty` needs three usable mons per side; co-op opponents are
  capped at three (`halfTeam`).

Order (effort):

1. Server `reward_mode = Local`, per-member role (participant/helper), party
   records in the snapshot commit, generated trainer encounter catalog,
   partner on the same or a connected map (3-4 d).
2. Launcher forwards the party records (1 d).
3. ROM runtime: remove allowlists, reverse trainer lookup, 1-3 mons per side
   (2-3 d).
4. Single hook in `BattleSetup_StartTrainerBattle`: eligibility, 10 s partner
   offer, vanilla fallback that preserves the script flow, abort script,
   re-sight cooldown (3 d, ~6 B EWRAM).
5. Rewards: trainer flag, money (not for helpers), deferred EXP with
   evolution after the party restore (4-5 d, ~16 B EWRAM).
6. Second opponent mon for single-mon trainers, seeded identically (2 d).
7. Rematches; 8. gyms; 9. story battles (per-trainer responder scripts,
   checkpoint comparison, re-enable Wally).

Eligibility: grouped; partner presence fresh, visible, on the same or a
connected map within 12 tiles; trainer resolves to a server identity; not a
two-trainer approach, early rival, pyramid, trainer hill, secret base, sky
battle, or NPC-partner battle; ROUTE class during phase 1.

## B. Level 1 multiplayer outcome ledger

What exists: battle commit grants (Wally only), `CommitApplied`
acknowledgement, `pending_applied_commit` in finalize, dead trade staging
that the running ROM never applies.

Model: `LedgerEntry { commit_id, character_id, origin (battle|trade),
base_snapshot_id, base_revision, expected delta, status
Issued->Delivered->Applied|Voided }`, at most one open entry per character,
idempotent issuance, entries never expire while open, persisted through the
serialized `State`.

Expected deltas: a trade slot's exact 100-byte record and the outgoing
Pokémon gone from party and boxes; a trainer win's CSP1 bit and vanilla
trainer flag, with money bounded by the vanilla formula.

Finalize: a declared entry must match exactly or the snapshot is rejected;
an undeclared entry whose evidence already appears is rejected; no evidence
means a pre-apply save and is accepted; replays return the stored record;
lost uploads are redelivered from a new `GET ledger/open`.

Wire: reuse `BattleCommit`/`CommitApplied`; add `TradeCommit` (0x0119);
the ROM applies the trade, acknowledges, and requests a checkpoint with
controls locked. Bump the game protocol 2 -> 3.

Order (effort): coop-save accessors (2 d); ledger model and issuance (3 d);
finalize validation replacing the Wally-only path (3 d); trade issuance and
GET route (2 d); launcher redelivery (3 d); codec, ROM trade apply,
auto-checkpoint, manifest (4 d); end-to-end fault tests (3 d).

Already fixed while planning: SaveBlock1 vars were read at the stale 0x139C
offset (real 0x13FC); see `include/coop/save_layout.h`.

Follow-ups recorded while implementing B2/B3:
- Prune Applied/Voided ledger entries after a retention window (keep an id
  tombstone); issuance and lookup currently scan all entries.
- The ROM trade apply must checkpoint before any trade evolution: finalize
  requires the traded slot to hold exactly the server's record.
- Wally entries are voided when their CommitPending reservation expires or is
  cancelled, so a lapsed Wally battle cannot block trades forever.

Follow-ups recorded while implementing A4-A5 and B5-B6:
- A trade commit names a party slot. If the player reorders the party
  before it applies, the ROM rejects it and the open entry blocks that
  character's ledger. Either apply by personality/OT ID anywhere in the
  party, or let the server void an entry the ROM reports as rejected.
- The ROM refuses trade records that carry mail; the server should not
  issue a trade for a Pokémon holding mail.
- Helper/participant can disagree between server (last cloud save) and
  ROM (live trainer flag); money follows the ROM until the ledger covers
  battles.
- Co-op EXP learns new moves only into free slots (no replace prompt),
  applies EXP in opponent-slot order, and skips Pay Day, battle-special
  evolutions and match-call registration.
- The badge level cap is skipped in co-op battles (it projected only the
  local copy of the party).

Follow-ups recorded while implementing A7-A8:
- Rematches: `special BattleSetup_StartRematchBattle` has the same co-op hook
  as dotrainerbattle; the fallback starts the vanilla rematch. A player who
  has beaten the first battle is a participant (rematch prize); only the
  requester's ROM records the vanilla rematch win (flag, match-call
  registration, cleared rematch state). The server table mirrors
  `gRematchTable` and a test parses the C source.
- Gyms: the eight Hoenn leaders (and their rematches) are eligible; Kanto
  and Johto leaders and Brock's special stay vanilla. The partner's grants
  are `data/scripts/coop_gym_rewards.inc` (from each gym's `scripts.inc`)
  plus the TM in C; map redraws, Norman's walk-out cutscene and the
  match-call registration messages are not replayed on the partner.
- Server gym roles read the FLAG_BADGE01..08 flags of the last finalized
  save. A character that also progressed a Kanto campaign in the same save
  shares those flags; the ROM decides the rewards either way.
- A lost co-op gym battle (local mons still standing) releases the field
  instead of resuming the leader's script, which would grant the badge.
- A requester's co-op loss against a route trainer with local mons still
  standing resumes the trainer's post-battle script (defeat text) instead
  of releasing the field; no flag or money is given. Gyms already release.
  Fixed in A9: every requester loss (route, rematch, gym, story) releases.

### A9: Hoenn story battles

Rule: story flags go to both players if the partner is at the same story
point; otherwise the partner helps (EXP only, no money, no flags). The
requester's own post-battle script always runs as in vanilla after a win;
a requester loss with local mons standing releases the field (the trainer
stays unbeaten and its scene can replay; an object that walked up to the
player stays there until the map reloads). "Same story point" is the
concrete state the requester's script needed before the battle: the
battle's story flag clear, its story var at that value, and the trainer's
object not hidden. A GRANT battle's partner gets the pure flag/var/item
state of the post-battle script (`data/scripts/coop_story_rewards.inc`);
objects it hides that are on the partner's screen are removed by the
notice, as the next map load would. HELPER battles drive a scene the
partner's map cannot rebuild. ORDINARY entries only show text, so the
trainer flag decides as for a route trainer. Tables: `sCoopStoryBattles`
and `sCoopStoryTrainers` (`src/coop/trainer_rewards.c`), mirrored by
`HOENN_STORY_BATTLES` in `trainer_rules.rs` (a test parses the C tables
and the flag/var headers).

Inventory (from `data/maps/*/scripts.inc`; NI = trainerbattle_no_intro,
CS = trainerbattle_single with a post-battle script):

| Battle | Trainers | Map script | Mode | Post-battle script sets | Partner |
|---|---|---|---|---|---|
| Rival, Route 103 | MAY/BRENDAN_ROUTE_103_* | Route103 Rival (object) | NI | rival leaves; BIRCH_LAB_STATE 4, OLDALE_RIVAL_STATE 1, DEFEATED_RIVAL_ROUTE103, lab/Oldale rival shown | GRANT (flag clear, rival shown) |
| Rival, Rustboro / Route 104 | MAY/BRENDAN_RUSTBORO_* | RustboroCity and Route104 (optional) | NI | DEFEATED_RIVAL_RUSTBORO or DEFEATED_RIVAL_ROUTE_104 | HELPER: one trainer ID for two battles; the partner cannot tell which |
| Rival, Route 110 | MAY/BRENDAN_ROUTE_110_* | Route110 RivalTrigger (coord, ROUTE110_STATE 0) | NI | Dowsing Machine, rival bikes away, ROUTE110_STATE 1 | GRANT (var 0, rival shown) |
| Rival, Route 119 | MAY/BRENDAN_ROUTE_119_* | Route119 RivalTrigger (coord, ROUTE119_STATE 0) | NI | HM Fly + RECEIVED_HM_FLY, ROUTE119_STATE 1, Scott scene (SCOTT_STATE +1) | GRANT (var 0, no Fly) |
| Rival, Lilycove | MAY/BRENDAN_LILYCOVE_* | LilycoveCity Rival (optional) | NI | rival flies away, MET_RIVAL_LILYCOVE, rival's bedroom shown | GRANT (flag clear, rival shown; bedroom follows the partner's own rival) |
| Wally, Mauville | WALLY_MAUVILLE | MauvilleCity Wally | NI (called) | Wally/uncle leave, DEFEATED_WALLY_MAUVILLE, Verdanturf Wally shown, first Wally call, Scott scene | GRANT (flag clear, Wally shown) |
| Wally, Victory Road | WALLY_VR_1 | VictoryRoad_1F trigger (VICTORY_ROAD_1F_STATE 0) | NI after the legacy special (off) | DEFEATED_WALLY_VICTORY_ROAD, entrance Wally shown, state 1/2 | GRANT (var 0, flag clear); the partner's Wally waits at spot 1 |
| Wally, Victory Road exit | WALLY_VR_2 (+VR_3..5 rematches) | VictoryRoad_1F ExitWally | single / rematch | text | ORDINARY / rematch rule |
| Grunt, Petalburg Woods | GRUNT_PETALBURG_WOODS | PetalburgWoods (coord) | NI | grunt flees, researcher gives Great Ball, scene | HELPER |
| Grunt, Rusturf Tunnel | GRUNT_RUSTURF_TUNNEL | RusturfTunnel | NI | Devon Goods, Briney and Peeko scene, Rustboro state | HELPER |
| Grunts, Oceanic Museum | GRUNT_MUSEUM_1/2 | SlateportCity_OceanicMuseum_2F | NI | Archie scene, party healed, Devon Parts handed over | HELPER |
| Grunt, Jagged Pass | GRUNT_JAGGED_PASS | JaggedPass MagmaHideoutGuard | NI | BEAT_MAGMA_GRUNT_JAGGED_PASS | GRANT (flag clear, guard shown) |
| Grunts, Aqua Hideout | GRUNT_AQUA_HIDEOUT_1..4 | AquaHideout_1F/B1F/B2F | CS | text | ORDINARY (trainer flag) |
| Matt | MATT | AquaHideout_B2F | CS | submarine leaves; TEAM_AQUA_ESCAPED_IN_SUBMARINE, Lilycove grunts hidden | GRANT (flag clear, grunts shown) |
| Shelly, Weather Institute | SHELLY_WEATHER_INSTITUTE | Route119_WeatherInstitute_2F | CS | Aqua leaves, gift Castform (givemon), institute state | HELPER (gift Pokémon and scene) |
| Shelly, Seafloor; Tabitha, Mt. Chimney and Magma Hideout | SHELLY_SEAFLOOR_CAVERN, TABITHA_MT_CHIMNEY, TABITHA_MAGMA_HIDEOUT | their maps | single | text | ORDINARY (trainer flag) |
| Maxie, Mt. Chimney | MAXIE_MT_CHIMNEY | MtChimney Maxie | NI | Magma and Aqua leave; DEFEATED_EVIL_TEAM_MT_CHIMNEY, Cozmo and cookie lady flags | GRANT (flag clear, Magma shown) |
| Maxie, Magma Hideout | MAXIE_MAGMA_HIDEOUT | MagmaHideout_4F | NI after Groudon awakens | Groudon gone, Slateport states, GROUDON_AWAKENED | HELPER (pre-battle awakening only on the requester) |
| Archie, Seafloor Cavern | ARCHIE | SeafloorCavern_Room9 | NI | Kyogre awakens, weather, warp | HELPER |
| Grunts, Space Center | GRUNT_SPACE_CENTER_2/5/6/7 | MossdeepCity_SpaceCenter_1F/2F | NI | stair guard and center vars inside the Maxie/Tabitha scene | HELPER |
| Elite Four | SIDNEY, PHOEBE, GLACIA, DRAKE | EverGrandeCity_*Room | NI | DEFEATED_ELITE_4_*, door opens (Drake: fan counter) | GRANT (flag clear, ELITE_4_STATE = 1..4, i.e. in that room); the notice opens the door when the partner stands in it |
| Champion | WALLACE | EverGrandeCity_ChampionsRoom | NI | rival and Birch scene, Hall of Fame, credits | HELPER (never run on the partner) |
| Steven, Meteor Falls | STEVEN | MeteorFalls_StevensCave | NI | DEFEATED_METEOR_FALLS_STEVEN | GRANT (flag clear) |

Out of scope: Steven's multi battle with Maxie and Tabitha at the Space
Center (an NPC-partner battle), the Kanto early-rival mode (Hoenn's rivals
use no-intro battles), two-trainer approaches, frontier brains and Kanto.

Party size: gym leaders, Aqua/Magma admins and leaders, the Elite Four and
the champion field their whole team; rivals, Wally, Steven and grunts keep
the three cap and the second-mon rule. The Elite Four is a gauntlet: each
side's staged mons keep their HP and PP after each battle (the party
restore preserves the battle changes), exactly as vanilla carries the party
from room to room.

Wally: the legacy special (`Special_CoopBattleConsentBeginWally`) and its
ledger commit stay off; the script's `trainerbattle_no_intro` goes through
the shared co-op hook with local rewards. The server moved Wally from the
ledger to `Local` with the story rule and keeps his exact Victory Road map.
The ledger path remains for persisted `CommitPending` records; its tests
opt back in per thread.

Server: story battles are `TrainerRule::Story`; the requester is always
allowed (its own story script reached the battle, and Elite Four flags are
cleared on each run, so a lagging save must not refuse it); the partner
participates only in a GRANT battle at its story point, read from the
SaveBlock1 flags and vars of the last finalized save
(`ValidatedSave::event_var`, `COOP_SAVE_LAYOUT_VARS_START`). The ROM stays
authoritative for the local rewards.

Follow-ups recorded while implementing A9:
- A partner's story role on the server can lag its live ROM (last cloud
  save); money and grants follow the ROM.
- Story grants skip presentation-only parts (walk-outs, fly-away, Scott,
  submarine, fades). The Route 119 Scott and Mauville Scott `SCOTT_STATE`
  increments are applied.
- The no-intro story grunts that were already eligible as route trainers
  (Petalburg Woods, Rusturf, Museum, Space Center) are now HELPER: before
  A9 their partner got the trainer flag and prize without the story state.
- A requester loss in a scripted scene (a coord-triggered rival) leaves the
  approaching object where it stopped; re-stepping on the trigger replays
  the approach from there until the map reloads.

### B8: In-game trade UI (game protocol 4)

Entry point: ONLINE > "Trade with partner" (grouped only, between "Where is
my partner?" and "Leave group"); no start menu change. Flow: party menu,
checkpoint, `TradeOfferRequest` (0x0016), launcher creates the offer,
`TradeOfferStatus` (0x011B) back; the partner's launcher polls
`GET .../trade-offers/current` every 3 s and sends `TradeOfferReceived`
(0x011A); the partner's ROM prompts when the field is free (45 s), picks a
Pokémon, checkpoints and sends `TradeOfferDecision` (0x0017). Accepted
offers issue the ledger entries; the existing `TradeCommit` delivery
applies them. Mail and a player's last non-egg Pokémon are refused at
selection; eggs are allowed (the server allows them).

Server changes the UI forced:
- The existing offer anchored the partner's slot and revision at creation,
  but the partner picks after seeing the offer and checkpoints first. An
  *open* offer (no `partner_slot`/`partner_expected_revision`) re-anchors
  the accepting side to its current head, fence and chosen slot at accept
  time; strict offers keep the old rules.
- `own_pokemon` (personality, OT ID) on the offer and the accept: a slot
  that no longer holds the picked Pokémon is `Conflict` (stale head).
- Open offers read the initiator's slot at creation (mail refused before
  the partner is asked) and keep a species/level/egg/nickname summary.
- New `GET /v1/groups/{group_id}/trade-offers/current`; offer TTL 30 s ->
  60 s to fit poll, prompt, party menu and checkpoint.

Follow-ups recorded while implementing B8:
- Nicknames cross as the 10 boxed bytes; 11-12 character nicknames of this
  fork are shown truncated in the partner's prompt.
- A cancel lost to a transport outage can race the partner's accept; the
  trade then completes atomically through the ledger anyway.
- After an accept the field stays locked until the `TradeCommit` arrives
  (10 s cap); a commit later than that finds the Pokémon by personality,
  so moving it to the PC in that window still rejects the commit.
- The partner's Yes/No box is not closed when the requester withdraws; the
  withdrawal is shown when it is answered.
- Not verified with two real players yet.

## C. Friendly battles (4.6), decided 2026-09-29

- Singles and doubles, on the existing lockstep engine: each ROM stages the
  peer's party as the opponent and takes the peer's actions instead of AI;
  positions are mirrored so each player sees their own side at the bottom.
- The challenger picks "levels as is" or "scale all to 50" per challenge;
  the other player sees it before accepting (scaling uses a temporary copy
  like the badge level cap).
- The challenge sets a count of 1-6; each player picks that many Pokemon
  when accepting.
- No EXP, money or flags; parties are restored exactly.
- Fix first: an accepted friendly offer leaves CONSENT_ACCEPTED with no
  local deadline.
- Order: after the in-game trade UI.
