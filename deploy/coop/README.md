# Co-op pilot: one VPS, PostgreSQL, Firebase Storage

This package targets one Linux VPS with about 4 GB RAM. Run one co-op server;
PostgreSQL holds accounts, groups, leases and save metadata, and a private
Firebase Storage bucket holds save artifacts. Live presence is rebuilt after a
restart. Android clients reconnect to the same HTTPS server URL.

The server reuses the existing co-op transactions and staged save publication.
The PostgreSQL adapter stores a versioned state document, suitable for a small
pilot. This is not a horizontally scalable deployment. PostgreSQL has no
per-operation service bill here, but VPS disk, memory and storage traffic remain
finite. Keep the current server until measured load justifies a larger redesign.

## Before the first deployment

1. Provision the VPS and a DNS name pointing to it. Install Docker Engine with
   Compose v2. Permit inbound 80/443 and restrict SSH to the operator. Do not
   open PostgreSQL or port 3000. The Compose file only publishes Caddy's ports.
2. Enable Firebase's Blaze billing plan, create/select the Storage bucket, and
   choose its location deliberately. A European bucket improves proximity to a
   European VPS but has different free allowances from eligible US locations.
3. Create a dedicated server service account. Grant bucket-scoped object access
   needed to read, create and delete objects (Storage Object User), not project
   Owner. Enforce public access prevention and uniform bucket-level access.
   Install `storage.rules` as deny-all for client SDK access. The server uses
   Google Cloud IAM, which must also remain private. Clients upload through
   authenticated, expiring server capabilities, never through service keys.
4. Do not add an age-based delete rule to the live artifact bucket. An old object
   can still be the active canonical save. Configure billing alerts and monitor
   Storage bytes, operations and egress; billing alerts are not spending caps.
5. Keep the matching generated `dist/bridge_manifest.json` in the build workspace.
   The Rust server embeds it for save validation. Do not substitute another ROM's
   manifest when rebuilding or upgrading.

Run the following from this directory on the target host only when ready to deploy:

```sh
cp .env.example .env
# Edit the domain, bucket and release image tag in .env.
bash init-secrets.sh
# Copy the service account JSON securely to secrets/firebase-service-account.json.
chmod 444 secrets/firebase-service-account.json
mkdir -m 700 backups
docker compose config --quiet
docker compose build server
docker compose up -d
docker compose ps
```

`init-secrets.sh` refuses to replace existing secrets. Signing key and invite
pepper must remain stable across restarts and database restores. Back them up
securely together with the release image and matching bridge manifest. Read the
bootstrap invitation locally from `secrets/bootstrap_invite` to register the
first player; do not paste secrets into logs or public issue trackers.

To invite another player, replace only `secrets/bootstrap_invite` with a newly
generated code (`openssl rand -hex 24`), keep its mode 0444, and recreate the
server container so the new secret file is mounted. Each code admits one account.
Reusing a consumed code across restarts does not reactivate it. Do not regenerate
the signing key or pepper when adding invitations.

For Android, enter the HTTPS server URL, signing key ID `pilot-v1`, and the
64-character `public_key_hex` printed in the server's startup log. Share that
public identity with players through a trusted channel. Never copy the private
`secrets/signing_key` file into the app or send it to players.

Compose file secrets use host bind mounts, not an encrypted secret vault. Files
are readable inside the specifically authorized containers, while the host
`secrets` directory remains mode 0700. The server runs as UID 10001 with a
read-only filesystem. Never commit this directory or include it in build context.
The application database role owns only its database and has no superuser,
role-creation or database-creation privileges. PostgreSQL's separate admin
password is mounted only in the database container. Initialization scripts run
only on a fresh volume; changing them does not modify an existing deployment.

The Rust image pins its compiler version. PostgreSQL and Caddy tags track their
major release security fixes; resolve and record image digests before a deployed
release, and retain the previous server image. Do not automatically pull and
restart a running game server.

## Persistence, limits and recovery

Database persistence includes replay protection, idempotency records, active
snapshot pointers and incomplete upload/restore ownership. Prepared uploads also
contain short-lived bearer URLs: protect database dumps as credentials, even
though authentication token indexes store fingerprints. A failed object upload
must not advance the active save pointer; retry with the same idempotency key.
Cleanup leaves zero-byte retirement markers on abandoned object keys. They
prevent delayed uploads from recreating garbage after the database declaration
has been retired. The server hides these markers from artifact reads and never
deletes them; do not remove them with a bucket lifecycle policy. Their object
count and request charges still contribute to Storage usage.

Save quotas bound declared storage per character to 256 MiB and finalized
snapshot history to 100 entries. These are admission limits, not automatic
rotation: once reached, new saves are rejected. Do not delete database rows or
objects by hand to free quota; they participate in retry and cleanup ownership.
For the pilot, watch quota consumption and resolve retention before a player
reaches these limits. A resume artifact may be up to 32 MiB, so actual save sizes
matter more than the number of accounts when estimating cost.

After a server restart, reconnect clients; don't expect ephemeral WebSocket
presence to survive. Stable signing keys preserve the identity clients trust.
Do not run two replicas, including during upgrades. A second database owner must
fail startup instead of accepting concurrent ephemeral sessions.

## Backups and restore drill

The backup container makes a custom-format `pg_dump` immediately and every 24
hours, checks the archive directory, atomically publishes it, and retains seven
days after successful backups. Failures retry after five minutes. Its healthcheck
fails if no completed dump is newer than 25 hours. Logs are capped for all
containers. A dump is transactionally consistent but still needs a restore drill.

Local dumps alone do not survive loss of the VPS. Enable the separately budgeted
VPS backups and regularly copy completed dumps and secrets to private off-host
storage. If using Firebase for off-host dumps, use a separate bucket or dedicated
backup prefix with its own seven-day policy; never apply that policy to saves.
Keep backup access private, encrypt exports, and retain the keys needed to restore.
Daily backups imply up to 24 hours of metadata loss; canonical objects created
after a restored database backup can remain unreferenced. Do not blindly delete
them during recovery.

Create an extra dump before an upgrade:

```sh
docker compose run --rm --no-deps backup --once
```

Test restoration into a disposable database, while the production `coop`
database remains untouched. Replace the dump filename with a completed archive:

```sh
docker compose exec -T postgres createdb -U postgres -O coop coop_restore_check
docker compose exec -T postgres pg_restore -U coop -d coop_restore_check \
  --no-owner --no-acl --exit-on-error < backups/coop-YYYYMMDDTHHMMSSZ.dump
docker compose exec -T postgres psql -U coop -d coop_restore_check \
  -c 'SELECT count(*) FROM coop_pilot_checkpoint;'
```

Only use a separate test server and test object bucket for a functional restore
drill; restored state retains object keys and pending cleanup operations. Do not
point an isolated restored server at the live bucket while the live server runs.
For disaster recovery, stop the server first, restore into a fresh database,
restore the matching keys and image, verify readiness and a known player's
latest save, then reopen access. Never use `docker compose down -v` on this stack.

## Deployment validation and rollback

After deployment, the operator should watch for 30 minutes and then daily:

- `docker compose ps`: database and server healthy; backup becomes healthy after
  its first successful dump. `/health/ready` should return 200 through HTTPS.
- Register two test players, form a group, play, save, restart only the server,
  reconnect and check both players' canonical saves and group state.
- `docker compose logs --since=30m server backup`: look for startup, repository,
  object-store or backup failures. Access logging is disabled because upload URLs
  carry capabilities. Do not enable request/header/body logging for diagnosis.
  Caddy's runtime error logs also remove request fields; bounded header/body read
  times prevent an unfinished upload from holding a request open indefinitely.
- Check free disk, container memory, backup age and Google Storage billing.
  Investigate at 70% VPS disk or persistent memory pressure, and before save
  quotas are reached. No latency/player-capacity claim is made without load tests.

If readiness stays unhealthy, saves fail, or revisions regress, stop client
access and investigate before restarting repeatedly. To roll back compatible
code, stop `server`, set `COOP_IMAGE` to the retained previous image, and run
`docker compose up -d --no-build server`. A changed persistence format may also
require its matching database backup; never run old code against an unknown
format. Do not recreate PostgreSQL to fix an application startup error.

## Budget

Planning allowance: roughly **EUR 10–12/month** for a small pilot, using the
previously discussed VPS, IPv4, VPS backups and light Firebase usage. Taxes,
domain registration and actual Google usage vary; this is not a hard cap or an
unlimited database offer. Storage regions, large resume files and frequent
downloads can change the bill. There is no managed database subscription in this
layout, but the operator owns patching, backups, restore checks and monitoring.

Reference behavior: [Compose secrets](https://docs.docker.com/reference/compose-file/secrets/),
[Caddy WebSocket proxying](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy),
[Firebase Storage rules](https://firebase.google.com/docs/storage/security),
[GCS conditional requests](https://docs.cloud.google.com/storage/docs/request-preconditions).

## Local verification

Run `bash deploy/coop/verify.sh` from a Linux/WSL checkout with Docker available.
It creates a disposable PostgreSQL container, tests backup/restore, failed-backup
retention, secret initialization, Compose and Caddy configuration, then removes
its test container. It neither uses cloud credentials nor starts this stack.
After building the image, set `COOP_VERIFY_IMAGE=hoenn-coop:persistence-verification`
when running that script to also exercise production-mode startup and restart
with the non-superuser database account and mounted test secrets. That check uses
an offline Firebase credential fixture and does not verify real bucket access.

The Rust suite is `cargo test -p coop-server`. Run the ignored PostgreSQL tests
separately with `COOP_TEST_DATABASE_URL` pointing at a disposable local PostgreSQL
instance whose test user can create databases:

```sh
COOP_TEST_DATABASE_URL=postgresql://postgres@127.0.0.1:55439/postgres \
  cargo test -p coop-server --lib durable:: -- --ignored --test-threads=1
```

Those tests create and drop only randomly named test databases. Never supply a
production database URL. Live Firebase authentication and a real two-player
internet session remain deployment smoke checks requiring the target project.
