#!/usr/bin/env bash
# Compile le frontend puis le binaire silo, déposé dans bin/silo.
#
# web/dist/ étant versionné, cargo suffit à produire un binaire complet : le
# frontend n'est régénéré que si npm est disponible, ou si les sources ont
# changé depuis le dernier build versionné (auquel cas npm devient
# indispensable, sous peine d'embarquer une interface périmée).
#
# Usage : scripts/build.sh [OS] [ARCH] [--backend-only]
#         (défaut : linux amd64 ; ARCH parmi amd64, arm64, arm)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
statut_frontend="$repo_root/scripts/lib/frontend-status.sh"

os="linux"
arch="amd64"
backend_only="non"
positionnels=()

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --backend-only) backend_only="oui" ;;
      -h|--help)
        sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
      -*) echo "erreur : option inconnue $1" >&2; exit 1 ;;
      *) positionnels+=("$1") ;;
    esac
    shift
  done

  [ "${#positionnels[@]}" -ge 1 ] && os="${positionnels[0]}"
  [ "${#positionnels[@]}" -ge 2 ] && arch="${positionnels[1]}"
  [ "${#positionnels[@]}" -le 2 ] || { echo "erreur : trop de paramètres" >&2; exit 1; }
}

# cible_rust traduit le couple OS/architecture (vocabulaire historique de
# ce script, gardé pour ne pas changer son interface) en triplet Rust.
cible_rust() {
  [ "$os" = "linux" ] || { echo "erreur : OS non pris en charge : $os (linux uniquement)" >&2; exit 1; }
  case "$arch" in
    amd64) echo "x86_64-unknown-linux-gnu" ;;
    arm64) echo "aarch64-unknown-linux-gnu" ;;
    arm) echo "armv7-unknown-linux-gnueabihf" ;;
    *) echo "erreur : architecture non prise en charge : $arch (amd64, arm64, arm)" >&2; exit 1 ;;
  esac
}

exige_cargo() {
  command -v cargo >/dev/null 2>&1 || {
    echo "erreur : 'cargo' introuvable." >&2
    echo "Installer Rust ≥ 1.85 et un compilateur C (SQLite est compilé avec silo)." >&2
    echo "Debian 13 : apt install cargo build-essential" >&2
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

# build_backend compile pour la cible demandée. Vers une autre
# architecture que celle de la machine, il faut un éditeur de liens pour
# la cible : cargo-zigbuild (qui s'appuie sur zig) est utilisé s'il est
# installé, c'est le moyen le plus simple de compiler depuis Windows ou
# macOS pour le NAS.
build_backend() {
  local cible hote commande
  cible="$(cible_rust)"
  hote="$(rustc -vV | sed -n 's/^host: //p')"

  if [ "$cible" = "$hote" ]; then
    commande=(cargo build)
  elif command -v cargo-zigbuild >/dev/null 2>&1; then
    commande=(cargo zigbuild)
  else
    commande=(cargo build)
    echo "note : compilation croisée $hote -> $cible sans cargo-zigbuild." >&2
    echo "En cas d'échec à l'édition des liens : cargo install cargo-zigbuild (et zig)," >&2
    echo "ou compile directement sur le NAS : scripts/deploy.sh --local --build" >&2
  fi
  rustup target add "$cible" >/dev/null 2>&1 || true

  echo "== build silo ($cible) =="
  (cd "$repo_root" && "${commande[@]}" --release --locked --target "$cible")

  local dossier_cible="${CARGO_TARGET_DIR:-$repo_root/target}"
  mkdir -p "$repo_root/bin"
  cp "$dossier_cible/$cible/release/silo" "$repo_root/bin/silo"
}

main() {
  parse_args "$@"
  exige_cargo

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
