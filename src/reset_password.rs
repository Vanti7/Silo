//! Sous-commande `silo reset-password <utilisateur>` : chemin de
//! récupération en cas de perte du mot de passe administrateur. L'accès
//! shell au NAS tient lieu de preuve de propriété, aucun flux de
//! réinitialisation non authentifié n'étant exposé côté web.

use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::{Context, bail};
use silo::auth;
use silo::config::Config;
use silo::store::{Store, StoreError};

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let [username] = args else {
        bail!("usage: silo reset-password <utilisateur>");
    };

    let config = Config::load()?;
    // La base tolère l'accès concurrent (busy_timeout) : inutile d'arrêter
    // le service pour réinitialiser un mot de passe.
    let store = Store::open(&config.data_dir)?;

    let user = match store.get_user_by_username(username) {
        Ok(u) => u,
        Err(StoreError::NotFound) => {
            bail!("aucun compte nommé {username:?} dans {}", config.data_dir)
        }
        Err(e) => return Err(e.into()),
    };

    let password = read_new_password()?;
    store.update_user_password(user.id, &auth::hash_password(&password)?)?;
    // Comme pour un changement via l'interface, les sessions ouvertes avec
    // l'ancien mot de passe sont invalidées.
    store.delete_user_sessions(user.id)?;

    println!("Mot de passe de {username:?} réinitialisé. Les sessions ouvertes ont été révoquées.");
    Ok(())
}

/// Demande le nouveau mot de passe deux fois, saisie masquée, quand l'entrée
/// standard est un terminal ; sinon (entrée redirigée, usage scripté) lit
/// une seule ligne.
fn read_new_password() -> anyhow::Result<String> {
    let stdin = io::stdin();
    if !stdin.is_terminal() {
        let mut line = String::new();
        let n = stdin
            .lock()
            .read_line(&mut line)
            .context("lecture du mot de passe")?;
        if n == 0 {
            bail!("lecture du mot de passe: entrée standard vide");
        }
        let password = line.trim_end_matches(['\r', '\n']).to_string();
        auth::validate_password(&password)?;
        return Ok(password);
    }

    let first = prompt_hidden("Nouveau mot de passe : ")?;
    let second = prompt_hidden("Confirmer le mot de passe : ")?;
    if first != second {
        bail!("les deux saisies ne correspondent pas");
    }
    auth::validate_password(&first)?;
    Ok(first)
}

fn prompt_hidden(prompt: &str) -> anyhow::Result<String> {
    print!("{prompt}");
    io::stdout().flush().context("affichage de l'invite")?;
    rpassword::read_password().context("lecture du mot de passe")
}
