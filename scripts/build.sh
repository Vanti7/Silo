#!/usr/bin/env bash
# Compile le frontend puis le binaire silo pour Debian/amd64.
# Usage: scripts/build.sh [GOOS] [GOARCH]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
goos="${1:-linux}"
goarch="${2:-amd64}"

build_frontend() {
  echo "== build frontend =="
  (cd "$repo_root/web/frontend" && npm ci && npm run build)
}

build_backend() {
  echo "== build silo (${goos}/${goarch}) =="
  (
    cd "$repo_root"
    CGO_ENABLED=0 GOOS="$goos" GOARCH="$goarch" go build \
      -trimpath -ldflags="-s -w" \
      -o "bin/silo" \
      ./cmd/silo
  )
}

build_frontend
build_backend

echo "== terminé : $repo_root/bin/silo =="
