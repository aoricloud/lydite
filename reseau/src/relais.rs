//! Le relais : dans le bac, il ouvre une écoute TCP puis lance la commande ; chaque connexion va au socket Unix.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::connexions::{self, Plafond};

#[cfg(unix)]
use crate::flux::tunnel;

/// Ouvre l'écoute TCP, puis lance la commande. Rend le code de sortie de la commande.
pub fn relais<F>(ecoute: &str, vers: &Path, lancer: F) -> io::Result<i32>
where
    F: FnOnce() -> io::Result<i32>,
{
    let listener = TcpListener::bind(ecoute)?;
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "relay listen address must be loopback"));
    }
    relais_sur(listener, vers, lancer)
}

/// Même chose avec une écoute déjà ouverte : c'est le point testable de l'ordre « écoute avant la commande ».
pub fn relais_sur<F>(listener: TcpListener, vers: &Path, lancer: F) -> io::Result<i32>
where
    F: FnOnce() -> io::Result<i32>,
{
    let vers: PathBuf = vers.to_path_buf();
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
            let vers = vers.clone();
            let _ = thread::Builder::new().spawn(move || {
                let _garde = garde;
                let _ = relayer(client, &vers);
            });
        }
    });
    lancer()
}

#[cfg(unix)]
fn relayer(client: TcpStream, vers: &Path) -> io::Result<()> {
    let amont = std::os::unix::net::UnixStream::connect(vers)?;
    tunnel(client, amont)
}

#[cfg(not(unix))]
fn relayer(_client: TcpStream, _vers: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix sockets are not supported on this platform"))
}
