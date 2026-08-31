// Command silo démarre le serveur HTTP de l'interface d'administration
// du NAS (API + frontend embarqué).
package main

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"silo/internal/api"
	"silo/internal/config"
	"silo/internal/docker"
	"silo/internal/files"
	"silo/internal/store"
	"silo/web"
)

func main() {
	// Sans sous-commande, silo démarre le serveur : c'est le mode utilisé
	// par l'unité systemd.
	if len(os.Args) > 1 {
		if err := runSubcommand(os.Args[1], os.Args[2:]); err != nil {
			fmt.Fprintln(os.Stderr, "erreur :", err)
			os.Exit(1)
		}
		return
	}

	logger := slog.New(slog.NewTextHandler(os.Stdout, nil))

	if err := run(logger); err != nil {
		logger.Error("arrêt sur erreur fatale", "err", err)
		os.Exit(1)
	}
}

func runSubcommand(name string, args []string) error {
	switch name {
	case "reset-password":
		return runResetPassword(args)
	case "-h", "--help", "help":
		printUsage()
		return nil
	default:
		printUsage()
		return fmt.Errorf("sous-commande inconnue : %q", name)
	}
}

func printUsage() {
	fmt.Fprint(os.Stderr, `silo - interface d'administration du NAS

Usage :
  silo                              démarre le serveur HTTP
  silo reset-password <utilisateur> réinitialise le mot de passe d'un compte

La configuration est lue depuis les variables d'environnement (voir README).
`)
}

func run(logger *slog.Logger) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}

	st, err := store.Open(cfg.DataDir)
	if err != nil {
		return err
	}
	defer st.Close()

	dockerClient, err := docker.New(cfg.DockerHost)
	if err != nil {
		return err
	}
	defer dockerClient.Close()

	filesManager, err := files.NewManager(cfg.ShareRoots)
	if err != nil {
		return err
	}
	defer filesManager.Close()

	server := &api.Server{
		Config: cfg,
		Store:  st,
		Docker: dockerClient,
		Files:  filesManager,
		Logger: logger,
	}
	handler := api.NewRouter(server, web.DistFS())

	go expireSessionsPeriodically(st, logger)

	httpServer := &http.Server{
		Addr:              cfg.ListenAddr,
		Handler:           handler,
		ReadHeaderTimeout: 10 * time.Second,
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	serveErr := make(chan error, 1)
	go func() {
		logger.Info("démarrage du serveur", "addr", cfg.ListenAddr)
		serveErr <- httpServer.ListenAndServe()
	}()

	select {
	case err := <-serveErr:
		if errors.Is(err, http.ErrServerClosed) {
			return nil
		}
		return err
	case <-ctx.Done():
		logger.Info("arrêt demandé, fermeture propre du serveur")
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		return httpServer.Shutdown(shutdownCtx)
	}
}

// expireSessionsPeriodically purge les sessions expirées à intervalle
// régulier pour éviter la croissance illimitée de la table sessions.
func expireSessionsPeriodically(st *store.Store, logger *slog.Logger) {
	ticker := time.NewTicker(time.Hour)
	defer ticker.Stop()
	for range ticker.C {
		if err := st.DeleteExpiredSessions(); err != nil {
			logger.Warn("purge des sessions expirées", "err", err)
		}
	}
}
