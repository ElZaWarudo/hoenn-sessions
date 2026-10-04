#!/usr/bin/env bash
# Roll the coop-server container to an immutable image and server catalog,
# verify health, and roll back automatically on failure.
#
# The image must be digest-pinned (name@sha256:<64 hex>); tags are never
# accepted because GHCR tags are mutable. The server catalog must be the
# promoted, content-addressed file
#   $HOENN_ROOT/server-catalog/<sha256>/server-build-catalog.json
# and its bytes are hashed on the host before anything changes. This script
# never builds images, never runs `docker compose down -v`, and only recreates
# the `server` service; databases, secrets, volumes and Caddy are untouched.
#
# Order: validate arguments and host catalog -> take the deploy lock -> pull
# the image (failure leaves every file untouched) -> write .env.next that
# preserves all other keys and lines -> validate it with the candidate
# compose file -> timestamped 0600 backups of .env and compose.yaml -> rename
# both into place -> `up -d` -> bounded /health/ready check. On a failed
# rollout, an unexpected error or a signal after the rename, the backups are
# restored byte-for-byte (original modes kept), the previous server is brought
# back up and health is checked again (or, with --rollback-to-stopped, the
# server is left stopped).
#
# The rollback must survive a dead caller: SIGPIPE is ignored, a lost SSH
# channel or a cancelled workflow (HUP/INT/TERM) after the swap triggers the
# restore, signals are ignored while restoring, errexit is off inside it, and
# every rollback line and docker output goes to a log file (DEPLOY_LOG, else
# <deploy-dir>/deploy-rollback.<stamp>.log) instead of stdout/stderr.
# Production runs this script detached through deploy-detached.sh.
#
# Exit status: 0 healthy;
#              1 refused before any change (lock held, pull failed, invalid
#                candidate configuration, missing files, ...);
#              2 usage;
#              3 rollout failed, files restored and the previous server is
#                healthy again;
#              4 rollout failed and the rollback did not complete (files not
#                restored, previous server not healthy, or it could not be
#                stopped): PRODUCTION IS POSSIBLY DOWN, operator required;
#              5 rollout failed, files restored and the server was left
#                STOPPED as requested by --rollback-to-stopped.
#
# Usage: deploy-release.sh --image <immutable-ref> --catalog-path PATH
#          --catalog-sha256 HEX [--deploy-dir DIR] [--env-file FILE]
#          [--compose-file FILE] [--rollback-to-stopped] [--validate-only]
#
#   --compose-file   version-controlled compose file to validate and install
#                    into <deploy-dir>/compose.yaml as part of the rollout
#   --rollback-to-stopped
#                    on failure restore the files but leave the server stopped
#                    instead of starting the previous image (first multi-world
#                    rollout: the old image must never serve a fresh-started
#                    database)
#   --validate-only  check arguments, image reference, host catalog bytes and
#                    the candidate .env/compose pair, then exit 0 without any
#                    login, pull, file change or restart
#
# Environment:
#   HOENN_ROOT        release root on the host (default /srv/hoenn; the server
#                     container mounts the same path read-only)
#   HOENN_DEPLOY_DIR  default deploy directory
#   HEALTH_TIMEOUT    seconds to wait for /health/ready (default 180)
#   HEALTH_INTERVAL   seconds between health probes (default 5)
#   DEPLOY_LOG        file that receives rollback output (deploy-detached.sh
#                     points it at the detached deploy log)
#   DEPLOY_BACKUP_KEEP  newest .env.bak.* / compose.yaml.bak.* files of each
#                     kind kept after a completed run (default 5)
#   GHCR_USER/GHCR_TOKEN  optional read-only GHCR credentials; the token must
#                     arrive via environment (piped stdin in CI)
set -euo pipefail
# A closed SSH channel must surface as a failed write, never kill the script
# halfway through a rollout or a rollback.
trap '' PIPE

IMAGE=""
CATALOG_PATH=""
CATALOG_SHA256=""
DEPLOY_DIR="${HOENN_DEPLOY_DIR:-$(dirname -- "$0")}"
ENV_FILE=""
COMPOSE_FILE=""
VALIDATE_ONLY=0
ROLLBACK_TO_STOPPED=0
HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-180}"
HEALTH_INTERVAL="${HEALTH_INTERVAL:-5}"
DEPLOY_LOG="${DEPLOY_LOG:-}"
DEPLOY_BACKUP_KEEP="${DEPLOY_BACKUP_KEEP:-5}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --image) IMAGE="${2:?--image needs a value}"; shift 2 ;;
    --catalog-path) CATALOG_PATH="${2:?--catalog-path needs a value}"; shift 2 ;;
    --catalog-sha256) CATALOG_SHA256="${2:?--catalog-sha256 needs a value}"; shift 2 ;;
    --deploy-dir) DEPLOY_DIR="${2:?--deploy-dir needs a value}"; shift 2 ;;
    --env-file) ENV_FILE="${2:?--env-file needs a value}"; shift 2 ;;
    --compose-file) COMPOSE_FILE="${2:?--compose-file needs a value}"; shift 2 ;;
    --rollback-to-stopped) ROLLBACK_TO_STOPPED=1; shift ;;
    --validate-only) VALIDATE_ONLY=1; shift ;;
    -h | --help) sed -n '1,68p' -- "$0"; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done

# Reject anything that is not an immutable reference. The charset gate runs
# first so no later expansion or .env line can carry shell-active characters.
case "$IMAGE" in
  "" | *[!A-Za-z0-9_./:@-]* )
    echo "error: --image contains unsupported characters: $IMAGE" >&2
    exit 2
    ;;
esac
if [ "$IMAGE" != "${IMAGE%:latest}" ]; then
  echo "error: refusing to deploy the mutable :latest tag; use a digest-pinned reference" >&2
  exit 2
fi
case "$IMAGE" in
  *@sha256:* )
    digest="${IMAGE##*@sha256:}"
    case "$digest" in
      *[^0-9a-f]* | "" ) echo "error: invalid image digest: $IMAGE" >&2; exit 2 ;;
    esac
    if [ "${#digest}" -ne 64 ]; then
      echo "error: image digest must be 64 hex chars: $IMAGE" >&2
      exit 2
    fi
    ;;
  * )
    echo "error: --image must be digest-pinned (name@sha256:<64 hex chars>); tags are mutable and never accepted, got: $IMAGE" >&2
    exit 2
    ;;
esac

case "$DEPLOY_BACKUP_KEEP" in
  "" | *[!0-9]* | 0 ) echo "error: DEPLOY_BACKUP_KEEP must be a positive integer" >&2; exit 2 ;;
esac
case "$CATALOG_SHA256" in
  "" | *[!0-9a-f]* ) echo "error: --catalog-sha256 must be 64 lowercase hex chars" >&2; exit 2 ;;
esac
[ "${#CATALOG_SHA256}" -eq 64 ] || { echo "error: --catalog-sha256 must be 64 lowercase hex chars" >&2; exit 2; }
case "$HOENN_ROOT" in
  "" | *[!A-Za-z0-9_./-]* ) echo "error: HOENN_ROOT contains unsupported characters" >&2; exit 2 ;;
esac
EXPECTED_CATALOG_PATH="$HOENN_ROOT/server-catalog/$CATALOG_SHA256/server-build-catalog.json"
if [ "$CATALOG_PATH" != "$EXPECTED_CATALOG_PATH" ]; then
  echo "error: --catalog-path must be $EXPECTED_CATALOG_PATH (promoted, content-addressed catalog)" >&2
  exit 2
fi

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "$1" | cut -d' ' -f1
  else
    "${PYTHON_BIN:-python3}" -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$1"
  fi
}

if [ ! -f "$CATALOG_PATH" ] || [ -L "$CATALOG_PATH" ]; then
  echo "error: server catalog is missing or not a regular file: $CATALOG_PATH" >&2
  exit 1
fi
if [ "$(wc -c < "$CATALOG_PATH")" -gt 65536 ]; then
  echo "error: server catalog exceeds the 64 KiB limit" >&2
  exit 1
fi
if [ "$(sha256_of "$CATALOG_PATH")" != "$CATALOG_SHA256" ]; then
  echo "error: server catalog bytes do not match --catalog-sha256" >&2
  exit 1
fi

if [ -z "$ENV_FILE" ]; then
  ENV_FILE="$DEPLOY_DIR/.env"
fi
ACTIVE_COMPOSE="$DEPLOY_DIR/compose.yaml"
if [ ! -f "$ACTIVE_COMPOSE" ]; then
  echo "error: compose.yaml not found in $DEPLOY_DIR" >&2
  exit 1
fi
if [ ! -f "$ENV_FILE" ]; then
  echo "error: env file not found: $ENV_FILE (copy .env.example first)" >&2
  exit 1
fi
if [ -n "$COMPOSE_FILE" ] && [ ! -f "$COMPOSE_FILE" ]; then
  echo "error: compose file not found: $COMPOSE_FILE" >&2
  exit 1
fi
CANDIDATE_COMPOSE="${COMPOSE_FILE:-$ACTIVE_COMPOSE}"

compose_with() {
  local env_file="$1" compose_file="$2"
  shift 2
  docker compose --project-directory "$DEPLOY_DIR" --env-file "$env_file" -f "$compose_file" "$@"
}

# Copy the current env file (keeping its mode) and replace only the three
# release keys in place; every other line, comment and key is preserved.
render_env() {
  local source="$1" destination="$2" line image_done=0 path_done=0 sha_done=0
  cp -p -- "$source" "$destination"
  {
    while IFS= read -r line || [ -n "$line" ]; do
      case "$line" in
        COOP_IMAGE=*)
          [ "$image_done" -eq 1 ] || printf 'COOP_IMAGE=%s\n' "$IMAGE"; image_done=1 ;;
        COOP_PHASE2_RELEASE_CATALOG_PATH=*)
          [ "$path_done" -eq 1 ] || printf 'COOP_PHASE2_RELEASE_CATALOG_PATH=%s\n' "$CATALOG_PATH"; path_done=1 ;;
        COOP_PHASE2_RELEASE_CATALOG_SHA256=*)
          [ "$sha_done" -eq 1 ] || printf 'COOP_PHASE2_RELEASE_CATALOG_SHA256=%s\n' "$CATALOG_SHA256"; sha_done=1 ;;
        *) printf '%s\n' "$line" ;;
      esac
    done < "$source"
    [ "$image_done" -eq 1 ] || printf 'COOP_IMAGE=%s\n' "$IMAGE"
    [ "$path_done" -eq 1 ] || printf 'COOP_PHASE2_RELEASE_CATALOG_PATH=%s\n' "$CATALOG_PATH"
    [ "$sha_done" -eq 1 ] || printf 'COOP_PHASE2_RELEASE_CATALOG_SHA256=%s\n' "$CATALOG_SHA256"
  } > "$destination"
}

LOCK_DIR=""
NEXT_ENV=""
NEXT_COMPOSE=""
SWAPPED=0
FINISHED=0
ENV_BACKUP=""
COMPOSE_BACKUP=""
ENV_MODE=600
COMPOSE_MODE=644
STAMP="$(date -u +%Y%m%dT%H%M%SZ).$$"

wait_healthy() {
  local deadline=$((SECONDS + HEALTH_TIMEOUT))
  until compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" exec -T server \
      curl --fail --silent --max-time 4 http://127.0.0.1:3000/health/ready >/dev/null 2>&1; do
    if [ "$SECONDS" -ge "$deadline" ]; then
      return 1
    fi
    sleep "$HEALTH_INTERVAL"
  done
}

file_mode() {
  local mode
  mode="$(stat -c %a -- "$1" 2>/dev/null)" || mode=""
  case "$mode" in
    [0-7][0-7][0-7] | [0-7][0-7][0-7][0-7]) printf '%s' "$mode" ;;
    *) printf '%s' "$2" ;;
  esac
}

# Restores the bytes and the recorded original mode (backups are 0600).
restore_file() {
  local backup="$1" target="$2" mode="$3"
  cp -- "$backup" "$target.rollback.$$" && chmod "$mode" "$target.rollback.$$" \
    && mv -f -- "$target.rollback.$$" "$target"
}

# Backups hold secrets: keep them 0600 and only the newest DEPLOY_BACKUP_KEEP
# of each kind. The UTC stamp makes lexical (glob) order chronological.
prune_backups() {
  local prefix candidate count index
  local -a backups
  for prefix in "$ENV_FILE.bak." "$ACTIVE_COMPOSE.bak."; do
    backups=()
    for candidate in "$prefix"*; do
      if [ -f "$candidate" ] && [ ! -L "$candidate" ]; then backups+=("$candidate"); fi
    done
    count="${#backups[@]}"
    index=0
    while [ "$index" -lt "$count" ]; do
      if [ $((count - index)) -gt "$DEPLOY_BACKUP_KEEP" ]; then
        rm -f -- "${backups[$index]}"
      else
        chmod 0600 -- "${backups[$index]}"
      fi
      index=$((index + 1))
    done
  done
}

# Best-effort line to the caller's original stderr (fd 7, saved before the
# rollback redirected everything to its log). Never fails.
notify() {
  { printf '%s\n' "$*" >&7; } 2>/dev/null || true
}

# Restores the pre-rollout files and either restarts the previous server or,
# with --rollback-to-stopped, stops it. Never returns.
rollback() {
  local reason="$1" log
  set +e
  trap '' EXIT INT TERM HUP PIPE
  if ! exec 7>&2 2>/dev/null; then exec 7>/dev/null; fi
  log="${DEPLOY_LOG:-$DEPLOY_DIR/deploy-rollback.$STAMP.log}"
  if ( umask 077; : >> "$log" ) 2>/dev/null; then
    exec >>"$log" 2>&1
  else
    log=/dev/null
    exec >/dev/null 2>&1
  fi
  notify "error: rollout of $IMAGE failed ($reason); restoring previous configuration (log: $log)"
  echo "error: rollout of $IMAGE failed ($reason); restoring previous configuration"
  compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" ps server
  compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" logs --tail=50 server
  if restore_file "$ENV_BACKUP" "$ENV_FILE" "$ENV_MODE" \
      && restore_file "$COMPOSE_BACKUP" "$ACTIVE_COMPOSE" "$COMPOSE_MODE"; then
    echo "restored $ENV_FILE and $ACTIVE_COMPOSE from $ENV_BACKUP and $COMPOSE_BACKUP"
    if [ "$ROLLBACK_TO_STOPPED" -eq 1 ]; then
      if compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" stop server; then
        echo "rolled back files; server left STOPPED (--rollback-to-stopped)"
        notify "error: rollout failed; files restored and the server is STOPPED as requested (exit 5; log: $log)"
        prune_backups
        cleanup
        exit 5
      fi
    elif compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" up -d --no-build server && wait_healthy; then
      echo "rolled back: previous server configuration is healthy again"
      notify "rolled back: previous server configuration is healthy again (backups $ENV_BACKUP, $COMPOSE_BACKUP; log: $log)"
      prune_backups
      cleanup
      exit 3
    fi
  fi
  echo "error: ROLLBACK FAILED; production is possibly down; inspect $ENV_BACKUP and $COMPOSE_BACKUP manually"
  notify "error: ROLLBACK FAILED; production is possibly down; inspect $ENV_BACKUP and $COMPOSE_BACKUP manually (log: $log)"
  cleanup
  exit 4
}

cleanup() {
  [ -z "$NEXT_ENV" ] || rm -f -- "$NEXT_ENV"
  [ -z "$NEXT_COMPOSE" ] || rm -f -- "$NEXT_COMPOSE"
  if [ -n "$LOCK_DIR" ]; then rmdir -- "$LOCK_DIR" 2>/dev/null || true; fi
}

on_exit() {
  local status=$?
  if [ "$SWAPPED" -eq 1 ] && [ "$FINISHED" -eq 0 ]; then
    rollback "unexpected exit status $status"
  fi
  cleanup
}
trap on_exit EXIT
# Before the swap a signal just exits (nothing changed); after it, on_exit
# restores. HUP is a lost SSH session, INT/TERM a cancelled workflow.
trap 'exit 130' INT TERM HUP

if [ "$VALIDATE_ONLY" -eq 1 ]; then
  NEXT_ENV="$DEPLOY_DIR/.env.validate.$$"
  render_env "$ENV_FILE" "$NEXT_ENV"
  compose_with "$NEXT_ENV" "$CANDIDATE_COMPOSE" config --quiet
  echo "valid: $IMAGE with catalog $CATALOG_SHA256, $CANDIDATE_COMPOSE and $ENV_FILE"
  exit 0
fi

# Serialize deploys on this host. flock when available, else an atomic mkdir.
if command -v flock >/dev/null 2>&1; then
  exec 9>"$DEPLOY_DIR/.deploy-release.lock"
  flock -n 9 || { echo "error: another deploy-release.sh holds the deploy lock" >&2; exit 1; }
else
  LOCK_DIR="$DEPLOY_DIR/.deploy-release.lock.d"
  mkdir -- "$LOCK_DIR" 2>/dev/null || { LOCK_DIR=""; echo "error: another deploy-release.sh holds the deploy lock" >&2; exit 1; }
fi

# Authenticate only when read-only credentials are provided; the token is
# piped via stdin so it never appears in logs or process lists.
if [ -n "${GHCR_USER:-}" ] && [ -n "${GHCR_TOKEN:-}" ]; then
  printf '%s' "$GHCR_TOKEN" | docker login ghcr.io -u "$GHCR_USER" --password-stdin
fi

# Pull before touching any file: a missing or unauthorized image leaves the
# running configuration exactly as it was.
if ! docker pull "$IMAGE"; then
  echo "error: could not pull $IMAGE; nothing was changed" >&2
  exit 1
fi

NEXT_ENV="$DEPLOY_DIR/.env.next"
render_env "$ENV_FILE" "$NEXT_ENV"
if ! compose_with "$NEXT_ENV" "$CANDIDATE_COMPOSE" config --quiet; then
  echo "error: candidate .env/compose configuration is invalid; nothing was changed" >&2
  exit 1
fi
if [ -n "$COMPOSE_FILE" ]; then
  NEXT_COMPOSE="$DEPLOY_DIR/.compose.yaml.next.$$"
  install -m 0644 -- "$COMPOSE_FILE" "$NEXT_COMPOSE"
fi

ENV_MODE="$(file_mode "$ENV_FILE" 600)"
COMPOSE_MODE="$(file_mode "$ACTIVE_COMPOSE" 644)"
ENV_BACKUP="$ENV_FILE.bak.$STAMP"
COMPOSE_BACKUP="$ACTIVE_COMPOSE.bak.$STAMP"
( umask 077 && cp -- "$ENV_FILE" "$ENV_BACKUP" && cp -- "$ACTIVE_COMPOSE" "$COMPOSE_BACKUP" )
chmod 0600 -- "$ENV_BACKUP" "$COMPOSE_BACKUP"

SWAPPED=1
mv -f -- "$NEXT_ENV" "$ENV_FILE"
NEXT_ENV=""
if [ -n "$NEXT_COMPOSE" ]; then
  mv -f -- "$NEXT_COMPOSE" "$ACTIVE_COMPOSE"
  NEXT_COMPOSE=""
fi

# No --no-deps: the declared healthy-postgres dependency is enforced.
if ! compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" up -d --no-build server; then
  rollback "compose up failed"
fi
if ! wait_healthy; then
  rollback "server not ready within ${HEALTH_TIMEOUT}s"
fi

FINISHED=1
prune_backups || true
echo "server $IMAGE is healthy (/health/ready 200) with catalog $CATALOG_SHA256 (backups $ENV_BACKUP, $COMPOSE_BACKUP)" || true
compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" ps server || true
