#!/usr/bin/env bash
# Détermine si le build du frontend versionné dans web/dist/ correspond
# encore aux sources de web/frontend/.
#
# web/dist/ est suivi par git pour que `go build` fonctionne sans Node :
# c'est ce qui permet de compiler silo sur le NAS, qui n'a que Go. En
# contrepartie, ce build versionné peut prendre du retard sur les sources,
# et l'embarquer tel quel produirait un binaire à l'interface périmée,
# sans le moindre avertissement.
#
# Usage : frontend-status.sh [racine-du-depot]
# Codes de sortie :
#   0  build à jour
#   1  build périmé : les sources ont changé depuis
#   2  indéterminable (hors dépôt git, ou historique absent)
set -euo pipefail

repo_root="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"

# Fichiers dont une modification impose de régénérer web/dist/.
sources_frontend=(
  "web/frontend/src"
  "web/frontend/index.html"
  "web/frontend/package.json"
  "web/frontend/package-lock.json"
  "web/frontend/vite.config.ts"
)

cd "$repo_root"

if ! git rev-parse --git-dir >/dev/null 2>&1; then
  exit 2
fi

# Une modification non commitée des sources suffit à périmer le build,
# quelle que soit l'histoire des commits.
if [ -n "$(git status --porcelain -- "${sources_frontend[@]}" 2>/dev/null)" ]; then
  exit 1
fi

horodatage_sources="$(git log -1 --format=%ct -- "${sources_frontend[@]}" 2>/dev/null || true)"
horodatage_dist="$(git log -1 --format=%ct -- web/dist 2>/dev/null || true)"

if [ -z "$horodatage_sources" ] || [ -z "$horodatage_dist" ]; then
  exit 2
fi

if [ "$horodatage_sources" -gt "$horodatage_dist" ]; then
  exit 1
fi
exit 0
