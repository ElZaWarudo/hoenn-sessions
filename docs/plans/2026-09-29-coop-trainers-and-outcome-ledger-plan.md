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
