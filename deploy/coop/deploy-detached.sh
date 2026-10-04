#!/usr/bin/env bash
# Run deploy-release.sh detached from the SSH channel that started it, so a
# dropped connection or a cancelled workflow can never interrupt a rollout or
# its rollback halfway.
#
#   start  launches `bash deploy-release.sh <args>` under setsid+nohup (nohup
#          only where setsid is missing) with stdin from /dev/null and all
#          output appended to <log-dir>/deploy-<key>.log; DEPLOY_LOG points the
#          rollback at the same file. When deploy-release.sh exits, its status
#          is written atomically to <log-dir>/deploy-<key>.status. Returns at
#          once. Refuses a key that was already started.
#   wait   polls the status file, streaming new log bytes, and exits with the
#          deploy's own exit status (see deploy-release.sh: 0, 1, 2, 3, 4, 5).
#          After --timeout seconds without a status it exits 124 and leaves the
#          detached deploy running. Safe to repeat after a lost connection.
#
# Usage: deploy-detached.sh start --key KEY [--log-dir DIR] -- <deploy-release.sh args>
#        deploy-detached.sh wait  --key KEY [--log-dir DIR] [--timeout S] [--poll S]
#
#   KEY        [A-Za-z0-9._-]{1,128}; the workflow uses <sha>-<run id>-<attempt>
#   --log-dir  default $HOENN_DEPLOY_LOG_DIR or /var/log/hoenn (created 0700)
#
# Environment: DEPLOY_RELEASE_SCRIPT overrides the deploy-release.sh next to
# this file. GHCR_USER/GHCR_TOKEN and HEALTH_* are inherited by the deploy.
set -euo pipefail
trap '' PIPE

MODE="${1:-}"
[ "$#" -eq 0 ] || shift
KEY=""
LOG_DIR="${HOENN_DEPLOY_LOG_DIR:-/var/log/hoenn}"
TIMEOUT=1200
POLL=5
while [ "$#" -gt 0 ]; do
  case "$1" in
    --key) KEY="${2:?--key needs a value}"; shift 2 ;;
    --log-dir) LOG_DIR="${2:?--log-dir needs a value}"; shift 2 ;;
    --timeout) TIMEOUT="${2:?--timeout needs a value}"; shift 2 ;;
    --poll) POLL="${2:?--poll needs a value}"; shift 2 ;;
    --) shift; break ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ "$KEY" =~ ^[A-Za-z0-9._-]{1,128}$ ]] || { echo "error: --key must match [A-Za-z0-9._-]{1,128}" >&2; exit 2; }
[[ "$TIMEOUT" =~ ^[0-9]+$ ]] && [[ "$POLL" =~ ^[0-9]+$ ]] || { echo "error: --timeout/--poll must be whole seconds" >&2; exit 2; }
LOG="$LOG_DIR/deploy-$KEY.log"
STATUS="$LOG_DIR/deploy-$KEY.status"

case "$MODE" in
  start)
    DEPLOY_SCRIPT="${DEPLOY_RELEASE_SCRIPT:-$(cd -- "$(dirname -- "$0")" && pwd)/deploy-release.sh}"
    [ -f "$DEPLOY_SCRIPT" ] || { echo "error: deploy script not found: $DEPLOY_SCRIPT" >&2; exit 1; }
    if ! ( umask 077 && mkdir -p -- "$LOG_DIR" ) || [ ! -w "$LOG_DIR" ]; then
      echo "error: deploy log directory $LOG_DIR is not writable; create it once with" >&2
      echo "       sudo install -d -m 0750 -o \"\$USER\" $LOG_DIR" >&2
      exit 1
    fi
    if [ -e "$LOG" ] || [ -e "$STATUS" ]; then
      echo "error: deploy $KEY was already started; use '$0 wait --key $KEY'" >&2
      exit 1
    fi
    ( umask 077 && : > "$LOG" )
    launcher=(nohup)
    if command -v setsid >/dev/null 2>&1; then launcher=(setsid nohup); fi
    # The inner shell ignores HUP (nohup) and outlives this SSH session; its
    # only job is to record deploy-release.sh's exit status atomically.
    DEPLOY_LOG="$LOG" DEPLOY_STATUS="$STATUS" "${launcher[@]}" bash -c '
      trap "" HUP PIPE
      bash "$0" "$@"
      status=$?
      printf "%s\n" "$status" > "$DEPLOY_STATUS.tmp" && mv -f -- "$DEPLOY_STATUS.tmp" "$DEPLOY_STATUS"
    ' "$DEPLOY_SCRIPT" "$@" >>"$LOG" 2>&1 </dev/null &
    echo "started detached deploy $KEY (pid $!); log $LOG"
    ;;
  wait)
    [ -e "$LOG" ] || [ -e "$STATUS" ] || { echo "error: no deploy $KEY was started in $LOG_DIR" >&2; exit 1; }
    offset=0
    deadline=$((SECONDS + TIMEOUT))
    stream() {
      local size
      [ -f "$LOG" ] || return 0
      size="$(wc -c < "$LOG")"
      if [ "$size" -gt "$offset" ]; then
        tail -c +"$((offset + 1))" -- "$LOG" | head -c "$((size - offset))" || true
        offset="$size"
      fi
    }
    while :; do
      if [ -f "$STATUS" ]; then
        stream
        status="$(cat -- "$STATUS")"
        [[ "$status" =~ ^[0-9]{1,3}$ ]] || { echo "error: malformed deploy status: $status" >&2; exit 1; }
        exit "$status"
      fi
      stream
      if [ "$SECONDS" -ge "$deadline" ]; then
        echo "error: deploy $KEY still running after ${TIMEOUT}s; it continues detached (log $LOG, status $STATUS)" >&2
        exit 124
      fi
      sleep "$POLL"
    done
    ;;
  *)
    echo "usage: $0 start|wait --key KEY [--log-dir DIR] ..." >&2
    exit 2
    ;;
esac
