# One-time fresh start of the pilot checkpoint

Use this runbook only once: when an upgraded `coop-server` refuses to start because it cannot read
the existing checkpoint. The server log shows:

```text
co-op persistent repository: checkpoint decode failed: run `coop-server fresh-start` (see deploy/coop/FRESH_START.md)
```

The server fails closed and does not modify the checkpoint. This happens when a checkpoint written by
an older build holds snapshot records without `rom_world_id`. Characters from that build also cannot
resume, because they have no world heads.

The product decision is a **fresh start** for those players. There is no legacy save migration.

| Kept                                                                                           | Reset or dropped                                                                                             |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Accounts, usernames, password hashes (byte-identical)                                          | Every character: revision 0, world revision 0, no active snapshot, no world heads, new Littleroot start state |
| Invitations (codes, issuers, expiry)                                                           | Leases, snapshots, prepared/finalize/restore operations, upload tickets and objects records                  |
| Login sessions (access, refresh, families), unless `--drop-sessions` is passed                 | Groups, travel, trades, battles, ledger, progress feeds, realtime tickets, ROM handoff state                 |
| Character ownership and `last_session_epoch`, so session fencing stays monotonic               | Any top-level key the new build does not explicitly keep (allowlist, not denylist)                           |

Legacy snapshot IDs are added to `retired_snapshots`, up to its 1,024-entry bound. In-flight prepares go
first, then heads and history. A client that retries an old ID gets a conflict, so it can never adopt an
old object. The report gives `retired_snapshots_overflow` for any IDs that did not fit.

The command never touches Firebase Storage. Legacy save objects stay in the bucket, unreferenced. Do
not delete them as part of this procedure.

## Safety gates built into the command

- It runs only when invoked explicitly. Server startup never transforms state.
- It takes the server's PostgreSQL advisory lock (`1129271120, 1`). It refuses to run while a server
  holds that lock, and holds the lock itself so a server cannot start mid-run.
- `--expect-sha256` must equal the SHA-256 of the checkpoint payload you inspected. On a mismatch,
  nothing is written.
- One transaction does all of the following: it creates `coop_pilot_checkpoint_archive` if missing
  (additive), inserts the original bytes, and rewrites `coop_pilot_checkpoint` (`id = 1 AND
  format_version = 1`). Before writing, it re-decodes the new payload with this build and verifies the
  invariants. Any failure rolls the whole transaction back.
- It is idempotent. Re-running with the same digest after success reports `already_fresh` and exits
  0, and so does a checkpoint this build can already serve. A checkpoint whose characters already
  play on world heads is never wiped.

## Procedure (Docker Compose stack in `deploy/coop`)

1. Deploy the new image, but leave the server stopped. Keep the old image tag for rollback.

   ```sh
   docker compose stop server
   docker compose ps   # server must not be running; postgres healthy
   ```

2. Take and verify an extra backup (see README "Backups and restore drill").

   ```sh
   docker compose run --rm --no-deps backup --once
   ```

3. Inspect the checkpoint and record its digest:

   ```sh
   docker compose exec -T postgres psql -U coop -d coop -Atc \
     "SELECT format_version, octet_length(payload), encode(sha256(payload), 'hex'), updated_at
        FROM coop_pilot_checkpoint WHERE id = 1"
   ```

   `format_version` must be `1`. Copy the 64-character hex digest.

4. Dry run (no writes). Review `before`/`after`, `dropped_keys`, and `retired_snapshots_overflow`.

   ```sh
   docker compose run --rm --no-deps server fresh-start --expect-sha256 <hex> --dry-run
   ```

5. Run it for real. Add `--drop-sessions` only if every player must log in again.

   ```sh
   docker compose run --rm --no-deps server fresh-start --expect-sha256 <hex>
   ```

   Keep the printed JSON report: it includes `archived_payload_sha256` and `new_payload_sha256`.
   Exit status is 0 for `fresh_started`, `dry_run` and `already_fresh`, and 1 for any refusal.

6. Verify, then start the server:

   ```sh
   docker compose exec -T postgres psql -U coop -d coop -Atc \
     "SELECT id, archived_at, reason, encode(payload_sha256, 'hex') FROM coop_pilot_checkpoint_archive;
      SELECT encode(sha256(payload), 'hex') FROM coop_pilot_checkpoint WHERE id = 1"
   docker compose up -d server
   docker compose logs --since=5m server
   ```

   `/health/ready` should return 200. Log in with an existing test account. The first launch starts
   a new campaign in world 1, and its first save creates the character's world head.

## Refusals and what they mean

| Message                                       | Meaning / action                                                                                                       |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `advisory lock is held`                       | A server (or another run) owns the database. Stop it first.                                                             |
| `checkpoint sha256 is …, not the expected value` | The checkpoint changed since inspection, or the digest was mistyped. Re-inspect. Nothing was written.               |
| `unsupported checkpoint format_version`       | Not a pilot v1 checkpoint. Stop and investigate.                                                                       |
| `already archived but … not fresh`            | The current checkpoint equals an archived original, probably restored. Investigate before acting.                      |
| `legacy checkpoint is malformed` / `self-check` | The payload has a shape this tool cannot prove safe. Nothing was written. Escalate with the report.                  |

## Rollback

Before the server writes anything new, use either of these:

- Restore the step 2 dump.
- Copy the archived payload back:

  ```sql
  UPDATE coop_pilot_checkpoint
     SET payload = (SELECT payload FROM coop_pilot_checkpoint_archive WHERE reason = 'fresh-start'
                    ORDER BY id DESC LIMIT 1), updated_at = now()
   WHERE id = 1;
  ```

Then redeploy the previous image, because the new build cannot read the old payload. After players
create new progress on the fresh checkpoint, rolling back discards that progress.
