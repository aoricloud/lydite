//! Les codes de Kraken et le refus qui les porte, sur le modèle des erreurs du contrat v2 de passerelle. Un code est
//! un numéro figé, un nom `domaine.quoi` et la règle qu'il défend, en une phrase ; un refus dit cette règle, ce qui a
//! été vu et ce qu'il faut faire.
//!
//! Les codes sont rangés par blocs de mille, un domaine chacun, et chaque nom porte le préfixe de son bloc :
//!
//! - 1000 `contrat` : emprunté au contrat v2 de passerelle avec ses numéros, sans PR chez lui, et jamais étendu ici ;
//! - 6000 `lancer` : tout ce qui précède l'ouverture de la vague, c'est-à-dire la ligne de commande, ce que
//!   `preparer` lit (profil et fournisseur, dépôt, dossier), `lydite status` compris, et le plan, écrit à la main ou
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
//! 1000, 6001 à 6035 du bloc 6000, 7001 à 7021 du bloc 7000, 8001 à 8013 du bloc 8000. Les autres lignes du bloc 1000
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
        "A line of the thread is a JSON object in UTF-8 (RFC 8259).";
    VersionDifferente = 1002, "contrat.version_differente",
        "A reader reads `v` before anything else, and reads only the thread version it knows.";
    CommandeInconnue = 1005, "contrat.commande_inconnue",
        "The command is a word from Lydite's closed list, which the usage gives.";
    CommandeMalFormee = 1006, "contrat.commande_mal_formee",
        "The command line names a command and gives it the arguments of its usage, each flag once, with its \
         value if it takes one.";
    EvenementMalForme = 1009, "contrat.evenement_mal_forme",
        "A v2 line carries its whole envelope and flat values; a known type carries all its fields, each in \
         its domain, and no other.";
    SequenceRompue = 1010, "contrat.sequence_rompue",
        "Within a session, `seq` runs 1, 2, 3… in thread order, with no gap and no repeat.";
    // --- 6000 : lancer ---
    ProfilInconnu = 6001, "lancer.profil_inconnu",
        "A profile is named among the profiles Lydite knows.";
    DepotIntrouvable = 6002, "lancer.depot_introuvable",
        "The target repository is a folder that exists.";
    BacImpossible = 6003, "lancer.bac_impossible",
        "The target repository is a git repository with a commit, from which the task worktrees start; without \
         it, no wave opens.";
    DossierRelatif = 6004, "lancer.dossier_relatif",
        "`LYDITE_DOSSIER`, if set, is an absolute path.";
    DossierIntrouvable = 6005, "lancer.dossier_introuvable",
        "Without `LYDITE_DOSSIER`, Lydite's folder is taken from `USERPROFILE` on Windows, from \
         `XDG_STATE_HOME` or `HOME` elsewhere, each absolute.";
    EntreeSortie = 6006, "lancer.entree_sortie",
        "A read or a write by Lydite succeeds, or its refusal names its path or its program.";
    ProfilNomRefuse = 6007, "lancer.profil_nom_refuse",
        "A profile name is made of ASCII letters, digits, `_` and `-`.";
    ProfilIllisible = 6008, "lancer.profil_illisible",
        "A profile file is read as UTF-8 (BOM and carriage return tolerated); otherwise its refusal names its \
         path.";
    ProfilLigneRefusee = 6009, "lancer.profil_ligne_refusee",
        "Each line of the profile is `key = value` for a known key, its value in its domain, each key once; no \
         budget.";
    ProfilIncomplet = 6010, "lancer.profil_incomplet",
        "A profile says its kind.";
    ProfilNonAccepte = 6011, "lancer.profil_non_accepte",
        "A profile in a file is launched only once its fingerprint is accepted.";
    ProfilChange = 6012, "lancer.profil_change",
        "The launched profile has the accepted fingerprint; a new fingerprint is accepted again by `lydite \
         accept`.";
    ProfilSansFichier = 6013, "lancer.profil_sans_fichier",
        "Only a profile in a file is accepted; the built-in profile has no fingerprint to keep.";
    PreuveVide = 6014, "lancer.preuve_vide",
        "`--verify`, if given, carries a command that is not blank.";
    DossierSousLeDepot = 6015, "lancer.dossier_sous_le_depot",
        "Lydite's folder is outside the target repository.";
    PlanVersion = 6016, "lancer.plan_version",
        "A plan starts with its version, `plan 1`, which Lydite reads before anything else.";
    PlanEncodage = 6017, "lancer.plan_encodage",
        "A plan is UTF-8 text, a leading BOM tolerated; a plan in UTF-16 is rewritten in UTF-8.";
    PlanLigne = 6018, "lancer.plan_ligne",
        "In column 0, a plan line is empty, `tache <id>` within the limit of 10,000 tasks, or a known key \
         (`depend`, `preuve`, `modele`, `relance`, `fichiers`, `critique` under a task), once only and in its \
         place, before the indented lines; an indented line follows a task.";
    PlanIdentifiant = 6019, "lancer.plan_identifiant",
        "A task id is a word of ASCII letters and digits, `-` and `_`, unique in the plan.";
    PlanDependance = 6020, "lancer.plan_dependance",
        "A task depends only on other tasks of the plan, without a cycle.";
    PlanBlanc = 6021, "lancer.plan_blanc",
        "A plan has an objective, a proof and at least one task; each task has an objective, and a written \
         `preuve`, `modele`, `relance` or `critique` is not blank (only `depend` may be).";
    BaseRouge = 6022, "lancer.base_rouge",
        "The wave's proof passes on the base before any hand, so that a green assembly is owed only to the \
         tasks.";
    DossierCheminRefuse = 6023, "lancer.dossier_chemin_refuse",
        "A path that Lydite writes into a permission rule, its folder or a Claude connection folder, is \
         absolute and is written as is: on Windows it starts with a drive letter, and its names carry no \
         parenthesis, no `*`, `?`, `[`, `]`, `{`, `}`, no quote and no backslash.";
    PlanChemin = 6024, "lancer.plan_chemin",
        "A `fichiers` path is relative to the repository root, written with `/`, without `..`, outside `.git/` \
         and `.claude/`, without a comma; in a split plan, it has the case that git knows for it in the base.";
    PlanBorne = 6025, "lancer.plan_borne",
        "A plan weighs 1 MB at most; a split plan has 12 tasks at most, and each has `fichiers` and `modele`, \
         with no `preuve` of its own (all play the wave's); a `fichiers` line has 100 paths at most.";
    PlanEntete = 6026, "lancer.plan_entete",
        "A split plan carries the header that Lydite asked for: the human's objective and proof, as they are.";
    Fournisseur = 6027, "lancer.fournisseur",
        "A profile that names a provider gives `api`, an https URL, and `cle`, the name of an environment \
         variable that is set and not empty, never the key itself.";
    MurRefuse = 6028, "lancer.mur_refuse",
        "Under a required wall, the wall that replays the proof plays `true` before any hand and before the base \
         check (srt under `mur = exige`, Lydite's native wall under `mur = natif`); otherwise nothing is \
         launched.";
    IssueSansMur = 6029, "lancer.issue_sans_mur",
        "`--issue` only runs under a profile whose wall is required, because the text of an issue comes from \
         someone else: `mur = exige` in the profile, then `lydite accept <profile>`, or an objective typed by \
         hand.";
    Quota = 6030, "lancer.quota",
        "Under a daily quota that is set (`profils/quota`, in dollars), a hand does not start when the estimated \
         spend of the UTC day reaches it, when the file cannot be read, or when its profile with `api` has no \
         `prix` for its model; spending beyond it is for the human.";
    LeconMalFormee = 6031, "lancer.lecon_mal_formee",
        "A lesson is a `.agents/skills/<name>/SKILL.md` of 4096 bytes at most, in UTF-8: a header `name` (the \
         folder) and `description` (one line), then `## What happened`, `## Why`, `## How it is checked`, \
         `## Pitfall`, `## Files` (lines `- <path>`) and `## Falls when`, each once, in order, with text.";
    DossierPris = 6032, "lancer.dossier_pris",
        "A launch holds Lydite's folder for itself until its process ends: a second launch on the same folder \
         is refused, because they would write the same thread and read the same quota.";
    EffortNonPermis = 6033, "lancer.effort_non_permis",
        "A plan asks only for an effort that the profile allows (`efforts_permis`).";
    MemoireTexteAbsent = 6034, "lancer.memoire_texte_absent",
        "A memory recall names a line that the thread carries, by its address `<session>/<seq>` or by its \
         SHA-256; otherwise nothing is recalled.";
    MemoireEmpreinteFausse = 6035, "lancer.memoire_empreinte_fausse",
        "A memory recall by hash gives 64 lowercase hexadecimal digits; any other text without `/` is refused before \
         the thread is read.";
    // --- 7000 : vague ---
    VagueBacImpossible = 7001, "vague.bac_impossible",
        "Each task has its own worktree, which git creates just before its hand; a task without a worktree is \
         refused, and the others go on.";
    MainActive = 7002, "vague.main_active",
        "A task never has two active hands: a hand starts only on a task with no lease, open or expired.";
    ResultatPerime = 7003, "vague.resultat_perime",
        "A result is taken only if it carries the attempt of its task's open lease; any other is stale.";
    MainExpiree = 7004, "vague.main_expiree",
        "A hand that has returned nothing when its lease runs out expires, and its task is refused without a \
         retry.";
    MainTuee = 7005, "vague.main_tuee",
        "A hand that Lydite killed at its deadline is not retried, and its task is refused: killing it does \
         not prove that its whole tree is dead.";
    ClaudeTouche = 7006, "vague.claude_touche",
        "A hand touches no `.claude/` folder of its worktree, at any depth, except the state that Claude Code \
         keeps there itself outside git; Lydite checks it before any retry in that worktree, and at the \
         freeze, in what it commits as in the history it brings. A wall stop, an empty read-only ordinary file \
         laid by the sandbox, is not a write by the hand.";
    Conflit = 7007, "vague.conflit",
        "A hand's work merges onto the wave's head without conflict; in conflict, it is refused without a \
         retry.";
    PreuveRouge = 7008, "vague.preuve_rouge",
        "After each merge, the wave's proof passes on the head; red, the merge is undone and the task refused \
         without a retry.";
    GitImpossible = 7009, "vague.git_impossible",
        "The wave's git steps succeed; any other git failure refuses the task without advancing the wave's \
         branch.";
    AssemblageFerme = 7010, "vague.assemblage_ferme",
        "The wave's worktree returns to its head after each failure; if it cannot, the assembly closes and \
         nothing is merged into it any more.";
    Sautee = 7011, "vague.sautee",
        "A task starts only if all its dependencies are assembled; otherwise it is skipped, without a \
         worktree.";
    FilInjoignable = 7012, "vague.fil_injoignable",
        "Every step of the wave is noted in the thread before it acts; a failed note stops the starts and the \
         merges.";
    HorsFichiers = 7013, "vague.hors_fichiers",
        "A hand changes only the paths of its task's `fichiers`; any other path turns its attempt red.";
    BacTouche = 7014, "vague.bac_touche",
        "The splitter and the reviewer write only their own file, an ordinary one that the repository does not \
         ignore, at the root of their worktree; nothing else changes there.";
    RemarquesIllisibles = 7015, "vague.remarques_illisibles",
        "The reviewer's file follows the `remarques 2` format; otherwise its attempt is red.";
    RelectureManquee = 7016, "vague.relecture_manquee",
        "The work of a critical task is pushed only once reviewed; a review that is set aside, and nothing is \
         pushed.";
    BloquantesRestantes = 7017, "vague.bloquantes_restantes",
        "Blocking remarks are worth a single correction; not assembled, nothing is pushed.";
    CheminProtege = 7018, "vague.chemin_protege",
        "A hand changes no path that its profile protects (proteges), neither by an edit nor by a command.";
    PlafondAtteint = 7019, "vague.plafond_atteint",
        "A hand that reaches its spending cap is stopped and not retried; its task is refused, and what \
         follows is for the human.";
    BloquanteHorsDePortee = 7020, "vague.bloquante_hors_de_portee",
        "A blocking remark targets a file that the correction cannot touch; the wave stops before any correction \
         and nothing is pushed.";
    OuvertureExecutable = 7021, "vague.ouverture_executable",
        "A hand adds or changes no file that runs when the branch is next opened (.mcp.json, .vscode, .idea, a \
         shell or git configuration file); the freeze that carries one is refused before the merge.";
    // --- 8000 : forge ---
    DistantIllisible = 8001, "forge.distant_illisible",
        "With `--pr`, the target repository has a remote `origin` whose URL names a host, an owner and a \
         repository.";
    BaseInconnue = 8002, "forge.base_inconnue",
        "The base that `--pr` names is a branch of the remote, which Lydite fetches before the wave.";
    GhInjoignable = 8003, "forge.gh_injoignable",
        "`gh` starts and says it is logged in to the remote's host before the wave.";
    RienAPousser = 8004, "forge.rien_a_pousser",
        "The wave's branch has at least one more commit than its base; otherwise nothing is pushed.";
    PublicationRefusee = 8005, "forge.publication_refusee",
        "Lydite publishes no change under `.github/`, no path of the workstation, no commit it did not make, \
         and no word of the user's `garde` file; publishing those is for the human.";
    PousseeRefusee = 8006, "forge.poussee_refusee",
        "The wave is pushed without force, within its deadline; otherwise nothing is opened.";
    PrImpossible = 8007, "forge.pr_impossible",
        "Once the branch is pushed, `gh` opens the PR against the base and reads it back on the pushed sha; \
         otherwise the branch stays pushed, with no PR.";
    CiMuette = 8008, "forge.ci_muette",
        "The PR's CI gives its verdict within its deadline; a PR without CI is never taken.";
    AReintegrer = 8009, "forge.a_reintegrer",
        "Lydite retries a red CI only on a PR without conflict whose base has not moved; otherwise the base is \
         to be merged back in.";
    PrChangee = 8010, "forge.pr_changee",
        "While Lydite waits for its CI, the PR carries the pushed sha, is not closed, and is not merged on a \
         red CI; otherwise it stops.";
    CiRouge = 8011, "forge.ci_rouge",
        "A red CI is worth a single retry, with the error in context; red again, or the retry not assembled, \
         the PR stays open and red.";
    IssueIllisible = 8012, "forge.issue_illisible",
        "The issue that `--issue` names is read by `gh`, exists on the remote `origin` and is open; otherwise \
         nothing is launched.";
    IssueTropLongue = 8013, "forge.issue_trop_longue",
        "The issue's title and body are UTF-8 and weigh 64 KiB at most together (65,536 bytes); otherwise \
         nothing is launched.";
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
        write!(f, "lydite : {code}\n  rule: {regle}\n  seen: {preuve}\n  fix: {correction}")
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
            "Check that the path, or the program of the proof, exists and that Lydite can read, write or run it, \
             then retry.",
        )
    }
}

/// Le texte sur une seule ligne : ses lignes, rognées et non vides, mises bout à bout, et tout autre caractère de
/// contrôle écrit (`coeur::texte`).
fn sur_une_ligne(texte: &str) -> String {
    let bout_a_bout =
        texte.split(['\r', '\n']).map(str::trim).filter(|ligne| !ligne.is_empty()).collect::<Vec<_>>().join(" ; ");
    crate::coeur::texte::en_ligne(&bout_a_bout)
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
