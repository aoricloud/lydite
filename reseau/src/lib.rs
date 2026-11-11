#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
//! Filtre réseau et relais du mur natif de Lydite.
//!
//! Le bac n'a pas de réseau : un programme sort par un relais local (`relais`), qui parle à un socket Unix, derrière
//! lequel le filtre (`proxy`) décide par nom de domaine. La décision est une fonction pure ; les sockets et le
//! résolveur sont injectés.

pub mod adresses;
pub mod connexions;
pub mod decision;
pub mod flux;
pub mod journal;
pub mod proxy;
pub mod relais;

#[cfg(unix)]
pub mod filtre;

pub use decision::{Refus, Verdict, decider};
pub use flux::{Flux, tunnel};
pub use journal::Journal;
pub use proxy::{DELAI, Decideur, servir};
pub use relais::{relais, relais_sur};
