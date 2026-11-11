//! Le relais : dans le bac, il ouvre une écoute TCP puis lance la commande ; chaque connexion va au socket Unix.
//!
//! La cible est épinglée avant la commande : le dossier du socket s'ouvre une fois, et chaque connexion passe par ce
//! descripteur, jamais par le chemin d'origine. Ce que la commande ferait de ce chemin ne change donc rien.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::connexions::{self, Plafond};

#[cfg(unix)]
use crate::flux::tunnel;

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::path::PathBuf;

/// La cible du relais, fixée au départ. Sous Unix : le dossier du socket, ouvert pour toute la vie du relais, et le
/// nom du socket dedans.
#[cfg(unix)]
struct Cible {
    dossier: File,
    nom: OsString,
}

#[cfg(not(unix))]
struct Cible;

/// Les connexions Unix ne tiennent pas plus de 107 octets de chemin.
#[cfg(unix)]
const CHEMIN_MAX: usize = 107;

#[cfg(unix)]
fn epingler(vers: &Path) -> io::Result<Cible> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let refus = |texte: String| io::Error::new(io::ErrorKind::InvalidInput, texte);
    let nom = vers
        .file_name()
        .ok_or_else(|| refus(format!("relay target {} has no socket name", vers.display())))?
        .to_os_string();
    let dossier = match vers.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    // Sans suivre de lien : un dossier qui est un lien est refusé.
    let avant = std::fs::symlink_metadata(dossier)?;
    if avant.file_type().is_symlink() {
        return Err(refus(format!("relay target directory {} is a symbolic link", dossier.display())));
    }
    if !avant.is_dir() {
        return Err(refus(format!("relay target directory {} is not a directory", dossier.display())));
    }
    let ouvert = File::open(dossier)?;
    // Le dossier ouvert est celui qu'on a regardé : un remplacement entre les deux ne passe pas.
    let vu = ouvert.metadata()?;
    if (vu.dev(), vu.ino()) != (avant.dev(), avant.ino()) {
        return Err(refus(format!("relay target directory {} changed while it was opened", dossier.display())));
    }
    let cible = Cible { dossier: ouvert, nom };
    let chemin = cible.chemin();
    if chemin.as_os_str().len() > CHEMIN_MAX {
        return Err(refus(format!("relay target {} is too long for a Unix socket", vers.display())));
    }
    // L'entrée se regarde par le descripteur, sans suivre de lien : ni lien, ni fichier, ni rien.
    let entree = std::fs::symlink_metadata(&chemin)
        .map_err(|e| refus(format!("relay target {} cannot be read: {e}", vers.display())))?;
    if !entree.file_type().is_socket() {
        return Err(refus(format!("relay target {} is not a Unix socket", vers.display())));
    }
    Ok(cible)
}

#[cfg(not(unix))]
fn epingler(_vers: &Path) -> io::Result<Cible> {
    Ok(Cible)
}

#[cfg(unix)]
impl Cible {
    /// Le chemin de connexion : par le descripteur du dossier, jamais par le chemin d'origine.
    fn chemin(&self) -> PathBuf {
        use std::os::unix::io::AsRawFd;
        Path::new("/proc/self/fd").join(self.dossier.as_raw_fd().to_string()).join(&self.nom)
    }
}

/// Ouvre l'écoute TCP, puis lance la commande. Rend le code de sortie de la commande. La cible `vers` est épinglée
/// d'abord : si elle ne convient pas, rien ne s'ouvre et la commande ne part pas.
pub fn relais<F>(ecoute: &str, vers: &Path, lancer: F) -> io::Result<i32>
where
    F: FnOnce() -> io::Result<i32>,
{
    let cible = epingler(vers)?;
    let listener = TcpListener::bind(ecoute)?;
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "relay listen address must be loopback"));
    }
    relais_epingle(listener, cible, lancer)
}

/// Même chose avec une écoute déjà ouverte : la cible s'épingle ici, après l'écoute, puis tout passe à
/// `relais_epingle`, comme dans `relais`. C'est le point testable de l'ordre « écoute avant la commande ».
pub fn relais_sur<F>(listener: TcpListener, vers: &Path, lancer: F) -> io::Result<i32>
where
    F: FnOnce() -> io::Result<i32>,
{
    relais_epingle(listener, epingler(vers)?, lancer)
}

fn relais_epingle<F>(listener: TcpListener, cible: Cible, lancer: F) -> io::Result<i32>
where
    F: FnOnce() -> io::Result<i32>,
{
    let cible = std::sync::Arc::new(cible);
    let _ecoute = thread::spawn(move || {
        let plafond = Plafond::nouveau(connexions::MAX);
        for client in listener.incoming() {
            let client = match client {
                Ok(client) => client,
                Err(erreur)
                    if matches!(
                        erreur.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset
                    ) =>
                {
                    continue;
                }
                Err(_) => {
                    // Erreur passagère (descripteurs épuisés) : ne pas boucler à vide.
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
            };
            let Some(garde) = plafond.prendre() else {
                continue;
            };
            let cible = std::sync::Arc::clone(&cible);
            let _ = thread::Builder::new().spawn(move || {
                let _garde = garde;
                let _ = relayer(client, &cible);
            });
        }
    });
    lancer()
}

#[cfg(unix)]
fn relayer(client: TcpStream, cible: &Cible) -> io::Result<()> {
    let amont = std::os::unix::net::UnixStream::connect(cible.chemin())?;
    tunnel(client, amont)
}

#[cfg(not(unix))]
fn relayer(_client: TcpStream, _cible: &Cible) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix sockets are not supported on this platform"))
}
