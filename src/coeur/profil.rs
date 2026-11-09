//! Le profil d'une main : sa sorte, son modèle, ce qu'on lui prête et ce qu'on lui ferme, et le temps qu'on lui laisse.
//! C'est un fichier du dossier de Kraken, que l'humain écrit puis accepte (`lydite accepter <nom>`), et que son
//! empreinte garde avant chaque lancement. Aucun budget : il rendrait la main sous-productive. Un plafond de dépense
//! par main (`plafond`), non un budget : haut par défaut, il ne bride pas une main normale ; il arrête une main qui
//! s'emballe et rend la décision à l'humain.
//!
//! Le format :
//!
//! - Le fichier est `<dossier de Kraken>/profils/<nom>.profil`, le nom en `[A-Za-z0-9_-]`, hors du dépôt visé. Seul un
//!   fichier absent laisse place au profil intégré du même nom (`claude`).
//! - Il se lit en UTF-8, un BOM de tête et le `\r` des fins de ligne tolérés. Une ligne vaut `clé = valeur`, coupée au
//!   premier `=`, les blancs ôtés autour de la clé et de la valeur ; une ligne blanche, ou dont le premier caractère
//!   non blanc est `#`, ne dit rien.
//! - Chaque clé s'écrit une fois au plus, et sa valeur n'est ni vide ni en `-` (elle n'en commence pas). `outils`,
//!   `reseau` et `lecture` sont des listes à virgules, des blancs tolérés autour de chaque élément, qui suit la même
//!   règle.
//! - Les clés, et aucune autre, `budget` compris :
//!   - `sorte`, la seule obligatoire : `claude`, une main Claude Code ;
//!   - `programme` : un nom nu, que le système cherche, ou un chemin absolu ; par défaut `LYDITE_CLAUDE`, sinon
//!     `claude` ;
//!   - `modele`, et `relance` pour la seconde tentative : `sonnet` par défaut, en `[a-z0-9.-]`, un alias (`sonnet`,
//!     `haiku`, `opus`) ou un nom complet (`claude-sonnet-5-5`) ;
//!   - `decoupeur` : le modèle de la main qui découpe un objectif (`--decouper`), pour ses deux tentatives : `opus` par
//!     défaut, dans le domaine de `modele` ;
//!   - `relecteur` : le modèle de la main qui relit le travail des tâches critiques d'une vague, pour ses deux
//!     tentatives : `opus` par défaut, dans le domaine de `modele`. La main qui corrige ses remarques bloquantes prend
//!     `relance` ;
//!   - `api` et `cle`, ensemble ou pas du tout : le fournisseur d'une main qui parle à une autre API que celle
//!     d'Anthropic, au format d'Anthropic. `api` est l'URL de l'API, en `https://` : un hôte en `[A-Za-z0-9.-]`, puis
//!     un port et un chemin facultatifs, le chemin en `[A-Za-z0-9._~/-]`, ni identifiants ni requête. `cle` est le
//!     *nom* de la variable d'environnement de Kraken qui tient la clé, en `[A-Z0-9_]`, jamais la clé elle-même :
//!     l'empreinte du profil porte ce nom, jamais la valeur. Au lancement, la variable est posée et non blanche
//!     ([`Profil::fournir`]), sinon la main ne part pas. Une faute est le refus 6027, qui ne reprend jamais la valeur
//!     d'une de ces lignes. Seule la main reçoit le fournisseur, `ANTHROPIC_BASE_URL` et `ANTHROPIC_API_KEY`, un
//!     `ANTHROPIC_AUTH_TOKEN` hérité retiré : sa preuve n'a ni ces deux variables ni celle que `cle` nomme. Le fil
//!     note l'hôte de `api` à chaque lancement. Le mur, les réglages et les règles de la main ne changent pas ;
//!   - `prix`, avec `api` seulement : le prix des modèles chez ce fournisseur, une liste à virgules, un élément par
//!     modèle : son nom, dans le domaine de `modele`, puis de deux à quatre décimaux positifs, le prix d'un million de
//!     jetons d'entrée, de sortie, de cache lu, de cache écrit, en dollars, six décimales au plus
//!     (`prix = deepseek-flash 0.15 0.60 0.015`). La sortie contient déjà la réflexion, qui n'a pas de prix à elle.
//!     Sans prix du cache lu, le cache lu se compte au prix d'entrée ; sans prix du cache écrit, le cache écrit
//!     aussi. Un modèle redit, plus de cinq mots ou un nombre mal formé est un refus de profil. Un prix, non un
//!     budget : [`Profil::prix`] le rend en [`Prix`], en micro-dollars par million de jetons ;
//!   - `outils` : les sortants que le profil ouvre, chacun un nom de `SORTANTS` ou `mcp__<serveur>` (le serveur en
//!     `[A-Za-z0-9_-]`), ce dernier avec `mcp` seulement ;
//!   - `mcp` : une configuration MCP, un objet JSON sur la ligne, sans clé redite, qui porte l'objet `mcpServers`,
//!     dont les clés nomment les serveurs : passé tel quel à `--mcp-config` ;
//!   - `mur`, le bac à sable de Claude Code : `exige` (par défaut, Windows compris) ou `demande`.
//!     Sous un mur demandé, les commandes de la main peuvent tourner hors du bac à sable de Claude Code, y voient la
//!     clé du fournisseur, et la preuve se rejoue hors du mur ; `demande` ne vaut que si un profil l'écrit. Le profil
//!     intégré écrit `mur = exige` ;
//!   - `reseau` : les domaines que la main joint, en `[A-Za-z0-9.-]`, sans joker ni port ;
//!   - `lecture` : ce que les commandes de la main lisent hors du bac, des chemins absolus ou en `~/…`, sans `..` ni
//!     joker (`* ? [ ] { }`), qui ne recouvrent ni le dossier de Kraken ni un dossier de connexion de Claude
//!     (`CLAUDE_CONFIG_DIR`, le `.claude` du dossier personnel) : ni l'un d'eux, ni un de ses ancêtres, ni un chemin
//!     sous lui ;
//!   - `proteges` : les chemins du dépôt visé qu'aucune main ne change, une liste à virgules : chacun un fichier, ou
//!     un dossier fini par `/`, relatif à la racine du dépôt et de la forme des `fichiers` d'une tâche (voir
//!     `plan::hors_forme` : ni absolu, ni `..`, ni sous `.git/` ou `.claude/`), sans `( ) * ? [ ] { }` ni guillemet
//!     (un joker est refusé). Un chemin protégé l'emporte sur les `fichiers` d'une tâche : le réglage de la main
//!     refuse son édition, et un changement par une commande rougit la tentative puis refuse la tâche au gel (7018).
//!     Sans la clé, rien ne change, sauf que `.agents/skills/` (les leçons) est toujours protégé, clé ou non : une
//!     main ne change jamais une leçon, seul l'humain le fait. La main ne commite pas : son réglage refuse `git
//!     commit`, que Lydite fait au gel ;
//!   - `mesure` : un ou deux réglages d'un banc de mesure, jamais d'une vraie vague, parmi `sans-lecture` (le prompt
//!     système garde le bloc des leçons, mais pas la consigne de lecture) et `sans-a1` (la consigne d'une main de
//!     tâche perd ses deux phrases fixes d'honnêteté et de périmètre). Une autre valeur, ou un réglage redit, est un
//!     refus de profil. Le fil les note (`main.consigne.mesure`) et le reçu les dit : « Measurement: <réglages>. » ;
//!   - `delai_main` (3600 par défaut) et `delai_preuve` (300), en secondes, des entiers positifs : au premier, Kraken
//!     tue la main ; au second, la preuve qu'il rejoue ;
//!   - `plafond` (20 par défaut) : le plafond de dépense de chaque main, en dollars, un décimal positif de deux
//!     décimales au plus (`plafond = 7.5`). Chaque main Claude reçoit `--max-budget-usd <plafond>` : passé le plafond,
//!     Claude Code l'arrête, et sa tâche est refusée sans relance (7019). Un plafond, non un budget : haut, il ne
//!     bride pas une main normale ; il arrête une main qui s'emballe, sous-agents compris, et rend la décision à
//!     l'humain. Une valeur nulle, négative ou mal formée est un refus de profil.
//! - L'empreinte d'un profil est le SHA-256 des octets de son fichier. `lydite accepter <nom>` l'écrit dans
//!   `<dossier de Kraken>/profils/<nom>.empreinte`, en 64 hexadécimaux minuscules ; `lydite lancer` refuse un profil
//!   dont l'empreinte n'est pas celle-là. Le profil intégré a l'empreinte de son texte : elle s'inscrit au fil, sans
//!   être gardée. Le programme désigné n'est pas empreint, pas plus que le mur : ce sont des briques externes.
//!
//! ```text
//! # La main des corrections : Opus, Sonnet à la relance, le mur exigé.
//! sorte = claude
//! modele = opus
//! relance = sonnet
//! mur = exige
//! reseau = docs.exemple.test, paquets.exemple.test
//! lecture = ~/.cargo, ~/.rustup
//! delai_main = 1800
//! ```

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::erreur::{Code, Refus};
use crate::coeur::fil::lot;
use crate::coeur::fil::{Evenement, Mur};
use crate::coeur::json::{self, Json};
use crate::coeur::plan::hors_forme;

/// Les délais d'un profil qui ne les dit pas : une heure pour la main, cinq minutes pour la preuve que Kraken rejoue.
const DELAI_MAIN: Duration = Duration::from_secs(3600);
const DELAI_PREUVE: Duration = Duration::from_secs(300);

/// Le plafond de dépense d'une main qui ne le dit pas, en centimes de dollar : 20 dollars.
const PLAFOND_MAIN: u64 = 2000;

/// Le profil `claude` intégré, au format du fichier. Sonnet, et non le défaut du compte (Opus) : `--model` s'écrit
/// toujours.
const INTEGRE: &str =
    "sorte = claude\nmodele = sonnet\nrelance = sonnet\nmur = exige\ndelai_main = 3600\ndelai_preuve = 300\n";

/// Les clés d'un profil, et aucune autre.
const CLES: [&str; 19] = [
    "sorte",
    "programme",
    "modele",
    "relance",
    "decoupeur",
    "relecteur",
    "api",
    "cle",
    "prix",
    "outils",
    "mcp",
    "mur",
    "reseau",
    "lecture",
    "proteges",
    "delai_main",
    "delai_preuve",
    "plafond",
    "mesure",
];

/// Les clés dont la valeur est une liste à virgules.
const LISTES: [&str; 6] = ["prix", "outils", "reseau", "lecture", "proteges", "mesure"];

/// Les réglages de `mesure`, et aucun autre.
pub(crate) const SANS_LECTURE: &str = "sans-lecture";
pub(crate) const SANS_A1: &str = "sans-a1";

/// Les réglages d'un banc de mesure (clé `mesure`) : ce qu'une main ne reçoit pas. Jamais posés dans une vraie vague.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Mesure {
    /// Le prompt système garde le bloc des leçons, mais pas la consigne de lecture.
    pub(crate) sans_lecture: bool,
    /// La consigne d'une main de tâche perd `HONNETETE` et `PERIMETRE`.
    pub(crate) sans_a1: bool,
}

impl Mesure {
    /// Les réglages actifs, un par ligne (`main.consigne.mesure`), dans l'ordre `sans-lecture`, `sans-a1` ; vide si
    /// aucun n'est posé.
    pub(crate) fn texte(&self) -> String {
        let actifs = [(self.sans_lecture, SANS_LECTURE), (self.sans_a1, SANS_A1)];
        actifs.iter().filter(|(actif, _)| *actif).map(|(_, nom)| *nom).collect::<Vec<_>>().join("\n")
    }

    /// Les réglages d'une valeur de `mesure` (un ou deux, sans répétition), `None` si l'un est inconnu ou redit.
    fn lire(elements: &[&str]) -> Option<Mesure> {
        let mut mesure = Mesure::default();
        for element in elements {
            let pose = match *element {
                SANS_LECTURE => &mut mesure.sans_lecture,
                SANS_A1 => &mut mesure.sans_a1,
                _ => return None,
            };
            if std::mem::replace(pose, true) {
                return None;
            }
        }
        Some(mesure)
    }
}

/// Ce que la main ne lance jamais : `git commit`, que Lydite fait lui-même au gel. Les deux formes de préfixe des
/// règles de Claude Code, pour `Bash` comme pour `PowerShell`.
const COMMIT_REFUSE: [&str; 4] =
    ["Bash(git commit:*)", "Bash(git commit *)", "PowerShell(git commit:*)", "PowerShell(git commit *)"];

/// Ce que la main a d'office dans `permissions.allow` : ce qui, sinon, demanderait une permission dans son mode.
const PERMIS: [&str; 3] = ["Skill", "Workflow", "ExitPlanMode"];

/// Ce qui sort du bac, fermé par défaut par son nom nu dans `permissions.deny`, dans cet ordre : ce qui réveille une
/// main hors de Kraken, ce qui parle aux autres sessions de l'humain, ce qui publie ailleurs, ce qui demande à
/// l'humain, ce qui fait entrer du contenu extérieur. Un profil en ouvre par `outils`. Faits constatés (Claude Code
/// 2.1.312 sous Windows, 2.1.316 sous WSL) : un nom absent de la version ne cause ni erreur ni perte du
/// `--settings`, un nom présent retire l'outil de la session, et `Artifact` retire aussi `ArtifactComments` et
/// `ArtifactData`.
#[rustfmt::skip]
const SORTANTS: [&str; 19] = [
    "CronCreate", "CronDelete", "CronList", "ScheduleWakeup", "RemoteTrigger",
    "SendMessage", "ListAgents", "PushNotification",
    "SendUserFile", "Artifact", "ArtifactComments", "ArtifactData", "ArtifactCheck", "DesignSync", "SendToProject",
    "SendToProjectContext", "ShareOnboardingGuide",
    "AskUserQuestion",
    "WebSearch",
];

/// La correction du refus 6023 du dossier de Kraken.
const CHEMIN_A_REECRIRE: &str = "Pose LYDITE_DOSSIER à un chemin absolu qu'une règle de permission sait écrire : sous \
    Windows avec sa lettre de lecteur, et partout des noms sans parenthèse, joker, guillemet ni barre inverse ; puis \
    relance.";

/// La correction du refus 6023 du dossier de connexion de Claude que nomme `CLAUDE_CONFIG_DIR`.
const CONFIG_A_REECRIRE: &str = "Pose CLAUDE_CONFIG_DIR à un chemin absolu qu'une règle de permission sait écrire : \
    sous Windows avec sa lettre de lecteur, et partout des noms sans parenthèse, joker, guillemet ni barre inverse ; \
    puis relance.";

/// La correction du refus 6023 du `.claude` du dossier personnel.
const PERSONNEL_A_QUITTER: &str = "Pose CLAUDE_CONFIG_DIR à un chemin absolu qu'une règle de permission sait écrire \
    (sous Windows avec sa lettre de lecteur, et partout des noms sans parenthèse, joker, guillemet ni barre inverse), \
    connecte claude dans ce dossier, puis retire l'ancien fichier de connexion, .credentials.json, du .claude du \
    dossier personnel ; puis relance.";

/// La correction du même refus quand `CLAUDE_CONFIG_DIR` est posé : la connexion de claude vit là, et le fichier de
/// connexion du dossier personnel, qu'aucune règle ne sait fermer, n'a plus de raison d'y rester.
const PERSONNEL_A_RETIRER: &str = "Retire du .claude du dossier personnel son fichier de connexion, .credentials.json, \
    ou déplace-le : la connexion de claude vit dans CLAUDE_CONFIG_DIR ; puis relance.";

/// La correction du refus 6023 du git commun du dépôt visé.
const COMMUN_A_DEPLACER: &str = "Pose le dépôt visé, et son git commun, à un chemin absolu qu'une règle de \
    permission sait écrire : sous Windows avec sa lettre de lecteur, et partout des noms sans parenthèse, joker, \
    guillemet ni barre inverse ; puis relance.";

#[derive(Clone)]
pub(crate) struct Profil {
    sorte: Sorte,
    delai_main: Duration,
    delai_preuve: Duration,
    /// Le plafond de dépense de chaque main, en centimes de dollar (`plafond`).
    plafond: u64,
    /// Les chemins que nulle main ne change : [`LECONS`] d'abord, toujours, puis ceux de `proteges`, de la forme des
    /// `fichiers` d'une tâche.
    proteges: Vec<String>,
    /// Les réglages de la clé `mesure`, aucun par défaut.
    mesure: Mesure,
}

/// Le dossier des leçons du dépôt visé : protégé de toute main, que le profil le nomme ou non.
pub(crate) const LECONS: &str = ".agents/skills/";

#[derive(Clone)]
enum Sorte {
    /// Une main brute : sa commande telle quelle, rien de prêté ni de déclaré (`faux`).
    // raccourci : seul `Profil::faux`, des tests, la construit ; la première main qui n'est pas Claude Code (Hermes)
    // la construira pour de bon, et rustc dira alors cette attente inutile
    #[cfg_attr(not(test), expect(dead_code))]
    Brute(Vec<String>),
    /// Une main Claude Code, que son profil déclare.
    Claude(Box<Claude>),
}

#[derive(Clone)]
struct Claude {
    nom: String,
    /// Le fichier du profil, `None` pour le profil intégré.
    fichier: Option<PathBuf>,
    /// Le SHA-256 du fichier, ou du texte intégré, en 64 hexadécimaux minuscules.
    empreinte: String,
    programme: String,
    modele: String,
    relance: String,
    /// Le modèle du découpeur, pour ses deux tentatives.
    decoupeur: String,
    /// Le modèle du relecteur, pour ses deux tentatives.
    relecteur: String,
    /// Le fournisseur que le profil nomme, `None` pour une main qui parle chez Anthropic.
    fournisseur: Option<Fournisseur>,
    /// Les prix déclarés (`prix`) : le modèle, puis ses [`Prix`]. Vide sans `api`.
    prix: Vec<(String, Prix)>,
    /// Le mur exigé (`failIfUnavailable`), et non seulement demandé.
    exige: bool,
    /// Les sortants et les `mcp__<serveur>` déclarés, dans l'ordre du fichier.
    outils: Vec<String>,
    mcp: Option<String>,
    reseau: Vec<String>,
    lecture: Vec<String>,
    /// La ligne `lecture`, son numéro et son texte, que nomme un refus de `borner`.
    ligne_lecture: (usize, String),
    /// Ce que `borner` a posé ; sans lui, la main ne part pas.
    bornes: Option<Bornes>,
    /// Le programme qui rejoue la preuve sous un mur exigé : `srt`, ou ce que pose [`Profil::avec_srt`].
    srt: OsString,
}

/// Le fournisseur d'une main Claude, quand son profil en nomme un : une API au format d'Anthropic, et la variable
/// d'environnement de Kraken qui tient sa clé. Sans la valeur de la variable, que [`Profil::fournir`] lit au
/// lancement, la main ne part pas : elle parlerait à l'URL du fournisseur avec les identifiants de l'humain. Aucun
/// `Debug` : la valeur ne s'écrit nulle part.
#[derive(Clone)]
struct Fournisseur {
    /// L'URL de l'API, telle que le profil l'écrit (`ANTHROPIC_BASE_URL` de la main).
    api: String,
    /// Son hôte, port compris, que le fil note : un jeton, de l'ASCII sans blanc.
    hote: String,
    /// Le nom de la variable d'environnement qui tient la clé.
    cle: String,
    /// La clé, lue dans cette variable ; `None` tant que `fournir` ne l'a pas lue. Elle ne va qu'à la main
    /// (`ANTHROPIC_API_KEY`).
    valeur: Option<String>,
}

/// Ce que `borner` pose sur une main Claude : le dossier de Kraken, tel quel et en règle de permission, les dossiers de
/// connexion de Claude en règle, et le git commun du dépôt visé, tel quel et en règle.
#[derive(Clone)]
struct Bornes {
    dossier: String,
    regle: String,
    /// Les dossiers de connexion de Claude en règle, sans doublon, dans l'ordre où `borner` les prend.
    connexions: Vec<String>,
    git_commun: String,
    commun: String,
}

/// Ce que [`Profil::demande`] rend pour une tentative : la ligne de commande de la main, et ce qu'elle déclare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Demande {
    /// Le programme, puis ses arguments.
    pub(crate) argv: Vec<String>,
    /// `None` pour une main brute, qui ne déclare rien.
    pub(crate) declaration: Option<Declaration>,
}

/// Ce qu'une main Claude déclare de son lancement, dont le `main.lancement` du fil est fait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declaration {
    /// Le nom du profil.
    pub(crate) profil: String,
    /// Son empreinte, en 64 hexadécimaux minuscules.
    pub(crate) empreinte: String,
    /// Le modèle de la tentative : `modele` à la première, `relance` ensuite.
    pub(crate) modele: String,
    /// Vrai si le profil exige le mur, faux s'il le demande seulement.
    pub(crate) mur: bool,
    /// Le JSON exact de `--settings`.
    pub(crate) reglages: String,
    /// Les noms des serveurs du `mcpServers` de `--mcp-config`, triés : ceux que le constat attend à l'ouverture.
    /// `None` si `--mcp-config` ne part pas.
    pub(crate) mcp: Option<Vec<String>>,
    /// L'hôte de l'`api` du profil, le fournisseur déclaré de la main ; `None` si le profil n'en nomme aucun.
    pub(crate) fournisseur: Option<String>,
}

impl Declaration {
    /// Le `main.lancement` de la tentative `tentative` de la main `main` de la vague `vague` : ce que le profil
    /// déclare, et `reglages_depot`, les réglages du dépôt que le bac porte à cet instant, lus juste avant (voir
    /// `bac::reglages_du_bac`).
    pub(crate) fn lancement_selon(
        &self,
        vague: &str,
        main: usize,
        tentative: usize,
        reglages_depot: String,
    ) -> Evenement {
        Evenement::Lancement {
            vague: vague.into(),
            main,
            tentative,
            profil: self.profil.clone(),
            empreinte: self.empreinte.clone(),
            modele: self.modele.clone(),
            mur: if self.mur { Mur::Exige } else { Mur::Demande },
            reglages: self.reglages.clone(),
            reglages_depot,
        }
    }

    /// Le `main.fournisseur` de la même tentative, que le fil note juste après son lancement : l'hôte de l'`api` du
    /// profil, toujours déclaré. `None` si le profil ne nomme aucun fournisseur.
    pub(crate) fn fournisseur(&self, vague: &str, main: usize, tentative: usize) -> Option<Evenement> {
        let fournisseur = self.fournisseur.clone()?;
        Some(Evenement::Fournisseur { vague: vague.into(), main, tentative, fournisseur })
    }
}

impl Profil {
    /// Le temps que la vérification a pour finir : au-delà, elle est un échec.
    pub(crate) fn delai_preuve(&self) -> Duration {
        self.delai_preuve
    }

    pub(crate) fn delai_main(&self) -> Duration {
        self.delai_main
    }

    /// Le plafond de dépense de chaque main, en dollars, tel que la ligne de commande le porte : `20`, `7.5`, `0.25`.
    pub(crate) fn plafond(&self) -> String {
        let (dollars, centimes) = (self.plafond / 100, self.plafond % 100);
        match centimes {
            0 => dollars.to_string(),
            c if c % 10 == 0 => format!("{dollars}.{}", c / 10),
            c => format!("{dollars}.{c:02}"),
        }
    }

    /// Ce que `lydite profil` dit de ce profil, une rangée par ligne, dans l'ordre : le nom et sa source, l'empreinte,
    /// le modèle et la relance, le mur, la preuve, le plafond par main, les deux délais. Tout vient du profil lu, et
    /// rien d'autre n'est lu. Une main brute n'a pas de fichier : une seule rangée.
    pub(crate) fn rangees(&self) -> Vec<String> {
        let Sorte::Claude(claude) = &self.sorte else { return vec!["main brute : aucun profil à lire".into()] };
        let source = claude.fichier.as_ref().map_or_else(|| "intégré".to_string(), |f| f.display().to_string());
        let (mur, preuve) = if claude.exige {
            (
                "exigé : les commandes de la main tournent dans le bac à sable de Claude Code",
                "rejouée par srt, dans le mur",
            )
        } else {
            (
                "demandé : les commandes de la main peuvent tourner hors du bac à sable, et y voient la clé du \
                 fournisseur s'il y en a un",
                "rejouée hors du mur",
            )
        };
        vec![
            format!("profil : {} ({source})", claude.nom),
            format!("empreinte : {}", claude.empreinte),
            format!("modèle : {} ; relance : {}", claude.modele, claude.relance),
            format!("mur : {mur}"),
            format!("preuve : {preuve}"),
            format!("plafond par main : {} $", self.plafond()),
            format!("délai de la main : {} s", self.delai_main.as_secs()),
            format!("délai de la preuve : {} s", self.delai_preuve.as_secs()),
        ]
    }

    /// Les réglages de mesure du profil (clé `mesure`) ; aucun pour une main brute.
    pub(crate) fn mesure(&self) -> Mesure {
        self.mesure
    }

    /// Un profil de test aux réglages de mesure qu'on lui donne, comme le ferait la clé `mesure`.
    #[cfg(test)]
    pub(crate) fn avec_mesure(mut self, mesure: Mesure) -> Profil {
        self.mesure = mesure;
        self
    }

    /// Une copie de ce profil aux modèles d'une tâche : `modele` pour sa première tentative, `relance` pour la
    /// seconde, chacun à défaut celui du profil. Tout le reste est celui du profil (empreinte, bornes, mur, délais) :
    /// la copie se lance, se déclare et se constate comme lui, sous le modèle de la tâche, et le profil lui-même ne
    /// bouge pas. Une main brute n'a pas de modèle : sa copie est la même.
    pub(crate) fn avec_modeles(&self, modele: Option<&str>, relance: Option<&str>) -> Profil {
        let mut copie = self.clone();
        if let Sorte::Claude(claude) = &mut copie.sorte {
            if let Some(modele) = modele {
                claude.modele = modele.into();
            }
            if let Some(relance) = relance {
                claude.relance = relance.into();
            }
        }
        copie
    }

    /// Le modèle du découpeur, la clé `decoupeur` (`opus` par défaut), pour ses deux tentatives ; `None` pour une main
    /// brute, qui n'a pas de modèle.
    pub(crate) fn decoupeur(&self) -> Option<&str> {
        match &self.sorte {
            Sorte::Claude(claude) => Some(&claude.decoupeur),
            Sorte::Brute(_) => None,
        }
    }

    /// Le modèle du relecteur, la clé `relecteur` (`opus` par défaut), pour ses deux tentatives ; `None` pour une main
    /// brute, qui n'a pas de modèle.
    pub(crate) fn relecteur(&self) -> Option<&str> {
        match &self.sorte {
            Sorte::Claude(claude) => Some(&claude.relecteur),
            Sorte::Brute(_) => None,
        }
    }

    /// Le modèle de la relance, la clé `relance` (`sonnet` par défaut) : celui de la seconde tentative d'une main,
    /// et de la main qui corrige les remarques bloquantes d'une relecture, à ses deux tentatives. `None` pour une main
    /// brute, qui n'a pas de modèle.
    pub(crate) fn relance(&self) -> Option<&str> {
        match &self.sorte {
            Sorte::Claude(claude) => Some(&claude.relance),
            Sorte::Brute(_) => None,
        }
    }

    /// Ce que la main reçoit de son fournisseur, `(URL de l'API, clé)`, soit `ANTHROPIC_BASE_URL` et
    /// `ANTHROPIC_API_KEY`. `None` sans fournisseur, une main brute comprise, et tant que [`Profil::fournir`] n'a pas
    /// lu la clé : la main d'un tel profil ne part pas alors (voir [`Profil::demande`]).
    pub(crate) fn fournisseur(&self) -> Option<(&str, &str)> {
        let Sorte::Claude(claude) = &self.sorte else { return None };
        let fournisseur = claude.fournisseur.as_ref()?;
        Some((fournisseur.api.as_str(), fournisseur.valeur.as_deref()?))
    }

    /// Le nom de la variable d'environnement qui tient la clé du fournisseur, `None` sans fournisseur : la preuve que
    /// Kraken rejoue ne la reçoit pas, car la clé ne va qu'à la main.
    pub(crate) fn variable_de_la_cle(&self) -> Option<&str> {
        let Sorte::Claude(claude) = &self.sorte else { return None };
        claude.fournisseur.as_ref().map(|fournisseur| fournisseur.cle.as_str())
    }

    /// Les chemins qu'aucune main ne change : [`LECONS`], toujours (sans la clé `proteges`, et pour une main brute
    /// aussi), puis les `proteges` du profil.
    pub(crate) fn proteges(&self) -> &[String] {
        &self.proteges
    }

    /// Un profil de test aux chemins protégés qu'on lui donne, comme le ferait la clé `proteges` : les leçons
    /// restent protégées.
    #[cfg(test)]
    pub(crate) fn avec_proteges(mut self, proteges: &[&str]) -> Profil {
        self.proteges = [LECONS].iter().chain(proteges).map(|chemin| chemin.to_string()).collect();
        self
    }

    /// Les [`Prix`] d'un modèle chez le fournisseur du profil, en micro-dollars par million de jetons ; `None` si le
    /// profil ne lui donne pas de prix. Un prix du cache que le profil ne donne pas vaut le prix d'entrée.
    pub(crate) fn prix(&self, modele: &str) -> Option<Prix> {
        let Sorte::Claude(claude) = &self.sorte else { return None };
        claude.prix.iter().find(|(nom, _)| nom == modele).map(|(_, prix)| *prix)
    }

    /// Un profil de test : la commande et le délai qu'on lui donne.
    #[cfg(test)]
    pub(crate) fn faux(main: Vec<String>, delai_preuve: Duration) -> Profil {
        Profil {
            sorte: Sorte::Brute(main),
            delai_main: DELAI_MAIN,
            delai_preuve,
            plafond: PLAFOND_MAIN,
            proteges: vec![LECONS.to_string()],
            mesure: Mesure::default(),
        }
    }

    /// Le profil a l'empreinte que l'humain a acceptée : celle de `<nom>.empreinte`, à côté de son fichier, que `lire`
    /// lit (`None` s'il est absent), comme `preparer_dans` la lit dans le dossier de Kraken. Absente, c'est le refus
    /// 6011 ; autre, le refus 6012, qui dit l'acceptée et la lue ; illisible, le refus 6006, que `lire` rend. Le profil
    /// intégré et une main brute n'ont rien à garder : `lire` n'est pas appelée.
    pub(crate) fn garder_selon(&self, lire: impl Fn(&Path) -> Result<Option<String>, Refus>) -> Result<(), Refus> {
        let Sorte::Claude(claude) = &self.sorte else { return Ok(()) };
        let Some(fichier) = &claude.fichier else { return Ok(()) };
        let (garde, nom, lue) = (fichier.with_extension("empreinte"), &claude.nom, &claude.empreinte);
        let Some(acceptee) = lire(&garde)? else {
            return Err(Refus::new(
                Code::ProfilNonAccepte,
                format!("{} n'a pas d'empreinte acceptée : {} est absent", fichier.display(), garde.display()),
                format!("Relis {}, puis accepte-le : lydite accepter {nom}.", fichier.display()),
            ));
        };
        if acceptee != *lue {
            return Err(Refus::new(
                Code::ProfilChange,
                format!("{} a changé : empreinte acceptée « {acceptee} », empreinte lue « {lue} »", fichier.display()),
                format!("Relis {}, puis réaccepte-le : lydite accepter {nom}.", fichier.display()),
            ));
        }
        Ok(())
    }

    /// Pose sur une main Claude ce que ses réglages tiennent du lancement : le dossier de Kraken et les dossiers de
    /// connexion de Claude, que ses règles ferment, et le git commun du dépôt visé, que ses commandes lisent et que
    /// ses règles ferment en écriture. Les dossiers de connexion sont `config`, la valeur de `CLAUDE_CONFIG_DIR` si
    /// elle est posée, puis le `.claude` de `personnel`, le dossier personnel, qui mène aussi les `~/` de `lecture` ;
    /// deux de même règle n'en font qu'un. Un chemin qu'une règle ne sait pas écrire, relatif compris, git commun
    /// compris, est le refus 6023, comme un chemin qui n'est pas de l'UTF-8 : sa correction dit quoi faire pour ce
    /// chemin-là. `CLAUDE_CONFIG_DIR` posé, la connexion de la main vit là : la paire du `.claude` du dossier
    /// personnel n'est plus exigée. Il se ferme encore si une règle l'écrit, mais sans fichier de connexion
    /// (`.credentials.json`, dont seule l'existence se regarde, jamais le contenu), un chemin qu'une règle ne sait
    /// pas écrire n'est ni fermé ni refusé ; avec lui, c'est le refus 6023, dont la correction dit de retirer ce
    /// fichier ou de le déplacer. Sans `CLAUDE_CONFIG_DIR`, rien ne change : ce `.claude` se ferme, ou est refusé.
    /// Une `lecture` qui recouvre l'un de ces dossiers, fermé ou non, ou mène par `~/` à un dossier personnel
    /// inconnu, est le refus 6009. Une main brute n'a rien à borner. `presente` dit si un dossier de connexion garde
    /// son fichier de connexion, que `preparer_dans` regarde sur le disque ; elle n'est appelée que pour un chemin
    /// refusé, `CLAUDE_CONFIG_DIR` posé.
    // raccourci : les chemins de `lecture` se comparent tels qu'écrits, un lien vers le dossier de Kraken passe ;
    // monter (canoniser) si un profil doit s'en garder
    pub(crate) fn borner_selon(
        &mut self,
        dossier: &Path,
        git_commun: &Path,
        config: Option<&Path>,
        personnel: Option<&Path>,
        presente: impl Fn(&Path) -> bool,
    ) -> Result<(), Refus> {
        let Sorte::Claude(claude) = &mut self.sorte else { return Ok(()) };
        let (kraken, de_config, de_personnel) = (
            "le dossier de Lydite",
            "le dossier de connexion de Claude (CLAUDE_CONFIG_DIR)",
            "le dossier de connexion de Claude (dossier personnel)",
        );
        let regle = en_regle(dossier, kraken, CHEMIN_A_REECRIRE)?;
        let commun = en_regle(git_commun, "le git commun du dépôt visé", COMMUN_A_DEPLACER)?;
        let corrige = if config.is_some() { PERSONNEL_A_RETIRER } else { PERSONNEL_A_QUITTER };
        let de_la_config = config.map(|config| (config.to_path_buf(), de_config, CONFIG_A_REECRIRE));
        let du_personnel = personnel.map(|personnel| (personnel.join(".claude"), de_personnel, corrige));
        let mut regles = Vec::new();
        if let Some((chemin, quoi, correction)) = &de_la_config {
            regles.push(en_regle(chemin, quoi, correction)?);
        }
        if let Some((chemin, quoi, correction)) = &du_personnel {
            match en_regle(chemin, quoi, correction) {
                Ok(regle) if !regles.contains(&regle) => regles.push(regle),
                Err(refus) if config.is_none() || presente(chemin) => return Err(refus),
                _ => {}
            }
        }
        let mut gardes = vec![(dossier, kraken)];
        let connexions = de_la_config.iter().chain(&du_personnel);
        gardes.extend(connexions.map(|(chemin, quoi, _)| (chemin.as_path(), *quoi)));
        let personnel = personnel.filter(|personnel| personnel.is_absolute());
        for entree in &claude.lecture {
            let chemin = match entree.strip_prefix("~/") {
                Some(reste) => personnel.map(|personnel| personnel.join(reste)),
                None => Some(PathBuf::from(entree)),
            };
            let faute = match chemin {
                Some(chemin) => {
                    let recouvre = |garde: &Path| garde.starts_with(&chemin) || chemin.starts_with(garde);
                    let Some((garde, quoi)) = gardes.iter().find(|(garde, _)| recouvre(garde)) else { continue };
                    format!(
                        "une lecture ne recouvre pas {quoi} {} (ni lui, ni un de ses ancêtres, ni un chemin sous lui), \
                         et {entree} le recouvre",
                        garde.display()
                    )
                }
                None => format!("{entree} mène par ~/ au dossier personnel, qui n'est pas connu en chemin absolu"),
            };
            let (n, ligne) = &claude.ligne_lecture;
            return Err(ligne_refusee(&claude.lieu(), &claude.nom, *n, ligne, &faute));
        }
        claude.bornes = Some(Bornes {
            dossier: dossier.to_string_lossy().into_owned(),
            regle,
            connexions: regles,
            git_commun: git_commun.to_string_lossy().into_owned(),
            commun,
        });
        Ok(())
    }

    /// Lit au lancement la clé du fournisseur du profil, dans la variable d'environnement que `cle` nomme : `lire` la
    /// cherche par son nom, que `preparer` fait lire dans l'environnement, pour qu'aucun test n'y touche. Absente, qui
    /// n'est pas de l'UTF-8, vide ou blanche, c'est le refus 6027, dont la preuve nomme la variable et jamais sa
    /// valeur. Tant que la clé n'est pas lue, la main d'un profil qui nomme un fournisseur ne part pas (voir
    /// [`Profil::demande`]). Sans fournisseur, comme pour une main brute, il n'y a rien à lire.
    pub(crate) fn fournir(&mut self, lire: impl Fn(&str) -> Option<OsString>) -> Result<(), Refus> {
        let Sorte::Claude(claude) = &mut self.sorte else { return Ok(()) };
        let lieu = claude.lieu();
        let Some(fournisseur) = &mut claude.fournisseur else { return Ok(()) };
        let (nom, hote) = (fournisseur.cle.clone(), fournisseur.hote.clone());
        let refus = |motif: &str| {
            Refus::new(
                Code::Fournisseur,
                format!("{lieu} : la variable d'environnement {nom}, que cle nomme, {motif}"),
                format!(
                    "Pose {nom} dans l'environnement de Lydite, à la clé du fournisseur {hote} (sa valeur ne s'écrit \
                     jamais dans le profil), puis relance."
                ),
            )
        };
        let Some(valeur) = lire(&nom) else { return Err(refus("n'est pas posée")) };
        let Ok(valeur) = valeur.into_string() else { return Err(refus("n'est pas de l'UTF-8")) };
        if valeur.trim().is_empty() {
            return Err(refus("est vide"));
        }
        fournisseur.valeur = Some(valeur);
        Ok(())
    }

    /// La preuve est prêtée à la main, qui peut la jouer elle-même : voir `pret`. Une main brute n'a rien de prêté.
    pub(crate) fn prete(&self, verifier: Option<&str>) -> bool {
        matches!(self.sorte, Sorte::Claude(_)) && verifier.and_then(pret).is_some()
    }

    /// Le profil exige le mur : ses preuves se rejouent par srt ([`Profil::rejeu`]). Une main brute ne l'exige pas.
    pub(crate) fn exige(&self) -> bool {
        matches!(&self.sorte, Sorte::Claude(claude) if claude.exige)
    }

    /// Le programme qui rejoue la preuve sous un mur exigé, `srt` sinon : `preparer` y pose `LYDITE_SRT`, et un test
    /// son faux srt, sans toucher à l'environnement. Une main brute n'a pas de mur.
    pub(crate) fn avec_srt(&mut self, srt: OsString) {
        if let Sorte::Claude(claude) = &mut self.sorte {
            claude.srt = srt;
        }
    }

    /// Sous un mur exigé, ce qui rejoue une preuve dans `worktree` : le programme srt, un fichier de réglages neuf du
    /// dossier de Kraken, `srt-<lot>.json`, et ce qu'il porte, toutes ses clés écrites, même vides, car srt refuse un
    /// fichier incomplet :
    ///
    /// ```text
    /// {"filesystem":{"denyRead":[…],"allowRead":[…],"allowWrite":["<worktree>","<temporaire>"],
    /// "denyWrite":["<git commun>"]},"network":{"allowedDomains":[<reseau>…],"deniedDomains":[]}}
    /// ```
    ///
    /// `denyRead`, `allowRead` et `allowedDomains` sont ceux du `--settings` d'une main posée dans `worktree` (voir
    /// [`Profil::demande`]), qui écrit dans son dossier de travail ; `temporaire`, le dossier temporaire du worktree
    /// (`bac::temporaire_de`), s'ouvre aussi en écriture ; le git commun du dépôt visé, lu, se ferme en écriture,
    /// comme la règle `Edit(<git commun en règle>/**)` de la main le ferme à son bac à sable. Fait constaté : Claude
    /// Code y met ses sockets de pont (`<dossier temporaire>/claude-http-<16 hex>.sock`) et donne à ses commandes
    /// `<dossier temporaire>/claude-<uid>` en `TMPDIR`. `None` sous un mur demandé et pour une main brute : la preuve
    /// se rejoue par la coquille. Un mur exigé que `borner` n'a pas borné est une erreur : la preuve ne se rejoue
    /// pas.
    pub(crate) fn rejeu(&self, worktree: &Path, temporaire: &Path) -> Result<Option<Srt>, String> {
        let Sorte::Claude(claude) = &self.sorte else { return Ok(None) };
        if !claude.exige {
            return Ok(None);
        }
        let Some(bornes) = &claude.bornes else {
            return Err("le mur est exigé, et le profil n'est pas borné : la preuve ne se rejoue pas".into());
        };
        let (fermes, lus, domaines) = claude.regles(bornes, worktree);
        let reglages = format!(
            concat!(
                r#"{{"filesystem":{{"denyRead":{fermes},"allowRead":{lus},"allowWrite":{ecrits},"#,
                r#""denyWrite":{commun}}},"#,
                r#""network":{{"allowedDomains":{domaines},"deniedDomains":[]}}}}"#,
            ),
            fermes = tableau(fermes),
            lus = tableau(lus),
            ecrits = tableau([worktree, temporaire].map(|chemin| chemin.to_string_lossy().into_owned())),
            commun = tableau([bornes.git_commun.clone()]),
            domaines = tableau(domaines),
        );
        Ok(Some(Srt {
            programme: claude.srt.clone(),
            fichier: Path::new(&bornes.dossier).join(format!("srt-{}.json", lot())),
            reglages,
            temporaire: temporaire.into(),
        }))
    }

    /// La ligne de commande d'une tentative dans `bac`, et sa déclaration ; pure, deux appels rendent la même. Une
    /// main brute part telle quelle, sans rien déclarer. Une main Claude que `borner` n'a pas bornée ne part pas : son
    /// argv est vide ; ni celle d'un profil qui nomme un fournisseur, tant que `fournir` n'a pas lu sa clé. Le
    /// fournisseur ne change ni l'argv ni les réglages : la main parle à l'URL de son `api` par son environnement
    /// (voir [`Profil::fournisseur`]), et sa déclaration en dit l'hôte. Une main Claude bornée :
    ///
    /// `<programme> -p --input-format stream-json --output-format stream-json --verbose --permission-mode acceptEdits
    /// --permission-prompts none --strict-mcp-config [--mcp-config <mcp>] --setting-sources user --model <m>
    /// --settings <json> --max-budget-usd <plafond>`
    ///
    /// - `--max-budget-usd` porte le `plafond` du profil : passé ce plafond, Claude Code arrête la main ;
    /// - `<m>` vaut `modele`, ou `relance` à la relance ; il s'écrit toujours, car sans lui la main tourne sur Opus ;
    /// - `--permission-mode acceptEdits` l'emporte sur un `defaultMode` de `--settings` ou du dépôt visé, et garde les
    ///   outils de fichiers au bac ; avec `--permission-prompts none`, ce qui demanderait une permission est refusé,
    ///   jamais demandé ;
    /// - `--strict-mcp-config` écarte les serveurs MCP de l'humain, et `--mcp-config` ne part qu'avec `mcp`, dont la
    ///   déclaration porte les noms des serveurs (voir [`serveurs`]) ;
    /// - `--setting-sources user` écarte les réglages du dépôt visé, `.claude/settings.json` et
    ///   `.claude/settings.local.json` (leur `env`, leur `apiKeyHelper`, leurs règles) : la main ne les charge plus, et
    ///   le `CLAUDE.md` du dépôt visé n'est pas lu non plus (constaté avec Claude Code 2.1.317) ;
    /// - aucun `--tools` : la main a tout l'ensemble par défaut de la version installée, moins ce que `deny` ferme ;
    /// - `<json>` reste une chaîne (cassée, elle arrête la main ; un fichier cassé serait ignoré en silence), compacte,
    ///   ses clés dans cet ordre et ses chaînes écrites par `json::chaine` :
    ///   ```text
    ///   {"sandbox":{"enabled":true,"failIfUnavailable":<mur exigé>,"allowUnsandboxedCommands":false,<exclues>
    ///   "filesystem":{"denyRead":["~/","<dossier>"],"allowRead":["<bac>","<git commun>",<lecture>…]},
    ///   "network":{"allowedDomains":[<reseau>…],"strictAllowlist":true},
    ///   "credentials":{"envVars":[{"name":"ANTHROPIC_API_KEY","mode":"deny"},
    ///   {"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}},
    ///   "permissions":{"allow":["Skill","Workflow","ExitPlanMode",<WebFetch(domain:<d>)>…,<sortants déclarés>…,
    ///   <mcp__<serveur> déclarés>…,<prêt de la preuve>…],"deny":[<sortants non déclarés>…,
    ///   "Edit(<dossier en règle>/fil.jsonl)","Edit(<dossier en règle>/profils/**)",
    ///   "Read(<dossier en règle>/srt-*.json)","Edit(<dossier en règle>/srt-*.json)",
    ///   <"Read(<connexion en règle>/.credentials.json)","Edit(<connexion en règle>/.credentials.json)">…,
    ///   "Edit(<git commun en règle>/**)","Bash(git commit:*)","Bash(git commit *)","PowerShell(git commit:*)",
    ///   "PowerShell(git commit *)",<"Edit(<bac en règle>/<chemin protégé>)", ou pour un dossier
    ///   "Edit(<bac en règle>/<dossier>/**)">…]},
    ///   "disableAllHooks":true,"autoMemoryEnabled":false}
    ///   ```
    ///   `credentials` ferme aux commandes du bac à sable la clé du fournisseur et un jeton hérité, que la main tient
    ///   de son environnement (constaté avec Claude Code 2.1.317 : sans la règle, une commande les lit).
    ///   `<exclues>` n'est rien, sauf sous un mur demandé quand un morceau de la preuve est prêté :
    ///   `"excludedCommands":[<morceaux prêtés>…],`, les morceaux des règles du prêt, dans leur ordre. Sous un mur
    ///   demandé, la preuve prêtée tourne ainsi hors du bac à sable de Claude Code, comme Kraken la rejoue ensuite :
    ///   sans bac à sable (sous Windows), `allowUnsandboxedCommands` faux la refuserait sinon à la main, même permise
    ///   par sa règle. Sous un mur exigé, elle reste dans le bac à sable. `allowUnsandboxedCommands`
    ///   reste faux partout : vrai, il ouvrirait la sortie du bac à sable à toute commande permise, lectures comprises.
    ///   Les sortants s'y suivent dans l'ordre de `SORTANTS`. Une valeur mal typée ferait ignorer tout le
    ///   `--settings`, une règle mal formée tomberait seule, sans un mot : seul le constat le voit, et l'égalité exacte
    ///   des tests garde l'orthographe. Les deux `Edit(...)` ferment l'écriture du fil et des profils, quoi qu'ouvrent
    ///   d'autres réglages, car un refus l'emporte ; `Read(...)` et `Edit(...)` ferment aux outils de la main les
    ///   réglages de srt du rejeu, `srt-<lot>.json` (voir [`Profil::rejeu`]) ; de même, pour chaque dossier de
    ///   connexion de Claude que `borner` a posé, `Read(...)` et `Edit(...)` ferment aux outils de la main son fichier
    ///   de connexion, `.credentials.json`.
    ///   Le dernier `Edit(...)` ferme le git commun du dépôt visé, que le bac à sable de Claude Code ouvre d'office en
    ///   écriture à une main posée dans un worktree : il le reprend en `denyWrite`, et les commandes ne l'écrivent pas.
    ///   Sans `autoMemoryEnabled` faux, une main posée dans un worktree prendrait pour mémoire celle du dépôt
    ///   principal, hors du bac. `lecture` ne va qu'à `allowRead`, pour les commandes : un dossier de travail de plus
    ///   ouvrirait aussi les éditions.
    ///
    /// `consigne` est le fichier des leçons que la main reçoit (voir `dossier::ecrire_consigne`) : l'argv de
    /// la main Claude porte alors, après `--settings`, `--append-system-prompt-file <chemin>`. Une main brute n'en
    /// reçoit pas.
    pub(crate) fn demande_avec(
        &self,
        bac: &Path,
        verifier: Option<&str>,
        relance: bool,
        consigne: Option<&Path>,
    ) -> Demande {
        let claude = match &self.sorte {
            Sorte::Brute(argv) => return Demande { argv: argv.clone(), declaration: None },
            Sorte::Claude(claude) => claude,
        };
        // Sans la clé de son fournisseur, une main parlerait à son URL avec les identifiants de l'humain : elle ne
        // part pas.
        let sans_cle = claude.fournisseur.as_ref().is_some_and(|fournisseur| fournisseur.valeur.is_none());
        let Some(bornes) = claude.bornes.as_ref().filter(|_| !sans_cle) else {
            return Demande { argv: Vec::new(), declaration: None };
        };
        // Une règle `Edit` par chemin protégé, dans le bac : un bac qu'une règle ne sait pas écrire ne ferait pas
        // tomber ces règles seules, en silence, et la main ne part pas.
        let protegees = match protegees(&self.proteges, bac) {
            Ok(regles) => regles,
            Err(()) => return Demande { argv: Vec::new(), declaration: None },
        };
        let modele = if relance { &claude.relance } else { &claude.modele };
        let declare = |sortant: &str| claude.outils.iter().any(|outil| outil == sortant);
        let pretes = verifier.and_then(pret).unwrap_or_default();
        let allow = PERMIS
            .iter()
            .map(|permis| permis.to_string())
            .chain(claude.reseau.iter().map(|domaine| format!("WebFetch(domain:{domaine})")))
            .chain(SORTANTS.iter().filter(|sortant| declare(sortant)).map(|sortant| sortant.to_string()))
            .chain(claude.outils.iter().filter(|outil| outil.starts_with("mcp__")).cloned())
            .chain(pretes.iter().flat_map(|morceau| [format!("Bash({morceau})"), format!("PowerShell({morceau})")]));
        let connexion = |regle: &String| ["Read", "Edit"].map(|outil| format!("{outil}({regle}/.credentials.json)"));
        let deny = SORTANTS
            .iter()
            .filter(|sortant| !declare(sortant))
            .map(|sortant| sortant.to_string())
            .chain(["fil.jsonl", "profils/**"].map(|garde| format!("Edit({}/{garde})", bornes.regle)))
            .chain(["Read", "Edit"].map(|outil| format!("{outil}({}/srt-*.json)", bornes.regle)))
            .chain(bornes.connexions.iter().flat_map(connexion))
            .chain([format!("Edit({}/**)", bornes.commun)])
            .chain(COMMIT_REFUSE.map(String::from))
            .chain(protegees);
        let (fermes, lus, domaines) = claude.regles(bornes, bac);
        // raccourci : sous un mur demandé et actif, la preuve prêtée tourne hors du mur pendant le travail de la main,
        // ce que personne n'a joué sous Linux ; monter (la laisser dans le bac à sable quand il est actif) si une vraie
        // main Linux sous le mur le montre utile
        let exclues = if claude.exige || pretes.is_empty() {
            String::new()
        } else {
            format!(r#""excludedCommands":{},"#, tableau(pretes.iter().copied().map(String::from)))
        };
        let reglages = format!(
            concat!(
                r#"{{"sandbox":{{"enabled":true,"failIfUnavailable":{mur},"allowUnsandboxedCommands":false,"#,
                r#"{exclues}"filesystem":{{"denyRead":{fermes},"allowRead":{lus}}},"#,
                r#""network":{{"allowedDomains":{domaines},"strictAllowlist":true}},"#,
                r#""credentials":{{"envVars":[{{"name":"ANTHROPIC_API_KEY","mode":"deny"}},"#,
                r#"{{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}}]}}}},"#,
                r#""permissions":{{"allow":{allow},"deny":{deny}}},"disableAllHooks":true,"autoMemoryEnabled":false}}"#,
            ),
            mur = claude.exige,
            exclues = exclues,
            fermes = tableau(fermes),
            lus = tableau(lus),
            domaines = tableau(domaines),
            allow = tableau(allow),
            deny = tableau(deny),
        );
        let mut argv = Vec::from(
            [
                claude.programme.as_str(),
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-mode",
                "acceptEdits",
                "--permission-prompts",
                "none",
                "--strict-mcp-config",
            ]
            .map(String::from),
        );
        if let Some(mcp) = &claude.mcp {
            argv.extend(["--mcp-config".to_string(), mcp.clone()]);
        }
        argv.extend(["--setting-sources", "user", "--model"].map(String::from));
        argv.extend([modele.clone(), "--settings".to_string(), reglages.clone()]);
        // raccourci : pour un profil qui nomme un autre fournisseur (`api`), Claude Code compte la dépense aux prix
        // qu'il connaît, peut-être nuls ; le plafond peut alors ne jamais jouer. Monter vers un compte de Lydite au
        // prix du profil (`prix`) si un banc le montre.
        argv.extend(["--max-budget-usd".to_string(), self.plafond()]);
        if let Some(consigne) = consigne {
            argv.extend(["--append-system-prompt-file".to_string(), consigne.to_string_lossy().into_owned()]);
        }
        let declaration = Declaration {
            profil: claude.nom.clone(),
            empreinte: claude.empreinte.clone(),
            modele: modele.clone(),
            mur: claude.exige,
            reglages,
            mcp: claude.mcp.as_deref().and_then(serveurs),
            fournisseur: claude.fournisseur.as_ref().map(|fournisseur| fournisseur.hote.clone()),
        };
        Demande { argv, declaration: Some(declaration) }
    }

    /// [`Profil::demande_avec`], sans leçon.
    pub(crate) fn demande(&self, bac: &Path, verifier: Option<&str>, relance: bool) -> Demande {
        self.demande_avec(bac, verifier, relance, None)
    }

    /// Vrai pour une main nue (`Profil::faux`), qui ne reçoit pas de leçon.
    pub(crate) fn nue(&self) -> bool {
        matches!(self.sorte, Sorte::Brute(_))
    }
}

impl Claude {
    /// Où le profil s'écrit, pour un refus : son fichier, ou le profil intégré.
    fn lieu(&self) -> String {
        self.fichier.as_ref().map_or_else(|| format!("le profil intégré « {} »", self.nom), |f| f.display().to_string())
    }

    /// Les règles de fichiers et de réseau d'une main posée dans `bac` : ce que ses commandes ne lisent pas (le dossier
    /// personnel, le dossier de Kraken), ce qu'elles lisent (le bac, le git commun, puis `lecture`) et les domaines
    /// qu'elles joignent (`reseau`). Le `--settings` de la main et les réglages de srt du rejeu les prennent ici.
    fn regles(&self, bornes: &Bornes, bac: &Path) -> (Vec<String>, Vec<String>, Vec<String>) {
        let fermes = vec!["~/".to_string(), bornes.dossier.clone()];
        let lus = [bac.to_string_lossy().into_owned(), bornes.git_commun.clone()].into_iter();
        (fermes, lus.chain(self.lecture.clone()).collect(), self.reseau.clone())
    }
}

/// Les règles `Edit` des chemins protégés dans `bac` : `Edit(<bac>/<fichier>)`, ou `Edit(<bac>/<dossier>/**)` pour un
/// dossier (fini par `/`), chacune par [`regle_de_chemin`]. `Err` si le bac n'a pas de règle.
fn protegees(proteges: &[String], bac: &Path) -> Result<Vec<String>, ()> {
    if proteges.is_empty() {
        return Ok(Vec::new());
    }
    let regle = bac.to_str().ok_or(()).and_then(|bac| regle_de_chemin(bac, cfg!(windows)).map_err(|_| ()))?;
    Ok(proteges
        .iter()
        .map(|chemin| match chemin.strip_suffix('/') {
            Some(dossier) => format!("Edit({regle}/{dossier}/**)"),
            None => format!("Edit({regle}/{chemin})"),
        })
        .collect())
}

/// Ce qui rejoue une preuve sous un mur exigé (voir [`Profil::rejeu`]).
#[derive(Debug)]
pub(crate) struct Srt {
    /// Le programme : `LYDITE_SRT`, que `preparer` lit, sinon `srt`.
    pub(crate) programme: OsString,
    /// Le fichier de réglages, neuf, dans le dossier de Kraken : hors de la portée des mains.
    pub(crate) fichier: PathBuf,
    pub(crate) reglages: String,
    /// Le dossier temporaire du rejeu (`bac::temporaire_de`), ouvert en écriture.
    pub(crate) temporaire: PathBuf,
}

/// Le profil `nom` lu de son texte, au format de l'en-tête, sans rien lire d'autre : `chemin` est son fichier, `None`
/// pour le profil intégré, et `kraken_claude` la valeur de `LYDITE_CLAUDE`, programme d'un profil qui n'en nomme
/// pas. Une ligne fausse est le refus 6009, qui la nomme par `<chemin>:<n>` avec la règle de son cas ; un profil qui
/// ne dit pas sa sorte, le refus 6010. Un fournisseur mal donné (voir `api` et `cle`) est le refus 6027 : une URL qui
/// n'est pas en https, un nom hors de `[A-Z0-9_]`, une des deux clés seule. Il ne lit pas l'environnement : la clé se
/// lit au lancement, par [`Profil::fournir`].
pub(crate) fn lire(
    nom: &str,
    texte: &str,
    chemin: Option<&Path>,
    kraken_claude: Option<&str>,
) -> Result<Profil, Refus> {
    let mut claude = Claude {
        nom: nom.into(),
        fichier: chemin.map(Path::to_path_buf),
        empreinte: empreinte(texte),
        programme: kraken_claude.unwrap_or("claude").into(),
        modele: "sonnet".into(),
        relance: "sonnet".into(),
        decoupeur: "opus".into(),
        relecteur: "opus".into(),
        fournisseur: None,
        prix: Vec::new(),
        exige: true,
        outils: Vec::new(),
        mcp: None,
        reseau: Vec::new(),
        lecture: Vec::new(),
        ligne_lecture: (0, String::new()),
        bornes: None,
        srt: "srt".into(),
    };
    let lieu = claude.lieu();
    let refusee = |n: usize, ligne: &str, regle: &str| ligne_refusee(&lieu, nom, n, ligne, regle);
    let (mut delai_main, mut delai_preuve, mut sorte) = (DELAI_MAIN, DELAI_PREUVE, false);
    let mut plafond = PLAFOND_MAIN;
    let mut vues: Vec<&str> = Vec::new();
    let mut ligne_outils = (0, "");
    // `api` (sa ligne, sa valeur, son hôte) et `cle` (sa ligne, sa valeur), réunies après la lecture.
    let (mut api, mut variable) = (None, None);
    let mut ligne_prix = 0;
    let mut proteges = vec![LECONS.to_string()];
    let mut mesure = Mesure::default();
    for (i, ligne) in texte.strip_prefix('\u{feff}').unwrap_or(texte).split('\n').enumerate() {
        let (n, ligne) = (i + 1, ligne.strip_suffix('\r').unwrap_or(ligne));
        let refus = |regle: &str| Err(refusee(n, ligne, regle));
        if ligne.trim().is_empty() || ligne.trim_start().starts_with('#') {
            continue;
        }
        let Some((cle, valeur)) = ligne.split_once('=') else { return refus("une ligne vaut « clé = valeur »") };
        let (cle, valeur) = (cle.trim(), valeur.trim());
        if cle == "budget" {
            return refus("aucun budget : il rendrait la main sous-productive");
        }
        let Some(cle) = CLES.into_iter().find(|connue| *connue == cle) else {
            return refus(&format!("la clé est une de celles-ci : {}", CLES.join(", ")));
        };
        if vues.contains(&cle) {
            return refus("chaque clé s'écrit une fois");
        }
        vues.push(cle);
        let elements: Vec<&str> =
            if LISTES.contains(&cle) { valeur.split(',').map(str::trim).collect() } else { vec![valeur] };
        if elements.iter().any(|element| element.is_empty() || element.starts_with('-')) {
            return refus("une valeur n'est ni vide ni en « - »");
        }
        let liste = || elements.iter().map(|element| element.to_string()).collect::<Vec<_>>();
        let modele =
            |modele: &str| modele.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b".-".contains(&c));
        let domaine = |domaine: &&str| domaine.bytes().all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c));
        match cle {
            "sorte" if valeur == "claude" => sorte = true,
            "sorte" => return refus("la sorte d'une main est claude"),
            "programme" if nom_nu(valeur) || Path::new(valeur).is_absolute() => claude.programme = valeur.into(),
            "programme" => return refus("un programme est un nom nu ou un chemin absolu"),
            "modele" if modele(valeur) => claude.modele = valeur.into(),
            "relance" if modele(valeur) => claude.relance = valeur.into(),
            "decoupeur" if modele(valeur) => claude.decoupeur = valeur.into(),
            "relecteur" if modele(valeur) => claude.relecteur = valeur.into(),
            "modele" | "relance" | "decoupeur" | "relecteur" => {
                return refus(
                    "un modèle s'écrit en [a-z0-9.-] : un alias (sonnet, haiku, opus) ou un nom complet \
                     (claude-sonnet-5-5)",
                );
            }
            "api" => match hote_de(valeur) {
                Some(hote) => api = Some((n, valeur, hote)),
                None => {
                    let regle = "l'URL d'une API s'écrit https://<hôte>[:<port>][/<chemin>], l'hôte en [A-Za-z0-9.-] \
                                 et le chemin en [A-Za-z0-9._~/-] : ni http, ni identifiants, ni requête";
                    return Err(fournisseur_refuse(&lieu, nom, n, "api", regle));
                }
            },
            "cle" if nom_de_variable(valeur) => variable = Some((n, valeur)),
            "cle" => {
                let regle = "cle est le nom d'une variable d'environnement, en [A-Z0-9_], jamais la clé elle-même";
                return Err(fournisseur_refuse(&lieu, nom, n, "cle", regle));
            }
            "prix" => {
                let mut prix: Vec<(String, Prix)> = Vec::new();
                for element in &elements {
                    let Some((nom_prix, un_prix)) = prix_de(element).filter(|(m, _)| modele(m)) else {
                        return refus(
                            "un prix s'écrit <modèle> <entrée> <sortie> [<cache lu> [<cache écrit>]] : le modèle en \
                             [a-z0-9.-], puis de deux à quatre décimaux positifs en dollars par million de jetons, \
                             six décimales au plus",
                        );
                    };
                    if prix.iter().any(|(deja, _)| deja == nom_prix) {
                        return refus("un modèle n'a qu'un prix");
                    }
                    prix.push((nom_prix.into(), un_prix));
                }
                (claude.prix, ligne_prix) = (prix, n);
            }
            "outils" if elements.iter().all(|outil| SORTANTS.contains(outil) || serveur_mcp(outil)) => {
                (claude.outils, ligne_outils) = (liste(), (n, ligne));
            }
            "outils" => {
                return refus(&format!(
                    "un outil déclaré est mcp__<serveur> ou l'un de ceux-ci : {}",
                    SORTANTS.join(", ")
                ));
            }
            "mcp" if serveurs(valeur).is_some() => claude.mcp = Some(valeur.into()),
            "mcp" => {
                return refus(
                    "une configuration MCP est un objet JSON sur la ligne, sans clé redite, qui porte l'objet \
                     mcpServers",
                );
            }
            "mur" if valeur == "exige" || valeur == "demande" => claude.exige = valeur == "exige",
            "mur" => return refus("le mur est exige ou demande"),
            "reseau" if elements.iter().all(domaine) => claude.reseau = liste(),
            "reseau" => return refus("un domaine s'écrit en [A-Za-z0-9.-], sans joker ni port"),
            "lecture" if elements.iter().all(|chemin| lecture_permise(chemin)) => {
                (claude.lecture, claude.ligne_lecture) = (liste(), (n, ligne.to_string()));
            }
            "lecture" => return refus("une lecture est un chemin absolu ou en ~/…, sans .. ni joker (* ? [ ] { })"),
            "mesure" => match Mesure::lire(&elements) {
                Some(lue) => mesure = lue,
                None => {
                    return refus(
                        "mesure liste un ou deux réglages, chacun une fois : sans-lecture, sans-a1 (pour un banc de \
                         mesure seulement)",
                    );
                }
            },
            "proteges" => {
                for chemin in &elements {
                    let faute = hors_forme(chemin).map(String::from).or_else(|| {
                        chemin.chars().find(|c| "()*?[]{}\"\\".contains(*c)).map(|c| format!("porte « {c} »"))
                    });
                    if let Some(faute) = faute {
                        return refus(&format!(
                            "un chemin protégé est relatif à la racine du dépôt, en noms séparés par /, un / final \
                             pour un dossier, sans ( ) * ? [ ] {{ }} ni guillemet, hors de .git/ et de .claude/ : \
                             {chemin} {faute}"
                        ));
                    }
                }
                proteges = std::iter::once(LECONS.to_string()).chain(liste()).collect();
            }
            "plafond" => match centimes(valeur) {
                Some(centimes) => plafond = centimes,
                None => return refus("un plafond est un nombre positif de dollars, deux décimales au plus"),
            },
            _ => {
                // `delai_main` ou `delai_preuve` : des chiffres seuls, sans signe, et plus que zéro.
                let secondes = valeur.bytes().all(|c| c.is_ascii_digit()).then(|| valeur.parse::<u64>().ok());
                let Some(secondes) = secondes.flatten().filter(|secondes| *secondes > 0) else {
                    return refus("un délai est un entier positif de secondes");
                };
                let delai = if cle == "delai_main" { &mut delai_main } else { &mut delai_preuve };
                *delai = Duration::from_secs(secondes);
            }
        }
    }
    if claude.mcp.is_none() && claude.outils.iter().any(|outil| outil.starts_with("mcp__")) {
        let (n, ligne) = ligne_outils;
        return Err(refusee(n, ligne, "un serveur mcp__<serveur> ne se déclare qu'avec sa configuration, mcp"));
    }
    // Un fournisseur se donne par `api` et `cle` ensemble : l'une seule est le refus 6027, sur sa ligne.
    let seule = |n: usize, cle: &str, manque: &str| {
        Refus::new(
            Code::Fournisseur,
            format!("{lieu}:{n} : « {cle} » est seule : un fournisseur se donne par api et cle ensemble"),
            format!(
                "Ajoute la ligne « {manque} » à {lieu}, ou retire la ligne {n}, puis accepte le profil : lydite \
                 accepter {nom}."
            ),
        )
    };
    claude.fournisseur = match (api, variable) {
        (None, None) => None,
        (Some((_, api, hote)), Some((_, cle))) => {
            Some(Fournisseur { api: api.into(), hote: hote.into(), cle: cle.into(), valeur: None })
        }
        (Some((n, ..)), None) => return Err(seule(n, "api", "cle = <NOM_DE_LA_VARIABLE>")),
        (None, Some((n, _))) => return Err(seule(n, "cle", "api = https://<hôte>/<chemin>")),
    };
    if !claude.prix.is_empty() && claude.fournisseur.is_none() {
        return Err(Refus::new(
            Code::Fournisseur,
            format!("{lieu}:{ligne_prix} : « prix » est seul : un prix se donne avec api"),
            format!(
                "Ajoute les lignes « api = https://<hôte>/<chemin> » et « cle = <NOM_DE_LA_VARIABLE> » à {lieu}, ou \
                 retire la ligne {ligne_prix}, puis accepte le profil : lydite accepter {nom}."
            ),
        ));
    }
    if !sorte {
        return Err(Refus::new(
            Code::ProfilIncomplet,
            format!("{lieu} ne dit pas sa sorte"),
            format!("Ajoute la ligne « sorte = claude » à {lieu}, puis accepte le profil : lydite accepter {nom}."),
        ));
    }
    Ok(Profil { sorte: Sorte::Claude(Box::new(claude)), delai_main, delai_preuve, plafond, proteges, mesure })
}

/// Le chemin absolu en forme POSIX d'une règle de permission de Claude Code : `//` puis le chemin, `C:\x` donnant
/// `//c/x` sous Windows et `/x` donnant `//x` ailleurs. Pure : `windows` dit comment lire le chemin, pour que les tests
/// jouent les deux systèmes. Un chemin qu'une règle ne sait pas écrire, où elle tomberait seule et en silence, rend sa
/// raison, dont `borner` fait le refus 6023 : sous Windows, sans lettre de lecteur (UNC, `\\?\`) ; partout, relatif, ou
/// avec un nom qui porte une parenthèse, `*`, `?`, `[`, `]`, `{`, `}`, un guillemet ou une barre inverse.
pub(crate) fn regle_de_chemin(chemin: &str, windows: bool) -> Result<String, String> {
    let (mut regle, reste, separateurs): (String, &str, &[char]) = match (windows, chemin.as_bytes()) {
        (true, [lecteur, b':', b'\\' | b'/', ..]) if lecteur.is_ascii_alphabetic() => {
            (format!("//{}", char::from(lecteur.to_ascii_lowercase())), &chemin[3..], &['\\', '/'])
        }
        (false, [b'/', ..]) => ("/".into(), &chemin[1..], &['/']),
        (true, _) => return Err("sous Windows, il ne commence pas par une lettre de lecteur".into()),
        (false, _) => return Err("il n'est pas absolu".into()),
    };
    for nom in reste.split(separateurs).filter(|nom| !nom.is_empty()) {
        if let Some(c) = nom.chars().find(|c| "()*?[]{}\"\\".contains(*c)) {
            return Err(format!("le nom « {nom} » porte « {c} »"));
        }
        regle.push('/');
        regle.push_str(nom);
    }
    Ok(regle)
}

/// La règle de permission de ce chemin, que `quoi` nomme, sur le système qui joue : un chemin qui n'est pas de l'UTF-8,
/// ou qu'une règle ne sait pas écrire (voir [`regle_de_chemin`]), est le refus 6023, qui le nomme et dit `correction`.
fn en_regle(chemin: &Path, quoi: &str, correction: &str) -> Result<String, Refus> {
    let refus = |preuve: String| Refus::new(Code::DossierCheminRefuse, preuve, correction);
    let Some(texte) = chemin.to_str() else {
        return Err(refus(format!("{quoi} {} n'est pas de l'UTF-8", chemin.display())));
    };
    regle_de_chemin(texte, cfg!(windows)).map_err(|raison| refus(format!("{quoi} {texte} : {raison}")))
}

/// L'empreinte d'un texte : le SHA-256 de ses octets, BOM compris, en 64 hexadécimaux minuscules.
pub(crate) fn empreinte(texte: &str) -> String {
    hex(&sha256(texte.as_bytes()))
}

pub(crate) fn integre(nom: &str) -> Option<&'static str> {
    (nom == "claude").then_some(INTEGRE)
}

/// Le refus 6009 d'une ligne du profil : `<chemin>:<n>`, la ligne, et la règle de son cas.
fn ligne_refusee(lieu: &str, nom: &str, n: usize, ligne: &str, regle: &str) -> Refus {
    Refus::new(
        Code::ProfilLigneRefusee,
        format!("{lieu}:{n} : « {ligne} » : {regle}"),
        format!("Corrige la ligne {n} de {lieu}, puis accepte le profil : lydite accepter {nom}."),
    )
}

/// Un programme sans dossier, que le système cherche.
fn nom_nu(programme: &str) -> bool {
    let mut composants = Path::new(programme).components();
    matches!((composants.next(), composants.next()), (Some(Component::Normal(_)), None))
}

/// Les noms des serveurs d'une configuration MCP : les clés de son objet `mcpServers`, triées. `None` si ce n'est pas
/// un objet JSON qui porte l'objet `mcpServers`, ou si une clé s'y redit à quelque niveau que ce soit : `lire` refuse
/// alors la ligne `mcp`, et la déclaration d'une main a toujours ses noms.
fn serveurs(mcp: &str) -> Option<Vec<String>> {
    let Ok(Json::Objet(racine)) = json::document(mcp) else { return None };
    let Some(Json::Objet(serveurs)) = racine.get("mcpServers") else { return None };
    Some(serveurs.keys().cloned().collect())
}

/// Un `mcp__<serveur>`, le serveur en `[A-Za-z0-9_-]`.
fn serveur_mcp(outil: &str) -> bool {
    let serveur = outil.strip_prefix("mcp__");
    serveur.is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)))
}

/// L'hôte d'une URL d'API, port compris, si elle s'écrit `https://<hôte>[:<port>][/<chemin>]` : l'hôte en
/// `[A-Za-z0-9.-]`, le port en un à cinq chiffres, le chemin en `[A-Za-z0-9._~/-]`. Ni `http`, ni identifiants, ni
/// requête, ni fragment : aucune clé ne se glisse dans une URL, et l'hôte, que le fil note, est de l'ASCII sans
/// blanc.
fn hote_de(api: &str) -> Option<&str> {
    let reste = api.strip_prefix("https://")?;
    let (hote, chemin) = reste.split_once('/').unwrap_or((reste, ""));
    let (nom, port) = hote.split_once(':').map_or((hote, None), |(nom, port)| (nom, Some(port)));
    let nom_permis = !nom.is_empty() && nom.bytes().all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c));
    let port_permis = port.is_none_or(|port| (1..=5).contains(&port.len()) && port.bytes().all(|c| c.is_ascii_digit()));
    let chemin_permis = chemin.bytes().all(|c| c.is_ascii_alphanumeric() || b"._~/-".contains(&c));
    (nom_permis && port_permis && chemin_permis).then_some(hote)
}

/// Le prix d'un modèle chez un fournisseur, en micro-dollars par million de jetons. La réflexion n'a pas de prix à
/// elle : la sortie la contient déjà.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prix {
    pub(crate) entree: u64,
    pub(crate) sortie: u64,
    pub(crate) cache_lu: u64,
    pub(crate) cache_ecrit: u64,
}

/// Un élément de `prix` : le modèle, puis ses [`Prix`]. Un cache sans prix se compte au prix d'entrée.
fn prix_de(element: &str) -> Option<(&str, Prix)> {
    let mut mots = element.split_whitespace();
    let modele = mots.next()?;
    let entree = micro_dollars(mots.next()?)?;
    let sortie = micro_dollars(mots.next()?)?;
    let cache_lu = mots.next().map(micro_dollars).unwrap_or(Some(entree))?;
    let cache_ecrit = mots.next().map(micro_dollars).unwrap_or(Some(entree))?;
    mots.next().is_none().then_some((modele, Prix { entree, sortie, cache_lu, cache_ecrit }))
}

/// Un plafond : un décimal strictement positif, sans signe ni exposant, deux décimales au plus, en centimes.
fn centimes(nombre: &str) -> Option<u64> {
    let (entier, fraction) = nombre.split_once('.').unwrap_or((nombre, ""));
    let chiffres = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    let permis = !entier.is_empty() && chiffres(entier) && chiffres(fraction) && fraction.len() <= 2;
    if !permis || (nombre.contains('.') && fraction.is_empty()) {
        return None;
    }
    let fraction: u64 = format!("{fraction:0<2}").parse().ok()?;
    let total = entier.parse::<u64>().ok()?.checked_mul(100)?.checked_add(fraction)?;
    (total > 0).then_some(total)
}

/// Un décimal positif ou nul, sans signe ni exposant, six décimales au plus, en millionièmes.
fn micro_dollars(nombre: &str) -> Option<u64> {
    let (entier, fraction) = nombre.split_once('.').unwrap_or((nombre, ""));
    let chiffres = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    let permis = !entier.is_empty() && chiffres(entier) && chiffres(fraction) && fraction.len() <= 6;
    if !permis || (nombre.contains('.') && fraction.is_empty()) {
        return None;
    }
    let entier: u64 = entier.parse().ok()?;
    let fraction: u64 = format!("{fraction:0<6}").parse().ok()?;
    entier.checked_mul(1_000_000)?.checked_add(fraction)
}

/// Le nom d'une variable d'environnement, en `[A-Z0-9_]`, non vide.
fn nom_de_variable(nom: &str) -> bool {
    !nom.is_empty() && nom.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
}

/// Le refus 6027 d'une ligne du fournisseur : `<chemin>:<n> : « <clé> » : <règle>`. Il ne reprend pas la valeur de
/// la ligne : une clé collée dans `cle`, ou des identifiants dans `api`, ne s'écrivent pas sur stderr.
fn fournisseur_refuse(lieu: &str, nom: &str, n: usize, cle: &str, regle: &str) -> Refus {
    Refus::new(
        Code::Fournisseur,
        format!("{lieu}:{n} : « {cle} » : {regle}"),
        format!("Corrige la ligne {n} de {lieu}, puis accepte le profil : lydite accepter {nom}."),
    )
}

/// Une entrée de `lecture` : un chemin absolu ou en `~/…`, sans `..` ni joker.
fn lecture_permise(chemin: &str) -> bool {
    (chemin.starts_with("~/") || Path::new(chemin).is_absolute())
        && !Path::new(chemin).components().any(|composant| composant == Component::ParentDir)
        && !chemin.contains(['*', '?', '[', ']', '{', '}'])
}

/// Les morceaux de la preuve que Kraken prête à la main : la commande coupée sur ` && `. Chaque morceau donne
/// `Bash(<morceau>)` et `PowerShell(<morceau>)` (`Bash(...)` vaut aussi pour Monitor), et sort du bac à sable de Claude
/// Code sous un mur demandé (voir `demande`). Un morceau blanc, ou qui sort de `[A-Za-z0-9 ._/=-]`, prêterait plus que
/// lui-même (un joker, un `:*`, une parenthèse qui ferme la règle, un enchaînement, une redirection, une expansion) :
/// alors rien n'est prêté, `None`. Sous le mur, les commandes passent sans invite ; le prêt sert la main sans mur.
fn pret(verifier: &str) -> Option<Vec<&str>> {
    let morceaux: Vec<&str> = verifier.split(" && ").collect();
    let permis = |c: u8| c.is_ascii_alphanumeric() || b" ._/=-".contains(&c);
    if morceaux.iter().any(|morceau| morceau.trim().is_empty() || !morceau.bytes().all(permis)) {
        return None;
    }
    Some(morceaux)
}

/// Un tableau JSON compact de chaînes, chacune écrite par `json::chaine`.
fn tableau(chaines: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", chaines.into_iter().map(|chaine| json::chaine(&chaine)).collect::<Vec<_>>().join(","))
}

#[cfg(test)]
mod tests;
