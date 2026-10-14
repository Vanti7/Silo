//! Frontend compilé (`web/dist`, build Vite) embarqué dans le binaire pour
//! un déploiement en un seul exécutable. En build de debug, les fichiers
//! sont relus sur disque, ce qui évite de recompiler pour tester le frontend.

use rust_embed::RustEmbed;

/// Contenu de `web/dist`, sans le préfixe `dist`.
#[derive(RustEmbed)]
#[folder = "web/dist/"]
pub struct Dist;
