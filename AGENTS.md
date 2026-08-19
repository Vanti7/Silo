# AGENTS.md — Conventions de développement

> À placer à la racine du dépôt. Règles que tout agent IA doit respecter sur
> ce projet. Pour un monorepo, ajouter un AGENTS.md par sous-projet (le plus
> proche dans l'arborescence prend le pas). Adapter la section « Contexte
> projet » à chaque dépôt.

## 1. Principes généraux

- Répondre et rédiger en français (commentaires et docs inclus), sauf convention contraire du projet.
- En cas de doute ou d'ambiguïté, demander avant d'agir. Ne jamais inventer une API, une option ou un comportement.
- Modifications minimales et ciblées : pas de refactorisation, renommage ou réorganisation non demandés.
- Respecter les conventions existantes du code avant d'en imposer de nouvelles, même « meilleures ».
- Ne pas ajouter de fonctionnalité non demandée.
- Expliquer les choix non évidents dans la PR ou le commit, pas dans des commentaires qui paraphrasent le code.

## 2. Style de code

### Python

- PEP 8, formatage `black` (ou `ruff format`), lint `ruff`.
- Type hints sur les signatures publiques ; docstrings pour modules, classes et fonctions publiques.
- Nommage : `snake_case` (fonctions/variables), `PascalCase` (classes), `MAJUSCULES` (constantes).
- Fonctions courtes à responsabilité unique ; éviter plus de 2 niveaux d'imbrication.
- F-strings pour le formatage de chaînes.

### Bash

- `#!/usr/bin/env bash` + `set -euo pipefail` en tête de tout script.
- `shellcheck` propre ; toutes les expansions quotées (`"$var"`).
- Fonctions plutôt que script linéaire dès que la logique dépasse ~20 lignes.

### SQL

- Requêtes paramétrées uniquement — jamais d'interpolation de variables dans une requête.
- Mots-clés en majuscules, identifiants en `snake_case`.

### YAML / config

- `yamllint` propre.

### Règles transverses

- Pas de code mort ni de code commenté.
- Les commentaires expliquent le « pourquoi », jamais le « quoi ».

## 3. Architecture et dépendances

- Configuration par variables d'environnement (12-factor) ; aucune valeur d'environnement en dur dans le code.
- Pas de nouvelle dépendance sans justification explicite ; versions épinglées (`requirements.txt` / lockfile).
- Séparer logique métier, accès aux données et interface (routes/CLI) — pas de SQL dans les handlers HTTP.
- Une responsabilité par module ; pas de fichier fourre-tout.
- Ne pas casser les interfaces existantes (signatures, routes, formats) sans le signaler explicitement.

## 4. Erreurs et logs

- Exceptions spécifiques ; `except:` nu ou `except: pass` interdits.
- Messages d'erreur actionnables : quoi, où, cause probable.
- Logs structurés via le module `logging` ; pas de `print` hors scripts jetables.
- Jamais de secret, token ou donnée personnelle dans les logs ou les messages d'erreur.

## 5. Tests

- Toute nouvelle logique arrive avec ses tests (`pytest`) ; tout bugfix ajoute un test de non-régression.
- Les tests existants doivent passer avant tout commit ; ne jamais désactiver ou skipper un test sans accord.
- Tests isolés : pas de dépendance à l'ordre d'exécution, pas d'état partagé, pas d'accès réseau ou base réelle (mocks/fixtures).
- Nommage explicite : `test_<fonction>_<cas>_<résultat attendu>`.

## 6. Git et pull requests

- Branche dédiée par tâche ; jamais de commit direct sur `main`.
- Conventional Commits : `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:` — `!` ou footer `BREAKING CHANGE:` pour signaler une rupture de compatibilité.
- Commits atomiques : un changement logique par commit.
- Pas de code commenté, de `TODO` sans ticket, ni de fichier de debug dans un commit.
- PR petite et auto-portante : quoi, pourquoi, comment tester, type de bump proposé.

## 7. Versioning et changelog (SemVer)

- Versionnement sémantique obligatoire : `MAJEUR.MINEUR.CORRECTIF` (ex. 1.4.2).
  - MAJEUR : changement incompatible avec l'API ou le comportement public.
  - MINEUR : nouvelle fonctionnalité rétrocompatible.
  - CORRECTIF : bugfix rétrocompatible.
- Correspondance avec les commits : `fix:` → patch, `feat:` → minor, `BREAKING CHANGE` / `!` → major (quel que soit le type de commit).
- Resets : bump mineur → correctif remis à 0 ; bump majeur → mineur et correctif remis à 0.
- Toute PR qui change le comportement = entrée dans `CHANGELOG.md` + bump de version. Aucune exception.
- Format Keep a Changelog :
  - Section `## [Unreleased]` en tête de fichier, alimentée à chaque PR.
  - Une section par version publiée, la plus récente d'abord : `## [X.Y.Z] - AAAA-MM-JJ`.
  - Catégories : `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`.
  - Rédigé pour des humains : ce qui change pour l'utilisateur, jamais un dump de `git log`.
- Source de vérité unique pour la version : `pyproject.toml` (ou fichier `VERSION`). Jamais de numéro dupliqué en dur dans le code.
- Release = commit de bump + tag Git annoté `vX.Y.Z`.
- Avant la 1.0.0 (versions 0.y.z) : API instable tolérée, mais le changelog se tient quand même dès le départ.
- L'agent propose et justifie le type de bump dans la PR ; en cas de doute sur la rétrocompatibilité, il demande au lieu de trancher.

## 8. Documentation

- README à jour : installation, lancement, tests, variables d'environnement.
- Docstrings sur l'API publique.
- Toute décision d'architecture notable → ADR court dans `docs/adr/`.

## 9. Définition de « terminé »

Un changement n'est terminé que si :

1. Lint et formatage propres (`ruff`, `shellcheck`, `yamllint`).
2. Tests écrits et suite verte.
3. Docs/README à jour si le comportement change.
4. Aucun secret ni donnée sensible dans le diff.
5. PR ouverte avec description complète.
6. `CHANGELOG.md` mis à jour (section `[Unreleased]`) et bump SemVer appliqué.

## Contexte projet (à personnaliser)

- Langage(s) et versions : <Python 3.x, Bash, ...>
- Commandes : install `<...>`, tests `<...>`, lint `<...>`, run `<...>`
- Fichier de version : <pyproject.toml | VERSION>
- Conventions spécifiques : <framework, structure des dossiers, choix figés>
