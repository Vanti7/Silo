//! Configuration de silo, chargée uniquement depuis des variables
//! d'environnement (12-factor).

use std::collections::HashMap;
use std::env;

/// Paramètres de configuration du service.
#[derive(Debug, Clone)]
pub struct Config {
    /// Adresse d'écoute HTTP (ex : `:8080`).
    pub listen_addr: String,
    /// Dossier contenant la base SQLite et les données internes de l'app.
    pub data_dir: String,
    /// Racines de partage exposées par l'explorateur de fichiers (nom -> chemin).
    pub share_roots: HashMap<String, String>,
    /// Force l'attribut `Secure` sur le cookie de session.
    pub session_cookie_secure: bool,
    /// Durée de vie d'une session, en heures.
    pub session_ttl_hours: i64,
    /// Hôte du daemon Docker (vide = valeur par défaut, `DOCKER_HOST` ou socket unix).
    pub docker_host: String,
}

/// Erreur de chargement de la configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config: SILO_SHARE_ROOTS invalide: entrée {0:?} invalide, format attendu nom=chemin")]
    InvalidShareRoot(String),
}

impl Config {
    /// Lit la configuration depuis les variables d'environnement et applique
    /// des valeurs par défaut raisonnables pour un déploiement NAS.
    pub fn load() -> Result<Self, ConfigError> {
        Ok(Self {
            listen_addr: get_env("SILO_LISTEN_ADDR", ":8080"),
            data_dir: get_env("SILO_DATA_DIR", "/var/lib/silo"),
            share_roots: parse_share_roots(&env::var("SILO_SHARE_ROOTS").unwrap_or_default())?,
            session_cookie_secure: parse_bool(&env::var("SILO_COOKIE_SECURE").unwrap_or_default())
                .unwrap_or(true),
            session_ttl_hours: env::var("SILO_SESSION_TTL_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(168),
            docker_host: env::var("SILO_DOCKER_HOST").unwrap_or_default(),
        })
    }
}

/// Parse `nom=chemin,nom2=chemin2` en association nom -> chemin.
pub fn parse_share_roots(raw: &str) -> Result<HashMap<String, String>, ConfigError> {
    let mut roots = HashMap::new();
    for entry in raw.trim().split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let invalid = || ConfigError::InvalidShareRoot(entry.to_string());
        let (name, path) = entry.split_once('=').ok_or_else(invalid)?;
        let (name, path) = (name.trim(), path.trim());
        if name.is_empty() || path.is_empty() {
            return Err(invalid());
        }
        roots.insert(name.to_string(), path.to_string());
    }
    Ok(roots)
}

/// Interprète un booléen avec les mêmes formes que `strconv.ParseBool` en Go,
/// pour que les fichiers `silo.env` existants restent valides.
pub fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
        _ => None,
    }
}

fn get_env(key: &str, fallback: &str) -> String {
    match env::var(key) {
        Ok(v) if !v.is_empty() => v,
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_share_roots_deux_entrees_retourne_les_deux() {
        let got = parse_share_roots("pool=/mnt/pool, backup=/mnt/backup").unwrap();
        let want = HashMap::from([
            ("pool".to_string(), "/mnt/pool".to_string()),
            ("backup".to_string(), "/mnt/backup".to_string()),
        ]);
        assert_eq!(got, want);
    }

    #[test]
    fn test_parse_share_roots_vide_retourne_aucune_racine() {
        assert!(parse_share_roots("").unwrap().is_empty());
    }

    #[test]
    fn test_parse_share_roots_sans_egal_retourne_erreur() {
        assert!(parse_share_roots("pool-sans-egal").is_err());
    }

    #[test]
    fn test_parse_share_roots_nom_ou_chemin_vide_retourne_erreur() {
        for input in ["=/mnt/pool", "pool=", "  =  "] {
            assert!(parse_share_roots(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn test_parse_bool_formes_go_reconnues() {
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("True"), Some(true));
        assert_eq!(parse_bool("oui"), None);
        assert_eq!(parse_bool(""), None);
    }
}
