#!/usr/bin/env bash
# Tests de scripts/lib/apply-update.sh : bascule, vérification et retour
# arrière. systemctl et curl sont remplacés par des doublures pilotables,
# et les chemins d'installation redirigés vers un dossier temporaire, ce
# qui permet d'exercer le script sans systemd ni privilèges.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sous_test="$script_dir/apply-update.sh"

echecs=0

# prepare_bac_a_sable crée un environnement isolé : faux binaire installé,
# doublures de systemctl et curl, chemins redirigés.
prepare_bac_a_sable() {
  bac="$(mktemp -d)"
  mkdir -p "$bac/bin" "$bac/faux-outils"

  export SILO_BIN="$bac/bin/silo"
  export SILO_BIN_BACKUP="$bac/bin/silo.precedent"
  export SILO_ENV_FILE="$bac/silo.env"
  export SILO_UNIT_FILE="$bac/silo.service"
  export SILO_SERVICE="silo-test"

  # Journal des appels, pour vérifier ce qui a réellement été invoqué.
  export JOURNAL="$bac/appels.log"
  : > "$JOURNAL"

  cat > "$bac/faux-outils/systemctl" <<'OUTIL'
#!/usr/bin/env bash
echo "systemctl $*" >> "$JOURNAL"
exit 0
OUTIL

  # La santé du service est pilotée par le fichier ETAT_SANTE : "ok" pour
  # une réponse HTTP valide, autre chose pour un échec.
  cat > "$bac/faux-outils/curl" <<'OUTIL'
#!/usr/bin/env bash
echo "curl $*" >> "$JOURNAL"
[ "$(cat "$ETAT_SANTE")" = "ok" ]
OUTIL

  # Raccourcit l'attente de 15 s de la boucle de vérification.
  cat > "$bac/faux-outils/sleep" <<'OUTIL'
#!/usr/bin/env bash
exit 0
OUTIL

  # Le harnais ne tourne pas en root : sudo est neutralisé et exécute
  # simplement la commande qu'on lui passe.
  cat > "$bac/faux-outils/sudo" <<'OUTIL'
#!/usr/bin/env bash
[ "${1:-}" = "-n" ] && shift
exec "$@"
OUTIL

  chmod +x "$bac/faux-outils/systemctl" "$bac/faux-outils/curl" \
    "$bac/faux-outils/sleep" "$bac/faux-outils/sudo"
  export PATH="$bac/faux-outils:$PATH"

  export ETAT_SANTE="$bac/etat_sante"
  echo "ok" > "$ETAT_SANTE"

  printf 'SILO_LISTEN_ADDR=:8080\n' > "$SILO_ENV_FILE"

  # Version « déjà installée » et nouvelle version à déployer.
  printf 'ancienne version\n' > "$SILO_BIN"
  chmod +x "$SILO_BIN"
  printf 'nouvelle version\n' > "$bac/silo.nouveau"
}

nettoyer() {
  [ -n "${bac:-}" ] && rm -rf "$bac"
}

verifier() {
  local description="$1" attendu="$2" obtenu="$3"
  if [ "$attendu" = "$obtenu" ]; then
    echo "  OK    $description"
  else
    echo "  ECHEC $description : attendu '$attendu', obtenu '$obtenu'"
    echecs=$((echecs + 1))
  fi
}

# --- Cas 1 : mise à jour réussie ------------------------------------------
test_bascule_reussie() {
  echo "cas : le service repart correctement"
  prepare_bac_a_sable

  local statut=0
  bash "$sous_test" "$bac/silo.nouveau" non >/dev/null 2>&1 || statut=$?

  verifier "sortie en succès" "0" "$statut"
  verifier "nouvelle version installée" "nouvelle version" "$(cat "$SILO_BIN")"
  verifier "version précédente sauvegardée" "ancienne version" "$(cat "$SILO_BIN_BACKUP")"
  verifier "binaire source préservé" "nouvelle version" "$(cat "$bac/silo.nouveau")"
  verifier "service redémarré" "1" "$(grep -c 'systemctl restart silo-test' "$JOURNAL")"

  nettoyer
}

# --- Cas 2 : le service ne répond plus, retour arrière ---------------------
test_retour_arriere() {
  echo "cas : le service ne répond pas, retour arrière attendu"
  prepare_bac_a_sable
  echo "ko" > "$ETAT_SANTE"

  local statut=0
  bash "$sous_test" "$bac/silo.nouveau" non >/dev/null 2>&1 || statut=$?

  verifier "sortie en erreur" "1" "$statut"
  verifier "version précédente restaurée" "ancienne version" "$(cat "$SILO_BIN")"
  # Un redémarrage pour la nouvelle version, un second pour la restauration.
  verifier "deux redémarrages" "2" "$(grep -c 'systemctl restart silo-test' "$JOURNAL")"

  nettoyer
}

# --- Cas 3 : première installation, aucune version à restaurer -------------
test_premiere_installation() {
  echo "cas : première installation en échec, rien à restaurer"
  prepare_bac_a_sable
  rm -f "$SILO_BIN"
  echo "ko" > "$ETAT_SANTE"

  local statut=0
  local sortie
  sortie="$(bash "$sous_test" "$bac/silo.nouveau" non 2>&1)" || statut=$?

  verifier "sortie en erreur" "1" "$statut"
  verifier "aucune sauvegarde créée" "absent" "$([ -e "$SILO_BIN_BACKUP" ] && echo present || echo absent)"
  if echo "$sortie" | grep -q "aucune version précédente"; then
    echo "  OK    message « aucune version précédente » affiché"
  else
    echo "  ECHEC message « aucune version précédente » absent"
    echecs=$((echecs + 1))
  fi

  nettoyer
}

# --- Cas 4 : mise à jour de l'unité systemd --------------------------------
test_avec_unite() {
  echo "cas : --with-unit installe l'unité et recharge systemd"
  prepare_bac_a_sable
  printf '[Unit]\nDescription=silo test\n' > "$bac/silo.service.source"

  local statut=0
  bash "$sous_test" "$bac/silo.nouveau" oui "$bac/silo.service.source" >/dev/null 2>&1 || statut=$?

  verifier "sortie en succès" "0" "$statut"
  verifier "unité installée" "[Unit]" "$(head -n1 "$SILO_UNIT_FILE")"
  verifier "daemon-reload appelé" "1" "$(grep -c 'systemctl daemon-reload' "$JOURNAL")"

  nettoyer
}

# --- Cas 5 : unité demandée mais source absente ----------------------------
test_unite_absente() {
  echo "cas : --with-unit sans fichier d'unité"
  prepare_bac_a_sable

  local statut=0
  bash "$sous_test" "$bac/silo.nouveau" oui "$bac/inexistante.service" >/dev/null 2>&1 || statut=$?

  verifier "sortie en erreur" "1" "$statut"

  nettoyer
}

# --- Cas 6 : adresse d'écoute personnalisée --------------------------------
test_adresse_personnalisee() {
  echo "cas : l'adresse interrogée suit SILO_LISTEN_ADDR"
  prepare_bac_a_sable
  printf 'SILO_LISTEN_ADDR=0.0.0.0:9999\n' > "$SILO_ENV_FILE"

  bash "$sous_test" "$bac/silo.nouveau" non >/dev/null 2>&1 || true

  if grep -q 'http://127.0.0.1:9999/api/v1/setup/status' "$JOURNAL"; then
    echo "  OK    vérification HTTP sur 127.0.0.1:9999"
  else
    echo "  ECHEC adresse interrogée incorrecte : $(grep curl "$JOURNAL" | head -n1)"
    echecs=$((echecs + 1))
  fi

  nettoyer
}

test_bascule_reussie
test_retour_arriere
test_premiere_installation
test_avec_unite
test_unite_absente
test_adresse_personnalisee

echo
if [ "$echecs" -eq 0 ]; then
  echo "tous les cas passent"
else
  echo "$echecs assertion(s) en échec"
  exit 1
fi
