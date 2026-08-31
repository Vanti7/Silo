#!/usr/bin/env bash
# Compile le frontend puis le binaire silo.
#
# web/dist/ étant versionné, Go suffit à produire un binaire complet : le
# frontend n'est régénéré que si npm est disponible, ou si les sources ont
# changé depuis le dernier build versionné (auquel cas npm devient
# indispensable, sous peine d'embarquer une interface périmée).
#
# Usage : scripts/build.sh [GOOS] [GOARCH] [--backend-only]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
statut_frontend="$repo_root/scripts/lib/frontend-status.sh"

goos="linux"
goarch="amd64"
backend_only="non"
positionnels=()

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --backend-only) backend_only="oui" ;;
      -h|--help)
        sed -n '2,9p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
      -*) echo "erreur : option inconnue $1" >&2; exit 1 ;;
      *) positionnels+=("$1") ;;
    esac
    shift
  done

  [ "${#positionnels[@]}" -ge 1 ] && goos="${positionnels[0]}"
  [ "${#positionnels[@]}" -ge 2 ] && goarch="${positionnels[1]}"
  [ "${#positionnels[@]}" -le 2 ] || { echo "erreur : trop de paramètres" >&2; exit 1; }
}

exige_go() {
  command -v go >/dev/null 2>&1 || {
    echo "erreur : 'go' introuvable." >&2
    echo "Installer Go ≥ 1.24 (Debian 13 : apt install golang-go)." >&2
    exit 1
  }
}

# decide_frontend renseigne frontend_a_regenerer, et interrompt le script
# si régénérer est indispensable mais impossible. Le résultat passe par
# une variable plutôt que par la sortie standard : dans une substitution
# de commande, un `exit` ne quitterait que le sous-shell et le build se
# poursuivrait malgré l'erreur, en embarquant un frontend périmé.
frontend_a_regenerer=""

decide_frontend() {
  if [ "$backend_only" = "oui" ]; then
    frontend_a_regenerer="non"
    return
  fi

  local etat=0
  bash "$statut_frontend" "$repo_root" || etat=$?

  if command -v npm >/dev/null 2>&1; then
    frontend_a_regenerer="oui"
    return
  fi

  # npm absent : on ne peut réutiliser web/dist/ que s'il est à jour.
  if [ "$etat" -eq 1 ]; then
    echo "erreur : les sources du frontend ont changé depuis le build versionné" >&2
    echo "dans web/dist/, et npm est absent pour le régénérer." >&2
    echo >&2
    echo "Compile le frontend sur une machine disposant de Node/npm et commite" >&2
    echo "web/dist/, ou installe npm ici, ou force le build backend seul :" >&2
    echo "  scripts/build.sh --backend-only   # embarque le frontend versionné tel quel" >&2
    exit 1
  fi

  if [ ! -f "$repo_root/web/dist/index.html" ]; then
    echo "erreur : ni npm ni build frontend versionné dans web/dist/." >&2
    exit 1
  fi

  if [ "$etat" -eq 2 ]; then
    echo "note : impossible de vérifier la fraîcheur de web/dist/ (hors dépôt git)." >&2
  fi
  frontend_a_regenerer="non"
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

main() {
  parse_args "$@"
  exige_go

  decide_frontend
  if [ "$frontend_a_regenerer" = "oui" ]; then
    build_frontend
  else
    echo "== frontend : réutilisation du build versionné dans web/dist/ =="
  fi

  build_backend
  echo "== terminé : $repo_root/bin/silo =="
}

main "$@"
