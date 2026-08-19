// Package files fournit un explorateur de fichiers confiné aux racines de
// partage configurées. Chaque racine est ouverte via os.Root (Go 1.24+),
// qui garantit au niveau de l'OS qu'aucune opération ne peut échapper au
// répertoire racine (y compris via des liens symboliques), ce qui exclut
// les attaques par traversée de chemin (../..).
package files

import (
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"sort"
	"strings"
)

// ErrUnknownRoot est retourné quand la racine demandée n'est pas configurée.
var ErrUnknownRoot = fmt.Errorf("files: racine de partage inconnue")

// Entry décrit un fichier ou dossier listé.
type Entry struct {
	Name      string `json:"name"`
	IsDir     bool   `json:"isDir"`
	SizeBytes int64  `json:"sizeBytes"`
	ModTime   string `json:"modTime"`
}

// Manager donne accès aux racines de partage configurées.
type Manager struct {
	roots map[string]*os.Root
}

// NewManager ouvre chaque racine de partage listée dans roots (nom -> chemin
// absolu sur le système hôte).
func NewManager(roots map[string]string) (*Manager, error) {
	m := &Manager{roots: make(map[string]*os.Root, len(roots))}
	for name, p := range roots {
		r, err := os.OpenRoot(p)
		if err != nil {
			return nil, fmt.Errorf("files: ouverture racine %q (%s): %w", name, p, err)
		}
		m.roots[name] = r
	}
	return m, nil
}

// Close ferme toutes les racines ouvertes.
func (m *Manager) Close() error {
	var firstErr error
	for _, r := range m.roots {
		if err := r.Close(); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	return firstErr
}

// Roots retourne les noms des racines de partage configurées.
func (m *Manager) Roots() []string {
	names := make([]string, 0, len(m.roots))
	for name := range m.roots {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

func (m *Manager) root(name string) (*os.Root, error) {
	r, ok := m.roots[name]
	if !ok {
		return nil, ErrUnknownRoot
	}
	return r, nil
}

// toFSPath normalise un chemin fourni par le client (potentiellement
// "/", "", ou contenant des "..") vers le format attendu par fs.FS : "."
// pour la racine, jamais de "/" en tête. path.Clean neutralise les "..".
func toFSPath(p string) string {
	p = strings.TrimPrefix(p, "/")
	p = path.Clean("/" + p)
	p = strings.TrimPrefix(p, "/")
	if p == "" {
		p = "."
	}
	return p
}

// List énumère le contenu d'un dossier dans une racine de partage.
func (m *Manager) List(rootName, dir string) ([]Entry, error) {
	r, err := m.root(rootName)
	if err != nil {
		return nil, err
	}
	fsPath := toFSPath(dir)

	entries, err := fs.ReadDir(r.FS(), fsPath)
	if err != nil {
		return nil, fmt.Errorf("files: lecture dossier %q: %w", dir, err)
	}

	out := make([]Entry, 0, len(entries))
	for _, e := range entries {
		info, err := e.Info()
		if err != nil {
			continue
		}
		out = append(out, Entry{
			Name:      e.Name(),
			IsDir:     e.IsDir(),
			SizeBytes: info.Size(),
			ModTime:   info.ModTime().UTC().Format("2006-01-02T15:04:05Z"),
		})
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].IsDir != out[j].IsDir {
			return out[i].IsDir
		}
		return out[i].Name < out[j].Name
	})
	return out, nil
}

// Open ouvre un fichier en lecture pour téléchargement/streaming.
// L'appelant doit fermer le fichier retourné.
func (m *Manager) Open(rootName, filePath string) (*os.File, os.FileInfo, error) {
	r, err := m.root(rootName)
	if err != nil {
		return nil, nil, err
	}
	fsPath := toFSPath(filePath)
	f, err := r.Open(fsPath)
	if err != nil {
		return nil, nil, fmt.Errorf("files: ouverture fichier %q: %w", filePath, err)
	}
	info, err := f.Stat()
	if err != nil {
		_ = f.Close()
		return nil, nil, fmt.Errorf("files: stat fichier %q: %w", filePath, err)
	}
	return f, info, nil
}

// WriteFile crée ou remplace un fichier avec le contenu lu depuis src.
func (m *Manager) WriteFile(rootName, filePath string, src io.Reader) error {
	r, err := m.root(rootName)
	if err != nil {
		return err
	}
	fsPath := toFSPath(filePath)
	f, err := r.OpenFile(fsPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o644)
	if err != nil {
		return fmt.Errorf("files: création fichier %q: %w", filePath, err)
	}
	defer f.Close()
	if _, err := io.Copy(f, src); err != nil {
		return fmt.Errorf("files: écriture fichier %q: %w", filePath, err)
	}
	return nil
}

// Mkdir crée un dossier (et ses parents éventuels) dans une racine.
func (m *Manager) Mkdir(rootName, dirPath string) error {
	r, err := m.root(rootName)
	if err != nil {
		return err
	}
	if err := r.MkdirAll(toFSPath(dirPath), 0o755); err != nil {
		return fmt.Errorf("files: création dossier %q: %w", dirPath, err)
	}
	return nil
}

// Remove supprime un fichier ou un dossier (récursivement) dans une racine.
func (m *Manager) Remove(rootName, targetPath string) error {
	r, err := m.root(rootName)
	if err != nil {
		return err
	}
	if err := r.RemoveAll(toFSPath(targetPath)); err != nil {
		return fmt.Errorf("files: suppression %q: %w", targetPath, err)
	}
	return nil
}

// Rename renomme/déplace un fichier ou dossier au sein d'une même racine.
func (m *Manager) Rename(rootName, oldPath, newPath string) error {
	r, err := m.root(rootName)
	if err != nil {
		return err
	}
	if err := r.Rename(toFSPath(oldPath), toFSPath(newPath)); err != nil {
		return fmt.Errorf("files: renommage %q -> %q: %w", oldPath, newPath, err)
	}
	return nil
}
