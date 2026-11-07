//! La main Claude Code : son processus, son dialogue en stream-json et son dossier temporaire.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{self, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::branchements::bac::temporaire_de;
use crate::branchements::git::IDENTITE;
use crate::branchements::processus::{SORTIE, attendre, echeance, retire_de_l_env, sans_session, tuer_arbre};
use crate::coeur::erreur::queue;
use crate::coeur::protocole::{self, Attendu, Constat, DEMANDES, Ligne};

/// Le temps qu'ont les quatre réponses de contrôle, puis l'ouverture, chacune, jamais plus que ce qui reste du délai de
/// la main.
// raccourci : 60 s pour l'ouverture, vue en moins de 2 s ; monter dans le profil si une machine lente le demande
const BORNE: Duration = Duration::from_secs(60);

/// Joue la main une fois dans le bac, sous `delai` : Ok(sortie) si elle finit sans erreur, sinon Err(erreur), avec le
/// constat de ce que Kraken a lu d'elle, même sur un écart. Sans `attendu`, la main est brute : sa
/// consigne en texte sur son entrée, sa sortie telle quelle, ses textes d'erreur, aucun constat. Avec, elle parle en
/// stream-json (voir protocole.rs) : ses réponses de contrôle se jugent contre `attendu` avant sa consigne, son
/// ouverture dès qu'elle arrive, et sa sortie est le texte de son résultat. Au délai, sur un écart ou à une borne, son
/// arbre meurt ; une erreur de délai commence par « main : temps dépassé (N s) ». `kraken` met le dossier de
/// l'exécutable de Kraken en tête de son `PATH` ([`path_de_kraken`]). `fournisseur` est l'URL de l'API de la main, sa
/// clé et le nom de la variable qui la tient, s'il y en a : ils vont à l'environnement de la main seule ([`partir`]).
pub(crate) fn jouer(
    argv: &[String],
    bac: &Path,
    consigne: &str,
    delai: Duration,
    attendu: Option<&Attendu>,
    kraken: bool,
    fournisseur: Option<(&str, &str, &str)>,
) -> (Option<Constat>, Result<String, String>) {
    let mut constat = Constat::default();
    let jeu = partir(argv, bac, delai, kraken, fournisseur).and_then(|mut main| match attendu {
        None => main.brute(consigne),
        Some(attendu) => main.dialoguer(attendu, consigne, &mut constat),
    });
    (attendu.map(|_| constat), jeu)
}

/// D'où vient une ligne de la main.
#[derive(Clone, Copy)]
enum Voie {
    Sortie,
    Erreur,
}

/// Ce qu'un fil de lecture rapporte : une ligne de sa voie, `\n` compris, ou `None` à la fin de la voie.
type Lu = (Voie, Option<Vec<u8>>);

/// L'échéance d'une attente est passée.
struct Echue;

/// Une main partie : son processus, son entrée, ce que ses fils de lecture rapportent, et la fin de son délai.
struct Partie {
    enfant: Child,
    /// Ce qui part sur son entrée : l'entrée se ferme quand il tombe, une fois tout écrit.
    entree: Option<Sender<String>>,
    lu: Receiver<Lu>,
    erreurs: Vec<u8>,
    sortie_finie: bool,
    erreurs_finies: bool,
    fin: Instant,
    delai: Duration,
}

/// Lance la main dans le bac, sous `delai` : sa sortie et ses erreurs lues ligne à ligne dans des fils d'exécution, son
/// entrée écrite dans un autre, son dossier temporaire à côté du bac, l'identité de Kraken pour ses commits, sans les
/// variables que [`retire_de_l_env`] désigne, et, avec `kraken`, le `PATH` de [`path_de_kraken`]. Avec `fournisseur`,
/// `(URL de l'API, clé, nom de la variable qui tient la clé)`, elle reçoit `ANTHROPIC_BASE_URL` et `ANTHROPIC_API_KEY`,
/// et perd cette variable et un `ANTHROPIC_AUTH_TOKEN` hérité : le jeton de l'humain ne va pas chez un autre
/// fournisseur, et la clé ne va qu'à la main. L'ordre compte : la variable est retirée avant que `ANTHROPIC_API_KEY` ne
/// soit posée, car le profil peut nommer `ANTHROPIC_API_KEY` elle-même. Sans fournisseur, rien de cela ne change. Un
/// argv vide est une erreur, pas une panique.
#[expect(clippy::expect_used, reason = "tuyaux demandés au lancement de l'enfant")]
fn partir(
    argv: &[String],
    bac: &Path,
    delai: Duration,
    kraken: bool,
    fournisseur: Option<(&str, &str, &str)>,
) -> Result<Partie, String> {
    let fin = echeance(delai);
    let Some((programme, args)) = argv.split_first() else {
        return Err("main : aucun programme à lancer, la commande de la main est vide".into());
    };
    let mut commande = Command::new(programme);
    commande.args(args).current_dir(bac).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Ses commits sont ceux de Kraken : ni le nom ni l'adresse de l'humain.
    commande.envs(IDENTITE);
    if let Some((api, cle, nom)) = fournisseur {
        commande
            .env_remove("ANTHROPIC_AUTH_TOKEN")
            .env_remove(nom)
            .env("ANTHROPIC_BASE_URL", api)
            .env("ANTHROPIC_API_KEY", cle);
    }
    if kraken {
        commande.env("PATH", path_de_kraken()?);
    }
    if let Some(temporaire) = temporaire(bac, false)? {
        let noms: &[&str] = if cfg!(windows) { &["TEMP", "TMP"] } else { &["TMPDIR"] };
        for nom in noms {
            commande.env(nom, &temporaire);
        }
    }
    let mut enfant = sans_session(&mut commande, retire_de_l_env)
        .spawn()
        .map_err(|e| format!("lancement impossible de {programme} : {e}"))?;
    let (envoi, lu) = mpsc::channel();
    lire_en_fond(enfant.stdout.take().expect("stdout en tuyau"), Voie::Sortie, envoi.clone());
    lire_en_fond(enfant.stderr.take().expect("stderr en tuyau"), Voie::Erreur, envoi);
    let entree = Some(ecrivain(enfant.stdin.take().expect("stdin en tuyau")));
    Ok(Partie { enfant, entree, lu, erreurs: Vec::new(), sortie_finie: false, erreurs_finies: false, fin, delai })
}

/// Le dossier temporaire de la main ([`temporaire_de`]), créé par un `create_dir` qui ne crée jamais de parent, une
/// fois vu que le bac est un dossier qui existe. Avec `vider` (sous un mur exigé), il est recréé vide à chaque
/// tentative, la relance comprise : celui d'une tentative d'avant s'efface d'abord. Sans, il reste tel quel s'il est
/// là, pour qu'un fichier tenu sous Windows ne fasse pas échouer une relance. `None` pour un bac absent : la main n'y
/// part pas, son lancement le dit, et rien n'est créé. Un bac sans nom de dossier est une erreur.
pub(crate) fn temporaire(bac: &Path, vider: bool) -> Result<Option<PathBuf>, String> {
    if !bac.is_dir() {
        return Ok(None);
    }
    let absolu = path::absolute(bac).map_err(|e| format!("main : chemin absolu du bac {} : {e}", bac.display()))?;
    let Some(temporaire) = temporaire_de(&absolu) else {
        return Err(format!("main : le bac {} n'a pas de nom, son dossier temporaire ne se forme pas", bac.display()));
    };
    // raccourci : seul le rangement d'une vague à --pr efface le dossier temporaire, avec le bac d'une tâche assemblée
    // dont le travail est poussé (bac::retirer) ; tout autre reste avec son bac ; monter quand Kraken rangera aussi les
    // vagues sans --pr
    let impossible = |e: io::Error| format!("main : dossier temporaire {} impossible : {e}", temporaire.display());
    if temporaire.is_dir() {
        if !vider {
            return Ok(Some(temporaire));
        }
        fs::remove_dir_all(&temporaire).map_err(impossible)?;
    }
    fs::create_dir(&temporaire).map_err(impossible)?;
    Ok(Some(temporaire))
}

/// Le `PATH` d'une main dont la preuve prêtée est une commande de Kraken (voir `Juge`) : le dossier de l'exécutable de
/// Kraken, puis le `PATH` de Kraken tel quel, à l'octet près. C'est le seul droit qu'elle a de plus qu'une autre main.
/// Un exécutable introuvable, ou un dossier qui ne s'écrit pas dans un `PATH` (un `"` sous Windows, un `:` ailleurs),
/// est une erreur : la main ne part pas.
fn path_de_kraken() -> Result<OsString, String> {
    let exe = env::current_exe().map_err(|e| format!("main : l'exécutable de Kraken ne se trouve pas : {e}"))?;
    let Some(dossier) = exe.parent() else {
        return Err(format!("main : l'exécutable de Kraken {} n'a pas de dossier", exe.display()));
    };
    let mut path = env::join_paths([dossier])
        .map_err(|e| format!("main : le dossier de Kraken {} ne s'écrit pas dans un PATH : {e}", dossier.display()))?;
    if let Some(ancien) = env::var_os("PATH").filter(|ancien| !ancien.is_empty()) {
        path.push(if cfg!(windows) { ";" } else { ":" });
        path.push(ancien);
    }
    Ok(path)
}

/// Ce qui écrit sur l'entrée de la main, dans un fil à lui : une écriture qui bloque, devant une main qui ne lit pas,
/// ne retient pas Kraken. L'entrée se ferme quand l'envoi tombe, une fois tout écrit, ou dès qu'une écriture rate.
fn ecrivain(mut entree: ChildStdin) -> Sender<String> {
    let (envoi, recu) = mpsc::channel::<String>();
    thread::spawn(move || {
        for texte in recu {
            // Une main qui meurt sans lire casse le tuyau ; sa sortie le dira.
            if entree.write_all(texte.as_bytes()).is_err() {
                break;
            }
        }
    });
    envoi
}

/// Lit `flux` ligne à ligne dans un fil à lui, chaque ligne envoyée telle quelle, puis la fin de la voie.
fn lire_en_fond(flux: impl Read + Send + 'static, voie: Voie, envoi: Sender<Lu>) {
    thread::spawn(move || {
        let mut lecteur = BufReader::new(flux);
        loop {
            let mut ligne = Vec::new();
            match lecteur.read_until(b'\n', &mut ligne) {
                Ok(n) if n > 0 => {
                    if envoi.send((voie, Some(ligne))).is_err() {
                        return;
                    }
                }
                // La fin de la voie, ou une lecture ratée.
                _ => break,
            }
        }
        let _ = envoi.send((voie, None));
    });
}

impl Partie {
    /// Attend un rapport des fils de lecture jusqu'à l'échéance et le range : une ligne d'erreur s'amasse, la fin d'une
    /// voie se note. Rend la ligne de sortie qu'il porte, `None` s'il n'en porte pas, `Err(Echue)` si rien n'est venu
    /// avant l'échéance. Passée l'échéance, plus rien ne se lit : une main qui écrit sans cesse n'y échappe pas.
    fn recevoir(&mut self, echeance: Instant) -> Result<Option<Vec<u8>>, Echue> {
        let reste = echeance.saturating_duration_since(Instant::now());
        if reste.is_zero() {
            return Err(Echue);
        }
        match self.lu.recv_timeout(reste) {
            Ok((Voie::Sortie, Some(ligne))) => return Ok(Some(ligne)),
            Ok((Voie::Sortie, None)) => self.sortie_finie = true,
            Ok((Voie::Erreur, Some(ligne))) => self.erreurs.extend(ligne),
            Ok((Voie::Erreur, None)) => self.erreurs_finies = true,
            Err(RecvTimeoutError::Timeout) => return Err(Echue),
            // Les deux fils sont partis : rien ne viendra plus.
            Err(RecvTimeoutError::Disconnected) => (self.sortie_finie, self.erreurs_finies) = (true, true),
        }
        Ok(None)
    }

    /// La prochaine ligne de sa sortie avant l'échéance, `None` à la fin de sa sortie.
    fn ligne(&mut self, echeance: Instant) -> Result<Option<Vec<u8>>, Echue> {
        while !self.sortie_finie {
            if let Some(ligne) = self.recevoir(echeance)? {
                return Ok(Some(ligne));
            }
        }
        Ok(None)
    }

    /// Lit ses deux voies jusqu'à leur fin, ou jusqu'à l'échéance : ses erreurs s'amassent, le reste de sa sortie se
    /// perd.
    fn vider(&mut self, echeance: Instant) {
        while !(self.sortie_finie && self.erreurs_finies) && self.recevoir(echeance).is_ok() {}
    }

    /// Son statut, attendu jusqu'à la fin de son délai, puis la fin de ses voies, 10 s au plus. Au délai, son arbre
    /// meurt.
    fn statut(&mut self) -> Result<ExitStatus, String> {
        match attendre(&mut self.enfant, self.fin) {
            Ok(Some(statut)) => {
                self.vider(Instant::now() + SORTIE);
                Ok(statut)
            }
            Ok(None) => Err(self.temps_depasse()),
            Err(e) => Err(format!("attente impossible : {e}")),
        }
    }

    /// Tue son arbre, avant tout `wait`, puis attend sa sortie et la fin de ses voies, 10 s au plus chacune. Rend ce
    /// qui suit l'erreur : rien, ou l'échec de la mise à mort.
    fn tuer(&mut self) -> String {
        let tuee = tuer_arbre(self.enfant.id());
        let _ = self.enfant.kill();
        self.entree = None;
        let _ = attendre(&mut self.enfant, Instant::now() + SORTIE);
        self.vider(Instant::now() + SORTIE);
        tuee.err().map_or_else(String::new, |e| format!(" ; mise à mort : {e}"))
    }

    /// L'erreur d'une main tuée au délai : « main : temps dépassé (N s) », hors de la coupe de `queue`, puis la fin de
    /// ses erreurs et l'échec de sa mise à mort, s'il y en a.
    fn temps_depasse(&mut self) -> String {
        let mise_a_mort = self.tuer();
        let suite = self.avec_erreurs(String::new()) + &mise_a_mort;
        format!("main : temps dépassé ({} s){}", self.delai.as_secs_f32(), queue(&suite))
    }

    fn avec_erreurs(&self, debut: String) -> String {
        let erreurs = String::from_utf8_lossy(&self.erreurs);
        let erreurs = erreurs.trim();
        if erreurs.is_empty() { debut } else { format!("{debut} : {erreurs}") }
    }

    /// La main brute : sa consigne en texte sur son entrée, qui se ferme derrière elle, puis toute sa sortie.
    /// Ok(sortie) si elle sort sans erreur, sinon Err(son statut, ses erreurs puis sa sortie).
    fn brute(&mut self, consigne: &str) -> Result<String, String> {
        if let Some(entree) = self.entree.take() {
            let _ = entree.send(consigne.into());
        }
        let mut sortie = Vec::new();
        loop {
            match self.ligne(self.fin) {
                Ok(Some(ligne)) => sortie.extend(ligne),
                Ok(None) => break,
                Err(Echue) => return Err(self.temps_depasse()),
            }
        }
        let statut = self.statut()?;
        let texte = String::from_utf8_lossy(&sortie).into_owned();
        if !statut.success() {
            let erreur = String::from_utf8_lossy(&self.erreurs);
            return Err(queue(&format!("{statut} ; {erreur}{texte}")));
        }
        Ok(texte)
    }

    /// La main en stream-json (voir protocole.rs) : ses demandes de contrôle, leurs réponses jugées, sa consigne, son
    /// ouverture jugée dès qu'elle arrive, puis son résultat. Les réponses, puis l'ouverture, ont chacune [`BORNE`] au
    /// plus, jamais plus que ce qui reste du délai. Au moindre écart, son arbre meurt, et sa consigne ne part pas si
    /// elle n'est pas encore partie. `constat` garde ce qui s'est lu d'elle, même sur un écart.
    fn dialoguer(&mut self, attendu: &Attendu, consigne: &str, constat: &mut Constat) -> Result<String, String> {
        if let Some(entree) = &self.entree {
            let _ = entree.send(protocole::demandes());
        }
        let mut reponses = BTreeMap::new();
        let mut messages = protocole::Messages::new();
        // Ce qu'on attend a sa borne : les réponses, puis l'ouverture ; plus rien ensuite.
        let mut borne = Some(Instant::now() + BORNE);
        let (mut partie, mut ouverte) = (false, false);
        loop {
            let echeance = borne.map_or(self.fin, |borne| borne.min(self.fin));
            let ligne = match self.ligne(echeance) {
                Ok(Some(ligne)) => ligne,
                Ok(None) => {
                    let statut = self.statut()?;
                    let quoi = if partie { "aucun résultat" } else { "sortie avant ses réponses de contrôle" };
                    return Err(queue(&self.avec_erreurs(format!("main : {quoi} ({statut})"))));
                }
                // Le délai tombe avant la borne : c'est l'erreur du délai.
                Err(Echue) if borne.is_some_and(|borne| borne < self.fin) => {
                    let manque = if partie {
                        format!("aucune ouverture en {} s", BORNE.as_secs())
                    } else {
                        let ids: Vec<&str> = DEMANDES.into_iter().filter(|id| !reponses.contains_key(*id)).collect();
                        format!("aucune réponse de contrôle en {} s : {}", BORNE.as_secs(), ids.join(", "))
                    };
                    return Err(format!("main : {manque}{}", self.tuer()));
                }
                Err(Echue) => return Err(self.temps_depasse()),
            };
            match protocole::lire_ligne(ligne.strip_suffix(b"\n").unwrap_or(&ligne)) {
                // Avant l'ouverture, une ligne illisible est un écart ; après, seule compte la ligne du résultat.
                Err(ecart) if !ouverte => return Err(format!("{ecart}{}", self.tuer())),
                Ok(Ligne::Reponse(id, reponse)) if !partie => {
                    reponses.insert(id, reponse);
                }
                Ok(Ligne::Ouverture(ouverture)) if !ouverte => {
                    let (version, jugement) = protocole::juger_ouverture(attendu, &ouverture);
                    constat.version = version;
                    constat.source_cle = protocole::source_cle(&ouverture);
                    if let Err(ecart) = jugement {
                        return Err(format!("{ecart}{}", self.tuer()));
                    }
                    ouverte = true;
                    if partie {
                        borne = None;
                    }
                }
                // Une main tuée avant son résultat garde le cumul de ses lignes `assistant`, un usage par message.
                Ok(Ligne::Assistant(assistant)) if ouverte => {
                    protocole::cumuler(&mut constat.usage, &mut messages, &assistant);
                }
                Ok(Ligne::Resultat(resultat)) if ouverte && partie => {
                    // Un `result` sans usage (jetons, dépense ou modèle) garde le cumul des `assistant`, dont il
                    // reprend pourtant la durée et les tours.
                    let usage = protocole::usage_du_resultat(&resultat);
                    if !usage.est_vide() || constat.usage.est_vide() {
                        constat.usage = usage;
                    } else {
                        constat.usage.duree_ms = usage.duree_ms;
                        constat.usage.tours = usage.tours;
                    }
                    return self.finir(protocole::resultat(&resultat));
                }
                _ => {}
            }
            if !partie && DEMANDES.iter().all(|id| reponses.contains_key(*id)) {
                let (mur_actif, jugement) = protocole::juger_reponses(attendu, &reponses);
                constat.mur_actif = mur_actif;
                if let Err(ecart) = jugement {
                    return Err(format!("{ecart}{}", self.tuer()));
                }
                // La consigne part, et l'entrée se ferme derrière elle.
                if let Some(entree) = self.entree.take() {
                    let _ = entree.send(protocole::message(consigne));
                }
                partie = true;
                borne = (!ouverte).then(|| Instant::now() + BORNE);
            }
        }
    }

    /// La fin d'une main qui a rendu son résultat : 10 s pour sortir, sinon son arbre meurt, et le résultat compte. Une
    /// main qui sort d'elle-même en erreur est rouge.
    fn finir(&mut self, resultat: Result<String, String>) -> Result<String, String> {
        let statut = match attendre(&mut self.enfant, Instant::now() + SORTIE) {
            Ok(Some(statut)) => Some(statut),
            _ => {
                let _ = self.tuer();
                None
            }
        };
        self.vider(Instant::now() + SORTIE);
        let sortie = resultat.map_err(|erreur| queue(&erreur))?;
        match statut {
            Some(statut) if !statut.success() => {
                Err(queue(&self.avec_erreurs(format!("main : sortie en erreur ({statut})"))))
            }
            _ => Ok(sortie),
        }
    }
}

#[cfg(test)]
mod tests;
