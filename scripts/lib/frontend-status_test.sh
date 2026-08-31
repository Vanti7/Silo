#!/usr/bin/env bash
# Tests de scripts/lib/frontend-status.sh, sur des dépôts git synthétiques.
# L'enjeu : ne jamais laisser compiler un binaire embarquant un frontend
# périmé sans le signaler, tout en autorisant le build sans npm quand
# web/dist/ est à jour.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sous_test="$script_dir/frontend-status.sh"

echecs=0

# depot_synthetique crée un dépôt minimal reproduisant l'arborescence
# utile : sources du frontend et build versionné.
depot_synthetique() {
  depot="$(mktemp -d)"
  git -C "$depot" init -q
  git -C "$depot" config user.email "test@silo"
  git -C "$depot" config user.name "test"
  mkdir -p "$depot/web/frontend/src" "$depot/web/dist"
}

commiter() {
  local message="$1"
  git -C "$depot" add -A
  # Les horodatages de commit sont espacés explicitement : sans cela, deux
  # commits successifs partagent la même seconde et l'ordre devient
  # indécidable.
  GIT_AUTHOR_DATE="$horodatage" GIT_COMMITTER_DATE="$horodatage" \
    git -C "$depot" commit -q -m "$message"
  horodatage=$((horodatage + 60))
}

nettoyer() {
  [ -n "${depot:-}" ] && rm -rf "$depot"
}

verifier_code() {
  local description="$1" attendu="$2"
  local obtenu=0
  bash "$sous_test" "$depot" || obtenu=$?
  if [ "$obtenu" = "$attendu" ]; then
    echo "  OK    $description"
  else
    echo "  ECHEC $description : code $obtenu, attendu $attendu"
    echecs=$((echecs + 1))
  fi
}

horodatage=1700000000

# --- dist construit après les sources : à jour -----------------------------
test_a_jour() {
  echo "cas : web/dist commité après les sources"
  depot_synthetique
  echo "source" > "$depot/web/frontend/src/App.tsx"
  commiter "sources frontend"
  echo "build" > "$depot/web/dist/index.html"
  commiter "build frontend"

  verifier_code "build à jour -> 0" 0
  nettoyer
}

# --- sources modifiées après le dernier build : périmé ---------------------
test_perime() {
  echo "cas : sources modifiées après le dernier build versionné"
  depot_synthetique
  echo "build" > "$depot/web/dist/index.html"
  commiter "build frontend"
  echo "source modifiee" > "$depot/web/frontend/src/App.tsx"
  commiter "modification des sources"

  verifier_code "build périmé -> 1" 1
  nettoyer
}

# --- modification non commitée : périmé ------------------------------------
test_modification_non_commitee() {
  echo "cas : modification des sources non commitée"
  depot_synthetique
  echo "source" > "$depot/web/frontend/src/App.tsx"
  commiter "sources frontend"
  echo "build" > "$depot/web/dist/index.html"
  commiter "build frontend"
  echo "modification locale" >> "$depot/web/frontend/src/App.tsx"

  verifier_code "modification en cours -> 1" 1
  nettoyer
}

# --- fichier de configuration du frontend modifié --------------------------
test_config_frontend() {
  echo "cas : vite.config.ts modifié après le build"
  depot_synthetique
  echo "build" > "$depot/web/dist/index.html"
  commiter "build frontend"
  echo "config" > "$depot/web/frontend/vite.config.ts"
  commiter "modification de la configuration vite"

  verifier_code "configuration modifiée -> 1" 1
  nettoyer
}

# --- modification hors frontend : sans effet -------------------------------
test_modification_backend() {
  echo "cas : modification du backend seul"
  depot_synthetique
  echo "source" > "$depot/web/frontend/src/App.tsx"
  commiter "sources frontend"
  echo "build" > "$depot/web/dist/index.html"
  commiter "build frontend"
  mkdir -p "$depot/internal/api"
  echo "package api" > "$depot/internal/api/router.go"
  commiter "modification du backend"

  verifier_code "backend modifié -> 0" 0
  nettoyer
}

# --- hors dépôt git : indéterminable ---------------------------------------
test_hors_depot() {
  echo "cas : arborescence hors dépôt git"
  depot="$(mktemp -d)"
  mkdir -p "$depot/web/dist"

  verifier_code "indéterminable -> 2" 2
  nettoyer
}

test_a_jour
test_perime
test_modification_non_commitee
test_config_frontend
test_modification_backend
test_hors_depot

echo
if [ "$echecs" -eq 0 ]; then
  echo "tous les cas passent"
else
  echo "$echecs cas en échec"
  exit 1
fi
