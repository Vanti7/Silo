use rusqlite::{ErrorCode, OptionalExtension, Row, params};

use super::{Store, StoreError, db_err};

/// Compte utilisateur de silo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    /// `admin` ou `user`.
    pub role: String,
    pub created_at: String,
}

impl User {
    pub(super) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            username: row.get(1)?,
            password_hash: row.get(2)?,
            role: row.get(3)?,
            created_at: row.get(4)?,
        })
    }
}

const SELECT_USER: &str = "SELECT id, username, password_hash, role, created_at FROM users";

impl Store {
    /// Nombre total de comptes utilisateurs.
    pub fn count_users(&self) -> Result<i64, StoreError> {
        self.conn()
            .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
            .map_err(db_err("comptage utilisateurs"))
    }

    /// Insère un nouvel utilisateur et retourne son ID.
    pub fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        role: &str,
    ) -> Result<i64, StoreError> {
        let conn = self.conn();
        match conn.execute(
            "INSERT INTO users (username, password_hash, role) VALUES (?1, ?2, ?3)",
            params![username, password_hash, role],
        ) {
            Ok(_) => Ok(conn.last_insert_rowid()),
            Err(e) if is_unique_violation(&e) => Err(StoreError::AlreadyExists),
            Err(e) => Err(db_err("création utilisateur")(e)),
        }
    }

    /// Recherche un utilisateur par son identifiant de connexion.
    pub fn get_user_by_username(&self, username: &str) -> Result<User, StoreError> {
        self.conn()
            .query_row(
                &format!("{SELECT_USER} WHERE username = ?1"),
                [username],
                User::from_row,
            )
            .optional()
            .map_err(db_err("lecture utilisateur"))?
            .ok_or(StoreError::NotFound)
    }

    /// Recherche un utilisateur par son ID.
    pub fn get_user_by_id(&self, id: i64) -> Result<User, StoreError> {
        self.conn()
            .query_row(
                &format!("{SELECT_USER} WHERE id = ?1"),
                [id],
                User::from_row,
            )
            .optional()
            .map_err(db_err("lecture utilisateur"))?
            .ok_or(StoreError::NotFound)
    }

    /// Tous les utilisateurs, triés par nom.
    pub fn list_users(&self) -> Result<Vec<User>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(&format!("{SELECT_USER} ORDER BY username"))
            .map_err(db_err("liste utilisateurs"))?;
        stmt.query_map([], User::from_row)
            .map_err(db_err("liste utilisateurs"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err("lecture utilisateur"))
    }

    /// Supprime un utilisateur ; ses sessions partent avec lui (ON DELETE CASCADE).
    pub fn delete_user(&self, id: i64) -> Result<(), StoreError> {
        let n = self
            .conn()
            .execute("DELETE FROM users WHERE id = ?1", [id])
            .map_err(db_err("suppression utilisateur"))?;
        expect_affected(n)
    }

    /// Remplace le hash de mot de passe d'un utilisateur.
    pub fn update_user_password(&self, id: i64, password_hash: &str) -> Result<(), StoreError> {
        let n = self
            .conn()
            .execute(
                "UPDATE users SET password_hash = ?1 WHERE id = ?2",
                params![password_hash, id],
            )
            .map_err(db_err("mise à jour mot de passe"))?;
        expect_affected(n)
    }

    /// Change le rôle d'un utilisateur (`admin` ou `user`).
    pub fn update_user_role(&self, id: i64, role: &str) -> Result<(), StoreError> {
        let n = self
            .conn()
            .execute(
                "UPDATE users SET role = ?1 WHERE id = ?2",
                params![role, id],
            )
            .map_err(db_err("mise à jour rôle"))?;
        expect_affected(n)
    }

    /// Nombre de comptes ayant le rôle admin.
    pub fn count_admins(&self) -> Result<i64, StoreError> {
        self.conn()
            .query_row("SELECT COUNT(*) FROM users WHERE role = 'admin'", [], |r| {
                r.get(0)
            })
            .map_err(db_err("comptage admins"))
    }
}

fn expect_affected(n: usize) -> Result<(), StoreError> {
    if n == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == ErrorCode::ConstraintViolation
                && f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

#[cfg(test)]
mod tests {
    use crate::store::test_store;

    use super::*;

    #[test]
    fn test_create_user_doublon_retourne_already_exists() {
        let (st, _dir) = test_store();
        st.create_user("alice", "h", "admin").unwrap();
        assert!(matches!(
            st.create_user("alice", "h2", "user"),
            Err(StoreError::AlreadyExists)
        ));
    }

    #[test]
    fn test_create_user_role_invalide_retourne_erreur_db() {
        let (st, _dir) = test_store();
        assert!(matches!(
            st.create_user("alice", "h", "root"),
            Err(StoreError::Db { .. })
        ));
    }

    #[test]
    fn test_list_users_trie_par_nom() {
        let (st, _dir) = test_store();
        st.create_user("zoe", "h", "user").unwrap();
        st.create_user("alice", "h", "admin").unwrap();
        let names: Vec<_> = st
            .list_users()
            .unwrap()
            .into_iter()
            .map(|u| u.username)
            .collect();
        assert_eq!(names, ["alice", "zoe"]);
    }

    #[test]
    fn test_update_user_role_inconnu_retourne_not_found() {
        let (st, _dir) = test_store();
        assert!(matches!(
            st.update_user_role(42, "user"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn test_count_admins_ignore_les_utilisateurs() {
        let (st, _dir) = test_store();
        st.create_user("alice", "h", "admin").unwrap();
        st.create_user("bob", "h", "user").unwrap();
        assert_eq!(st.count_admins().unwrap(), 1);
        assert_eq!(st.count_users().unwrap(), 2);
    }
}
