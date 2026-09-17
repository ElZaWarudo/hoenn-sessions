#!/usr/bin/env bash
# Roll the coop-server container to an immutable image and verify health.
#
# The image must be referenced digest-pinned (name@sha256:<64 hex>, as
# recorded in release.json by CI). Tags are never accepted, not even full
# commit SHAs: GHCR tags are mutable and a pipeline re-run overwrites them,
# so a tag cannot prove which build it points at. This script never builds
# anything: the image must already exist in GHCR. It preserves databases,
# secrets, volumes and Caddy configuration; it only recreates the `server`
# service. Never runs `docker compose down -v`.
#
# Usage: deploy-release.sh --image <immutable-ref> [--deploy-dir DIR]
#        [--env-file FILE] [--validate-only]
#
#   --image       e.g. ghcr.io/<owner>/hoenn-sessions-server@sha256:<digest>
#   --deploy-dir  directory containing compose.yaml (default: $HOENN_DEPLOY_DIR
#                 or the directory this script lives in)
#   --env-file    env file holding COOP_IMAGE (default: <deploy-dir>/.env);
#                 passed to every compose invocation, so a custom path is
#                 honoured instead of silently ignored
#   --validate-only  check arguments, image reference, compose file and env
#                 file, then exit 0 before any login, pull, or restart
#
# Environment:
#   HOENN_DEPLOY_DIR   default deploy directory
#   HEALTH_TIMEOUT     seconds to wait for /health/ready (default 180)
#   GHCR_USER/GHCR_TOKEN  optional read-only GHCR credentials for `pull`;
#                 the token must arrive via environment (piped stdin in CI),
#                 never on a command line
set -euo pipefail

IMAGE=""
DEPLOY_DIR="${HOENN_DEPLOY_DIR:-$(dirname -- "$0")}"
ENV_FILE=""
VALIDATE_ONLY=0
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-180}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --image) IMAGE="${2:?--image needs a value}"; shift 2 ;;
    --deploy-dir) DEPLOY_DIR="${2:?--deploy-dir needs a value}"; shift 2 ;;
    --env-file) ENV_FILE="${2:?--env-file needs a value}"; shift 2 ;;
    --validate-only) VALIDATE_ONLY=1; shift ;;
    -h | --help) sed -n '1,24p' -- "$0"; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done

# Reject anything that is not an immutable reference. The charset gate runs
# first so later parameter expansions and the sed replacement cannot be
# abused by shell- or sed-active characters.
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

if [ -z "$ENV_FILE" ]; then
  ENV_FILE="$DEPLOY_DIR/.env"
fi
if [ ! -f "$DEPLOY_DIR/compose.yaml" ]; then
  echo "error: compose.yaml not found in $DEPLOY_DIR" >&2
  exit 1
fi
if [ ! -f "$ENV_FILE" ]; then
  echo "error: env file not found: $ENV_FILE (copy .env.example first)" >&2
  exit 1
fi

# Route every compose invocation through the selected env file so --env-file
# is honoured instead of silently falling back to the project default.
compose() {
  docker compose --project-directory "$DEPLOY_DIR" --env-file "$ENV_FILE" "$@"
}

if [ "$VALIDATE_ONLY" -eq 1 ]; then
  compose config --quiet
  echo "valid: $IMAGE with $DEPLOY_DIR/compose.yaml and $ENV_FILE"
  exit 0
fi

# Pin COOP_IMAGE to the new immutable reference, keeping a backup.
cp -p -- "$ENV_FILE" "$ENV_FILE.bak"
if grep -q '^COOP_IMAGE=' -- "$ENV_FILE"; then
  sed -i "s#^COOP_IMAGE=.*#COOP_IMAGE=$IMAGE#" -- "$ENV_FILE"
else
  printf 'COOP_IMAGE=%s\n' "$IMAGE" >> "$ENV_FILE"
fi
compose config --quiet

# Authenticate to GHCR only when read-only credentials are provided; the
# token is piped via stdin so it never appears in logs or process lists.
if [ -n "${GHCR_USER:-}" ] && [ -n "${GHCR_TOKEN:-}" ]; then
  printf '%s' "$GHCR_TOKEN" | docker login ghcr.io -u "$GHCR_USER" --password-stdin
fi

compose pull server
# No --no-deps: the declared healthy-postgres dependency is enforced, so a
# rollout also starts a stopped database instead of timing out against it.
compose up -d --no-build server

# Bounded readiness gate against the existing endpoint. Fails the deploy.
deadline=$((SECONDS + HEALTH_TIMEOUT))
until compose exec -T server \
    curl --fail --silent --max-time 4 http://127.0.0.1:3000/health/ready >/dev/null; do
  if [ "$SECONDS" -ge "$deadline" ]; then
    echo "error: server $IMAGE not ready within ${HEALTH_TIMEOUT}s; previous image retained locally for rollback" >&2
    compose ps server || true
    compose logs --tail=50 server || true
    exit 1
  fi
  sleep 5
done

echo "server $IMAGE is healthy (/health/ready 200)"
compose ps server
