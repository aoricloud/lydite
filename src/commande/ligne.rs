//! La lecture de la ligne de commande : `Commande`, `Options`, l'usage et les drapeaux. Chaque mot a un double
//! anglais, lu exactement comme lui ; le mot français reste lu, et tout ce que Lydite écrit d'ici nomme le mot anglais.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::coeur::erreur::{Code, Refus};
use crate::coeur::fil::NOMBRE_MAX;
use crate::conduite::mener::Options;

/// L'usage, que la correction d'un refus de la ligne de commande donne. Il ne montre que les mots anglais : chacun a
/// un double français, encore lu, que `ALIAS_COMMANDES` et `ALIAS_DRAPEAUX` disent.
const USAGE: &str = concat!(
    "lydite launch <profile> \"<objective>\" [--n N] [--verify \"<command>\"] [--repo <path>]",
    " or lydite launch <profile> --plan <file> [--repo <path>] [--pr <branch>]",
    " or lydite launch <profile> \"<objective>\" --split --verify \"<proof>\" [--dry-run] [--repo <path>]",
    " [--pr <branch>]",
    " or lydite launch <profile> --issue <n> --split --verify \"<proof>\" [--dry-run] [--repo <path>]",
    " [--pr <branch>]",
    " or lydite plan [--split] <file>",
    " or lydite remarks <file>",
    " or lydite lessons <folder>",
    " or lydite accept <profile>",
    " or lydite profile <profile>",
    " or lydite doctor [<profile>]",
    " or lydite status",
    " or lydite wait",
    " or lydite --version",
    " or lydite memory",
    " or lydite memory recall <address>",
    " or lydite skill",
);

/// Le double français de chaque commande, qui a son mot anglais : `(français, anglais)`.
const ALIAS_COMMANDES: [(&str, &str); 7] = [
    ("lancer", "launch"),
    ("etat", "status"),
    ("attendre", "wait"),
    ("accepter", "accept"),
    ("profil", "profile"),
    ("remarques", "remarks"),
    ("lecons", "lessons"),
];

/// Le double français de chaque drapeau de `launch`, qui a son mot anglais : `(français, anglais)`.
const ALIAS_DRAPEAUX: [(&str, &str); 4] =
    [("--verifier", "--verify"), ("--depot", "--repo"), ("--decouper", "--split"), ("--observer", "--dry-run")];

/// Les drapeaux de `launch` qui prennent une valeur, chacun une fois : `--pr` et `--issue` n'y vont qu'avec
/// `--split`. Ce sont les mots anglais, que `anglais` rend de leur double français.
const DRAPEAUX: [&str; 5] = ["--n", "--verify", "--repo", "--pr", "--issue"];

/// Les drapeaux de `launch` qui ne prennent pas de valeur, chacun une fois : `--dry-run` n'y va qu'avec `--split`.
const DRAPEAUX_SANS_VALEUR: [&str; 2] = ["--split", "--dry-run"];

/// Les drapeaux de `launch --plan`, chacun une fois avec sa valeur : les seuls qui vont avec lui.
const DRAPEAUX_PLAN: [&str; 3] = ["--plan", "--repo", "--pr"];

/// Le mot anglais d'un mot que l'humain a écrit : lui-même s'il est déjà anglais, ou n'a pas de double.
fn anglais<'a>(mot: &'a str, alias: &[(&str, &'static str)]) -> &'a str {
    alias.iter().find(|(francais, _)| *francais == mot).map_or(mot, |(_, anglais)| *anglais)
}

/// Le texte d'un drapeau redit : le mot anglais, et les deux mots vus quand l'humain a mêlé les deux langues.
fn drapeau_redit(nom: &str, avant: &str, vu: &str) -> String {
    if avant == vu {
        format!("flag repeated: {nom}")
    } else {
        format!("flag repeated: {nom}, seen as « {avant} » and « {vu} »")
    }
}

/// Le mot anglais d'un drapeau de `launch` : son double français s'il en a un.
fn drapeau_anglais(arg: &OsStr) -> Option<&str> {
    arg.to_str().map(|mot| anglais(mot, &ALIAS_DRAPEAUX))
}

/// Ce que la ligne de commande demande.
#[derive(Debug, PartialEq)]
pub(crate) enum Commande {
    /// `launch` : la vague d'un objectif, ou de ses copies.
    Lancer(Options),
    /// La vague d'un plan. Les deux chemins gardent leur `OsString` ; `pr` est la branche du distant que `--pr` nomme,
    /// base de la PR de la vague.
    Plan { profil: String, plan: PathBuf, depot: Option<PathBuf>, pr: Option<String> },
    /// Le nom du profil dont le fichier s'accepte.
    Accepter(String),
    /// Le nom du profil que `lydite profil` dit, en lecture seule.
    Profil(String),
    /// `lydite doctor` : ce qui manque pour lancer une vague ; avec un profil, ce profil et son mur aussi.
    Doctor(Option<String>),
    /// La dernière vague du fil.
    Etat,
    /// `lydite --version` : la version du binaire, du `CARGO_PKG_VERSION` du `Cargo.toml`.
    Version,
    /// Attend la fin du lancement qui tient le dossier, puis dit la dernière vague comme `Etat`.
    Attendre,
    /// Le plan du fichier, lu comme `--plan` le lit, ou en plan découpé, sans rien lancer. Le chemin garde son
    /// `OsString`.
    LirePlan { fichier: PathBuf, decoupe: bool },
    /// Le fichier de remarques, lu comme Kraken lira celui du relecteur, sans rien lancer. Le chemin garde son
    /// `OsString`.
    LireRemarques { fichier: PathBuf },
    /// Les leçons `.agents/skills/*/SKILL.md` du dépôt de ce dossier, lues sans rien lancer. Le chemin garde son
    /// `OsString`.
    LireLecons { dossier: PathBuf },
    /// `lydite memory` : l'index du fil, une ligne par ligne lue, en lecture seule.
    Memoire,
    /// `lydite memory recall <address>` : la ligne du fil que l'adresse ou l'empreinte nomme, en lecture seule.
    Rappel(String),
    /// `lydite skill` : écrit la skill de pilotage que ce binaire porte, sans rien lire ni écrire d'autre.
    Skill,
    /// `lydite --help` ou `lydite -h`, seul : écrit l'usage sur stdout, sans rien lire ni écrire d'autre.
    Aide,
}

/// Si le mot est `--help` ou `-h`, mot entier.
fn est_aide(arg: &OsStr) -> bool {
    arg == "--help" || arg == "-h"
}

/// L'usage que `lydite --help` écrit : `Usage:`, puis chaque forme de `USAGE` sur sa ligne, après deux espaces. Un
/// morceau qui continue une forme (` [--pr <branch>]`) reste sur la ligne de sa forme.
pub(crate) fn aide() -> String {
    let formes: Vec<String> = USAGE.split(" or ").map(|forme| format!("  {forme}\n")).collect();
    format!("Usage:\n{}", formes.concat())
}

/// Lit ce qui suit `memory` : rien, ou `recall <address>`. Tout autre mot, un mot de plus, une adresse qui manque ou
/// qui n'est pas de l'UTF-8 est le refus 1006, qui nomme le mot vu. Ces mots n'ont pas de double français.
fn options_memoire(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    match args {
        [] => Ok(Commande::Memoire),
        [rappel, adresse] if rappel == "recall" => Ok(Commande::Rappel(texte(adresse, "the address")?)),
        [rappel] if rappel == "recall" => Err(mal_formee("`memory recall`: the address is missing".into())),
        [rappel, _, autre, ..] if rappel == "recall" => {
            Err(mal_formee(format!("`memory recall` takes only `<address>`, and {} follows it", mot(autre))))
        }
        [autre, ..] => {
            Err(mal_formee(format!("`memory` takes no argument but `recall <address>`, and {} follows it", mot(autre))))
        }
    }
}

/// Lit ce qui suit `lessons` : `<dossier>`, rien d'autre. Le dossier garde son `OsString`, et n'est pas un drapeau.
fn options_lire_lecons(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    let [dossier, suite @ ..] = args else {
        return Err(mal_formee("`lessons`: the repository folder is missing".into()));
    };
    if dossier.to_str().is_some_and(|nom| nom.starts_with('-')) {
        return Err(mal_formee(format!("`lessons` wants a repository folder, and {} is a flag", mot(dossier))));
    }
    if let Some(autre) = suite.first() {
        return Err(mal_formee(format!("`lessons` takes only `<folder>`, and {} follows it", mot(autre))));
    }
    Ok(Commande::LireLecons { dossier: PathBuf::from(dossier) })
}

/// Lit ce qui suit `remarks` : `<fichier>`, rien d'autre. Le fichier garde son `OsString`, et n'est pas un drapeau :
/// un mot en `-` à sa place est le refus 1006, comme un mot de plus.
fn options_lire_remarques(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    let [fichier, suite @ ..] = args else {
        return Err(mal_formee("`remarks`: the remarks file is missing".into()));
    };
    if fichier.to_str().is_some_and(|nom| nom.starts_with('-')) {
        return Err(mal_formee(format!("`remarks` wants the reviewer's file, and {} is a flag", mot(fichier))));
    }
    if let Some(autre) = suite.first() {
        return Err(mal_formee(format!("`remarks` takes only `<file>`, and {} follows it", mot(autre))));
    }
    Ok(Commande::LireRemarques { fichier: PathBuf::from(fichier) })
}

/// Lit ce qui suit `plan` : `[--split] <fichier>`, rien d'autre, `--decoupe` étant le double français de `--split`.
/// Le fichier garde son `OsString`, et n'est pas un drapeau : un mot en `-` à sa place est le refus 1006, comme un mot
/// de plus, car l'usage n'a pas d'autre drapeau.
fn options_lire_plan(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    let (decoupe, reste) = match args {
        [drapeau, reste @ ..] if drapeau == "--split" || drapeau == "--decoupe" => (true, reste),
        _ => (false, args),
    };
    let [fichier, suite @ ..] = reste else {
        return Err(mal_formee("`plan`: the plan file is missing".into()));
    };
    if fichier.to_str().is_some_and(|nom| nom.starts_with('-')) {
        return Err(mal_formee(format!("`plan` wants a plan file, and {} is a flag", mot(fichier))));
    }
    if let Some(autre) = suite.first() {
        return Err(mal_formee(format!("`plan` takes only `[--split] <file>`, and {} follows it", mot(autre))));
    }
    Ok(Commande::LirePlan { fichier: PathBuf::from(fichier), decoupe })
}

/// Lit la ligne de commande : `launch` et ses drapeaux, `launch --plan`, `accept`, `status`, `plan` ou `remarks`,
/// chacun lu aussi par son double français. `--pr` sans `--plan` ni `--split` est refusé (1006), et `--split` et
/// `--plan` s'excluent. Une ligne sans commande, ou de forme fausse, est refusée en 1006 ; un premier mot qui n'est
/// pas une commande, non UTF-8 compris, en 1005.
pub(crate) fn options(args: &[OsString]) -> Result<Commande, Refus> {
    let Some((commande, reste)) = args.split_first() else {
        return Err(refus_de_ligne(Code::CommandeMalFormee, "no command"));
    };
    // `--help` et `-h` ne se lisent que seuls, avant toute autre lecture : ailleurs, ils seraient un mot de la
    // commande.
    if est_aide(commande) {
        return match reste.first() {
            None => Ok(Commande::Aide),
            Some(autre) => {
                let preuve = format!("{} takes no argument, and {} follows it", mot(commande), mot(autre));
                Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
            }
        };
    }
    if let Some(aide) = reste.iter().find(|arg| est_aide(arg)) {
        let preuve = format!("{} is read only alone: type `lydite --help` by itself", mot(aide));
        return Err(refus_de_ligne(Code::CommandeMalFormee, preuve));
    }
    match (commande.to_str().map(|mot| anglais(mot, &ALIAS_COMMANDES)), reste) {
        (Some("status"), []) => Ok(Commande::Etat),
        (Some("--version"), []) | (Some("-V"), []) => Ok(Commande::Version),
        (Some("status"), [autre, ..]) => {
            let preuve = format!("`status` takes no argument, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("wait"), []) => Ok(Commande::Attendre),
        (Some("wait"), [autre, ..]) => {
            let preuve = format!("`wait` takes no argument, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("skill"), []) => Ok(Commande::Skill),
        (Some("skill"), [autre, ..]) => {
            let preuve = format!("`skill` takes no argument, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("memory"), reste) => options_memoire(reste),
        (Some("plan"), reste) => options_lire_plan(reste),
        (Some("remarks"), reste) => options_lire_remarques(reste),
        (Some("lessons"), reste) => options_lire_lecons(reste),
        (Some("accept"), [nom]) => Ok(Commande::Accepter(texte(nom, "the profile")?)),
        (Some("accept"), []) => Err(refus_de_ligne(Code::CommandeMalFormee, "`accept`: the profile is missing")),
        (Some("accept"), [_, autre, ..]) => {
            let preuve = format!("`accept` takes only the profile name, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("profile"), [nom]) => Ok(Commande::Profil(texte(nom, "the profile")?)),
        (Some("profile"), []) => Err(refus_de_ligne(Code::CommandeMalFormee, "`profile`: the profile is missing")),
        (Some("profile"), [_, autre, ..]) => {
            let preuve = format!("`profile` takes only the profile name, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("doctor"), []) => Ok(Commande::Doctor(None)),
        (Some("doctor"), [nom]) if nom.to_str().is_some_and(|nom| nom.starts_with('-')) => {
            let preuve = format!("`doctor` wants a profile name, and {} is a flag", mot(nom));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("doctor"), [nom]) => Ok(Commande::Doctor(Some(texte(nom, "the profile")?))),
        (Some("doctor"), [_, autre, ..]) => {
            let preuve = format!("`doctor` takes only a profile name, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("launch"), reste) if contient(reste, "--plan") && contient(reste, "--split") => {
            let preuve = "`launch`: --split excludes --plan: the plan the wave plays is the one the split writes";
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("launch"), reste) if contient(reste, "--plan") => options_plan(reste),
        (Some("launch"), reste) if contient(reste, "--pr") && !contient(reste, "--split") => Err(pr_hors_de_sa_place()),
        (Some("launch"), reste) => options_lancer(reste),
        _ => Err(refus_de_ligne(Code::CommandeInconnue, format!("{} is not a command", mot(commande)))),
    }
}

/// Lit ce qui suit `launch` quand `--plan` y paraît : le profil, premier mot et pas un drapeau, puis
/// `--plan <fichier>`, `--repo <chemin>` et `--pr <branche>`, chacun une fois avec sa valeur, dans l'ordre
/// qu'on veut. Un chemin garde son `OsString`, et une valeur n'est jamais l'un des trois drapeaux. La branche de
/// `--pr` ne commence pas par `-` : git la prendrait pour une option. Tout autre mot (`--n`, `--verify`, un
/// objectif) est le refus 1006, qui le nomme : un drapeau ignoré en silence ferait croire à n copies, ou à une
/// autre preuve.
fn options_plan(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    let [profil, reste @ ..] = args else {
        return Err(mal_formee("`launch`: the profile is missing".into()));
    };
    if profil.to_str().is_some_and(|nom| nom.starts_with('-')) {
        return Err(mal_formee(format!("`launch`: the first word is the profile, and {} is a flag", mot(profil))));
    }
    let profil = texte(profil, "the profile")?;
    let drapeau = |arg: &OsStr| {
        let anglais = drapeau_anglais(arg)?;
        DRAPEAUX_PLAN.into_iter().find(|nom| *nom == anglais)
    };
    let (mut plan, mut depot, mut pr) = (None, None, None);
    // Les drapeaux vus : le mot anglais, et le mot que l'humain a écrit.
    let mut vus: Vec<(&str, &str)> = Vec::new();
    let mut reste = reste.iter();
    while let Some(vu) = reste.next() {
        let (Some(nom), Some(tape)) = (drapeau(vu), vu.to_str()) else {
            let preuve = format!(
                "with --plan, only --plan, --repo and --pr go after the profile, and {} is not one of them",
                mot(vu)
            );
            return Err(mal_formee(preuve));
        };
        if let Some((_, avant)) = vus.iter().find(|(deja, _)| *deja == nom) {
            return Err(mal_formee(drapeau_redit(nom, avant, tape)));
        }
        vus.push((nom, tape));
        let Some(valeur) = reste.next().filter(|valeur| drapeau(valeur).is_none()) else {
            return Err(mal_formee(format!("flag without a value: {nom}")));
        };
        match nom {
            "--plan" => plan = Some(PathBuf::from(valeur)),
            "--repo" => depot = Some(PathBuf::from(valeur)),
            _ => pr = Some(branche_de_pr(valeur)?),
        }
    }
    let Some(plan) = plan else {
        return Err(mal_formee("`launch --plan`: `--plan <file>` is missing".into()));
    };
    Ok(Commande::Plan { profil, plan, depot, pr })
}

/// La branche que `--pr` nomme : un texte UTF-8 qui ne commence pas par `-`, car git la lirait comme une option.
fn branche_de_pr(valeur: &OsStr) -> Result<String, Refus> {
    let branche = texte(valeur, "the branch of --pr")?;
    if branche.starts_with('-') {
        let preuve = format!("--pr wants a branch name, which does not start with `-`, not {}", mot(valeur));
        return Err(refus_de_ligne(Code::CommandeMalFormee, preuve));
    }
    Ok(branche)
}

/// Lit ce qui suit `launch` : le profil, l'objectif, puis les drapeaux, chacun une fois avec sa valeur, sauf
/// `--split` et `--dry-run`, qui n'en prennent pas. Un drapeau à la place du profil ou de l'objectif est refusé
/// (1006) : `launch claude --verify x` ne lance pas une main sur l'objectif « --verify ». `--n` va de 1 à
/// `NOMBRE_MAX`, la borne que le fil lit. `--verify` refusé blanc (6014) : rejouée, une commande blanche
/// réussirait toujours et ferait prendre toute main pour prouvée. `--repo` garde son `OsString` : un chemin n'est
/// pas forcément de l'UTF-8. `--split` demande `--verify` et un objectif d'une ligne, et exclut `--n` ;
/// `--dry-run` et `--pr` ne vont qu'avec lui. `--issue <n>`, un entier positif, tient lieu d'objectif : le profil
/// n'est suivi que des drapeaux, un objectif donné avec lui est refusé, et il ne va qu'avec `--split` (1006).
fn options_lancer(args: &[OsString]) -> Result<Commande, Refus> {
    let manquant = |manque: &str| refus_de_ligne(Code::CommandeMalFormee, format!("`launch`: {manque}"));
    let est_drapeau = |arg: &OsStr| drapeau_anglais(arg).is_some_and(drapeau_connu);
    let par_issue = contient(args, "--issue");
    let (profil, objectif, reste) = if par_issue {
        let [profil, reste @ ..] = args else {
            return Err(manquant("the profile is missing"));
        };
        if est_drapeau(profil) {
            return Err(manquant("the profile is missing"));
        }
        if let Some(objectif) = reste.first().filter(|arg| !est_drapeau(arg)) {
            let preuve = format!(
                "`launch`: --issue replaces the objective, which is not given with it, and {} is one",
                mot(objectif)
            );
            return Err(refus_de_ligne(Code::CommandeMalFormee, preuve));
        }
        (profil, String::new(), reste)
    } else {
        let [profil, objectif, reste @ ..] = args else {
            return Err(manquant(if args.is_empty() {
                "the profile and the objective are missing"
            } else {
                "the objective is missing"
            }));
        };
        for (arg, manque) in
            [(profil, "the profile and the objective are missing"), (objectif, "the objective is missing")]
        {
            if est_drapeau(arg) {
                return Err(manquant(manque));
            }
        }
        (profil, texte(objectif, "the objective")?, reste)
    };
    let mut o = Options {
        profil: texte(profil, "the profile")?,
        objectif,
        n: 1,
        verifier: None,
        depot: None,
        decouper: false,
        observer: false,
        pr: None,
        issue: None,
    };
    // Les drapeaux vus : le mot anglais, et le mot que l'humain a écrit.
    let mut vus: Vec<(&str, &str)> = Vec::new();
    let mut reste = reste.iter();
    while let Some(drapeau) = reste.next() {
        // Un mot qui n'est pas de l'UTF-8 n'est aucun drapeau : le texte vide n'en est pas un.
        let tape = drapeau.to_str().unwrap_or_default();
        let nom = anglais(tape, &ALIAS_DRAPEAUX);
        if !drapeau_connu(nom) {
            return Err(refus_de_ligne(Code::CommandeMalFormee, format!("unknown flag: {}", mot(drapeau))));
        }
        if let Some((_, avant)) = vus.iter().find(|(deja, _)| *deja == nom) {
            return Err(refus_de_ligne(Code::CommandeMalFormee, drapeau_redit(nom, avant, tape)));
        }
        vus.push((nom, tape));
        match nom {
            "--split" => {
                o.decouper = true;
                continue;
            }
            "--dry-run" => {
                o.observer = true;
                continue;
            }
            _ => {}
        }
        let Some(valeur) = reste.next() else {
            return Err(refus_de_ligne(Code::CommandeMalFormee, format!("flag without a value: {nom}")));
        };
        match nom {
            "--n" => {
                let n = valeur.to_str().and_then(|n| n.parse().ok()).filter(|n| (1..=NOMBRE_MAX).contains(n));
                o.n = n.ok_or_else(|| {
                    let preuve = format!("--n wants an integer from 1 to {NOMBRE_MAX}, not {}", mot(valeur));
                    refus_de_ligne(Code::CommandeMalFormee, preuve)
                })?;
            }
            "--verify" => {
                let commande = texte(valeur, "the command of --verify")?;
                if commande.trim().is_empty() {
                    return Err(Refus::new(
                        Code::PreuveVide,
                        format!("--verify is {commande:?}, a blank command"),
                        "Give --verify a command that is not blank, the one Lydite replays in each worktree, or \
                         drop --verify: the work then rests only on the hand's PREUVE: line, and stays declared.",
                    ));
                }
                o.verifier = Some(commande);
            }
            "--pr" => o.pr = Some(branche_de_pr(valeur)?),
            "--issue" => {
                let n = valeur.to_str().and_then(|n| n.parse::<u64>().ok()).filter(|n| *n > 0);
                o.issue = Some(n.ok_or_else(|| {
                    let preuve = format!("--issue wants an issue number, a positive integer, not {}", mot(valeur));
                    refus_de_ligne(Code::CommandeMalFormee, preuve)
                })?);
            }
            _ => o.depot = Some(valeur.into()),
        }
    }
    let mal_formee = |preuve: &str| refus_de_ligne(Code::CommandeMalFormee, preuve);
    if o.observer && !o.decouper {
        return Err(mal_formee("`launch`: --dry-run goes only with --split"));
    }
    if o.pr.is_some() && !o.decouper {
        return Err(pr_hors_de_sa_place());
    }
    if o.issue.is_some() && !o.decouper {
        return Err(mal_formee(
            "`launch`: --issue goes only with --split: the issue is split into a plan, which the wave plays",
        ));
    }
    if par_issue && o.issue.is_none() {
        return Err(manquant("the objective, or `--issue <n>`, is missing"));
    }
    if o.decouper && vus.iter().any(|(nom, _)| *nom == "--n") {
        return Err(mal_formee("`launch`: --split excludes --n: the split plan sets its own tasks, hence its hands"));
    }
    if o.decouper && o.verifier.is_none() {
        return Err(mal_formee(
            "`launch`: --split requires --verify \"<proof>\", the wave's proof, which only the human gives",
        ));
    }
    // L'en-tête du plan, `objectif : <une ligne>`, n'en tient qu'une.
    if o.decouper && o.objectif.contains(['\n', '\r']) {
        return Err(mal_formee(&format!("`launch`: --split wants a one-line objective, not {:?}", o.objectif)));
    }
    Ok(Commande::Lancer(o))
}

/// Si le mot anglais est un drapeau de `launch` que `options_lancer` lit.
fn drapeau_connu(nom: &str) -> bool {
    DRAPEAUX.contains(&nom) || DRAPEAUX_SANS_VALEUR.contains(&nom)
}

fn pr_hors_de_sa_place() -> Refus {
    refus_de_ligne(Code::CommandeMalFormee, "`launch`: --pr goes only with --plan <file> or --split")
}

/// Si un des mots paraît, sous son nom anglais ou sous son double français.
fn contient(args: &[OsString], drapeau: &str) -> bool {
    args.iter().any(|arg| drapeau_anglais(arg) == Some(drapeau))
}

fn refus_de_ligne(code: Code, preuve: impl Into<String>) -> Refus {
    Refus::new(code, preuve, format!("Write the command as the usage shows: {USAGE}."))
}

fn mot(arg: &OsStr) -> String {
    arg.to_str().map_or_else(|| format!("{arg:?}"), |texte| format!("« {texte} »"))
}

fn texte(arg: &OsStr, quoi: &str) -> Result<String, Refus> {
    arg.to_str()
        .map(str::to_string)
        .ok_or_else(|| refus_de_ligne(Code::CommandeMalFormee, format!("{quoi} must be UTF-8 text, not {}", mot(arg))))
}

#[cfg(test)]
mod tests;
