package files

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestToFSPath(t *testing.T) {
	cases := map[string]string{
		"":              ".",
		"/":             ".",
		"/foo":          "foo",
		"foo/bar":       "foo/bar",
		"/foo/../../..": ".",
		"/../../etc":    "etc",
		"/a/../b":       "b",
	}
	for in, want := range cases {
		if got := toFSPath(in); got != want {
			t.Errorf("toFSPath(%q) = %q, attendu %q", in, got, want)
		}
	}
}

func newTestManager(t *testing.T) (*Manager, string) {
	t.Helper()
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "exemple.txt"), []byte("contenu"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(filepath.Join(dir, "sous"), 0o755); err != nil {
		t.Fatal(err)
	}
	m, err := NewManager(map[string]string{"test": dir})
	if err != nil {
		t.Fatalf("NewManager: %v", err)
	}
	t.Cleanup(func() { _ = m.Close() })
	return m, dir
}

func TestList(t *testing.T) {
	m, _ := newTestManager(t)
	entries, err := m.List("test", "/")
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(entries) != 2 {
		t.Fatalf("nombre d'entrées = %d, attendu 2", len(entries))
	}
	// Les dossiers doivent être triés avant les fichiers.
	if !entries[0].IsDir || entries[0].Name != "sous" {
		t.Errorf("première entrée = %+v, attendu le dossier 'sous'", entries[0])
	}
}

func TestList_RacineInconnue(t *testing.T) {
	m, _ := newTestManager(t)
	if _, err := m.List("inconnue", "/"); err != ErrUnknownRoot {
		t.Errorf("erreur = %v, attendu ErrUnknownRoot", err)
	}
}

func TestOpen_TraverseeChemin(t *testing.T) {
	m, dir := newTestManager(t)
	parent := filepath.Dir(dir)
	secretName := "secret-hors-racine.txt"
	secretPath := filepath.Join(parent, secretName)
	if err := os.WriteFile(secretPath, []byte("ne doit pas être lisible"), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Remove(secretPath) })

	// toFSPath neutralise "../.." avant même d'atteindre le système de
	// fichiers : le fichier hors racine doit rester inatteignable, quel
	// que soit le nombre de "../" utilisés pour tenter de s'en approcher.
	if _, _, err := m.Open("test", "/../../"+secretName); err == nil {
		t.Fatal("le fichier hors racine ne devrait pas être accessible")
	}
}

func TestWriteFileThenOpen(t *testing.T) {
	m, _ := newTestManager(t)
	if err := m.WriteFile("test", "/nouveau.txt", strings.NewReader("bonjour")); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	f, info, err := m.Open("test", "/nouveau.txt")
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	defer f.Close()
	if info.Size() != int64(len("bonjour")) {
		t.Errorf("taille = %d, attendu %d", info.Size(), len("bonjour"))
	}
}

func TestMkdirAndRemove(t *testing.T) {
	m, _ := newTestManager(t)
	if err := m.Mkdir("test", "/a/b/c"); err != nil {
		t.Fatalf("Mkdir: %v", err)
	}
	entries, err := m.List("test", "/a/b")
	if err != nil || len(entries) != 1 || entries[0].Name != "c" {
		t.Fatalf("List après Mkdir: entries=%v err=%v", entries, err)
	}

	if err := m.Remove("test", "/a"); err != nil {
		t.Fatalf("Remove: %v", err)
	}
	if _, err := m.List("test", "/a"); err == nil {
		t.Error("le dossier supprimé ne devrait plus exister")
	}
}

func TestRename(t *testing.T) {
	m, _ := newTestManager(t)
	if err := m.Rename("test", "/exemple.txt", "/renomme.txt"); err != nil {
		t.Fatalf("Rename: %v", err)
	}
	if _, _, err := m.Open("test", "/renomme.txt"); err != nil {
		t.Errorf("le fichier renommé devrait être accessible: %v", err)
	}
	if _, _, err := m.Open("test", "/exemple.txt"); err == nil {
		t.Error("l'ancien nom ne devrait plus exister")
	}
}
