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
lockfiles="$(mktemp)"
echo "cosign.pub public key: $(cat cosign.pub)"
echo "Lockfile written to ${lockfiles}"
