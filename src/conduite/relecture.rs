//! La relecture : après la vague d'un plan, une main, le relecteur, relit le travail de ses tâches critiques
//! (`critique : oui`) et en dit des remarques, que Kraken lit lui-même ; des remarques bloquantes valent une seule
//! correction. `lydite remarques <fichier>` lit un fichier de remarques comme Kraken le lira. Dans l'ordre, une fois la
//! vague finie, avant la forge ([`relire`]) :
//!
//! 1. sans tâche critique assemblée, il n'y a pas de relecture ;
//! 2. sinon le relecteur part : une vague de copies d'une main, dans un bac posé sur `t`, la tête de l'assemblage.
//!    Son modèle est la clé `relecteur` du profil, pour ses deux tentatives. Avant qu'il parte, Kraken écrit dans son
//!    bac [`DIFF`], le diff de la base à `t` limité aux `fichiers` des tâches critiques assemblées, 1 Mo au plus :
//!    sous Windows, la main ne peut lancer que sa preuve, et ne peut donc pas le lire de git. À côté, Kraken écrit
//!    [`FICHIER_PREUVE`] : la commande de preuve de la vague, le commit `t` où Lydite l'a rejouée verte, et sa sortie
//!    telle que le dernier `vague.fusion` de la vague la garde (4000 derniers caractères) ([`texte_de_la_preuve`]) ;
//!    sans `vague.fusion` qui porte `t`, ou sans commande de preuve, le fichier le dit en une ligne. Le fichier du
//!    relecteur est en forme 2 (`remarques 2`, avec ses blocs `rejoue` et `non rejoue`), que le juge exige. Sa
//!    consigne est un texte fixe de Kraken ([`consigne`]), dont la preuve prêtée est [`PREUVE`], avec le dossier de
//!    l'exécutable de Kraken en tête de son `PATH` (`tentative::Juge`) ;
//! 3. Kraken rejoue la preuve lui-même ([`juger`]) : [`REMARQUES`] est un fichier ordinaire que le dépôt visé n'ignore
//!    pas, les seuls changements du bac sont lui et [`DIFF`] (sinon 7014, `assemblage::seul_change`), et il se lit
//!    ([`lire_fichier`] puis [`crate::coeur::remarques::Fichier::exiger_la_forme_2`], sinon 7015). Rouge, le
//!    relecteur a une relance, le refus en contexte ; rouge encore, la relecture est classée : le travail critique
//!    n'a pas été relu, rien ne se pousse, et c'est le refus 7016 ;
//! 4. prise, la relecture se note au fil avec le nombre de remarques et leur texte (le fil reste local), puis son
//!    verdict (`vague.verdict` : la tête `t` relue, et le profil, l'empreinte et le modèle du lancement du relecteur,
//!    `null` pour une main nue), avant la correction et la forge ; puis le bac du relecteur se retire. Sans remarque
//!    bloquante, la forge peut partir de `t`. Avec, une seule correction part :
//!    une vague d'une tâche `correction`, partie de `t`, de modèle la clé `relance` du profil, de `fichiers` l'union
//!    de ceux des tâches critiques, de preuve celle de la vague. Assemblée en `t2`, elle fait avancer la branche de
//!    la vague de `t` à `t2`, d'où la forge part ; non assemblée, ou sans rien changer à `t`, rien ne se pousse, et
//!    c'est le refus 7017. La correction n'a pas de seconde relecture.
//!
//! Une vague née d'une issue se relit par [`relire_issue`], sous [`Origine::Issue`] ; hors issue, rien ne change.
//! Sous elle, le travail se relit en entier ou pas du tout : avant que le relecteur parte, Kraken pèse le diff de
//! l'étape 2, lu depuis la racine du dépôt visé comme il l'est dans le bac, quel que soit le dossier d'où Kraken part ;
//! s'il dépasse [`TAILLE_MAX`] octets, la coupe qu'il subirait vaut une relecture manquée, c'est le refus 7016, rien ne
//! se pousse, et aucun relecteur ne part.
//!
//! Le texte des remarques n'est jamais publié : il vit au fil, et la correction le reçoit comme objectif, que le corps
//! de la PR ne reprend pas (`publication::corps` n'en garde que les nombres), à une exception : les lignes du bloc
//! `non rejoue`, dix au plus et coupées à 200 caractères, sous « Not replayed (from the review): ».
//! Avant toute correction, une remarque bloquante dont le fichier n'est couvert (`plan::couvre`) par aucune entrée des
//! `fichiers` de la correction arrête la relecture : c'est le refus 7020, aucune main ne part et rien ne se pousse ;
//! sans `fichiers` aux tâches critiques, rien ne borne la correction et la règle ne joue pas.
//! Les refus 7016, 7017 et 7020 se notent au fil
//! par un `forge.refus` de la vague relue.

use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::branchements::dossier::{Fil, lire};
use crate::branchements::git::{faire, faire_borne, geste, racines};
use crate::coeur::erreur::{Code, Refus, nommee, queue};
use crate::coeur::fil::Evenement;
use crate::coeur::plan::{Plan, TAILLE_MAX, Tache, couvre};
use crate::coeur::profil::Profil;
use crate::coeur::remarques::{FORME, Remarque, lire_fichier};
use crate::conduite::alarmes;
use crate::conduite::assemblage::seul_change;
use crate::conduite::decoupage::retirer_le_bac;
use crate::conduite::lancer::{Liens, Menee, en_erreur, lancer_avec};

/// Le fichier du relecteur, à la racine de son bac.
pub(crate) const REMARQUES: &str = "REMARQUES.lydite";

/// Le diff que Kraken écrit dans le bac du relecteur avant qu'il parte : le travail à relire.
pub(crate) const DIFF: &str = "DIFF.lydite";

/// Le fichier que Kraken écrit dans le bac du relecteur, à côté de [`DIFF`] : la preuve que Lydite a rejouée sur la
/// tête relue ([`texte_de_la_preuve`]).
pub(crate) const FICHIER_PREUVE: &str = "PREUVE.lydite";

/// Les derniers caractères de la sortie de la preuve que [`FICHIER_PREUVE`] garde, telle que `vague.fusion` la garde.
const SORTIE_MAX: usize = 4000;

/// La preuve que Kraken prête au relecteur, seule et telle quelle : `lydite remarques` lit son fichier comme Kraken le
/// lira. Kraken ne la rejoue pas : son verdict est celui de [`juger`].
pub(crate) const PREUVE: &str = "lydite remarques REMARQUES.lydite";

/// La première ligne de l'objectif de la correction ; les remarques bloquantes la suivent.
const CORRIGER: &str = "Corrige au plus petit les remarques bloquantes de la relecture, et rien d'autre";

/// La dernière ligne de l'objectif de la correction : la main répond à chaque remarque bloquante.
const REPONDRE: &str = "Réponds à chaque remarque bloquante : dans ton dernier message, nomme chacune par son \
                        fichier et sa ligne, avec le changement qui y répond, ou dis pourquoi tes fichiers ne le \
                        permettent pas.";

/// La correction d'une relecture classée (7016).
const SANS_RELECTURE: &str = "Rien ne se pousse : le travail des tâches critiques n'est pas relu. Lis l'erreur du \
                              relecteur, puis relance la vague, ou relis son travail toi-même avant de le pousser.";

/// La correction d'une correction qui ne s'assemble pas (7017).
const SANS_CORRECTION: &str = "Rien ne se pousse : des remarques bloquantes restent sans correction. Lis-les au fil \
                               (vague.relecture), puis corrige-les toi-même dans la branche de la vague, ou relance \
                               une vague neuve.";

/// La correction d'une remarque bloquante hors de portée (7020).
const SANS_PORTEE: &str = "Nothing is pushed: no correction was started. Rerun the task in a fresh wave with that \
                           file in its `fichiers`, or fix it yourself on the branch, then have it reviewed.";

/// La correction d'un fil qui ne reçoit pas la note d'une relecture prise (7012).
const SANS_NOTE: &str = "Rien ne se pousse : une relecture que le fil n'a pas reçue ne compte pas, et son bac demeure. \
                         Rends le fil inscriptible, puis relance la vague.";

/// Les octets d'un fichier de remarques, [`TAILLE_MAX`] au plus et un de plus pour voir qu'il dépasse : un fichier plus
/// gros ne se lit pas en entier.
pub(crate) fn lire_borne(fichier: &Path) -> io::Result<Vec<u8>> {
    let mut octets = Vec::new();
    fs::File::open(fichier).and_then(|ouvert| ouvert.take(TAILLE_MAX as u64 + 1).read_to_end(&mut octets))?;
    Ok(octets)
}

/// Le juge du relecteur, sur son bac parti de `depart` : [`REMARQUES`] y est un fichier ordinaire que le dépôt n'ignore
/// pas, et le seul changement du bac avec [`DIFF`], que Kraken y a écrit (7014, ou 7009 si git ne le dit pas) ; puis il
/// se lit (7015 ; une lecture ratée, 6006). Rend les remarques et leur texte, ou le refus.
fn juger(bac: &Path, depart: &str) -> Result<(Vec<Remarque>, String), Refus> {
    seul_change(bac, depart, REMARQUES, &[DIFF, FICHIER_PREUVE])?;
    let chemin = bac.join(REMARQUES);
    let octets = lire_borne(&chemin).map_err(|e| Refus::from(nommee(e, format!("lecture de {}", chemin.display()))))?;
    let fichier = lire_fichier(&octets)?.exiger_la_forme_2()?;
    Ok((fichier.remarques, String::from_utf8_lossy(&octets).into_owned()))
}

/// Le texte de [`FICHIER_PREUVE`] : la commande de preuve de la vague `id` (`preuve`), le commit `tete` où Lydite l'a
/// rejouée verte, et la sortie telle que le dernier `vague.fusion` de la vague qui porte `tete` la garde (`verif`),
/// ses [`SORTIE_MAX`] derniers caractères. Sans commande de preuve, ou sans `vague.fusion` qui porte `tete`, le texte
/// le dit en une ligne, sans sortie.
fn texte_de_la_preuve(preuve: Option<&str>, evenements: &[Evenement], id: &str, tete: &str) -> String {
    let Some(preuve) = preuve.map(str::trim).filter(|preuve| !preuve.is_empty()) else {
        return "This wave has no proof command: nothing was replayed.\n".to_string();
    };
    let sortie = evenements.iter().rev().find_map(|evenement| match evenement {
        Evenement::Fusion { vague, sha, verif, .. } if vague == id && sha == tete => Some(verif.as_str()),
        _ => None,
    });
    let Some(verif) = sortie else {
        return format!("No vague.fusion of this wave carries commit {tete}: the proof was not replayed on it.\n");
    };
    let sauter = verif.chars().count().saturating_sub(SORTIE_MAX);
    let fin: String = verif.chars().skip(sauter).collect();
    let nl = if fin.ends_with('\n') || fin.is_empty() { "" } else { "\n" };
    format!(
        "Proof command: {preuve}\nReplayed green by Lydite on commit: {tete}\nOutput (last {SORTIE_MAX} characters), \
         as Lydite kept it:\n{fin}{nl}"
    )
}

/// Le plafond, en caractères, de la couche fixe du relecteur (sa consigne sans objectif ni tâche, avec la phrase des
/// points à justifier et un point) : sa taille d'aujourd'hui (2569, avec la phrase de la preuve et les blocs de la
/// forme 2) arrondie au 500 supérieur.
#[cfg(test)]
const PLAFOND: usize = 3000;

/// Le nombre de points à justifier ([`alarmes::points`]) que la consigne nomme.
const POINTS_NOMMES: usize = 20;

/// La consigne du relecteur : un texte fixe de Kraken, où n'entrent que l'objectif de la vague, puis l'id, les
/// fichiers et l'objectif de chaque tâche critique, puis, s'il y en a, les `points` à justifier (20 au plus).
fn consigne(objectif: &str, critiques: &[&Tache], points: &[String]) -> String {
    let mut a_justifier = String::new();
    if !points.is_empty() {
        a_justifier.push_str(
            "Lydite found these points without a model, in the diff you review. For each one, say in a remark \
             whether it is justified:\n\n",
        );
        for point in points.iter().take(POINTS_NOMMES) {
            a_justifier.push_str(&format!("- {point}\n"));
        }
        a_justifier.push('\n');
    }
    let mut taches = String::new();
    for tache in critiques {
        let id = &tache.id;
        let fichiers = tache.fichiers.as_ref().map_or_else(|| "tous, rien ne la borne".to_string(), |f| f.join(" "));
        taches.push_str(&format!("- tâche {id}, fichiers : {fichiers}\n"));
        for ligne in tache.objectif.lines() {
            taches.push_str(&format!("    {ligne}\n"));
        }
    }
    format!(
        "Relis le travail des tâches critiques d'une vague de Lydite, sans rien construire ni corriger toi-même : \
         d'autres mains corrigeront ce que tu juges bloquant.\n\n\
         Objectif de la vague : {objectif}\n\n\
         Les tâches critiques, dont le travail est assemblé :\n\n\
         {taches}\n\
         {a_justifier}\
         Le travail à relire est dans le fichier {DIFF}, à la racine de ton dossier : le diff entre la base de la \
         vague et son travail, limité aux fichiers de ces tâches. Ton \
         dossier porte le dépôt tel que ce travail le laisse : lis-y ce que le diff ne montre pas.\n\n\
         {FICHIER_PREUVE} holds the proof that Lydite replayed on this commit: judge the work on it and on the code, \
         never on what a hand says it ran.\n\n\
         Une remarque est bloquante (`bloquante : oui`) si le travail manque son objectif, perd des données, publie \
         ce qu'il ne doit pas, affaiblit un contrôle, ou laisse un test qui ne peut pas échouer. Toute autre remarque \
         est à noter (`bloquante : non`).\n\n\
         Écris tes remarques dans le fichier {REMARQUES}, à la racine de ton dossier, et ne touche à rien d'autre : \
         ni à un autre fichier, ni à l'histoire du dépôt (aucun commit). Sa forme :\n\n\
         {FORME}\n\n\
         La première ligne du fichier est exactement `remarques 2`, quel que soit le nombre de remarques (aucune, \
         une, dix) : le 2 est la forme du fichier, jamais un nombre de remarques.\n\n\
         Les remarques sont numérotées 1, 2, 3… dans l'ordre. `fichier` est un chemin relatif à la racine du dépôt, \
         écrit avec `/`. `ligne` est facultative : le numéro de la ligne du fichier, tel que le travail le laisse. \
         `bloquante` vaut `oui` ou `non`. Le texte dit le constat, puis la correction, en lignes indentées. Après la \
         dernière remarque, ou seuls s'il n'y en a pas, les blocs `rejoue` puis `non rejoue` sont obligatoires, dans \
         cet ordre, chacun suivi d'au moins une ligne indentée : `rejoue` dit ce que montre la preuve rejouée ; \
         `non rejoue`, ce que le travail affirme ou demande et que rien de rejoué ici ne montre (une autre \
         plateforme, le réseau, un chemin qu'aucun test ne joue).\n\n\
         Quand tu as fini, joue toi-même dans ton dossier, seule et telle qu'elle est écrite, sans rien y ajouter : \
         {PREUVE}. Elle lit ton fichier comme Lydite le lira, et dit ce qui ne va pas : corrige-le jusqu'à ce qu'elle \
         passe. Lydite relira ensuite {REMARQUES} lui-même. La dernière ligne de ta réponse est exactement PREUVE: \
         vert si elle passe, sinon PREUVE: rouge."
    )
}

/// Les `fichiers` de ces tâches, réunis sans doublon, dans l'ordre ; `None` si l'une n'a pas de `fichiers` : rien ne la
/// borne, et rien ne borne donc ce qui s'en relit ou s'en corrige.
fn fichiers_de(taches: &[&Tache]) -> Option<Vec<String>> {
    let mut tous: Vec<String> = Vec::new();
    for tache in taches {
        for fichier in tache.fichiers.as_ref()? {
            if !tous.contains(fichier) {
                tous.push(fichier.clone());
            }
        }
    }
    Some(tous).filter(|tous| !tous.is_empty())
}

/// Écrit [`DIFF`] dans le bac : `git diff <base> <tete>`, limité à `fichiers` (tous les chemins s'il n'y en a pas),
/// sans couleur, ni programme de diff ni conversion de texte du dépôt visé, et sans joker dans les chemins
/// (`GIT_LITERAL_PATHSPECS`). La sortie se lit bornée : Kraken en garde [`TAILLE_MAX`] + 1 octets et compte le reste
/// sans le garder. Un git qui ne le dit pas est le refus 7009 ; une écriture ratée, le refus 6006.
fn ecrire_le_diff(bac: &Path, base: &str, tete: &str, fichiers: Option<&[String]>) -> Result<(), Refus> {
    let (diff, total) = lire_le_diff(bac, base, tete, fichiers)?;
    let chemin = bac.join(DIFF);
    fs::write(&chemin, borner(diff, total))
        .map_err(|e| Refus::from(nommee(e, format!("écriture de {}", chemin.display()))))
}

/// Le diff de [`ecrire_le_diff`], lu borné dans le worktree `dossier` : ce que Kraken en garde ([`TAILLE_MAX`] + 1
/// octets au plus, en texte), et le nombre d'octets que git a écrits. Un git qui ne le dit pas est le refus 7009.
fn lire_le_diff(dossier: &Path, base: &str, tete: &str, fichiers: Option<&[String]>) -> Result<(String, u64), Refus> {
    let mut diff = geste(dossier, &["diff", "--no-color", "--no-ext-diff", "--no-textconv", base, tete, "--"]);
    diff.env("GIT_LITERAL_PATHSPECS", "1").args(fichiers.unwrap_or_default());
    let correction = "Vérifie que git se lance et que le bac est un worktree du dépôt visé : sans le diff, le \
                      relecteur ne part pas.";
    let (gardes, total) =
        faire_borne(diff, TAILLE_MAX + 1).map_err(|vu| Refus::new(Code::GitImpossible, vu, correction))?;
    Ok((String::from_utf8_lossy(&gardes).into_owned(), total))
}

/// Ce que pèse le diff tel que [`borner`] le voit : `diff` est ce que la lecture bornée en a gardé et `total` le nombre
/// d'octets que git a écrits. Un octet qui n'est pas de l'UTF-8 devient plusieurs caractères de remplacement : le texte
/// gardé peut alors peser plus que les octets lus, et le plus grand des deux compte. Au-delà de [`TAILLE_MAX`], le diff
/// est coupé.
fn poids(diff: &str, total: u64) -> u64 {
    total.max(diff.len() as u64)
}

/// Le diff, [`TAILLE_MAX`] octets au plus : `diff` est ce que la lecture bornée en a gardé et `total` le nombre
/// d'octets que git a écrits. Au-delà de [`TAILLE_MAX`] ([`poids`]), il est coupé à une fin de ligne, et une dernière
/// ligne le dit.
fn borner(diff: String, total: u64) -> String {
    let total = poids(&diff, total);
    if total <= TAILLE_MAX as u64 {
        return diff;
    }
    let mention = |gardes: u64| {
        format!("[diff coupé par Lydite : {gardes} octets gardés sur {total}, le reste n'est pas montré]\n")
    };
    // La mention se pèse au plus long, celle de `total` octets gardés, et un saut de ligne de plus s'il faut clore la
    // ligne.
    let limite = TAILLE_MAX - mention(total).len() - 1;
    let mut fin = limite;
    while !diff.is_char_boundary(fin) {
        fin -= 1;
    }
    let fin = diff[..fin].rfind('\n').map_or(fin, |saut| saut + 1);
    let clore = if fin == 0 || diff[..fin].ends_with('\n') { "" } else { "\n" };
    format!("{}{clore}{}", &diff[..fin], mention(fin as u64))
}

/// Ce que la relecture d'une vague rend.
pub(crate) enum Relue {
    /// Aucune tâche critique assemblée : il n'y a rien à relire.
    Aucune,
    /// Relue, et corrigée si des remarques bloquantes l'ont voulu : la forge peut partir.
    Prise(Prise),
    /// Rien ne se pousse : le refus 7016 (la relecture classée), 7017 (la correction non assemblée), 7020 (une
    /// bloquante hors de portée) ou 7012 (une note ratée), noté au fil quand le fil le permet.
    Dehors(Refus),
}

/// Une relecture prise.
pub(crate) struct Prise {
    /// La main du relecteur, `main-<vague>-1`.
    pub(crate) main: String,
    pub(crate) n: usize,
    pub(crate) bloquantes: usize,
    /// La vague de la correction, assemblée, si des remarques bloquantes en ont voulu une : sa tête est `t2`.
    pub(crate) correction: Option<Menee>,
}

/// D'où vient la vague relue : d'un plan, ou d'une issue. Sous une issue, la relecture ne se fait pas en partie
/// (un diff coupé est le refus 7016).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origine {
    Plan,
    Issue,
}

/// La relecture de la vague `menee` du plan `plan`, partie de `base` (voir l'en-tête du module), sous le bail `bail` de
/// chaque main. Ne rend jamais d'erreur : ce qui empêche de relire ou de corriger est un [`Relue::Dehors`].
pub(crate) fn relire(
    depot: &Path,
    dossier: &Path,
    profil: &Profil,
    plan: &Plan,
    menee: &Menee,
    base: &str,
    bail: Duration,
) -> Relue {
    relire_selon(depot, dossier, profil, plan, menee, base, bail, Origine::Plan)
}

/// La relecture d'une vague née d'une issue : celle de [`relire`], sous [`Origine::Issue`].
pub(crate) fn relire_issue(
    depot: &Path,
    dossier: &Path,
    profil: &Profil,
    plan: &Plan,
    menee: &Menee,
    base: &str,
    bail: Duration,
) -> Relue {
    relire_selon(depot, dossier, profil, plan, menee, base, bail, Origine::Issue)
}

/// La relecture de [`relire`], selon l'origine de la vague.
#[allow(clippy::too_many_arguments)]
fn relire_selon(
    depot: &Path,
    dossier: &Path,
    profil: &Profil,
    plan: &Plan,
    menee: &Menee,
    base: &str,
    bail: Duration,
    origine: Origine,
) -> Relue {
    // Une vague arrêtée (7012) n'a plus de fil sûr, et des copies n'ont pas de tête : la forge le dit.
    let Some(tete) = menee.tete.as_deref().filter(|_| !menee.arretee) else { return Relue::Aucune };
    let critiques: Vec<&Tache> = (plan.taches.iter().zip(&menee.issues))
        .filter(|(tache, (_, issue))| tache.critique && issue.is_ok())
        .map(|(tache, _)| tache)
        .collect();
    if critiques.is_empty() {
        return Relue::Aucune;
    }
    let relecture = Relecture { depot, dossier, profil, plan, menee, base, tete, bail, critiques, origine };
    match relecture.mener() {
        Ok(prise) => Relue::Prise(prise),
        Err(refus) => Relue::Dehors(relecture.noter_le_refus(refus)),
    }
}

/// Ce que la relecture d'une vague tient d'elle : de quoi mener le relecteur, puis sa correction.
struct Relecture<'a> {
    depot: &'a Path,
    dossier: &'a Path,
    profil: &'a Profil,
    plan: &'a Plan,
    /// La vague relue : son id et sa branche.
    menee: &'a Menee,
    /// La base de la vague, d'où le diff se compte.
    base: &'a str,
    /// La tête `t` de la vague, d'où partent le relecteur et la correction.
    tete: &'a str,
    bail: Duration,
    /// Ses tâches critiques assemblées.
    critiques: Vec<&'a Tache>,
    /// D'où vient la vague relue.
    origine: Origine,
}

/// Ce que le relecteur a rendu : sa vague et sa main, ses remarques et leur texte.
struct Lue {
    vague: String,
    main: String,
    remarques: Vec<Remarque>,
    texte: String,
}

impl Relecture<'_> {
    /// Le relecteur, puis, des remarques bloquantes, la correction. Le refus est le 7016, le 7017, le 7020 (une
    /// remarque bloquante vise un fichier que la correction ne peut pas toucher : aucune correction ne part) ou le
    /// 7012.
    fn mener(&self) -> Result<Prise, Refus> {
        let Lue { vague, main, remarques, texte } = self.relecteur()?;
        let (n, bloquantes) = (remarques.len(), remarques.iter().filter(|remarque| remarque.bloquante).count());
        let vague_du_relecteur = vague.clone();
        let note = Evenement::Relecture { vague, relue: self.menee.id.clone(), n, bloquantes, remarques: texte };
        if let Err(erreur) = Fil::seance(self.dossier, "humain").noter(&note) {
            let preuve = format!("la relecture {main} de la vague {} n'est pas notée : {erreur}", self.menee.id);
            return Err(Refus::new(Code::FilInjoignable, preuve, SANS_NOTE));
        }
        // Le verdict : la tête relue, et le profil, l'empreinte et le modèle du lancement du relecteur.
        let (profil, empreinte, modele) = self.lancement_du_relecteur(&vague_du_relecteur).map_err(|erreur| {
            let preuve = format!("le lancement de la relecture {main} ne se lit pas au fil : {erreur}");
            Refus::new(Code::FilInjoignable, preuve, SANS_NOTE)
        })?;
        let sha = self.tete.to_string();
        let verdict = Evenement::VerdictRelecture {
            vague: vague_du_relecteur,
            relue: self.menee.id.clone(),
            sha,
            profil,
            empreinte,
            modele,
        };
        if let Err(erreur) = Fil::seance(self.dossier, "humain").noter(&verdict) {
            let preuve =
                format!("le verdict de la relecture {main} de la vague {} n'est pas noté : {erreur}", self.menee.id);
            return Err(Refus::new(Code::FilInjoignable, preuve, SANS_NOTE));
        }
        let ecrits = [REMARQUES, DIFF, FICHIER_PREUVE];
        retirer_le_bac(self.depot, self.dossier, "le bac du relecteur", &main, self.tete, &ecrits);
        if let Some((numero, fichier)) = self.bloquante_hors_de_portee(&remarques) {
            let preuve = format!(
                "the review of wave {} has a blocking remark, number {numero}, on {fichier}, \
                 which the correction cannot touch",
                self.menee.id
            );
            return Err(Refus::new(Code::BloquanteHorsDePortee, preuve, SANS_PORTEE));
        }
        let correction = if bloquantes == 0 { None } else { Some(self.corriger(&remarques)?) };
        Ok(Prise { main, n, bloquantes, correction })
    }

    /// Le numéro (à partir de 1) et le fichier de la première remarque bloquante qu'aucune entrée des fichiers de la
    /// correction ne couvre (`plan::couvre`, à la lettre). Quand une tâche critique n'a pas de `fichiers`
    /// ([`fichiers_de`] rend `None`), rien ne borne la correction : aucune remarque n'est hors de portée. Une remarque
    /// non bloquante n'y change rien.
    fn bloquante_hors_de_portee<'r>(&self, remarques: &'r [Remarque]) -> Option<(usize, &'r str)> {
        let fichiers = fichiers_de(&self.critiques)?;
        remarques.iter().enumerate().find_map(|(i, remarque)| {
            let couvert = fichiers.iter().any(|entree| couvre(entree, &remarque.fichier));
            (remarque.bloquante && !couvert).then_some((i + 1, remarque.fichier.as_str()))
        })
    }

    /// Le profil, l'empreinte et le modèle du dernier `main.lancement` de la main du relecteur dans sa vague `vague`,
    /// lus au fil ; `None` pour chacun si la main est nue et n'en a noté aucun. Un fil qui ne se relit pas est une
    /// erreur.
    #[allow(clippy::type_complexity)]
    fn lancement_du_relecteur(&self, vague: &str) -> Result<(Option<String>, Option<String>, Option<String>), String> {
        let lu = lire(self.dossier).map_err(|refus| refus.preuve)?;
        let lancement = lu.notes.into_iter().rev().find_map(|note| match note.evenement {
            Evenement::Lancement { vague: v, main: 1, profil, empreinte, modele, .. } if v == vague => {
                Some((profil, empreinte, modele))
            }
            _ => None,
        });
        Ok(match lancement {
            Some((profil, empreinte, modele)) => (Some(profil), Some(empreinte), Some(modele)),
            None => (None, None, None),
        })
    }

    /// Écrit [`FICHIER_PREUVE`] dans le bac du relecteur, d'après le fil ([`texte_de_la_preuve`]). Un fil qui ne se
    /// relit pas, ou une écriture ratée (6006), est le refus.
    fn ecrire_la_preuve(&self, bac: &Path) -> Result<(), Refus> {
        let lu = lire(self.dossier)?;
        let evenements: Vec<Evenement> = lu.notes.into_iter().map(|note| note.evenement).collect();
        let texte = texte_de_la_preuve(self.plan.preuve.as_deref(), &evenements, &self.menee.id, self.tete);
        let chemin = bac.join(FICHIER_PREUVE);
        fs::write(&chemin, texte).map_err(|e| Refus::from(nommee(e, format!("écriture de {}", chemin.display()))))
    }

    /// Le relecteur : sa vague de copies, son juge, son diff. Rend ce qu'il a rendu, ou le refus 7016. Sous une
    /// issue, un diff qui serait coupé est le refus 7016, sans que le relecteur parte.
    fn relecteur(&self) -> Result<Lue, Refus> {
        let (id, tete, base) = (&self.menee.id, self.tete, self.base);
        let fichiers = fichiers_de(&self.critiques);
        // Un git qui ne lit pas le diff est dit par `ecrire_le_diff` (7009) : sans lui, pas de point. Un échec de
        // lecture a deux effets : la publication s'arrête (`points_a_justifier`, comme `reglages_changes`), et le
        // relecteur ne reçoit aucun point (`unwrap_or_default`).
        let points = racines(self.depot)
            .ok()
            .and_then(|(racine, _)| alarmes::lire(&racine, base, tete, fichiers.as_deref()).ok())
            .unwrap_or_default();
        let relecteur = plan_du_relecteur(self.profil, self.plan, &self.critiques, &points);
        let classee = |pourquoi: String| {
            let preuve = format!("la relecture de la vague {id} est classée : {pourquoi}");
            Refus::new(Code::RelectureManquee, preuve, SANS_RELECTURE)
        };
        if self.origine == Origine::Issue {
            // Depuis la racine du dépôt visé, comme dans le bac : les `fichiers` sont relatifs à elle, et le dépôt visé
            // peut être un de ses sous-dossiers (le dossier courant de Kraken, par défaut).
            let ne_se_lit_pas =
                |refus: Refus| classee(format!("son diff ne se lit pas, {} : {}", refus.code, refus.preuve));
            let (racine, _) = racines(self.depot).map_err(ne_se_lit_pas)?;
            let (diff, total) = lire_le_diff(&racine, base, tete, fichiers.as_deref()).map_err(ne_se_lit_pas)?;
            let poids = poids(&diff, total);
            if poids > TAILLE_MAX as u64 {
                return Err(classee(format!(
                    "son diff pèse {poids} octets, plus que 1 Mo ({TAILLE_MAX} octets), et une vague née d'une issue \
                     ne se relit pas en partie"
                )));
            }
        }
        // Ce que le juge a lu à la prise : c'est lui que le fil note.
        let lu: Mutex<Option<(Vec<Remarque>, String)>> = Mutex::new(None);
        let juge = |bac: &Path| -> Result<(), String> {
            let pris = juger(bac, tete).map_err(|refus| en_erreur(&refus))?;
            *lu.lock().unwrap_or_else(PoisonError::into_inner) = Some(pris);
            Ok(())
        };
        let preparer = |bac: &Path| {
            ecrire_le_diff(bac, base, tete, fichiers.as_deref())?;
            self.ecrire_la_preuve(bac)
        };
        let liens = Liens {
            juge: Some(&juge),
            relit: Some(id),
            preparation: Some(&preparer),
            lecons: Some(&self.menee.lecons),
            ..Liens::default()
        };
        let profil = self.profil.pour_le_relecteur();
        let rendue = lancer_avec(self.depot, self.dossier, &profil, &relecteur, self.bail, tete, liens)
            .map_err(|echec| classee(format!("sa vague ne s'ouvre pas, {} : {}", echec.code, echec.preuve)))?;
        let (main, issue) = rendue.issues.into_iter().next().unwrap_or_else(|| {
            (format!("main-{}-1", rendue.id), Err("la vague du relecteur n'a rendu aucune issue".into()))
        });
        let lu = lu.into_inner().unwrap_or_else(PoisonError::into_inner);
        match (issue, lu) {
            (Ok(_), Some((remarques, texte))) => Ok(Lue { vague: rendue.id, main, remarques, texte }),
            (Ok(_), None) => Err(classee(format!("{main} : le juge du relecteur n'a gardé aucune remarque"))),
            (Err(dehors), _) => Err(classee(format!("{main} : {}", queue(&dehors)))),
        }
    }

    /// La correction des remarques bloquantes : une vague d'une tâche `correction`, partie de `t`. Assemblée en `t2`,
    /// la branche de la vague avance de `t` à `t2`, et la vague de la correction est rendue ; sinon le refus 7017.
    fn corriger(&self, remarques: &[Remarque]) -> Result<Menee, Refus> {
        let (id, tete) = (&self.menee.id, self.tete);
        let correction = plan_de_la_correction(self.profil, self.plan, &self.critiques, remarques);
        let manque = |preuve: String| Refus::new(Code::BloquantesRestantes, preuve, SANS_CORRECTION);
        let liens = Liens { corrige: Some(id), lecons: Some(&self.menee.lecons), ..Liens::default() };
        let profil = self.profil.pour_la_correction();
        let corrigee =
            lancer_avec(self.depot, self.dossier, &profil, &correction, self.bail, tete, liens).map_err(|echec| {
                manque(format!("la correction de la vague {id} ne s'ouvre pas, {} : {}", echec.code, echec.preuve))
            })?;
        let dehors = corrigee.issues.iter().find_map(|(_, issue)| issue.as_ref().err());
        if corrigee.arretee || dehors.is_some() {
            let dehors = dehors.map(|erreur| queue(erreur)).unwrap_or_default();
            return Err(manque(format!("la vague de correction {} n'a pas assemblé sa tâche : {dehors}", corrigee.id)));
        }
        let Some(nouvelle) = corrigee.tete.clone().filter(|nouvelle| nouvelle != tete) else {
            return Err(manque(format!("la vague de correction {} n'a rien changé à la tête {tete}", corrigee.id)));
        };
        if let Some(branche) = &self.menee.branche {
            let reference = format!("refs/heads/{branche}");
            if let Err(vu) = faire(geste(self.depot, &["update-ref", &reference, &nouvelle, tete])) {
                eprintln!("la branche locale {branche} n'avance pas : {vu}");
            }
        }
        Ok(corrigee)
    }

    /// Note le refus au fil, par un `forge.refus` de la vague relue, et le rend. Une note ratée s'écrit sur stderr.
    fn noter_le_refus(&self, refus: Refus) -> Refus {
        let Refus { code, preuve, correction } = &refus;
        let (regle, preuve, correction) = (code.regle().into(), preuve.clone(), correction.clone());
        let note =
            Evenement::RefusForge { vague: self.menee.id.clone(), code: code.numero(), regle, preuve, correction };
        if let Err(erreur) = Fil::seance(self.dossier, "humain").noter(&note) {
            eprintln!("le refus {code} ne se note pas au fil : {erreur}");
        }
        refus
    }
}

/// Le plan du relecteur : une vague de copies d'une seule tâche, `relecteur`, sans preuve de vague, de consigne
/// [`consigne`], de preuve prêtée [`PREUVE`], et de modèle la clé `relecteur` du profil à ses deux tentatives.
fn plan_du_relecteur(profil: &Profil, plan: &Plan, critiques: &[&Tache], points: &[String]) -> Plan {
    let modele = profil.relecteur().map(str::to_string);
    let tache = Tache {
        id: "relecteur".into(),
        preuve: Some(PREUVE.into()),
        objectif: consigne(&plan.objectif, critiques, points),
        modele: modele.clone(),
        relance: modele,
        ..Tache::default()
    };
    Plan { objectif: format!("Relire : {}", plan.objectif), preuve: None, taches: vec![tache] }
}

/// Le plan de la correction : une vague d'une seule tâche, `correction`, de preuve celle de la vague, d'objectif
/// [`objectif_de_la_correction`], de `fichiers` l'union de ceux des tâches critiques, et de modèle la clé `relance` du
/// profil à ses deux tentatives. Elle n'est pas critique : la correction n'a pas de seconde relecture.
fn plan_de_la_correction(profil: &Profil, plan: &Plan, critiques: &[&Tache], remarques: &[Remarque]) -> Plan {
    let relance = profil.relance().map(str::to_string);
    let objectif = objectif_de_la_correction(remarques);
    let tache = Tache {
        id: "correction".into(),
        objectif: objectif.clone(),
        modele: relance.clone(),
        relance,
        fichiers: fichiers_de(critiques),
        ..Tache::default()
    };
    Plan { objectif, preuve: plan.preuve.clone(), taches: vec![tache] }
}

/// L'objectif de la correction : [`CORRIGER`], puis chaque remarque bloquante, `<fichier>, ligne <n> : <texte>` (sans
/// `, ligne <n>` si elle n'a pas de ligne), les lignes de son texte qui suivent la première indentées, puis la ligne
/// fixe [`REPONDRE`].
fn objectif_de_la_correction(remarques: &[Remarque]) -> String {
    let mut objectif = CORRIGER.to_string();
    for remarque in remarques.iter().filter(|remarque| remarque.bloquante) {
        let lieu = match remarque.ligne {
            Some(ligne) => format!("{}, ligne {ligne}", remarque.fichier),
            None => remarque.fichier.clone(),
        };
        let mut lignes = remarque.texte.lines();
        objectif.push_str(&format!("\n{lieu} : {}", lignes.next().unwrap_or_default()));
        for suite in lignes {
            objectif.push_str(&format!("\n    {suite}"));
        }
    }
    objectif.push('\n');
    objectif.push_str(REPONDRE);
    objectif
}

#[cfg(test)]
mod tests;
