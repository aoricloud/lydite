//! Le plafond de connexions servies en même temps. Le garde rend sa place en sortant, même sur panique.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Le nombre de connexions servies en parallèle par le filtre et par le relais.
pub const MAX: usize = 64;

/// Un compteur de places. Il n'est pas cloné : le filtre et le relais en tiennent chacun un.
pub struct Plafond {
    max: usize,
    courant: Arc<AtomicUsize>,
}

impl Plafond {
    pub fn nouveau(max: usize) -> Self {
        Self { max, courant: Arc::new(AtomicUsize::new(0)) }
    }

    /// Prend une place, ou rend `None` quand le plafond est atteint. Le garde rend la place en tombant.
    pub fn prendre(&self) -> Option<Garde> {
        let pris = self.courant.fetch_add(1, Ordering::SeqCst);
        if pris >= self.max {
            self.courant.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Garde { courant: Arc::clone(&self.courant) })
    }
}

/// Une place prise ; elle est rendue à la fin du fil qui la tient.
pub struct Garde {
    courant: Arc<AtomicUsize>,
}

impl Drop for Garde {
    fn drop(&mut self) {
        self.courant.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn le_plafond_refuse_la_connexion_de_trop_et_la_rend() {
        let plafond = Plafond::nouveau(2);
        let premier = plafond.prendre().unwrap();
        let second = plafond.prendre().unwrap();
        assert!(plafond.prendre().is_none(), "la troisième doit être refusée");
        drop(premier);
        assert!(plafond.prendre().is_some(), "une place rendue doit resservir");
        drop(second);
    }
}
