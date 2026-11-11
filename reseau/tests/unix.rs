#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use lydite_reseau::relais_sur;

fn racine(nom: &str) -> PathBuf {
    let chemin = std::env::temp_dir().join(format!("lydite-reseau-unix-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&chemin);
    std::fs::create_dir_all(&chemin).unwrap();
    chemin
}

fn lire_jusqu_a(flux: &mut impl Read, motif: &[u8]) -> Vec<u8> {
    let mut lu = Vec::new();
    let mut octet = [0u8; 1];
    while !lu.ends_with(motif) {
        let n = flux.read(&mut octet).unwrap();
        assert!(n > 0, "flux fermé avant {motif:?} : {lu:?}");
        lu.push(octet[0]);
    }
    lu
}

#[test]
#[ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it"]
fn le_filtre_sert_un_connect_sur_le_socket_unix() {
    let racine = racine("filtre");
    let socket = racine.join("filtre.sock");
    let journal = racine.join("journal.log");

    let amont = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = amont.local_addr().unwrap().port();
    let fil_amont = thread::spawn(move || {
        let (mut flux, _) = amont.accept().unwrap();
        let mut recu = [0u8; 4];
        flux.read_exact(&mut recu).unwrap();
        assert_eq!(&recu, b"ping");
        flux.write_all(b"pong").unwrap();
    });

    let binaire = env!("CARGO_BIN_EXE_lydite-reseau");
    let mut enfant = Command::new(binaire)
        .args(["filtre", "--socket"])
        .arg(&socket)
        .args(["--domaines", "127.0.0.1", "--journal"])
        .arg(&journal)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdout = enfant.stdout.take().unwrap();
        let mut lecteur = BufReader::new(stdout);
        let mut ligne = String::new();
        lecteur.read_line(&mut ligne).unwrap();
        assert_eq!(ligne.trim_end(), "ready");
    }

    let mut client = UnixStream::connect(&socket).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    client.write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n").as_bytes()).unwrap();
    let reponse = lire_jusqu_a(&mut client, b"\r\n\r\n");
    assert_eq!(reponse, b"HTTP/1.1 200 Connection Established\r\n\r\n");
    client.write_all(b"ping").unwrap();
    let mut pong = [0u8; 4];
    client.read_exact(&mut pong).unwrap();
    assert_eq!(&pong, b"pong");
    drop(client);

    drop(enfant.stdin.take());
    let statut = enfant.wait().unwrap();
    assert!(statut.success());
    assert!(!socket.exists(), "le socket n'a pas été retiré");
    let contenu = std::fs::read_to_string(&journal).unwrap();
    assert!(contenu.contains(&format!("permis 127.0.0.1 {port} 127.0.0.1 liste")), "{contenu}");
    fil_amont.join().unwrap();
    let _ = std::fs::remove_dir_all(racine);
}

#[test]
#[ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it"]
fn le_relais_passe_les_octets_par_le_socket_unix() {
    let racine = racine("relais");
    let socket = racine.join("relais.sock");
    let amont = UnixListener::bind(&socket).unwrap();
    let fil_amont = thread::spawn(move || {
        let (mut flux, _) = amont.accept().unwrap();
        let mut recu = [0u8; 4];
        flux.read_exact(&mut recu).unwrap();
        assert_eq!(&recu, b"ping");
        flux.write_all(b"pong").unwrap();
    });

    let ecoute = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let adresse = ecoute.local_addr().unwrap();
    let code = relais_sur(ecoute, &socket, move || {
        let mut client = TcpStream::connect(adresse)?;
        client.write_all(b"ping")?;
        let mut pong = [0u8; 4];
        client.read_exact(&mut pong)?;
        assert_eq!(&pong, b"pong");
        Ok(0)
    })
    .unwrap();
    assert_eq!(code, 0);
    fil_amont.join().unwrap();
    let _ = std::fs::remove_dir_all(racine);
}

#[test]
#[ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it"]
fn le_plafond_refuse_la_connexion_de_trop() {
    let racine = racine("plafond");
    let socket = racine.join("plafond.sock");
    let journal = racine.join("journal.log");
    let binaire = env!("CARGO_BIN_EXE_lydite-reseau");
    let mut enfant = Command::new(binaire)
        .args(["filtre", "--socket"])
        .arg(&socket)
        .args(["--domaines", "127.0.0.1", "--journal"])
        .arg(&journal)
        .args(["--plafond", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdout = enfant.stdout.take().unwrap();
        let mut lecteur = BufReader::new(stdout);
        let mut ligne = String::new();
        lecteur.read_line(&mut ligne).unwrap();
        assert_eq!(ligne.trim_end(), "ready");
    }

    let mut premier = UnixStream::connect(&socket).unwrap();
    let mut second = UnixStream::connect(&socket).unwrap();
    second.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut tampon = [0u8; 1];
    let lu = second.read(&mut tampon);
    assert!(matches!(lu, Ok(0) | Err(_)), "la connexion de trop doit être fermée : {lu:?}");

    // La première connexion est toujours servie.
    premier.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    premier.write_all(b"PAS UNE REQUETE\r\n\r\n").unwrap();
    let mut reponse = Vec::new();
    premier.read_to_end(&mut reponse).unwrap();
    assert!(reponse.starts_with(b"HTTP/1.1 400 Bad Request"), "{reponse:?}");
    drop(premier);

    drop(enfant.stdin.take());
    let statut = enfant.wait().unwrap();
    assert!(statut.success());
    assert!(!socket.exists(), "le socket n'a pas été retiré");
    let contenu = std::fs::read_to_string(&journal).unwrap();
    assert!(contenu.contains("refuse - - plafond"), "{contenu}");
    assert!(contenu.contains("refuse - - requete-invalide"), "{contenu}");
    let _ = std::fs::remove_dir_all(racine);
}
