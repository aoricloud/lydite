//! Le flux du serveur : TCP pour les tests, Unix pour le filtre. La copie respecte les demi-fermetures et rend la
//! place du plafond sur toute sortie, erreur comprise.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

/// Un flux que l'on peut cloner pour lire et écrire des deux côtés, fermer l'écriture seule, ou fermer les deux sens.
pub trait Flux: Read + Write + Sized + Send + 'static {
    fn clone_flux(&self) -> io::Result<Self>;
    fn fin_ecriture(&self);
    /// Ferme les deux directions de la socket : réveille un lecteur local bloqué sur un clone.
    fn fermer(&self);
    /// Règle le délai de lecture ; `None` le retire (le tunnel ne doit pas expirer).
    fn delai_lecture(&self, delai: Option<Duration>);
    /// Règle le délai d'écriture : une écriture qui reste bloquée pendant ce délai échoue.
    fn delai_ecriture(&self, delai: Duration) -> io::Result<()>;
}

impl Flux for TcpStream {
    fn clone_flux(&self) -> io::Result<Self> {
        self.try_clone()
    }

    fn fin_ecriture(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }

    fn fermer(&self) {
        let _ = self.shutdown(Shutdown::Both);
    }

    fn delai_lecture(&self, delai: Option<Duration>) {
        let _ = self.set_read_timeout(delai);
    }

    fn delai_ecriture(&self, delai: Duration) -> io::Result<()> {
        self.set_write_timeout(Some(delai))
    }
}

#[cfg(unix)]
impl Flux for UnixStream {
    fn clone_flux(&self) -> io::Result<Self> {
        self.try_clone()
    }

    fn fin_ecriture(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }

    fn fermer(&self) {
        let _ = self.shutdown(Shutdown::Both);
    }

    fn delai_lecture(&self, delai: Option<Duration>) {
        let _ = self.set_read_timeout(delai);
    }

    fn delai_ecriture(&self, delai: Duration) -> io::Result<()> {
        self.set_write_timeout(Some(delai))
    }
}

/// La borne d'écriture du tunnel. Quand les deux fils sont pris du même côté (l'un bloqué en écriture vers un pair
/// qui ne lit plus, l'autre en lecture de ce pair, ou les deux bloqués en écriture), personne ne lit l'autre socket :
/// un client qui s'en va n'y est pas vu, et sa fermeture peut même rester en file derrière ses propres données. Seule
/// une écriture qui reste bloquée le dit alors ; passé cette borne, elle échoue et le tunnel se ferme.
///
/// La borne tient la vivacité, pas la sécurité : une place prise est une sur `connexions::MAX`, et seulement pour le
/// mur d'une preuve. La raccourcir ne rapporte presque rien ; trop courte, elle couperait un pair lent mais vivant et
/// ferait rougir une preuve à tort. 120 s, c'est deux fois le `send_timeout` par défaut de nginx (60 s, au même sens :
/// aucune avancée entre deux écritures).
pub const BORNE_ECRITURE: Duration = Duration::from_secs(120);

/// Copie `a` vers `b` dans les deux sens, jusqu'à la fin des deux flux. Sur une fin de lecture, ferme l'écriture du
/// côté opposé ; l'autre sens voit alors EOF à son tour. Sur une erreur de lecture ou d'écriture, ferme **les deux
/// sockets, les deux directions** : un fil bloqué en lecture comme un fil bloqué en `write_all` se réveillent, même
/// si le pair garde sa socket ouverte et inactive. Une écriture qui reste bloquée pendant `BORNE_ECRITURE` est une
/// erreur d'écriture.
pub fn tunnel<A: Flux, B: Flux>(a: A, b: B) -> io::Result<()> {
    tunnel_borne(a, b, BORNE_ECRITURE)
}

fn tunnel_borne<A: Flux, B: Flux>(a: A, b: B, borne: Duration) -> io::Result<()> {
    // Le délai est celui de la socket, donc de chacun des clones qui suivent. Sans sa borne, le tunnel ne part pas.
    a.delai_ecriture(borne)?;
    b.delai_ecriture(borne)?;
    let a_lecture = a.clone_flux()?;
    let b_lecture = b.clone_flux()?;
    let a_arret_aller = a.clone_flux()?;
    let b_arret_aller = b.clone_flux()?;
    let a_arret_retour = a.clone_flux()?;
    let b_arret_retour = b.clone_flux()?;
    let fil = thread::spawn(move || copier(a_lecture, b, a_arret_aller, b_arret_aller));
    let resultat = copier(b_lecture, a, a_arret_retour, b_arret_retour);
    let joint = fil.join().map_err(|_| io::Error::other("copy thread panicked"))?;
    resultat.and(joint)
}

fn copier<R: Read, W: Write + Flux, A: Flux, B: Flux>(
    mut lecture: R,
    mut ecriture: W,
    arret_a: A,
    arret_b: B,
) -> io::Result<()> {
    let mut tampon = [0u8; 8 * 1024];
    loop {
        match lecture.read(&mut tampon) {
            Ok(0) => {
                // raccourci : la demi-fermeture est respectée ; si le pair reste muet, l'autre sens peut tenir sa
                // place du plafond jusqu'à la fin de la preuve. Une borne d'inactivité pourra venir plus tard.
                ecriture.fin_ecriture();
                return Ok(());
            }
            Ok(n) => {
                if let Err(erreur) = ecriture.write_all(&tampon[..n]).and_then(|_| ecriture.flush()) {
                    ecriture.fermer();
                    arret_a.fermer();
                    arret_b.fermer();
                    return Err(erreur);
                }
            }
            Err(erreur) if erreur.kind() == io::ErrorKind::Interrupted => continue,
            Err(erreur) => {
                ecriture.fermer();
                arret_a.fermer();
                arret_b.fermer();
                return Err(erreur);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use crate::connexions::Plafond;

    /// Un faux flux : la lecture rend une erreur, un octet, ou EOF selon son état ; l'écriture échoue si demandé ;
    /// `fin_ecriture` et `fermer` posent le même drapeau partagé. Il sert la logique de copie, pas la preuve socket.
    struct Faux {
        read_erreur: bool,
        ecriture_erreur: bool,
        ferme: Arc<Mutex<bool>>,
    }

    impl Faux {
        fn nouveau(read_erreur: bool, ecriture_erreur: bool) -> Self {
            Self { read_erreur, ecriture_erreur, ferme: Arc::new(Mutex::new(false)) }
        }
    }

    impl Read for Faux {
        fn read(&mut self, tampon: &mut [u8]) -> io::Result<usize> {
            if self.read_erreur {
                return Err(io::Error::other("erreur de lecture"));
            }
            if *self.ferme.lock().unwrap() {
                return Ok(0);
            }
            tampon[0] = b'x';
            Ok(1)
        }
    }

    impl Write for Faux {
        fn write(&mut self, tampon: &[u8]) -> io::Result<usize> {
            if self.ecriture_erreur {
                return Err(io::Error::other("erreur d'écriture"));
            }
            Ok(tampon.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Flux for Faux {
        fn clone_flux(&self) -> io::Result<Self> {
            Ok(Self {
                read_erreur: self.read_erreur,
                ecriture_erreur: self.ecriture_erreur,
                ferme: Arc::clone(&self.ferme),
            })
        }

        fn fin_ecriture(&self) {
            *self.ferme.lock().unwrap() = true;
        }

        fn fermer(&self) {
            *self.ferme.lock().unwrap() = true;
        }

        fn delai_lecture(&self, _: Option<Duration>) {}

        fn delai_ecriture(&self, _: Duration) -> io::Result<()> {
            Ok(())
        }
    }

    fn tunnel_avec_plafond<A: Flux, B: Flux>(a: A, b: B) -> (bool, Plafond) {
        let plafond = Plafond::nouveau(1);
        let garde = plafond.prendre().unwrap();
        let (tx, rx) = mpsc::channel();
        let fil = thread::spawn(move || {
            let _garde = garde;
            let resultat = tunnel(a, b);
            let _ = tx.send(resultat.is_err());
        });
        // Un garde-fou, non une attente : la réponse arrive dès que le tunnel rend la main.
        let rendu = rx.recv_timeout(Duration::from_secs(10)).expect("le tunnel ne rend pas la main");
        fil.join().unwrap();
        (rendu, plafond)
    }

    fn place_rendue(plafond: &Plafond) -> bool {
        plafond.prendre().is_some()
    }

    #[test]
    fn le_tunnel_rend_sa_place_sur_une_erreur_de_lecture() {
        let (rendu, plafond) = tunnel_avec_plafond(Faux::nouveau(true, false), Faux::nouveau(false, false));
        assert!(rendu, "le tunnel doit rendre l'erreur");
        assert!(place_rendue(&plafond), "la place du plafond doit être rendue");
    }

    #[test]
    fn le_tunnel_rend_sa_place_sur_une_erreur_d_ecriture() {
        let (rendu, plafond) = tunnel_avec_plafond(Faux::nouveau(false, true), Faux::nouveau(false, false));
        assert!(rendu, "le tunnel doit rendre l'erreur");
        assert!(place_rendue(&plafond), "la place du plafond doit être rendue");
    }

    fn paire_tcp() -> (TcpStream, TcpStream) {
        let ecoute = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let pair = TcpStream::connect(ecoute.local_addr().unwrap()).unwrap();
        let (tunnel, _) = ecoute.accept().unwrap();
        (pair, tunnel)
    }

    #[test]
    fn le_tunnel_rend_sa_place_sur_une_erreur_de_lecture_sur_socket() {
        let (client_pair, client_tunnel) = paire_tcp();
        let (mut amont_pair, amont_tunnel) = paire_tcp();
        let plafond = Plafond::nouveau(1);
        let garde = plafond.prendre().unwrap();
        let (tx, rx) = mpsc::channel();
        let fil = thread::spawn(move || {
            let _garde = garde;
            let resultat = tunnel(client_tunnel, amont_tunnel);
            let _ = tx.send(resultat.is_err());
        });
        amont_pair.write_all(b"x").unwrap();
        client_pair.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut tampon = [0u8; 1];
        client_pair.peek(&mut tampon).unwrap();
        drop(client_pair);
        let rendu = rx.recv_timeout(Duration::from_secs(10)).expect("le tunnel ne rend pas la main");
        assert!(rendu, "le tunnel doit rendre l'erreur");
        fil.join().unwrap();
        assert!(place_rendue(&plafond));
        drop(amont_pair);
    }

    /// Le client ferme ses deux sens sans fermer sa socket, et aucun des deux pairs ne lit. Sa fermeture peut rester
    /// en file derrière ses données, la fenêtre du tunnel étant pleine : le tunnel ne la voit pas, et les deux fils
    /// restent bloqués en écriture (vu sur une VM : 7 passes bloquées sur 20 sans borne). La borne d'écriture les
    /// libère ; elle est courte ici pour que l'essai ne dure pas.
    #[test]
    fn le_tunnel_rend_sa_place_sur_une_erreur_d_ecriture_sur_socket() {
        let (client_pair, client_tunnel) = paire_tcp();
        let (amont_pair, amont_tunnel) = paire_tcp();
        let plafond = Plafond::nouveau(1);
        let garde = plafond.prendre().unwrap();
        let (tx, rx) = mpsc::channel();
        let fil = thread::spawn(move || {
            let _garde = garde;
            let resultat = tunnel_borne(client_tunnel, amont_tunnel, Duration::from_millis(300));
            let _ = tx.send(resultat.is_err());
        });
        let mut client_envoi = client_pair.try_clone().unwrap();
        let _envoi = thread::spawn(move || {
            let _ = client_envoi.write_all(&vec![0u8; 64 * 1024 * 1024]);
        });
        let mut amont_envoi = amont_pair.try_clone().unwrap();
        let _amont = thread::spawn(move || {
            let _ = amont_envoi.write_all(&vec![b'y'; 64 * 1024 * 1024]);
        });
        client_pair.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut tampon = [0u8; 1];
        client_pair.peek(&mut tampon).unwrap();
        client_pair.shutdown(Shutdown::Both).unwrap();
        let rendu = rx.recv_timeout(Duration::from_secs(10)).expect("le tunnel ne rend pas la main");
        assert!(rendu, "le tunnel doit rendre l'erreur");
        fil.join().unwrap();
        assert!(place_rendue(&plafond));
        drop(amont_pair);
    }

    /// Personne ne lit ni ne ferme : le client envoie sans fin, l'amont ne lit pas et se tait. Sans borne, l'aller
    /// reste bloqué pour toujours en écriture vers l'amont, et le retour en lecture de l'amont : seule la borne rend la
    /// main, et la place.
    fn la_borne_seule_rend_la_place<F: Flux>(paire: fn() -> (F, F)) {
        let (client_pair, client_tunnel) = paire();
        let (amont_pair, amont_tunnel) = paire();
        let plafond = Plafond::nouveau(1);
        let garde = plafond.prendre().unwrap();
        let (tx, rx) = mpsc::channel();
        let fil = thread::spawn(move || {
            let _garde = garde;
            let resultat = tunnel_borne(client_tunnel, amont_tunnel, Duration::from_millis(300));
            let _ = tx.send(resultat.is_err());
        });
        let mut client_envoi = client_pair.clone_flux().unwrap();
        let _envoi = thread::spawn(move || {
            let _ = client_envoi.write_all(&vec![0u8; 64 * 1024 * 1024]);
        });
        let rendu = rx.recv_timeout(Duration::from_secs(10)).expect("le tunnel ne rend pas la main");
        assert!(rendu, "la borne doit rendre une erreur d'écriture");
        fil.join().unwrap();
        assert!(place_rendue(&plafond));
        drop((client_pair, amont_pair));
    }

    #[test]
    fn la_borne_seule_rend_la_place_sur_socket() {
        la_borne_seule_rend_la_place(paire_tcp);
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it"]
    fn la_borne_seule_rend_la_place_sur_socket_unix() {
        la_borne_seule_rend_la_place(|| std::os::unix::net::UnixStream::pair().unwrap());
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "requires Unix socket creation; run `cargo test -- --ignored` on a machine that allows it"]
    fn le_tunnel_rend_sa_place_sur_une_erreur_sur_socket_unix() {
        use std::os::unix::net::UnixStream;

        fn paire() -> (UnixStream, UnixStream) {
            UnixStream::pair().unwrap()
        }

        let (client_pair, client_tunnel) = paire();
        let (mut amont_pair, amont_tunnel) = paire();
        let plafond = Plafond::nouveau(1);
        let garde = plafond.prendre().unwrap();
        let (tx, rx) = mpsc::channel();
        let fil = thread::spawn(move || {
            let _garde = garde;
            let resultat = tunnel_borne(client_tunnel, amont_tunnel, Duration::from_millis(300));
            let _ = tx.send(resultat.is_err());
        });
        let mut client_envoi = client_pair.try_clone().unwrap();
        let _envoi = thread::spawn(move || {
            let _ = client_envoi.write_all(&vec![0u8; 64 * 1024 * 1024]);
        });
        amont_pair.write_all(b"x").unwrap();
        client_pair.shutdown(Shutdown::Read).unwrap();
        let rendu = rx.recv_timeout(Duration::from_secs(10)).expect("le tunnel ne rend pas la main");
        assert!(rendu, "le tunnel doit rendre l'erreur");
        fil.join().unwrap();
        assert!(place_rendue(&plafond));
        drop(amont_pair);
    }
}
