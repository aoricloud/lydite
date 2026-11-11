#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use lydite_reseau::{DELAI, Decideur, Journal, Refus, Verdict, servir};

fn decideur(domaines: &[&str], adresses: Vec<IpAddr>) -> Decideur {
    let domaines = domaines.iter().map(|domaine| domaine.to_string()).collect();
    Decideur::nouveau(domaines, Box::new(move |_| Ok(adresses.clone()))).unwrap()
}

/// Une paire TCP locale : le client, et le côté que le proxy sert.
fn paire() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let adresse = listener.local_addr().unwrap();
    let client = TcpStream::connect(adresse).unwrap();
    let (cote, _) = listener.accept().unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    cote.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    (client, cote)
}

/// Le client des tests : son `Drop` ferme le flux des deux côtés, pour qu'une assertion qui rougit ne bloque pas la
/// portée sur un proxy encore dans son tunnel.
struct Client(TcpStream);

impl Deref for Client {
    type Target = TcpStream;
    fn deref(&self) -> &TcpStream {
        &self.0
    }
}

impl DerefMut for Client {
    fn deref_mut(&mut self) -> &mut TcpStream {
        &mut self.0
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}

fn amont() -> (TcpListener, u16) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
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

fn lire_tout(flux: &mut impl Read) -> Vec<u8> {
    let mut lu = Vec::new();
    flux.read_to_end(&mut lu).unwrap();
    lu
}

fn journal_temporaire(nom: &str) -> PathBuf {
    let chemin = std::env::temp_dir().join(format!("lydite-reseau-{}-{nom}.log", std::process::id()));
    let _ = std::fs::remove_file(&chemin);
    chemin
}

#[test]
fn un_connect_permis_passe_des_octets_dans_les_deux_sens() {
    let (client, cote) = paire();
    let (amont_listener, port) = amont();
    let decideur = decideur(&["127.0.0.1"], Vec::new());
    let journal = Journal::nouveau(None).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let _amont = thread::spawn(move || {
            let (mut flux, _) = amont_listener.accept().unwrap();
            let mut recu = [0u8; 4];
            flux.read_exact(&mut recu).unwrap();
            assert_eq!(&recu, b"ping");
            flux.write_all(b"pong").unwrap();
        });
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n").as_bytes()).unwrap();
        let reponse = lire_jusqu_a(&mut *client, b"\r\n\r\n");
        assert_eq!(reponse, b"HTTP/1.1 200 Connection Established\r\n\r\n");
        client.write_all(b"ping").unwrap();
        let mut pong = [0u8; 4];
        client.read_exact(&mut pong).unwrap();
        assert_eq!(&pong, b"pong");
        client.shutdown(std::net::Shutdown::Write).unwrap();
        proxy.join().unwrap();
    });
}

#[test]
fn un_connect_hors_liste_recoit_le_403_et_le_serveur_ne_voit_rien() {
    let (client, cote) = paire();
    let (amont_listener, _port) = amont();
    amont_listener.set_nonblocking(true).unwrap();
    let decideur = decideur(&["example.com"], vec![Ipv4Addr::LOCALHOST.into()]);
    let journal = Journal::nouveau(None).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"CONNECT example.org:443 HTTP/1.1\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(
            reponse,
            b"HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nX-Proxy-Error: blocked-by-allowlist\r\n\r\nConnection blocked by network allowlist"
        );
        proxy.join().unwrap();
    });
    assert!(matches!(amont_listener.accept(), Err(erreur) if erreur.kind() == std::io::ErrorKind::WouldBlock));
}

#[test]
fn une_adresse_resolue_refusee_recoit_le_403_adresse() {
    let (client, cote) = paire();
    let decideur = decideur(&["example.com"], vec![Ipv4Addr::LOCALHOST.into()]);
    let journal = Journal::nouveau(None).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"CONNECT example.com:443 HTTP/1.1\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(
            reponse,
            b"HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nX-Proxy-Error: blocked-by-resolved-address\r\n\r\nConnection blocked by resolved address"
        );
        proxy.join().unwrap();
    });
}

#[test]
fn une_requete_a_uri_absolue_est_transmise_en_forme_d_origine() {
    let (client, cote) = paire();
    let (amont_listener, port) = amont();
    let decideur = decideur(&["127.0.0.1"], Vec::new());
    let journal = Journal::nouveau(None).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let _amont = thread::spawn(move || {
            let (mut flux, _) = amont_listener.accept().unwrap();
            let entete = lire_jusqu_a(&mut flux, b"\r\n\r\n");
            let texte = String::from_utf8(entete).unwrap();
            let mut lignes = texte.split("\r\n");
            // La réponse part avant les assertions : une assertion qui rougit ne laisse pas le client en attente.
            flux.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nbody").unwrap();
            assert_eq!(lignes.next(), Some("GET /chemin?q=1 HTTP/1.1"));
            let reste = lignes.collect::<Vec<_>>();
            let hote = format!("Host: 127.0.0.1:{port}");
            assert!(reste.contains(&hote.as_str()), "{reste:?}");
            assert!(reste.contains(&"Connection: close"), "{reste:?}");
            assert!(reste.contains(&"X-Test: 1"), "{reste:?}");
        });
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client
            .write_all(
                format!("GET http://127.0.0.1:{port}/chemin?q=1 HTTP/1.1\r\nHost: ignored\r\nX-Test: 1\r\n\r\n")
                    .as_bytes(),
            )
            .unwrap();
        let reponse = lire_tout(&mut *client);
        assert!(reponse.ends_with(b"body"), "{reponse:?}");
        client.shutdown(std::net::Shutdown::Write).unwrap();
        proxy.join().unwrap();
    });
}

#[test]
fn une_requete_mal_formee_est_400() {
    let (client, cote) = paire();
    let chemin = journal_temporaire("journal-mal-forme");
    let decideur = decideur(&["example.com"], Vec::new());
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"PAS UNE REQUETE\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nBad Request");
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(contenu.lines().collect::<Vec<_>>(), ["refuse - - requete-invalide"]);
    std::fs::remove_file(chemin).unwrap();
}

#[test]
fn un_lf_dans_l_hote_ne_fabrique_pas_une_ligne_de_journal() {
    let (client, cote) = paire();
    let chemin = journal_temporaire("journal-lf");
    let decideur = decideur(&["example.com"], vec![Ipv4Addr::LOCALHOST.into()]);
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"CONNECT a\nrefuse:443 HTTP/1.1\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nBad Request");
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(contenu.lines().collect::<Vec<_>>(), ["refuse - - requete-invalide"]);
    std::fs::remove_file(chemin).unwrap();
}

#[test]
fn un_hote_mal_forme_imprimable_est_au_journal() {
    let (client, cote) = paire();
    let chemin = journal_temporaire("journal-hote");
    let decideur = decideur(&["example.com"], Vec::new());
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"CONNECT a@b:443 HTTP/1.1\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nBad Request");
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(contenu.lines().collect::<Vec<_>>(), ["refuse - 443 hote-invalide"]);
    std::fs::remove_file(chemin).unwrap();
}

#[test]
fn une_resolution_en_erreur_est_502_exact() {
    let (client, cote) = paire();
    let chemin = journal_temporaire("journal-resolution-erreur");
    let decideur =
        Decideur::nouveau(vec!["example.com".into()], Box::new(|_| Err(io::Error::other("pas de DNS")))).unwrap();
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"CONNECT example.com:443 HTTP/1.1\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\n\r\nBad Gateway");
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(contenu.lines().collect::<Vec<_>>(), ["refuse example.com 443 resolution-impossible"]);
    std::fs::remove_file(chemin).unwrap();
}

#[test]
fn une_resolution_vide_est_502_exact() {
    let (client, cote) = paire();
    let chemin = journal_temporaire("journal-resolution-vide");
    let decideur = Decideur::nouveau(vec!["example.com".into()], Box::new(|_| Ok(Vec::new()))).unwrap();
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"CONNECT example.com:443 HTTP/1.1\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\n\r\nBad Gateway");
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(contenu.lines().collect::<Vec<_>>(), ["refuse example.com 443 resolution-impossible"]);
    std::fs::remove_file(chemin).unwrap();
}

#[test]
fn un_connect_permis_par_adresse_resolue_listee_joint_le_serveur() {
    let (client, cote) = paire();
    let (amont_listener, port) = amont();
    let appels = Arc::new(AtomicUsize::new(0));
    let temoin = Arc::clone(&appels);
    let decideur = Decideur::nouveau(
        vec!["example.com".into(), "127.0.0.1".into()],
        Box::new(move |_| {
            temoin.fetch_add(1, Ordering::SeqCst);
            Ok(vec![Ipv4Addr::LOCALHOST.into()])
        }),
    )
    .unwrap();
    let journal = Journal::nouveau(None).unwrap();
    // Détaché : si le proxy refuse (mutation), le test rougit sans attendre un accept qui ne viendra pas.
    let _amont = thread::spawn(move || {
        let (mut flux, _) = amont_listener.accept().unwrap();
        let mut recu = [0u8; 4];
        flux.read_exact(&mut recu).unwrap();
        assert_eq!(&recu, b"ping");
        flux.write_all(b"pong").unwrap();
    });
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(format!("CONNECT example.com:{port} HTTP/1.1\r\n\r\n").as_bytes()).unwrap();
        let reponse = lire_jusqu_a(&mut *client, b"\r\n\r\n");
        assert_eq!(reponse, b"HTTP/1.1 200 Connection Established\r\n\r\n");
        client.write_all(b"ping").unwrap();
        let mut pong = [0u8; 4];
        client.read_exact(&mut pong).unwrap();
        assert_eq!(&pong, b"pong");
        client.shutdown(std::net::Shutdown::Write).unwrap();
        proxy.join().unwrap();
    });
    assert_eq!(appels.load(Ordering::SeqCst), 1, "une seule résolution");
}

#[test]
fn un_nom_hors_liste_n_est_jamais_resolu() {
    let appele = Arc::new(AtomicBool::new(false));
    let temoin = Arc::clone(&appele);
    let decideur = Decideur::nouveau(
        vec!["example.com".into()],
        Box::new(move |_| {
            temoin.store(true, Ordering::SeqCst);
            Ok(vec![Ipv4Addr::LOCALHOST.into()])
        }),
    )
    .unwrap();
    assert_eq!(decideur.decider("example.org"), Verdict::Refuse(Refus::HorsListe));
    assert_eq!(decideur.decider("192.0.2.1"), Verdict::Refuse(Refus::HorsListe));
    assert_eq!(decideur.decider("a b"), Verdict::Refuse(Refus::HoteInvalide));
    assert!(!appele.load(Ordering::SeqCst), "le résolveur ne doit pas être appelé");
}

#[test]
fn un_octet_de_controle_dans_l_entete_est_400() {
    let (client, cote) = paire();
    let decideur = decideur(&["127.0.0.1"], Vec::new());
    let journal = Journal::nouveau(None).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"GET http://127.0.0.1/a\nb HTTP/1.1\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nBad Request");
        proxy.join().unwrap();
    });
}

#[test]
fn un_nom_d_entete_avec_espace_est_400() {
    let (client, cote) = paire();
    let decideur = decideur(&["127.0.0.1"], Vec::new());
    let journal = Journal::nouveau(None).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"GET http://127.0.0.1/ HTTP/1.1\r\nHost : x\r\n\r\n").unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nBad Request");
        proxy.join().unwrap();
    });
}

#[test]
fn un_client_muet_recoit_le_400_du_delai() {
    let (client, cote) = paire();
    let chemin = journal_temporaire("journal-delai");
    let decideur = decideur(&["example.com"], Vec::new());
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, Duration::from_millis(50)).unwrap());
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nBad Request");
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(contenu.lines().collect::<Vec<_>>(), ["refuse - - delai"]);
    std::fs::remove_file(chemin).unwrap();
}

#[test]
fn un_entete_valide_suivi_d_un_corps_ne_depasse_pas_la_borne() {
    let (client, cote) = paire();
    let (amont_listener, port) = amont();
    let decideur = decideur(&["127.0.0.1"], Vec::new());
    let journal = Journal::nouveau(None).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let _amont = thread::spawn(move || {
            let (mut flux, _) = amont_listener.accept().unwrap();
            let entete = lire_jusqu_a(&mut flux, b"\r\n\r\n");
            let texte = String::from_utf8(entete).unwrap();
            assert!(texte.contains("X-Big: "), "{texte}");
            let mut corps = vec![0u8; 2048];
            flux.read_exact(&mut corps).unwrap();
            assert_eq!(&corps, &vec![b'b'; 2048]);
            flux.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").unwrap();
        });
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        let mut requete = format!("GET http://127.0.0.1:{port}/ HTTP/1.1\r\nX-Big: ");
        requete.push_str(&"a".repeat(7000));
        requete.push_str("\r\nContent-Length: 2048\r\n\r\n");
        requete.push_str(&"b".repeat(2048));
        client.write_all(requete.as_bytes()).unwrap();
        let reponse = lire_tout(&mut *client);
        assert!(reponse.ends_with(b"ok"), "{reponse:?}");
        client.shutdown(std::net::Shutdown::Write).unwrap();
        proxy.join().unwrap();
    });
}

#[test]
fn un_entete_trop_long_est_400_avant_toute_connexion() {
    let (client, cote) = paire();
    let (amont_listener, port) = amont();
    let recu = Arc::new(AtomicBool::new(false));
    let temoin = Arc::clone(&recu);
    // Un faux serveur qui accepte : sans la borne, le proxy le joindrait et le test verrait un 200, pas un 400.
    thread::spawn(move || {
        if let Ok((mut flux, _)) = amont_listener.accept() {
            temoin.store(true, Ordering::SeqCst);
            let _ = flux.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        }
    });
    let chemin = journal_temporaire("journal-entete-long");
    let decideur = decideur(&["127.0.0.1"], Vec::new());
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        let mut requete = format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\nX-Big: ");
        requete.push_str(&"a".repeat(8 * 1024));
        requete.push_str("\r\n\r\n");
        let _ = client.write_all(requete.as_bytes());
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nBad Request");
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(contenu.lines().collect::<Vec<_>>(), ["refuse - - entete-trop-long"]);
    std::fs::remove_file(chemin).unwrap();
    assert!(!recu.load(Ordering::SeqCst), "le faux serveur a reçu une connexion malgré la borne");
}

#[test]
fn le_journal_a_une_ligne_et_rien_du_contenu() {
    let (client, cote) = paire();
    let chemin = journal_temporaire("journal");
    let decideur = decideur(&["example.com"], vec![Ipv4Addr::LOCALHOST.into()]);
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(b"CONNECT example.org:443 HTTP/1.1\r\nX-Secret: ne-pas-journaliser\r\n\r\n").unwrap();
        let _ = lire_tout(&mut *client);
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(contenu.lines().collect::<Vec<_>>(), ["refuse example.org 443 hors-liste"]);
    assert!(!contenu.contains("ne-pas-journaliser"));
    std::fs::remove_file(chemin).unwrap();
}

#[test]
fn un_connect_tcp_est_502_sans_upstream() {
    let (client, cote) = paire();
    let chemin = journal_temporaire("journal-502");
    let decideur = decideur(&["127.0.0.1"], Vec::new());
    let journal = Journal::nouveau(Some(&chemin)).unwrap();
    let (listener, port) = amont();
    drop(listener);
    thread::scope(|portee| {
        let mut client = Client(client);
        let proxy = portee.spawn(|| servir(cote, &decideur, &journal, DELAI).unwrap());
        client.write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n").as_bytes()).unwrap();
        let reponse = lire_tout(&mut *client);
        assert_eq!(reponse, b"HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\n\r\nBad Gateway");
        proxy.join().unwrap();
    });
    let contenu = std::fs::read_to_string(&chemin).unwrap();
    assert_eq!(
        contenu.lines().collect::<Vec<_>>(),
        [format!("permis 127.0.0.1 {port} 127.0.0.1 liste"), format!("echec 127.0.0.1 {port} injoignable")]
    );
    std::fs::remove_file(chemin).unwrap();
}
