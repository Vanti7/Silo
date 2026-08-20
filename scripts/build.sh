#!/usr/bin/env bash
# Compile le frontend puis le binaire silo pour Debian/amd64.
# Usage: scripts/build.sh [GOOS] [GOARCH]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
goos="${1:-linux}"
goarch="${2:-amd64}"

check_build_tools() {
  # Ce script compile silo : il doit tourner sur une machine de build
  # (poste de dev, CI...), pas sur le NAS cible, qui n'a besoin que du
  # binaire final (voir README.md, section Prérequis).
  for tool in go npm; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      echo "erreur : '$tool' introuvable." >&2
      echo "Ce script doit être exécuté sur une machine de build (Go + Node/npm)," >&2
      echo "pas sur le NAS cible. Voir README.md, section Prérequis / Déploiement." >&2
      exit 1
    fi
  done
}

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

check_build_tools
build_frontend
build_backend

echo "== terminé : $repo_root/bin/silo =="
