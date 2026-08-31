#!/usr/bin/env bash
# Met à jour silo, dans l'un des deux modes suivants :
#
#   - distant (défaut) : depuis la machine de build, compile, transfère le
#     binaire vers le NAS par SSH et bascule ;
#   - local (--local) : directement sur le NAS, à partir d'un binaire déjà
#     compilé ailleurs et transféré (le NAS n'a ni Go ni Node/npm).
#
# Dans les deux cas, la bascule proprement dite est faite par
# scripts/lib/apply-update.sh, exécuté sur l'hôte cible : sauvegarde,
# redémarrage, vérification et retour arrière automatique si le service
# ne repart pas.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
apply_script="$repo_root/scripts/lib/apply-update.sh"

target=""
binaire_local=""
mode="distant"
with_unit="non"
skip_build="non"
goarch="amd64"

usage() {
  cat <<'USAGE'
Usage :
  scripts/deploy.sh <utilisateur@hote> [options]   depuis la machine de build
  scripts/deploy.sh --local [binaire] [options]    directement sur le NAS

Options :
  --local          bascule sur place, sans SSH ni compilation. Le binaire
                   doit avoir été compilé ailleurs puis transféré ; par
                   défaut bin/silo du dépôt, sinon le chemin donné.
  --with-unit      met aussi à jour /etc/systemd/system/silo.service
  --skip-build     (mode distant) réutilise bin/silo sans recompiler
  --arch <arch>    (mode distant) architecture cible (défaut : amd64)
  -h, --help       affiche cette aide

Exemples :
  scripts/deploy.sh root@nas.local          # tout-en-un depuis le poste
  scripts/deploy.sh --local                 # sur le NAS, depuis bin/silo
  scripts/deploy.sh --local /tmp/silo       # sur le NAS, binaire transféré
USAGE
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --local) mode="local" ;;
      --with-unit) with_unit="oui" ;;
      --skip-build) skip_build="oui" ;;
      --arch)
        [ $# -ge 2 ] || { echo "erreur : --arch attend une valeur" >&2; exit 1; }
        goarch="$2"
        shift
        ;;
      -h|--help) usage; exit 0 ;;
      -*) echo "erreur : option inconnue $1" >&2; usage >&2; exit 1 ;;
      *)
        # Le paramètre positionnel désigne la cible SSH en mode distant, et
        # le binaire à installer en mode local.
        if [ "$mode" = "local" ]; then
          [ -z "$binaire_local" ] || { echo "erreur : binaire déjà défini ($binaire_local)" >&2; exit 1; }
          binaire_local="$1"
        else
          [ -z "$target" ] || { echo "erreur : cible déjà définie ($target)" >&2; exit 1; }
          target="$1"
        fi
        ;;
    esac
    shift
  done

  # --local peut apparaître après le paramètre positionnel : on rattrape le
  # cas où celui-ci a été rangé comme cible SSH.
  if [ "$mode" = "local" ] && [ -n "$target" ]; then
    if [ -n "$binaire_local" ]; then
      echo "erreur : trop de paramètres ($target, $binaire_local)" >&2
      exit 1
    fi
    binaire_local="$target"
    target=""
  fi

  if [ "$mode" = "distant" ] && [ -z "$target" ]; then
    echo "erreur : cible SSH manquante (ex. root@nas.local)" >&2
    echo "Pour mettre à jour depuis le NAS lui-même : scripts/deploy.sh --local" >&2
    usage >&2
    exit 1
  fi

  [ -f "$apply_script" ] || {
    echo "erreur : script de bascule introuvable : $apply_script" >&2
    exit 1
  }
}

# --- Mode local -----------------------------------------------------------

deploy_local() {
  local binaire="${binaire_local:-$repo_root/bin/silo}"

  if [ ! -f "$binaire" ]; then
    echo "erreur : binaire introuvable : $binaire" >&2
    echo >&2
    echo "Ce mode ne compile pas : le NAS n'a pas besoin de Go ni de Node/npm," >&2
    echo "le binaire se construit sur la machine de dev puis se transfère :" >&2
    echo "  scripts/build.sh                       # sur la machine de dev" >&2
    echo "  scp bin/silo <cet-hote>:/tmp/silo      # puis, ici :" >&2
    echo "  scripts/deploy.sh --local /tmp/silo" >&2
    exit 1
  fi

  echo "== mise à jour locale depuis $binaire =="
  bash "$apply_script" "$binaire" "$with_unit" "$repo_root/deploy/silo.service"
}

# --- Mode distant ---------------------------------------------------------

check_prerequisites() {
  local tool
  for tool in ssh scp; do
    command -v "$tool" >/dev/null 2>&1 || {
      echo "erreur : '$tool' introuvable sur cette machine." >&2
      exit 1
    }
  done

  echo "== vérification de l'accès à $target =="
  if ! ssh -o BatchMode=yes -o ConnectTimeout=10 "$target" true 2>/dev/null; then
    echo "erreur : connexion SSH à $target impossible sans interaction." >&2
    echo "Vérifie l'accès (clé SSH, nom d'hôte) : ssh $target" >&2
    exit 1
  fi
}

build() {
  if [ "$skip_build" = "oui" ]; then
    [ -f "$repo_root/bin/silo" ] || {
      echo "erreur : --skip-build demandé mais $repo_root/bin/silo est absent." >&2
      exit 1
    }
    echo "== build ignoré, réutilisation de bin/silo =="
    return
  fi
  "$repo_root/scripts/build.sh" linux "$goarch"
}

transfer() {
  echo "== transfert vers $target =="
  scp -q "$repo_root/bin/silo" "$target:/tmp/silo.new"
  if [ "$with_unit" = "oui" ]; then
    scp -q "$repo_root/deploy/silo.service" "$target:/tmp/silo.service.new"
  fi
}

# cleanup_remote retire les fichiers déposés dans /tmp sur la cible. La
# bascule ne les supprime pas elle-même : apply-update.sh ignore d'où
# vient le binaire, et en mode local il ne doit surtout pas l'effacer.
cleanup_remote() {
  ssh "$target" "rm -f /tmp/silo.new /tmp/silo.service.new" 2>/dev/null || true
}

deploy_remote() {
  check_prerequisites
  build
  transfer

  echo "== bascule et redémarrage du service =="
  local statut=0
  ssh "$target" bash -s -- /tmp/silo.new "$with_unit" /tmp/silo.service.new \
    < "$apply_script" || statut=$?

  cleanup_remote
  return $statut
}

# --- Point d'entrée -------------------------------------------------------

main() {
  parse_args "$@"

  if [ "$mode" = "local" ]; then
    deploy_local
    echo "== mise à jour terminée =="
  else
    deploy_remote
    echo "== mise à jour terminée sur $target =="
  fi
}

main "$@"
