package config

import (
	"reflect"
	"testing"
)

func TestParseShareRoots(t *testing.T) {
	got, err := parseShareRoots("pool=/mnt/pool, backup=/mnt/backup")
	if err != nil {
		t.Fatalf("erreur inattendue: %v", err)
	}
	want := map[string]string{"pool": "/mnt/pool", "backup": "/mnt/backup"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, attendu %v", got, want)
	}
}

func TestParseShareRoots_Vide(t *testing.T) {
	got, err := parseShareRoots("")
	if err != nil {
		t.Fatalf("erreur inattendue: %v", err)
	}
	if len(got) != 0 {
		t.Errorf("got %v, attendu vide", got)
	}
}

func TestParseShareRoots_EntreeInvalide(t *testing.T) {
	if _, err := parseShareRoots("pool-sans-egal"); err == nil {
		t.Error("attendu une erreur pour une entrée sans '='")
	}
}

func TestParseShareRoots_NomOuCheminVide(t *testing.T) {
	cases := []string{"=/mnt/pool", "pool=", "  =  "}
	for _, in := range cases {
		if _, err := parseShareRoots(in); err == nil {
			t.Errorf("parseShareRoots(%q) : attendu une erreur", in)
		}
	}
}
