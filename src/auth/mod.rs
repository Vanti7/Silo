//! Authentification par sessions opaques (cookie httpOnly) : hashage des
//! mots de passe et cycle de vie des jetons de session.

pub mod middleware;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// Nom du cookie de session HTTP.
pub const COOKIE_NAME: &str = "silo_session";

/// Longueur minimale d'un mot de passe, partagée par l'API HTTP et la
/// sous-commande CLI de réinitialisation afin qu'elles ne divergent pas.
pub const MIN_PASSWORD_LENGTH: usize = 8;
/// Longueur maximale d'un mot de passe.
pub const MAX_PASSWORD_LENGTH: usize = 256;

// Les hashs existants ont été produits avec le coût par défaut de Go
// (10) : le conserver garde des temps de connexion identiques.
const BCRYPT_COST: u32 = 10;

// bcrypt ignore tout ce qui dépasse 72 octets ; plutôt que d'accepter
// silencieusement un mot de passe tronqué, on le refuse, comme le faisait
// golang.org/x/crypto/bcrypt.
const BCRYPT_MAX_INPUT: usize = 72;

/// Erreur du module d'authentification.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("mot de passe trop court ({MIN_PASSWORD_LENGTH} caractères minimum)")]
    PasswordTooShort,
    #[error("mot de passe trop long ({MAX_PASSWORD_LENGTH} caractères maximum)")]
    PasswordTooLong,
    #[error("auth: hashage mot de passe: {0}")]
    Hash(String),
    #[error("auth: génération jeton: {0}")]
    Random(String),
}

/// Vérifie qu'un mot de passe respecte les contraintes de longueur.
pub fn validate_password(password: &str) -> Result<(), AuthError> {
    if password.len() < MIN_PASSWORD_LENGTH {
        return Err(AuthError::PasswordTooShort);
    }
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(AuthError::PasswordTooLong);
    }
    Ok(())
}

/// Dérive un hash bcrypt à partir d'un mot de passe en clair.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    if password.len() > BCRYPT_MAX_INPUT {
        return Err(AuthError::Hash(format!(
            "mot de passe de plus de {BCRYPT_MAX_INPUT} octets"
        )));
    }
    bcrypt::hash(password, BCRYPT_COST).map_err(|e| AuthError::Hash(e.to_string()))
}

/// Compare un mot de passe en clair à un hash bcrypt.
pub fn verify_password(hash: &str, password: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Génère un jeton de session aléatoire (256 bits) encodé en base64
/// URL-safe, à transmettre au client via cookie.
pub fn new_session_token() -> Result<String, AuthError> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(|e| AuthError::Random(e.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

/// Calcule le SHA-256 d'un jeton de session, seule forme persistée en base
/// (le jeton en clair ne quitte jamais le cookie client).
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password_puis_verify_accepte_seulement_le_bon() {
        let hash = hash_password("mot-de-passe-correct").unwrap();
        assert!(verify_password(&hash, "mot-de-passe-correct"));
        assert!(!verify_password(&hash, "mauvais-mot-de-passe"));
    }

    #[test]
    fn test_verify_password_hash_produit_par_go_accepte() {
        // Hash généré par golang.org/x/crypto/bcrypt (coût 10) pour
        // "mot-de-passe-correct" : les comptes existants doivent rester
        // utilisables après la migration.
        let go_hash = "$2a$10$OOMwALbjxrKSVLbegTuj..SwleG/vNVYP0cSx3TZlcTgo9MpOcOg.";
        assert!(verify_password(go_hash, "mot-de-passe-correct"));
        assert!(!verify_password(go_hash, "autre"));
    }

    #[test]
    fn test_hash_password_plus_de_72_octets_retourne_erreur() {
        assert!(hash_password(&"a".repeat(73)).is_err());
    }

    #[test]
    fn test_new_session_token_deux_appels_uniques_et_non_vides() {
        let a = new_session_token().unwrap();
        let b = new_session_token().unwrap();
        assert!(!a.is_empty() && !b.is_empty());
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
    }

    #[test]
    fn test_validate_password_bornes_respectees() {
        for p in ["12345678", "un-mot-de-passe-correct"] {
            assert!(validate_password(p).is_ok(), "{p:?}");
        }
        let too_long = "a".repeat(MAX_PASSWORD_LENGTH + 1);
        for p in ["", "1234567", too_long.as_str()] {
            assert!(validate_password(p).is_err(), "{p:?}");
        }
    }

    #[test]
    fn test_hash_token_deterministe_et_discriminant() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("def"));
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
