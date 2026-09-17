#!/usr/bin/env bash
# Focused regression tests for the VPS release scripts.
#
# Covers the adversarial-review findings: strict commit matching, idempotent
# re-promotion, staging/release conflict detection, and immutable image
# references. Runs entirely in a temporary HOENN_ROOT; never touches
# /srv/hoenn, docker daemons, or the network (deploy validation uses
# --validate-only, which stops before login/pull/up).
#
# Usage: bash deploy/coop/test-release-scripts.sh
set -u
cd -- "$(dirname -- "$0")"

PROMOTE=./promote-release.sh
DEPLOY=./deploy-release.sh
PASS=0
FAIL=0

report() { # $1=status $2=name [$3=detail]
  if [ "$1" = 0 ]; then PASS=$((PASS + 1)); printf 'ok   %s\n' "$2";
  else FAIL=$((FAIL + 1)); printf 'FAIL %s\n' "$2"; [ $# -lt 3 ] || printf '     %s\n' "$3"; fi
}

# Build a staging release dir with controllable content.
# make_staging <root> <id> <commit-value> [tamper-manifest]
make_staging() {
  mkdir -p "$1/staging/$2"
  (
    cd -- "$1/staging/$2"
    head -c 1024 /dev/urandom > game.gba
    head -c 512 /dev/urandom > coop-sidecar.exe
    rom_sha="$(sha256sum game.gba | awk '{print $1}')"
    if [ "${4:-}" = tamper ]; then rom_sha="$(printf '0%.0s' $(seq 64))"; fi
    python3 -c 'import json,sys; json.dump({"game_build": {"rom_sha256": sys.argv[1]}}, open("bridge_manifest.json", "w"))' "$rom_sha"
    python3 -c 'import json,sys; json.dump({"commit": sys.argv[1]}, open("release.json", "w"))' "$3"
    sha256sum game.gba coop-sidecar.exe bridge_manifest.json release.json > SHA256SUMS
  )
}

ROOT="$(mktemp -d /tmp/hoenn-release-test.XXXXXX)" || exit 1
[ -n "$ROOT" ] && [ -d "$ROOT" ] || exit 1
# Promoted releases are made read-only; restore writability for cleanup.
trap 'chmod -R u+w -- "$ROOT" 2>/dev/null; rm -rf -- "$ROOT"' EXIT
export HOENN_ROOT="$ROOT"

FULL_A=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
FULL_B=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb

# 1. Fresh promotion succeeds and switches current.
make_staging "$ROOT" "$FULL_A" "$FULL_A"
out="$($PROMOTE "$FULL_A" 2>&1)" && [ "$(readlink "$ROOT/current")" = "releases/$FULL_A" ]
report $? "fresh promotion switches current" "$out"

# 2. Re-promoting the same release is a successful no-op (retry-safe).
out="$($PROMOTE "$FULL_A" 2>&1)"
report $? "re-promotion of active release succeeds" "$out"

# 3. Tampered ROM (manifest mismatch) is rejected; current unchanged.
make_staging "$ROOT" "$FULL_B" "$FULL_B" tamper
out="$($PROMOTE "$FULL_B" 2>&1)"; status=$?
[ $status -ne 0 ] && [ "$(readlink "$ROOT/current")" = "releases/$FULL_A" ] && ! printf '%s' "$out" | grep -q 'promoted '
report $? "tampered ROM rejected, current untouched" "$out"
rm -rf -- "$ROOT/staging/$FULL_B"

# 4. Missing required file is rejected.
make_staging "$ROOT" "$FULL_B" "$FULL_B"
rm -- "$ROOT/staging/$FULL_B/coop-sidecar.exe"
out="$($PROMOTE "$FULL_B" 2>&1)"; status=$?
[ $status -ne 0 ]
report $? "missing file rejected" "$out"
rm -rf -- "$ROOT/staging/$FULL_B"

# 5. Commit merely containing the id as a substring is rejected.
SHORT=deadbee
make_staging "$ROOT" "$SHORT" "1111${SHORT}2222"
out="$($PROMOTE "$SHORT" 2>&1)"; status=$?
[ $status -ne 0 ]
report $? "substring commit rejected" "$out"
rm -rf -- "$ROOT/staging/$SHORT"

# 6. Abbreviated prefix of a full SHA commit is accepted.
make_staging "$ROOT" "$FULL_B" "${FULL_B:0:12}"
out="$($PROMOTE "$FULL_B" 2>&1)"; status=$?
[ $status -eq 0 ] && [ "$(readlink "$ROOT/current")" = "releases/$FULL_B" ]
report $? "abbreviated prefix commit accepted" "$out"

# 7. Conflicting staging content under an already-promoted id is rejected.
# Fresh random bytes guarantee the re-upload differs while verifying alone.
make_staging "$ROOT" "$FULL_B" "$FULL_B"
out="$($PROMOTE "$FULL_B" 2>&1)"; status=$?
[ $status -ne 0 ] && printf '%s' "$out" | grep -q 'differ'
report $? "conflicting re-upload rejected" "$out"
rm -rf -- "$ROOT/staging/$FULL_B"

# --- deploy-release.sh image reference validation (hermetic) ---
DEPLOY_DIR="$ROOT/deploy"
mkdir -p "$DEPLOY_DIR"
printf 'COOP_IMAGE=placeholder\n' > "$DEPLOY_DIR/.env"
printf 'services:\n  server:\n    image: placeholder\n' > "$DEPLOY_DIR/compose.yaml"

DIGEST_REF="ghcr.io/example/hoenn-sessions-server@sha256:$(printf 'a%.0s' $(seq 64))"
SHA_REF="ghcr.io/example/hoenn-sessions-server:$FULL_A"

check_validate() { # $1=expected-status $2=name $3=image [$4=required message fragment]
  image="$3"
  out="$($DEPLOY --image "$image" --deploy-dir "$DEPLOY_DIR" --validate-only 2>&1)"; status=$?
  if [ "$status" -eq "$1" ] && { [ $# -lt 4 ] || printf '%s' "$out" | grep -q "$4"; }; then
    report 0 "$2 (exit $status, want $1)"
  else
    report 1 "$2 (exit $status, want $1)" "$out"
  fi
}

check_validate 0 "digest-pinned image accepted" "$DIGEST_REF" "valid:"
# Tags are mutable (a re-run overwrites them), so even full-SHA tags refuse.
check_validate 2 "full-SHA tag rejected" "$SHA_REF" "digest-pinned"
check_validate 2 "mutable :latest rejected" "ghcr.io/example/hoenn-sessions-server:latest" "mutable :latest"
check_validate 2 "registry port without tag rejected" "registry.example:5000/hoenn-server" "digest-pinned"
check_validate 2 "mutable named tag rejected" "ghcr.io/example/hoenn-sessions-server:stable" "digest-pinned"
check_validate 2 "missing tag rejected" "ghcr.io/example/hoenn-sessions-server" "digest-pinned"
check_validate 2 "short hex tag rejected" "ghcr.io/example/hoenn-sessions-server:abc123" "digest-pinned"

# 16. Re-uploading a byte-identical staging copy for the active release
# (FULL_B) is a successful no-op that cleans up staging (retry path).
cp -r -- "$ROOT/releases/$FULL_B" "$ROOT/staging/$FULL_B"
chmod -R u+w -- "$ROOT/staging/$FULL_B"
out="$($PROMOTE "$FULL_B" 2>&1)"; status=$?
[ $status -eq 0 ] && [ ! -e "$ROOT/staging/$FULL_B" ] && \
  [ "$(readlink "$ROOT/current")" = "releases/$FULL_B" ]
report $? "identical re-upload is a clean no-op" "$out"

# 17. Re-promoting an older release flips current back and records the
# previous one (rollback / recovery path).
out="$($PROMOTE "$FULL_A" 2>&1)"; status=$?
[ $status -eq 0 ] && [ "$(readlink "$ROOT/current")" = "releases/$FULL_A" ] && \
  [ "$(cat "$ROOT/.previous-release")" = "$FULL_B" ]
report $? "re-promote flips current and records previous" "$out"

# 18. The workflow must feed the GHCR token over stdin and export it as a
# remote-shell statement: an env-prefix (`VAR=x cmd1 && cmd2`) would leave
# the deploy step without the token, and any command-line interpolation
# would leak it to /proc.
WORKFLOW=../../.github/workflows/deploy.yml
if grep -q 'GHCR_TOKEN=\\"\\\$(cat)\\"' "$WORKFLOW" && \
   grep -q 'export GHCR_USER GHCR_TOKEN;' "$WORKFLOW"; then
  report 0 "workflow token transport uses stdin + export"
else
  report 1 "workflow token transport uses stdin + export" "deploy.yml lost the stdin/export shape"
fi

# 19. Token propagation, behaviorally: the exact remote-invocation shape
# used in deploy.yml ("...; export ...;" over stdin, NOT an env-prefix)
# must deliver the token to every command across `&&`, never via argv.
# (Shape mirror of the "Promote release and roll the server" step.)
ssh() {
  printf '%s' "$*" > "$ROOT/ssh-argv"
  bash -c "$2"
}
out="$(printf '%s' "tok-123" | ssh hoenn-vps "GHCR_USER='deploy-user'; GHCR_TOKEN=\"\$(cat)\"; export GHCR_USER GHCR_TOKEN; echo \"first:[\$GHCR_USER][\$GHCR_TOKEN]\" && bash -c 'echo \"second:[\$GHCR_USER][\$GHCR_TOKEN]\"'")"
unset -f ssh
if printf '%s' "$out" | grep -q 'first:\[deploy-user\]\[tok-123\]' && \
   printf '%s' "$out" | grep -q 'second:\[deploy-user\]\[tok-123\]' && \
   ! grep -q 'tok-123' "$ROOT/ssh-argv"; then
  report 0 "token reaches all remote commands via stdin, absent from argv"
else
  report 1 "token reaches all remote commands via stdin, absent from argv" "$out"
fi

# 20. SSH connection values must be grammar-validated before deploy.yml
# writes them into ~/.ssh/config (newline injection → ProxyCommand).
if grep -q 'VPS_HOST must be a hostname or IP address' "$WORKFLOW" && \
   grep -q 'VPS_PORT must be empty or numeric' "$WORKFLOW"; then
  report 0 "workflow validates SSH config values"
else
  report 1 "workflow validates SSH config values" "deploy.yml lost the ssh-config validation"
fi

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
