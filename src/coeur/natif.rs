//! Le mur de Lydite en arguments de bubblewrap : les règles que le rejeu de la preuve donne à srt
//! ([`crate::coeur::profil::Profil::rejeu`] : `denyRead`, `allowRead`, `allowWrite`, `denyWrite`) traduites en
//! arguments de `bwrap`, sans appeler le système. Ce que seul le disque sait (le chemin réel d'un lien, l'existence
//! d'un chemin, dossier ou fichier) est donné en entrée par l'appelant, par une fonction [`Disque`] ; cette fonction-ci
//! ne fait que de l'ordre et de la comparaison de chemins.
//!
//! Les arguments, dans cet ordre :
//!
//! ```text
//! --new-session --die-with-parent
//! --unshare-user --unshare-pid --unshare-net --unshare-ipc --unshare-uts
//! --cap-drop ALL
//! --ro-bind / /
//! <--tmpfs <dossier fermé>  |  --ro-bind /dev/null <fichier fermé>>…
//! <--ro-bind <chemin lu> <chemin lu>>…
//! <--bind <chemin écrit> <chemin écrit>>…
//! <--ro-bind <chemin refusé en écriture> <chemin refusé en écriture>>…
//! <--ro-bind <ancêtre> <ancêtre>>…
//! --dev /dev --proc /proc
//! ```
//!
//! - Chaque chemin se monte à son chemin réel, des deux côtés : `bwrap` suit un lien, et monter à l'endroit du lien
//!   monterait par-dessus sa cible.
//! - Un chemin lu ou écrit absent est sauté et rendu dans [`Natif::sautes`], que l'appelant dira. Un chemin fermé ou
//!   refusé en écriture qui est absent n'a rien à fermer : il est sauté sans bruit.
//! - Un dossier fermé se couvre d'un `--tmpfs`, un fichier fermé de `/dev/null`. Le `--tmpfs` vient avant tout
//!   montage : un chemin lu ou écrit sous un dossier fermé est rouvert par son propre montage, qui le suit.
//! - Un chemin lu et écrit à la fois (le bac) est monté deux fois, en lecture seule puis en écriture : l'écriture, qui
//!   vient après, l'emporte. Un chemin refusé en écriture vient encore après, et reprend la lecture seule sous le
//!   bac : le git commun, `.vscode`.
//! - Les ancêtres épinglés sont les dossiers entre un chemin refusé en écriture et la racine écrite qui le contient
//!   (la plus profonde, ni l'un ni l'autre compris) : en lecture seule, la main ne les renomme ni ne les remplace.
//! - Un montage qui recouvrirait un chemin fermé, le sien compris, le rouvrirait : c'est une erreur, jamais un
//!   chemin fermé qui s'ouvre en silence. Un chemin refusé en écriture que la fermeture cache, sans qu'un chemin lu ou
//!   écrit ne le rouvre, est sauté : le monter exposerait ce qu'il couvre.

#![cfg_attr(not(test), expect(dead_code))]
// raccourci : rien n'appelle encore cette fonction ; l'appelant qui lance `bwrap` à la place de srt la prendra, et
// rustc dira alors cette attente inutile

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Les fichiers et dossiers que la main n'écrit jamais dans le bac, s'ils y existent : ce qui s'exécute à la
/// prochaine ouverture de la forge, du shell ou de l'éditeur, et les commandes et agents de Claude Code. L'appelant
/// donne cette liste dans [`Regles::toujours`].
// raccourci : un fichier de cette liste qui n'existe pas encore n'est pas refusé, à monter si une preuve en crée un
pub(crate) const TOUJOURS_REFUSES: [&str; 13] = [
    ".gitconfig",
    ".gitmodules",
    ".bashrc",
    ".bash_profile",
    ".zshrc",
    ".zprofile",
    ".profile",
    ".ripgreprc",
    ".mcp.json",
    ".vscode",
    ".idea",
    ".claude/commands",
    ".claude/agents",
];

/// Les règles du mur, telles que srt les reçoit.
pub(crate) struct Regles<'a> {
    /// `denyRead` : les dossiers et fichiers fermés.
    pub(crate) fermes: &'a [PathBuf],
    /// `allowRead` : les chemins lus.
    pub(crate) lus: &'a [PathBuf],
    /// `allowWrite` : les chemins écrits, les racines écrites.
    pub(crate) ecrits: &'a [PathBuf],
    /// `denyWrite` : les chemins refusés en écriture.
    pub(crate) refuses: &'a [PathBuf],
    /// Le bac, où vivent les chemins de [`Regles::toujours`].
    pub(crate) bac: &'a Path,
    /// Les noms, relatifs au bac, toujours refusés en écriture s'ils existent : [`TOUJOURS_REFUSES`].
    pub(crate) toujours: &'a [&'a str],
}

/// Ce que le disque dit d'un chemin : absent, ou fichier ou dossier avec son chemin réel (liens suivis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Disque {
    Absent,
    Fichier(PathBuf),
    Dossier(PathBuf),
}

/// Ce que [`arguments`] rend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Natif {
    /// Les arguments de `bwrap`, sans le programme ni la commande qu'il lance.
    pub(crate) arguments: Vec<OsString>,
    /// Les chemins lus ou écrits absents, tels que les règles les nomment : d'abord les lus, puis les écrits.
    pub(crate) sautes: Vec<PathBuf>,
}

/// Un chemin que le disque connaît : son chemin réel, et s'il est un dossier.
struct Present {
    reel: PathBuf,
    dossier: bool,
}

/// Les arguments de `bwrap` des `regles`, `disque` disant ce que le disque tient de chaque chemin (voir l'en-tête).
/// Pure : `disque` est la seule chose qui touche le monde, et l'appelant la donne. Une erreur dit le montage qui
/// rouvrirait un chemin fermé.
pub(crate) fn arguments(regles: &Regles, disque: impl Fn(&Path) -> Disque) -> Result<Natif, String> {
    let toujours: Vec<PathBuf> = regles.toujours.iter().map(|nom| regles.bac.join(nom)).collect();
    let (fermes, _) = resoudre(regles.fermes, &disque);
    let (lus, mut sautes) = resoudre(regles.lus, &disque);
    let (ecrits, sautes_ecrits) = resoudre(regles.ecrits, &disque);
    sautes.extend(sautes_ecrits);
    let (refuses, _) = resoudre(regles.refuses.iter().chain(&toujours), &disque);

    let rouverts: Vec<&Path> = lus.iter().chain(&ecrits).map(|present| present.reel.as_path()).collect();
    // Un chemin que la fermeture cache : sous un dossier fermé, sans chemin lu ou écrit entre les deux qui le rouvre.
    let cache = |chemin: &Path| {
        let sous = |ferme: &&Present| ferme.dossier && chemin.starts_with(&ferme.reel);
        let rouvre = |ferme: &Present| rouverts.iter().any(|r| r.starts_with(&ferme.reel) && chemin.starts_with(r));
        fermes.iter().filter(sous).any(|ferme| !rouvre(ferme))
    };
    let refuses: Vec<Present> = refuses.into_iter().filter(|refuse| !cache(&refuse.reel)).collect();

    let racines: Vec<&Path> = ecrits.iter().filter(|e| e.dossier).map(|e| e.reel.as_path()).collect();
    let mut epingles: Vec<PathBuf> = Vec::new();
    for refuse in &refuses {
        for ancetre in entre(&refuse.reel, &racines) {
            let deja_refuse = refuses.iter().any(|r| r.reel == ancetre);
            if !deja_refuse && !epingles.contains(&ancetre) {
                epingles.push(ancetre);
            }
        }
    }

    let montes = lus.iter().chain(&ecrits).chain(&refuses).map(|present| present.reel.as_path());
    for monte in montes.chain(epingles.iter().map(PathBuf::as_path)) {
        if let Some(ferme) = fermes.iter().find(|ferme| ferme.reel.starts_with(monte)) {
            return Err(format!("monter {} rouvrirait {}, un chemin fermé", monte.display(), ferme.reel.display()));
        }
    }

    let mut args: Vec<OsString> = Vec::new();
    let fixes = [
        "--new-session",
        "--die-with-parent",
        "--unshare-user",
        "--unshare-pid",
        "--unshare-net",
        "--unshare-ipc",
        "--unshare-uts",
        "--cap-drop",
        "ALL",
        "--ro-bind",
        "/",
        "/",
    ];
    args.extend(fixes.map(OsString::from));
    for ferme in &fermes {
        if ferme.dossier {
            args.extend(["--tmpfs".into(), ferme.reel.clone().into_os_string()]);
        } else {
            monter(&mut args, "--ro-bind", Path::new("/dev/null"), &ferme.reel);
        }
    }
    for lu in &lus {
        monter(&mut args, "--ro-bind", &lu.reel, &lu.reel);
    }
    for ecrit in &ecrits {
        monter(&mut args, "--bind", &ecrit.reel, &ecrit.reel);
    }
    for refuse in &refuses {
        monter(&mut args, "--ro-bind", &refuse.reel, &refuse.reel);
    }
    for ancetre in &epingles {
        monter(&mut args, "--ro-bind", ancetre, ancetre);
    }
    args.extend(["--dev", "/dev", "--proc", "/proc"].map(OsString::from));
    Ok(Natif { arguments: args, sautes })
}

/// Ce que `disque` sait de chacun des `chemins`, dans leur ordre : les présents par leur chemin réel, sans doublon, et
/// les absents tels que les règles les nomment.
fn resoudre<'a>(
    chemins: impl IntoIterator<Item = &'a PathBuf>,
    disque: &impl Fn(&Path) -> Disque,
) -> (Vec<Present>, Vec<PathBuf>) {
    let (mut presents, mut absents) = (Vec::<Present>::new(), Vec::new());
    for chemin in chemins {
        let (reel, dossier) = match disque(chemin) {
            Disque::Absent => {
                absents.push(chemin.clone());
                continue;
            }
            Disque::Fichier(reel) => (reel, false),
            Disque::Dossier(reel) => (reel, true),
        };
        if !presents.iter().any(|present| present.reel == reel) {
            presents.push(Present { reel, dossier });
        }
    }
    (presents, absents)
}

/// Les dossiers entre `refuse` et la racine écrite la plus profonde qui le contient, ni l'un ni l'autre compris, de
/// la racine vers `refuse`. Aucun si aucune racine ne le contient.
fn entre(refuse: &Path, racines: &[&Path]) -> Vec<PathBuf> {
    let contenant = racines.iter().copied().filter(|racine| refuse.starts_with(racine) && *racine != refuse);
    let Some(racine) = contenant.max_by_key(|racine| racine.components().count()) else { return Vec::new() };
    let mut dossiers: Vec<PathBuf> =
        refuse.ancestors().skip(1).take_while(|ancetre| *ancetre != racine).map(Path::to_path_buf).collect();
    dossiers.reverse();
    dossiers
}

/// Un montage `option source dest`.
fn monter(args: &mut Vec<OsString>, option: &str, source: &Path, dest: &Path) {
    args.extend([option.into(), source.as_os_str().to_owned(), dest.as_os_str().to_owned()]);
}

#[cfg(test)]
mod tests;
