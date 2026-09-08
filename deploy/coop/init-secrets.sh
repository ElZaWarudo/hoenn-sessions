#!/usr/bin/env bash
set -euo pipefail
umask 077
cd -- "$(dirname -- "$0")"
if [[ -e secrets ]]; then
    printf 'secrets already exists; refusing to replace keys or credentials\n' >&2
    exit 1
fi
mkdir -m 700 secrets
openssl rand -hex 32 >secrets/database_admin_password
openssl rand -hex 32 >secrets/database_password
printf 'postgresql://coop:%s@postgres:5432/coop\n' "$(cat secrets/database_password)" >secrets/database_url
openssl rand 32 >secrets/signing_key
openssl rand 32 >secrets/invite_pepper
openssl rand -hex 24 >secrets/bootstrap_invite
# Compose file secrets are bind mounts: host file permissions apply in containers.
# Directory remains 0700; only the explicitly mounted files are readable by uid10001.
chmod 444 secrets/database_admin_password secrets/database_password secrets/database_url secrets/signing_key secrets/invite_pepper secrets/bootstrap_invite
printf 'Keys created. Install firebase-service-account.json into secrets (mode 0444). Keep this directory private and back it up securely.\n'
