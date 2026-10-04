# coop-server test fixtures

## `origin-main-17153772bc-checkpoint.cbor`

A pilot checkpoint payload (`coop_pilot_checkpoint.payload`, format_version 1) written by an actual
origin/main build, the last pre-multi-world server. The fresh-start tests in
`src/phase2/fresh_start_golden_tests.rs` use it.

- Origin commit: `17153772bc77c9abac15600a13930ac37e280040`
- SHA-256: `0a0d99a693f06e17d553b6dd753d87c5f47dfaaa1e13c46bee28b0de5ccdef33` (pinned by the tests)
- Size: 23,954 bytes
- Toolchain: rustc/cargo 1.96.0, `cargo test --offline --locked -p coop-server --lib`

### How it was made

1. A throwaway worktree of `17153772bc` was created outside this repository.
2. A throwaway test was appended to origin/main's `phase2::battles::tests` module. Small visibility tweaks
   let it call origin/main's own `persistent::encode` and the `phase2::tests` save helpers.
3. Using only origin/main's `Phase2App` and storage, the test:
   - registered `battlealice` / `password-a` and `battlebob` / `password-b` through operator invitations,
     acquired leases, formed a group (group invitation + accept), seeded finalized party saves, and ran a
     Wally battle to completion with commit grants;
   - logged both in (access, refresh, families);
   - issued a member invitation (`invitation_issuers`, `invitation_expires_at`) and added an unused
     operator invitation;
   - opened a ledger entry (two open entries in total);
   - registered `fixtureuser` / `correct horse battery staple` and, through the real APIs, prepared,
     uploaded and finalized a snapshot (`finalize_ops`), restored it (`restore_ops`), and left a prepared
     snapshot unfinalized (`prepared`, `prepare_ops`);
   - wrote `persistent::encode(&state)` to disk.
4. An open trade offer was attempted but origin/main refused it (`Conflict`) because of the active battle,
   so `trade_offers` is empty. Every other map listed above is populated.

Resulting counts: 3 users, 3 characters (revisions 1, 1, 2), 5 invitations, 1 issuer, 1 expiry,
3 access / 3 refresh / 3 families, 3 leases, 4 snapshots, 1 prepared, 1 finalize op, 1 restore op,
2 tickets, 1 group, 1 battle reservation, 2 ledger entries.

The throwaway test and patch are not part of this branch. Copies live with the build evidence
(`S:\cormoria-build\origin-main-golden\golden_probe.rs` and `origin-main-golden-probe.patch`).

### Rollback-compatibility probe

The ignored test `write_rollback_probe_inputs` (env `COOP_FRESH_START_PROBE_DIR`) writes this build's
fresh-start outputs of the golden checkpoint. The same throwaway origin/main worktree then decoded each with
origin/main's `State`, exactly like its `PostgresStateRepository::connect` (whole payload, no trailing
bytes), loaded it into an origin/main `Phase2App`, logged in, and acquired a lease:

| Input                                                   | origin/main decode | origin/main login + acquire  |
| ------------------------------------------------------- | ------------------ | ---------------------------- |
| fresh-start output                                      | decodes            | ok, revision 0               |
| fresh-start output with `--drop-sessions`               | decodes            | ok, revision 0               |
| fresh output after a new-build login + world acquire    | decodes            | ok, revision 0               |
| fresh output after the new build's first save           | fails: unknown field `rom_world_id` | n/a         |
