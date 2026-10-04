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
# compose file -> timestamped backups of .env and compose.yaml -> rename both
# into place -> `up -d` -> bounded /health/ready check. On a failed rollout,
# an unexpected error or a signal after the rename, the backups are restored
# byte-for-byte, the previous server is brought back up and health is checked
# again.
#
# Exit status: 0 healthy; 1 refused before any change; 2 usage;
#              3 rollout failed and the previous configuration is healthy again;
#              4 rollout failed and the rollback did not become healthy.
#
# Usage: deploy-release.sh --image <immutable-ref> --catalog-path PATH
#          --catalog-sha256 HEX [--deploy-dir DIR] [--env-file FILE]
#          [--compose-file FILE] [--validate-only]
#
#   --compose-file   version-controlled compose file to validate and install
#                    into <deploy-dir>/compose.yaml as part of the rollout
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
#   GHCR_USER/GHCR_TOKEN  optional read-only GHCR credentials; the token must
#                     arrive via environment (piped stdin in CI)
set -euo pipefail

IMAGE=""
CATALOG_PATH=""
CATALOG_SHA256=""
DEPLOY_DIR="${HOENN_DEPLOY_DIR:-$(dirname -- "$0")}"
ENV_FILE=""
COMPOSE_FILE=""
VALIDATE_ONLY=0
HOENN_ROOT="${HOENN_ROOT:-/srv/hoenn}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-180}"
HEALTH_INTERVAL="${HEALTH_INTERVAL:-5}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --image) IMAGE="${2:?--image needs a value}"; shift 2 ;;
    --catalog-path) CATALOG_PATH="${2:?--catalog-path needs a value}"; shift 2 ;;
    --catalog-sha256) CATALOG_SHA256="${2:?--catalog-sha256 needs a value}"; shift 2 ;;
    --deploy-dir) DEPLOY_DIR="${2:?--deploy-dir needs a value}"; shift 2 ;;
    --env-file) ENV_FILE="${2:?--env-file needs a value}"; shift 2 ;;
    --compose-file) COMPOSE_FILE="${2:?--compose-file needs a value}"; shift 2 ;;
    --validate-only) VALIDATE_ONLY=1; shift ;;
    -h | --help) sed -n '1,45p' -- "$0"; exit 0 ;;
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

restore_file() {
  local backup="$1" target="$2"
  cp -p -- "$backup" "$target.rollback.$$" && mv -f -- "$target.rollback.$$" "$target"
}

# Restores the pre-rollout files and server. Never returns.
rollback() {
  local reason="$1"
  trap - EXIT INT TERM HUP
  echo "error: rollout of $IMAGE failed ($reason); restoring previous configuration" >&2
  compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" ps server >&2 || true
  compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" logs --tail=50 server >&2 || true
  if restore_file "$ENV_BACKUP" "$ENV_FILE" && restore_file "$COMPOSE_BACKUP" "$ACTIVE_COMPOSE" \
      && compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" up -d --no-build server && wait_healthy; then
    echo "rolled back: previous server configuration is healthy again (backups $ENV_BACKUP, $COMPOSE_BACKUP)" >&2
    cleanup
    exit 3
  fi
  echo "error: ROLLBACK FAILED; inspect $ENV_BACKUP and $COMPOSE_BACKUP manually" >&2
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

STAMP="$(date -u +%Y%m%dT%H%M%SZ).$$"
ENV_BACKUP="$ENV_FILE.bak.$STAMP"
COMPOSE_BACKUP="$ACTIVE_COMPOSE.bak.$STAMP"
cp -p -- "$ENV_FILE" "$ENV_BACKUP"
cp -p -- "$ACTIVE_COMPOSE" "$COMPOSE_BACKUP"

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
echo "server $IMAGE is healthy (/health/ready 200) with catalog $CATALOG_SHA256 (backups $ENV_BACKUP, $COMPOSE_BACKUP)"
compose_with "$ENV_FILE" "$ACTIVE_COMPOSE" ps server || true
