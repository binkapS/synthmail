#!/usr/bin/env bash
# Build (and optionally push) SynthMail container images for distribution.
# Usage:
#   ./scripts/build-images.sh [--push] [--platform linux/amd64,linux/arm64]
#
# Environment:
#   REGISTRY   Image registry prefix (default: ghcr.io/synthmail)
#   TAG        Image tag (default: latest)
#   PLATFORMS  Target platforms (default: linux/amd64,linux/arm64)
set -euo pipefail

cd "$(dirname "$0")/.."

PUSH=0
PLATFORMS="${PLATFORMS:-linux/amd64,linux/arm64}"
while [ $# -gt 0 ]; do
  case "$1" in
    --push) PUSH=1; shift ;;
    --platform) PLATFORMS="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done
REGISTRY="${REGISTRY:-ghcr.io/synthmail}"
TAG="${TAG:-latest}"

# service -> "dockerfile context"
declare -A DOCKERFILES=(
  [synth-control]="docker/synth-control/Dockerfile:."
  [synth-webmail]="docker/synth-webmail/Dockerfile:."
  [synth-watchdog]="docker/synth-watchdog/Dockerfile:."
  [synth-ui]="docker/synth-ui/Dockerfile:."
  [backup]="docker/backup/Dockerfile:./docker/backup"
)

echo "==> Building SynthMail images"
echo "    registry:  $REGISTRY"
echo "    tag:       $TAG"
echo "    platforms: $PLATFORMS"
echo "    push:      $PUSH"

for svc in "${!DOCKERFILES[@]}"; do
  entry="${DOCKERFILES[$svc]}"
  df="${entry%%:*}"
  ctx="${entry#*:}"
  img="${REGISTRY}/${svc}:${TAG}"
  echo ""
  echo "---- $svc -> $img"
  if [ "$PUSH" = 1 ]; then
    docker buildx build \
      --platform "$PLATFORMS" \
      --file "$df" \
      --tag "$img" \
      --push \
      "$ctx"
  else
    # Single-platform local build (multi-arch cannot be loaded without a registry).
    if [[ "$PLATFORMS" == *","* ]]; then
      echo "    (multi-arch requested; building linux/amd64 only for local load)"
      PLATFORMS_LOCAL="linux/amd64"
    else
      PLATFORMS_LOCAL="$PLATFORMS"
    fi
    docker buildx build \
      --platform "$PLATFORMS_LOCAL" \
      --file "$df" \
      --tag "$img" \
      --load \
      "$ctx"
  fi
done

echo ""
echo "==> Images ready"
if [ "$PUSH" = 1 ]; then
  for svc in "${!DOCKERFILES[@]}"; do
    echo "    docker pull ${REGISTRY}/${svc}:${TAG}"
  done
else
  echo "    Built locally. Re-run with --push to publish."
  echo "    Tip: docker compose -f compose.yaml up -d"
fi
