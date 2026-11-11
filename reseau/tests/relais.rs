#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lydite_reseau::relais::{relais, relais_sur};

#[test]
fn le_code_de_la_commande_est_rendu() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let code = relais_sur(listener, Path::new("/nonexistent.lydite"), || Ok(7)).unwrap();
    assert_eq!(code, 7);
}

#[test]
fn relais_sur_laisse_l_ecoute_ouverte_avant_la_commande() {
    // L'écoute est liée par l'appelant : ce test prouve que `relais_sur` ne la ferme pas. L'ordre du binaire
    // (`relais` lie puis appelle `relais_sur`) se lit en deux lignes dans `relais.rs`.
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let adresse = listener.local_addr().unwrap();
    let vu = Arc::new(AtomicBool::new(false));
    let temoin = Arc::clone(&vu);
    let code = relais_sur(listener, Path::new("/nonexistent.lydite"), move || {
        let ouvert = TcpStream::connect(adresse).is_ok();
        temoin.store(ouvert, Ordering::SeqCst);
        Ok(42)
    })
    .unwrap();
    assert_eq!(code, 42);
    assert!(vu.load(Ordering::SeqCst), "l'écoute n'était pas ouverte avant la commande");
}

#[test]
fn un_ecoute_non_locale_est_refusee() {
    let resultat = relais("0.0.0.0:0", Path::new("/nonexistent.lydite"), || Ok(0));
    assert!(resultat.is_err(), "une écoute non locale doit être refusée");
}

#[test]
fn une_commande_introuvable_sort_127() {
    let binaire = env!("CARGO_BIN_EXE_lydite-reseau");
    let statut = Command::new(binaire)
        .args([
            "relais",
            "--ecoute",
            "127.0.0.1:0",
            "--vers",
            "/nonexistent.lydite",
            "--",
            "/nonexistent-commande-lydite",
        ])
        .status()
        .unwrap();
    assert_eq!(statut.code(), Some(127));
}

#[cfg(unix)]
#[test]
fn une_commande_tuee_par_signal_rend_128_plus_le_signal() {
    let binaire = env!("CARGO_BIN_EXE_lydite-reseau");
    let statut = Command::new(binaire)
        .args(["relais", "--ecoute", "127.0.0.1:0", "--vers", "/nonexistent.lydite", "--", "sh", "-c", "kill -TERM $$"])
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
