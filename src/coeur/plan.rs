//! Le plan : ce que `lydite lancer <profil> --plan <fichier>` lit avant d'ouvrir la vague, ce qu'une main écrit quand
//! elle découpe un objectif, et ce que `lydite plan [--decoupe] <fichier>` lit sans rien lancer.
//!
//! ```text
//! plan 1
//! objectif : <une ligne>
//! preuve : <commande de la vague>
//!
//! tache <id>
//! depend : <id> <id>
//! preuve : <commande de la tâche>
//! modele : <modèle de la première tentative>
//! relance : <modèle de la seconde>
//! effort : <niveau des deux tentatives>
//! fichiers : <chemin> <dossier>/
//! critique : oui
//! sha256 : <fichier> : <empreinte>
//!     <objectif de la tâche, en lignes indentées>
//! ```
//!
//! Le texte :
//!
//! - 1 Mo au plus ([`TAILLE_MAX`] octets), pesé avant d'être lu ;
//! - de l'UTF-8, un BOM de tête toléré. Un plan en UTF-16 (ce qu'écrit `>` sous PowerShell 5.1 : il commence par
//!   `FF FE`, ou `FE FF`) et tout octet hors de l'UTF-8 sont refusés en 6017 ; la correction donne la commande qui
//!   réécrit le fichier, `(Get-Content <fichier>) | Set-Content -Encoding utf8 <fichier>` ;
//! - les lignes se coupent sur `\n`, un `\r` final ôté : un plan en CRLF se lit ;
//! - la première ligne est `plan 1`, lue avant tout le reste ; une autre est refusée en 6016 ;
//! - ensuite, en colonne 0, une ligne est vide (ou faite de blancs), `tache <id>`, ou `<clé> : <valeur>`, les blancs
//!   ôtés autour du `:` ; une ligne indentée (espaces ou tabulations) est l'objectif de la tâche en cours. Il n'y a ni
//!   commentaire ni continuation de ligne.
//!
//! Les clés sont fermées, et chacune se donne une fois au plus, hors `exact` et `sha256` :
//!
//! - dans l'en-tête, avant la première tâche : `objectif` et `preuve`, obligatoires l'une et l'autre ;
//! - sous `tache <id>`, avant l'objectif, toutes facultatives : `depend`, des ids séparés par des blancs (aucun, si
//!   elle est vide) ; `preuve`, que ne porte aucune tâche d'un plan découpé ; `modele` et `relance`, les modèles de la
//!   première tentative de la tâche et de sa relance, en `[a-z0-9.-]` sans `-` de tête, comme au profil, dont ils
//!   prennent la place ; `effort`, l'effort de Claude Code des deux tentatives de la tâche, un de `low`, `medium`,
//!   `high`, `xhigh`, `max`, qui prend la place des clés `effort` et `effort_relance` du profil, et que le profil doit
//!   permettre (`efforts_permis`, refus 6033 au lancement) ; `fichiers`, les chemins que la tâche peut changer,
//!   séparés par des blancs ; `critique`, de seule valeur `oui`, pour une tâche dont le travail va à la relecture ;
//!   `exact` et `sha256`, qui se répètent, une ligne par fichier (voir plus bas).
//!
//! Les deux clés de tâche qui se répètent :
//!
//! - `exact : <fichier> : <texte>` exige ce texte dans ce fichier, octet pour octet. Le fichier est un chemin de
//!   fichier comme dans `fichiers`, que les `fichiers` de la tâche couvrent quand elle en a ; le texte n'est pas vide,
//!   et garde ses blancs et ses ` : `. Une valeur qui n'a pas cette forme, un chemin hors de la forme, ou un fichier
//!   que les `fichiers` ne couvrent pas est refusé en 6024 ;
//! - `sha256 : <fichier> : <empreinte>` épingle un fichier entier par son SHA-256. La lecture ne calcule rien et
//!   n'ouvre aucun fichier. Le fichier est un chemin de fichier comme dans `fichiers` (sans blanc, sans caractère de
//!   contrôle, sans `/` final : un dossier ne s'épingle pas), que les `fichiers` de la tâche couvrent quand elle en
//!   a ; l'empreinte est faite de 64 hexadécimaux minuscules. Un fichier n'est épinglé qu'une fois par tâche, et n'a
//!   qu'une empreinte dans tout le plan (la même sous deux tâches se lit). Refusés : en 6024, une valeur qui n'est pas
//!   `<fichier> : <empreinte>` (une empreinte vide comprise), un chemin hors de la forme, avec un blanc ou un
//!   caractère de contrôle, ou fini par `/`, et un fichier que les `fichiers` ne couvrent pas ; en 6018, une empreinte
//!   qui n'est pas de 64 hexadécimaux minuscules, un fichier épinglé deux fois dans la tâche, et un fichier épinglé
//!   sous deux empreintes par deux tâches (refus à la fin de la lecture, sur la seconde ligne) ; en 6021, une valeur
//!   blanche.
//!
//! Ce que le plan dit :
//!
//! - l'objectif d'une tâche est ses lignes indentées : l'indentation commune ôtée, les lignes vides du milieu gardées,
//!   celles du début et de la fin ôtées. Une tâche en a un ;
//! - un id est un mot de lettres et de chiffres ASCII, de `-` et de `_`, unique dans le plan. `depend` nomme les ids
//!   d'autres tâches, placées avant ou après : jamais la tâche elle-même, jamais en cycle ;
//! - une tâche joue sa preuve, à défaut celle de la vague. Dans un plan découpé, aucune n'a la sienne : toutes jouent
//!   la preuve de la vague, que seul l'humain donne, car la commande d'une tâche se rejoue hors du mur, avec les droits
//!   de l'humain. Les tâches sont les mains 1 à n, dans l'ordre du fichier : au plus [`NOMBRE_MAX`], car chacune est
//!   une main du fil, que son lecteur borne à 10 000 ;
//! - un chemin de `fichiers` est relatif à la racine du dépôt et s'écrit comme git l'écrit : des noms séparés par `/`,
//!   sans `/` ni lettre de lecteur en tête, sans barre inverse, sans nom vide, `.` ou `..`, et sans nom `.git` ou
//!   `.claude` (en toute casse, et avec les points et les blancs de fin que Windows ôte). Un dossier finit par `/` et
//!   couvre ce qu'il contient ([`couvre`]). Les chemins se comparent exactement, octet à octet, sensibles à la casse,
//!   sous Windows comme ailleurs ; un chemin n'a pas de blanc, et son dossier le couvre. Sans `fichiers`, rien ne borne
//!   la tâche. Dans un plan découpé, la ligne en nomme [`FICHIERS_MAX`] au plus : le passage en série qui suit compare
//!   deux à deux les chemins de deux tâches ;
//! - deux tâches qui pourraient tourner ensemble (aucune ne dépend de l'autre, même de loin) et qui partagent un chemin
//!   (l'un des leurs couvre l'autre) passent en série, sans refus : Kraken ajoute au `depend` de l'une l'id de l'autre,
//!   en suivant un ordre topologique du plan, les dépendances d'abord, puis l'ordre du fichier. Toutes les dépendances
//!   ajoutées vont donc dans le même sens, et aucune ne fait de cycle ; [`Plan::lire_en`] les rend, et `lydite plan`
//!   les dit.
//!
//! Un plan se lit selon son mode ([`Mode`]). Écrit à la main (`--plan`), ses clés de tâche sont facultatives. Découpé
//! par une main, il a 12 tâches au plus, chacune a `fichiers` (100 chemins au plus) et `modele`, et aucune n'a de
//! `preuve`. Au rejeu d'un découpage, Kraken juge de plus son en-tête, qui porte l'objectif et la preuve de l'humain
//! tels quels, et la casse de chaque chemin, celle que git lui connaît dans l'arbre de la base : sous une autre, un
//! chemin ferait rater le gel à tort.
//!
//! Chaque refus enseigne, et nomme le numéro et le texte de la ligne qu'il vise (hors un plan trop lourd, un en-tête
//! incomplet et un plan sans tâche, qui n'en visent aucune). Tous tombent avant l'ouverture de la vague, au bloc 6000
//! (`lancer.`), comme la base rouge (6022), que la vague éprouve et non la lecture :
//!
//! - 6016 : la première ligne n'est pas `plan 1` ;
//! - 6017 : de l'UTF-16, ou un octet hors de l'UTF-8 (son rang) ;
//! - 6018 : une ligne hors de la forme (colonne 0 inconnue, clé inconnue, redite ou hors de sa place, ligne indentée
//!   hors tâche), une valeur hors de son domaine (`modele`, `relance`, `effort`, `critique`), ou une 10 001e tâche ;
//! - 6019 : un id qui n'est pas un mot de cette grammaire, ou redit ;
//! - 6020 : une dépendance inconnue ou à soi, ou un cycle, que le refus nomme et dont il cite la première ligne
//!   `depend` ;
//! - 6021 : un objectif ou une preuve blancs ou absents, une autre clé blanche (hors `depend`), un plan sans tâche, une
//!   tâche sans objectif ;
//! - 6024 : un chemin de `fichiers` hors de cette forme, ou qui porte une virgule, ou, au rejeu d'un découpage, sous
//!   une autre casse que dans la base, que la correction donne ;
//! - 6025 : un plan de plus de 1 Mo ; un plan découpé de plus de 12 tâches, ou dont une tâche n'a pas `fichiers` ou
//!   `modele`, porte une `preuve`, ou nomme plus de 100 chemins dans `fichiers` ;
//! - 6026 : au rejeu d'un découpage, un en-tête autre que celui que Kraken a demandé.
//!
//! Aucune entrée ne fait paniquer la lecture : un plan est une frontière de confiance, et le lire strictement est ce
//! qui la garde.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt::{self, Display};
use std::iter;
use std::str;

use crate::coeur::erreur::{Code, Refus};
use crate::coeur::fil::NOMBRE_MAX;
use crate::coeur::profil::EFFORTS;
use crate::coeur::texte;

/// Ce qu'un plan pèse au plus : 1 Mo.
pub(crate) const TAILLE_MAX: usize = 1_000_000;

/// Les tâches d'un plan découpé, au plus.
const DECOUPEES_MAX: usize = 12;

/// Les chemins de la ligne `fichiers` d'une tâche d'un plan découpé, au plus : le passage en série compare deux à deux
/// les chemins de deux tâches, et le coût croît avec le produit de leurs nombres.
pub(crate) const FICHIERS_MAX: usize = 100;

/// Les clés de l'en-tête, puis celles d'une tâche.
const CLES_ENTETE: [&str; 2] = ["objectif", "preuve"];
const CLES_TACHE: [&str; 9] =
    ["depend", "preuve", "modele", "relance", "effort", "fichiers", "critique", "exact", "sha256"];

/// Les caractères d'une ligne qu'une preuve cite : au-delà, la citation est coupée.
const CITE_MAX: usize = 200;

/// La correction d'un plan qui n'est pas de l'UTF-8.
const REECRIRE: &str = concat!(
    "Réécris le plan en UTF-8 : sous PowerShell, `(Get-Content <fichier>) | Set-Content -Encoding utf8 <fichier>` ; ",
    "ailleurs, enregistre-le en UTF-8 depuis ton éditeur."
);

/// La correction d'une ligne hors de la forme du plan.
const FORME: &str = concat!(
    "Une ligne de plan est vide, `tache <id>`, `<clé> : <valeur>`, ou indentée (l'objectif de la tâche qui la ",
    "précède). Avant la première tâche, les clés sont `objectif` et `preuve` ; sous `tache <id>`, `depend`, `preuve`, ",
    "`modele`, `relance`, `effort`, `fichiers`, `critique`, `exact` et `sha256` (les deux dernières peuvent se ",
    "répéter), puis l'objectif en lignes indentées ; chaque autre clé une fois."
);

/// La correction d'une ligne `exact` hors de sa forme.
const EXACT: &str = concat!(
    "Écris `exact : <fichier> : <texte>` : le fichier comme dans `fichiers` (un chemin relatif à la racine du dépôt, ",
    "couvert par les `fichiers` de la tâche), puis ` : `, puis le texte, non vide, qui doit y paraître tel quel."
);

/// La correction d'une ligne `sha256` hors de sa forme.
const SHA256: &str = concat!(
    "Write `sha256 : <file> : <hash>`: the file as in `fichiers`, not a folder and with no blank, covered by the ",
    "task's `fichiers`; then its SHA-256, 64 lowercase hexadecimal digits; each file once per task, and one hash per ",
    "file in the whole plan."
);

/// La correction d'un modèle hors de son domaine.
const MODELE: &str = concat!(
    "Écris un modèle en [a-z0-9.-], qui ne commence pas par `-`, comme au profil : un alias (haiku, sonnet, opus) ou ",
    "un nom complet (claude-sonnet-5-5)."
);

/// La correction d'un effort hors de son domaine.
const EFFORT: &str = "Écris un effort parmi low, medium, high, xhigh et max, ou retire la ligne : la tâche prend alors \
                      ceux du profil.";

/// La correction d'un chemin de `fichiers` hors de la forme.
const CHEMIN: &str = concat!(
    "Écris dans `fichiers` des chemins relatifs à la racine du dépôt, tels que git les écrit : des noms séparés par ",
    "`/`, sans `.` ni `..`, hors de `.git/` et de `.claude/` ; un dossier finit par `/` et couvre ce qu'il contient."
);

/// La correction d'une tâche de plan découpé sans ses clés.
const DECOUPEE: &str = concat!(
    "Donne à chaque tâche d'un plan découpé, sous sa ligne `tache <id>`, `fichiers : <chemin> …` (les chemins qu'elle ",
    "peut changer, un dossier fini par `/`) et `modele : <alias>` (haiku ou sonnet pour l'essentiel, opus pour une ",
    "tâche critique)."
);

/// La correction d'une tâche de plan découpé qui porte une `preuve`.
const SANS_PREUVE: &str = concat!(
    "Retire la ligne `preuve` de la tâche : une tâche d'un plan découpé joue la preuve de la vague, que seul l'humain ",
    "donne."
);

/// La correction d'un id de tâche faux.
const IDENTIFIANT: &str = concat!(
    "Écris `tache <id>` avec un id d'un seul mot, unique dans le plan : lettres et chiffres ASCII, `-` et `_` ",
    "(par exemple `fil-v2`)."
);

/// Un plan : l'objectif de la vague, sa preuve, et ses tâches dans l'ordre du fichier (les mains 1 à n).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) objectif: String,
    /// La preuve de la vague, rejouée sur sa branche après chaque fusion. `None` pour des copies : sans elle, rien ne
    /// s'assemble.
    pub(crate) preuve: Option<String>,
    pub(crate) taches: Vec<Tache>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Tache {
    pub(crate) id: String,
    /// Les ids des tâches qui doivent être assemblées avant qu'elle parte : ceux de sa ligne `depend`, puis ceux que
    /// Kraken ajoute pour faire passer en série les tâches qui partagent un chemin.
    pub(crate) depend: Vec<String>,
    /// Sa preuve ; à défaut, la tâche joue celle de la vague. Une tâche lue dans un plan découpé n'en a jamais.
    pub(crate) preuve: Option<String>,
    pub(crate) objectif: String,
    /// Le modèle de sa première tentative, puis celui de sa relance ; à défaut, ceux du profil.
    pub(crate) modele: Option<String>,
    pub(crate) relance: Option<String>,
    /// L'effort de ses deux tentatives ; à défaut, ceux du profil.
    pub(crate) effort: Option<String>,
    /// Les chemins qu'elle peut changer, tels que le plan les écrit, un dossier fini par `/` ; `None` : rien ne la
    /// borne.
    pub(crate) fichiers: Option<Vec<String>>,
    /// Les textes que le plan exige dans ses fichiers, octet pour octet : un couple (fichier, texte) par ligne `exact`.
    pub(crate) exacts: Vec<(String, String)>,
    /// Les fichiers que le plan épingle par leur SHA-256 : un couple (fichier, empreinte) par ligne `sha256`, dans
    /// l'ordre du plan. La lecture ne calcule rien : l'empreinte est celle que le plan écrit.
    pub(crate) empreintes: Vec<(String, String)>,
    /// Son travail va à la relecture.
    pub(crate) critique: bool,
}

/// Le mode d'une lecture : qui a écrit le plan, et ce que Kraken sait de ce qu'il devait être.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Mode<'d> {
    /// Écrit à la main, et donné par `--plan` ou par `lydite plan` : ses clés de tâche sont facultatives.
    Main,
    /// Découpé par une main : 12 tâches au plus, `fichiers` (100 chemins au plus) et `modele` sur chacune, et aucune
    /// `preuve` de tâche. Avec la demande de Kraken, au rejeu du découpage, de plus l'en-tête demandé (6026) et la
    /// casse de la base (6024) ; sans elle, pour `lydite plan --decoupe`, seulement ce qui ne demande pas de contexte.
    Decoupe(Option<&'d Demande<'d>>),
}

/// Ce que Kraken a demandé au découpeur, et l'arbre de la base : de quoi juger un plan découpé au rejeu.
#[derive(Debug)]
pub(crate) struct Demande<'d> {
    /// L'objectif et la preuve de l'humain, tels quels : l'en-tête du plan doit les porter, aux blancs du bord près.
    pub(crate) objectif: &'d str,
    pub(crate) preuve: &'d str,
    /// Les chemins de l'arbre de la base, tels que git les rend (`git ls-tree -r -z --name-only <base>`).
    pub(crate) chemins: &'d [String],
}

/// Une dépendance que Kraken ajoute : la tâche `tache` attend `attend`, car elles partagent `chemin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attente {
    pub(crate) tache: String,
    pub(crate) attend: String,
    pub(crate) chemin: String,
}

/// `<id> attend <id> : elles partagent <chemin>`, ce que `lydite plan` écrit. Le chemin vient d'un fichier : ses
/// caractères de contrôle s'écrivent (`\u{1b}`), pour ne pas piéger un terminal.
impl fmt::Display for Attente {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} attend {} : elles partagent {}", self.tache, self.attend, texte::en_ligne(&self.chemin))
    }
}

impl Plan {
    /// Lit un plan écrit à la main, comme `--plan` le lit : [`Plan::lire_en`] en mode [`Mode::Main`], sans la liste des
    /// dépendances ajoutées, que le `depend` de leur tâche garde. Les tests s'en servent ; Kraken, de `lire_en`.
    #[cfg(test)]
    pub(crate) fn lire(octets: &[u8]) -> Result<Plan, Refus> {
        Plan::lire_en(octets, Mode::Main).map(|(plan, _)| plan)
    }

    /// Lit un plan dans ses octets, selon son mode : le plan et les dépendances que Kraken y ajoute (aussi dans le
    /// `depend` de leur tâche), ou le refus de la première faute, de 6016 à 6021 ou de 6024 à 6026 (voir l'en-tête du
    /// module pour l'ordre et la grammaire). Aucune entrée ne fait paniquer la lecture.
    pub(crate) fn lire_en(octets: &[u8], mode: Mode<'_>) -> Result<(Plan, Vec<Attente>), Refus> {
        if octets.len() > TAILLE_MAX {
            let preuve = format!("le plan dépasse 1 Mo : plus de {TAILLE_MAX} octets");
            let correction = "Allège le plan sous 1 Mo : raccourcis les objectifs de ses tâches, ou répartis-les en \
                              plusieurs plans.";
            return Err(Refus::new(Code::PlanBorne, preuve, correction));
        }
        let texte = en_utf8(octets)?;
        let coupees = texte.split('\n').map(|ligne| ligne.strip_suffix('\r').unwrap_or(ligne));
        let mut lignes = (1..).zip(coupees).map(|(numero, texte)| Ligne { numero, texte });
        let premiere = lignes.next().unwrap_or(Ligne { numero: 1, texte: "" });
        if premiere.texte != "plan 1" {
            let correction =
                "Écris `plan 1` seul en première ligne : Lydite ne lit que la version 1, avant tout le reste.";
            return Err(refus(Code::PlanVersion, premiere, "la première ligne d'un plan est `plan 1`", correction));
        }
        let mut lecteur = Lecteur::new(mode);
        for ligne in lignes {
            lecteur.lire(ligne)?;
        }
        lecteur.finir()
    }

    /// Les copies de `--n` : `n` tâches `1` à `n`, sans dépendance, qui jouent chacune `verifier` sur le même objectif.
    /// La vague n'a pas de preuve, donc pas d'assemblage. `n` va de 1 à [`NOMBRE_MAX`], ce que la ligne de commande
    /// garantit.
    pub(crate) fn copies(objectif: &str, verifier: Option<&str>, n: usize) -> Plan {
        let tache = |k: usize| Tache {
            id: k.to_string(),
            depend: Vec::new(),
            preuve: verifier.map(str::to_string),
            objectif: objectif.to_string(),
            ..Tache::default()
        };
        Plan { objectif: objectif.to_string(), preuve: None, taches: (1..=n).map(tache).collect() }
    }
}

/// Une ligne du plan : son numéro, à partir de 1, et son texte sans le `\r` final.
#[derive(Clone, Copy)]
struct Ligne<'a> {
    numero: usize,
    texte: &'a str,
}

/// Ce que la lecture a vu jusqu'ici, dans son mode. Tant qu'elle n'a vu aucune tâche, elle est dans l'en-tête.
struct Lecteur<'a, 'd> {
    mode: Mode<'d>,
    /// L'arbre de la base, au rejeu d'un découpage : de quoi dire la casse d'un chemin.
    base: Option<Base<'d>>,
    objectif: Option<&'a str>,
    preuve: Option<&'a str>,
    taches: Vec<Brouillon<'a>>,
    /// Le rang dans `taches` de chaque id vu.
    ids: HashMap<&'a str, usize>,
}

/// Une tâche en cours de lecture : ses lignes, avant d'être d'un seul tenant.
struct Brouillon<'a> {
    /// Sa ligne `tache <id>`.
    ligne: Ligne<'a>,
    id: &'a str,
    /// Sa ligne `depend`, et les ids qu'elle nomme.
    depend: Option<(Ligne<'a>, Vec<&'a str>)>,
    preuve: Option<&'a str>,
    modele: Option<&'a str>,
    relance: Option<&'a str>,
    effort: Option<&'a str>,
    fichiers: Option<Vec<&'a str>>,
    /// Ses lignes `exact`, chacune avec son fichier et son texte.
    exacts: Vec<(Ligne<'a>, &'a str, &'a str)>,
    /// Ses lignes `sha256`, chacune avec son fichier et son empreinte.
    empreintes: Vec<(Ligne<'a>, &'a str, &'a str)>,
    critique: bool,
    /// Ses lignes indentées, chacune avec le nombre de retours à la ligne qui la séparent de la précédente : 0 pour la
    /// première, puis 1 de plus par ligne vide entre elles.
    objectif: Vec<(usize, &'a str)>,
    /// Les lignes vides vues depuis la dernière ligne indentée : elles ne comptent que si une autre ligne suit.
    vides: usize,
}

impl Brouillon<'_> {
    fn donnee(&self, cle: &str) -> bool {
        match cle {
            "depend" => self.depend.is_some(),
            "preuve" => self.preuve.is_some(),
            "modele" => self.modele.is_some(),
            "relance" => self.relance.is_some(),
            "effort" => self.effort.is_some(),
            "fichiers" => self.fichiers.is_some(),
            "exact" | "sha256" => false,
            _ => self.critique,
        }
    }
}

impl<'a, 'd> Lecteur<'a, 'd> {
    fn new(mode: Mode<'d>) -> Lecteur<'a, 'd> {
        let base = match mode {
            Mode::Decoupe(Some(demande)) => Some(Base::new(demande.chemins)),
            Mode::Main | Mode::Decoupe(None) => None,
        };
        Lecteur { mode, base, objectif: None, preuve: None, taches: Vec::new(), ids: HashMap::new() }
    }

    /// Lit une ligne après la première.
    fn lire(&mut self, ligne: Ligne<'a>) -> Result<(), Refus> {
        let texte = ligne.texte;
        if texte.trim().is_empty() {
            // Une ligne vide ne dit rien, sauf entre deux lignes de l'objectif d'une tâche.
            if let Some(tache) = self.taches.last_mut().filter(|tache| !tache.objectif.is_empty()) {
                tache.vides += 1;
            }
            return Ok(());
        }
        if texte.starts_with([' ', '\t']) {
            return self.indentee(ligne);
        }
        match texte.strip_prefix("tache").filter(|id| id.is_empty() || id.starts_with([' ', '\t'])) {
            Some(id) => self.tache(ligne, id.trim()),
            None => self.cle(ligne),
        }
    }

    /// Une ligne indentée, non vide : elle continue l'objectif de la tâche en cours.
    fn indentee(&mut self, ligne: Ligne<'a>) -> Result<(), Refus> {
        let Some(tache) = self.taches.last_mut() else {
            let quoi = "cette ligne est indentée, et aucune ligne `tache <id>` ne la précède";
            return Err(refus(Code::PlanLigne, ligne, quoi, FORME));
        };
        let sauts = if tache.objectif.is_empty() { 0 } else { tache.vides + 1 };
        tache.vides = 0;
        tache.objectif.push((sauts, ligne.texte));
        Ok(())
    }

    /// Une ligne `tache <id>` : une tâche de plus, dans la limite de `NOMBRE_MAX` (12 pour un plan découpé), d'un id de
    /// la grammaire que le plan n'a pas encore donné.
    fn tache(&mut self, ligne: Ligne<'a>, id: &'a str) -> Result<(), Refus> {
        if matches!(self.mode, Mode::Decoupe(_)) && self.taches.len() == DECOUPEES_MAX {
            let quoi =
                format!("c'est la tâche {}, et un plan découpé en compte {DECOUPEES_MAX} au plus", DECOUPEES_MAX + 1);
            let correction = format!(
                "Regroupe les tâches en {DECOUPEES_MAX} au plus : chacune est une main, et un découpage plus fin n'en \
                 vaut pas davantage."
            );
            return Err(refus(Code::PlanBorne, ligne, quoi, correction));
        }
        if self.taches.len() == NOMBRE_MAX {
            let quoi = format!("c'est la tâche {}, et un plan en compte {NOMBRE_MAX} au plus", NOMBRE_MAX + 1);
            let correction = format!(
                "Répartis les tâches en plusieurs plans de {NOMBRE_MAX} au plus : chacune est une main du fil, qui \
                 n'en lit pas davantage par vague."
            );
            return Err(refus(Code::PlanLigne, ligne, quoi, correction));
        }
        let mot = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_');
        if id.is_empty() || !id.bytes().all(mot) {
            let quoi = if id.is_empty() {
                "il manque l'id".to_string()
            } else {
                format!("l'id {} n'est pas un mot de lettres et de chiffres ASCII, de `-` et de `_`", cite(id))
            };
            return Err(refus(Code::PlanIdentifiant, ligne, quoi, IDENTIFIANT));
        }
        if let Some(&rang) = self.ids.get(id) {
            let avant = self.taches[rang].ligne.numero;
            let quoi = format!("l'id {} est déjà celui de la tâche de la ligne {avant}", cite(id));
            return Err(refus(Code::PlanIdentifiant, ligne, quoi, IDENTIFIANT));
        }
        self.ids.insert(id, self.taches.len());
        self.taches.push(Brouillon {
            ligne,
            id,
            depend: None,
            preuve: None,
            modele: None,
            relance: None,
            effort: None,
            fichiers: None,
            exacts: Vec::new(),
            empreintes: Vec::new(),
            critique: false,
            objectif: Vec::new(),
            vides: 0,
        });
        Ok(())
    }

    /// Une ligne `<clé> : <valeur>` : une clé de sa place, une fois, avant l'objectif de sa tâche, sans valeur blanche
    /// (sauf `depend`, qui peut n'en nommer aucune), et dont la valeur est de son domaine.
    fn cle(&mut self, ligne: Ligne<'a>) -> Result<(), Refus> {
        let faute = |quoi: String| refus(Code::PlanLigne, ligne, quoi, FORME);
        let Some((cle, valeur)) = ligne.texte.split_once(':') else {
            return Err(faute("elle n'est ni vide, ni indentée, ni `tache <id>`, ni `<clé> : <valeur>`".into()));
        };
        let (cle, valeur) = (cle.trim(), valeur.trim());
        let entete = self.taches.is_empty();
        let (permises, lieu, liste): (&[&str], _, _) = if entete {
            (&CLES_ENTETE, "de l'en-tête", "`objectif` ou `preuve`")
        } else {
            (
                &CLES_TACHE,
                "d'une tâche",
                "`depend`, `preuve`, `modele`, `relance`, `effort`, `fichiers`, `critique`, `exact` ou `sha256`",
            )
        };
        if !permises.contains(&cle) {
            return Err(faute(format!("la clé {} n'est pas une clé {lieu} ({liste})", cite(cle))));
        }
        if self.taches.last().is_some_and(|tache| !tache.objectif.is_empty()) {
            let apres = "vient après les lignes indentées de la tâche, que ses clés précèdent";
            return Err(faute(format!("la clé {} {apres}", cite(cle))));
        }
        // La commande d'une tâche se rejoue hors du mur, avec les droits de l'humain : une main qui découpe n'en écrit
        // aucune, blanche ou non, et la vague joue la preuve que l'humain a donnée.
        if let Some(tache) = self.taches.last()
            && cle == "preuve"
            && matches!(self.mode, Mode::Decoupe(_))
        {
            let quoi =
                format!("la tâche {} porte une clé `preuve`, et aucune tâche d'un plan découpé n'en a", cite(tache.id));
            return Err(refus(Code::PlanBorne, ligne, quoi, SANS_PREUVE));
        }
        let donnee = match self.taches.last() {
            None if cle == "objectif" => self.objectif.is_some(),
            None => self.preuve.is_some(),
            Some(tache) => tache.donnee(cle),
        };
        if donnee {
            let ou = if entete { "dans l'en-tête" } else { "dans cette tâche" };
            return Err(faute(format!("la clé {} est déjà donnée {ou}", cite(cle))));
        }
        if valeur.is_empty() && cle != "depend" {
            let quoi = format!("la clé {} est blanche", cite(cle));
            return Err(refus(Code::PlanBlanc, ligne, quoi, blanche(entete, cle)));
        }
        let Some(tache) = self.taches.last_mut() else {
            demandee(self.mode, ligne, cle, valeur)?;
            *(if cle == "objectif" { &mut self.objectif } else { &mut self.preuve }) = Some(valeur);
            return Ok(());
        };
        match cle {
            "depend" => tache.depend = Some((ligne, valeur.split_whitespace().collect())),
            "preuve" => tache.preuve = Some(valeur),
            "modele" | "relance" if !modele_permis(valeur) => {
                let quoi = format!("le modèle {} ne s'écrit pas en [a-z0-9.-] sans `-` de tête", cite(valeur));
                return Err(refus(Code::PlanLigne, ligne, quoi, MODELE));
            }
            "modele" => tache.modele = Some(valeur),
            "relance" => tache.relance = Some(valeur),
            "effort" if !EFFORTS.contains(&valeur) => {
                let quoi = format!("l'effort {} n'est ni low, ni medium, ni high, ni xhigh, ni max", cite(valeur));
                return Err(refus(Code::PlanLigne, ligne, quoi, EFFORT));
            }
            "effort" => tache.effort = Some(valeur),
            "fichiers" => {
                let chemins: Vec<&str> = valeur.split_whitespace().collect();
                // Avant tout chemin : le passage en série compare ceux de deux tâches deux à deux.
                if matches!(self.mode, Mode::Decoupe(_)) && chemins.len() > FICHIERS_MAX {
                    let quoi = format!(
                        "elle nomme {} chemins, et la ligne `fichiers` d'une tâche d'un plan découpé en compte \
                         {FICHIERS_MAX} au plus",
                        chemins.len()
                    );
                    let correction = format!(
                        "Regroupe les chemins sous leurs dossiers : un dossier fini par `/` couvre ce qu'il \
                         contient, et une tâche en nomme {FICHIERS_MAX} au plus."
                    );
                    return Err(refus(Code::PlanBorne, ligne, quoi, correction));
                }
                for &chemin in &chemins {
                    // raccourci : un fichier dont le nom porte une virgule ne se nomme pas dans fichiers ; monter
                    // (l'accepter s'il est dans la base) quand un dépôt en a besoin
                    if chemin.contains(',') {
                        let quoi = format!("le chemin {} porte une virgule", cite(chemin));
                        let correction = "Sépare les chemins de `fichiers` par des blancs, sans virgule : Lydite lit \
                                          chaque mot comme un chemin, et `a.rs,` ou `a.rs,b.rs` serait un nom.";
                        return Err(refus(Code::PlanChemin, ligne, quoi, correction));
                    }
                    if let Some(faute) = hors_forme(chemin) {
                        let quoi = format!("le chemin {} {faute}", cite(chemin));
                        return Err(refus(Code::PlanChemin, ligne, quoi, CHEMIN));
                    }
                    if let Some(juste) = self.base.as_ref().and_then(|base| base.casse(chemin)) {
                        let quoi = format!("le chemin {} s'écrit {} dans la base", cite(chemin), cite(&juste));
                        let correction = format!(
                            "Écris-le {}, sous la casse que git lui connaît dans la base : Lydite compare les chemins \
                             exactement, et un écart de casse ferait rater le gel.",
                            cite(&juste)
                        );
                        return Err(refus(Code::PlanChemin, ligne, quoi, correction));
                    }
                }
                tache.fichiers = Some(chemins);
            }
            "exact" => {
                let Some((fichier, texte)) = valeur.split_once(" : ").filter(|(_, texte)| !texte.is_empty()) else {
                    let quoi =
                        format!("la valeur {} n'est pas `<fichier> : <texte>` d'un texte non vide", cite(valeur));
                    return Err(refus(Code::PlanChemin, ligne, quoi, EXACT));
                };
                if let Some(faute) = hors_forme(fichier) {
                    let quoi = format!("le chemin {} {faute}", cite(fichier));
                    return Err(refus(Code::PlanChemin, ligne, quoi, CHEMIN));
                }
                tache.exacts.push((ligne, fichier, texte));
            }
            "sha256" => {
                let Some((fichier, empreinte)) =
                    valeur.split_once(" : ").filter(|(_, empreinte)| !empreinte.is_empty())
                else {
                    let quoi = format!("the value {} is not `<file> : <hash>` with a hash", cite(valeur));
                    return Err(refus(Code::PlanChemin, ligne, quoi, SHA256));
                };
                let (blanc, dossier) =
                    (fichier.chars().any(|c| c.is_whitespace() || c.is_control()), fichier.ends_with('/'));
                if blanc || dossier || hors_forme(fichier).is_some() {
                    let quoi = format!(
                        "the file {} is not a file path as `fichiers` writes it, or has a blank or a control \
                         character, or is a folder",
                        cite(fichier)
                    );
                    return Err(refus(Code::PlanChemin, ligne, quoi, SHA256));
                }
                let hexa = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
                if empreinte.len() != 64 || !empreinte.bytes().all(hexa) {
                    let quoi = format!("the hash {} is not 64 lowercase hexadecimal digits", cite(empreinte));
                    return Err(refus(Code::PlanLigne, ligne, quoi, SHA256));
                }
                if let Some(&(avant, ..)) = tache.empreintes.iter().find(|&&(_, vu, _)| vu == fichier) {
                    let quoi = format!(
                        "the file {} is already pinned by the `sha256` line {} of this task",
                        cite(fichier),
                        avant.numero
                    );
                    return Err(refus(Code::PlanLigne, ligne, quoi, SHA256));
                }
                tache.empreintes.push((ligne, fichier, empreinte));
            }
            _ if valeur != "oui" => {
                let quoi = format!("la clé « critique » vaut {}, et sa seule valeur est `oui`", cite(valeur));
                let correction =
                    "Écris `critique : oui` pour une tâche dont le travail va à la relecture, ou retire la ligne.";
                return Err(refus(Code::PlanLigne, ligne, quoi, correction));
            }
            _ => tache.critique = true,
        }
        Ok(())
    }

    /// La fin du plan : il a un objectif, une preuve et des tâches, chacune d'un objectif (et, découpé, de
    /// `fichiers` et de `modele`), dont les dépendances sont des tâches du plan, sans cycle. Puis les tâches qui
    /// partagent un chemin passent en série (`en_serie`).
    fn finir(self) -> Result<(Plan, Vec<Attente>), Refus> {
        let manque = |cle: &str| {
            let correction = format!("Écris `{cle} : <valeur>` après `plan 1`, avant la première tâche.");
            Refus::new(Code::PlanBlanc, format!("l'en-tête n'a pas de ligne `{cle} : …`"), correction)
        };
        let objectif = self.objectif.ok_or_else(|| manque("objectif"))?;
        let preuve = self.preuve.ok_or_else(|| manque("preuve"))?;
        if self.taches.is_empty() {
            let correction = "Écris au moins une tâche : `tache <id>`, puis son objectif en lignes indentées.";
            return Err(Refus::new(Code::PlanBlanc, "le plan ne contient aucune tâche", correction));
        }
        let mut taches = Vec::with_capacity(self.taches.len());
        let mut dependances = Vec::with_capacity(self.taches.len());
        // Chaque fichier épinglé, avec la ligne, la tâche et l'empreinte de sa première mention.
        let mut epingles: HashMap<&str, (usize, &str, &str)> = HashMap::new();
        for brouillon in &self.taches {
            if brouillon.objectif.is_empty() {
                let quoi = format!(
                    "la tâche {} n'a pas d'objectif : aucune ligne indentée ne suit ses clés",
                    cite(brouillon.id)
                );
                let correction = "Écris l'objectif de la tâche en lignes indentées, sous ses clés.";
                return Err(refus(Code::PlanBlanc, brouillon.ligne, quoi, correction));
            }
            let absentes = match (self.mode, brouillon.fichiers.is_none(), brouillon.modele.is_none()) {
                (Mode::Main, ..) | (_, false, false) => None,
                (_, true, true) => Some("n'a ni `fichiers` ni `modele`"),
                (_, true, false) => Some("n'a pas de clé `fichiers`"),
                (_, false, true) => Some("n'a pas de clé `modele`"),
            };
            if let Some(absentes) = absentes {
                let quoi =
                    format!("la tâche {} {absentes}, que tout plan découpé donne à chaque tâche", cite(brouillon.id));
                return Err(refus(Code::PlanBorne, brouillon.ligne, quoi, DECOUPEE));
            }
            if let Some(fichiers) = &brouillon.fichiers {
                for &(ligne, fichier, _) in &brouillon.exacts {
                    if !fichiers.iter().any(|entree| couvre(entree, fichier)) {
                        let quoi = format!(
                            "le fichier {} n'est couvert par aucun chemin de `fichiers` de la tâche",
                            cite(fichier)
                        );
                        return Err(refus(Code::PlanChemin, ligne, quoi, EXACT));
                    }
                }
                for &(ligne, fichier, _) in &brouillon.empreintes {
                    if !fichiers.iter().any(|entree| couvre(entree, fichier)) {
                        let quoi =
                            format!("the file {} is not covered by any path of the task's `fichiers`", cite(fichier));
                        return Err(refus(Code::PlanChemin, ligne, quoi, SHA256));
                    }
                }
            }
            // Un fichier n'a qu'une empreinte dans tout le plan : la seconde ligne qui le dit autrement est refusée.
            for &(ligne, fichier, empreinte) in &brouillon.empreintes {
                match epingles.get(fichier) {
                    Some(&(avant, tache, autre)) if autre != empreinte => {
                        let quoi = format!(
                            "the file {} is already pinned under another hash by the task {} (line {avant})",
                            cite(fichier),
                            cite(tache)
                        );
                        return Err(refus(Code::PlanLigne, ligne, quoi, SHA256));
                    }
                    Some(_) => {}
                    None => {
                        epingles.insert(fichier, (ligne.numero, brouillon.id, empreinte));
                    }
                }
            }
            let (mut depend, mut rangs) = (Vec::new(), Vec::new());
            if let Some((ligne, ids)) = &brouillon.depend {
                for &id in ids {
                    if id == brouillon.id {
                        let quoi = format!("la tâche {} dépend d'elle-même", cite(id));
                        let correction = "Retire cet id de la ligne `depend` : une tâche ne part qu'après ses \
                                          dépendances, donc jamais après elle-même.";
                        return Err(refus(Code::PlanDependance, *ligne, quoi, correction));
                    }
                    let Some(&rang) = self.ids.get(id) else {
                        let quoi = format!("la dépendance {} n'est l'id d'aucune tâche du plan", cite(id));
                        let correction = "Nomme dans `depend` des ids de tâches du plan, ceux de leur ligne \
                                          `tache <id>`, séparés par des blancs.";
                        return Err(refus(Code::PlanDependance, *ligne, quoi, correction));
                    };
                    depend.push(id.to_string());
                    rangs.push(rang);
                }
            }
            dependances.push(rangs);
            taches.push(Tache {
                id: brouillon.id.to_string(),
                depend,
                preuve: brouillon.preuve.map(str::to_string),
                objectif: objectif_de(&brouillon.objectif),
                modele: brouillon.modele.map(str::to_string),
                relance: brouillon.relance.map(str::to_string),
                effort: brouillon.effort.map(str::to_string),
                fichiers: brouillon.fichiers.as_ref().map(|chemins| chemins.iter().map(|c| c.to_string()).collect()),
                exacts: brouillon
                    .exacts
                    .iter()
                    .map(|&(_, fichier, texte)| (fichier.to_string(), texte.to_string()))
                    .collect(),
                empreintes: brouillon
                    .empreintes
                    .iter()
                    .map(|&(_, fichier, empreinte)| (fichier.to_string(), empreinte.to_string()))
                    .collect(),
                critique: brouillon.critique,
            });
        }
        if let Some(cycle) = cycle(&dependances) {
            let mut ids: Vec<&str> = cycle.iter().map(|&rang| self.taches[rang].id).collect();
            ids.push(ids[0]);
            let premiere = &self.taches[cycle[0]];
            let ligne = premiere.depend.as_ref().map_or(premiere.ligne, |(ligne, _)| *ligne);
            let quoi = format!("cycle de dépendances : {} (chaque tâche dépend de la suivante)", ids.join(" → "));
            let correction = "Casse le cycle : retire de l'une de ces lignes `depend` la tâche qui reboucle, ou réunis \
                              les tâches qui s'attendent l'une l'autre.";
            return Err(refus(Code::PlanDependance, ligne, quoi, correction));
        }
        let attentes = en_serie(&mut taches, &dependances);
        Ok((Plan { objectif: objectif.to_string(), preuve: Some(preuve.to_string()), taches }, attentes))
    }
}

/// La correction d'une clé blanche, selon sa place.
fn blanche(entete: bool, cle: &str) -> String {
    match (entete, cle) {
        (true, _) => format!("Écris une valeur après `{cle} :` : sans elle, Lydite n'a rien à lire."),
        (false, "preuve") => {
            "Écris la commande après `preuve :`, ou retire la ligne : la tâche joue alors la preuve de la vague.".into()
        }
        (false, "modele" | "relance") => {
            format!("Écris le modèle après `{cle} :`, ou retire la ligne : la tentative prend alors celui du profil.")
        }
        (false, "effort") => format!("Écris le niveau après `effort :`, ou retire la ligne : {EFFORT}"),
        (false, "fichiers") => "Écris après `fichiers :` les chemins que la tâche peut changer, ou retire la ligne : \
                                rien ne borne alors la tâche."
            .into(),
        (false, "exact") => {
            "Write `exact : <file> : <text>` after the key, or remove the line: the task then requires \
                             no text in that file."
                .into()
        }
        (false, "sha256") => "Write `sha256 : <file> : <hash>` after the key (the file as in `fichiers`, then its \
                              SHA-256, 64 lowercase hexadecimal digits), or remove the line: the task then pins \
                              no file."
            .into(),
        (false, _) => "Écris `critique : oui`, ou retire la ligne : la tâche n'est alors pas critique.".into(),
    }
}

/// Au rejeu d'un découpage, une clé de l'en-tête porte ce que Kraken a demandé, tel quel aux blancs du bord près :
/// l'objectif et la preuve de l'humain (6026). Dans un autre mode, rien n'est attendu.
fn demandee(mode: Mode<'_>, ligne: Ligne<'_>, cle: &str, valeur: &str) -> Result<(), Refus> {
    let Mode::Decoupe(Some(demande)) = mode else { return Ok(()) };
    let attendue = if cle == "objectif" { demande.objectif } else { demande.preuve };
    if valeur == attendue.trim() {
        return Ok(());
    }
    let quoi = format!("la clé {} vaut {}, et Lydite a demandé {}", cite(cle), cite(valeur), cite(attendue));
    let correction = format!(
        "Écris l'en-tête que Lydite a demandé, tel quel : `objectif : {}`, puis `preuve : {}`.",
        demande.objectif.trim(),
        demande.preuve.trim()
    );
    Err(refus(Code::PlanEntete, ligne, quoi, correction))
}

/// Un modèle de `modele` ou de `relance`, comme au profil : en `[a-z0-9.-]`, sans `-` de tête, que `--model`
/// prendrait pour une option.
fn modele_permis(modele: &str) -> bool {
    !modele.starts_with('-')
        && modele.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b".-".contains(&c))
}

/// Ce qui écarte un chemin de `fichiers` de la forme que git écrit, s'il y a quelque chose : `None` pour un chemin
/// relatif à la racine du dépôt, en noms séparés par `/` (un `/` final pour un dossier), hors de `.git/` et de
/// `.claude/`.
pub(crate) fn hors_forme(chemin: &str) -> Option<&'static str> {
    if chemin.contains('\\') {
        return Some("s'écrit avec une barre inverse, et git sépare les noms par `/`");
    }
    let lecteur = matches!(chemin.as_bytes(), [lettre, b':', ..] if lettre.is_ascii_alphabetic());
    if chemin.starts_with('/') || lecteur {
        return Some("est absolu");
    }
    for nom in chemin.strip_suffix('/').unwrap_or(chemin).split('/') {
        // Windows ôte les points et les blancs de fin d'un nom : `.git.` y est `.git`.
        let lu = nom.trim_end_matches(['.', ' ']);
        if nom == ".." {
            return Some("sort du dépôt par `..`");
        }
        if lu.is_empty() {
            return Some("a un nom vide ou fait de points");
        }
        if lu.eq_ignore_ascii_case(".git") {
            return Some("est sous `.git/`");
        }
        if lu.eq_ignore_ascii_case(".claude") {
            return Some("est sous `.claude/`");
        }
    }
    None
}

/// L'arbre de la base : chaque chemin, fichier ou dossier (fini par `/`), tel que git l'écrit, puis le même par sa
/// forme en minuscules, de quoi dire la casse d'un chemin écrit autrement.
struct Base<'d> {
    exacts: HashSet<&'d str>,
    minuscules: HashMap<String, &'d str>,
}

impl<'d> Base<'d> {
    /// L'arbre de ces chemins de fichiers, et de leurs dossiers.
    fn new(chemins: &'d [String]) -> Base<'d> {
        let mut base = Base { exacts: HashSet::new(), minuscules: HashMap::new() };
        for chemin in chemins {
            let dossiers = chemin.match_indices('/').map(|(fin, _)| &chemin[..=fin]);
            for vu in iter::once(chemin.as_str()).chain(dossiers) {
                if base.exacts.insert(vu) {
                    base.minuscules.entry(vu.to_lowercase()).or_insert(vu);
                }
            }
        }
        base
    }

    /// Le chemin sous la casse de la base, s'il y paraît sous une autre : chacun de ses dossiers, puis lui, tels que la
    /// base les écrit ; un nom qu'elle ne connaît pas, et ce qui le suit, restent tels quels. `None` si le chemin est
    /// déjà sous sa casse, ou neuf.
    fn casse(&self, chemin: &str) -> Option<String> {
        let mut juste = String::with_capacity(chemin.len());
        let mut reste = chemin;
        while !reste.is_empty() {
            let (nom, suite) = reste.split_at(reste.find('/').map_or(reste.len(), |fin| fin + 1));
            let vu = format!("{juste}{nom}");
            if self.exacts.contains(vu.as_str()) {
                juste = vu;
            } else if let Some(&ecrit) = self.minuscules.get(&vu.to_lowercase()) {
                juste = ecrit.to_string();
            } else {
                juste = vu + suite;
                break;
            }
            reste = suite;
        }
        (juste != chemin).then_some(juste)
    }
}

/// Le chemin `entree` de `fichiers` couvre `chemin` : il l'est, ou c'est un dossier (fini par `/`) qui le contient. La
/// comparaison est exacte, octet à octet, comme git rend les chemins : sensible à la casse sous Windows comme ailleurs.
pub(crate) fn couvre(entree: &str, chemin: &str) -> bool {
    entree == chemin || (entree.ends_with('/') && chemin.starts_with(entree))
}

/// Le chemin que deux tâches partagent, s'il y en a un : de deux de leurs chemins dont l'un couvre l'autre, le plus
/// étroit. Une tâche sans `fichiers` n'en partage aucun.
fn partage<'t>(une: &'t Tache, autre: &'t Tache) -> Option<&'t str> {
    let (siens, autres) = (une.fichiers.as_deref()?, autre.fichiers.as_deref()?);
    siens.iter().find_map(|sien| {
        autres.iter().find_map(|autre| match (couvre(sien, autre), couvre(autre, sien)) {
            (true, _) => Some(autre.as_str()),
            (_, true) => Some(sien.as_str()),
            _ => None,
        })
    })
}

/// Un ordre topologique du plan : les dépendances d'abord, puis l'ordre du fichier. Parmi les tâches dont toutes les
/// dépendances sont placées, la première du fichier vient d'abord. Le plan est sans cycle (`cycle`) : toutes y sont.
fn topologique(dependances: &[Vec<usize>]) -> Vec<usize> {
    let mut restantes: Vec<usize> = dependances.iter().map(Vec::len).collect();
    let mut suivantes = vec![Vec::new(); dependances.len()];
    for (tache, siennes) in dependances.iter().enumerate() {
        for &dependance in siennes {
            suivantes[dependance].push(tache);
        }
    }
    let mut pretes: BinaryHeap<Reverse<usize>> =
        (0..dependances.len()).filter(|&tache| restantes[tache] == 0).map(Reverse).collect();
    let mut ordre = Vec::with_capacity(dependances.len());
    while let Some(Reverse(tache)) = pretes.pop() {
        ordre.push(tache);
        for &suivante in &suivantes[tache] {
            restantes[suivante] -= 1;
            if restantes[suivante] == 0 {
                pretes.push(Reverse(suivante));
            }
        }
    }
    ordre
}

/// Fait passer en série les tâches qui partagent un chemin, et rend les dépendances ajoutées, chacune aussi au
/// `depend` de sa tâche. Les tâches se prennent dans un ordre topologique du plan (`topologique`) : chacune regarde
/// celles d'avant, de la plus proche à la plus lointaine, et attend celle qui partage un de ses chemins et dont elle
/// ne dépend pas déjà, même de loin. Une suite de tâches qui partagent un chemin s'enchaîne donc d'une dépendance
/// à la suivante. Toute dépendance ajoutée va vers une tâche d'avant dans cet ordre, qui reste topologique : aucune
/// ne fait de cycle.
// raccourci : les tâches se comparent deux à deux, chemin à chemin, et le coût croît avec le carré du nombre de
// chemins ; un plan écrit à la main de milliers de tâches ou de chemins se lit lentement (un plan découpé a 12 tâches
// de `FICHIERS_MAX` chemins au plus, ce que `cle` impose) ; monter vers un index des chemins s'il s'en écrit
fn en_serie(taches: &mut [Tache], dependances: &[Vec<usize>]) -> Vec<Attente> {
    let mut attentes = Vec::new();
    if taches.iter().all(|tache| tache.fichiers.is_none()) {
        return attentes;
    }
    let ordre = topologique(dependances);
    // Les ancêtres de chaque tâche, celles dont elle dépend même de loin : un bit par rang du plan.
    let mots = taches.len().div_ceil(64);
    let mut ancetres: Vec<Vec<u64>> = vec![Vec::new(); taches.len()];
    for (place, &tache) in ordre.iter().enumerate() {
        let mut siens = vec![0; mots];
        for &dependance in &dependances[tache] {
            ajouter(&mut siens, &ancetres[dependance], dependance);
        }
        for &avant in ordre[..place].iter().rev() {
            if siens[avant / 64] & (1 << (avant % 64)) != 0 {
                continue;
            }
            let Some(chemin) = partage(&taches[tache], &taches[avant]).map(str::to_string) else { continue };
            let attend = taches[avant].id.clone();
            taches[tache].depend.push(attend.clone());
            attentes.push(Attente { tache: taches[tache].id.clone(), attend, chemin });
            ajouter(&mut siens, &ancetres[avant], avant);
        }
        ancetres[tache] = siens;
    }
    attentes
}

/// Ajoute aux ancêtres `siens` la tâche de rang `rang`, et ses ancêtres à elle, `les_siens`.
fn ajouter(siens: &mut [u64], les_siens: &[u64], rang: usize) {
    for (mot, autre) in siens.iter_mut().zip(les_siens) {
        *mot |= autre;
    }
    siens[rang / 64] |= 1 << (rang % 64);
}

/// Le texte du plan : ses octets en UTF-8, le BOM de tête ôté. Une marque UTF-16 ou un octet hors de l'UTF-8 est le
/// refus 6017, qui dit le rang de l'octet (à partir de 1, BOM compris) et sa ligne.
fn en_utf8(octets: &[u8]) -> Result<&str, Refus> {
    let marque = match octets {
        [0xFF, 0xFE, ..] => Some("FF FE"),
        [0xFE, 0xFF, ..] => Some("FE FF"),
        _ => None,
    };
    if let Some(marque) = marque {
        let preuve = format!("le plan commence par les octets {marque}, la marque d'un texte en UTF-16");
        return Err(Refus::new(Code::PlanEncodage, preuve, REECRIRE));
    }
    let (bom, utf8) = match octets.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(reste) => (3, reste),
        None => (0, octets),
    };
    str::from_utf8(utf8).map_err(|erreur| {
        let avant = erreur.valid_up_to();
        let ligne = utf8[..avant].iter().filter(|&&octet| octet == b'\n').count() + 1;
        let octet = utf8.get(avant).map_or_else(String::new, |octet| format!(" ({octet:#04X})"));
        let preuve = format!("l'octet de rang {}{octet}, ligne {ligne}, n'est pas de l'UTF-8", bom + avant + 1);
        Refus::new(Code::PlanEncodage, preuve, REECRIRE)
    })
}

/// Le refus d'une ligne du plan : son numéro et son texte d'abord, puis ce qui ne va pas.
fn refus(code: Code, ligne: Ligne<'_>, quoi: impl Display, correction: impl Into<String>) -> Refus {
    Refus::new(code, format!("ligne {} {} : {quoi}", ligne.numero, cite(ligne.texte)), correction)
}

/// Un bout du fichier pour une preuve : entre guillemets français, ses caractères de contrôle écrits (`\t`, `\x1b`), et
/// coupé à `CITE_MAX` caractères. Une ligne peut être immense, ou piéger un terminal.
fn cite(texte: &str) -> String {
    let mut citation = String::from("« ");
    for c in texte.chars().take(CITE_MAX) {
        if c.is_control() {
            citation.extend(c.escape_default());
        } else {
            citation.push(c);
        }
    }
    if texte.chars().nth(CITE_MAX).is_some() {
        citation.push('…');
    }
    citation.push_str(" »");
    citation
}

/// L'objectif d'une tâche : ses lignes sans leur indentation commune (la plus longue suite de blancs par quoi elles
/// commencent toutes), chacune après les retours à la ligne qui la séparent de la précédente.
fn objectif_de(lignes: &[(usize, &str)]) -> String {
    let commune = lignes
        .iter()
        .map(|(_, ligne)| &ligne[..ligne.len() - ligne.trim_start_matches([' ', '\t']).len()])
        .reduce(|commune, indentation| {
            let egaux = commune.bytes().zip(indentation.bytes()).take_while(|(a, b)| a == b).count();
            &commune[..egaux]
        })
        .unwrap_or_default();
    let mut objectif = String::new();
    for &(sauts, ligne) in lignes {
        objectif.extend(iter::repeat_n('\n', sauts));
        objectif.push_str(ligne.strip_prefix(commune).unwrap_or(ligne));
    }
    objectif
}

/// Un cycle des dépendances, s'il y en a un : le rang de ses tâches, chacune dépendant de la suivante, la dernière de
/// la première. Le parcours en profondeur se fait sans récursion : 10 000 tâches en chaîne n'empilent pas 10 000
/// appels.
fn cycle(dependances: &[Vec<usize>]) -> Option<Vec<usize>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Vu {
        Non,
        SurLeChemin,
        Fini,
    }
    let mut vus = vec![Vu::Non; dependances.len()];
    for depart in 0..dependances.len() {
        if vus[depart] != Vu::Non {
            continue;
        }
        vus[depart] = Vu::SurLeChemin;
        // Les tâches en cours, chacune avec le rang de la prochaine dépendance à suivre.
        let mut chemin = vec![(depart, 0)];
        while let Some((tache, suite)) = chemin.last_mut() {
            let Some(&voisine) = dependances[*tache].get(*suite) else {
                vus[*tache] = Vu::Fini;
                chemin.pop();
                continue;
            };
            *suite += 1;
            match vus[voisine] {
                Vu::Non => {
                    vus[voisine] = Vu::SurLeChemin;
                    chemin.push((voisine, 0));
                }
                Vu::SurLeChemin => {
                    let debut = chemin.iter().position(|&(tache, _)| tache == voisine)?;
                    return Some(chemin[debut..].iter().map(|&(tache, _)| tache).collect());
                }
                Vu::Fini => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
