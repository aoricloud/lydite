//! Le journal du filtre : une ligne par décision, rien du contenu des requêtes.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

/// Le journal, ouvert en ajout. Absent (`None`) quand `--journal` n'est pas donné.
pub struct Journal {
    fichier: Option<Mutex<File>>,
}

impl Journal {
    pub fn nouveau(chemin: Option<&Path>) -> io::Result<Self> {
        let fichier = match chemin {
            Some(chemin) => Some(Mutex::new(OpenOptions::new().create(true).append(true).open(chemin)?)),
            None => None,
        };
        Ok(Self { fichier })
    }

    /// Écrit une ligne déjà formée. Une erreur d'écriture ne coupe pas le proxy : le journal n'est pas la décision.
    pub fn noter(&self, ligne: &str) {
        if let Some(fichier) = &self.fichier
            && let Ok(mut fichier) = fichier.lock()
        {
            let _ = writeln!(fichier, "{ligne}");
            let _ = fichier.flush();
        }
    }
}
