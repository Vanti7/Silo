// Package store gère la persistance de silo (utilisateurs, sessions) via
// une base SQLite embarquée (pure Go, sans cgo).
package store

import (
	"database/sql"
	"fmt"
	"os"
	"path/filepath"

	_ "modernc.org/sqlite"
)

// Store encapsule la connexion à la base SQLite et les opérations de
// persistance de l'application.
type Store struct {
	db *sql.DB
}

// Open ouvre (ou crée) la base SQLite dans dataDir et applique les
// migrations nécessaires.
func Open(dataDir string) (*Store, error) {
	if err := os.MkdirAll(dataDir, 0o750); err != nil {
		return nil, fmt.Errorf("store: création du dossier de données: %w", err)
	}

	dbPath := filepath.Join(dataDir, "silo.db")
	db, err := sql.Open("sqlite", dbPath+"?_pragma=busy_timeout(5000)&_pragma=foreign_keys(1)")
	if err != nil {
		return nil, fmt.Errorf("store: ouverture sqlite: %w", err)
	}
	// SQLite ne gère pas bien les écritures concurrentes multi-connexions.
	db.SetMaxOpenConns(1)

	s := &Store{db: db}
	if err := s.migrate(); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("store: migration: %w", err)
	}
	return s, nil
}

// Close ferme la connexion à la base.
func (s *Store) Close() error {
	return s.db.Close()
}

func (s *Store) migrate() error {
	const schema = `
CREATE TABLE IF NOT EXISTS users (
	id         INTEGER PRIMARY KEY AUTOINCREMENT,
	username   TEXT NOT NULL UNIQUE,
	password_hash TEXT NOT NULL,
	role       TEXT NOT NULL CHECK (role IN ('admin', 'user')),
	created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS sessions (
	token_hash TEXT PRIMARY KEY,
	user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
	created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
	expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
`
	_, err := s.db.Exec(schema)
	return err
}
