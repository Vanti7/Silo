#!/usr/bin/env bash
# Met à jour silo sur le NAS : build local, transfert, bascule du binaire
# et redémarrage du service. Si le service ne repart pas, la version
# précédente est automatiquement restaurée.
# Le fichier de configuration /etc/silo/silo.env n'est jamais touché.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target=""
with_unit="non"
skip_build="non"
goarch="amd64"

usage() {
  cat <<'USAGE'
Usage : scripts/deploy.sh <utilisateur@hote> [options]

  --with-unit      met aussi à jour /etc/systemd/system/silo.service
  --skip-build     réutilise bin/silo tel quel, sans recompiler
  --arch <arch>    architecture cible (défaut : amd64 ; ex. arm64)
  -h, --help       affiche cette aide

Exemple : scripts/deploy.sh root@nas.local
USAGE
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
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
        [ -z "$target" ] || { echo "erreur : cible déjà définie ($target)" >&2; exit 1; }
        target="$1"
        ;;
    esac
    shift
  done

  if [ -z "$target" ]; then
    echo "erreur : cible SSH manquante (ex. root@nas.local)" >&2
    usage >&2
    exit 1
  fi
}

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

# swap exécute la bascule sur le NAS. Le script distant est transmis tel
# quel (heredoc entre quotes) : rien n'est interprété localement, et les
# paramètres passent par des arguments positionnels.
swap() {
  echo "== bascule et redémarrage du service =="
  ssh "$target" bash -s -- "$with_unit" <<'REMOTE'
set -euo pipefail

with_unit="$1"
binaire="/usr/local/bin/silo"
sauvegarde="/usr/local/bin/silo.precedent"

if [ "$(id -u)" -eq 0 ]; then
  sudo_cmd=()
else
  sudo_cmd=(sudo -n)
  "${sudo_cmd[@]}" true 2>/dev/null || {
    echo "erreur : sudo non disponible sans mot de passe sur l'hôte distant." >&2
    echo "Connecte-toi en root, ou autorise sudo sans mot de passe pour ce compte." >&2
    exit 1
  }
fi

# Adresse d'écoute effective, pour la vérification HTTP. Une adresse sans
# hôte (":8080") ou à l'écoute de toutes les interfaces est interrogée sur
# la boucle locale.
adresse_sante() {
  local addr=":8080"
  local depuis_conf=""
  if [ -r /etc/silo/silo.env ]; then
    depuis_conf="$(grep -E '^[[:space:]]*SILO_LISTEN_ADDR=' /etc/silo/silo.env |
      tail -n1 | cut -d= -f2- | tr -d "\"' ")"
  fi
  [ -n "$depuis_conf" ] && addr="$depuis_conf"

  case "$addr" in
    :*) echo "127.0.0.1${addr}" ;;
    0.0.0.0:*) echo "127.0.0.1:${addr##*:}" ;;
    *) echo "$addr" ;;
  esac
}

# Le service peut mettre un instant à écouter : on lui laisse 15 s. Sans
# curl sur l'hôte, on se contente de l'état systemd.
verifier_service() {
  local i
  for i in $(seq 1 15); do
    if systemctl is-active --quiet silo; then
      if ! command -v curl >/dev/null 2>&1; then
        return 0
      fi
      if curl -fsS --max-time 3 "http://$(adresse_sante)/api/v1/setup/status" >/dev/null 2>&1; then
        return 0
      fi
    fi
    sleep 1
  done
  return 1
}

if [ -x "$binaire" ]; then
  "${sudo_cmd[@]}" cp -p "$binaire" "$sauvegarde"
fi

"${sudo_cmd[@]}" install -m 0755 /tmp/silo.new "$binaire"
"${sudo_cmd[@]}" rm -f /tmp/silo.new

if [ "$with_unit" = "oui" ]; then
  "${sudo_cmd[@]}" install -m 0644 /tmp/silo.service.new /etc/systemd/system/silo.service
  "${sudo_cmd[@]}" rm -f /tmp/silo.service.new
  "${sudo_cmd[@]}" systemctl daemon-reload
  echo "unité systemd mise à jour"
fi

"${sudo_cmd[@]}" systemctl restart silo

if verifier_service; then
  echo "service actif et répond aux requêtes"
  exit 0
fi

echo "erreur : le service ne répond pas après la mise à jour." >&2
if [ -x "$sauvegarde" ]; then
  echo "retour arrière vers la version précédente..." >&2
  "${sudo_cmd[@]}" install -m 0755 "$sauvegarde" "$binaire"
  "${sudo_cmd[@]}" systemctl restart silo
  if verifier_service; then
    echo "version précédente restaurée, le service répond de nouveau." >&2
  else
    echo "le service ne repart pas non plus avec la version précédente." >&2
  fi
else
  echo "aucune version précédente à restaurer (première installation)." >&2
fi
echo "Inspecter les journaux : journalctl -u silo -n 50" >&2
exit 1
REMOTE
}

main() {
  parse_args "$@"
  check_prerequisites
  build
  transfer
  swap
  echo "== mise à jour terminée sur $target =="
}

main "$@"
