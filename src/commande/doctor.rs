//! `lydite doctor [<profil>]` : ce qui manque à quelqu'un qui installe Lydite pour lancer une vague, et pour chaque
//! manque quoi faire. Elle ne lance aucune main et n'écrit rien, hors ce qu'écrit déjà l'épreuve du mur de `preparer`.
//!
//! Chaque contrôle écrit une ligne sur stdout, `<état> <contrôle>: <détail>`, l'état valant `ok`, `missing` ou
//! `skipped` ; une ligne `missing` finit par ` Fix: <ce qu'il faut faire>`. La dernière ligne dit `ready`, ou
//! `not ready: <n> missing`. Le code de sortie vaut 0 si prêt, 1 sinon.
//!
//! Un programme qu'on lance ne montre rien de ce qu'il écrit, hors la première ligne de ses `--version` : `claude auth
//! status` écrit le courriel et l'organisation du compte, `gh auth status` son nom, et seul leur code de sortie
//! compte. Chaque appel est borné à [`DELAI`] : un programme qui ne se termine pas est un manque.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt::Display;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::branchements::bac::temporaire_de;
use crate::branchements::dossier;
use crate::branchements::github::programme_gh;
use crate::branchements::natif;
use crate::branchements::processus::{attendre, echeance, tuer_et_attendre};
use crate::branchements::srt::PREUVE_DU_MUR_NATIF;
use crate::coeur::erreur::{Code, Refus};
use crate::coeur::profil::MurDuRejeu;
use crate::coeur::texte::en_ligne;
use crate::conduite::mener::preparer;

/// Le temps qu'a chaque programme pour répondre.
const DELAI: Duration = Duration::from_secs(30);

/// La longueur d'un chemin de socket, que Claude Code ne dépasse pas : 107 octets.
const SOCKET_MAX: usize = 107;

/// Le socket de pont que Claude Code ouvre dans le dossier temporaire d'un bac : `claude-http-` et 16 chiffres
/// hexadécimaux.
const SOCKET: &str = "claude-http-0123456789abcdef.sock";

/// Les programmes que `doctor` lance, et le temps de chacun.
pub(crate) struct Programmes {
    pub(crate) git: OsString,
    pub(crate) gh: OsString,
    pub(crate) claude: OsString,
    pub(crate) srt: OsString,
    /// Le `bwrap` du mur natif (`LYDITE_BWRAP`), que `doctor` ne lance que pour un profil `mur = natif`.
    pub(crate) bwrap: OsString,
    /// Le chemin de `lydite-reseau` (`LYDITE_RESEAU`, sinon à côté de `lydite`), ou pourquoi il n'y en a pas : `doctor`
    /// ne le regarde que pour un profil `mur = natif`.
    pub(crate) reseau: Result<PathBuf, String>,
    pub(crate) delai_git: Duration,
    pub(crate) delai_gh: Duration,
    /// Le délai de `claude --version` et de `claude auth status`.
    pub(crate) delai_claude: Duration,
    pub(crate) delai_srt: Duration,
    pub(crate) delai_bwrap: Duration,
}

/// Ce que `doctor` retient d'un profil que `preparer` a accepté.
pub(crate) struct ProfilVu {
    /// Le profil nomme un fournisseur : sa main n'a pas besoin d'un compte Claude.
    pub(crate) fournisseur: bool,
    /// Le profil exige le mur, que `preparer` a éprouvé.
    pub(crate) mur: bool,
    /// Le mur est le mur natif (`mur = natif`) : bubblewrap et `lydite-reseau` à la place de srt.
    pub(crate) natif: bool,
}

/// Comment un programme a fini.
enum Appel {
    /// Il n'a pas pu partir : le texte de l'erreur.
    NonParti(String),
    /// Il n'a pas fini avant le délai ; la mise à mort, si elle a échoué, s'y dit.
    Depasse(String),
    /// Il est sorti : son code (`None` s'il a été tué) et la première ligne de sa sortie, vide si on ne la lit pas.
    Fini { code: Option<i32>, ligne: String },
}

/// Lance le programme sans coquille, entrée et erreurs vers `Stdio::null()`, et sa sortie aussi sauf si `lire` : sa
/// première ligne, 4096 octets au plus, se lit alors.
fn jouer(programme: &OsStr, args: &[&str], delai: Duration, lire: bool) -> Appel {
    let mut commande = Command::new(programme);
    commande.args(args).stdin(Stdio::null()).stderr(Stdio::null());
    commande.stdout(if lire { Stdio::piped() } else { Stdio::null() });
    let mut enfant = match commande.spawn() {
        Ok(enfant) => enfant,
        Err(e) => return Appel::NonParti(e.to_string()),
    };
    // La sortie se lit à part : un programme qui en écrit beaucoup ne se bloque pas, et ne retient pas `doctor`.
    let lecteur = enfant.stdout.take().map(|sortie| {
        let (envoi, reception) = mpsc::channel();
        thread::spawn(move || {
            let mut octets = Vec::new();
            let _ = sortie.take(4096).read_to_end(&mut octets);
            let _ = envoi.send(octets);
        });
        reception
    });
    let echeance = echeance(delai);
    match attendre(&mut enfant, echeance) {
        Ok(Some(statut)) => {
            // Le fil lecteur s'attend jusqu'à la même échéance que le programme : sous charge, il peut tarder.
            let reste = echeance.saturating_duration_since(Instant::now());
            let octets = lecteur.and_then(|reception| reception.recv_timeout(reste).ok());
            let octets = octets.unwrap_or_default();
            let ligne = String::from_utf8_lossy(&octets).lines().next().unwrap_or("").trim().to_string();
            Appel::Fini { code: statut.code(), ligne: en_ligne(&ligne) }
        }
        Ok(None) => {
            // Le texte de l'échec est cité tel quel : il peut être en français, la phrase reste en anglais.
            let mort = tuer_et_attendre(&mut enfant)
                .err()
                .map_or_else(String::new, |e| format!(" ; stopping it failed: \"{e}\""));
            Appel::Depasse(mort)
        }
        Err(e) => {
            let _ = tuer_et_attendre(&mut enfant);
            Appel::NonParti(format!("waiting for it failed: {e}"))
        }
    }
}

/// Les lignes de `doctor` et le nombre de manques.
#[derive(Default)]
struct Rapport {
    lignes: Vec<String>,
    manques: usize,
}

impl Rapport {
    fn ok(&mut self, controle: &str, detail: impl Display) {
        self.lignes.push(format!("ok {controle}: {detail}"));
    }

    fn saute(&mut self, controle: &str, detail: impl Display) {
        self.lignes.push(format!("skipped {controle}: {detail}"));
    }

    fn manque(&mut self, controle: &str, detail: impl Display, fix: impl Display) {
        self.manques += 1;
        self.lignes.push(format!("missing {controle}: {detail} Fix: {fix}"));
    }
}

/// Un contrôle qui lance un programme et ne juge que son code de sortie.
struct Controle<'a> {
    nom: &'a str,
    programme: &'a OsStr,
    args: &'a [&'a str],
    /// Le détail de `ok`, ou, sans lui, la première ligne de la sortie du programme.
    ok: Option<&'a str>,
    /// Quoi faire s'il ne part pas.
    absent: &'a str,
    /// Le détail et quoi faire s'il sort avec un code autre que 0 ; sans détail, le code est dit.
    rouge: (Option<&'a str>, &'a str),
}

impl Controle<'_> {
    fn jouer(&self, delai: Duration, rapport: &mut Rapport) {
        let commande = format!("{} {}", self.programme.to_string_lossy(), self.args.join(" "));
        match jouer(self.programme, self.args, delai, self.ok.is_none()) {
            Appel::Fini { code: Some(0), ligne } => {
                let detail = self
                    .ok
                    .map_or_else(|| if ligne.is_empty() { "no version printed".into() } else { ligne }, str::to_string);
                rapport.ok(self.nom, detail);
            }
            Appel::Fini { code, .. } => {
                let dit =
                    code.map_or_else(|| "was stopped by a signal".into(), |code| format!("exited with code {code}"));
                let detail = self.rouge.0.map_or_else(|| format!("`{commande}` {dit}"), str::to_string);
                rapport.manque(self.nom, detail, self.rouge.1);
            }
            Appel::NonParti(e) => rapport.manque(self.nom, format!("`{commande}` could not start: {e}."), self.absent),
            Appel::Depasse(mort) => {
                let secondes = delai.as_secs();
                let detail = format!("`{commande}` did not answer in {secondes} s{mort}.");
                rapport.manque(self.nom, detail, format!("run `{commande}` yourself to see why."));
            }
        }
    }
}

/// Le dossier de Lydite, et, hors de Windows, que le socket de pont que Claude Code ouvrirait dans un bac y tienne.
fn dossier_de_lydite(rapport: &mut Rapport, dossier: &Result<PathBuf, Refus>, windows: bool) {
    let dossier = match dossier {
        Ok(dossier) => dossier,
        Err(refus) => return rapport.manque("state folder", en_ligne(&refus.preuve), en_ligne(&refus.correction)),
    };
    let longueur = dossier.as_os_str().as_encoded_bytes().len();
    let socket = temporaire_de(&dossier.join("bacs").join("x")).map(|temporaire| temporaire.join(SOCKET));
    if let Some(socket) = socket.filter(|_| !windows) {
        let total = socket.as_os_str().as_encoded_bytes().len();
        if total > SOCKET_MAX {
            let permis = SOCKET_MAX.saturating_sub(total - longueur);
            let detail = format!(
                "{} is {longueur} bytes long and may not exceed {permis}: the bridge socket Claude Code opens in a \
                 sandbox below it would be {total} bytes, over the {SOCKET_MAX} a socket path holds.",
                dossier.display()
            );
            return rapport.manque("state folder", detail, "set LYDITE_DOSSIER to a shorter absolute path.");
        }
    }
    let neuf = if dossier.exists() { "" } else { " (created at the first launch)" };
    rapport.ok("state folder", format!("{}{neuf}", dossier.display()));
}

/// Le profil nommé est-il un profil `mur = natif` ? Un profil que `preparer` a accepté le dit ; un profil que le mur
/// natif a refusé (6028, dont la preuve commence par [`PREUVE_DU_MUR_NATIF`]) l'est aussi : `doctor` dit alors ce qui
/// manque à ce mur, et ne demande pas srt. Un autre refus ne dit rien du mur.
fn mur_natif(vu: &Result<ProfilVu, Refus>) -> bool {
    match vu {
        Ok(vu) => vu.natif,
        Err(refus) => refus.code == Code::MurRefuse && refus.preuve.starts_with(PREUVE_DU_MUR_NATIF),
    }
}

/// Les contrôles du mur natif, à la place de srt : `bwrap` part, et `lydite-reseau` est un fichier là où Lydite le
/// cherche. Chaque manque dit quoi faire.
fn mur_natif_present(programmes: &Programmes, rapport: &mut Rapport) {
    let installer = "install bubblewrap (Linux only; on Debian and Ubuntu, `apt install bubblewrap`) or name its path \
                     in LYDITE_BWRAP.";
    Controle {
        nom: "bwrap",
        programme: &programmes.bwrap,
        args: &["--version"],
        ok: None,
        absent: installer,
        rouge: (None, installer),
    }
    .jouer(programmes.delai_bwrap, rapport);
    let construire = "build it (`cargo build --release --manifest-path reseau/Cargo.toml`) and put it next to the \
                      `lydite` executable, or name its path in LYDITE_RESEAU.";
    match &programmes.reseau {
        Ok(chemin) if chemin.is_file() => rapport.ok("lydite-reseau", chemin.display()),
        Ok(chemin) => rapport.manque("lydite-reseau", format!("{} is not a file.", chemin.display()), construire),
        Err(_) => {
            let detail = "there is no path to it: LYDITE_RESEAU is not set, and the folder of the Lydite executable is \
                          unknown.";
            rapport.manque("lydite-reseau", detail, construire);
        }
    }
}

/// Joue les contrôles dans l'ordre et rend leurs lignes, dont la dernière est le verdict, avec le code de sortie : 0 si
/// rien ne manque, 1 sinon. `dossier` est ce que rend `dossier::dossier()`, `profil` le profil nommé et ce que
/// `preparer` en a dit. Rien ne lit l'environnement : `doctor_cmd` le lit.
pub(crate) fn verifier(
    programmes: &Programmes,
    dossier: &Result<PathBuf, Refus>,
    profil: Option<(&str, &Result<ProfilVu, Refus>)>,
    windows: bool,
) -> (Vec<String>, u8) {
    let mut rapport = Rapport::default();
    let installer_git = "install git.";
    Controle {
        nom: "git",
        programme: &programmes.git,
        args: &["--version"],
        ok: None,
        absent: installer_git,
        rouge: (None, installer_git),
    }
    .jouer(programmes.delai_git, &mut rapport);
    Controle {
        nom: "gh",
        programme: &programmes.gh,
        args: &["auth", "status"],
        ok: Some("logged in"),
        absent: "install gh (https://cli.github.com) or name its path in LYDITE_GH.",
        rouge: (Some("not logged in."), "run `gh auth login`."),
    }
    .jouer(programmes.delai_gh, &mut rapport);
    let installer_claude = "install Claude Code (https://claude.com/claude-code) or name its path in LYDITE_CLAUDE.";
    Controle {
        nom: "claude",
        programme: &programmes.claude,
        args: &["--version"],
        ok: None,
        absent: installer_claude,
        rouge: (None, installer_claude),
    }
    .jouer(programmes.delai_claude, &mut rapport);
    let connexion = "run `claude auth login`. A profile that names another provider (`api`) needs no Claude account.";
    if profil.is_some_and(|(_, vu)| vu.as_ref().is_ok_and(|vu| vu.fournisseur)) {
        rapport.saute("claude login", "the profile names another provider, whose hands need no Claude account.");
    } else {
        Controle {
            nom: "claude login",
            programme: &programmes.claude,
            args: &["auth", "status"],
            ok: Some("signed in"),
            absent: connexion,
            rouge: (Some("not signed in."), connexion),
        }
        .jouer(programmes.delai_claude, &mut rapport);
    }
    if profil.is_some_and(|(_, vu)| mur_natif(vu)) {
        mur_natif_present(programmes, &mut rapport);
        let detail = "the profile uses the native wall (`mur = natif`), which runs bubblewrap itself: it needs no srt.";
        rapport.saute("srt", detail);
    } else if windows {
        let detail =
            "srt does not hold under Windows: a profile there asks `mur = demande` (see \"Windows\" in the README).";
        rapport.saute("srt", detail);
    } else {
        let installer = "install it as the \"Try it\" section of the README says, or name its path in LYDITE_SRT.";
        Controle {
            nom: "srt",
            programme: &programmes.srt,
            args: &["--version"],
            ok: None,
            absent: installer,
            rouge: (None, installer),
        }
        .jouer(programmes.delai_srt, &mut rapport);
    }
    dossier_de_lydite(&mut rapport, dossier, windows);
    match profil {
        None => rapport.saute(
            "profile",
            "name a profile, from your repository, to check it and its wall: lydite doctor <profile>",
        ),
        Some((nom, Ok(vu))) => {
            let mur = if vu.mur { ", wall checked" } else { "" };
            rapport.ok("profile", format!("{nom} accepted, ready to launch here{mur}"));
        }
        Some((nom, Err(refus))) => {
            rapport.manques += 1;
            rapport.lignes.push(format!("missing profile: {nom}, refusal {}", refus.code.numero()));
            rapport.lignes.extend(refus.to_string().lines().map(|ligne| format!("  {ligne}")));
        }
    }
    let verdict =
        if rapport.manques == 0 { "ready".to_string() } else { format!("not ready: {} missing", rapport.manques) };
    rapport.lignes.push(verdict);
    (rapport.lignes, u8::from(rapport.manques > 0))
}

/// `lydite doctor [<profil>]` : lit l'environnement, joue [`verifier`], écrit ses lignes sur stdout. Le profil nommé se
/// lit comme `lancer` le lit, depuis le dossier courant (`preparer`).
pub(crate) fn doctor_cmd(nom: Option<&str>) -> Result<ExitCode, Refus> {
    let programme = |variable: &str, defaut: &str| env::var_os(variable).unwrap_or_else(|| defaut.into());
    let programmes = Programmes {
        git: "git".into(),
        gh: programme_gh(),
        claude: programme("LYDITE_CLAUDE", "claude"),
        srt: programme("LYDITE_SRT", "srt"),
        bwrap: natif::programme(),
        reseau: natif::reseau(),
        delai_git: DELAI,
        delai_gh: DELAI,
        delai_claude: DELAI,
        delai_srt: DELAI,
        delai_bwrap: DELAI,
    };
    let profil = nom.map(|nom| {
        let vu = preparer(nom, None).map(|(profil, ..)| ProfilVu {
            fournisseur: profil.fournisseur().is_some(),
            mur: profil.exige(),
            natif: profil.mur_du_rejeu() == MurDuRejeu::Natif,
        });
        (nom, vu)
    });
    let (lignes, code) =
        verifier(&programmes, &dossier::dossier(), profil.as_ref().map(|(nom, vu)| (*nom, vu)), cfg!(windows));
    for ligne in lignes {
        println!("{ligne}");
    }
    Ok(ExitCode::from(code))
}

#[cfg(test)]
mod tests;
