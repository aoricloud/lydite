//! Les baux des mains d'une vague : les trois règles qui tiennent chaque main, relance comprise. Pur et sans horloge à
//! lui : chaque instant vient de la vague, qui lit la sienne.
//!
//! - Une tâche n'a jamais deux mains actives. [`Baux::ouvrir`] ne donne un bail qu'à une tâche sans bail ouvert ni
//!   expiré, sinon le refus 7002 : une relance ne part qu'après le message de la tentative d'avant, et une main expirée
//!   ne se relance jamais, car elle peut tourner encore.
//! - Un résultat périmé est refusé. Chaque bail porte sa tentative, le jeton qu'`ouvrir` rend et que le résultat de sa
//!   main porte ; [`Baux::conclure`] ne prend que le bail ouvert de cette tentative, sinon le refus 7003.
//! - Une main jamais conclue expire seule. Chaque bail a la durée passée à son ouverture ; [`Baux::expirer`] ferme et
//!   rend ceux qui atteignent leur échéance sans message.
//!
//! Les règles portent sur les mains que Kraken lance, pas sur les sous-agents qu'une main lance dans son bac.

use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};

use crate::coeur::erreur::{Code, Refus};

/// Les baux d'une vague : pour chaque main, celui de sa dernière tentative.
#[derive(Debug)]
pub(crate) struct Baux {
    baux: BTreeMap<usize, Bail>,
}

/// Le bail d'une tentative : son numéro, le jeton, et où il en est.
#[derive(Debug)]
struct Bail {
    tentative: usize,
    etat: Etat,
}

/// Où en est un bail.
#[derive(Debug)]
enum Etat {
    /// La main de la tentative court depuis `depuis`, et son échéance tombe quand `duree` s'est écoulée. L'écoulé se
    /// compare à la durée, sans addition : une durée qu'aucun instant n'atteint ne déborde pas, et n'échoit jamais.
    Ouvert { depuis: Instant, duree: Duration },
    /// La main a rendu son message : la tâche peut repartir.
    Conclu,
    /// La main n'avait rien rendu à l'échéance : elle peut tourner encore, et la tâche ne repart jamais.
    Expire,
}

impl Baux {
    pub(crate) fn new() -> Baux {
        Baux { baux: BTreeMap::new() }
    }

    /// Ouvre le bail de la tentative suivante de la main, pour `duree` à partir de `maintenant`, et rend cette
    /// tentative : le jeton que portera son résultat, 1 puis un de plus à chaque relance. Le refus 7002, qui ne change
    /// rien, si le bail de la main est ouvert (sa tentative court) ou expiré (sa main peut tourner encore).
    pub(crate) fn ouvrir(&mut self, main: usize, maintenant: Instant, duree: Duration) -> Result<usize, Refus> {
        let tentative = match self.baux.get(&main) {
            None => 1,
            Some(Bail { tentative, etat: Etat::Conclu }) => tentative + 1,
            Some(bail @ Bail { etat: Etat::Ouvert { .. }, .. }) => {
                let attendue = bail.tentative;
                return Err(Refus::new(
                    Code::MainActive,
                    format!("main {main} : {bail} ; une seconde main doublerait la sienne"),
                    format!("Attends le message de sa tentative {attendue} : une relance ne part qu'après lui."),
                ));
            }
            Some(bail @ Bail { etat: Etat::Expire, .. }) => {
                return Err(Refus::new(
                    Code::MainActive,
                    format!("main {main} : {bail} ; sa main peut tourner encore"),
                    "Ne relance pas cette tâche, refusée à son expiration : reprends-la dans une vague neuve.",
                ));
            }
        };
        self.baux.insert(main, Bail { tentative, etat: Etat::Ouvert { depuis: maintenant, duree } });
        Ok(tentative)
    }

    /// Conclut le bail de cette tentative de la main, s'il est ouvert : son résultat se prend, et la tâche peut
    /// repartir. Tout autre résultat est périmé, le refus 7003, qui ne change rien : une main sans bail, un bail déjà
    /// conclu ou expiré, ou le bail d'une autre tentative.
    pub(crate) fn conclure(&mut self, main: usize, tentative: usize) -> Result<(), Refus> {
        match self.baux.get_mut(&main) {
            Some(bail) if bail.tentative == tentative && matches!(bail.etat, Etat::Ouvert { .. }) => {
                bail.etat = Etat::Conclu;
                Ok(())
            }
            bail => {
                let etat =
                    bail.map_or_else(|| "aucun bail n'a été ouvert pour cette main".into(), |bail| bail.to_string());
                Err(Refus::new(
                    Code::ResultatPerime,
                    format!("main {main}, tentative {tentative} : {etat}"),
                    "Ne prends pas ce résultat : note-le en main.perimee, sans le figer ni l'assembler.",
                ))
            }
        }
    }

    /// Ferme les baux ouverts qui ont atteint leur échéance à `maintenant`, et les rend en (main, tentative), dans
    /// l'ordre des mains : ces mains n'ont rien rendu, et elles expirent. Un bail ne se ferme qu'une fois.
    pub(crate) fn expirer(&mut self, maintenant: Instant) -> Vec<(usize, usize)> {
        let mut echus = Vec::new();
        for (main, bail) in &mut self.baux {
            if let Etat::Ouvert { depuis, duree } = bail.etat
                && maintenant.saturating_duration_since(depuis) >= duree
            {
                bail.etat = Etat::Expire;
                echus.push((*main, bail.tentative));
            }
        }
        echus
    }
}

/// L'état du bail, pour la preuve d'un refus : « le bail de sa tentative 2 est ouvert, pour 3960 s ».
impl fmt::Display for Bail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tentative = self.tentative;
        match self.etat {
            Etat::Ouvert { duree, .. } => {
                write!(f, "le bail de sa tentative {tentative} est ouvert, pour {} s", duree.as_secs_f64())
            }
            Etat::Conclu => write!(f, "le bail de sa tentative {tentative} est conclu"),
            Etat::Expire => write!(f, "le bail de sa tentative {tentative} a expiré, sans message à son échéance"),
        }
    }
}

#[cfg(test)]
mod tests;
