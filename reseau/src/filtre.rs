//! L'écoute Unix du filtre, hors du bac. Un plafond borne les connexions ; l'entrée standard fermée l'arrête.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::connexions::Plafond;
use crate::journal::Journal;
use crate::proxy::{DELAI, Decideur, servir};

/// Retire le socket en tombant, même quand le filtre sort par une erreur (écriture de `ready`, mode non bloquant).
struct GardeSocket(PathBuf);

impl Drop for GardeSocket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Crée le socket, écrit `ready`, sert les connexions ; à la fermeture de l'entrée standard, retire le socket et sort.
pub fn filtre(socket: &Path, decideur: Decideur, journal: Journal, plafond_max: usize) -> io::Result<()> {
    if socket.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("socket already exists: {}", socket.display()),
        ));
    }
    let listener = UnixListener::bind(socket)?;
    let garde = GardeSocket(socket.to_path_buf());
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    println!("ready");
    io::stdout().flush()?;

    let arret = Arc::new(AtomicBool::new(false));
    let arret_entree = Arc::clone(&arret);
    thread::spawn(move || {
        let mut entree = io::stdin();
        let mut tampon = [0u8; 64];
        loop {
            match entree.read(&mut tampon) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        arret_entree.store(true, Ordering::SeqCst);
    });

    let decideur = Arc::new(decideur);
    let journal = Arc::new(journal);
    let plafond = Plafond::nouveau(plafond_max);
    loop {
        if arret.load(Ordering::SeqCst) {
            break;
        }
        match listener.accept() {
            Ok((flux, _)) => {
                // Sous Linux le flux accepté n'hérite pas du mode non bloquant ; ailleurs si. On le garantit ici.
                let _ = flux.set_nonblocking(false);
                let Some(garde_connexion) = plafond.prendre() else {
                    // Au-delà du plafond, la connexion de trop est fermée tout de suite ; les autres continuent.
                    journal.noter("refuse - - plafond");
                    continue;
                };
                let decideur = Arc::clone(&decideur);
                let journal = Arc::clone(&journal);
                let _ = thread::Builder::new().spawn(move || {
                    let _garde = garde_connexion;
                    let _ = servir(flux, decideur.as_ref(), journal.as_ref(), DELAI);
                });
            }
            Err(erreur) if erreur.kind() == io::ErrorKind::WouldBlock => {
                // raccourci : l'arrêt est vu au plus tard après 10 ms ; monter avec un réveil du accept (self-pipe) si
                // l'arrêt doit être immédiat.
                thread::sleep(Duration::from_millis(10));
            }
            Err(erreur)
                if matches!(
                    erreur.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset
                ) =>
            {
                continue;
            }
            Err(_) => {
                // Descripteurs épuisés ou erreur passagère : le filtre ne doit pas mourir et laisser le bac sans réseau.
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    drop(garde);
    Ok(())
}
