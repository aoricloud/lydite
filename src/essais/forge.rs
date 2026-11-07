//! Les aides que les tests de `github` et de `publication` partagent : un lieu de forge, des commits posés, et ce que
//! le faux `gh` rend d'une PR et de sa CI. À part : leur `PR` n'est pas celui de `gh_qui_suit`.
use super::*;
use crate::branchements::github::CHAMPS_DES_RUNS;

/// Le sha qu'une PR des essais porte, quand aucun dépôt n'en fait un.
pub(crate) const TETE: &str = "c3a91e07d5f2486b0a7e3d915c28b4f60e1d7a39";

/// Un lieu de forge neuf : la racine du test, le dépôt visé, que son distant nu atteint par `URL_DU_DISTANT` et dont la
/// base est poussée dans `main`, le distant nu, le dossier de Kraken et un faux `gh`.
pub(crate) struct Lieu {
    pub(crate) racine: PathBuf,
    pub(crate) depot: PathBuf,
    pub(crate) nu: PathBuf,
    pub(crate) kraken: PathBuf,
    pub(crate) gh: FauxGh,
}

pub(crate) fn lieu(nom: &str) -> Lieu {
    let racine = racine(nom);
    let depot = depot(&racine);
    let nu = distant_nu(&racine, &depot);
    let gh = FauxGh::ecrire(&racine);
    Lieu { kraken: racine.join("lydite"), racine, depot, nu, gh }
}

impl Lieu {
    /// La base : le `HEAD` du dépôt, que `main` porte sur le distant.
    pub(crate) fn base(&self) -> String {
        git(&self.depot, &["rev-parse", "HEAD"])
    }

    /// La tête de `refs/heads/<branche>` du distant nu.
    pub(crate) fn distante(&self, branche: &str) -> String {
        git(&self.nu, &["rev-parse", &format!("refs/heads/{branche}")])
    }

    /// Chaque appel du faux `gh` est parti du dossier de Kraken, jamais du dépôt visé, et nomme le dépôt du distant par
    /// `--repo`, sauf `gh auth status`, qui nomme son hôte, et `gh api`, qui nomme son hôte et le dépôt dans sa route.
    /// Rend le nombre d'appels.
    pub(crate) fn appels_bien_partis(&self) -> usize {
        let appels = self.gh.appels();
        for (n, appel) in (1..).zip(&appels) {
            let ici = format!("gh-{n}.ici");
            assert!(self.kraken.join(&ici).is_file(), "l'appel {n} part du dossier de Lydite : {appel}");
            assert!(!self.depot.join(&ici).exists(), "l'appel {n} ne part pas du dépôt visé : {appel}");
            let nomme = appel.ends_with(" --repo forge.test/p/d")
                || appel == "auth status --hostname forge.test"
                || appel.starts_with("api --hostname forge.test repos/p/d/");
            assert!(nomme, "l'appel {n} nomme le dépôt du distant : {appel}");
        }
        appels.len()
    }
}

/// L'identité de Kraken, `(nom, adresse)` : celle de `IDENTITE`, auteur et committer de tout ce qu'une vague commite.
pub(crate) const KRAKEN: (&str, &str) = ("lydite", "lydite@invalid");

/// Un commit posé sur `depart`, `HEAD` détaché, qui écrit ces fichiers : son sha. Il est de Kraken, comme tout ce
/// qu'une vague publie.
pub(crate) fn sur(depot: &Path, depart: &str, fichiers: &[(&str, &str)], message: &str) -> String {
    sur_par(depot, depart, fichiers, message, (KRAKEN, KRAKEN))
}

/// [`sur`], d'un `auteur` et d'un `committer` donnés, posés sur la seule commande du commit : l'environnement du test
/// n'y change rien.
pub(crate) fn sur_par(
    depot: &Path,
    depart: &str,
    fichiers: &[(&str, &str)],
    message: &str,
    (auteur, committer): ((&str, &str), (&str, &str)),
) -> String {
    git(depot, &["switch", "-q", "--detach", depart]);
    for (chemin, contenu) in fichiers {
        ecrire(depot, chemin, contenu);
    }
    git(depot, &["add", "-A"]);
    let identite = [
        ("GIT_AUTHOR_NAME", auteur.0),
        ("GIT_AUTHOR_EMAIL", auteur.1),
        ("GIT_COMMITTER_NAME", committer.0),
        ("GIT_COMMITTER_EMAIL", committer.1),
    ];
    let mut commit = Command::new("git");
    commit.current_dir(depot).args(["-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", message]);
    let sortie = commit.envs(identite).output().unwrap();
    assert!(sortie.status.success(), "git commit : {}", String::from_utf8_lossy(&sortie.stderr));
    git(depot, &["rev-parse", "HEAD"])
}

/// Ce que `gh pr view --json headRefOid,state,mergeable` rend.
pub(crate) fn vue_pr(oid: &str, etat: &str, mergeable: &str) -> String {
    format!(r#"{{"headRefOid":"{oid}","mergeable":"{mergeable}","state":"{etat}"}}"#)
}

/// Un run d'Actions, tel que `gh run list --json` le rend, en minuscules : son workflow porte son nom.
pub(crate) fn run(nom: &str, status: &str, conclusion: &str) -> String {
    run_le(nom, nom, (status, conclusion), ("", ""))
}

/// Un run daté d'un workflow : `(cree, debut)` sont son `createdAt` et son `startedAt`, vides s'ils manquent. Pour ce
/// qui n'a pas commencé, `gh` écrit l'instant nul `0001-01-01T00:00:00Z`.
pub(crate) fn run_le(
    workflow: &str,
    nom: &str,
    (status, conclusion): (&str, &str),
    (cree, debut): (&str, &str),
) -> String {
    format!(
        concat!(
            r#"{{"conclusion":"{conclusion}","createdAt":"{cree}","name":"{nom}","startedAt":"{debut}","status":"{st"#,
            r#"atus}","workflowName":"{workflow}"}}"#,
        ),
        conclusion = conclusion,
        cree = cree,
        nom = nom,
        debut = debut,
        status = status,
        workflow = workflow,
    )
}

/// Une liste JSON de ces éléments.
fn liste(elements: &[String]) -> String {
    format!("[{}]", elements.join(","))
}

/// Ce que `gh api repos/<p>/<d>/commits/<sha>/status` rend : ces statuts dans `statuses`.
pub(crate) fn statuts_du_commit(statuts: &[String]) -> String {
    format!(r#"{{"state":"pending","statuses":{},"total_count":{}}}"#, liste(statuts), statuts.len())
}

/// Le faux `gh` rend un tour de CI à ses appels `n` à `n + 2` : la PR ouverte sur `oid` sans conflit, ces runs, puis
/// ces statuts du commit.
pub(crate) fn rendre_ci(gh: &FauxGh, n: usize, oid: &str, runs: &[String], statuts: &[String]) {
    gh.rendre(n, 0, &vue_pr(oid, "OPEN", "MERGEABLE"), "");
    gh.rendre(n + 1, 0, &liste(runs), "");
    gh.rendre(n + 2, 0, &statuts_du_commit(statuts), "");
}

/// Les trois appels d'un tour de CI de la PR 12 sur `tete` : la PR, ses runs, les statuts de son commit.
pub(crate) fn tour_de_ci(tete: &str) -> [String; 3] {
    [
        "pr view 12 --json headRefOid,state,mergeable --repo forge.test/p/d".to_string(),
        format!("run list --commit {tete} --limit 100 --json {CHAMPS_DES_RUNS} --repo forge.test/p/d"),
        format!("api --hostname forge.test repos/p/d/commits/{tete}/status?per_page=100"),
    ]
}

/// L'adresse de la PR 12, que le faux `gh` ouvre.
pub(crate) const PR: &str = "https://forge.test/p/d/pull/12";

/// Ce que `gh pr view <branche> --json number,url,headRefOid` rend de la PR 12, portée par `tete`.
pub(crate) fn pr_vue(tete: &str) -> String {
    format!(r#"{{"headRefOid":"{tete}","number":12,"url":"{PR}"}}"#)
}
