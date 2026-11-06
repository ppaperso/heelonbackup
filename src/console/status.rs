//! Flux « statut » de la console (squelette phase 2 ; collecte des choix en phase 3)

use crate::error::Result;
use crate::service::Exit;

/// Stub : aucun appel à l'orchestration métier, on revient au menu
pub(crate) async fn run() -> Result<Exit> {
    println!("flux status : à implémenter en phase 3");
    Ok(Exit::Success)
}
