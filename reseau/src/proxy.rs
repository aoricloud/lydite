//! Le serveur du filtre : lit une requête HTTP/1.1, décide, puis relaie ou refuse.

use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use crate::adresses::normaliser_hote;
use crate::decision::{Refus, Verdict, decider};
use crate::flux::{Flux, tunnel};
use crate::journal::Journal;

/// La taille maximale de l'en-tête lu, terminaison comprise.
const ENTETE_MAX: usize = 8 * 1024;
/// Le délai de lecture de l'en-tête et de connexion sortante.
pub const DELAI: Duration = Duration::from_secs(5);

/// Le résolveur injecté : un nom → ses adresses. Les tests n'appellent jamais le DNS.
pub type Resolveur = dyn Fn(&str) -> io::Result<Vec<IpAddr>> + Send + Sync;

/// La liste et le résolveur, séparés des sockets.
pub struct Decideur {
    domaines: Vec<String>,
    resoudre: Box<Resolveur>,
}

impl Decideur {
    /// Normalise la liste une fois ; un nom mal formé est une erreur de démarrage.
    pub fn nouveau(domaines: Vec<String>, resoudre: Box<Resolveur>) -> io::Result<Self> {
        let mut normalises = Vec::with_capacity(domaines.len());
        for domaine in domaines {
            let Some(domaine) = normaliser_hote(&domaine) else {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("invalid domain: {domaine}")));
            };
            normalises.push(domaine);
        }
        Ok(Self { domaines: normalises, resoudre })
    }

    /// Décide pour un hôte, en résolvant les noms. L'IP écrite en clair est permise si elle est listée, sans DNS.
    pub fn decider(&self, hote: &str) -> Verdict {
        let Some(hote) = normaliser_hote(hote) else {
            return Verdict::Refuse(Refus::HoteInvalide);
        };
        // Ce contrôle double celui de `decider` : il évite d'appeler le résolveur pour un nom hors liste.
        if !self.domaines.iter().any(|domaine| domaine == &hote) {
            return Verdict::Refuse(Refus::HorsListe);
        }
        if hote.parse::<IpAddr>().is_ok() {
            return decider(&self.domaines, &hote, &[]);
        }
        match (self.resoudre)(&hote) {
            Ok(adresses) => decider(&self.domaines, &hote, &adresses),
            Err(_) => Verdict::Refuse(Refus::ResolutionImpossible),
        }
    }
}

enum ErreurEntete {
    TropLong,
    Delai,
    Incomplet,
    Io,
}

enum Requete {
    Connect { hote: String, port: u16 },
    Absolue { methode: String, version: String, hote: String, port: u16, chemin: String, entetes: Vec<String> },
}

/// Sert une connexion : lit l'en-tête, décide, répond ou relaie. Générique sur le flux pour être testé sur TCP.
pub fn servir<S: Flux>(mut client: S, decideur: &Decideur, journal: &Journal, delai: Duration) -> io::Result<()> {
    let lu = lire_entete(&mut client, delai);
    client.delai_lecture(None);
    let (entete, reste) = match lu {
        Ok(entete) => entete,
        Err(erreur) => {
            let raison = match erreur {
                ErreurEntete::TropLong => "entete-trop-long",
                ErreurEntete::Delai => "delai",
                ErreurEntete::Incomplet | ErreurEntete::Io => "requete-invalide",
            };
            journal_erreur(journal, raison);
            return repondre(&mut client, 400);
        }
    };
    let requete = match analyser(&entete) {
        Ok(requete) => requete,
        Err(_) => {
            journal_erreur(journal, "requete-invalide");
            return repondre(&mut client, 400);
        }
    };
    match requete {
        Requete::Connect { hote, port } => {
            let verdict = decideur.decider(&hote);
            journaliser(journal, &verdict, &hote, port);
            match verdict {
                Verdict::Refuse(refus) => repondre_refus(&mut client, refus),
                Verdict::Permis { adresse } => match connecter(adresse, port) {
                    Ok(mut amont) => {
                        client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
                        client.flush()?;
                        if !reste.is_empty() {
                            amont.write_all(&reste)?;
                            amont.flush()?;
                        }
                        tunnel(client, amont)
                    }
                    Err(_) => {
                        journal_echec(journal, &hote, port);
                        repondre(&mut client, 502)
                    }
                },
            }
        }
        Requete::Absolue { methode, version, hote, port, chemin, entetes } => {
            let verdict = decideur.decider(&hote);
            journaliser(journal, &verdict, &hote, port);
            match verdict {
                Verdict::Refuse(refus) => repondre_refus(&mut client, refus),
                Verdict::Permis { adresse } => match connecter(adresse, port) {
                    Ok(mut amont) => {
                        let ipv6 = hote.parse::<IpAddr>().is_ok_and(|adresse| adresse.is_ipv6());
                        let hote_entete = if ipv6 { format!("[{hote}]") } else { hote.clone() };
                        let hote_entete = if port == 80 { hote_entete } else { format!("{hote_entete}:{port}") };
                        let mut requete =
                            format!("{methode} {chemin} {version}\r\nHost: {hote_entete}\r\nConnection: close\r\n");
                        for ligne in entetes {
                            requete.push_str(&ligne);
                            requete.push_str("\r\n");
                        }
                        requete.push_str("\r\n");
                        amont.write_all(requete.as_bytes())?;
                        amont.flush()?;
                        if !reste.is_empty() {
                            amont.write_all(&reste)?;
                            amont.flush()?;
                        }
                        tunnel(client, amont)
                    }
                    Err(_) => {
                        journal_echec(journal, &hote, port);
                        repondre(&mut client, 502)
                    }
                },
            }
        }
    }
}

fn connecter(adresse: IpAddr, port: u16) -> io::Result<TcpStream> {
    TcpStream::connect_timeout(&SocketAddr::new(adresse, port), DELAI)
}

fn lire_entete<S: Flux>(client: &mut S, delai: Duration) -> Result<(Vec<u8>, Vec<u8>), ErreurEntete> {
    let mut tampon = Vec::new();
    let mut morceau = [0u8; 1024];
    let debut = Instant::now();
    loop {
        // La fin d'en-tête est cherchée avant la borne : un corps déjà lu ne fait pas refuser un en-tête valide.
        if let Some(fin) = position_fin(&tampon) {
            if fin > ENTETE_MAX {
                return Err(ErreurEntete::TropLong);
            }
            let reste = tampon.split_off(fin);
            let entete = tampon[..tampon.len() - 4].to_vec();
            return Ok((entete, reste));
        }
        if tampon.len() >= ENTETE_MAX {
            return Err(ErreurEntete::TropLong);
        }
        let restant = delai.saturating_sub(debut.elapsed());
        if restant.is_zero() {
            return Err(ErreurEntete::Delai);
        }
        client.delai_lecture(Some(restant));
        let n = client.read(&mut morceau).map_err(|erreur| match erreur.kind() {
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ErreurEntete::Delai,
            _ => ErreurEntete::Io,
        })?;
        if n == 0 {
            return Err(ErreurEntete::Incomplet);
        }
        tampon.extend_from_slice(&morceau[..n]);
    }
}

fn position_fin(tampon: &[u8]) -> Option<usize> {
    tampon.windows(4).position(|fenetre| fenetre == b"\r\n\r\n").map(|position| position + 4)
}

fn analyser(entete: &[u8]) -> Result<Requete, Refus> {
    controler_octets(entete)?;
    let texte = std::str::from_utf8(entete).map_err(|_| Refus::HoteInvalide)?;
    let mut lignes = texte.split("\r\n");
    let premiere = lignes.next().ok_or(Refus::HoteInvalide)?;
    let mut morceaux = premiere.split(' ');
    let methode = morceaux.next().unwrap_or_default();
    let cible = morceaux.next().unwrap_or_default();
    let version = morceaux.next().unwrap_or_default();
    if methode.is_empty() || cible.is_empty() || (version != "HTTP/1.1" && version != "HTTP/1.0") {
        return Err(Refus::HoteInvalide);
    }
    if morceaux.next().is_some() {
        return Err(Refus::HoteInvalide);
    }
    let entetes = entetes(lignes)?;
    if methode.eq_ignore_ascii_case("CONNECT") {
        let (hote, port) = autorite(cible, true).ok_or(Refus::HoteInvalide)?;
        Ok(Requete::Connect { hote, port })
    } else {
        let (hote, port, chemin) = absolue(cible).ok_or(Refus::HoteInvalide)?;
        Ok(Requete::Absolue { methode: methode.to_string(), version: version.to_string(), hote, port, chemin, entetes })
    }
}

/// Refuse tout octet de contrôle hors des `\r\n` de séparation ; la tabulation reste permise, comme dans une valeur.
fn controler_octets(entete: &[u8]) -> Result<(), Refus> {
    let mut i = 0;
    while i < entete.len() {
        match entete[i] {
            b'\r' => {
                if entete.get(i + 1) != Some(&b'\n') {
                    return Err(Refus::HoteInvalide);
                }
                i += 2;
            }
            b'\n' => return Err(Refus::HoteInvalide),
            b'\t' => i += 1,
            octet if octet < 0x20 || octet == 0x7f => return Err(Refus::HoteInvalide),
            _ => i += 1,
        }
    }
    Ok(())
}

fn entetes<'a>(lignes: impl Iterator<Item = &'a str>) -> Result<Vec<String>, Refus> {
    let mut gardees = Vec::new();
    for ligne in lignes {
        if ligne.is_empty() {
            continue;
        }
        let Some((nom, _)) = ligne.split_once(':') else {
            return Err(Refus::HoteInvalide);
        };
        // Un nom d'en-tête est un jeton sans blanc : `Host : x` ou une ligne qui commence par un blanc est refusé.
        if nom.is_empty()
            || !nom.bytes().all(|octet| octet.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&octet))
        {
            return Err(Refus::HoteInvalide);
        }
        // Le proxy réécrit ces trois en-têtes (dont `Host`) : les garder les ferait doublonner.
        if nom.eq_ignore_ascii_case("Host")
            || nom.eq_ignore_ascii_case("Connection")
            || nom.eq_ignore_ascii_case("Proxy-Connection")
        {
            continue;
        }
        gardees.push(ligne.to_string());
    }
    Ok(gardees)
}

fn autorite(cible: &str, port_requis: bool) -> Option<(String, u16)> {
    if let Some(reste) = cible.strip_prefix('[') {
        let fin = reste.find(']')?;
        let hote = &reste[..fin];
        let apres = &reste[fin + 1..];
        let port = if apres.is_empty() {
            if port_requis {
                return None;
            }
            80
        } else {
            apres.strip_prefix(':')?.parse().ok()?
        };
        if port == 0 || hote.is_empty() {
            return None;
        }
        return Some((hote.to_string(), port));
    }
    match cible.rsplit_once(':') {
        Some((hote, port)) => {
            if hote.is_empty() || hote.contains(':') {
                return None;
            }
            let port = port.parse().ok()?;
            (port != 0).then(|| (hote.to_string(), port))
        }
        None => {
            if port_requis || cible.is_empty() {
                None
            } else {
                Some((cible.to_string(), 80))
            }
        }
    }
}

fn absolue(cible: &str) -> Option<(String, u16, String)> {
    if !cible.get(..7).is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://")) {
        return None;
    }
    let reste = &cible[7..];
    let fin = reste.find(['/', '?', '#']).unwrap_or(reste.len());
    let morceau = &reste[..fin];
    let suite = &reste[fin..];
    let (hote, port) = autorite(morceau, false)?;
    let chemin = match suite.split('#').next().unwrap_or_default() {
        "" => "/".to_string(),
        chemin if chemin.starts_with('/') => chemin.to_string(),
        chemin => format!("/{chemin}"),
    };
    Some((hote, port, chemin))
}

fn journaliser(journal: &Journal, verdict: &Verdict, hote: &str, port: u16) {
    // Jamais les octets bruts du client : un LF dans l'hôte fabriquerait une seconde ligne de journal.
    let hote = hote_journal(hote);
    match verdict {
        Verdict::Permis { adresse } => journal.noter(&format!("permis {hote} {port} {adresse} liste")),
        Verdict::Refuse(refus) => journal.noter(&format!("refuse {hote} {port} {}", nom_refus(*refus))),
    }
}

fn journal_erreur(journal: &Journal, raison: &str) {
    journal.noter(&format!("refuse - - {raison}"));
}

fn journal_echec(journal: &Journal, hote: &str, port: u16) {
    let hote = hote_journal(hote);
    journal.noter(&format!("echec {hote} {port} injoignable"));
}

fn hote_journal(hote: &str) -> String {
    normaliser_hote(hote).unwrap_or_else(|| "-".to_string())
}

fn nom_refus(refus: Refus) -> &'static str {
    match refus {
        Refus::HoteInvalide => "hote-invalide",
        Refus::HorsListe => "hors-liste",
        Refus::AdresseRefusee => "adresse-refusee",
        Refus::ResolutionImpossible => "resolution-impossible",
    }
}

fn repondre_refus<S: Write>(client: &mut S, refus: Refus) -> io::Result<()> {
    let (code, entete, corps) = match refus {
        Refus::HorsListe => {
            ("403 Forbidden", "X-Proxy-Error: blocked-by-allowlist\r\n", "Connection blocked by network allowlist")
        }
        Refus::AdresseRefusee => (
            "403 Forbidden",
            "X-Proxy-Error: blocked-by-resolved-address\r\n",
            "Connection blocked by resolved address",
        ),
        Refus::HoteInvalide => ("400 Bad Request", "", "Bad Request"),
        Refus::ResolutionImpossible => ("502 Bad Gateway", "", "Bad Gateway"),
    };
    let reponse = format!("HTTP/1.1 {code}\r\nContent-Type: text/plain\r\n{entete}\r\n{corps}");
    client.write_all(reponse.as_bytes())?;
    client.flush()
}

fn repondre<S: Flux>(client: &mut S, code: u16) -> io::Result<()> {
    let (code, corps) = match code {
        400 => ("400 Bad Request", "Bad Request"),
        _ => ("502 Bad Gateway", "Bad Gateway"),
    };
    let reponse = format!("HTTP/1.1 {code}\r\nContent-Type: text/plain\r\n\r\n{corps}");
    client.write_all(reponse.as_bytes())?;
    client.flush()?;
    // Ferme l'écriture, puis lit et jette une courte suite : le noyau ne coupe pas la réponse par un RST.
    client.fin_ecriture();
    client.delai_lecture(Some(Duration::from_millis(50)));
    let mut tampon = [0u8; 512];
    for _ in 0..4 {
        match client.read(&mut tampon) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
    Ok(())
}
