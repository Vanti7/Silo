use rusqlite::{OptionalExtension, params};
use time::{Duration, OffsetDateTime};

use super::{Store, StoreError, User, db_err, format_utc};

impl Store {
    /// Enregistre une nouvelle session. `token_hash` est le SHA-256 du jeton
    /// (le jeton en clair n'est jamais persisté).
    pub fn create_session(
        &self,
        token_hash: &str,
        user_id: i64,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        let expires_at = format_utc(OffsetDateTime::now_utc() + ttl);
        self.conn()
            .execute(
                "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES (?1, ?2, ?3)",
                params![token_hash, user_id, expires_at],
            )
            .map_err(db_err("création session"))?;
        Ok(())
    }

    /// Utilisateur associé à une session non expirée, ou `NotFound`.
    pub fn get_session_user(&self, token_hash: &str) -> Result<User, StoreError> {
        self.conn()
            .query_row(
                "SELECT u.id, u.username, u.password_hash, u.role, u.created_at
                 FROM sessions s
                 JOIN users u ON u.id = s.user_id
                 WHERE s.token_hash = ?1 AND s.expires_at > ?2",
                params![token_hash, format_utc(OffsetDateTime::now_utc())],
                User::from_row,
            )
            .optional()
            .map_err(db_err("lecture session"))?
            .ok_or(StoreError::NotFound)
    }

    /// Invalide une session (déconnexion).
    pub fn delete_session(&self, token_hash: &str) -> Result<(), StoreError> {
        self.conn()
            .execute("DELETE FROM sessions WHERE token_hash = ?1", [token_hash])
            .map_err(db_err("suppression session"))?;
        Ok(())
    }

    /// Invalide toutes les sessions d'un utilisateur : après un changement
    /// de mot de passe, un cookie obtenu avant cesse de fonctionner.
    pub fn delete_user_sessions(&self, user_id: i64) -> Result<(), StoreError> {
        self.conn()
            .execute("DELETE FROM sessions WHERE user_id = ?1", [user_id])
            .map_err(db_err("suppression sessions utilisateur"))?;
        Ok(())
    }

    /// Purge les sessions expirées, pour borner la taille de la table.
    pub fn delete_expired_sessions(&self) -> Result<(), StoreError> {
        self.conn()
            .execute(
                "DELETE FROM sessions WHERE expires_at <= ?1",
                [format_utc(OffsetDateTime::now_utc())],
            )
            .map_err(db_err("purge sessions expirées"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store;

    use super::*;

    #[test]
    fn test_delete_user_sessions_revoque_seulement_ce_compte() {
        let (st, _dir) = test_store();
        let alice = st.create_user("alice", "hash-alice", "admin").unwrap();
        let bob = st.create_user("bob", "hash-bob", "user").unwrap();
        for token in ["alice-1", "alice-2"] {
            st.create_session(token, alice, Duration::hours(1)).unwrap();
        }
        st.create_session("bob-1", bob, Duration::hours(1)).unwrap();

        st.delete_user_sessions(alice).unwrap();

        for token in ["alice-1", "alice-2"] {
            assert!(matches!(
                st.get_session_user(token),
                Err(StoreError::NotFound)
            ));
        }
        assert_eq!(st.get_session_user("bob-1").unwrap().username, "bob");
    }

    #[test]
    fn test_get_session_user_expiree_retourne_not_found() {
        let (st, _dir) = test_store();
        let id = st.create_user("alice", "hash", "admin").unwrap();
        st.create_session("expiree", id, Duration::minutes(-1))
            .unwrap();
        assert!(matches!(
            st.get_session_user("expiree"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn test_delete_session_deconnexion_invalide_le_jeton() {
        let (st, _dir) = test_store();
        let id = st.create_user("alice", "hash", "admin").unwrap();
        st.create_session("jeton", id, Duration::hours(1)).unwrap();
        assert!(st.get_session_user("jeton").is_ok());

        st.delete_session("jeton").unwrap();
        assert!(matches!(
            st.get_session_user("jeton"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn test_delete_user_supprime_ses_sessions_en_cascade() {
        let (st, _dir) = test_store();
        let id = st.create_user("alice", "hash", "admin").unwrap();
        st.create_session("jeton", id, Duration::hours(1)).unwrap();

        st.delete_user(id).unwrap();
        assert!(matches!(
            st.get_session_user("jeton"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn test_delete_expired_sessions_garde_les_valides() {
        let (st, _dir) = test_store();
        let id = st.create_user("alice", "hash", "admin").unwrap();
        st.create_session("valide", id, Duration::hours(1)).unwrap();
        st.create_session("expiree", id, Duration::minutes(-1))
            .unwrap();

        st.delete_expired_sessions().unwrap();

        let restantes: i64 = st
            .conn()
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(restantes, 1);
    }

    #[test]
    fn test_get_session_user_session_ecrite_par_go_reconnue() {
        // Une session créée par la version Go (format RFC3339 sans fraction)
        // doit rester valide après la migration.
        let (st, _dir) = test_store();
        let id = st.create_user("alice", "hash", "admin").unwrap();
        st.conn()
            .execute(
                "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ('go', ?1, '2999-01-01T00:00:00Z')",
                [id],
            )
            .unwrap();
        assert_eq!(st.get_session_user("go").unwrap().id, id);
    }
}
