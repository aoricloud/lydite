#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
//! Le binaire : `lydite-reseau filtre` (hors du bac) et `lydite-reseau relais` (dans le bac).

use std::env;
use std::io;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use lydite_reseau::Journal;
use lydite_reseau::connexions;
use lydite_reseau::proxy::{Decideur, Resolveur};
use lydite_reseau::relais::relais;

fn main() -> ExitCode {
    match executer() {
        Ok(code) => match u8::try_from(code) {
            Ok(code) => ExitCode::from(code),
            Err(_) => ExitCode::from(2),
        },
        Err(erreur) => {
            eprintln!("lydite-reseau: {erreur}");
            ExitCode::from(2)
        }
    }
}

fn executer() -> io::Result<i32> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("filtre") => filtre(&args[1..]),
        Some("relais") => relais_commande(&args[1..]),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: lydite-reseau filtre --socket <path> --domaines <a,b,c> [--journal <file>] [--plafond <n>] | \
             lydite-reseau relais --ecoute 127.0.0.1:<port> --vers <socket> -- <command> [args...]",
        )),
    }
}

fn filtre(args: &[String]) -> io::Result<i32> {
    let mut socket = None;
    let mut domaines = None;
    let mut journal = None;
    let mut plafond = connexions::MAX;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                i += 1;
                socket = Some(PathBuf::from(valeur(args, i, "--socket")?));
            }
            "--domaines" => {
                i += 1;
                domaines = Some(valeur(args, i, "--domaines")?.to_string());
            }
            "--journal" => {
                i += 1;
                journal = Some(PathBuf::from(valeur(args, i, "--journal")?));
            }
            "--plafond" => {
                i += 1;
                let texte = valeur(args, i, "--plafond")?;
                plafond = texte
                    .parse()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid --plafond: {texte}")))?;
                if plafond == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "--plafond must be at least 1"));
                }
            }
            autre => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("unknown argument: {autre}")));
            }
        }
        i += 1;
    }
    let socket = socket.ok_or_else(|| argument_manquant("--socket"))?;
    let domaines = domaines.ok_or_else(|| argument_manquant("--domaines"))?;
    let liste = domaines.split(',').map(str::to_string).collect();
    let resolveur: Box<Resolveur> = Box::new(resoudre);
    let decideur = Decideur::nouveau(liste, resolveur)?;
    let journal = Journal::nouveau(journal.as_deref())?;

    #[cfg(unix)]
    {
        lydite_reseau::filtre::filtre(&socket, decideur, journal, plafond)?;
        Ok(0)
    }
    #[cfg(not(unix))]
    {
        let _ = (socket, decideur, journal);
        Err(io::Error::new(io::ErrorKind::Unsupported, "Unix sockets are not supported on this platform"))
    }
}

fn relais_commande(args: &[String]) -> io::Result<i32> {
    let mut ecoute = None;
    let mut vers = None;
    let mut commande = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--ecoute" => {
                i += 1;
                ecoute = Some(valeur(args, i, "--ecoute")?.to_string());
            }
            "--vers" => {
                i += 1;
                vers = Some(PathBuf::from(valeur(args, i, "--vers")?));
            }
            "--" => {
                commande = args[i + 1..].to_vec();
                break;
            }
            autre => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("unknown argument: {autre}")));
            }
        }
        i += 1;
    }
    let ecoute = ecoute.ok_or_else(|| argument_manquant("--ecoute"))?;
    let vers = vers.ok_or_else(|| argument_manquant("--vers"))?;
    let Some((programme, arguments)) = commande.split_first() else {
        return Err(argument_manquant("-- <command>"));
    };
    let programme = programme.clone();
    let arguments = arguments.to_vec();
    relais(&ecoute, &vers, move || lancer_commande(&programme, &arguments))
}

fn lancer_commande(programme: &str, arguments: &[String]) -> io::Result<i32> {
    match Command::new(programme)
        .args(arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(statut) => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = statut.signal() {
                    return Ok(128 + signal);
                }
            }
            Ok(statut.code().unwrap_or(1))
        }
        Err(erreur) => {
            eprintln!("lydite-reseau: cannot start the command: {erreur}");
            Ok(127)
        }
    }
}

fn valeur<'a>(args: &'a [String], i: usize, nom: &str) -> io::Result<&'a str> {
    args.get(i).map(String::as_str).ok_or_else(|| argument_manquant(nom))
}

fn argument_manquant(nom: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("missing argument: {nom}"))
}

// raccourci : la résolution système n'a pas de délai propre ; elle occupe une place du plafond de connexions.
// Monter avec un résolveur borné si un DNS lent doit être interrompu.
fn resoudre(hote: &str) -> io::Result<Vec<IpAddr>> {
    let adresses = (hote, 0).to_socket_addrs()?;
    Ok(adresses.map(|adresse| adresse.ip()).collect())
}
