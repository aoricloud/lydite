#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lydite_reseau::relais::{relais, relais_sur};

/// Un dossier neuf qui tient un socket `s` à l'écoute (sous Unix), retiré à la fin. Hors Unix, seul le chemin existe.
struct Socket {
    racine: PathBuf,
    #[cfg(unix)]
    _ecoute: std::os::unix::net::UnixListener,
}

impl Socket {
    fn neuf(nom: &str) -> Socket {
        let racine = std::env::temp_dir().join(format!("lr-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&racine);
        std::fs::create_dir_all(&racine).unwrap();
        Socket {
            #[cfg(unix)]
            _ecoute: std::os::unix::net::UnixListener::bind(racine.join("s")).unwrap(),
            racine,
        }
    }

    fn chemin(&self) -> PathBuf {
        self.racine.join("s")
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.racine);
    }
}

#[test]
#[cfg_attr(unix, ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it")]
fn le_code_de_la_commande_est_rendu() {
    let socket = Socket::neuf("code");
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let code = relais_sur(listener, &socket.chemin(), || Ok(7)).unwrap();
    assert_eq!(code, 7);
}

#[test]
#[cfg_attr(unix, ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it")]
fn relais_sur_laisse_l_ecoute_ouverte_avant_la_commande() {
    // L'écoute est liée par l'appelant : ce test prouve que `relais_sur` ne la ferme pas. L'ordre du binaire
    // (`relais` épingle la cible, lie l'écoute, puis passe à `relais_epingle`, comme `relais_sur`) se lit dans
    // `relais.rs`.
    let socket = Socket::neuf("ecoute");
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let adresse = listener.local_addr().unwrap();
    let vu = Arc::new(AtomicBool::new(false));
    let temoin = Arc::clone(&vu);
    let code = relais_sur(listener, &socket.chemin(), move || {
        let ouvert = TcpStream::connect(adresse).is_ok();
        temoin.store(ouvert, Ordering::SeqCst);
        Ok(42)
    })
    .unwrap();
    assert_eq!(code, 42);
    assert!(vu.load(Ordering::SeqCst), "l'écoute n'était pas ouverte avant la commande");
}

#[test]
#[cfg_attr(unix, ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it")]
fn un_ecoute_non_locale_est_refusee() {
    let socket = Socket::neuf("locale");
    let erreur = relais("0.0.0.0:0", &socket.chemin(), || Ok(0)).unwrap_err();
    assert!(erreur.to_string().contains("must be loopback"), "{erreur}");
}

#[test]
#[cfg_attr(unix, ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it")]
fn une_commande_introuvable_sort_127() {
    let socket = Socket::neuf("introuvable");
    let binaire = env!("CARGO_BIN_EXE_lydite-reseau");
    let statut = Command::new(binaire)
        .args(["relais", "--ecoute", "127.0.0.1:0", "--vers"])
        .arg(socket.chemin())
        .args(["--", "/nonexistent-commande-lydite"])
        .status()
        .unwrap();
    assert_eq!(statut.code(), Some(127));
}

#[cfg(unix)]
#[test]
#[ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it"]
fn une_commande_tuee_par_signal_rend_128_plus_le_signal() {
    let socket = Socket::neuf("signal");
    let binaire = env!("CARGO_BIN_EXE_lydite-reseau");
    let statut = Command::new(binaire)
        .args(["relais", "--ecoute", "127.0.0.1:0", "--vers"])
        .arg(socket.chemin())
        .args(["--", "sh", "-c", "kill -TERM $$"])
        .status()
        .unwrap();
    assert_eq!(statut.code(), Some(143), "SIGTERM doit rendre 128 + 15");
}

#[cfg(unix)]
#[test]
fn un_plafond_nul_est_refuse() {
    let binaire = env!("CARGO_BIN_EXE_lydite-reseau");
    let racine = std::env::temp_dir().join(format!("lydite-reseau-plafond-{}", std::process::id()));
    let statut = Command::new(binaire)
        .args(["filtre", "--socket"])
        .arg(racine.join("f.sock"))
        .args(["--domaines", "127.0.0.1", "--plafond", "0"])
        .status()
        .unwrap();
    assert_eq!(statut.code(), Some(2));
    let _ = std::fs::remove_dir_all(racine);
}

/// Un socket Unix qui sert une seule connexion : il lit quatre octets, puis répond `reponse`.
#[cfg(unix)]
fn serveur_unique(
    ecoute: std::os::unix::net::UnixListener,
    reponse: &'static [u8],
) -> std::thread::JoinHandle<Vec<u8>> {
    use std::io::{Read, Write};
    std::thread::spawn(move || {
        let (mut flux, _) = ecoute.accept().unwrap();
        let mut recu = [0u8; 4];
        flux.read_exact(&mut recu).unwrap();
        flux.write_all(reponse).unwrap();
        recu.to_vec()
    })
}

#[cfg(unix)]
#[test]
#[ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it"]
fn un_dossier_renomme_et_remplace_apres_le_depart_ne_change_pas_la_cible() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;

    let racine = std::env::temp_dir().join(format!("lr-{}-epingle", std::process::id()));
    let _ = std::fs::remove_dir_all(&racine);
    let dossier = racine.join("d");
    std::fs::create_dir_all(&dossier).unwrap();
    let premier = UnixListener::bind(dossier.join("s")).unwrap();
    let serveur = serveur_unique(premier, b"premier");

    let tcp = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let adresse = tcp.local_addr().unwrap();
    let ecart = racine.join("ancien");
    let (neuf, temoin_socket) = (dossier.clone(), dossier.join("s"));
    let mut lu = [0u8; 7];
    let mut lecture = Ok(());
    let mut chez_le_temoin = None;
    let code = relais_sur(tcp, &dossier.join("s"), || {
        // Après le départ : le dossier est renommé, puis un dossier neuf prend sa place, où le même nom désigne un
        // autre socket, le témoin.
        std::fs::rename(&neuf, &ecart).unwrap();
        std::fs::create_dir(&neuf).unwrap();
        let temoin = UnixListener::bind(&temoin_socket).unwrap();
        temoin.set_nonblocking(true).unwrap();
        let mut client = TcpStream::connect(adresse).unwrap();
        client.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        client.write_all(b"ping").unwrap();
        lecture = client.read_exact(&mut lu);
        // Le témoin ne reçoit rien : une connexion y serait déjà en attente.
        chez_le_temoin = Some(temoin.accept().map(|_| ()).map_err(|e| e.kind()));
        Ok(0)
    })
    .unwrap();
    assert_eq!(code, 0);
    lecture.unwrap();
    assert_eq!(&lu, b"premier", "la connexion n'a pas atteint le premier socket");
    assert_eq!(chez_le_temoin, Some(Err(std::io::ErrorKind::WouldBlock)), "le témoin a reçu une connexion");
    assert_eq!(serveur.join().unwrap(), b"ping");
    let _ = std::fs::remove_dir_all(&racine);
}

/// Lance le binaire avec `--vers <vers>` et une commande qui écrit `marque` ; rend le statut et l'erreur écrite.
#[cfg(unix)]
fn lancer_avec(vers: &std::path::Path, marque: &std::path::Path) -> (Option<i32>, String) {
    let binaire = env!("CARGO_BIN_EXE_lydite-reseau");
    let sortie = Command::new(binaire)
        .args(["relais", "--ecoute", "127.0.0.1:0", "--vers"])
        .arg(vers)
        .args(["--", "sh", "-c", "touch \"$1\"", "sh"])
        .arg(marque)
        .output()
        .unwrap();
    (sortie.status.code(), String::from_utf8_lossy(&sortie.stderr).into_owned())
}

/// Le relais refuse de partir pour `vers` : statut 2, l'erreur dit `texte`, et la commande n'est pas partie.
#[cfg(unix)]
fn refuse(vers: &std::path::Path, marque: &std::path::Path, texte: &str) {
    let (code, erreur) = lancer_avec(vers, marque);
    assert_eq!(code, Some(2), "{}: {erreur}", vers.display());
    assert!(erreur.contains(texte), "{}: {erreur}", vers.display());
    assert!(!marque.exists(), "la commande est partie malgré {}", vers.display());
}

#[cfg(unix)]
#[test]
fn un_vers_qui_est_un_lien_un_fichier_ou_rien_empeche_le_depart() {
    use std::os::unix::fs::symlink;

    let racine = std::env::temp_dir().join(format!("lr-{}-refus", std::process::id()));
    let _ = std::fs::remove_dir_all(&racine);
    let reel = racine.join("reel");
    let marque = racine.join("marque");
    std::fs::create_dir_all(&reel).unwrap();
    std::fs::write(reel.join("fichier"), b"").unwrap();
    symlink(&reel, racine.join("lien")).unwrap();
    symlink(reel.join("fichier"), reel.join("lien-fichier")).unwrap();

    refuse(&racine.join("lien").join("fichier"), &marque, "is a symbolic link");
    refuse(&reel.join("lien-fichier"), &marque, "is not a Unix socket");
    refuse(&reel.join("fichier"), &marque, "is not a Unix socket");
    refuse(&reel.join("absent"), &marque, "cannot be read");
    let _ = std::fs::remove_dir_all(&racine);
}

#[cfg(unix)]
#[test]
#[ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it"]
fn un_vers_qui_est_un_lien_vers_un_vrai_socket_empeche_le_depart_et_le_vrai_socket_fait_partir() {
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;

    let racine = std::env::temp_dir().join(format!("lr-{}-lien", std::process::id()));
    let _ = std::fs::remove_dir_all(&racine);
    let reel = racine.join("reel");
    let marque = racine.join("marque");
    std::fs::create_dir_all(&reel).unwrap();
    let _ecoute = UnixListener::bind(reel.join("s")).unwrap();
    symlink(&reel, racine.join("lien")).unwrap();
    symlink(reel.join("s"), reel.join("lien-socket")).unwrap();

    // Le dossier ou l'entrée est un lien : il mène pourtant à un vrai socket, et le relais refuse.
    refuse(&racine.join("lien").join("s"), &marque, "is a symbolic link");
    refuse(&reel.join("lien-socket"), &marque, "is not a Unix socket");
    // Témoin : le vrai socket dans le vrai dossier laisse partir, et la commande écrit sa marque.
    let (code, erreur) = lancer_avec(&reel.join("s"), &marque);
    assert_eq!((code, marque.exists()), (Some(0), true), "{erreur}");
    let _ = std::fs::remove_dir_all(&racine);
}
