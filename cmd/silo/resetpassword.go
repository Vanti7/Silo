package main

import (
	"bufio"
	"errors"
	"fmt"
	"os"
	"strings"

	"golang.org/x/term"

	"silo/internal/auth"
	"silo/internal/config"
	"silo/internal/store"
)

// runResetPassword réinitialise le mot de passe d'un compte depuis l'hôte.
// C'est le chemin de récupération prévu en cas de perte du mot de passe
// administrateur : disposer d'un accès shell sur le NAS tient lieu de
// preuve de propriété, aucun flux de réinitialisation non authentifié
// n'étant exposé côté web.
func runResetPassword(args []string) error {
	if len(args) != 1 {
		return errors.New("usage: silo reset-password <utilisateur>")
	}
	username := args[0]

	cfg, err := config.Load()
	if err != nil {
		return err
	}

	// La base tolère l'accès concurrent (busy_timeout) : inutile d'arrêter
	// le service pour réinitialiser un mot de passe.
	st, err := store.Open(cfg.DataDir)
	if err != nil {
		return err
	}
	defer st.Close()

	user, err := st.GetUserByUsername(username)
	if errors.Is(err, store.ErrNotFound) {
		return fmt.Errorf("aucun compte nommé %q dans %s", username, cfg.DataDir)
	}
	if err != nil {
		return err
	}

	password, err := readNewPassword()
	if err != nil {
		return err
	}

	hash, err := auth.HashPassword(password)
	if err != nil {
		return err
	}
	if err := st.UpdateUserPassword(user.ID, hash); err != nil {
		return err
	}
	// Cohérent avec le changement de mot de passe via l'UI : les sessions
	// ouvertes avec l'ancien mot de passe sont invalidées.
	if err := st.DeleteUserSessions(user.ID); err != nil {
		return err
	}

	fmt.Printf("Mot de passe de %q réinitialisé. Les sessions ouvertes ont été révoquées.\n", username)
	return nil
}

// readNewPassword demande le nouveau mot de passe deux fois. La saisie est
// masquée quand l'entrée standard est un terminal ; sinon (entrée
// redirigée, usage scripté) une simple ligne est lue.
func readNewPassword() (string, error) {
	if !term.IsTerminal(int(os.Stdin.Fd())) {
		reader := bufio.NewReader(os.Stdin)
		line, err := reader.ReadString('\n')
		if err != nil && line == "" {
			return "", fmt.Errorf("lecture du mot de passe: %w", err)
		}
		password := strings.TrimRight(line, "\r\n")
		if err := auth.ValidatePassword(password); err != nil {
			return "", err
		}
		return password, nil
	}

	fmt.Print("Nouveau mot de passe : ")
	first, err := term.ReadPassword(int(os.Stdin.Fd()))
	fmt.Println()
	if err != nil {
		return "", fmt.Errorf("lecture du mot de passe: %w", err)
	}

	fmt.Print("Confirmer le mot de passe : ")
	second, err := term.ReadPassword(int(os.Stdin.Fd()))
	fmt.Println()
	if err != nil {
		return "", fmt.Errorf("lecture du mot de passe: %w", err)
	}

	if string(first) != string(second) {
		return "", errors.New("les deux saisies ne correspondent pas")
	}
	password := string(first)
	if err := auth.ValidatePassword(password); err != nil {
		return "", err
	}
	return password, nil
}
