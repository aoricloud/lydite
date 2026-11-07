//! Comment la forge lance ses commandes : git par `git::geste`, avec en plus `GIT_NO_REPLACE_OBJECTS=1` (ce que Kraken
//! lit est ce qu'il pousse), `GIT_TERMINAL_PROMPT=0` et `GCM_INTERACTIVE=never` ; `gh` ([`programme_gh`]) avec
//! `--repo <h/p/d>`, sauf `gh auth status`, qui n'a pas ce drapeau et prend `--hostname <h>`, lancé dans le dossier de
//! Kraken et jamais dans le dépôt visé, avec `GH_PROMPT_DISABLED=1` et l'environnement de Kraken (ses identifiants).
//! Chaque commande écrit sa sortie et ses erreurs dans deux fichiers du dossier de Kraken, effacés une fois lus, et a
//! son délai ([`DELAI`]) : au-delà, son arbre meurt, et c'est le refus de son étape.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::branchements::git::{IDENTITE, geste};
use crate::branchements::processus::{SORTIE, attendre, echeance, tuer_arbre};
use crate::coeur::erreur::{Code, Refus, nommee, queue};
use crate::coeur::fil::{Couleur, lot};
use crate::coeur::json::{self, Json};

/// La lecture de la CI : un tour tout de suite, puis toutes les 15 s, 30 min au plus.
pub(crate) const ATTENTE_DE_LA_CI: (Duration, Duration) = (Duration::from_secs(15), Duration::from_secs(30 * 60));

/// Le délai de chaque commande de la forge.
// raccourci : un ssh qui attendrait une saisie n'est borné que par ce délai ; monter (BatchMode) si un fetch ou un push
// ssh se montre lent à échouer
pub(crate) const DELAI: Duration = Duration::from_secs(120);

/// Ce que la forge pose sur chacun de ses gestes git, en plus de `git::geste` : git lit les objets tels qu'ils sont,
/// sans les remplacements que le dépôt déclare, et ne demande rien à personne.
pub(crate) const GIT_FORGE: [(&str, &str); 3] =
    [("GIT_NO_REPLACE_OBJECTS", "1"), ("GIT_TERMINAL_PROMPT", "0"), ("GCM_INTERACTIVE", "never")];

/// L'issue d'un check en échec, en majuscules : la conclusion d'un run, ou l'état d'un statut.
const ECHECS: [&str; 6] = ["FAILURE", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STARTUP_FAILURE", "ERROR"];

/// Les runs que `gh run list --limit` rend d'un commit au plus, et les statuts que l'api en rend sur une page
/// (`per_page`) : 100. Une liste de cette taille est peut-être coupée, et un vert ne se prouve pas sur elle.
const COUPEE: usize = 100;

/// Les champs d'un run que `gh run list --json` rend, et que la CI lit.
pub(crate) const CHAMPS_DES_RUNS: &str = "workflowName,name,status,conclusion,startedAt,createdAt";

/// La correction du refus 8005.
const PUBLIER: &str =
    "Rien n'est poussé, et la branche de la vague reste locale : relis-la, puis pousse-la et ouvre sa PR toi-même.";

/// La correction du refus 8009.
const REINTEGRER: &str = "La PR reste ouverte, et Kraken ne relance pas sa CI : réintègre la base dans la \
                          branche de la PR, puis relance-la toi-même.";

/// La correction du refus 8010.
const CHANGEE: &str = "Kraken s'arrête et ne touche plus à la PR : regarde qui l'a fermée, fusionnée ou poussée.";

/// Le programme `gh` de la forge : celui que nomme `KRAKEN_GH`, sinon `gh`. Le seul endroit où Kraken lit `KRAKEN_GH`.
pub(crate) fn programme_gh() -> OsString {
    env::var_os("KRAKEN_GH").unwrap_or_else(|| "gh".into())
}

/// Le distant `origin` du dépôt visé, lu une fois, avant la vague : Kraken s'en sert pour toute la suite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Distant {
    /// L'URL que le dépôt écrit pour `origin`, où git va chercher et pousse : ses `insteadOf` s'y appliquent, comme à
    /// `origin`.
    pub(crate) url: String,
    /// L'hôte, que `gh auth status --hostname` prend.
    pub(crate) hote: String,
    /// `<hôte>/<propriétaire>/<dépôt>`, que `gh --repo` prend.
    pub(crate) depot: String,
}

/// Le distant `origin` du dépôt visé (8001) : la première URL que le dépôt écrit pour lui (`git config --get-all
/// remote.origin.url`), lue en hôte, propriétaire et dépôt par [`analyser`]. `git remote get-url` rendrait l'URL après
/// ses `insteadOf` : un distant que l'humain ne nomme pas, local dans les essais.
pub(crate) fn distant(dossier: &Path, depot: &Path) -> Result<Distant, Refus> {
    let refus = |preuve: String| {
        Refus::new(
            Code::DistantIllisible,
            preuve,
            "Donne au dépôt visé un distant origin qui nomme sa forge (git remote add origin \
             https://<hôte>/<propriétaire>/<dépôt>), ou lance la vague sans --pr.",
        )
    };
    let lue = jouer(git_forge(depot, &["config", "--get-all", "remote.origin.url"]), dossier, None, DELAI)
        .and_then(Rendu::reussi)
        .map_err(|vu| refus(format!("le dépôt {} n'a pas de distant origin : {vu}", depot.display())))?;
    let url = lue.lines().next().unwrap_or_default().trim();
    analyser(url).ok_or_else(|| {
        // Une URL https qui porte des identifiants ne se recopie pas dans la preuve, que stderr reçoit.
        let identifiants =
            url.strip_prefix("https://").is_some_and(|reste| reste.split('/').next().is_some_and(|a| a.contains('@')));
        let vue =
            if identifiants { "une URL https qui porte des identifiants".to_string() } else { format!("« {url} »") };
        refus(format!(
            "l'URL d'origin, {vue}, ne nomme pas un hôte, un propriétaire et un dépôt : Kraken lit https://h/p/d, \
             ssh://[u@]h[:port]/p/d et [u@]h:p/d, chacune avec ou sans .git"
        ))
    })
}

/// L'hôte, le propriétaire et le dépôt d'une URL de distant, dans l'une de ses trois formes : `https://h/p/d`,
/// `ssh://[u@]h[:port]/p/d` et `[u@]h:p/d` (celle de scp), chacune avec ou sans `.git`. `None` pour une URL locale (un
/// chemin, `file://`, une lettre de lecteur), un autre schéma, des identifiants dans une URL https, un hôte ou un
/// utilisateur qui n'est pas un nom simple (une option de ssh en serait une), ou un chemin qui n'a pas deux noms.
fn analyser(url: &str) -> Option<Distant> {
    let (hote, chemin) = if let Some(reste) = url.strip_prefix("https://") {
        reste.split_once('/')?
    } else if let Some(reste) = url.strip_prefix("ssh://") {
        let (autorite, chemin) = reste.split_once('/')?;
        let hote = sans_utilisateur(autorite)?;
        let hote = match hote.split_once(':') {
            Some((hote, port)) if !port.is_empty() && port.bytes().all(|c| c.is_ascii_digit()) => hote,
            Some(_) => return None,
            None => hote,
        };
        (hote, chemin)
    } else if !url.contains("://") {
        // La forme de scp : un `/` avant le premier `:` en fait un chemin du poste, pour git comme pour Kraken.
        let (avant, chemin) = url.split_once(':')?;
        if avant.contains('/') {
            return None;
        }
        (sans_utilisateur(avant)?, chemin)
    } else {
        return None;
    };
    let (proprietaire, nom) = chemin.strip_suffix(".git").unwrap_or(chemin).split_once('/')?;
    // Une lettre seule est un lecteur sous Windows : `C:depot/x` est un chemin du poste.
    let hote_lu = hote.len() > 1
        && !hote.starts_with(['-', '.'])
        && hote.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-'));
    let depot = format!("{hote}/{proprietaire}/{nom}");
    (hote_lu && nom_lu(proprietaire) && nom_lu(nom)).then(|| Distant { url: url.into(), hote: hote.into(), depot })
}

/// L'hôte d'une autorité `[u@]h`, dont l'utilisateur, s'il y en a un, est un nom simple : un mot de passe ne s'y écrit
/// pas.
fn sans_utilisateur(autorite: &str) -> Option<&str> {
    match autorite.split_once('@') {
        Some((utilisateur, hote)) => nom_lu(utilisateur).then_some(hote),
        None => Some(autorite),
    }
}

/// Un nom simple, de propriétaire, de dépôt ou d'utilisateur : des lettres et des chiffres ASCII, `.`, `-` et `_`, ni
/// `.` ni `..`, et pas d'abord un `-`.
fn nom_lu(nom: &str) -> bool {
    !nom.is_empty()
        && nom != "."
        && nom != ".."
        && !nom.starts_with('-')
        && nom.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
}

/// Un geste git de la forge dans `dir` : `git::geste`, et les variables de [`GIT_FORGE`].
fn git_forge(dir: &Path, args: &[&str]) -> Command {
    let mut git = geste(dir, args);
    git.envs(GIT_FORGE);
    git
}

/// Ce qu'une commande de la forge a rendu, lu dans ses fichiers : la commande dite pour une preuve, son statut, sa
/// sortie et ses erreurs.
#[derive(Debug)]
struct Rendu {
    quoi: String,
    statut: ExitStatus,
    sortie: String,
    erreurs: String,
}

impl Rendu {
    fn reussi(self) -> Result<String, String> {
        if self.statut.success() {
            return Ok(self.sortie);
        }
        let dit = format!("{}{}", self.erreurs, self.sortie);
        Err(format!("{} a échoué ({}) : {}", self.quoi, self.statut, queue(dit.trim())))
    }
}

/// Joue la commande sous `delai`, comme `executer` (srt.rs) joue une preuve : son entrée lue dans le fichier
/// `entree` (rien sans lui), sa sortie et ses erreurs écrites dans deux fichiers du dossier de Kraken, effacés une fois
/// lus. Kraken n'attend que l'enfant : un petit-enfant qui garde ces fichiers ouverts ne retient rien. Au-delà du
/// délai, son arbre entier meurt. Err dit, en commençant par la commande, pourquoi elle n'a rien rendu : elle n'a pas
/// pu partir, ou elle a dépassé son délai.
fn jouer(commande: Command, dossier: &Path, entree: Option<&Path>, delai: Duration) -> Result<Rendu, String> {
    let Brut { quoi, statut, sortie, erreurs } = jouer_borne(commande, dossier, entree, delai, u64::MAX)?;
    Ok(Rendu { quoi, statut, sortie: String::from_utf8_lossy(&sortie).into_owned(), erreurs })
}

/// Ce que [`jouer_borne`] rend : la sortie en octets, telle qu'elle est lue, au plus `borne` + 1.
struct Brut {
    quoi: String,
    statut: ExitStatus,
    sortie: Vec<u8>,
    erreurs: String,
}

/// [`jouer`], dont la sortie se lit en octets, `borne` + 1 au plus : une sortie plus longue que `borne` en a donc plus
/// que `borne`, et le reste n'est jamais lu. Les erreurs se lisent bornées de même.
fn jouer_borne(
    mut commande: Command,
    dossier: &Path,
    entree: Option<&Path>,
    delai: Duration,
    borne: u64,
) -> Result<Brut, String> {
    let quoi = decrire(&commande);
    let fichiers = dossier.join(format!("forge-{}", lot()));
    let (sortie, erreurs) = (fichiers.with_extension("sortie"), fichiers.with_extension("erreurs"));
    let issue = match partir(&mut commande, dossier, entree, (&sortie, &erreurs)) {
        Err(e) => Err(format!("lancement impossible de {quoi} : {e}")),
        Ok(mut enfant) => match attendre(&mut enfant, echeance(delai)) {
            Ok(Some(statut)) => Ok(statut),
            Ok(None) => {
                let tuee = tuer_arbre(enfant.id());
                let _ = enfant.kill();
                let _ = attendre(&mut enfant, Instant::now() + SORTIE);
                let mise_a_mort = tuee.err().map_or_else(String::new, |e| format!(" ; mise à mort : {e}"));
                Err(format!("{quoi} : temps dépassé ({} s){mise_a_mort}", delai.as_secs_f32()))
            }
            Err(e) => Err(format!("{quoi} : attente impossible : {e}")),
        },
    };
    let lire = |chemin: &Path| {
        let mut octets = Vec::new();
        let lu = File::open(chemin).and_then(|f| f.take(borne.saturating_add(1)).read_to_end(&mut octets));
        let _ = fs::remove_file(chemin);
        lu.map(|_| octets).map_err(|e| format!("{quoi} : {} illisible : {e}", chemin.display()))
    };
    let (lue, dite) = (lire(&sortie), lire(&erreurs));
    let statut = issue?;
    let erreurs = String::from_utf8_lossy(&dite?).into_owned();
    Ok(Brut { statut, sortie: lue?, erreurs, quoi })
}

/// Lance la commande, son entrée lue dans `entree` (rien sans lui), sa sortie et ses erreurs écrites dans ces deux
/// fichiers, que le dossier de Kraken, créé s'il manque, reçoit.
fn partir(
    commande: &mut Command,
    dossier: &Path,
    entree: Option<&Path>,
    (sortie, erreurs): (&Path, &Path),
) -> io::Result<Child> {
    fs::create_dir_all(dossier).map_err(|e| nommee(e, format!("dossier de Kraken {}", dossier.display())))?;
    let entree = match entree {
        Some(chemin) => Stdio::from(File::open(chemin).map_err(|e| nommee(e, chemin.display()))?),
        None => Stdio::null(),
    };
    let creer = |chemin: &Path| File::create(chemin).map_err(|e| nommee(e, chemin.display()));
    commande.stdin(entree).stdout(creer(sortie)?).stderr(creer(erreurs)?).spawn()
}

/// La commande, pour une preuve : son programme et ses arguments, sans les réglages `-c` que porte un geste git en
/// tête, puis son dossier.
fn decrire(commande: &Command) -> String {
    let mut args = commande.get_args().map(OsStr::to_string_lossy).peekable();
    while args.peek().is_some_and(|arg| arg == "-c") {
        args.next();
        args.next();
    }
    let mots: Vec<_> = std::iter::once(commande.get_program().to_string_lossy()).chain(args).collect();
    let dans = commande.get_current_dir().map_or_else(String::new, |dir| format!(" dans {}", dir.display()));
    format!("{}{dans}", mots.join(" "))
}

/// Un sha de commit complet, tel que le fil l'écrit : 40 hexadécimaux minuscules.
fn est_sha(texte: &str) -> bool {
    texte.len() == 40 && texte.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

/// La branche de la vague a de quoi pousser (8004) : `tete`, la tête que rend l'assemblage, n'est plus `base`. Seul
/// Kraken avance cette branche, par des fusions : si elle a quitté sa base, elle a au moins un commit de plus.
pub(crate) fn a_pousser(base: &str, tete: &str) -> Result<(), Refus> {
    if tete != base {
        return Ok(());
    }
    Err(Refus::new(
        Code::RienAPousser,
        format!("la branche de la vague est restée sur sa base {base} : aucune tâche n'y est assemblée"),
        "Rien ne se pousse ni ne s'ouvre : relance les tâches dans une vague neuve.",
    ))
}

/// Ce que le titre et le corps d'une issue pèsent ensemble au plus, en octets d'UTF-8 (8013).
const POIDS_D_UNE_ISSUE: usize = 65_536;

/// Ce que Kraken lit au plus de la sortie de `gh issue view` : le titre et le corps au plus long qu'ils puissent
/// s'écrire en JSON (un caractère en vaut au plus six, `<`), et la place du reste de l'objet.
const SORTIE_D_UNE_ISSUE: u64 = 6 * POIDS_D_UNE_ISSUE as u64 + 4096;

/// Une issue ouverte du distant, lue par [`lire_issue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Issue {
    pub(crate) numero: u64,
    pub(crate) titre: String,
    pub(crate) corps: String,
}

/// L'issue `numero` du distant `origin` du dépôt visé : le distant se lit ([`distant`], 8001), puis `gh issue view
/// <numero> --json number,state,title,body,url --repo <hôte>/<propriétaire>/<dépôt>` se joue comme tout appel de la
/// forge à `gh` ([`Forge::gh`] : dans le dossier de Kraken, sous [`DELAI`], `GH_PROMPT_DISABLED=1`). Sa sortie se lit
/// bornée ([`SORTIE_D_UNE_ISSUE`]) et se pèse avant tout usage : plus longue que la borne, pas de l'UTF-8, ou un titre
/// et un corps de plus de [`POIDS_D_UNE_ISSUE`] octets, c'est 8013. `gh` en échec, un JSON faux, un champ qui manque,
/// un autre numéro, une PR (son `url` porte `/pull/`) ou un état autre que `OPEN`, c'est 8012. La sortie de `gh` porte
/// le titre et le corps, écrits par un autre : ni le JSON faux ni le champ qui manque ne la recopient, et ne disent que
/// ce que Kraken en a mesuré.
pub(crate) fn lire_issue(gh: &OsStr, dossier: &Path, depot: &Path, numero: u64) -> Result<Issue, Refus> {
    let distant = distant(dossier, depot)?;
    let illisible = |preuve: String| {
        Refus::new(
            Code::IssueIllisible,
            preuve,
            format!(
                "Nomme par --issue une issue ouverte de {} (gh issue view {numero} --repo {}), connecte gh à son hôte, \
                 puis relance.",
                distant.url, distant.depot
            ),
        )
    };
    let trop_longue = |preuve: String| {
        Refus::new(
            Code::IssueTropLongue,
            preuve,
            "Raccourcis le titre et le corps de l'issue (64 Kio d'UTF-8 au plus), ou écris un plan, puis relance.",
        )
    };
    let n = numero.to_string();
    let forge = Forge { gh, dossier, depot, distant: distant.clone() };
    let vue = forge.gh(&["issue", "view", &n, "--json", "number,state,title,body,url"]);
    let Brut { quoi, statut, sortie, erreurs } =
        jouer_borne(vue, dossier, None, DELAI, SORTIE_D_UNE_ISSUE).map_err(illisible)?;
    if !statut.success() {
        // Ses erreurs seules : sa sortie peut porter le titre et le corps.
        return Err(illisible(format!("{quoi} a échoué ({statut}) : {}", queue(erreurs.trim()))));
    }
    if sortie.len() as u64 > SORTIE_D_UNE_ISSUE {
        return Err(trop_longue(format!(
            "gh rend l'issue {numero} en plus de {SORTIE_D_UNE_ISSUE} octets : son titre et son corps passent \
             {POIDS_D_UNE_ISSUE} octets"
        )));
    }
    let Ok(json) = std::str::from_utf8(&sortie) else {
        return Err(trop_longue(format!("gh ne rend pas l'issue {numero} en UTF-8")));
    };
    let Ok(Json::Objet(lue)) = json::document(json) else {
        return Err(illisible(format!("gh ne rend pas l'issue {numero} en un objet JSON ({} octets)", sortie.len())));
    };
    let texte = |cle: &str| match lue.get(cle) {
        Some(Json::Texte(texte)) => Some(texte.as_str()),
        _ => None,
    };
    let lu = match lue.get("number") {
        Some(Json::Nombre(nombre)) => nombre.parse::<u64>().ok(),
        _ => None,
    };
    let (etat, titre, corps, url) = (texte("state"), texte("title"), texte("body"), texte("url"));
    let (Some(lu), Some(etat), Some(titre), Some(corps), Some(url)) = (lu, etat, titre, corps, url) else {
        let lus = [("number", lu.is_some()), ("state", etat.is_some()), ("title", titre.is_some())];
        let lus = lus.into_iter().chain([("body", corps.is_some()), ("url", url.is_some())]);
        let manquants: Vec<&str> = lus.filter(|(_, lu)| !lu).map(|(cle, _)| cle).collect();
        return Err(illisible(format!(
            "gh ne rend pas, de l'issue {numero}, ces champs en leur type : {}",
            manquants.join(", ")
        )));
    };
    if lu != numero {
        return Err(illisible(format!("gh rend l'issue {lu}, pas l'issue {numero}")));
    }
    if url.contains("/pull/") {
        return Err(illisible(format!(
            "le numéro {numero} est celui d'une PR, pas d'une issue : son url porte /pull/"
        )));
    }
    if etat != "OPEN" {
        return Err(illisible(format!("l'issue {numero} n'est pas ouverte : son état est {etat}")));
    }
    let poids = titre.len() + corps.len();
    if poids > POIDS_D_UNE_ISSUE {
        return Err(trop_longue(format!(
            "le titre et le corps de l'issue {numero} pèsent {poids} octets, plus que {POIDS_D_UNE_ISSUE}"
        )));
    }
    Ok(Issue { numero, titre: titre.into(), corps: corps.into() })
}

/// Ce dont chaque geste de la forge a besoin, une fois le distant lu ([`distant`]).
pub(crate) struct Forge<'a> {
    /// Le programme `gh` : [`programme_gh`], ou un faux dans les essais.
    pub(crate) gh: &'a OsStr,
    /// Le dossier de Kraken : `gh` y part, et chaque commande y écrit sa sortie.
    pub(crate) dossier: &'a Path,
    /// Le dépôt visé, où joue git.
    pub(crate) depot: &'a Path,
    pub(crate) distant: Distant,
}

/// Une PR ouverte, telle que `gh` la relit : son numéro et son adresse.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PrOuverte {
    pub(crate) numero: u64,
    pub(crate) url: String,
}

/// La CI d'une PR, lue : sa couleur, et une ligne par check, `<nom> : <issue>`.
#[derive(Debug, PartialEq)]
pub(crate) struct Ci {
    pub(crate) couleur: Couleur,
    pub(crate) texte: String,
}

/// Ce qu'un tour de lecture de la CI a vu.
enum Tour {
    Verdict(Ci),
    /// Pas encore de verdict : des checks en cours ou sans succès, aucun check, ou une lecture ratée, dit en clair.
    Attente(String),
    /// La PR a changé (8010) ou est en conflit (8009) : Kraken s'arrête.
    Arret(Refus),
}

impl Forge<'_> {
    /// Joue la commande sous [`DELAI`] : sa sortie si elle réussit, sinon ce qui a été vu.
    pub(crate) fn faire(&self, commande: Command, entree: Option<&Path>) -> Result<String, String> {
        jouer(commande, self.dossier, entree, DELAI).and_then(Rendu::reussi)
    }

    pub(crate) fn git(&self, args: &[&str]) -> Command {
        git_forge(self.depot, args)
    }

    /// `gh`, sans argument : lancé dans le dossier de Kraken, jamais dans le dépôt visé, sans rien demander à personne,
    /// avec l'environnement de Kraken.
    fn gh_seul(&self) -> Command {
        let mut gh = Command::new(self.gh);
        gh.current_dir(self.dossier).env("GH_PROMPT_DISABLED", "1");
        gh
    }

    /// `gh <args> --repo <h/p/d>` : un appel qui vise le dépôt le nomme, et ne le tire jamais d'un dossier.
    fn gh(&self, args: &[&str]) -> Command {
        let mut gh = self.gh_seul();
        gh.args(args).args(["--repo", &self.distant.depot]);
        gh
    }

    /// La base que nomme `--pr` (8002) : `git check-ref-format --branch` la prend telle quelle, puis la tête de
    /// `refs/heads/<branche>` du distant se lit ([`Forge::tete_distante`]). Rend son sha, d'où part la vague : les
    /// commits locaux non poussés n'y sont pas.
    pub(crate) fn base_distante(&self, branche: &str) -> Result<String, Refus> {
        let refus = |preuve: String| {
            let correction = format!(
                "Donne à --pr une branche qui existe sur le distant {} (git ls-remote --heads {}), puis relance.",
                self.distant.url, self.distant.url
            );
            Refus::new(Code::BaseInconnue, preuve, correction)
        };
        let lue = self.faire(self.git(&["check-ref-format", "--branch", branche]), None).map_err(refus)?;
        if lue.trim_end() != branche {
            let lue = lue.trim_end();
            return Err(refus(format!(
                "« {branche} » n'est pas une branche que git prend telle quelle : il y lit « {lue} »"
            )));
        }
        self.tete_distante(branche).map_err(refus)
    }

    /// La tête de `refs/heads/<branche>` du distant, lue dans une ref à Kraken et non dans `FETCH_HEAD`, que le fetch
    /// automatique d'un éditeur peut réécrire entre le fetch de Kraken et sa lecture : `git fetch --no-tags
    /// --no-write-fetch-head <url> refs/heads/<branche>:refs/kraken/base/<lot>` la pose, `rev-parse` la lit, et
    /// `update-ref -d` la retire, quoi qu'ait rendu la lecture. Le `<lot>` est celui de la commande ([`lot`]), que
    /// personne d'autre ne tire : deux vagues à `--pr` d'un même dépôt ne se disputent rien. `--no-tags` : un fetch qui
    /// range dans une ref rapporte aussi les étiquettes de l'histoire ramenée, que Kraken n'a pas à poser dans le dépôt
    /// visé. `--no-write-fetch-head` : Kraken n'écrit ni ne lit `FETCH_HEAD`, celui de l'humain reste tel qu'il est.
    /// Une ref qui ne se retire pas se dit sur stderr, et reste.
    // raccourci : un Kraken tué entre le fetch et le retrait laisse sa ref refs/kraken/base/… dans le dépôt visé ;
    // monter vers un ménage de ces refs au lancement si elles s'accumulent
    fn tete_distante(&self, branche: &str) -> Result<String, String> {
        self.tete_distante_avec(branche, || {})
    }

    /// [`Forge::tete_distante`], où `entre` joue entre le fetch et la lecture : la fenêtre où le fetch automatique d'un
    /// éditeur réécrit `FETCH_HEAD`, que les essais remplissent.
    fn tete_distante_avec(&self, branche: &str, entre: impl FnOnce()) -> Result<String, String> {
        let ici = format!("refs/kraken/base/{}", lot());
        let ramene = format!("refs/heads/{branche}:{ici}");
        self.faire(self.git(&["fetch", "--no-tags", "--no-write-fetch-head", &self.distant.url, &ramene]), None)?;
        entre();
        let lue = self.faire(self.git(&["rev-parse", "--verify", &format!("{ici}^{{commit}}")]), None);
        if let Err(vu) = self.faire(self.git(&["update-ref", "-d", &ici]), None) {
            eprintln!("la ref {ici} ne se retire pas, elle reste dans le dépôt visé : {vu}");
        }
        Ok(lue?.trim().to_string())
    }

    /// `gh` se lance et se dit connecté à l'hôte du distant (8003) : `gh auth status --hostname <h>`, sans `--repo`,
    /// que cette commande n'a pas.
    pub(crate) fn connecte(&self) -> Result<(), Refus> {
        let mut gh = self.gh_seul();
        gh.args(["auth", "status", "--hostname", &self.distant.hote]);
        self.faire(gh, None).map(drop).map_err(|vu| {
            let hote = &self.distant.hote;
            let correction = format!(
                "Installe gh et connecte-le à {hote} (gh auth login --hostname {hote}), ou nomme par KRAKEN_GH le \
                 programme gh à lancer, puis relance."
            );
            Refus::new(Code::GhInjoignable, vu, correction)
        })
    }

    /// Rien de ce que Kraken publierait ne revient à l'humain (8005) : aucun fichier de `.github/`, à la racine et en
    /// toute casse, ne change entre `base` et `tete` ; aucun commit de `base..tete` n'a pour auteur ou pour committer
    /// un autre que Kraken ([`commit_etranger`]) ; et aucun des `chemins` du poste (le dossier personnel, le dossier de
    /// Kraken, le dépôt visé), écrit avec `\` et avec `/`, sans casse sous Windows, ne paraît dans une ligne ajoutée du
    /// diff (`git diff -U0 --text --no-ext-diff`), dans le message d'un commit poussé (`base..tete`), dans le titre ou
    /// dans le corps. Les deux `git diff` portent `--no-relative` : le dépôt visé peut être un sous-dossier du dépôt,
    /// et avec `diff.relative` (la configuration de l'humain ou du dépôt) git ne dirait que ce qui change sous lui. Un
    /// chemin sans nom, une racine, ne compte pas : il paraîtrait partout.
    // raccourci : un chemin écrit autrement (échappé en `\\`, en nom court 8.3, sous `~`) passe ; monter si une PR en
    // montre un
    pub(crate) fn publiable(
        &self,
        base: &str,
        tete: &str,
        titre: &str,
        corps: &str,
        chemins: &[&Path],
    ) -> Result<(), Refus> {
        let refus = |preuve: String| Refus::new(Code::PublicationRefusee, preuve, PUBLIER);
        let lire = |args: &[&str]| {
            let vu = |vu: String| refus(format!("ce que Kraken publierait ne se lit pas : {vu}"));
            self.faire(self.git(args), None).map_err(vu)
        };
        let noms = lire(&["diff", "--name-only", "--no-relative", "-z", "--no-renames", base, tete, "--"])?;
        let github: Vec<&str> = noms
            .split('\0')
            .filter(|nom| {
                let nom = nom.to_ascii_lowercase();
                nom == ".github" || nom.starts_with(".github/")
            })
            .collect();
        if !github.is_empty() {
            return Err(refus(format!("{} change entre la base {base} et la tête {tete}", github.join(", "))));
        }
        let diff = lire(&[
            "diff",
            "-U0",
            "--no-relative",
            "--text",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            base,
            tete,
            "--",
        ])?;
        let plage = format!("{base}..{tete}");
        let messages = lire(&["log", "--format=%B", "--no-show-signature", &plage, "--"])?;
        let auteurs = lire(&["log", "--format=%H%x00%an <%ae>%x00%cn <%ce>", "--no-show-signature", &plage, "--"])?;
        if let Some(etranger) = commit_etranger(&auteurs) {
            return Err(refus(format!("{etranger}, entre la base {base} et la tête {tete}")));
        }
        let casse = |texte: &str| if cfg!(windows) { texte.to_lowercase() } else { texte.to_string() };
        let mut lieux = ajoutees(&diff);
        lieux.push(("les messages des commits poussés".into(), messages));
        lieux.push(("le titre".into(), titre.into()));
        lieux.push(("le corps".into(), corps.into()));
        let lieux: Vec<(String, String)> = lieux.into_iter().map(|(lieu, texte)| (lieu, casse(&texte))).collect();
        for chemin in chemins.iter().filter(|chemin| chemin.file_name().is_some()) {
            let ecrit = chemin.to_string_lossy();
            for forme in [ecrit.replace('\\', "/"), ecrit.replace('/', "\\")] {
                let cherche = casse(&forme);
                if let Some((lieu, _)) = lieux.iter().find(|(_, texte)| texte.contains(&cherche)) {
                    return Err(refus(format!("le chemin du poste {forme} paraît dans {lieu}")));
                }
            }
        }
        Ok(())
    }

    /// Pousse `tete`, un sha et jamais un nom, dans `refs/heads/<branche>` du distant (8006) : `git -c
    /// push.gpgSign=false push --no-follow-tags --recurse-submodules=no <url> <tete>:refs/heads/<branche>`, sans
    /// forcer. Un commit étranger sur la branche distante fait refuser le push, et le distant ne bouge pas ; la tête
    /// d'une relance, qui descend de la précédente, avance la branche.
    pub(crate) fn pousser(&self, tete: &str, branche: &str) -> Result<(), Refus> {
        let refus = |preuve: String| {
            let correction = format!(
                "Rien ne s'ouvre : regarde pourquoi le distant refuse {branche} (une branche du même nom, des droits), \
                 puis pousse-la toi-même."
            );
            Refus::new(Code::PousseeRefusee, preuve, correction)
        };
        if !est_sha(tete) {
            return Err(refus(format!(
                "« {tete} » n'est pas un sha de 40 hexadécimaux : Kraken ne pousse qu'un commit"
            )));
        }
        let cible = format!("{tete}:refs/heads/{branche}");
        let push = self.git(&[
            "-c",
            "push.gpgSign=false",
            "push",
            "--no-follow-tags",
            "--recurse-submodules=no",
            &self.distant.url,
            &cible,
        ]);
        self.faire(push, None).map(drop).map_err(refus)
    }

    /// Ouvre la PR de `branche` contre `base` (8007) : `gh pr create … --title <titre> --body-file -`, le corps sur son
    /// entrée, puis `gh pr view <branche> … --json number,url,headRefOid`, dont la tête doit être `tete`, le sha
    /// poussé. Rend son numéro et son adresse. Un create raté, ou tué à son délai, a pu ouvrir la PR avant de mourir :
    /// la relecture de la branche se joue donc après lui quoi qu'il ait rendu, et la PR qu'elle donne sur `tete` est
    /// prise, dite sur stderr, sans que rien se rejoue. Sinon, la branche reste poussée, sans PR, et le refus dit le
    /// create puis la relecture.
    pub(crate) fn ouvrir(
        &self,
        base: &str,
        branche: &str,
        titre: &str,
        corps: &str,
        tete: &str,
    ) -> Result<PrOuverte, Refus> {
        let refus = |preuve: String| {
            let correction = format!(
                "La branche {branche} reste poussée, sans PR : ouvre-la toi-même contre {base}, ou retire la branche \
                 du distant."
            );
            Refus::new(Code::PrImpossible, preuve, correction)
        };
        let fichier = self.dossier.join(format!("forge-{}.corps", lot()));
        fs::create_dir_all(self.dossier)
            .and_then(|()| fs::write(&fichier, corps))
            .map_err(|e| refus(format!("le corps de la PR ne s'écrit pas dans {} : {e}", fichier.display())))?;
        let cree = self.gh(&["pr", "create", "--base", base, "--head", branche, "--title", titre, "--body-file", "-"]);
        let creee = self.faire(cree, Some(&fichier));
        let _ = fs::remove_file(&fichier);
        let vue = self.faire(self.gh(&["pr", "view", branche, "--json", "number,url,headRefOid"]), None);
        match (creee, vue.and_then(|json| lire_pr(&json, tete))) {
            (creee, Ok(pr)) => {
                if let Err(vu) = creee {
                    eprintln!(
                        "gh pr create a échoué, mais la PR {} porte déjà {tete} : Kraken la prend ({vu})",
                        pr.numero
                    );
                }
                Ok(pr)
            }
            (Ok(_), Err(relue)) => Err(refus(relue)),
            (Err(vu), Err(relue)) => Err(refus(format!("{vu} ; la PR de la branche ne se relit pas : {relue}"))),
        }
    }

    /// La CI de la PR `numero` sur `tete`, tout de suite, puis toutes les `intervalle`, `delai` au plus (15 s et 30 min
    /// au contrat). À chaque tour, dans cet ordre : la PR (`gh pr view <numero> … --json headRefOid,state,mergeable`)
    /// fermée, ou sa tête autre que `tete`, est le refus 8010 (y compris une PR fusionnée qui porte encore `avant`) ;
    /// ouverte et en conflit (`mergeable` à `CONFLICTING`), le refus 8009 ; une PR fusionnée sur `tete` se lit comme
    /// une ouverte (son `mergeable` n'a plus de sens et ne se lit pas), sauf qu'une CI rouge y est le refus 8010 (rien
    /// n'est à relancer) ; sinon les runs d'Actions de `tete` (`gh run list --commit`) et les statuts de son commit
    /// (`gh api repos/<p>/<d>/commits/<tete>/status`) se classent ensemble ([`classer`]), sans jamais de vert quand
    /// l'une des deux listes a [`COUPEE`] éléments ou plus. Un tour sans verdict, ou que `gh` ne rend pas, attend le
    /// suivant ; aucun verdict dans le délai est le refus 8008, qui dit pourquoi. `avant` est, après une relance, le
    /// sha qu'elle remplace : la forge met quelques secondes à porter ce qu'on vient de pousser, et une PR ouverte qui
    /// porte encore `avant` n'a pas été changée par un autre, elle attend aussi le tour suivant.
    pub(crate) fn lire_ci(
        &self,
        numero: u64,
        tete: &str,
        avant: Option<&str>,
        intervalle: Duration,
        delai: Duration,
    ) -> Result<Ci, Refus> {
        let fin = echeance(delai);
        loop {
            let vu = match self.tour(numero, tete, avant) {
                Tour::Verdict(ci) => return Ok(ci),
                Tour::Arret(refus) => return Err(refus),
                Tour::Attente(vu) => vu,
            };
            if Instant::now().checked_add(intervalle).is_none_or(|prochain| prochain > fin) {
                let secondes = delai.as_secs_f32();
                return Err(Refus::new(
                    Code::CiMuette,
                    format!("la CI de la PR {numero} n'a pas rendu de verdict en {secondes} s : {vu}"),
                    "Regarde la CI de la PR sur la forge ; si la PR est ouverte, fusionne-la ou relance-la toi-même.",
                ));
            }
            thread::sleep(intervalle);
        }
    }

    /// Un tour de [`Self::lire_ci`] : la PR d'abord, puis la CI de `tete`. Une PR `MERGED` dont la tête est `tete` a sa
    /// CI lue et classée comme celle d'une PR ouverte (mêmes runs, mêmes statuts, même garde de [`COUPEE`]), sans lire
    /// son `mergeable` ; son vert est un verdict, son rouge le refus 8010, sans verdict le tour suivant. Une PR
    /// ouverte sur `avant` attend le tour suivant ; une PR `CLOSED`, fusionnée sur une autre tête que `tete`
    /// (`avant` compris), ou ouverte sur une tête qui n'est ni `tete` ni `avant`, est le refus 8010 tout de suite.
    fn tour(&self, numero: u64, tete: &str, avant: Option<&str>) -> Tour {
        let n = numero.to_string();
        let vue = self.gh(&["pr", "view", &n, "--json", "headRefOid,state,mergeable"]);
        let json = match self.faire(vue, None) {
            Ok(json) => json,
            Err(vu) => return Tour::Attente(vu),
        };
        let Ok(Json::Objet(pr)) = json::document(&json) else {
            return Tour::Attente(format!("gh ne rend pas la PR en un objet JSON : {}", queue(json.trim())));
        };
        let texte = |cle: &str| match pr.get(cle) {
            Some(Json::Texte(texte)) => Some(texte.as_str()),
            _ => None,
        };
        let (Some(etat), Some(oid)) = (texte("state"), texte("headRefOid")) else {
            return Tour::Attente(format!("gh ne dit ni l'état ni la tête de la PR : {}", queue(json.trim())));
        };
        let arret = |code, preuve: String, correction| Tour::Arret(Refus::new(code, preuve, correction));
        let fusionnee = etat == "MERGED";
        if etat != "OPEN" && !fusionnee {
            return arret(Code::PrChangee, format!("la PR {numero} est {etat}, et n'est plus ouverte"), CHANGEE);
        }
        if oid != tete {
            if avant == Some(oid) && !fusionnee {
                return Tour::Attente(format!("la PR {numero} porte encore {oid}, pas le sha poussé {tete}"));
            }
            return arret(
                Code::PrChangee,
                format!("la PR {numero} est {etat} et porte {oid}, pas le sha poussé {tete}"),
                CHANGEE,
            );
        }
        if !fusionnee && texte("mergeable") == Some("CONFLICTING") {
            return arret(Code::AReintegrer, format!("la PR {numero} est en conflit avec sa base"), REINTEGRER);
        }
        // raccourci : seuls les runs d'Actions et les statuts du commit se lisent, pas les check runs d'une autre
        // application, qu'un jeton à grain fin ne lit pas ; monter vers `gh api …/commits/<sha>/check-runs` quand une
        // PR en attend un
        let limite = COUPEE.to_string();
        let runs = self.gh(&["run", "list", "--commit", tete, "--limit", &limite, "--json", CHAMPS_DES_RUNS]);
        let (_, chemin) = self.distant.depot.split_once('/').unwrap_or_default();
        let mut statuts = self.gh_seul();
        let route = format!("repos/{chemin}/commits/{tete}/status?per_page={COUPEE}");
        statuts.args(["api", "--hostname", &self.distant.hote, &route]);
        let lus = self.liste(runs, None).and_then(|runs| Ok((runs, self.liste(statuts, Some("statuses"))?)));
        let (runs, statuts) = match lus {
            Ok(lus) => lus,
            Err(vu) => return Tour::Attente(vu),
        };
        let (n_runs, n_statuts) = (runs.len(), statuts.len());
        let checks: Vec<Json> = runs.into_iter().chain(statuts).collect();
        match classer(&checks) {
            (Some(Couleur::Vert), texte) if n_runs >= COUPEE || n_statuts >= COUPEE => Tour::Attente(format!(
                "la PR a {n_runs} runs et {n_statuts} statuts, et gh n'en rend que {COUPEE} de chaque au plus : \
                 une liste est peut-être coupée, et un vert ne se prouve pas sur elle :\n{texte}"
            )),
            (Some(Couleur::Rouge), texte) if fusionnee => arret(
                Code::PrChangee,
                format!("la PR {numero} est fusionnée et sa CI est rouge, rien n'est à relancer :\n{texte}"),
                CHANGEE,
            ),
            (Some(couleur), texte) => Tour::Verdict(Ci { couleur, texte }),
            (None, texte) if texte.is_empty() => Tour::Attente("la PR n'a aucun check".into()),
            (None, texte) => Tour::Attente(format!("des checks en cours, ou aucun qui ait réussi :\n{texte}")),
        }
    }

    /// La liste JSON que rend `commande` : le document, ou son champ `cle`. Err dit ce qui a été vu : une commande
    /// ratée, ou autre chose qu'une liste.
    fn liste(&self, commande: Command, cle: Option<&str>) -> Result<Vec<Json>, String> {
        let json = self.faire(commande, None)?;
        let lue = match (json::document(&json), cle) {
            (Ok(Json::Liste(liste)), None) => Some(liste),
            (Ok(Json::Objet(mut objet)), Some(cle)) => match objet.remove(cle) {
                Some(Json::Liste(liste)) => Some(liste),
                _ => None,
            },
            _ => None,
        };
        lue.ok_or_else(|| format!("gh ne rend pas une liste : {}", queue(json.trim())))
    }

    /// La fin du journal d'une CI rouge sur `tete` : `gh run list … --commit <tete> --json databaseId,conclusion`, puis
    /// `gh run view <id> … --log-failed` de chaque run en échec, bout à bout, coupé par `queue`. Vide sans journal : le
    /// contexte d'une relance n'a alors que ses checks.
    pub(crate) fn journal_rouge(&self, tete: &str) -> String {
        let liste = self.faire(self.gh(&["run", "list", "--commit", tete, "--json", "databaseId,conclusion"]), None);
        let Some(Ok(Json::Liste(runs))) = liste.ok().map(|liste| json::document(&liste)) else {
            return String::new();
        };
        let mut journal = String::new();
        for run in &runs {
            let Json::Objet(run) = run else { continue };
            let (Some(Json::Nombre(id)), Some(Json::Texte(conclusion))) =
                (run.get("databaseId"), run.get("conclusion"))
            else {
                continue;
            };
            let en_echec = ECHECS.contains(&conclusion.to_ascii_uppercase().as_str());
            if en_echec
                && id.bytes().all(|c| c.is_ascii_digit())
                && let Ok(log) = self.faire(self.gh(&["run", "view", id, "--log-failed"]), None)
            {
                journal.push_str(&log);
            }
        }
        queue(&journal)
    }

    /// La base `branche` n'a pas bougé depuis la vague (8009) : sa tête distante, relue ([`Forge::tete_distante`]), est
    /// un ancêtre de `tete`. Une base qui ne se relit pas compte comme une base qui a bougé : Kraken ne relance que sur
    /// ce qu'il a vu, et ne réintègre pas.
    pub(crate) fn base_a_bouge(&self, branche: &str, tete: &str) -> Result<(), Refus> {
        let refus = |preuve: String| Refus::new(Code::AReintegrer, preuve, REINTEGRER);
        let distante =
            self.tete_distante(branche).map_err(|vu| refus(format!("la base {branche} ne se relit pas : {vu}")))?;
        let ancetre = jouer(self.git(&["merge-base", "--is-ancestor", &distante, tete]), self.dossier, None, DELAI);
        let ancetre = ancetre.map_err(refus)?;
        match ancetre.statut.code() {
            Some(0) => Ok(()),
            Some(1) => Err(refus(format!(
                "la base {branche} a bougé : sa tête distante {distante} n'est plus un ancêtre de {tete}"
            ))),
            _ => Err(refus(ancetre.reussi().err().unwrap_or_default())),
        }
    }

    /// `x`, un sha ou un nom que git lit dans `dir`, est `p` ou l'un de ses ancêtres : `git merge-base --is-ancestor`,
    /// joué dans `dir`. Un geste qui ne part pas ou qui rate vaut non : Kraken ne retire que ce qu'il a vu dans `p`.
    pub(crate) fn dans(&self, dir: &Path, x: &str, p: &str) -> bool {
        let ancetre = git_forge(dir, &["merge-base", "--is-ancestor", x, p]);
        jouer(ancetre, self.dossier, None, DELAI).is_ok_and(|rendu| rendu.statut.success())
    }
}

/// La PR que relit `gh pr view --json number,url,headRefOid` : un numéro positif, une adresse, et `tete` pour tête.
fn lire_pr(json: &str, tete: &str) -> Result<PrOuverte, String> {
    let Ok(Json::Objet(pr)) = json::document(json) else {
        return Err(format!("gh ne relit pas la PR en un objet JSON : {}", queue(json.trim())));
    };
    let numero = match pr.get("number") {
        Some(Json::Nombre(numero)) => numero.parse::<u64>().ok().filter(|numero| *numero > 0),
        _ => None,
    };
    let url = match pr.get("url") {
        Some(Json::Texte(url)) if !url.trim().is_empty() => Some(url),
        _ => None,
    };
    let (Some(numero), Some(url), Some(Json::Texte(oid))) = (numero, url, pr.get("headRefOid")) else {
        return Err(format!("la PR relue n'a pas un numéro positif, une adresse et une tête : {}", queue(json.trim())));
    };
    if oid != tete {
        return Err(format!("la PR {numero} relue porte {oid}, pas le sha poussé {tete}"));
    }
    Ok(PrOuverte { numero, url: url.clone() })
}

/// Classe les checks, runs de `gh run list` et statuts de l'api, chacun par lui-même : un statut porte `state`, un run
/// `status` et `conclusion`, que `gh` écrit en minuscules et que Kraken lit en majuscules. Un workflow relancé laisse
/// dans la liste le run d'avant : de deux checks de même désignation, seul le plus récent compte ([`derniers`]). Un
/// check est en cours (un run pas `COMPLETED`, un statut `PENDING` ou `EXPECTED`), en échec (son issue dans
/// [`ECHECS`]), réussi (`SUCCESS`), ou rien de cela (`NEUTRAL`, `SKIPPED`, `STALE`, une issue inconnue). La CI est
/// rouge si un check a échoué, une fois plus aucun en cours ; verte si aucun n'est en cours ni en échec et qu'au moins
/// un a réussi ; sans verdict sinon, ce que reste une PR sans check. Rend aussi une ligne par check, `<nom> : <issue>`.
fn classer(checks: &[Json]) -> (Option<Couleur>, String) {
    let (mut en_cours, mut en_echec, mut reussi) = (false, false, false);
    let mut lignes = Vec::new();
    for check in derniers(checks) {
        let champ = |cle: &str| champ_de(check, cle).to_ascii_uppercase();
        let (nom, issue, court) = if est_statut(check) {
            let etat = champ("state");
            let court = matches!(etat.as_str(), "PENDING" | "EXPECTED");
            (champ_de(check, "context"), etat, court)
        } else if champ("status") == "COMPLETED" {
            (champ_de(check, "name"), champ("conclusion"), false)
        } else {
            (champ_de(check, "name"), champ("status"), true)
        };
        en_cours |= court;
        en_echec |= !court && ECHECS.contains(&issue.as_str());
        reussi |= !court && issue == "SUCCESS";
        lignes.push(format!("{nom} : {issue}"));
    }
    let couleur = match (en_cours, en_echec, reussi) {
        (true, _, _) => None,
        (false, true, _) => Some(Couleur::Rouge),
        (false, false, true) => Some(Couleur::Vert),
        (false, false, false) => None,
    };
    (couleur, lignes.join("\n"))
}

fn champ_de<'a>(check: &'a Json, cle: &str) -> &'a str {
    match check {
        Json::Objet(check) => match check.get(cle) {
            Some(Json::Texte(texte)) => texte.as_str(),
            _ => "",
        },
        _ => "",
    }
}

/// Le check est un statut (il porte `state`) et non un run.
fn est_statut(check: &Json) -> bool {
    matches!(check, Json::Objet(check) if check.contains_key("state"))
}

/// Ce qui désigne un check : un statut, son contexte ; un run, son workflow. Le même nom dans un autre workflow est un
/// autre run, dont le succès ne cache pas l'échec de l'un.
fn designation(check: &Json) -> (bool, &str) {
    if est_statut(check) { (true, champ_de(check, "context")) } else { (false, champ_de(check, "workflowName")) }
}

/// L'instant `cle` (`startedAt` ou `createdAt`) d'un check, tel que `gh` l'écrit ; `None` sans lui, vide, ou à
/// l'instant nul `0001-01-01T00:00:00Z` que `gh` écrit pour ce qui n'a pas commencé.
fn instant<'a>(check: &'a Json, cle: &str) -> Option<&'a str> {
    Some(champ_de(check, cle)).filter(|instant| !instant.is_empty() && !instant.starts_with("0001-01-01"))
}

/// Le check `b` est plus récent que `a`, chacun avec son rang dans la liste : par `startedAt` s'ils l'ont tous deux,
/// sinon par `createdAt`, sinon `b` l'est s'il vient plus bas dans la liste.
// raccourci : les instants se comparent comme du texte (UTC et à la seconde, comme gh les écrit) ; monter vers des
// instants lus si gh change leur forme
fn plus_recent((a, rang_a): (&Json, usize), (b, rang_b): (&Json, usize)) -> bool {
    for cle in ["startedAt", "createdAt"] {
        if let (Some(avant), Some(apres)) = (instant(a, cle), instant(b, cle))
            && avant != apres
        {
            return apres > avant;
        }
    }
    rang_b > rang_a
}

/// Les checks de la liste sans ceux qu'un autre de même désignation remplace : un workflow relancé laisse dans la liste
/// le run d'avant, et seul le plus récent compte ([`plus_recent`]). La liste garde son ordre. Un check sans nom, que gh
/// ne dirait plus par le champ qu'on lit, ne remplace aucun autre et n'est remplacé par aucun : sinon tous se
/// réduiraient à un seul, et un succès cacherait un échec.
fn derniers(checks: &[Json]) -> Vec<&Json> {
    let mut retenus: Vec<usize> = Vec::new();
    for (rang, check) in checks.iter().enumerate() {
        let nomme = !designation(check).1.is_empty();
        match retenus.iter_mut().find(|retenu| nomme && designation(&checks[**retenu]) == designation(check)) {
            Some(retenu) if plus_recent((&checks[*retenu], *retenu), (check, rang)) => *retenu = rang,
            Some(_) => {}
            None => retenus.push(rang),
        }
    }
    retenus.sort_unstable();
    retenus.into_iter().map(|rang| &checks[rang]).collect()
}

pub(crate) fn en_echec(texte: &str) -> String {
    let echec = |ligne: &&str| ligne.rsplit_once(" : ").is_some_and(|(_, issue)| ECHECS.contains(&issue));
    texte.lines().filter(echec).collect::<Vec<_>>().join("\n")
}

/// Le premier commit de `log` qui n'est pas de Kraken, dit en clair, ou `None` si tous le sont. `log` est le `git log
/// --format=%H%x00%an <%ae>%x00%cn <%ce>` de la plage publiée : une ligne par commit, `<sha>`, l'auteur et le committer
/// séparés par un NUL, chacun écrit `nom <adresse>`. Un commit est de Kraken si l'un et l'autre sont l'identité de
/// [`IDENTITE`], que `git::geste` pose sur chacun de ses gestes et la main sur son processus ; une ligne d'une autre
/// forme n'est pas de lui.
// raccourci : l'identité se déclare, et un commit qui la porte sans être de Kraken passe ; monter vers des commits
// signés d'une clé de Kraken
fn commit_etranger(log: &str) -> Option<String> {
    let identite = |cle: &str| IDENTITE.iter().find(|(nom, _)| *nom == cle).map_or("", |&(_, valeur)| valeur);
    let kraken = |nom: &str, adresse: &str| format!("{} <{}>", identite(nom), identite(adresse));
    let attendu = format!(
        "{}\0{}",
        kraken("GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL"),
        kraken("GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL")
    );
    let ligne = log.lines().find(|ligne| ligne.split_once('\0').is_none_or(|(_, qui)| qui != attendu))?;
    let (sha, qui) = ligne.split_once('\0').unwrap_or((ligne, ""));
    let (auteur, committer) = qui.split_once('\0').unwrap_or((qui, ""));
    Some(format!("le commit {sha} n'est pas de Kraken : auteur « {auteur} », committer « {committer} »"))
}

/// Les lignes qu'ajoute un diff de git (`-U0`), fichier par fichier : `les lignes ajoutées de <fichier>`, et leur
/// texte. Une ligne ajoutée commence par `+` dans un morceau (après `@@`) ; l'en-tête d'un fichier (`+++ b/…`) n'en est
/// pas une.
fn ajoutees(diff: &str) -> Vec<(String, String)> {
    let mut fichiers: Vec<(String, String)> = Vec::new();
    let mut dans_un_morceau = false;
    for ligne in diff.lines() {
        if ligne.starts_with("diff --git ") {
            dans_un_morceau = false;
        } else if ligne.starts_with("@@") {
            dans_un_morceau = true;
        } else if !dans_un_morceau && let Some(nom) = ligne.strip_prefix("+++ ") {
            fichiers.push((format!("les lignes ajoutées de {nom}"), String::new()));
        } else if dans_un_morceau
            && let (Some(ajout), Some((_, texte))) = (ligne.strip_prefix('+'), fichiers.last_mut())
        {
            texte.push_str(ajout);
            texte.push('\n');
        }
    }
    fichiers
}

#[cfg(test)]
mod tests;
