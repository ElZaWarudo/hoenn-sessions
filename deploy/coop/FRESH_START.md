# One-time fresh start of the pilot checkpoint

This runbook covers one data step of the first multi-world rollout: existing players start fresh. Run it
only inside the hold that [RELEASES.md](RELEASES.md) "First multi-world rollout" provides, after the new
server image and server catalog are promoted and before the workflow is rerun to deploy and activate them.
It needs explicit operator confirmation at execution time.

Why it exists: the multi-world server cannot decode a checkpoint written by the pre-multi-world build
(origin/main `17153772bc`). Its snapshot and prepare records lack `rom_world_id`, and its characters have no
world heads. A new server started on such a checkpoint fails closed and logs:

```text
co-op persistent repository: checkpoint decode failed; if this checkpoint was written by a pre-multi-world build, run `coop-server fresh-start` (deploy/coop/FRESH_START.md); otherwise restore from backup
```

Do **not** deploy the new image first and let it crash-loop. Every command below names the image and
catalog explicitly, so nothing depends on what `.env` currently says.

## What a fresh start keeps

| Kept (byte-identical entries)                                                                    | Reset or dropped                                                                                         |
| ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| Accounts: `users_by_name`, `users_by_id` (usernames, Argon2 password hashes)                     | Every character: revision 0, world revision 0, no active snapshot, no world heads, new Littleroot state  |
| Invitations: `invitations`, `invitation_issuers`, `invitation_expires_at`                        | Leases, snapshots, prepares, finalize/restore records, upload tickets and object records                |
| Login sessions (`access`, `refresh`, `families`), unless `--drop-sessions`                       | Groups, travel, trades, battles, ledger, progress feeds, realtime tickets                                |
| Character owner and `last_session_epoch`, so session fencing stays monotonic                     | Every other top-level key (allowlist, not denylist)                                                      |

Legacy snapshot IDs are tombstoned in `retired_snapshots` so a client journal that retries an old ID gets
a conflict and can never adopt an old object. The server never evicts tombstones (cap 1,024), so the
rebuilt set is capped at **512** to leave room for the new build:

1. In-flight IDs (`prepared`, `prepare_ops`, `restore_staging`) and active character heads, always all of
   them. If they alone exceed 512 the command refuses; `--allow-tombstone-overflow` lets them use the full
   1,024 cache, and only after review.
2. Committed history (`snapshot_by_revision`, `snapshots`).
3. Tombstones already in the checkpoint (their objects were already cleaned up).

The report gives `retired_snapshots_in_flight_and_heads`, `retired_snapshots_final`,
`retired_snapshots_bound`, `retired_snapshots_headroom` and `retired_snapshots_overflow`.

The command never touches Firebase Storage. Legacy save objects stay in the bucket, unreferenced. Do not
delete them as part of this procedure.

## Safety gates built into the command

- It runs only when invoked explicitly. Server startup never transforms state.
- It takes the server's PostgreSQL advisory lock (`1129271120, 1`). It refuses while a server holds it and
  holds it itself, so a server cannot start mid-run.
- `--expect-sha256` must equal the SHA-256 of the checkpoint payload you inspected. On a mismatch nothing is
  written.
- It transforms only a checkpoint that this build **cannot** decode **and** whose raw CBOR positively
  matches the origin/main schema: top-level keys are a subset of origin/main's 49 `State` fields and include
  all 25 non-defaulted ones, no character has `world_heads`, and no snapshot or prepare record has
  `rom_world_id`. Anything else (corruption, a checkpoint from a newer build) is refused.
- A checkpoint this build decodes is never rewritten: `already_fresh` when every character can resume, a
  refusal otherwise.
- One transaction creates `coop_pilot_checkpoint_archive` if missing (additive), inserts the original
  bytes, and rewrites `coop_pilot_checkpoint` (`id = 1 AND format_version = 1`). Before writing it decodes
  the new payload with this build twice and checks the invariants. Any failure rolls everything back.
- Re-running with the same `--expect-sha256` after a committed run reports `already_fresh` (exit 0) and
  writes nothing, as long as the current checkpoint still has no legacy characters. After a Rollback B
  (below) the archived digest makes it refuse with `already archived but … not fresh` until that archive
  row is exported and deleted.

## Placeholders

| Placeholder | Value                                                                                         |
| ----------- | --------------------------------------------------------------------------------------------- |
| `<NEW>`     | New server image, digest-pinned (`…/hoenn-coop-server@sha256:…`), from the promoted release   |
| `<OLD>`     | Server image currently in `.env` (`COOP_IMAGE`), for Rollback A                               |
| `<CAT>`     | SHA-256 of the promoted `server-build-catalog.json` (`/srv/hoenn/server-catalog/<CAT>/`)      |
| `<HEX>`     | SHA-256 of the current checkpoint payload, from step 4                                        |
| `<CURRENT>` | SHA-256 of the checkpoint payload just before a Rollback B                                    |

Run everything from the compose directory on the VPS, as the user that owns `backups/`.

## Procedure

1. **Pin and verify the new image.** Nothing runs yet.

   ```sh
   docker pull <NEW>
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose config --images
   sha256sum /srv/hoenn/server-catalog/<CAT>/server-build-catalog.json
   ```

   The server line must be exactly `<NEW>`, and the catalog digest must be `<CAT>`. If the server image is
   anything else, stop: an old image given `fresh-start` ignores the argument and starts a server.

2. **Stop the server and wait for it to exit.**

   ```sh
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose stop server
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose ps --all server postgres
   ```

   The server must show `exited`; postgres must be `healthy`. `stop` waits up to the 45 s grace period.

3. **Back up and copy the dump out of the rotation.** `backup.sh` deletes dumps older than seven days from
   `backups/`, so keep a separate copy under `backups/pre-fresh-start/` and one off-host.

   ```sh
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose run --rm --no-deps backup --once
   DUMP=$(ls -t backups/coop-*.dump | head -n 1); echo "$DUMP"
   mkdir -p backups/pre-fresh-start
   cp -p "$DUMP" backups/pre-fresh-start/
   (cd backups/pre-fresh-start && sha256sum "$(basename "$DUMP")" | tee "$(basename "$DUMP").sha256")
   docker run --rm --network none -v "$PWD/backups/pre-fresh-start:/d:ro" postgres:16-bookworm \
     pg_restore --list "/d/$(basename "$DUMP")" > /dev/null && echo pg_restore-list-ok
   ```

   Copy the dump and its `.sha256` off-host (for example `scp` to the operator machine) and run
   `sha256sum -c` there. Do not continue without `pg_restore-list-ok` and a matching off-host digest.

4. **Fingerprint the checkpoint.**

   ```sh
   docker compose exec -T postgres psql -U coop -d coop -Atc \
     "SELECT format_version, octet_length(payload), encode(sha256(payload), 'hex'), updated_at
        FROM coop_pilot_checkpoint WHERE id = 1"
   docker compose exec -T postgres psql -U coop -d coop -Atc \
     "SELECT to_regclass('coop_pilot_checkpoint_archive')"
   ```

   (Prefix these with the same three variables if `compose.yaml` already requires them.) `format_version`
   must be `1`. The 64-character digest is `<HEX>`. The archive table should not exist yet; if it does,
   stop and find out why.

5. **Dry run** (no writes; keep the report).

   ```sh
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose run --rm --no-deps server fresh-start --expect-sha256 <HEX> --dry-run | tee backups/pre-fresh-start/dry-run-<HEX>.json
   ```

   Abort unless every one of these holds:

   - `outcome` is `dry_run` and `current_payload_sha256` is `<HEX>`;
   - `before.users == after.users` and `before.characters == after.characters`;
   - `characters_at_revision_zero == after.characters` and `characters_without_owner == 0`;
   - `retired_snapshots_overflow == 0` and `retired_snapshots_final <= 512`;
   - `unparsable_snapshot_ids == 0`, or every unparsable ID is explained before continuing;
   - `dropped_keys` lists only gameplay maps from the table above;
   - sessions: `after.access_tokens`, `refresh_tokens`, `token_families` equal `before` (or are 0 with
     `--drop-sessions`).

   A refusal (exit 1) writes nothing; see the refusal table.

6. **Re-check the digest** with the step 4 query, and confirm the server is still `exited`. The digest must
   still be `<HEX>`.

7. **Run it.** Add `--drop-sessions` only if every player must log in again.

   ```sh
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose run --rm --no-deps server fresh-start --expect-sha256 <HEX> | tee backups/pre-fresh-start/fresh-start-<HEX>.json
   ```

   Exit 0 means `fresh_started` (or `already_fresh` on a rerun); exit 1 is a refusal with nothing written.

8. **Verify the archive and the new checkpoint.**

   ```sh
   docker compose exec -T postgres psql -U coop -d coop -Atc \
     "SELECT id, reason, format_version, octet_length(payload), encode(payload_sha256, 'hex'),
             encode(sha256(payload), 'hex') FROM coop_pilot_checkpoint_archive;
      SELECT encode(sha256(payload), 'hex') FROM coop_pilot_checkpoint WHERE id = 1"
   ```

   Exactly one archive row, `reason = fresh-start`, and both archive digests equal `<HEX>`. The checkpoint
   digest must equal the report's `new_payload_sha256`. Record it: it is `<CURRENT>` for a Rollback B as
   long as nothing has written since.

9. **Smoke-start the new server with the same pins.**

   ```sh
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose up -d --no-deps server
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose exec -T server curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:3000/health/ready
   COOP_IMAGE=<NEW> COOP_PHASE2_RELEASE_CATALOG_PATH=/srv/hoenn/server-catalog/<CAT>/server-build-catalog.json COOP_PHASE2_RELEASE_CATALOG_SHA256=<CAT> docker compose logs --since=5m server
   ```

   Health must print `200`. Log in once with a test account; do not start or save a campaign. The new image
   is now running on the fresh checkpoint, but `.env` still names the old one.

10. **Hand back to the workflow.** Continue with [RELEASES.md](RELEASES.md) "First multi-world rollout":
    rerun the workflow so `deploy-release.sh` records `<NEW>` and `<CAT>` in `.env` and the release is
    activated. Do not edit `.env` or activate by hand from this runbook.

### If `deploy-release.sh` exits 3 or 4 after the fresh start

Exit 3 means it rolled back to the previous (old) server and that server is healthy; exit 4 means the
rollback itself failed. Either way, **stop the server immediately**:

```sh
docker compose stop server
```

An old server on the fresh checkpoint lets players save in the old format. The new build cannot read those
records, so a later new deploy would need a second fresh start and would wipe that progress too. With the
server stopped, either fix the cause and rerun the workflow (the fresh checkpoint is still valid for the new
build), or choose one of the rollbacks below.

## Refusals

| Message                                               | Meaning / action                                                                                                   |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `advisory lock is held`                               | A server (or another run) owns the database. Stop it and wait for exit.                                            |
| `checkpoint sha256 is …, not the expected value`      | The checkpoint changed since step 4, or the digest was mistyped. Repeat from step 4.                               |
| `unsupported checkpoint format_version`               | Not a pilot v1 checkpoint. Stop and investigate.                                                                   |
| `is not a recognised origin/main … checkpoint`        | Corrupt, or written by a newer build. Do not force anything: restore from backup or escalate with the message.     |
| `decodes with this build but … cannot resume`         | Already a current-format checkpoint. It is never wiped. Investigate.                                               |
| `exceed the 512-entry tombstone headroom`             | Many in-flight saves. Review the dry run; rerun with `--allow-tombstone-overflow` only if accepted.                |
| `already archived but … not fresh`                    | The current checkpoint equals an archived original (after a Rollback B). Export and delete the archive row first.  |
| `legacy checkpoint is malformed` / `self-check`       | A shape this tool cannot prove safe. Nothing was written. Escalate with the report.                                |

## Rollback

Both rollbacks start with the server stopped (`docker compose stop server`, then confirm `exited`).

### Rollback A: old server on the fresh checkpoint

Use this to go back to the old image while keeping the fresh start, **only before the first save by a new
server**. The origin/main build decodes the fresh-start output (see "Rollback compatibility" below). After a
new-build save, the checkpoint holds snapshot records with `rom_world_id`, which the old build rejects; then
only Rollback B or a forward fix remains.

```sh
COOP_IMAGE=<OLD> docker compose up -d --no-deps server
```

Use the compose file that matches `<OLD>` (`deploy-release.sh` keeps `compose.yaml.bak.<ts>` and
`.env.bak.<ts>`). Any progress players then save under the old build is lost again by a later fresh start.

### Rollback B: restore the archived original

This puts the original checkpoint back byte for byte. Everything since the fresh start is discarded. Read
the current digest first (step 4 query) and use it as `<CURRENT>`. It equals the report's
`new_payload_sha256` if nothing has written since; if it differs, players have new progress that this
discards, so get explicit approval first.

Run the block with `psql -v ON_ERROR_STOP=1`, for example
`docker compose exec -T postgres psql -U coop -d coop -v ON_ERROR_STOP=1 <<'SQL'` … `SQL`, after
replacing both placeholders with literal 64-character hex digests:

<!-- fresh-start-sql:rollback-b -->
```sql
BEGIN;
DO $rollback$
DECLARE
  restored integer;
BEGIN
  IF NOT pg_try_advisory_xact_lock(1129271120, 1) THEN
    RAISE EXCEPTION 'co-op server advisory lock is held: stop the server first';
  END IF;
  UPDATE coop_pilot_checkpoint AS checkpoint
     SET payload = archive.payload, updated_at = now()
    FROM coop_pilot_checkpoint_archive AS archive
   WHERE checkpoint.id = 1
     AND checkpoint.format_version = 1
     AND sha256(checkpoint.payload) = decode('<CURRENT>', 'hex')
     AND archive.reason = 'fresh-start'
     AND archive.format_version = 1
     AND archive.payload_sha256 = decode('<HEX>', 'hex')
     AND sha256(archive.payload) = decode('<HEX>', 'hex');
  GET DIAGNOSTICS restored = ROW_COUNT;
  IF restored <> 1 THEN
    RAISE EXCEPTION 'rollback matched % rows, expected exactly 1: check <HEX> and <CURRENT>', restored;
  END IF;
END
$rollback$;
COMMIT;
```

Verify that the checkpoint digest is now `<HEX>` (step 4 query) and that the archive still has one row.
Then start the old image as in Rollback A. The new build cannot read this checkpoint.

The PostgreSQL advisory lock is taken for the transaction, so the block fails instead of racing a running
server. The `UPDATE` is pinned to both digests and must touch exactly one row.

**Fresh-starting again after a Rollback B.** The archive still holds `<HEX>`, so `fresh-start` refuses. With
the server stopped, export the archive row, verify the export, then delete exactly that row:

```sh
docker compose exec -T postgres psql -U coop -d coop -Atc \
  "SELECT encode(payload, 'base64') FROM coop_pilot_checkpoint_archive WHERE payload_sha256 = decode('<HEX>', 'hex')" \
  > backups/pre-fresh-start/archive-<HEX>.b64
base64 -d backups/pre-fresh-start/archive-<HEX>.b64 | sha256sum   # must print <HEX>
```

<!-- fresh-start-sql:delete-archive -->
```sql
BEGIN;
DO $delete$
DECLARE
  removed integer;
BEGIN
  IF NOT pg_try_advisory_xact_lock(1129271120, 1) THEN
    RAISE EXCEPTION 'co-op server advisory lock is held: stop the server first';
  END IF;
  DELETE FROM coop_pilot_checkpoint_archive AS archive
   WHERE archive.reason = 'fresh-start'
     AND archive.payload_sha256 = decode('<HEX>', 'hex')
     AND sha256(archive.payload) = decode('<HEX>', 'hex')
     AND EXISTS (SELECT 1 FROM coop_pilot_checkpoint AS checkpoint
                  WHERE checkpoint.id = 1 AND sha256(checkpoint.payload) = decode('<HEX>', 'hex'));
  GET DIAGNOSTICS removed = ROW_COUNT;
  IF removed <> 1 THEN
    RAISE EXCEPTION 'delete matched % rows, expected exactly 1: check <HEX>', removed;
  END IF;
END
$delete$;
COMMIT;
```

The delete only succeeds while the checkpoint itself holds the archived bytes, so the only copy of the
original is never removed. Then repeat the procedure from step 3.

A full database restore from the step 3 dump (README "Backups and restore drill") remains the last resort.

## Rollback compatibility

Tested with a checkpoint written by an actual origin/main (`17153772bc`) build and decoded by that same
build (details in `coop/crates/coop-server/tests/fixtures/README.md`):

| Checkpoint                                           | origin/main result                                              |
| ---------------------------------------------------- | --------------------------------------------------------------- |
| fresh-start output (with or without `--drop-sessions`) | decodes; login and lease acquire work at revision 0           |
| after a new-build login and world acquire            | decodes; login and lease acquire work                           |
| after the new build's first save                     | rejected: unknown field `rom_world_id` (Rollback A unavailable) |
