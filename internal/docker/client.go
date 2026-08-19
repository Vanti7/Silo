// Package docker encapsule le client Docker officiel pour exposer les
// opérations nécessaires à la gestion des containers depuis l'UI.
package docker

import (
	"context"
	"fmt"
	"io"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/image"
	"github.com/docker/docker/client"
)

// Client encapsule le client Docker SDK.
type Client struct {
	cli *client.Client
}

// New crée un client Docker. host vide utilise la configuration par défaut
// du SDK (variable DOCKER_HOST ou socket unix par défaut).
func New(host string) (*Client, error) {
	opts := []client.Opt{client.FromEnv, client.WithAPIVersionNegotiation()}
	if host != "" {
		opts = append(opts, client.WithHost(host))
	}
	cli, err := client.NewClientWithOpts(opts...)
	if err != nil {
		return nil, fmt.Errorf("docker: connexion au daemon: %w", err)
	}
	return &Client{cli: cli}, nil
}

// Close libère les ressources du client Docker.
func (c *Client) Close() error {
	return c.cli.Close()
}

// Container résume l'état d'un container pour l'affichage en liste.
type Container struct {
	ID      string            `json:"id"`
	Names   []string          `json:"names"`
	Image   string            `json:"image"`
	State   string            `json:"state"`
	Status  string            `json:"status"`
	Ports   []Port            `json:"ports"`
	Created int64             `json:"created"`
	Labels  map[string]string `json:"labels"`
}

// Port décrit un mappage de port publié par un container.
type Port struct {
	PrivatePort uint16 `json:"privatePort"`
	PublicPort  uint16 `json:"publicPort"`
	Type        string `json:"type"`
}

// ListContainers retourne tous les containers (démarrés ou arrêtés).
func (c *Client) ListContainers(ctx context.Context) ([]Container, error) {
	list, err := c.cli.ContainerList(ctx, container.ListOptions{All: true})
	if err != nil {
		return nil, fmt.Errorf("docker: liste containers: %w", err)
	}

	out := make([]Container, 0, len(list))
	for _, item := range list {
		ports := make([]Port, 0, len(item.Ports))
		for _, p := range item.Ports {
			ports = append(ports, Port{PrivatePort: p.PrivatePort, PublicPort: p.PublicPort, Type: p.Type})
		}
		out = append(out, Container{
			ID:      item.ID,
			Names:   item.Names,
			Image:   item.Image,
			State:   item.State,
			Status:  item.Status,
			Ports:   ports,
			Created: item.Created,
			Labels:  item.Labels,
		})
	}
	return out, nil
}

// StartContainer démarre un container par son ID.
func (c *Client) StartContainer(ctx context.Context, id string) error {
	if err := c.cli.ContainerStart(ctx, id, container.StartOptions{}); err != nil {
		return fmt.Errorf("docker: démarrage container %s: %w", id, err)
	}
	return nil
}

// StopContainer arrête un container par son ID (arrêt propre avec délai
// de grâce géré par le daemon Docker).
func (c *Client) StopContainer(ctx context.Context, id string) error {
	if err := c.cli.ContainerStop(ctx, id, container.StopOptions{}); err != nil {
		return fmt.Errorf("docker: arrêt container %s: %w", id, err)
	}
	return nil
}

// RestartContainer redémarre un container par son ID.
func (c *Client) RestartContainer(ctx context.Context, id string) error {
	if err := c.cli.ContainerRestart(ctx, id, container.StopOptions{}); err != nil {
		return fmt.Errorf("docker: redémarrage container %s: %w", id, err)
	}
	return nil
}

// RemoveContainer supprime un container par son ID. Le container doit être
// arrêté au préalable, sauf si force est vrai.
func (c *Client) RemoveContainer(ctx context.Context, id string, force bool) error {
	if err := c.cli.ContainerRemove(ctx, id, container.RemoveOptions{Force: force}); err != nil {
		return fmt.Errorf("docker: suppression container %s: %w", id, err)
	}
	return nil
}

// ContainerLogs retourne un flux des logs (stdout+stderr) d'un container.
// L'appelant est responsable de fermer le ReadCloser retourné.
func (c *Client) ContainerLogs(ctx context.Context, id string, follow bool, tail string) (io.ReadCloser, error) {
	rc, err := c.cli.ContainerLogs(ctx, id, container.LogsOptions{
		ShowStdout: true,
		ShowStderr: true,
		Follow:     follow,
		Tail:       tail,
		Timestamps: true,
	})
	if err != nil {
		return nil, fmt.Errorf("docker: logs container %s: %w", id, err)
	}
	return rc, nil
}

// Image résume une image Docker locale.
type Image struct {
	ID        string   `json:"id"`
	Tags      []string `json:"tags"`
	SizeBytes int64    `json:"sizeBytes"`
	Created   int64    `json:"created"`
}

// ListImages retourne les images Docker présentes localement.
func (c *Client) ListImages(ctx context.Context) ([]Image, error) {
	list, err := c.cli.ImageList(ctx, image.ListOptions{})
	if err != nil {
		return nil, fmt.Errorf("docker: liste images: %w", err)
	}
	out := make([]Image, 0, len(list))
	for _, item := range list {
		out = append(out, Image{ID: item.ID, Tags: item.RepoTags, SizeBytes: item.Size, Created: item.Created})
	}
	return out, nil
}

// Info résume les informations du daemon Docker (version, nombre de
// containers, driver de stockage).
type Info struct {
	ServerVersion     string `json:"serverVersion"`
	ContainersRunning int    `json:"containersRunning"`
	ContainersStopped int    `json:"containersStopped"`
	Images            int    `json:"images"`
	StorageDriver     string `json:"storageDriver"`
}

// GetInfo retourne un résumé de l'état du daemon Docker.
func (c *Client) GetInfo(ctx context.Context) (*Info, error) {
	info, err := c.cli.Info(ctx)
	if err != nil {
		return nil, fmt.Errorf("docker: info daemon: %w", err)
	}
	return &Info{
		ServerVersion:     info.ServerVersion,
		ContainersRunning: info.ContainersRunning,
		ContainersStopped: info.ContainersStopped,
		Images:            info.Images,
		StorageDriver:     info.Driver,
	}, nil
}
