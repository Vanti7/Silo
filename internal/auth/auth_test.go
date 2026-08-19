package auth

import "testing"

func TestHashAndVerifyPassword(t *testing.T) {
	hash, err := HashPassword("mot-de-passe-correct")
	if err != nil {
		t.Fatalf("HashPassword: %v", err)
	}
	if !VerifyPassword(hash, "mot-de-passe-correct") {
		t.Error("VerifyPassword devrait accepter le bon mot de passe")
	}
	if VerifyPassword(hash, "mauvais-mot-de-passe") {
		t.Error("VerifyPassword ne devrait pas accepter un mauvais mot de passe")
	}
}

func TestNewSessionToken_UniqueEtNonVide(t *testing.T) {
	a, err := NewSessionToken()
	if err != nil {
		t.Fatalf("NewSessionToken: %v", err)
	}
	b, err := NewSessionToken()
	if err != nil {
		t.Fatalf("NewSessionToken: %v", err)
	}
	if a == "" || b == "" {
		t.Fatal("un jeton de session ne devrait jamais être vide")
	}
	if a == b {
		t.Error("deux jetons générés successivement ne devraient pas être identiques")
	}
}

func TestHashToken_Deterministe(t *testing.T) {
	if HashToken("abc") != HashToken("abc") {
		t.Error("HashToken devrait être déterministe pour une même entrée")
	}
	if HashToken("abc") == HashToken("def") {
		t.Error("HashToken ne devrait pas produire la même empreinte pour des entrées différentes")
	}
}
