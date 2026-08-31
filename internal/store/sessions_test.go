package store

import (
	"errors"
	"testing"
	"time"
)

// newTestStore ouvre une base SQLite jetable dans un dossier temporaire.
func newTestStore(t *testing.T) *Store {
	t.Helper()
	st, err := Open(t.TempDir())
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })
	return st
}

func TestDeleteUserSessions(t *testing.T) {
	st := newTestStore(t)

	alice, err := st.CreateUser("alice", "hash-alice", "admin")
	if err != nil {
		t.Fatalf("CreateUser: %v", err)
	}
	bob, err := st.CreateUser("bob", "hash-bob", "user")
	if err != nil {
		t.Fatalf("CreateUser: %v", err)
	}

	// Deux sessions pour alice (deux appareils) et une pour bob.
	for _, tokenHash := range []string{"alice-1", "alice-2"} {
		if err := st.CreateSession(tokenHash, alice, time.Hour); err != nil {
			t.Fatalf("CreateSession: %v", err)
		}
	}
	if err := st.CreateSession("bob-1", bob, time.Hour); err != nil {
		t.Fatalf("CreateSession: %v", err)
	}

	if err := st.DeleteUserSessions(alice); err != nil {
		t.Fatalf("DeleteUserSessions: %v", err)
	}

	for _, tokenHash := range []string{"alice-1", "alice-2"} {
		if _, err := st.GetSessionUser(tokenHash); !errors.Is(err, ErrNotFound) {
			t.Errorf("session %q : erreur = %v, attendu ErrNotFound", tokenHash, err)
		}
	}
	// Les sessions des autres comptes ne doivent pas être affectées.
	if _, err := st.GetSessionUser("bob-1"); err != nil {
		t.Errorf("la session de bob ne devait pas être révoquée : %v", err)
	}
}

func TestGetSessionUser_Expiree(t *testing.T) {
	st := newTestStore(t)

	userID, err := st.CreateUser("alice", "hash", "admin")
	if err != nil {
		t.Fatalf("CreateUser: %v", err)
	}
	if err := st.CreateSession("expiree", userID, -time.Minute); err != nil {
		t.Fatalf("CreateSession: %v", err)
	}

	if _, err := st.GetSessionUser("expiree"); !errors.Is(err, ErrNotFound) {
		t.Errorf("erreur = %v, attendu ErrNotFound pour une session expirée", err)
	}
}

func TestDeleteSession(t *testing.T) {
	st := newTestStore(t)

	userID, err := st.CreateUser("alice", "hash", "admin")
	if err != nil {
		t.Fatalf("CreateUser: %v", err)
	}
	if err := st.CreateSession("jeton", userID, time.Hour); err != nil {
		t.Fatalf("CreateSession: %v", err)
	}
	if _, err := st.GetSessionUser("jeton"); err != nil {
		t.Fatalf("la session devrait être valide : %v", err)
	}

	if err := st.DeleteSession("jeton"); err != nil {
		t.Fatalf("DeleteSession: %v", err)
	}
	if _, err := st.GetSessionUser("jeton"); !errors.Is(err, ErrNotFound) {
		t.Errorf("erreur = %v, attendu ErrNotFound après déconnexion", err)
	}
}

func TestDeleteUser_SupprimeSesSessions(t *testing.T) {
	st := newTestStore(t)

	userID, err := st.CreateUser("alice", "hash", "admin")
	if err != nil {
		t.Fatalf("CreateUser: %v", err)
	}
	if err := st.CreateSession("jeton", userID, time.Hour); err != nil {
		t.Fatalf("CreateSession: %v", err)
	}

	if err := st.DeleteUser(userID); err != nil {
		t.Fatalf("DeleteUser: %v", err)
	}
	// La contrainte ON DELETE CASCADE doit avoir emporté la session.
	if _, err := st.GetSessionUser("jeton"); !errors.Is(err, ErrNotFound) {
		t.Errorf("erreur = %v, attendu ErrNotFound après suppression du compte", err)
	}
}
