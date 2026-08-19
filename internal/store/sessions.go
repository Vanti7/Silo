package store

import (
	"database/sql"
	"errors"
	"fmt"
	"time"
)

// CreateSession enregistre une nouvelle session pour l'utilisateur donné.
// tokenHash est le SHA-256 du jeton de session (le jeton en clair n'est
// jamais persisté).
func (s *Store) CreateSession(tokenHash string, userID int64, ttl time.Duration) error {
	expiresAt := time.Now().Add(ttl).UTC().Format(time.RFC3339)
	_, err := s.db.Exec(
		`INSERT INTO sessions (token_hash, user_id, expires_at) VALUES (?, ?, ?)`,
		tokenHash, userID, expiresAt,
	)
	if err != nil {
		return fmt.Errorf("store: création session: %w", err)
	}
	return nil
}

// GetSessionUser retourne l'utilisateur associé à une session valide
// (non expirée). Renvoie ErrNotFound si le jeton est inconnu ou expiré.
func (s *Store) GetSessionUser(tokenHash string) (*User, error) {
	row := s.db.QueryRow(`
		SELECT u.id, u.username, u.password_hash, u.role, u.created_at
		FROM sessions s
		JOIN users u ON u.id = s.user_id
		WHERE s.token_hash = ? AND s.expires_at > ?`,
		tokenHash, time.Now().UTC().Format(time.RFC3339),
	)
	var u User
	err := row.Scan(&u.ID, &u.Username, &u.PasswordHash, &u.Role, &u.CreatedAt)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, ErrNotFound
	}
	if err != nil {
		return nil, fmt.Errorf("store: lecture session: %w", err)
	}
	return &u, nil
}

// DeleteSession invalide une session (déconnexion).
func (s *Store) DeleteSession(tokenHash string) error {
	_, err := s.db.Exec(`DELETE FROM sessions WHERE token_hash = ?`, tokenHash)
	if err != nil {
		return fmt.Errorf("store: suppression session: %w", err)
	}
	return nil
}

// DeleteExpiredSessions purge les sessions expirées. À appeler
// périodiquement pour éviter la croissance illimitée de la table.
func (s *Store) DeleteExpiredSessions() error {
	_, err := s.db.Exec(`DELETE FROM sessions WHERE expires_at <= ?`, time.Now().UTC().Format(time.RFC3339))
	if err != nil {
		return fmt.Errorf("store: purge sessions expirées: %w", err)
	}
	return nil
}
