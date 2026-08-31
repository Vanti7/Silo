// Package auth gère l'authentification par sessions opaques (cookie
// httpOnly) pour silo : hashage des mots de passe et cycle de vie des
// jetons de session.
package auth

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"fmt"

	"golang.org/x/crypto/bcrypt"
)

// CookieName est le nom du cookie de session HTTP.
const CookieName = "silo_session"

// Bornes de longueur d'un mot de passe, partagées par l'API HTTP et la
// sous-commande CLI de réinitialisation afin qu'elles ne divergent pas.
const (
	MinPasswordLength = 8
	MaxPasswordLength = 256
)

// ValidatePassword vérifie qu'un mot de passe respecte les contraintes de
// longueur, et retourne une erreur explicite sinon.
func ValidatePassword(password string) error {
	if len(password) < MinPasswordLength {
		return fmt.Errorf("mot de passe trop court (%d caractères minimum)", MinPasswordLength)
	}
	if len(password) > MaxPasswordLength {
		return fmt.Errorf("mot de passe trop long (%d caractères maximum)", MaxPasswordLength)
	}
	return nil
}

// HashPassword dérive un hash bcrypt à partir d'un mot de passe en clair.
func HashPassword(password string) (string, error) {
	hash, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	if err != nil {
		return "", fmt.Errorf("auth: hashage mot de passe: %w", err)
	}
	return string(hash), nil
}

// VerifyPassword compare un mot de passe en clair à un hash bcrypt.
func VerifyPassword(hash, password string) bool {
	return bcrypt.CompareHashAndPassword([]byte(hash), []byte(password)) == nil
}

// NewSessionToken génère un jeton de session aléatoire (256 bits) encodé
// en base64 URL-safe, à transmettre au client via cookie.
func NewSessionToken() (string, error) {
	buf := make([]byte, 32)
	if _, err := rand.Read(buf); err != nil {
		return "", fmt.Errorf("auth: génération jeton: %w", err)
	}
	return base64.RawURLEncoding.EncodeToString(buf), nil
}

// HashToken calcule le SHA-256 d'un jeton de session, seule forme
// persistée en base (le jeton en clair ne quitte jamais le cookie client).
func HashToken(token string) string {
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:])
}
