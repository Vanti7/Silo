#!/usr/bin/env bash
# Applique une mise à jour de silo sur l'hôte où ce script s'exécute :
# sauvegarde du binaire en place, bascule, redémarrage du service et
# vérification qu'il répond. Si le service ne repart pas, la version
# précédente est restaurée.
#
# Ce script est appelé de deux façons par scripts/deploy.sh :
#   - à distance : transmis à `ssh <cible> bash -s -- <args>` ;
#   - localement : exécuté directement sur le NAS (mode --local).
# D'où l'absence de toute dépendance au dépôt : il ne reçoit que des
# chemins de fichiers déjà présents sur l'hôte.
#
# Usage : apply-update.sh <binaire-source> <oui|non> [unite-source]
#
# Le fichier de configuration /etc/silo/silo.env n'est jamais touché, et
# le binaire source n'est jamais supprimé (l'appelant gère ses fichiers).
set -euo pipefail

binaire_source="${1:?chemin du binaire source manquant}"
with_unit="${2:-non}"
unite_source="${3:-}"

# Chemins de production, surchargeables pour permettre de tester la
# bascule et le retour arrière hors d'un vrai système (voir
# apply-update_test.sh).
binaire="${SILO_BIN:-/usr/local/bin/silo}"
sauvegarde="${SILO_BIN_BACKUP:-/usr/local/bin/silo.precedent}"
fichier_env="${SILO_ENV_FILE:-/etc/silo/silo.env}"
unite_cible="${SILO_UNIT_FILE:-/etc/systemd/system/silo.service}"
service="${SILO_SERVICE:-silo}"

if [ ! -f "$binaire_source" ]; then
  echo "erreur : binaire source introuvable : $binaire_source" >&2
  exit 1
fi

if [ "$(id -u)" -eq 0 ]; then
  sudo_cmd=()
else
  sudo_cmd=(sudo -n)
  "${sudo_cmd[@]}" true 2>/dev/null || {
    echo "erreur : sudo non disponible sans mot de passe sur cet hôte." >&2
    echo "Exécute en root, ou autorise sudo sans mot de passe pour ce compte." >&2
    exit 1
  }
fi

# Adresse d'écoute effective, pour la vérification HTTP. Une adresse sans
# hôte (":8080") ou à l'écoute de toutes les interfaces est interrogée sur
# la boucle locale.
adresse_sante() {
  local addr=":8080"
  local depuis_conf=""
  if [ -r "$fichier_env" ]; then
    depuis_conf="$(grep -E '^[[:space:]]*SILO_LISTEN_ADDR=' "$fichier_env" |
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
    if systemctl is-active --quiet "$service"; then
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

if [ -f "$binaire" ]; then
  "${sudo_cmd[@]}" cp -p "$binaire" "$sauvegarde"
fi

"${sudo_cmd[@]}" install -m 0755 "$binaire_source" "$binaire"

if [ "$with_unit" = "oui" ]; then
  if [ ! -f "$unite_source" ]; then
    echo "erreur : unité systemd source introuvable : $unite_source" >&2
    exit 1
  fi
  "${sudo_cmd[@]}" install -m 0644 "$unite_source" "$unite_cible"
  "${sudo_cmd[@]}" systemctl daemon-reload
  echo "unité systemd mise à jour"
fi

"${sudo_cmd[@]}" systemctl restart "$service"

if verifier_service; then
  echo "service actif et répond aux requêtes"
  exit 0
fi

echo "erreur : le service ne répond pas après la mise à jour." >&2
if [ -f "$sauvegarde" ]; then
  echo "retour arrière vers la version précédente..." >&2
  "${sudo_cmd[@]}" install -m 0755 "$sauvegarde" "$binaire"
  "${sudo_cmd[@]}" systemctl restart "$service"
  if verifier_service; then
    echo "version précédente restaurée, le service répond de nouveau." >&2
  else
    echo "le service ne repart pas non plus avec la version précédente." >&2
  fi
else
  echo "aucune version précédente à restaurer (première installation)." >&2
fi
echo "Inspecter les journaux : journalctl -u $service -n 50" >&2
exit 1
