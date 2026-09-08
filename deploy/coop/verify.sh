#!/usr/bin/env bash
# Local-only deployment checks. Creates a disposable PostgreSQL container.
set -euo pipefail
cd -- "$(dirname -- "$0")"
export COOP_DOMAIN=coop.example.com COOP_FIREBASE_BUCKET=example.firebasestorage.app COOP_IMAGE=hoenn-coop:verification
docker compose config --quiet
bash -n backup.sh init-secrets.sh init-database.sh

testdir=$(mktemp -d /tmp/hoenn-deploy-check.XXXXXX)
container="hoenn-deploy-check-$$"
cleanup() {
    docker rm -f "${container}-server" >/dev/null 2>&1 || true
    docker rm -f "$container" >/dev/null 2>&1 || true
    case "$testdir" in /tmp/hoenn-deploy-check.*) rm -rf -- "$testdir" ;; esac
}
trap cleanup EXIT
cp init-secrets.sh "$testdir/init-secrets.sh"
bash "$testdir/init-secrets.sh"
test "$(stat -c %s "$testdir/secrets/signing_key")" = 32
test "$(stat -c %s "$testdir/secrets/invite_pepper")" = 32
test "$(stat -c %a "$testdir/secrets")" = 700
before=$(sha256sum "$testdir/secrets/signing_key")
if bash "$testdir/init-secrets.sh"; then exit 1; fi
test "$before" = "$(sha256sum "$testdir/secrets/signing_key")"

docker run -d --name "$container" --tmpfs /var/lib/postgresql/data --tmpfs /backups \
    --mount "type=bind,source=$PWD/backup.sh,target=/backup.sh,readonly" \
    --mount "type=bind,source=$PWD/init-database.sh,target=/init-database.sh,readonly" \
    -e POSTGRES_PASSWORD=isolated-deployment-test -e PGHOST=127.0.0.1 \
    -e PGUSER=postgres -e PGDATABASE=postgres postgres:16 >/dev/null
for _ in {1..30}; do
    if docker exec "$container" pg_isready -U postgres >/dev/null 2>&1; then break; fi
    sleep 1
done
docker exec -i "$container" bash -se <<'CHECKS'
mkdir -p /run/secrets
printf %s isolated-deployment-test >/run/secrets/database_password
createdb -U postgres coop
POSTGRES_USER=postgres POSTGRES_DB=coop bash /init-database.sh
test "$(psql -U postgres -tAc "SELECT rolsuper OR rolcreatedb OR rolcreaterole FROM pg_roles WHERE rolname = 'coop'")" = f
psql -U postgres -v ON_ERROR_STOP=1 -c 'CREATE TABLE backup_probe(id integer PRIMARY KEY); INSERT INTO backup_probe VALUES (42);'
bash /backup.sh --once
createdb -U postgres restored
pg_restore -U postgres -d restored --exit-on-error /backups/coop-*.dump
test "$(psql -U postgres -d restored -tAc 'SELECT id FROM backup_probe')" = 42
bash /backup.sh --once & first_backup=$!
bash /backup.sh --once & second_backup=$!
wait "$first_backup"
wait "$second_backup"
test "$(find /backups -maxdepth 1 -name 'coop-*.dump' | wc -l)" -eq 3
for archive in /backups/coop-*.dump; do pg_restore --list "$archive" >/dev/null; done
touch -d '10 days ago' /backups/coop-old.dump /backups/unrelated.txt
if PGHOST=127.0.0.1 PGPORT=1 bash /backup.sh --once; then exit 1; fi
test -f /backups/coop-old.dump
test -z "$(find /backups -name '*.partial' -print -quit)"
bash /backup.sh --once
test ! -f /backups/coop-old.dump
test -f /backups/unrelated.txt
CHECKS
if [[ -n ${COOP_VERIFY_IMAGE:-} ]]; then
    # Fake service-account material exercises parsing/mounts only. No Google
    # request is made: readiness checks database ownership, not bucket access.
    openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$testdir/fake.pem" 2>/dev/null
    python3 - "$testdir" <<'PY'
import json
import pathlib
import sys
root = pathlib.Path(sys.argv[1])
(root / "secrets/firebase-service-account.json").write_text(json.dumps({
    "type": "service_account", "client_email": "test@example.invalid",
    "private_key_id": "local-verification", "private_key": (root / "fake.pem").read_text(),
    "token_uri": "https://oauth2.googleapis.com/token",
}))
PY
    chmod 644 "$testdir/secrets/database_url"
    printf 'postgresql://coop:isolated-deployment-test@127.0.0.1:5432/coop' >"$testdir/secrets/database_url"
    chmod 444 "$testdir/secrets/database_url" "$testdir/secrets/firebase-service-account.json"
    secret_mounts=()
    for secret in database_url firebase-service-account.json signing_key invite_pepper bootstrap_invite; do
        secret_mounts+=(--mount "type=bind,source=$testdir/secrets/$secret,target=/run/secrets/$secret,readonly")
    done
    docker run -d --name "${container}-server" --network "container:$container" \
        --read-only --cap-drop ALL --security-opt no-new-privileges:true \
        "${secret_mounts[@]}" \
        -e COOP_SERVER_MODE=postgres-firebase -e COOP_SERVER_BIND_ADDR=0.0.0.0:3000 \
        -e COOP_DATABASE_URL_FILE=/run/secrets/database_url -e COOP_FIREBASE_BUCKET=example.firebasestorage.app \
        -e COOP_FIREBASE_SERVICE_ACCOUNT_FILE=/run/secrets/firebase-service-account.json \
        -e COOP_SIGNING_KEY_FILE=/run/secrets/signing_key -e COOP_INVITE_PEPPER_FILE=/run/secrets/invite_pepper \
        -e COOP_BOOTSTRAP_INVITE_FILE=/run/secrets/bootstrap_invite -e COOP_UPLOAD_BASE_URL=https://coop.example.com \
        "$COOP_VERIFY_IMAGE" >/dev/null
    for _ in {1..30}; do
        if docker exec "${container}-server" curl -fsS --max-time 2 http://127.0.0.1:3000/health/ready >/dev/null 2>&1; then break; fi
        sleep 1
    done
    docker exec "${container}-server" curl -fsS --max-time 2 http://127.0.0.1:3000/health/ready >/dev/null
    test "$(docker exec "$container" psql -U postgres -d coop -tAc 'SELECT count(*) FROM coop_pilot_checkpoint')" = 1
    docker restart "${container}-server" >/dev/null
    for _ in {1..30}; do
        if docker exec "${container}-server" curl -fsS --max-time 2 http://127.0.0.1:3000/health/ready >/dev/null 2>&1; then break; fi
        sleep 1
    done
    docker exec "${container}-server" curl -fsS --max-time 2 http://127.0.0.1:3000/health/ready >/dev/null
    printf 'Production image non-root startup and restart PASS (offline Firebase fixture)\n'
fi
docker run --rm --mount "type=bind,source=$PWD/Caddyfile,target=/etc/caddy/Caddyfile,readonly" \
    -e COOP_DOMAIN caddy:2-alpine caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
printf 'Compose, secrets, backup/restore, failure retention and Caddy checks PASS\n'
