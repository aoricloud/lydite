//! La lecture de la ligne de commande : `Commande`, `Options`, l'usage et les drapeaux.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::coeur::erreur::{Code, Refus};
use crate::coeur::fil::NOMBRE_MAX;
use crate::conduite::mener::Options;

/// L'usage, que la correction d'un refus de la ligne de commande donne.
const USAGE: &str = concat!(
    "lydite lancer <profile> \"<objective>\" [--n N] [--verifier \"<command>\"] [--depot <path>]",
    " or lydite lancer <profile> --plan <file> [--depot <path>] [--pr <branch>]",
    " or lydite lancer <profile> \"<objective>\" --decouper --verifier \"<proof>\" [--observer] [--depot <path>]",
    " [--pr <branch>]",
    " or lydite lancer <profile> --issue <n> --decouper --verifier \"<proof>\" [--observer] [--depot <path>]",
    " [--pr <branch>]",
    " or lydite plan [--decoupe] <file>",
    " or lydite remarques <file>",
    " or lydite lecons <folder>",
    " or lydite accepter <profile>",
    " or lydite profil <profile>",
    " or lydite doctor [<profile>]",
    " or lydite etat",
    " or lydite attendre",
);

/// Les drapeaux de `lancer` qui prennent une valeur, chacun une fois : `--pr` et `--issue` n'y vont qu'avec
/// `--decouper`.
const DRAPEAUX: [&str; 5] = ["--n", "--verifier", "--depot", "--pr", "--issue"];

/// Les drapeaux de `lancer` qui ne prennent pas de valeur, chacun une fois : `--observer` n'y va qu'avec `--decouper`.
const DRAPEAUX_SANS_VALEUR: [&str; 2] = ["--decouper", "--observer"];

/// Les drapeaux de `lancer --plan`, chacun une fois avec sa valeur : les seuls qui vont avec lui.
const DRAPEAUX_PLAN: [&str; 3] = ["--plan", "--depot", "--pr"];

/// Ce que la ligne de commande demande.
#[derive(Debug, PartialEq)]
pub(crate) enum Commande {
    /// `lancer` : la vague d'un objectif, ou de ses copies.
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
}

/// Lit ce qui suit `lecons` : `<dossier>`, rien d'autre. Le dossier garde son `OsString`, et n'est pas un drapeau.
fn options_lire_lecons(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    let [dossier, suite @ ..] = args else {
        return Err(mal_formee("`lecons`: the repository folder is missing".into()));
    };
    if dossier.to_str().is_some_and(|nom| nom.starts_with('-')) {
        return Err(mal_formee(format!("`lecons` wants a repository folder, and {} is a flag", mot(dossier))));
    }
    if let Some(autre) = suite.first() {
        return Err(mal_formee(format!("`lecons` takes only `<folder>`, and {} follows it", mot(autre))));
    }
    Ok(Commande::LireLecons { dossier: PathBuf::from(dossier) })
}

/// Lit ce qui suit `remarques` : `<fichier>`, rien d'autre. Le fichier garde son `OsString`, et n'est pas un drapeau :
/// un mot en `-` à sa place est le refus 1006, comme un mot de plus.
fn options_lire_remarques(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    let [fichier, suite @ ..] = args else {
        return Err(mal_formee("`remarques`: the remarks file is missing".into()));
    };
    if fichier.to_str().is_some_and(|nom| nom.starts_with('-')) {
        return Err(mal_formee(format!("`remarques` wants the reviewer's file, and {} is a flag", mot(fichier))));
    }
    if let Some(autre) = suite.first() {
        return Err(mal_formee(format!("`remarques` takes only `<file>`, and {} follows it", mot(autre))));
    }
    Ok(Commande::LireRemarques { fichier: PathBuf::from(fichier) })
}

/// Lit ce qui suit `plan` : `[--decoupe] <fichier>`, rien d'autre. Le fichier garde son `OsString`, et n'est pas un
/// drapeau : un mot en `-` à sa place est le refus 1006, comme un mot de plus, car l'usage n'a pas d'autre drapeau.
fn options_lire_plan(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    let (decoupe, reste) = match args {
        [drapeau, reste @ ..] if drapeau == "--decoupe" => (true, reste),
        _ => (false, args),
    };
    let [fichier, suite @ ..] = reste else {
        return Err(mal_formee("`plan`: the plan file is missing".into()));
    };
    if fichier.to_str().is_some_and(|nom| nom.starts_with('-')) {
        return Err(mal_formee(format!("`plan` wants a plan file, and {} is a flag", mot(fichier))));
    }
    if let Some(autre) = suite.first() {
        return Err(mal_formee(format!("`plan` takes only `[--decoupe] <file>`, and {} follows it", mot(autre))));
    }
    Ok(Commande::LirePlan { fichier: PathBuf::from(fichier), decoupe })
}

/// Lit la ligne de commande : `lancer` et ses drapeaux, `lancer --plan`, `accepter`, `etat`, `plan` ou `remarques`.
/// `--pr` sans `--plan` ni `--decouper` est refusé (1006), et `--decouper` et `--plan` s'excluent. Une ligne sans
/// commande, ou de forme fausse, est refusée en 1006 ; un premier mot qui n'est pas une commande, non UTF-8 compris,
/// en 1005.
pub(crate) fn options(args: &[OsString]) -> Result<Commande, Refus> {
    let Some((commande, reste)) = args.split_first() else {
        return Err(refus_de_ligne(Code::CommandeMalFormee, "no command"));
    };
    match (commande.to_str(), reste) {
        (Some("etat"), []) => Ok(Commande::Etat),
        (Some("etat"), [autre, ..]) => {
            let preuve = format!("`etat` takes no argument, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("attendre"), []) => Ok(Commande::Attendre),
        (Some("attendre"), [autre, ..]) => {
            let preuve = format!("`attendre` takes no argument, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("plan"), reste) => options_lire_plan(reste),
        (Some("remarques"), reste) => options_lire_remarques(reste),
        (Some("lecons"), reste) => options_lire_lecons(reste),
        (Some("accepter"), [nom]) => Ok(Commande::Accepter(texte(nom, "the profile")?)),
        (Some("accepter"), []) => Err(refus_de_ligne(Code::CommandeMalFormee, "`accepter`: the profile is missing")),
        (Some("accepter"), [_, autre, ..]) => {
            let preuve = format!("`accepter` takes only the profile name, and {} follows it", mot(autre));
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("profil"), [nom]) => Ok(Commande::Profil(texte(nom, "the profile")?)),
        (Some("profil"), []) => Err(refus_de_ligne(Code::CommandeMalFormee, "`profil`: the profile is missing")),
        (Some("profil"), [_, autre, ..]) => {
            let preuve = format!("`profil` takes only the profile name, and {} follows it", mot(autre));
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
        (Some("lancer"), reste) if contient(reste, "--plan") && contient(reste, "--decouper") => {
            let preuve = "`lancer`: --decouper excludes --plan: the plan the wave plays is the one the split writes";
            Err(refus_de_ligne(Code::CommandeMalFormee, preuve))
        }
        (Some("lancer"), reste) if contient(reste, "--plan") => options_plan(reste),
        (Some("lancer"), reste) if contient(reste, "--pr") && !contient(reste, "--decouper") => {
            Err(pr_hors_de_sa_place())
        }
        (Some("lancer"), reste) => options_lancer(reste),
        _ => Err(refus_de_ligne(Code::CommandeInconnue, format!("{} is not a command", mot(commande)))),
    }
}

/// Lit ce qui suit `lancer` quand `--plan` y paraît : le profil, premier mot et pas un drapeau, puis
/// `--plan <fichier>`, `--depot <chemin>` et `--pr <branche>`, chacun une fois avec sa valeur, dans l'ordre
/// qu'on veut. Un chemin garde son `OsString`, et une valeur n'est jamais l'un des trois drapeaux. La branche de
/// `--pr` ne commence pas par `-` : git la prendrait pour une option. Tout autre mot (`--n`, `--verifier`, un
/// objectif) est le refus 1006, qui le nomme : un drapeau ignoré en silence ferait croire à n copies, ou à une
/// autre preuve.
fn options_plan(args: &[OsString]) -> Result<Commande, Refus> {
    let mal_formee = |preuve: String| refus_de_ligne(Code::CommandeMalFormee, preuve);
    let [profil, reste @ ..] = args else {
        return Err(mal_formee("`lancer`: the profile is missing".into()));
    };
    if profil.to_str().is_some_and(|nom| nom.starts_with('-')) {
        return Err(mal_formee(format!("`lancer`: the first word is the profile, and {} is a flag", mot(profil))));
    }
    let profil = texte(profil, "the profile")?;
    let drapeau = |arg: &OsStr| DRAPEAUX_PLAN.into_iter().find(|nom| arg == *nom);
    let (mut plan, mut depot, mut pr) = (None, None, None);
    let mut reste = reste.iter();
    while let Some(vu) = reste.next() {
        let Some(nom) = drapeau(vu) else {
            let preuve = format!(
                "with --plan, only --plan, --depot and --pr go after the profile, and {} is not one of them",
                mot(vu)
            );
            return Err(mal_formee(preuve));
        };
        let redit = match nom {
            "--plan" => plan.is_some(),
            "--depot" => depot.is_some(),
            _ => pr.is_some(),
        };
        if redit {
            return Err(mal_formee(format!("flag repeated: {nom}")));
        }
        let Some(valeur) = reste.next().filter(|valeur| drapeau(valeur).is_none()) else {
            return Err(mal_formee(format!("flag without a value: {nom}")));
        };
        match nom {
            "--plan" => plan = Some(PathBuf::from(valeur)),
            "--depot" => depot = Some(PathBuf::from(valeur)),
            _ => pr = Some(branche_de_pr(valeur)?),
        }
    }
    let Some(plan) = plan else {
        return Err(mal_formee("`lancer --plan`: `--plan <file>` is missing".into()));
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

/// Lit ce qui suit `lancer` : le profil, l'objectif, puis les drapeaux, chacun une fois avec sa valeur, sauf
/// `--decouper` et `--observer`, qui n'en prennent pas. Un drapeau à la place du profil ou de l'objectif est refusé
/// (1006) : `lancer claude --verifier x` ne lance pas une main sur l'objectif « --verifier ». `--n` va de 1 à
/// `NOMBRE_MAX`, la borne que le fil lit. `--verifier` refusé blanc (6014) : rejouée, une commande blanche
/// réussirait toujours et ferait prendre toute main pour prouvée. `--depot` garde son `OsString` : un chemin n'est
/// pas forcément de l'UTF-8. `--decouper` demande `--verifier` et un objectif d'une ligne, et exclut `--n` ;
/// `--observer` et `--pr` ne vont qu'avec lui. `--issue <n>`, un entier positif, tient lieu d'objectif : le profil
/// n'est suivi que des drapeaux, un objectif donné avec lui est refusé, et il ne va qu'avec `--decouper` (1006).
fn options_lancer(args: &[OsString]) -> Result<Commande, Refus> {
    let manquant = |manque: &str| refus_de_ligne(Code::CommandeMalFormee, format!("`lancer`: {manque}"));
    let est_drapeau =
        |arg: &OsStr| arg.to_str().is_some_and(|mot| DRAPEAUX.contains(&mot) || DRAPEAUX_SANS_VALEUR.contains(&mot));
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
                "`lancer`: --issue replaces the objective, which is not given with it, and {} is one",
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
    let mut vus: Vec<&str> = Vec::new();
    let mut reste = reste.iter();
    while let Some(drapeau) = reste.next() {
        let Some(nom) = drapeau.to_str().filter(|nom| DRAPEAUX.contains(nom) || DRAPEAUX_SANS_VALEUR.contains(nom))
        else {
            return Err(refus_de_ligne(Code::CommandeMalFormee, format!("unknown flag: {}", mot(drapeau))));
        };
        if vus.contains(&nom) {
            return Err(refus_de_ligne(Code::CommandeMalFormee, format!("flag repeated: {nom}")));
        }
        vus.push(nom);
        match nom {
            "--decouper" => {
                o.decouper = true;
                continue;
            }
            "--observer" => {
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
            "--verifier" => {
                let commande = texte(valeur, "the command of --verifier")?;
                if commande.trim().is_empty() {
                    return Err(Refus::new(
                        Code::PreuveVide,
                        format!("--verifier is {commande:?}, a blank command"),
                        "Give --verifier a command that is not blank, the one Lydite replays in each worktree, or \
                         drop --verifier: the work then rests only on the hand's PREUVE: line, and stays declared.",
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
        return Err(mal_formee("`lancer`: --observer goes only with --decouper"));
    }
    if o.pr.is_some() && !o.decouper {
        return Err(pr_hors_de_sa_place());
    }
    if o.issue.is_some() && !o.decouper {
        return Err(mal_formee(
            "`lancer`: --issue goes only with --decouper: the issue is split into a plan, which the wave plays",
        ));
    }
    if par_issue && o.issue.is_none() {
        return Err(manquant("the objective, or `--issue <n>`, is missing"));
    }
    if o.decouper && vus.contains(&"--n") {
        return Err(mal_formee(
            "`lancer`: --decouper excludes --n: the split plan sets its own tasks, hence its hands",
        ));
    }
    if o.decouper && o.verifier.is_none() {
        return Err(mal_formee(
            "`lancer`: --decouper requires --verifier \"<proof>\", the wave's proof, which only the human gives",
        ));
    }
    // L'en-tête du plan, `objectif : <une ligne>`, n'en tient qu'une.
    if o.decouper && o.objectif.contains(['\n', '\r']) {
        return Err(mal_formee(&format!("`lancer`: --decouper wants a one-line objective, not {:?}", o.objectif)));
    }
    Ok(Commande::Lancer(o))
}

fn pr_hors_de_sa_place() -> Refus {
    refus_de_ligne(Code::CommandeMalFormee, "`lancer`: --pr goes only with --plan <file> or --decouper")
}

fn contient(args: &[OsString], drapeau: &str) -> bool {
    args.iter().any(|arg| arg == drapeau)
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
