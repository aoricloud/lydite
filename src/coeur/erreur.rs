//! Les codes de Kraken et le refus qui les porte, sur le modèle des erreurs du contrat v2 de passerelle. Un code est
//! un numéro figé, un nom `domaine.quoi` et la règle qu'il défend, en une phrase ; un refus dit cette règle, ce qui a
//! été vu et ce qu'il faut faire.
//!
//! Les codes sont rangés par blocs de mille, un domaine chacun, et chaque nom porte le préfixe de son bloc :
//!
//! - 1000 `contrat` : emprunté au contrat v2 de passerelle avec ses numéros, sans PR chez lui, et jamais étendu ici ;
//! - 6000 `lancer` : tout ce qui précède l'ouverture de la vague, c'est-à-dire la ligne de commande, ce que
//!   `preparer` lit (profil et fournisseur, dépôt, dossier), `lydite etat` compris, et le plan, écrit à la main ou
//!   découpé, avec sa base éprouvée ;
//! - 7000 `vague` : les mains et l'assemblage, puis les fichiers de chaque main, le découpeur, le relecteur et la
//!   correction qui suit sa relecture ;
//! - 8000 `forge` : la PR et la CI, avant comme après la vague.
//!
//! Un code publié n'est jamais renuméroté ni réattribué : on en ajoute au bout de son bloc, on n'en retire pas. Son
//! numéro et son nom sont figés, sa règle peut se reformuler, et `tests::table_figee` tient la table à la lettre. Les
//! numéros s'écrivent dans les plans, jamais par une main.
//!
//! Deux lignes repères ferment la liste, dans `codes!` comme dans `TABLE` : `// --- 7000 : vague ---` et
//! `// --- 8000 : forge ---`. Un code neuf s'ajoute au bout de son bloc : les 6xxx avant la première ligne repère,
//! les 7xxx entre les deux, les 8xxx après la seconde. Deux blocs qui s'étendent chacun du leur ne se heurtent
//! donc pas.
//!
//! L'énumération ne tient que les codes que le programme construit : 1001, 1002, 1005, 1006, 1009 et 1010 du bloc
//! 1000, 6001 à 6032 du bloc 6000, 7001 à 7018 du bloc 7000, 8001 à 8013 du bloc 8000. Les autres lignes du bloc 1000
//! de passerelle (1003, 1004, 1007, 1008 et 1011) n'y sont pas ; `tests::copie_du_bloc_1000` en garde la copie entière,
//! sous `#[cfg(test)]` comme `Code::TOUS` : chaque code de 1000 à 5999 et de 9000 à 9999 en est une ligne, et ceux de
//! ce dépôt vivent dans 6000 à 8999.
//!
//! Ce qui diffère de passerelle, un écart par ligne avec sa raison :
//!
//! - un `v` par ligne au lieu d'une poignée : un fichier n'a pas de dialogue (d'où l'absence de 1003 et de 1004) ;
//! - `seance` au lieu de `demande` : le fil ne répond à personne, il n'a ni demande ni flux (d'où l'absence de 1007 et
//!   de 1011) ;
//! - l'instant à la milliseconde, pour mesurer les durées ;
//! - les types inconnus ignorés au lieu de refusés en 1008 : d'autres pièces y ajoutent les leurs ;
//! - des valeurs plates seulement, quand passerelle imbrique (`flux.entree`, `flux.erreur`, `bascule.publiee`) :
//!   l'enveloppe se lit toujours, et sans bibliothèque ;
//! - `Refus` à plat (code, preuve, correction) au lieu d'`Erreur {code, message}` : un refus enseigne, et le fil ne
//!   tient que des valeurs plates ;
//! - `codes!` à quatre éléments, la règle en plus : le refus la tient du code au lieu de la recopier ;
//! - l'origine `humain` au lieu de `patron`, car la doctrine nomme les gens par leur place ;
//! - `origine_foi` sur chaque ligne : passerelle tient l'origine pour déclarée à la poignée, sans le dire sur ses
//!   événements ;
//! - seules les origines que Kraken écrit se lisent : les autres entrent avec leur premier écrivain, sans changer `v` ;
//! - 6000, 7000 et 8000 sont les blocs de Kraken, et 2000 à 5000 comme 9000 restent à passerelle ;
//! - ni serde ni `Domaine` : le dépôt n'a aucune dépendance, un code s'écrit par son numéro, et la table des blocs ne
//!   sert qu'aux tests.

use std::fmt;
use std::io;

/// Déclare les codes : variante, numéro, nom, règle. Une seule liste, d'où sortent l'énumération, `numero()`, `nom()`,
/// `regle()` et, sous `#[cfg(test)]`, `Code::TOUS`. La règle sert aussi de documentation à la variante.
macro_rules! codes {
    ($($variante:ident = $numero:literal, $nom:literal, $regle:literal;)+) => {
        /// Un code de Kraken. Au fil : son numéro.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Code {
            $(#[doc = $regle] $variante,)+
        }

        impl Code {
            /// Le numéro, figé.
            pub(crate) const fn numero(self) -> u16 {
                match self {
                    $(Code::$variante => $numero,)+
                }
            }

            /// Le nom stable, `domaine.quoi`.
            pub(crate) const fn nom(self) -> &'static str {
                match self {
                    $(Code::$variante => $nom,)+
                }
            }

            /// La règle que le code défend, en une phrase.
            pub(crate) const fn regle(self) -> &'static str {
                match self {
                    $(Code::$variante => $regle,)+
                }
            }

            /// Le code de ce numéro, `None` pour un numéro que Kraken ne construit pas : un autre écrivain du fil peut
            /// en noter un.
            pub(crate) const fn de_numero(numero: u16) -> Option<Code> {
                match numero {
                    $($numero => Some(Code::$variante),)+
                    _ => None,
                }
            }
        }

        #[cfg(test)]
        impl Code {
            /// Tous les codes, dans l'ordre de déclaration (celui des numéros).
            pub(crate) const TOUS: &[Code] = &[$(Code::$variante,)+];
        }
    };
}

codes! {
    // --- 1000 : contrat ---
    JsonIllisible = 1001, "contrat.json_illisible",
        "Une ligne du fil est un objet JSON en UTF-8 (RFC 8259).";
    VersionDifferente = 1002, "contrat.version_differente",
        "Un lecteur lit `v` avant tout le reste, et ne lit que la version du fil qu'il connaît.";
    CommandeInconnue = 1005, "contrat.commande_inconnue",
        "La commande est un mot de la liste fermée de Lydite, que donne l'usage.";
    CommandeMalFormee = 1006, "contrat.commande_mal_formee",
        "La ligne de commande nomme une commande et lui donne les arguments de son usage, chaque drapeau une \
         fois, avec sa valeur s'il en prend une.";
    EvenementMalForme = 1009, "contrat.evenement_mal_forme",
        "Une ligne v2 porte son enveloppe entière et des valeurs plates ; un type connu porte tous ses \
         champs, chacun dans son domaine, et aucun autre.";
    SequenceRompue = 1010, "contrat.sequence_rompue",
        "Dans une séance, `seq` va 1, 2, 3… dans l'ordre du fil, sans trou ni redite.";
    // --- 6000 : lancer ---
    ProfilInconnu = 6001, "lancer.profil_inconnu",
        "Un profil se nomme parmi les profils que Lydite connaît.";
    DepotIntrouvable = 6002, "lancer.depot_introuvable",
        "Le dépôt visé est un dossier qui existe.";
    BacImpossible = 6003, "lancer.bac_impossible",
        "Le dépôt visé est un dépôt git qui a un commit, d'où partent les bacs ; sans lui, aucune vague ne s'ouvre.";
    DossierRelatif = 6004, "lancer.dossier_relatif",
        "`LYDITE_DOSSIER`, s'il est posé, est un chemin absolu.";
    DossierIntrouvable = 6005, "lancer.dossier_introuvable",
        "Sans `LYDITE_DOSSIER`, le dossier de Lydite se tire de `USERPROFILE` sous Windows, de \
         `XDG_STATE_HOME` ou de `HOME` ailleurs, chacun absolu.";
    EntreeSortie = 6006, "lancer.entree_sortie",
        "Une lecture ou une écriture de Lydite réussit, ou son refus nomme son chemin ou son programme.";
    ProfilNomRefuse = 6007, "lancer.profil_nom_refuse",
        "Un nom de profil est fait de lettres ASCII, de chiffres, de `_` et de `-`.";
    ProfilIllisible = 6008, "lancer.profil_illisible",
        "Un fichier de profil se lit, en UTF-8 (BOM et retour chariot tolérés) ; sinon son refus nomme son chemin.";
    ProfilLigneRefusee = 6009, "lancer.profil_ligne_refusee",
        "Chaque ligne du profil est `clé = valeur` d'une clé connue, sa valeur dans son domaine, chaque clé \
         une fois ; aucun budget.";
    ProfilIncomplet = 6010, "lancer.profil_incomplet",
        "Un profil dit sa sorte.";
    ProfilNonAccepte = 6011, "lancer.profil_non_accepte",
        "Un profil en fichier n'est lancé qu'une fois son empreinte acceptée.";
    ProfilChange = 6012, "lancer.profil_change",
        "Le profil lancé a l'empreinte acceptée ; une empreinte neuve se réaccepte par `lydite accepter`.";
    ProfilSansFichier = 6013, "lancer.profil_sans_fichier",
        "Seul un profil en fichier s'accepte ; le profil intégré n'a pas d'empreinte à garder.";
    PreuveVide = 6014, "lancer.preuve_vide",
        "`--verifier`, s'il est donné, porte une commande qui n'est pas blanche.";
    DossierSousLeDepot = 6015, "lancer.dossier_sous_le_depot",
        "Le dossier de Lydite est hors du dépôt visé.";
    PlanVersion = 6016, "lancer.plan_version",
        "Un plan commence par sa version, `plan 1`, que Lydite lit avant tout le reste.";
    PlanEncodage = 6017, "lancer.plan_encodage",
        "Un plan est un texte UTF-8, un BOM de tête toléré ; un plan en UTF-16 se réécrit en UTF-8.";
    PlanLigne = 6018, "lancer.plan_ligne",
        "En colonne 0, une ligne de plan est vide, `tache <id>` dans la limite de 10 000 tâches, ou une clé \
         connue (`depend`, `preuve`, `modele`, `relance`, `fichiers`, `critique` sous une tâche), une seule \
         fois et à sa place, avant les lignes indentées ; une ligne indentée suit une tâche.";
    PlanIdentifiant = 6019, "lancer.plan_identifiant",
        "L'id d'une tâche est un mot de lettres et de chiffres ASCII, de `-` et de `_`, unique dans le plan.";
    PlanDependance = 6020, "lancer.plan_dependance",
        "Une tâche ne dépend que d'autres tâches du plan, sans cycle.";
    PlanBlanc = 6021, "lancer.plan_blanc",
        "Un plan a un objectif, une preuve et au moins une tâche ; chaque tâche a un objectif, et une \
         `preuve`, un `modele`, une `relance` ou une `critique` écrits ne sont pas blancs (seul `depend` \
         peut l'être).";
    BaseRouge = 6022, "lancer.base_rouge",
        "La preuve de la vague passe sur la base avant toute main, pour qu'un vert d'assemblage ne se doive \
         qu'aux tâches.";
    DossierCheminRefuse = 6023, "lancer.dossier_chemin_refuse",
        "Un chemin que Lydite écrit dans une règle de permission, son dossier ou un dossier de connexion de \
         Claude, est absolu et s'écrit tel quel : sous Windows il commence par une lettre de lecteur, et ses \
         noms ne portent ni parenthèse, ni `*`, `?`, `[`, `]`, `{`, `}`, ni guillemet, ni barre inverse.";
    PlanChemin = 6024, "lancer.plan_chemin",
        "Un chemin de `fichiers` est relatif à la racine du dépôt, écrit avec `/`, sans `..`, hors de `.git/` \
         et de `.claude/` ; dans un plan découpé, il porte la casse que git lui connaît dans la base.";
    PlanBorne = 6025, "lancer.plan_borne",
        "Un plan pèse 1 Mo au plus ; un plan découpé a 12 tâches au plus, et chacune a `fichiers` et \
         `modele`, sans `preuve` à elle (toutes jouent celle de la vague) ; une ligne `fichiers` a 100 \
         chemins au plus.";
    PlanEntete = 6026, "lancer.plan_entete",
        "Un plan découpé porte l'en-tête que Lydite a demandé : l'objectif et la preuve de l'humain, tels quels.";
    Fournisseur = 6027, "lancer.fournisseur",
        "Un profil qui nomme un fournisseur donne `api`, une URL en https, et `cle`, le nom d'une variable \
         d'environnement posée et non vide, jamais la clé elle-même.";
    MurRefuse = 6028, "lancer.mur_refuse",
        "Sous un mur exigé, srt joue `true` sous des réglages de rejeu avant toute main et avant l'épreuve de \
         la base ; sinon rien n'est lancé.";
    IssueSansMur = 6029, "lancer.issue_sans_mur",
        "`--issue` ne se lance que sous un profil au mur exigé, car le texte d'une issue vient d'autrui : \
         `mur = exige` dans le profil puis `lydite accepter <profil>`, ou un objectif tapé à la main.";
    Quota = 6030, "lancer.quota",
        "Sous un quota du jour posé (`profils/quota`, en dollars), une main ne part pas quand la dépense estimée \
         du jour UTC l'atteint, quand le fichier ne se lit pas, ni quand son profil à `api` n'a pas de `prix` \
         pour son modèle ; dépenser au-delà revient à l'humain.";
    LeconMalFormee = 6031, "lancer.lecon_mal_formee",
        "Une leçon est un `.agents/skills/<nom>/SKILL.md` de 4096 octets au plus, en UTF-8 : une en-tête `name` (le \
         dossier) et `description` (une ligne), puis `## What happened`, `## Why`, `## How it is checked`, \
         `## Pitfall`, `## Files` (des lignes `- <chemin>`) et `## Falls when`, chacune une fois, dans l'ordre, avec \
         du texte.";
    DossierPris = 6032, "lancer.dossier_pris",
        "Un lancement tient le dossier de Lydite pour lui seul jusqu'à la fin de son processus : un second \
         lancement sur le même dossier est refusé, car ils écriraient le même fil et liraient le même quota.";
    // --- 7000 : vague ---
    VagueBacImpossible = 7001, "vague.bac_impossible",
        "Chaque tâche a son bac, que git pose juste avant sa main ; une tâche sans bac est refusée, et les \
         autres continuent.";
    MainActive = 7002, "vague.main_active",
        "Une tâche n'a jamais deux mains actives : une main ne part que sur une tâche sans bail ouvert ni expiré.";
    ResultatPerime = 7003, "vague.resultat_perime",
        "Un résultat n'est pris que s'il porte la tentative du bail ouvert de sa tâche ; tout autre est périmé.";
    MainExpiree = 7004, "vague.main_expiree",
        "Une main qui n'a rien rendu à l'échéance de son bail expire, et sa tâche est refusée sans relance.";
    MainTuee = 7005, "vague.main_tuee",
        "Une main que Lydite a tuée à son délai n'est pas relancée, et sa tâche est refusée : sa mise à mort \
         ne prouve pas que tout son arbre est mort.";
    ClaudeTouche = 7006, "vague.claude_touche",
        "Une main ne touche à aucun dossier `.claude/` de son bac, à toute profondeur, sauf l'état que Claude \
         Code y tient lui-même hors de git ; Lydite le vérifie avant toute relance dans ce bac, et au gel, dans \
         ce qu'il commite comme dans l'histoire qu'il apporte. Une butée du mur, fichier ordinaire vide en \
         lecture seule posé par le bac à sable, n'est pas une écriture de la main.";
    Conflit = 7007, "vague.conflit",
        "Une prise s'assemble sans conflit sur la tête de la vague ; en conflit, elle est refusée sans relance.";
    PreuveRouge = 7008, "vague.preuve_rouge",
        "Après chaque fusion, la preuve de la vague passe sur la tête ; rouge, la fusion est défaite et la \
         tâche refusée sans relance.";
    GitImpossible = 7009, "vague.git_impossible",
        "Les gestes git de la vague réussissent ; un autre échec de git refuse la tâche sans faire avancer la \
         branche de la vague.";
    AssemblageFerme = 7010, "vague.assemblage_ferme",
        "Le worktree de la vague revient à sa tête après chaque échec ; s'il ne le peut, l'assemblage se \
         ferme et plus rien ne s'y fusionne.";
    Sautee = 7011, "vague.sautee",
        "Une tâche ne part que si toutes ses dépendances sont assemblées ; sinon, elle est sautée, sans bac.";
    FilInjoignable = 7012, "vague.fil_injoignable",
        "Tout geste de la vague est noté au fil avant d'agir ; une note ratée arrête les départs et les fusions.";
    HorsFichiers = 7013, "vague.hors_fichiers",
        "Une main ne change que les chemins des `fichiers` de sa tâche ; un autre chemin rougit sa tentative.";
    BacTouche = 7014, "vague.bac_touche",
        "Le découpeur et le relecteur n'écrivent que leur fichier, ordinaire et que le dépôt n'ignore pas, à \
         la racine de leur bac ; rien d'autre n'y change.";
    RemarquesIllisibles = 7015, "vague.remarques_illisibles",
        "Le fichier du relecteur suit le format `remarques 2` ; sinon sa tentative est rouge.";
    RelectureManquee = 7016, "vague.relecture_manquee",
        "Le travail d'une tâche critique ne se pousse qu'une fois relu ; une relecture classée, et rien ne se pousse.";
    BloquantesRestantes = 7017, "vague.bloquantes_restantes",
        "Des remarques bloquantes valent une seule correction ; non assemblée, rien ne se pousse.";
    CheminProtege = 7018, "vague.chemin_protege",
        "A hand changes no path that its profile protects (proteges), neither by an edit nor by a command.";
    PlafondAtteint = 7019, "vague.plafond_atteint",
        "Une main qui atteint son plafond de dépense est arrêtée et pas relancée ; sa tâche est refusée, et la \
         suite revient à l'humain.";
    // --- 8000 : forge ---
    DistantIllisible = 8001, "forge.distant_illisible",
        "Avec `--pr`, le dépôt visé a un distant `origin` dont l'URL nomme un hôte, un propriétaire et un dépôt.";
    BaseInconnue = 8002, "forge.base_inconnue",
        "La base que nomme `--pr` est une branche du distant, que Lydite va chercher avant la vague.";
    GhInjoignable = 8003, "forge.gh_injoignable",
        "`gh` se lance et se dit connecté à l'hôte du distant avant la vague.";
    RienAPousser = 8004, "forge.rien_a_pousser",
        "La branche de la vague a au moins un commit de plus que sa base ; sinon rien ne se pousse.";
    PublicationRefusee = 8005, "forge.publication_refusee",
        "Lydite ne publie ni un changement de `.github/`, ni un chemin du poste, ni un commit qu'il n'a pas \
         fait, ni un mot du fichier `garde` de l'utilisateur ; publier ceux-là revient à l'humain.";
    PousseeRefusee = 8006, "forge.poussee_refusee",
        "La vague se pousse sans forcer, dans son délai ; sinon rien ne s'ouvre.";
    PrImpossible = 8007, "forge.pr_impossible",
        "Une fois la branche poussée, `gh` ouvre la PR contre la base et la relit sur le sha poussé ; sinon \
         la branche reste poussée, sans PR.";
    CiMuette = 8008, "forge.ci_muette",
        "La CI de la PR rend son verdict dans son délai ; une PR sans CI n'est jamais prise.";
    AReintegrer = 8009, "forge.a_reintegrer",
        "Lydite ne relance une CI rouge que sur une PR sans conflit dont la base n'a pas bougé ; sinon la \
         base est à réintégrer.";
    PrChangee = 8010, "forge.pr_changee",
        "Pendant que Lydite attend sa CI, la PR porte le sha poussé, n'est pas fermée, et n'est pas fusionnée sur \
         une CI rouge ; sinon il s'arrête.";
    CiRouge = 8011, "forge.ci_rouge",
        "Une CI rouge vaut une seule relance, l'erreur en contexte ; rouge encore, ou la relance pas \
         assemblée, la PR reste ouverte et rouge.";
    IssueIllisible = 8012, "forge.issue_illisible",
        "L'issue que nomme `--issue` se lit par `gh`, existe sur le distant `origin` et est ouverte ; sinon \
         rien n'est lancé.";
    IssueTropLongue = 8013, "forge.issue_trop_longue",
        "Le titre et le corps de l'issue sont de l'UTF-8 et pèsent ensemble 64 Kio au plus (65 536 octets) ; \
         sinon rien n'est lancé.";
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.numero(), self.nom())
    }
}

/// Un refus qui enseigne : la règle que le code défend, la preuve de ce qui a été vu, la correction de ce qu'il faut
/// faire. Toute sortie de Kraken en 2 en est un, écrit sur stderr. Au fil, il se notera en champs plats : `code` (le
/// numéro), `regle`, `preuve`, `correction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refus {
    pub(crate) code: Code,
    /// Ce qui a été vu.
    pub(crate) preuve: String,
    /// Ce qu'il faut faire.
    pub(crate) correction: String,
}

impl Refus {
    /// Un refus de ce code. La règle vient du code ; la preuve dit ce qui a été vu, la correction ce qu'il faut faire,
    /// et ni l'une ni l'autre n'est vide : sans elles, le refus n'enseigne rien.
    pub(crate) fn new(code: Code, preuve: impl Into<String>, correction: impl Into<String>) -> Refus {
        let (preuve, correction) = (preuve.into(), correction.into());
        debug_assert!(
            !preuve.trim().is_empty() && !correction.trim().is_empty(),
            "{code} : un refus enseigne, sa preuve et sa correction ne sont pas vides"
        );
        Refus { code, preuve, correction }
    }
}

/// Quatre lignes, que `main` écrit sur stderr. Une preuve ou une correction sur plusieurs lignes (la sortie de git) se
/// met bout à bout : chacune des trois dernières tient sur la sienne.
impl fmt::Display for Refus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (code, regle) = (self.code, self.code.regle());
        let (preuve, correction) = (sur_une_ligne(&self.preuve), sur_une_ligne(&self.correction));
        write!(f, "lydite : {code}\n  règle : {regle}\n  preuve : {preuve}\n  correction : {correction}")
    }
}

impl std::error::Error for Refus {}

/// Une lecture ou une écriture ratée est la règle 6006. Celui qui la pose met le chemin ou le programme dans son
/// message et garde son `kind()` ; la preuve reprend ce message tel quel.
impl From<io::Error> for Refus {
    fn from(erreur: io::Error) -> Refus {
        Refus::new(
            Code::EntreeSortie,
            erreur.to_string(),
            "Vérifie que le chemin ou le programme de la preuve existe et que Lydite peut le lire, l'écrire ou le \
             lancer, puis relance.",
        )
    }
}

/// Le texte sur une seule ligne : ses lignes, rognées et non vides, mises bout à bout.
fn sur_une_ligne(texte: &str) -> String {
    texte.split(['\r', '\n']).map(str::trim).filter(|ligne| !ligne.is_empty()).collect::<Vec<_>>().join(" ; ")
}

/// L'erreur d'E/S avec ce qu'elle visait (un chemin, un programme) en tête de son message : `?` en fait le refus 6006,
/// dont la preuve le nomme. Son `kind()` ne change pas.
pub(crate) fn nommee(erreur: io::Error, ce: impl fmt::Display) -> io::Error {
    io::Error::new(erreur.kind(), format!("{ce} : {erreur}"))
}

/// Les 4000 derniers caractères, pour qu'une erreur tienne dans la consigne de relance.
pub(crate) fn queue(texte: &str) -> String {
    let n = texte.chars().count();
    texte.chars().skip(n.saturating_sub(4000)).collect()
}

#[cfg(test)]
mod tests;
