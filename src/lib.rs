//! Kraken : la bibliothèque et la commande.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod branchements {
    pub(crate) mod bac;
    pub(crate) mod claude;
    pub(crate) mod dossier;
    pub(crate) mod git;
    pub(crate) mod github;
    pub(crate) mod processus;
    pub(crate) mod srt;
}

mod coeur {
    pub(crate) mod bail;
    pub(crate) mod empreinte;
    pub(crate) mod erreur;
    pub(crate) mod fil;
    pub(crate) mod json;
    pub(crate) mod plan;
    pub(crate) mod profil;
    pub(crate) mod protocole;
    pub(crate) mod remarques;
}

mod commande;
mod conduite {
    pub(crate) mod assemblage;
    pub(crate) mod decoupage;
    pub(crate) mod lancer;
    pub(crate) mod mener;
    pub(crate) mod publication;
    pub(crate) mod relecture;
    pub(crate) mod socle;
    pub(crate) mod tentative;
}
#[cfg(test)]
mod essais;

pub use commande::principal;
