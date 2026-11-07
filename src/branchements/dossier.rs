//! Le dossier de Kraken, où vivent le fil et les bacs, hors de tout dépôt visé. [`dossier`] le lit dans
//! l'environnement, [`dossier_selon`] fait le choix sur des valeurs qu'on lui donne, pour que les tests n'aient pas à
//! toucher à l'environnement.

use std::env;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use crate::coeur::erreur::{Code, Refus, nommee};
use crate::coeur::fil::{
    Base, Evenement, IssueVague, Lecture, ligne_d_issue, ligne_de_note, lire_octets, lot, origine_lue,
};
use crate::coeur::profil::{self, Profil, empreinte, integre};

/// Le dossier de Kraken, qui peut ne pas exister encore. Sans chemin absolu pour le dire, le refus 6004 ou 6005.
pub(crate) fn dossier() -> Result<PathBuf, Refus> {
    let var = env::var_os;
    dossier_selon(var("LYDITE_DOSSIER"), var("USERPROFILE"), var("XDG_STATE_HOME"), var("HOME"), cfg!(windows))
}

/// Le choix du dossier sur les valeurs des variables, dans l'ordre où `dossier` les lit : les tests n'ont pas à
/// toucher l'environnement. `LYDITE_DOSSIER` s'il est posé, sinon `%USERPROFILE%\.lydite` sous Windows,
/// `$XDG_STATE_HOME/lydite` ailleurs, à défaut `$HOME/.local/state/lydite`. Sous Windows, le dossier reste hors
/// d'`AppData` : un programme lancé depuis une app empaquetée (MSIX) qui y crée un dossier neuf peut le voir
/// redirigé dans celui de l'app, invisible ailleurs et effacé avec elle. Seul un chemin absolu vaut (git, lancé dans
/// le dépôt, lirait un chemin relatif depuis le dépôt) : une variable vide ou relative est ignorée, sauf
/// `LYDITE_DOSSIER`, posée exprès : le refus 6004 plutôt qu'un repli en silence. Aucune ne vaut : le refus 6005.
fn dossier_selon(
    kraken_dossier: Option<OsString>,
    userprofile: Option<OsString>,
    xdg_state_home: Option<OsString>,
    home: Option<OsString>,
    windows: bool,
) -> Result<PathBuf, Refus> {
    if let Some(dossier) = kraken_dossier.map(PathBuf::from) {
        if dossier.is_absolute() {
            return Ok(dossier);
        }
        return Err(Refus::new(
            Code::DossierRelatif,
            format!("LYDITE_DOSSIER vaut « {} », qui n'est pas un chemin absolu", dossier.display()),
            "Pose LYDITE_DOSSIER à un chemin absolu, ou retire-la.",
        ));
    }
    let absolu = |valeur: Option<OsString>| valeur.map(PathBuf::from).filter(|chemin| chemin.is_absolute());
    let dossier = if windows {
        absolu(userprofile).map(|profil| profil.join(".lydite"))
    } else {
        absolu(xdg_state_home)
            .or_else(|| absolu(home).map(|home| home.join(".local").join("state")))
            .map(|etat| etat.join("lydite"))
    };
    dossier.ok_or_else(|| {
        let preuve = if windows {
            "USERPROFILE n'est pas posée, ou pas en chemin absolu"
        } else {
            "ni XDG_STATE_HOME ni HOME n'est posée en chemin absolu"
        };
        Refus::new(
            Code::DossierIntrouvable,
            preuve,
            "Pose LYDITE_DOSSIER à un chemin absolu : Lydite y range son fil et ses bacs.",
        )
    })
}

/// Le fichier du fil, dans le dossier de Kraken.
pub(crate) const FICHIER: &str = "fil.jsonl";

/// Une séance du fil d'un dossier de Kraken : son id, son origine et son compteur, que les mains se prêtent.
pub(crate) struct Fil {
    dossier: PathBuf,
    seance: String,
    origine: String,
    /// Le dernier `seq` écrit. Le verrou se tient pendant l'écriture : les numéros suivent l'ordre du fichier.
    seq: Mutex<u64>,
}

impl Fil {
    /// Une séance neuve sur le fil de `kraken`, à l'origine `origine` : son id est un lot, qu'aucune autre séance ne
    /// tire, et sa première note aura le `seq` 1.
    pub(crate) fn seance(kraken: &Path, origine: &str) -> Fil {
        debug_assert!(origine_lue(origine), "une origine que le fil lit : {origine}");
        Fil { dossier: kraken.to_path_buf(), seance: lot(), origine: origine.into(), seq: Mutex::new(0) }
    }

    pub(crate) fn dossier(&self) -> &Path {
        &self.dossier
    }

    /// Ajoute l'événement au bout du fil, d'une seule écriture, dans son enveloppe : le `seq` suivant de la séance,
    /// l'instant, et l'origine de la séance, sauf sur une fin ou une périmée déclarée, qui porte celle de sa main. Une
    /// erreur nomme le fil et garde son `kind()`, et la note ratée ne prend pas de numéro. Si le fil finit sur un bout
    /// de ligne sans `\n` (une écriture coupée), la ligne commence par `\n` : elle ne se colle pas au bout.
    // raccourci : sans sync_data, une coupure de courant peut perdre les dernières lignes et en laisser un bout, que
    // la note suivante clôt ; l'ajout d'un bloc ne tient que sur un disque local ; monter quand le fil devra survivre
    // à la machine ou vivre sur un partage.
    pub(crate) fn noter(&self, evenement: &Evenement) -> io::Result<()> {
        // Après une panique, le verrou se reprend : le compteur n'avance qu'après une écriture entière, il reste juste.
        let mut seq = self.seq.lock().unwrap_or_else(PoisonError::into_inner);
        let ligne = ligne_de_note(&self.seance, *seq + 1, &self.origine, evenement);
        self.ecrire(&mut seq, ligne)
    }

    /// Ajoute l'issue d'une vague au bout du fil, comme [`Fil::noter`] un événement : dans son enveloppe, à l'origine
    /// de la séance.
    pub(crate) fn noter_issue(&self, issue: &IssueVague) -> io::Result<()> {
        let mut seq = self.seq.lock().unwrap_or_else(PoisonError::into_inner);
        let ligne = ligne_d_issue(&self.seance, *seq + 1, &self.origine, issue);
        self.ecrire(&mut seq, ligne)
    }

    /// Écrit une ligne, son enveloppe puis son corps, sous le verrou du compteur `seq`, qui n'avance qu'après.
    fn ecrire(&self, seq: &mut u64, ligne: String) -> io::Result<()> {
        let chemin = self.dossier.join(FICHIER);
        ajouter(&chemin, ligne).map_err(|e| nommee(e, format!("note au fil {}", chemin.display())))?;
        *seq += 1;
        Ok(())
    }
}

/// Ajoute la ligne et son `\n` au bout du fichier, d'une seule écriture, après un `\n` si le fichier finit sur un bout
/// de ligne. Chaque appel ouvre le fichier à lui : des fils d'exécution notent comme des processus.
fn ajouter(chemin: &Path, ligne: String) -> io::Result<()> {
    let mut ligne = ligne + "\n";
    let mut fichier = OpenOptions::new().create(true).read(true).append(true).open(chemin)?;
    if fichier.metadata()?.len() > 0 {
        let mut dernier = [0];
        fichier.seek(SeekFrom::End(-1))?;
        fichier.read_exact(&mut dernier)?;
        if dernier[0] != b'\n' {
            ligne.insert(0, '\n');
        }
    }
    fichier.write_all(ligne.as_bytes())
}

/// Le fil du dossier de Kraken, lu selon la recette de l'en-tête : ses notes de Kraken et ses écarts. Un fil absent est
/// un fil vide. Une ligne d'une autre version arrête la lecture : le refus 1002, qui nomme la ligne et les deux
/// versions. Une E/S ratée est le refus 6006, qui nomme le fil.
// raccourci : tout le fil en mémoire ; monter vers une lecture par morceaux quand il pèsera des centaines de Mo.
pub(crate) fn lire(kraken: &Path) -> Result<Lecture, Refus> {
    lire_tout(kraken).map(|(lecture, _)| lecture)
}

/// Le fil lu une fois : ce que [`lire`] en rend, et ses issues de vague, dans l'ordre du fil, lues comme ses notes :
/// une ligne `vague.issue` mal formée n'en donne pas, et se dit dans les écarts.
pub(crate) fn lire_tout(kraken: &Path) -> Result<(Lecture, Vec<IssueVague>), Refus> {
    let chemin = kraken.join(FICHIER);
    // `fs::read` ouvre en partage lecture, écriture et effacement : une note passe pendant la lecture.
    let octets = match fs::read(&chemin) {
        Ok(octets) => octets,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Lecture::default(), Vec::new())),
        Err(e) => return Err(nommee(e, format!("lecture du fil {}", chemin.display())).into()),
    };
    lire_octets(&chemin, &octets)
}

/// Le quota du jour du dossier de Kraken, en micro-dollars : le fichier `<dossier>/profils/quota`, que les réglages
/// d'une main ferment comme les profils. Une ligne : un nombre décimal positif de dollars par jour, deux décimales au
/// plus (`5`, `12.50`) ; un BOM de tête et les fins de ligne sont tolérés. Absent, `None` : rien ne se vérifie.
/// Illisible ou mal formé, c'est le refus 6030, qui nomme le fichier.
pub(crate) fn lire_quota(kraken: &Path) -> Result<Option<u64>, Refus> {
    let chemin = kraken.join("profils").join("quota");
    let refus = |preuve: String| {
        let correction = format!(
            "Écris dans {} une ligne : un nombre positif de dollars par jour, deux décimales au plus (12.50) ; ou \
             retire le fichier pour ne plus borner la dépense.",
            chemin.display()
        );
        Refus::new(Code::Quota, preuve, correction)
    };
    let octets = match fs::read(&chemin) {
        Ok(octets) => octets,
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => return Ok(None),
        Err(e) => return Err(refus(nommee(e, format!("lecture du quota {}", chemin.display())).to_string())),
    };
    let texte = String::from_utf8(octets).map_err(|e| {
        refus(format!("{} : UTF-8 invalide après l'octet {}", chemin.display(), e.utf8_error().valid_up_to()))
    })?;
    match quota_de(&texte) {
        Some(micro_dollars) => Ok(Some(micro_dollars)),
        None => {
            Err(refus(format!("{} : « {} » n'est pas un nombre positif de dollars", chemin.display(), texte.trim())))
        }
    }
}

/// Les micro-dollars que dit le texte du fichier du quota, `None` s'il est mal formé : zéro, une virgule, un signe, un
/// exposant, trois décimales, plusieurs lignes ou un nombre qui ne tient pas.
fn quota_de(texte: &str) -> Option<u64> {
    let ligne = texte.strip_prefix('\u{feff}').unwrap_or(texte).trim_end_matches(['\r', '\n']).trim();
    let (entier, decimales) = match ligne.split_once('.') {
        Some((entier, decimales)) if !decimales.is_empty() => (entier, decimales),
        Some(_) => return None,
        None => (ligne, ""),
    };
    let chiffres = |texte: &str| texte.bytes().all(|c| c.is_ascii_digit());
    if entier.is_empty() || decimales.len() > 2 || !chiffres(entier) || !chiffres(decimales) {
        return None;
    }
    let centimes =
        entier.parse::<u64>().ok()?.checked_mul(100)?.checked_add(format!("{decimales:0<2}").parse().ok()?)?;
    centimes.checked_mul(10_000).filter(|&micro_dollars| micro_dollars > 0)
}

/// La dépense estimée du jour, en micro-dollars : la somme des `micro_dollars` des `main.usage` de base `estime` du fil
/// du dossier de Kraken dont l'instant tombe le jour `jour`, `AAAA-MM-JJ` en UTC. Le forfait et l'inconnu ne comptent
/// pas. Une lecture du fil qui échoue est son refus, tel quel.
// raccourci : tout le fil se relit à chaque départ ; monter vers un total tenu par le nœud quand le fil pèsera.
pub(crate) fn depense_du_jour(kraken: &Path, jour: &str) -> Result<u64, Refus> {
    let total = lire(kraken)?.notes.iter().fold(0u64, |total, note| match &note.evenement {
        Evenement::Usage { micro_dollars, base: Base::Estime, .. } if note.instant.starts_with(jour) => {
            total.saturating_add(*micro_dollars)
        }
        _ => total,
    });
    Ok(total)
}

/// Le profil `nom` du dossier de Kraken : son fichier, `<dossier>/profils/<nom>.profil`, lu par [`profil::lire`] avec
/// `LYDITE_CLAUDE` ; s'il est absent, le profil intégré du même nom ; sinon le refus 6001. Un nom hors de
/// `[A-Za-z0-9_-]` est le refus 6007, un fichier qui ne se lit pas en UTF-8 le refus 6008. Rien n'est écrit.
pub(crate) fn charger_dans(nom: &str, dossier: &Path) -> Result<Profil, Refus> {
    let chemin = fichier(nom, dossier)?;
    let kraken_claude = env::var("LYDITE_CLAUDE").ok();
    match (lire_fichier(nom, &chemin)?, integre(nom)) {
        (Some(texte), _) => profil::lire(nom, &texte, Some(&chemin), kraken_claude.as_deref()),
        (None, Some(texte)) => profil::lire(nom, texte, None, kraken_claude.as_deref()),
        (None, None) => Err(inconnu(nom, &chemin)),
    }
}

/// `lydite accepter <nom>` : lit et valide le fichier du profil, puis écrit son empreinte dans
/// `<dossier>/profils/<nom>.empreinte`. Rend le texte du profil et son empreinte, que l'humain voit. C'est la fin du
/// geste d'écrire le profil, que tout processus du compte peut faire. Refus : le nom (6007), le fichier (6008, 6009,
/// 6010), un profil sans fichier (6013 s'il est intégré, 6001 sinon), l'écriture de l'empreinte (6006).
pub(crate) fn accepter(nom: &str, dossier: &Path) -> Result<(String, String), Refus> {
    let chemin = fichier(nom, dossier)?;
    let Some(texte) = lire_fichier(nom, &chemin)? else {
        return Err(match integre(nom) {
            Some(_) => Refus::new(
                Code::ProfilSansFichier,
                format!(
                    "le profil « {nom} » n'a pas de fichier, {} est absent : c'est le profil intégré",
                    chemin.display()
                ),
                format!(
                    "Lance le profil intégré tel quel ; pour le changer, écris {} puis accepte-le.",
                    chemin.display()
                ),
            ),
            None => inconnu(nom, &chemin),
        });
    };
    profil::lire(nom, &texte, Some(&chemin), None)?;
    let (garde, empreinte) = (chemin.with_extension("empreinte"), empreinte(&texte));
    fs::write(&garde, &empreinte).map_err(|e| nommee(e, format!("écriture de l'empreinte {}", garde.display())))?;
    Ok((texte, empreinte))
}

/// L'empreinte qu'a acceptée l'humain, lue dans `garde`, le `<nom>.empreinte` qu'écrit [`accepter`], sans les blancs
/// qui l'entourent : la lecture que reçoit [`Profil::garder_selon`]. `None` si le fichier est absent ; une lecture
/// ratée est le refus 6006, qui le nomme.
pub(crate) fn lire_empreinte(garde: &Path) -> Result<Option<String>, Refus> {
    match fs::read(garde) {
        Ok(octets) => Ok(Some(String::from_utf8_lossy(&octets).trim().to_string())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(nommee(e, format!("lecture de l'empreinte {}", garde.display())).into()),
    }
}

/// Le dossier de connexion de Claude `dossier` garde un fichier de connexion, `.credentials.json` : seule son existence
/// se regarde, jamais son contenu. Un fichier qu'on ne peut pas dire absent (un refus d'accès, un lien cassé) compte
/// comme là : Kraken ne ferme pas les yeux sur ce qu'il ne voit pas.
pub(crate) fn connexion_presente(dossier: &Path) -> bool {
    let absent = |e: &io::Error| matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory);
    !matches!(fs::symlink_metadata(dossier.join(".credentials.json")), Err(e) if absent(&e))
}

/// Le fichier du profil `nom` : `<dossier>/profils/<nom>.profil`. Un nom hors de `[A-Za-z0-9_-]`, qui sortirait de
/// `profils` ou viserait un autre fichier, est le refus 6007.
fn fichier(nom: &str, dossier: &Path) -> Result<PathBuf, Refus> {
    if nom.is_empty() || !nom.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)) {
        return Err(Refus::new(
            Code::ProfilNomRefuse,
            format!("profil « {nom} »"),
            "Nomme le profil en lettres ASCII, chiffres, « _ » et « - » : son fichier est \
             <dossier de Lydite>/profils/<nom>.profil.",
        ));
    }
    Ok(dossier.join("profils").join(format!("{nom}.profil")))
}

/// Le texte du fichier du profil `nom`, `None` s'il est absent. Une lecture ratée, ou des octets qui ne sont pas de
/// l'UTF-8, sont le refus 6008, qui nomme le chemin ; le message d'une erreur d'E/S y passe tel quel.
fn lire_fichier(nom: &str, chemin: &Path) -> Result<Option<String>, Refus> {
    let illisible = |preuve: String| {
        let correction =
            format!("Rends {} lisible en UTF-8, puis accepte-le : lydite accepter {nom}.", chemin.display());
        Refus::new(Code::ProfilIllisible, preuve, correction)
    };
    match fs::read(chemin) {
        Ok(octets) => String::from_utf8(octets).map(Some).map_err(|e| {
            illisible(format!("{} : UTF-8 invalide après l'octet {}", chemin.display(), e.utf8_error().valid_up_to()))
        }),
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => Ok(None),
        Err(e) => Err(illisible(nommee(e, format!("lecture du profil {}", chemin.display())).to_string())),
    }
}

/// Le refus 6001 d'un profil sans fichier ni profil intégré de ce nom.
fn inconnu(nom: &str, chemin: &Path) -> Refus {
    Refus::new(
        Code::ProfilInconnu,
        format!("profil « {nom} » inconnu : {} est absent, et aucun profil intégré ne porte ce nom", chemin.display()),
        format!(
            "Écris {} puis accepte-le (lydite accepter {nom}), ou nomme le profil intégré : claude.",
            chemin.display()
        ),
    )
}

#[cfg(test)]
mod tests;
