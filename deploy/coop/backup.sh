#!/usr/bin/env bash
set -euo pipefail
umask 077
export PGPASSWORD
PGPASSWORD=$(cat /run/secrets/database_password)
export PGCONNECT_TIMEOUT=10

backup_once() {
    local destination temporary
    temporary=$(mktemp "/backups/coop-$(date -u +%Y%m%dT%H%M%SZ).XXXXXX.partial") || return 1
    destination="${temporary%.partial}.dump"
    if pg_dump --format=custom --no-owner --no-acl --lock-wait-timeout=30s --file="$temporary" \
        && pg_restore --list "$temporary" >/dev/null \
        && ln -- "$temporary" "$destination"; then
        # The same-filesystem hard link publishes atomically and refuses to
        # overwrite another scheduled/manual backup, even on a name collision.
        rm -f -- "$temporary"
        # Only retire this job's completed dumps after a successful new backup.
        find /backups -maxdepth 1 -type f -name 'coop-*.dump' -mmin +10080 -delete
        printf '%s database backup complete\n' "$(date -u +%FT%TZ)"
    else
        rm -f -- "$temporary"
        printf '%s database backup failed\n' "$(date -u +%FT%TZ)" >&2
        return 1
    fi
}

if [[ ${1:-} == --once ]]; then
    backup_once
else
    while true; do
        if backup_once; then sleep 86400; else sleep 300; fi
    done
fi
