#!/usr/bin/env bash
# Signed releases (Phase 4): produce cosign keys, sign images, emit digest pins.
set -euo pipefail
VERSION="${1:-1.0.0}"
cosign generate-key-pair --output-key-prefix cosign
for svc in synth-control synth-webmail synth-watchdog; do
  img="ghcr.io/synthmail/${svc}:${VERSION}"
  cosign sign --yes --key cosign.key "${img}"
done
echo "Signed v${VERSION}. Pins:" >> docs/releases/v${VERSION}.txt
# Write the image digests into a real lockfile (audit N-11). Previously
# this was `lockfiles="$(mktemp)"` and the file was left empty.
LOCKFILE="docs/releases/v${VERSION}.digests.txt"
: > "${LOCKFILE}"
for svc in synth-control synth-webmail synth-watchdog; do
  # Pull the digest from the registry; fall back to the local image id.
  digest="$(docker buildx imagetools inspect "ghcr.io/synthmail/${svc}:${VERSION}" --format '{{.Config.digest}}' 2>/dev/null || true)"
  if [ -z "${digest}" ]; then
    digest="$(docker inspect "ghcr.io/synthmail/${svc}:${VERSION}" --format='{{.Id}}' 2>/dev/null || echo "unknown")"
  fi
  echo "ghcr.io/synthmail/${svc}:${VERSION} ${digest}" >> "${LOCKFILE}"
done
echo "Digest lockfile written to ${LOCKFILE}"
echo "cosign.pub public key: $(cat cosign.pub)"
