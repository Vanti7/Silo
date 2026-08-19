package store

import (
	"database/sql"
	"errors"
	"fmt"
	"strings"
)

// ErrNotFound est retourné quand une ressource demandée n'existe pas.
var ErrNotFound = errors.New("store: ressource introuvable")

// ErrAlreadyExists est retourné en cas de violation d'unicité (ex: username).
var ErrAlreadyExists = errors.New("store: ressource déjà existante")

// User représente un compte utilisateur de silo.
type User struct {
	ID           int64
	Username     string
	PasswordHash string
	Role         string // "admin" ou "user"
	CreatedAt    string
}

// CountUsers retourne le nombre total de comptes utilisateurs.
func (s *Store) CountUsers() (int, error) {
	var n int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM users`).Scan(&n)
	if err != nil {
		return 0, fmt.Errorf("store: comptage utilisateurs: %w", err)
	}
	return n, nil
}

// CreateUser insère un nouvel utilisateur et retourne son ID.
func (s *Store) CreateUser(username, passwordHash, role string) (int64, error) {
	res, err := s.db.Exec(
		`INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?)`,
		username, passwordHash, role,
	)
	if err != nil {
		if isUniqueConstraintErr(err) {
			return 0, ErrAlreadyExists
		}
		return 0, fmt.Errorf("store: création utilisateur: %w", err)
	}
	return res.LastInsertId()
}

// GetUserByUsername recherche un utilisateur par son identifiant de connexion.
func (s *Store) GetUserByUsername(username string) (*User, error) {
	row := s.db.QueryRow(
		`SELECT id, username, password_hash, role, created_at FROM users WHERE username = ?`,
		username,
	)
	return scanUser(row)
}

// GetUserByID recherche un utilisateur par son ID.
func (s *Store) GetUserByID(id int64) (*User, error) {
	row := s.db.QueryRow(
		`SELECT id, username, password_hash, role, created_at FROM users WHERE id = ?`,
		id,
	)
	return scanUser(row)
}

// ListUsers retourne tous les utilisateurs, triés par nom.
func (s *Store) ListUsers() ([]User, error) {
	rows, err := s.db.Query(
		`SELECT id, username, password_hash, role, created_at FROM users ORDER BY username`,
	)
	if err != nil {
		return nil, fmt.Errorf("store: liste utilisateurs: %w", err)
	}
	defer rows.Close()

	var users []User
	for rows.Next() {
		var u User
		if err := rows.Scan(&u.ID, &u.Username, &u.PasswordHash, &u.Role, &u.CreatedAt); err != nil {
			return nil, fmt.Errorf("store: lecture utilisateur: %w", err)
		}
		users = append(users, u)
	}
	return users, rows.Err()
}

// DeleteUser supprime un utilisateur par son ID.
func (s *Store) DeleteUser(id int64) error {
	res, err := s.db.Exec(`DELETE FROM users WHERE id = ?`, id)
	if err != nil {
		return fmt.Errorf("store: suppression utilisateur: %w", err)
	}
	n, err := res.RowsAffected()
	if err != nil {
		return fmt.Errorf("store: suppression utilisateur: %w", err)
	}
	if n == 0 {
		return ErrNotFound
	}
	return nil
}

// UpdateUserPassword remplace le hash de mot de passe d'un utilisateur.
func (s *Store) UpdateUserPassword(id int64, passwordHash string) error {
	res, err := s.db.Exec(`UPDATE users SET password_hash = ? WHERE id = ?`, passwordHash, id)
	if err != nil {
		return fmt.Errorf("store: mise à jour mot de passe: %w", err)
	}
	return checkRowsAffected(res)
}

// UpdateUserRole change le rôle d'un utilisateur ("admin" ou "user").
func (s *Store) UpdateUserRole(id int64, role string) error {
	res, err := s.db.Exec(`UPDATE users SET role = ? WHERE id = ?`, role, id)
	if err != nil {
		return fmt.Errorf("store: mise à jour rôle: %w", err)
	}
	return checkRowsAffected(res)
}

// CountAdmins retourne le nombre de comptes avec le rôle admin.
func (s *Store) CountAdmins() (int, error) {
	var n int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM users WHERE role = 'admin'`).Scan(&n)
	if err != nil {
		return 0, fmt.Errorf("store: comptage admins: %w", err)
	}
	return n, nil
}

func scanUser(row *sql.Row) (*User, error) {
	var u User
	err := row.Scan(&u.ID, &u.Username, &u.PasswordHash, &u.Role, &u.CreatedAt)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, ErrNotFound
	}
	if err != nil {
		return nil, fmt.Errorf("store: lecture utilisateur: %w", err)
	}
	return &u, nil
}

func checkRowsAffected(res sql.Result) error {
	n, err := res.RowsAffected()
	if err != nil {
		return fmt.Errorf("store: vérification résultat: %w", err)
	}
	if n == 0 {
		return ErrNotFound
	}
	return nil
}

func isUniqueConstraintErr(err error) bool {
	// modernc.org/sqlite renvoie un message contenant "UNIQUE constraint failed".
	return err != nil && strings.Contains(err.Error(), "UNIQUE constraint failed")
}
