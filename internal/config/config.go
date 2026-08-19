// Package config centralise la configuration de silo, chargée uniquement
// depuis des variables d'environnement (12-factor).
package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
)

// Config regroupe tous les paramètres de configuration du service.
type Config struct {
	// ListenAddr est l'adresse d'écoute HTTP (ex: ":8080").
	ListenAddr string
	// DataDir contient la base SQLite et les données internes de l'app.
	DataDir string
	// ShareRoots liste les racines de partage exposées par l'explorateur
	// de fichiers, sous la forme "nom=chemin" séparées par des virgules.
	ShareRoots map[string]string
	// SessionCookieSecure force l'attribut Secure sur le cookie de session.
	SessionCookieSecure bool
	// SessionTTLHours est la durée de vie d'une session en heures.
	SessionTTLHours int
	// DockerHost est l'hôte du daemon Docker (vide = valeur par défaut du SDK).
	DockerHost string
}

// Load lit la configuration depuis les variables d'environnement et
// applique des valeurs par défaut raisonnables pour un déploiement NAS.
func Load() (*Config, error) {
	cfg := &Config{
		ListenAddr:          getEnv("SILO_LISTEN_ADDR", ":8080"),
		DataDir:             getEnv("SILO_DATA_DIR", "/var/lib/silo"),
		SessionCookieSecure: getEnvBool("SILO_COOKIE_SECURE", true),
		SessionTTLHours:     getEnvInt("SILO_SESSION_TTL_HOURS", 168),
		DockerHost:          os.Getenv("SILO_DOCKER_HOST"),
	}

	roots, err := parseShareRoots(os.Getenv("SILO_SHARE_ROOTS"))
	if err != nil {
		return nil, fmt.Errorf("config: SILO_SHARE_ROOTS invalide: %w", err)
	}
	cfg.ShareRoots = roots

	return cfg, nil
}

// parseShareRoots parse "nom=chemin,nom2=chemin2" en map nom->chemin absolu.
func parseShareRoots(raw string) (map[string]string, error) {
	roots := make(map[string]string)
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return roots, nil
	}
	for _, entry := range strings.Split(raw, ",") {
		entry = strings.TrimSpace(entry)
		if entry == "" {
			continue
		}
		name, path, ok := strings.Cut(entry, "=")
		if !ok || strings.TrimSpace(name) == "" || strings.TrimSpace(path) == "" {
			return nil, fmt.Errorf("entrée %q invalide, format attendu nom=chemin", entry)
		}
		roots[strings.TrimSpace(name)] = strings.TrimSpace(path)
	}
	return roots, nil
}

func getEnv(key, fallback string) string {
	if v, ok := os.LookupEnv(key); ok && v != "" {
		return v
	}
	return fallback
}

func getEnvBool(key string, fallback bool) bool {
	v, ok := os.LookupEnv(key)
	if !ok || v == "" {
		return fallback
	}
	b, err := strconv.ParseBool(v)
	if err != nil {
		return fallback
	}
	return b
}

func getEnvInt(key string, fallback int) int {
	v, ok := os.LookupEnv(key)
	if !ok || v == "" {
		return fallback
	}
	i, err := strconv.Atoi(v)
	if err != nil {
		return fallback
	}
	return i
}
