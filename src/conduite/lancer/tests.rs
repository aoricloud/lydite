use super::*;
use crate::branchements::bac::temporaire_de;
use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::fil::{Issue, Lecture, Mur, Note, etat};
use crate::essais::{depot, fausse_main, git, main_de_vague, main_hors_fichiers, racine};
use crate::essais::{ecrire, effacer, faux_srt, lancer, lancer_sur, pour_sh, script};
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// Les notes du fil du dossier de Kraken, lues par `dossier::lire` : toutes ses lignes se lisent, sans écart, et aucune
/// tâche n'y a deux mains actives (`une_main_a_la_fois`). Chaque test de vague lit son fil par elle.
fn notes(kraken: &Path) -> Vec<Note> {
    let Lecture { notes, ecarts } = crate::branchements::dossier::lire(kraken).unwrap();
    assert!(ecarts.is_empty(), "écarts du fil : {ecarts:?}");
    une_main_a_la_fois(notes.iter().map(|note| &note.evenement));
    notes
}

/// Les événements du fil du dossier de Kraken.
fn fil(kraken: &Path) -> Vec<Evenement> {
    notes(kraken).into_iter().map(|note| note.evenement).collect()
}

/// Une tâche n'a jamais deux mains actives : aucun `main.debut` d'une tâche n'en suit un autre sans sa `main.fin`
/// entre eux, ni ne suit un refus de la tâche.
fn une_main_a_la_fois<'e>(evenements: impl IntoIterator<Item = &'e Evenement>) {
    let (mut actives, mut refusees) = (BTreeSet::new(), BTreeSet::new());
    for evenement in evenements {
        match evenement {
            Evenement::Debut { vague, main, tentative } => {
                let tache = (vague.as_str(), *main);
                assert!(!refusees.contains(&tache), "main-{vague}-{main} part, tentative {tentative}, après un refus");
                let seule = actives.insert(tache);
                assert!(seule, "main-{vague}-{main} part, tentative {tentative}, avant la fin de la précédente");
            }
            Evenement::Fin { vague, main, .. } => {
                actives.remove(&(vague.as_str(), *main));
            }
            Evenement::Refus { vague, main, .. } => {
                refusees.insert((vague.as_str(), *main));
            }
            _ => {}
        }
    }
}

/// Ce que le fil raconte, sans les textes : `vague 1`, `assemblage`, `tâche a`, `bail 1`, `début 1`,
/// `fin 1 Rouge Constate`, `périmée 1 Vert Declare`, `verdict Classee Constate`, `fusion`, `refus 7007`.
fn histoire(evenements: &[Evenement]) -> Vec<String> {
    let mots = |evenement: &Evenement| match evenement {
        Evenement::Vague { n, .. } => format!("vague {n}"),
        Evenement::Assemblage { .. } => "assemblage".into(),
        Evenement::Tache { id, .. } => format!("tâche {id}"),
        Evenement::Bail { tentative, .. } => format!("bail {tentative}"),
        Evenement::Debut { tentative, .. } => format!("début {tentative}"),
        Evenement::Lancement { tentative, .. } => format!("lancement {tentative}"),
        Evenement::Constat { tentative, .. } => format!("constat {tentative}"),
        Evenement::Fin { tentative, couleur, foi, .. } => format!("fin {tentative} {couleur:?} {foi:?}"),
        Evenement::Perimee { tentative, couleur, foi, .. } => format!("périmée {tentative} {couleur:?} {foi:?}"),
        Evenement::Verdict { verdict, foi, .. } => format!("verdict {verdict:?} {foi:?}"),
        Evenement::Fusion { .. } => "fusion".into(),
        Evenement::Refus { code, .. } => format!("refus {code}"),
        Evenement::Poussee { .. } => "poussée".into(),
        Evenement::Pr { numero, .. } => format!("pr {numero}"),
        Evenement::Ci { couleur, .. } => format!("ci {couleur:?}"),
        Evenement::RefusForge { code, .. } => format!("refus de forge {code}"),
        Evenement::Rangement { .. } => "rangement".into(),
        Evenement::Decoupage { taches, .. } => format!("découpage {taches}"),
        Evenement::Joue { .. } => "joue".into(),
        Evenement::Relecture { n, bloquantes, .. } => format!("relecture {n} {bloquantes}"),
        Evenement::Correction { .. } => "correction".into(),
        Evenement::Relecteur { .. } => "relecteur".into(),
        Evenement::Fournisseur { tentative, fournisseur, .. } => format!("fournisseur {tentative} {fournisseur}"),
        Evenement::Usage { tentative, modele, base, .. } => format!("usage {tentative} {modele} {base:?}"),
    };
    evenements.iter().map(mots).collect()
}

/// Les noms de ce que contient un dossier, triés.
fn noms(dir: &Path) -> Vec<String> {
    let mut noms: Vec<String> =
        fs::read_dir(dir).unwrap().map(|entree| entree.unwrap().file_name().to_string_lossy().into_owned()).collect();
    noms.sort();
    noms
}

const DELAI: Duration = Duration::from_secs(30);

/// Le bail des tests : bien au-delà de ce que prend une fausse main.
const BAIL: Duration = Duration::from_secs(600);

/// Un plan, lu dans ses lignes.
fn plan(lignes: &[&str]) -> Plan {
    Plan::lire(lignes.join("\n").as_bytes()).unwrap()
}

/// Le nom d'un bac, celui que portent les fichiers de ses fausses mains.
fn nom(bac: &Path) -> String {
    bac.file_name().map(|nom| nom.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Une fausse main d'attente : elle garde sa consigne dans `<bac>.txt`, pose `<bac>.marque`, puis attend
/// `<bac>.libre`, que le test pose quand il a agi ; libérée, elle écrit `sortie de <bac>` et finit sur
/// `PREUVE: vert`. Elle n'attend pas plus de 120 s, puis sort en erreur : un test qui tombe ne laisse sa main que
/// 120 s au plus. Chaque fichier porte le nom de son bac, et elle n'agit que dans son bac.
fn main_d_attente(dir: &Path) -> Vec<String> {
    if cfg!(windows) {
        let script = dir.join("attente.bat");
        fs::write(
            &script,
            "@echo off\r\n\
             for %%I in (.) do set bac=%%~nxI\r\n\
             findstr \"^\" > \"%bac%.txt\"\r\n\
             type nul > \"%bac%.marque\"\r\n\
             set n=0\r\n\
             :attendre\r\n\
             if exist \"%bac%.libre\" goto libre\r\n\
             set /a n+=1\r\n\
             if %n% geq 115 (echo %bac% pas liberee& exit 1)\r\n\
             ping -n 2 127.0.0.1 >nul\r\n\
             goto attendre\r\n\
             :libre\r\n\
             echo sortie de %bac%\r\n\
             echo PREUVE: vert\r\n",
        )
        .unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("attente.sh");
        fs::write(
            &script,
            "bac=$(basename \"$(pwd)\")\n\
             cat > \"$bac.txt\"\n\
             : > \"$bac.marque\"\n\
             n=0\n\
             while [ ! -f \"$bac.libre\" ]; do\n\
             n=$((n + 1))\n\
             if [ \"$n\" -ge 1150 ]; then echo \"$bac pas liberee\" >&2; exit 1; fi\n\
             sleep 0.1\n\
             done\n\
             echo \"sortie de $bac\"\n\
             echo 'PREUVE: vert'\n",
        )
        .unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Les bacs de la vague dont la main d'attente a posé sa marque, triés.
fn marques(kraken: &Path) -> Vec<PathBuf> {
    let Ok(bacs) = fs::read_dir(kraken.join("bacs")) else { return Vec::new() };
    let mut marques: Vec<PathBuf> = bacs
        .filter_map(|entree| entree.ok().map(|entree| entree.path()))
        .filter(|bac| bac.join(format!("{}.marque", nom(bac))).is_file())
        .collect();
    marques.sort();
    marques
}

/// Libère la main d'attente de ce bac.
fn liberer(bac: &Path) {
    fs::write(bac.join(format!("{}.libre", nom(bac))), "").unwrap();
}

/// Libère chaque main d'attente qui se pose, jusqu'à la fin de la vague : le dernier geste d'un test qui a posé ses
/// obstacles. Une vague qui s'écarte de ce que le test attend finit alors vite, et ses assertions le disent, au lieu de
/// laisser une main attendre ses 115 s.
fn tout_liberer<T>(kraken: &Path, vague: &thread::ScopedJoinHandle<'_, T>) {
    while !vague.is_finished() {
        for bac in marques(kraken) {
            if !bac.join(format!("{}.libre", nom(&bac))).exists() {
                liberer(&bac);
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Attend que `fait` soit vrai, en le regardant toutes les 50 ms, tant que la vague tourne et 120 s au plus : sinon
/// le test tombe, sur `quoi`.
fn attendre<T>(quoi: &str, vague: &thread::ScopedJoinHandle<'_, T>, mut fait: impl FnMut() -> bool) {
    let debut = Instant::now();
    while !fait() {
        assert!(!vague.is_finished(), "la vague a fini sans {quoi}");
        assert!(debut.elapsed() < Duration::from_secs(120), "120 s sans {quoi}");
        thread::sleep(Duration::from_millis(50));
    }
}

/// La fausse main des conflits : elle écrit `commun.txt`, le même chemin dans chaque bac, avec le nom de son bac pour
/// contenu, puis finit sur `PREUVE: vert`. Deux bacs partis de la même base qui la jouent se heurtent à la fusion, et
/// chacun garde le sien. Elle n'agit que dans son bac.
fn main_commune(dir: &Path) -> Vec<String> {
    if cfg!(windows) {
        let script = dir.join("commune.bat");
        fs::write(
            &script,
            "@echo off\r\n\
             for %%I in (.) do set bac=%%~nxI\r\n\
             >commun.txt echo %bac%\r\n\
             echo PREUVE: vert\r\n",
        )
        .unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("commune.sh");
        fs::write(&script, "bac=$(basename \"$(pwd)\")\necho \"$bac\" > commun.txt\necho 'PREUVE: vert'\n").unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Une preuve de vague verte sur la base et rouge dès qu'un `commun.txt` entre dans la branche : une tâche dont la main
/// l'écrit, et dont la propre preuve est verte, la rend rouge en s'assemblant. Elle ne lance pas git.
const SANS_COMMUN: &str = if cfg!(windows) {
    "if exist commun.txt (echo commun.txt vu& exit 1) else (echo vert)"
} else {
    "if [ -f commun.txt ]; then echo commun.txt vu; exit 1; fi; echo vert"
};

/// Une preuve de tâche rouge la première fois qu'elle se joue dans un bac, verte ensuite : la première pose `marque`
/// dans le bac et sort en 1, les suivantes la voient et sortent en 0. Une relance la fait passer.
const ROUGE_PUIS_VERTE: &str = if cfg!(windows) {
    "if exist marque (exit 0) else (type nul > marque & exit 1)"
} else {
    "if [ -f marque ]; then exit 0; fi; : > marque; exit 1"
};

/// La main dont parle un événement : `None` pour l'ouverture et l'assemblage de la vague, pour ses notes de forge et
/// pour ses liens de M5 (découpage, relecture, correction).
fn main_de(evenement: &Evenement) -> Option<usize> {
    match evenement {
        Evenement::Vague { .. }
        | Evenement::Assemblage { .. }
        | Evenement::Poussee { .. }
        | Evenement::Pr { .. }
        | Evenement::Ci { .. }
        | Evenement::RefusForge { .. }
        | Evenement::Rangement { .. }
        | Evenement::Decoupage { .. }
        | Evenement::Joue { .. }
        | Evenement::Relecture { .. }
        | Evenement::Correction { .. }
        | Evenement::Relecteur { .. } => None,
        Evenement::Tache { main, .. }
        | Evenement::Bail { main, .. }
        | Evenement::Debut { main, .. }
        | Evenement::Lancement { main, .. }
        | Evenement::Fournisseur { main, .. }
        | Evenement::Usage { main, .. }
        | Evenement::Constat { main, .. }
        | Evenement::Fin { main, .. }
        | Evenement::Perimee { main, .. }
        | Evenement::Verdict { main, .. }
        | Evenement::Fusion { main, .. }
        | Evenement::Refus { main, .. } => Some(*main),
    }
}

/// Ce que le fil raconte d'une seule main, la tâche `k` du plan (voir `histoire`).
fn histoire_de(evenements: &[Evenement], k: usize) -> Vec<String> {
    let siens: Vec<Evenement> = evenements.iter().filter(|evenement| main_de(evenement) == Some(k)).cloned().collect();
    histoire(&siens)
}

/// Ce que le fil raconte d'une tâche prise du premier coup : sa tâche, un bail, un début, une fin verte et un verdict
/// de prise, puis `suite` (`fusion`, ou le refus qui la suit).
fn prise_du_premier_coup(id: &str, suite: &str) -> Vec<String> {
    let mut histoire = vec![format!("tâche {id}")];
    histoire.extend(["bail 1", "début 1", "fin 1 Vert Constate", "verdict Prise Constate", suite].map(String::from));
    histoire
}

/// Les fusions du fil, dans l'ordre où il les note : la main, et le sha de la tête qu'elle a faite.
fn fusions(evenements: &[Evenement]) -> Vec<(usize, String)> {
    let fusion = |evenement: &Evenement| match evenement {
        Evenement::Fusion { main, sha, .. } => Some((*main, sha.clone())),
        _ => None,
    };
    evenements.iter().filter_map(fusion).collect()
}

/// L'ordre des mains `ordre` (de 1 à n) est un ordre topologique du plan : chaque tâche y vient après toutes ses
/// dépendances, et toutes y sont.
fn est_topologique(plan: &Plan, ordre: &[usize]) -> bool {
    let rang = |k: usize| ordre.iter().position(|&main| main == k);
    let tout = (1..=plan.taches.len()).all(|k| rang(k).is_some());
    tout && (1..).zip(&plan.taches).all(|(k, tache)| {
        tache.depend.iter().all(|id| {
            let dependance = 1 + plan.taches.iter().position(|autre| autre.id == *id).unwrap();
            rang(dependance) < rang(k)
        })
    })
}

/// Les fichiers d'un commit, triés.
fn fichiers(depot: &Path, commit: &str) -> Vec<String> {
    git(depot, &["ls-tree", "-r", "--name-only", commit]).lines().map(str::to_string).collect()
}

/// Le bac de la tâche `k` de la vague `vague`.
fn bac_de(kraken: &Path, vague: &str, k: usize) -> PathBuf {
    kraken.join("bacs").join(format!("main-{vague}-{k}"))
}

/// L'id de la vague que le fil du dossier de Kraken ouvre : le lot dont sont faits les ids de ses mains.
fn vague_du_fil(kraken: &Path) -> String {
    fil(kraken).first().map(|evenement| evenement.vague().to_string()).expect("la vague est ouverte au fil")
}

/// Lance le plan dans un fil d'exécution à part, joue `pendant` tant qu'il tourne, puis rend ce que `lancer` rend.
/// `pendant` regarde le dépôt, le fil et les bacs, pose ses obstacles et libère les mains d'attente, qu'il attend par
/// `attendre` sur le fil de la vague.
fn pendant_la_vague(
    depot: &Path,
    kraken: &Path,
    profil: &Profil,
    plan: &Plan,
    pendant: impl FnOnce(&thread::ScopedJoinHandle<'_, Result<(Option<String>, Issues), Refus>>),
) -> (Option<String>, Issues) {
    thread::scope(|s| {
        let vague = s.spawn(|| lancer(depot, kraken, profil, plan, BAIL));
        pendant(&vague);
        vague.join().unwrap()
    })
    .unwrap()
}

/// L'issue de la tâche `id` est son refus en clair de ce code (voir `refus_en_clair`) : rend son texte.
fn refusee<'a>(id: &str, issue: &'a Result<Pris, String>, code: u16) -> &'a str {
    let texte = issue.as_ref().err().unwrap_or_else(|| panic!("{id} est refusée en {code} : {issue:?}"));
    assert!(texte.starts_with(&format!("refusée ({code}) : ")), "{id} est refusée en {code} : {texte}");
    texte
}

/// L'issue de la tâche `id` est sa prise, assemblée sur une preuve que Kraken a rejouée : rend la prise.
fn assemblee<'a>(id: &str, issue: &'a Result<Pris, String>) -> &'a Pris {
    let pris = issue.as_ref().unwrap_or_else(|erreur| panic!("{id} est assemblée : {erreur}"));
    assert!(pris.prouve, "{id} est prise sur une preuve rejouée : {pris:?}");
    pris
}

/// Les chemins sous `dir`, à toute profondeur, dont le nom satisfait `garde`.
fn trouves(dir: &Path, garde: &dyn Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut vus = Vec::new();
    for entree in fs::read_dir(dir).unwrap() {
        let chemin = entree.unwrap().path();
        if garde(&nom(&chemin)) {
            vus.push(chemin.clone());
        }
        if chemin.is_dir() {
            vus.extend(trouves(&chemin, garde));
        }
    }
    vus
}

/// Ce qu'une vague ne laisse jamais, vague finie. Le dépôt visé n'a pas bougé : son HEAD est `base`, son arbre est
/// propre et ne contient que `.git`, ses seules références sont sa branche et celles de `kraken/`, aucun verrou ne
/// reste sous `.git`. Le dossier de Kraken ne garde que `bacs` et `fil.jsonl`. Aucun journal de preuve, `verif-*.tmp`,
/// ne reste nulle part sous la racine du test.
fn rien_derriere(racine: &Path, base: &str) {
    let (depot, kraken) = (racine.join("depot"), racine.join("kraken"));
    assert_eq!(git(&depot, &["rev-parse", "HEAD"]), base, "le HEAD du dépôt visé n'a pas bougé");
    assert_eq!(git(&depot, &["status", "--porcelain", "--untracked-files=all"]), "", "le dépôt visé est propre");
    assert_eq!(noms(&depot), [".git"], "rien dans le dépôt visé, hors ce que git worktree add pose sous .git");
    assert_eq!(noms(&kraken), ["bacs", "fil.jsonl"], "le fil et les bacs, et plus de fichier de vérification");
    let sienne = git(&depot, &["symbolic-ref", "HEAD"]);
    for reference in git(&depot, &["for-each-ref", "--format=%(refname)"]).lines() {
        let a_kraken = reference.starts_with("refs/heads/kraken/");
        assert!(reference == sienne || a_kraken, "une référence que Kraken n'a pas à poser : {reference}");
    }
    let journaux = trouves(racine, &|nom| nom.starts_with("verif-") && nom.ends_with(".tmp"));
    assert!(journaux.is_empty(), "aucun journal de preuve ne reste : {journaux:?}");
    let verrous = trouves(&depot.join(".git"), &|nom| nom.ends_with(".lock"));
    assert!(verrous.is_empty(), "aucun verrou ne reste sous .git : {verrous:?}");
}

/// Le worktree de la vague, à la tête de `branche` : détaché sur elle, propre, sans fusion en cours.
fn worktree_a_la_tete(depot: &Path, kraken: &Path, branche: &str) {
    let vague = branche.strip_prefix("kraken/vague-").expect("la branche d'une vague");
    let worktree = kraken.join("bacs").join(format!("vague-{vague}"));
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), git(depot, &["rev-parse", branche]), "à la tête");
    assert_eq!(git(&worktree, &["branch", "--show-current"]), "", "détaché");
    assert_eq!(git(&worktree, &["status", "--porcelain", "--untracked-files=all"]), "", "propre");
    let fusion =
        Command::new("git").current_dir(&worktree).args(["rev-parse", "-q", "--verify", "MERGE_HEAD"]).output();
    assert!(!fusion.unwrap().status.success(), "aucune fusion en cours");
}

/// Rend le dépôt visé hostile, par sa configuration locale : chaque crochet que les gestes de la vague pourraient jouer
/// ajoute son nom au témoin des crochets (`pre-commit` et `pre-merge-commit` sortent en 1) ; `core.fsmonitor` nomme un
/// programme qui écrit dans le témoin du moniteur ; `commit.gpgsign` vaut true, avec un `gpg.program` absent ;
/// `user.name` et `user.email` nomment un autre que Kraken. Rend les deux témoins.
fn hostile(racine: &Path, depot: &Path) -> (PathBuf, PathBuf) {
    let (crochets, temoin) = (depot.join(".git").join("hooks"), racine.join("crochets.txt"));
    let (programme, du_moniteur) = (racine.join("fsmonitor.sh"), racine.join("fsmonitor.txt"));
    let tous = [
        "pre-commit",
        "prepare-commit-msg",
        "commit-msg",
        "post-commit",
        "pre-merge-commit",
        "post-merge",
        "post-checkout",
        "reference-transaction",
    ];
    for crochet in tous {
        let fin = if matches!(crochet, "pre-commit" | "pre-merge-commit") { "\nexit 1" } else { "" };
        script(&crochets.join(crochet), &format!("echo {crochet} >> '{}'{fin}", pour_sh(&temoin)));
    }
    script(&programme, &format!("echo fsmonitor >> '{}'", pour_sh(&du_moniteur)));
    let reglages = [
        ("core.hooksPath", pour_sh(&crochets)),
        ("core.fsmonitor", pour_sh(&programme)),
        ("commit.gpgsign", "true".into()),
        ("gpg.program", pour_sh(&racine.join("gpg-absent"))),
        ("user.name", "autre".into()),
        ("user.email", "autre@exemple.invalid".into()),
    ];
    for (cle, valeur) in reglages {
        git(depot, &["config", cle, &valeur]);
    }
    (temoin, du_moniteur)
}

#[test]
fn rouge_puis_vert_sans_verifier_est_non_prouve() {
    let racine = racine("pris");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(fausse_main(&racine), DELAI);
    let (id, verdict) = lancer(&depot, &kraken, &profil, &Plan::copies("objectif", None, 1), BAIL).unwrap().1.remove(0);
    let pris = verdict.unwrap();
    assert!(!pris.prouve);
    let notes = notes(&kraken);
    let evenements: Vec<Evenement> = notes.iter().map(|note| note.evenement.clone()).collect();
    let attendu = [
        "vague 1",
        "bail 1",
        "début 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "fin 2 Vert Declare",
        "verdict Prise Declare",
    ];
    assert_eq!(histoire(&evenements), attendu);
    // La fin déclarée, ligne de preuve de la main, porte l'origine de la main ; les autres, celle de la séance.
    // Toutes sont déclarées.
    let origines: Vec<(&str, Foi)> = notes.iter().map(|note| (note.origine.as_str(), note.origine_foi)).collect();
    let (humain, main) = (("humain", Foi::Declare), format!("agent:{id}"));
    assert_eq!(origines, [humain, humain, humain, humain, humain, humain, (main.as_str(), Foi::Declare), humain]);
    let entree = fs::read_to_string(kraken.join("bacs").join(&id).join("entree.txt")).unwrap();
    assert!(entree.contains("Tentative pr"), "la relance porte l'erreur : {entree}");
    let vagues = etat(&evenements);
    assert_eq!(vagues[0].mains[&1].texte(), pris.sortie, "la sortie prise est celle du fil");
    git(&depot, &["rev-parse", "-q", "--verify", &format!("kraken/{id}")]);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn verifier_vert_est_prouve() {
    let racine = racine("verifie");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(fausse_main(&racine), DELAI);
    let verifier = Some("git status");
    let plan = Plan::copies("objectif", verifier, 1);
    let (id, verdict) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap().1.remove(0);
    assert!(verdict.unwrap().prouve);
    let evenements = fil(&kraken);
    let attendu = ["vague 1", "bail 1", "début 1", "fin 1 Vert Constate", "verdict Prise Constate"];
    assert_eq!(histoire(&evenements), attendu, "Kraken juge, pas la ligne PREUVE de la main");
    let entree = fs::read_to_string(kraken.join("bacs").join(&id).join("entree.txt")).unwrap();
    assert!(entree.contains("git status"), "la main connaît le test : {entree}");
    assert!(!entree.contains("Tentative pr"), "la première consigne ne porte pas d'erreur : {entree}");
    let Evenement::Fin { verif, .. } = &evenements[3] else { panic!("{evenements:?}") };
    let dit = verif.as_deref().is_some_and(|verif| verif.contains(&format!("kraken/{id}")));
    assert!(dit, "la fin de la vérification est au fil : {verif:?}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn verifier_rouge_est_un_echec() {
    let racine = racine("rouge");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let rouge = "echo oups && exit 3";
    let profil = Profil::faux(fausse_main(&racine), DELAI);
    let verifier = Some(rouge);
    let plan = Plan::copies("objectif", verifier, 1);
    let (id, verdict) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap().1.remove(0);
    let echec = verdict.unwrap_err();
    assert!(echec.contains(&format!("vérification « {rouge} » rouge")), "{echec}");
    let evenements = fil(&kraken);
    let attendu = [
        "vague 1",
        "bail 1",
        "début 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "fin 2 Rouge Constate",
        "verdict Classee Constate",
    ];
    assert_eq!(histoire(&evenements), attendu);
    let vagues = etat(&evenements);
    let derniere = &vagues[0].mains[&1].tentatives[&2];
    assert_eq!(derniere.texte, echec, "l'erreur classée est celle du fil");
    assert_eq!(derniere.verif.as_deref().map(str::trim), Some("oups"), "la fin de la vérification rouge est au fil");
    let entree = fs::read_to_string(kraken.join("bacs").join(&id).join("entree.txt")).unwrap();
    assert!(entree.contains(&format!("vérification « {rouge} » rouge")), "la relance porte l'erreur : {entree}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn temps_depasse_est_un_echec() {
    let racine = racine("lent");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let lente = if cfg!(windows) { "ping -n 3 127.0.0.1 >nul" } else { "sleep 2" };
    let delai = Duration::from_millis(200);
    let profil = Profil::faux(fausse_main(&racine), delai);
    let verifier = Some(lente);
    let plan = Plan::copies("objectif", verifier, 1);
    let (_, verdict) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap().1.remove(0);
    assert!(verdict.unwrap_err().contains("temps dépassé"));
    fil(&kraken);
    effacer(&racine);
}

#[test]
fn deux_echecs_sont_classes() {
    let racine = racine("echec");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let introuvable = Profil::faux(vec!["kraken-main-introuvable".into()], DELAI);
    let plan = Plan::copies("objectif", None, 1);
    let (_, verdict) = lancer(&depot, &kraken, &introuvable, &plan, BAIL).unwrap().1.remove(0);
    let erreur = verdict.unwrap_err();
    let evenements = fil(&kraken);
    let attendu = [
        "vague 1",
        "bail 1",
        "début 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "fin 2 Rouge Constate",
        "verdict Classee Constate",
    ];
    assert_eq!(histoire(&evenements), attendu);
    let vagues = etat(&evenements);
    assert_eq!(vagues[0].mains[&1].verdict, Some((Verdict::Classee, Foi::Constate)));
    assert_eq!(vagues[0].mains[&1].texte(), erreur, "l'erreur classée est celle du fil");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn trois_mains_trois_bacs_du_depot_vise() {
    let racine = racine("trois");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(fausse_main(&racine), DELAI);
    let verifier = Some("git status");
    let mains = lancer(&depot, &kraken, &profil, &Plan::copies("objectif", verifier, 3), BAIL).unwrap().1;
    assert_eq!(mains.len(), 3);
    for (id, verdict) in &mains {
        assert!(verdict.as_ref().unwrap().prouve);
        let bac = kraken.join("bacs").join(id);
        let commun = PathBuf::from(git(&bac, &["rev-parse", "--path-format=absolute", "--git-common-dir"]));
        assert_eq!(fs::canonicalize(commun).unwrap(), fs::canonicalize(depot.join(".git")).unwrap());
    }
    // Une ouverture, puis un bail, un début, une fin et un verdict par main : les trois mains notent en même temps.
    assert_eq!(fil(&kraken).len(), 1 + 3 * 4);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_depot_vise_ne_voit_rien() {
    let racine = racine("hors");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(fausse_main(&racine), DELAI);
    let verifier = Some("git status");
    let mains = lancer(&depot, &kraken, &profil, &Plan::copies("objectif", verifier, 2), BAIL).unwrap().1;
    assert!(!depot.join(".kraken").exists());
    assert_eq!(git(&depot, &["status", "--porcelain"]), "");
    assert_eq!(noms(&depot), [".git"], "rien dans le dépôt visé, hors ce que git worktree add pose sous .git");
    for (id, _) in &mains {
        let bac = kraken.join("bacs").join(id);
        assert!(bac.is_dir() && !bac.starts_with(&depot), "le bac est hors du dépôt visé : {}", bac.display());
    }
    assert_eq!(noms(&kraken), ["bacs", "fil.jsonl"], "le fil et les bacs, et plus de fichier de vérification");
    fil(&kraken);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_fil_rejoue_main_par_main_ce_que_lancer_a_rendu() {
    let racine = racine("rejeu");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let fausse = Profil::faux(fausse_main(&racine), DELAI);
    // Trois lancements vers un même dossier de Kraken, donc un même fil : prises constatées, prises déclarées,
    // classées. Deux mains chacun.
    let lancements = [
        (Some("git status"), (Verdict::Prise, Foi::Constate)),
        (None, (Verdict::Prise, Foi::Declare)),
        (Some("exit 3"), (Verdict::Classee, Foi::Constate)),
    ];
    let rendus: Vec<_> = lancements
        .iter()
        .map(|&(verifier, _)| lancer(&depot, &kraken, &fausse, &Plan::copies("objectif", verifier, 2), BAIL).unwrap().1)
        .collect();
    let vagues = etat(&fil(&kraken));
    assert_eq!(vagues.len(), 3);
    for ((vague, rendu), (verifier, attendu)) in vagues.iter().zip(&rendus).zip(lancements) {
        assert_eq!((vague.n, vague.verifier.as_deref(), vague.objectif.as_str()), (2, verifier, "objectif"));
        assert_eq!(vague.depot, depot.to_string_lossy());
        let rejoue: Vec<_> = vague
            .mains
            .iter()
            .map(|(n, main)| (format!("main-{}-{n}", vague.id), main.verdict, main.texte()))
            .collect();
        let rendu: Vec<_> = rendu
            .iter()
            .map(|(id, issue)| match issue {
                Ok(Pris { sortie, prouve: true }) => {
                    (id.clone(), Some((Verdict::Prise, Foi::Constate)), sortie.as_str())
                }
                Ok(Pris { sortie, prouve: false }) => {
                    (id.clone(), Some((Verdict::Prise, Foi::Declare)), sortie.as_str())
                }
                Err(erreur) => (id.clone(), Some((Verdict::Classee, Foi::Constate)), erreur.as_str()),
            })
            .collect();
        assert_eq!(rejoue, rendu);
        assert!(rendu.iter().all(|(_, verdict, _)| *verdict == Some(attendu)), "{attendu:?} attendu : {rendu:?}");
    }
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_dossier_impossible_nomme_son_chemin() {
    let racine = racine("pose");
    let depot = depot(&racine);
    // Le dossier de Kraken sous un fichier : il ne peut pas se créer.
    let fichier = racine.join("fichier");
    fs::write(&fichier, "x").unwrap();
    let kraken = fichier.join("kraken");
    let profil = Profil::faux(vec!["x".into()], DELAI);
    let refus = lancer(&depot, &kraken, &profil, &Plan::copies("objectif", None, 1), BAIL).unwrap_err();
    assert_eq!(refus.code, Code::EntreeSortie);
    assert!(refus.preuve.contains(&kraken.display().to_string()), "le dossier de Kraken : {refus}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn chaque_lancement_ouvre_sa_seance_numerotee_de_1() {
    let racine = racine("seances");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(fausse_main(&racine), DELAI);
    // Deux lancements vers le même dossier de Kraken, donc vers le même fil : deux vagues, deux séances.
    for _ in 0..2 {
        lancer(&depot, &kraken, &profil, &Plan::copies("objectif", Some("git status"), 2), BAIL).unwrap();
    }
    let mut seances: BTreeMap<String, (Vec<u64>, BTreeSet<String>)> = BTreeMap::new();
    for note in notes(&kraken) {
        let vague = note.evenement.vague().to_string();
        let (seqs, vagues) = seances.entry(note.seance).or_default();
        seqs.push(note.seq);
        vagues.insert(vague);
    }
    assert_eq!(seances.len(), 2, "une séance par lancement");
    let mut nommees = BTreeSet::new();
    for (seance, (seqs, vagues)) in &seances {
        // Une ouverture, puis un bail, un début, une fin et un verdict par main : les deux mains notent au même
        // compteur.
        assert_eq!(seqs, &(1..=9).collect::<Vec<_>>(), "séance {seance} : de 1 à ses neuf notes, sans trou");
        assert_eq!(vagues.len(), 1, "séance {seance} : une seule vague");
        assert!(!vagues.contains(seance), "la séance n'est pas la vague");
        nommees.extend(vagues.iter().cloned());
    }
    assert_eq!(nommees.len(), 2, "et chaque séance a la sienne");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_fil_injoignable_garde_la_sortie_de_chaque_main() {
    let racine = racine("injoignable");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let (chemin, cote) = (kraken.join("fil.jsonl"), racine.join("cote"));
    let plan = Plan::copies("objectif", None, 3);
    // Trois copies partent. Quand les trois mains attendent, le fil se casse (un dossier prend la place du fichier),
    // puis elles sont libérées : la première note qui suit rate.
    let (avant, mains) = thread::scope(|s| {
        let vague = s.spawn(|| lancer(&depot, &kraken, &profil, &plan, BAIL));
        attendre("les trois marques", &vague, || marques(&kraken).len() == 3);
        fs::rename(&chemin, &cote).unwrap();
        fs::create_dir(&chemin).unwrap();
        let avant = fs::read(&cote).unwrap();
        for bac in marques(&kraken) {
            liberer(&bac);
        }
        (avant, vague.join().unwrap())
    });
    let mains = mains.unwrap().1;
    // Trois issues, chacune avec la sortie de sa main, après le refus qui a arrêté la vague, dont la preuve nomme le
    // fil.
    assert_eq!(mains.len(), 3);
    for (id, issue) in &mains {
        let texte = issue.as_ref().unwrap_err();
        assert!(texte.starts_with("refusée (7012) : "), "le refus qui arrête la vague : {texte}");
        assert!(texte.contains(&chemin.display().to_string()), "sa preuve nomme le fil : {texte}");
        assert!(texte.contains(&format!("sortie de {id}")), "la sortie de sa main : {texte}");
    }
    // Remis en place, le fil ne porte que les notes d'avant la casse : ni ligne, ni bout de ligne de plus.
    fs::remove_dir(&chemin).unwrap();
    fs::rename(&cote, &chemin).unwrap();
    assert!(fs::read(&chemin).unwrap() == avant, "rien n'est écrit au fil après la casse");
    let attendu = ["vague 3", "bail 1", "début 1", "bail 1", "début 1", "bail 1", "début 1"];
    assert_eq!(histoire(&fil(&kraken)), attendu);
    let _ = fs::remove_dir_all(&racine);
}

/// Un profil `claude` lu de ce texte, comme un fichier d'un dossier de Kraken fictif, et borné par `kraken`, le git
/// commun de `depot` et deux leurres de dossiers de connexion de Claude à côté de `kraken`, jamais lus : ce que
/// `preparer` en fait. Sans `programme` écrit, c'est `claude` ou `KRAKEN_CLAUDE` : les textes de ces tests nomment un
/// programme qui n'existe pas, car aucun test ne lance un vrai `claude`.
fn profil_declare(texte: &str, kraken: &Path, depot: &Path) -> Profil {
    let mut profil = crate::coeur::profil::lire("essai", texte, None, None).unwrap();
    let (config, personnel) = (kraken.with_file_name("config"), kraken.with_file_name("personnel"));
    profil.borner(kraken, &depot.join(".git"), Some(&config), Some(&personnel)).unwrap();
    profil
}

/// Un programme qui dort 30 s sans rien répondre, que le système lance lui-même : le `programme` d'un profil
/// `claude`, qui lui passe ses arguments, qu'il ignore. Rend son chemin.
fn main_qui_dort(dir: &Path) -> PathBuf {
    let (nom, corps) = if cfg!(windows) {
        ("dort.cmd", "@echo off\r\nping -n 31 127.0.0.1 >nul\r\n")
    } else {
        ("dort.sh", "#!/bin/sh\nsleep 30\n")
    };
    let chemin = dir.join(nom);
    fs::write(&chemin, corps).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&chemin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    chemin
}

#[test]
fn une_main_declaree_est_precedee_de_son_lancement_et_suivie_de_son_constat() {
    let racine = racine("declaree");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    // Un réglage versionné dans le dépôt visé : chaque bac l'a au départ, et son empreinte entre au lancement.
    let reglages = r#"{"model":"opus"}"#;
    fs::create_dir(depot.join(".claude")).unwrap();
    fs::write(depot.join(".claude").join("settings.json"), reglages).unwrap();
    git(&depot, &["add", ".claude/settings.json"]);
    git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "réglages"]);
    // La main se déclare, mais son programme n'existe pas : elle ne part pas, et Kraken n'en lit rien.
    let texte = "sorte = claude\nprogramme = kraken-claude-introuvable\nmodele = opus\nrelance = haiku\nmur = exige\n";
    let mut profil = profil_declare(texte, &kraken, &depot);
    // Un faux srt, qui tient le mur : le test ne dépend pas du srt de la machine.
    profil.avec_srt(faux_srt(&racine, true));
    let verifier = Some("git status");
    let plan = Plan::copies("objectif", verifier, 1);
    let (id, issue) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap().1.remove(0);
    let erreur = issue.unwrap_err();
    assert!(erreur.starts_with("lancement impossible de kraken-claude-introuvable"), "{erreur}");
    // Dans chaque tentative : le bail et le début, le lancement avant que la main parte, le constat à son retour,
    // puis la fin.
    let notes = notes(&kraken);
    let evenements: Vec<Evenement> = notes.iter().map(|note| note.evenement.clone()).collect();
    let attendu = [
        "vague 1",
        "bail 1",
        "début 1",
        "lancement 1",
        "constat 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "lancement 2",
        "constat 2",
        "fin 2 Rouge Constate",
        "verdict Classee Constate",
    ];
    assert_eq!(histoire(&evenements), attendu);
    // Le lancement dit ce que le profil déclare pour cette tentative : le modèle de la tentative, le mur exigé, le
    // JSON exact de `--settings` dans ce bac, et l'empreinte des réglages que le bac porte.
    let Evenement::Vague { vague, .. } = &evenements[0] else { panic!("{evenements:?}") };
    let bac = kraken.join("bacs").join(&id);
    let depot_reglages = format!(".claude/settings.json {}", hex(&sha256(reglages.as_bytes())));
    for (place, tentative, modele) in [(3, 1, "opus"), (8, 2, "haiku")] {
        let declaration = profil.demande(&bac, verifier, tentative > 1).declaration.expect("une main déclarée");
        let lancement = Evenement::Lancement {
            vague: vague.clone(),
            main: 1,
            tentative,
            profil: "essai".into(),
            empreinte: hex(&sha256(texte.as_bytes())),
            modele: modele.into(),
            mur: Mur::Exige,
            reglages: declaration.reglages,
            reglages_depot: depot_reglages.clone(),
        };
        assert_eq!(evenements[place], lancement);
        // Un constat vide : rien n'a été lu d'une main qui n'est pas partie. Il n'est pas une preuve de main.
        let constat = Evenement::Constat { vague: vague.clone(), main: 1, tentative, version: None, mur_actif: None };
        assert_eq!(evenements[place + 1], constat);
        for note in &notes[place..=place + 1] {
            assert_eq!((note.origine.as_str(), note.origine_foi), ("humain", Foi::Declare));
        }
    }
    // Rejoué : le mur de la dernière tentative est celui que le profil demandait, et rien ne l'a constaté.
    let derniere = &etat(&evenements)[0].mains[&1].tentatives[&2];
    assert_eq!((derniere.mur, derniere.mur_actif), (Some(Mur::Exige), None));
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_main_dont_le_profil_nomme_un_fournisseur_le_note_apres_son_lancement() {
    let racine = racine("fournisseur");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    // Comme la main déclarée plus haut, celle-ci ne part pas : son programme n'existe pas. Sa clé se lit dans un
    // environnement fictif, jamais dans celui du test.
    let api = "https://api.exemple.test:8443/anthropic";
    let texte = format!("sorte = claude\nprogramme = kraken-claude-introuvable\napi = {api}\ncle = CLE_ESSAI\n");
    let leurre = "leurre-de-la-cle-4f9a";
    let mut profil = profil_declare(&texte, &kraken, &depot);
    profil.fournir(|nom| (nom == "CLE_ESSAI").then(|| std::ffi::OsString::from(leurre))).unwrap();
    let plan = Plan::copies("objectif", Some("git status"), 1);
    let (_, mut issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let erreur = issues.remove(0).1.unwrap_err();
    assert!(erreur.starts_with("lancement impossible de kraken-claude-introuvable"), "{erreur}");
    // À chaque tentative, le fournisseur déclaré suit le lancement et précède le constat.
    let notes = notes(&kraken);
    let evenements: Vec<Evenement> = notes.iter().map(|note| note.evenement.clone()).collect();
    let attendu = [
        "vague 1",
        "bail 1",
        "début 1",
        "lancement 1",
        "fournisseur 1 api.exemple.test:8443",
        "constat 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "lancement 2",
        "fournisseur 2 api.exemple.test:8443",
        "constat 2",
        "fin 2 Rouge Constate",
        "verdict Classee Constate",
    ];
    assert_eq!(histoire(&evenements), attendu);
    // Il est déclaré, et porte l'origine de la séance comme le lancement qu'il suit.
    let Evenement::Vague { vague, .. } = &evenements[0] else { panic!("{evenements:?}") };
    for (place, tentative) in [(4, 1), (10, 2)] {
        let fournisseur = "api.exemple.test:8443".into();
        let note = Evenement::Fournisseur { vague: vague.clone(), main: 1, tentative, fournisseur };
        assert_eq!(evenements[place], note);
        assert_eq!((notes[place].origine.as_str(), notes[place].origine_foi), ("humain", Foi::Declare));
    }
    // Rejoué, c'est celui de la tentative ; l'URL et la clé ne sont pas au fil, qui ne note que l'hôte.
    let derniere = &etat(&evenements)[0].mains[&1].tentatives[&2];
    assert_eq!(derniere.fournisseur.as_deref(), Some("api.exemple.test:8443"));
    let fil = fs::read_to_string(kraken.join("fil.jsonl")).unwrap();
    assert!(!fil.contains(leurre) && !fil.contains(api), "{fil}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn des_reglages_illisibles_arretent_la_vague_et_la_main_ne_part_pas() {
    let racine = racine("illisibles");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = profil_declare("sorte = claude\nprogramme = kraken-claude-introuvable\n", &kraken, &depot);
    // Un dossier à la place de `.claude/settings.json`, au dépôt : chaque bac l'a, et il ne se lit pas comme un
    // fichier.
    let place = depot.join(".claude").join("settings.json");
    fs::create_dir_all(&place).unwrap();
    fs::write(place.join("x"), "x").unwrap();
    git(&depot, &["add", ".claude"]);
    git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "un dossier à la place"]);
    let plan = Plan::copies("objectif", None, 1);
    let (id, issue) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap().1.remove(0);
    // La vague s'arrête, comme sur une note ratée : le refus 7012, dont la preuve nomme le fichier.
    let texte = refusee(&id, &issue, 7012);
    let reglages = kraken.join("bacs").join(&id).join(".claude").join("settings.json");
    assert!(texte.contains(&reglages.display().to_string()), "la preuve nomme le fichier : {texte}");
    // Elle s'arrête au début de la tentative, sans lancement ni fin ; la main n'est pas partie : `jouer` crée son
    // dossier temporaire avant de la lancer, et il n'existe pas.
    assert_eq!(histoire(&fil(&kraken)), ["vague 1", "bail 1", "début 1"]);
    assert!(!temporaire_de(&kraken.join("bacs").join(&id)).unwrap().exists());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn hors_depot_git_pas_de_bac() {
    let racine = racine("nu");
    let (dir, kraken) = (racine.join("nu"), racine.join("kraken"));
    fs::create_dir_all(&dir).unwrap();
    let profil = Profil::faux(vec!["x".into()], DELAI);
    let plan = Plan::copies("objectif", None, 1);
    let refus = lancer(&dir, &kraken, &profil, &plan, BAIL).unwrap_err();
    assert_eq!(refus.code, Code::BacImpossible);
    assert!(!kraken.join("fil.jsonl").exists(), "sans bac, la vague ne s'ouvre pas");
    let _ = fs::remove_dir_all(&racine);
    let refus = lancer(&dir, &kraken, &profil, &plan, BAIL).unwrap_err();
    assert_eq!(refus.code, Code::DepotIntrouvable, "dépôt absent");
    assert!(!dir.exists() && !kraken.exists(), "un dépôt absent ne crée rien");
}

#[test]
fn le_verif_dit_si_la_verification_a_tourne() {
    let racine = racine("verif");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let fausse = Profil::faux(fausse_main(&racine), DELAI);
    let introuvable = Profil::faux(vec!["kraken-main-introuvable".into()], DELAI);
    // Chaque lancement, et le `verif` de chacune de ses fins, rogné : `None` si la vérification n'a pas tourné.
    let cas = [
        // Sans commande : la main rouge, puis verte sur sa déclaration.
        (&fausse, None, vec![None, None]),
        // Une main rouge avant la vérification, deux fois.
        (&introuvable, Some("git status"), vec![None, None]),
        // Une vérification qui a tourné sans rien écrire, puis une qui parle.
        (&fausse, Some("exit 0"), vec![Some("")]),
        (&fausse, Some("echo bonjour"), vec![Some("bonjour")]),
    ];
    for (profil, verifier, _) in &cas {
        lancer(&depot, &kraken, profil, &Plan::copies("objectif", *verifier, 1), BAIL).unwrap();
    }
    let vagues = etat(&fil(&kraken));
    assert_eq!(vagues.len(), cas.len());
    for (vague, (_, verifier, attendus)) in vagues.iter().zip(&cas) {
        let tentatives = vague.mains[&1].tentatives.values();
        let verifs: Vec<Option<&str>> = tentatives.map(|t| t.verif.as_deref().map(str::trim)).collect();
        assert_eq!(&verifs, attendus, "--verifier {verifier:?}");
    }
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_dependante_part_de_l_assemblage_des_siennes() {
    let racine = racine("dependante");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    let plan = plan(&[
        "plan 1",
        "objectif : Écrire a et b, puis c d'après elles",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        "    Écrire b",
        "tache c",
        "depend : a b",
        "    Écrire c d'après a et b",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    for (id, issue) in &issues {
        assert!(issue.as_ref().is_ok_and(|pris| pris.prouve), "{id} est assemblée : {issue:?}");
    }
    // c ne part qu'après les fusions de a et de b, et son bac part de la tête de la vague où elles sont
    // assemblées : il a leur travail, et son gel a pour parent la dernière des deux fusions.
    let evenements = fil(&kraken);
    let debut_de_c = evenements.iter().position(|e| matches!(e, Evenement::Debut { main: 3, .. })).unwrap();
    let fusions: Vec<(usize, &str)> = (0..)
        .zip(&evenements)
        .filter_map(|(rang, e)| match e {
            Evenement::Fusion { main, sha, .. } if *main < 3 => Some((rang, sha.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(fusions.len(), 2, "{evenements:?}");
    assert!(fusions.iter().all(|(rang, _)| *rang < debut_de_c), "c part après les deux fusions");
    let bac = kraken.join("bacs").join(ids[2]);
    for id in &ids[..2] {
        assert!(bac.join(format!("{id}.txt")).is_file(), "le bac de c a le travail de {id}");
    }
    assert_eq!(git(&bac, &["rev-parse", "HEAD^1"]), fusions[1].1, "le bac de c part de la tête de la vague");
    // La branche de la vague a les trois.
    let branche = branche.expect("la branche d'une vague à plan");
    let fichiers: Vec<String> = ids.iter().map(|id| format!("{id}.txt")).collect();
    assert_eq!(git(&depot, &["ls-tree", "-r", "--name-only", &branche]).lines().collect::<Vec<_>>(), fichiers);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn les_dependantes_d_une_classee_sont_sautees() {
    let racine = racine("sautees");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    // a joue une preuve rouge : classée après sa relance ; b dépend d'elle, et c de b.
    let plan = plan(&[
        "plan 1",
        "objectif : Une chaîne",
        "preuve : exit 0",
        "tache a",
        "preuve : exit 3",
        "    Échouer",
        "tache b",
        "depend : a",
        "    Après a",
        "tache c",
        "depend : b",
        "    Après b",
    ]);
    let (_, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let [(_, classee), (b, sautee_b), (c, sautee_c)] = issues.as_slice() else { panic!("{issues:?}") };
    let erreur = classee.as_ref().unwrap_err();
    assert!(!erreur.starts_with("refusée (") && erreur.contains("exit 3"), "a est classée : {erreur}");
    for (id, issue) in [(b, sautee_b), (c, sautee_c)] {
        let texte = issue.as_ref().unwrap_err();
        assert!(texte.starts_with("refusée (7011) : "), "{id} est sautée : {texte}");
        assert!(!kraken.join("bacs").join(id).exists(), "{id} n'a pas de bac");
        assert_eq!(git(&depot, &["branch", "--list", &format!("kraken/{id}")]), "", "{id} n'a pas de branche");
    }
    let attendu = [
        "vague 3",
        "assemblage",
        "tâche a",
        "tâche b",
        "tâche c",
        "bail 1",
        "début 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "fin 2 Rouge Constate",
        "verdict Classee Constate",
        "refus 7011",
        "refus 7011",
    ];
    assert_eq!(histoire(&fil(&kraken)), attendu);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_base_rouge_ne_lance_rien() {
    let racine = racine("base-rouge");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    let plan = plan(&["plan 1", "objectif : o", "preuve : exit 1", "tache a", "    Écrire a"]);
    let refus = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap_err();
    assert_eq!(refus.code, Code::BaseRouge, "{refus}");
    // Ni fil, ni bac, ni branche, ni worktree.
    assert!(!kraken.join("fil.jsonl").exists(), "ni fil");
    let bacs = kraken.join("bacs");
    assert!(!bacs.exists() || noms(&bacs).is_empty(), "ni bac");
    assert_eq!(git(&depot, &["branch", "--list", "kraken/*"]), "", "ni branche");
    assert_eq!(git(&depot, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1, "ni worktree");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_main_jamais_conclue_expire_et_son_resultat_tardif_est_refuse() {
    let racine = racine("expire");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let plan = plan(&[
        "plan 1",
        "objectif : o",
        "preuve : exit 0",
        "tache a",
        "    Attendre",
        "tache b",
        "depend : a",
        "    Après a",
    ]);
    // La main de a attend sans rien rendre au-delà de son bail de 2 s : elle expire, puis le test la libère.
    let a_expire = |kraken: &Path| fil(kraken).iter().any(|e| matches!(e, Evenement::Refus { code: 7004, .. }));
    let (branche, issues) = thread::scope(|s| {
        let vague = s.spawn(|| lancer(&depot, &kraken, &profil, &plan, Duration::from_secs(2)));
        attendre("le refus 7004 au fil", &vague, || a_expire(&kraken) && marques(&kraken).len() == 1);
        for bac in marques(&kraken) {
            liberer(&bac);
        }
        vague.join().unwrap()
    })
    .unwrap();
    let [(a, expiree), (b, sautee)] = issues.as_slice() else { panic!("{issues:?}") };
    assert!(expiree.as_ref().unwrap_err().starts_with("refusée (7004) : "), "{expiree:?}");
    assert!(sautee.as_ref().unwrap_err().starts_with("refusée (7011) : "), "{sautee:?}");
    // Aucun second début ; son résultat tardif est noté périmé, après les refus, et rien d'autre.
    let attendu = [
        "vague 2",
        "assemblage",
        "tâche a",
        "tâche b",
        "bail 1",
        "début 1",
        "refus 7004",
        "refus 7011",
        "périmée 1 Vert Constate",
    ];
    assert_eq!(histoire(&fil(&kraken)), attendu);
    // Son bac n'est ni figé ni assemblé ; sa dépendante n'a pas de bac.
    assert_eq!(git(&kraken.join("bacs").join(a), &["rev-parse", "HEAD"]), base, "aucun commit dans son bac");
    assert_eq!(git(&depot, &["rev-parse", &branche.unwrap()]), base, "la branche de la vague reste à sa base");
    assert!(!kraken.join("bacs").join(b).exists());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_main_qui_touche_claude_n_est_pas_relancee() {
    let racine = racine("claude");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    // Une copie dont la preuve est rouge : sa première tentative finit rouge, et se relancerait.
    let plan = Plan::copies("objectif", Some("exit 1"), 1);
    let (_, issues) = thread::scope(|s| {
        let vague = s.spawn(|| lancer(&depot, &kraken, &profil, &plan, BAIL));
        attendre("la marque", &vague, || marques(&kraken).len() == 1);
        // Ce que la main aurait écrit sous un .claude/ : le test le pose pendant qu'elle attend.
        for bac in marques(&kraken) {
            ecrire(&bac, ".claude/settings.json", "{}");
            liberer(&bac);
        }
        vague.join().unwrap()
    })
    .unwrap();
    let (id, issue) = &issues[0];
    let texte = issue.as_ref().unwrap_err();
    assert!(texte.starts_with("refusée (7006) : "), "{texte}");
    assert!(texte.contains(".claude/settings.json"), "le refus nomme le fichier : {texte}");
    let attendu = ["vague 1", "bail 1", "début 1", "fin 1 Rouge Constate", "refus 7006"];
    assert_eq!(histoire(&fil(&kraken)), attendu, "pas de relance");
    let consigne = fs::read_to_string(kraken.join("bacs").join(id).join(format!("{id}.txt"))).unwrap();
    assert!(!consigne.contains("Tentative pr"), "la main n'a eu que sa première consigne : {consigne}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_bail_couvre_la_main_sa_preuve_et_une_marge() {
    let profil = Profil::faux(vec!["x".into()], Duration::from_secs(30));
    assert_eq!(duree_du_bail(&profil), profil.delai_main() + Duration::from_secs(30 + 60));
}

#[test]
fn seule_une_main_tuee_a_son_delai_perd_sa_relance() {
    // Le texte de M2, en tête de l'erreur d'une main qu'il a tuée à son délai : elle n'est pas relancée.
    assert!(main_tuee("main : temps dépassé (1 s)"));
    // Dit ailleurs, par la sortie de la main ou par une preuve trop lente, il n'empêche pas la relance.
    assert!(!main_tuee("exit code: 1 ; main : temps dépassé"));
    assert!(!main_tuee("vérification « x » : temps dépassé (1 s)"));
}

#[test]
fn une_main_tuee_a_son_delai_n_est_pas_relancee() {
    let racine = racine("tuee");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    // Un profil à `delai_main` d'une seconde, dont la main dort 30 s : Kraken tue son arbre à son délai. b dépend
    // de a.
    let texte = format!("sorte = claude\nprogramme = {}\ndelai_main = 1\n", main_qui_dort(&racine).display());
    let profil = profil_declare(&texte, &kraken, &depot);
    let plan = plan(&[
        "plan 1",
        "objectif : o",
        "preuve : exit 0",
        "tache a",
        "    Dormir",
        "tache b",
        "depend : a",
        "    Après a",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    // a n'est pas relancée : elle est refusée en 7005, dont la preuve dit le temps dépassé, et b est sautée, sans
    // bac.
    let refus = refusee(ids[0], &issues[0].1, 7005);
    assert!(refus.contains("main : temps dépassé (1 s)"), "la preuve dit le délai de la main : {refus}");
    refusee(ids[1], &issues[1].1, 7011);
    assert!(!kraken.join("bacs").join(ids[1]).exists(), "b n'a pas de bac");
    // Un seul début, donc une seule main : la fin rouge de a dit le temps dépassé, puis son refus a clos la tâche.
    let evenements = fil(&kraken);
    let a = ["tâche a", "bail 1", "début 1", "lancement 1", "constat 1", "fin 1 Rouge Constate", "refus 7005"];
    assert_eq!(histoire_de(&evenements, 1), a);
    assert_eq!(histoire_de(&evenements, 2), ["tâche b", "refus 7011"]);
    let debuts = evenements.iter().filter(|evenement| matches!(evenement, Evenement::Debut { .. }));
    assert_eq!(debuts.count(), 1, "aucune relance : {evenements:?}");
    let fins: Vec<&Evenement> = evenements.iter().filter(|e| matches!(e, Evenement::Fin { .. })).collect();
    let [Evenement::Fin { texte, .. }] = fins.as_slice() else { panic!("une seule fin : {fins:?}") };
    assert!(texte.starts_with("main : temps dépassé (1 s)"), "{texte}");
    // Son bac n'est ni figé ni assemblé : la branche de la vague est restée à sa base.
    assert_eq!(git(&kraken.join("bacs").join(ids[0]), &["rev-parse", "HEAD"]), base, "aucun commit dans son bac");
    assert_eq!(git(&depot, &["rev-parse", &branche]), base);
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    effacer(&racine);
}

#[test]
fn une_main_sans_programme_est_classee_sans_faire_tomber_la_vague() {
    let racine = racine("sans-programme");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    // Une commande vide : `tenter` la rend en erreur à chaque tentative. Avant M2 elle paniquait, et la vague tenait
    // par `en_message` ; plus rien dans `tenter` ne panique, et `une_panique_de_main_devient_son_message` garde
    // ce cas.
    let profil = Profil::faux(Vec::new(), DELAI);
    let (_, issues) = lancer(&depot, &kraken, &profil, &Plan::copies("objectif", None, 2), BAIL).unwrap();
    assert_eq!(issues.len(), 2);
    for (id, issue) in &issues {
        let erreur = issue.as_ref().unwrap_err();
        assert!(erreur.starts_with("main : aucun programme à lancer"), "{id} : {erreur}");
    }
    // Chacune est relancée une fois, comme toute fin rouge, puis classée.
    let vagues = etat(&fil(&kraken));
    for main in vagues[0].mains.values() {
        assert_eq!((main.tentatives.len(), main.verdict), (2, Some((Verdict::Classee, Foi::Constate))));
    }
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_panique_de_main_devient_son_message() {
    let (constat, verif, verdict) = en_message(|| panic!("boum"));
    assert_eq!((constat, verif, verdict.unwrap_err()), (None, None, "la main a paniqué : boum".to_string()));
    let (_, _, verdict) = en_message(|| panic!("boum {}", 2));
    assert_eq!(verdict.unwrap_err(), "la main a paniqué : boum 2");
    // Sans panique, ce que la main rend passe tel quel, son constat compris.
    let lu = Constat { version: Some("2.1.312".into()), mur_actif: Some(false), ..Constat::default() };
    let pris = Ok(Pris { sortie: "fait".into(), prouve: true });
    let (constat, verif, verdict) = en_message(|| (Some(lu.clone()), Some("vert".into()), pris));
    let pris = verdict.unwrap();
    assert_eq!(constat, Some(lu));
    assert_eq!((verif.as_deref(), pris.sortie.as_str(), pris.prouve), (Some("vert"), "fait", true));
}

#[test]
fn le_fil_rejoue_une_vague_a_plan() {
    let racine = racine("rejeu-plan");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    // a et b, qui dépend d'elle, assemblées ; c classée ; d, qui dépend de c, sautée.
    let plan = plan(&[
        "plan 1",
        "objectif : Quatre tâches",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        "depend : a",
        "    Écrire b",
        "tache c",
        "preuve : exit 3",
        "    Échouer",
        "tache d",
        "depend : c",
        "    Après c",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let vagues = etat(&fil(&kraken));
    let [vague] = vagues.as_slice() else { panic!("une vague : {vagues:?}") };
    assert_eq!((vague.n, vague.objectif.as_str(), vague.verifier.as_deref()), (4, "Quatre tâches", None));
    let assemblage = vague.assemblage.as_ref().map(|(branche, preuve)| (branche.clone(), preuve.as_str()));
    assert_eq!(assemblage, branche.map(|branche| (branche, "exit 0")));
    // Main par main, ce que lancer a rendu : sa tâche, et son issue.
    assert_eq!(vague.mains.len(), issues.len());
    for ((n, main), ((id, issue), tache)) in vague.mains.iter().zip(issues.iter().zip(&plan.taches)) {
        assert_eq!(format!("main-{}-{n}", vague.id), *id);
        assert_eq!(main.tache, Some((tache.id.clone(), tache.depend.clone())), "{id}");
        match issue {
            Ok(pris) => {
                assert!(pris.prouve, "{id}");
                assert_eq!((&main.issue, main.texte()), (&Some(Issue::Assemblee), pris.sortie.as_str()), "{id}");
            }
            Err(texte) if texte.starts_with("refusée (") => {
                let Some(Issue::Refusee { code, regle, preuve, correction }) = &main.issue else {
                    panic!("{id} : {:?}", main.issue)
                };
                assert_eq!(refus_en_clair(*code, regle, preuve, correction), *texte, "{id}");
            }
            Err(texte) => {
                let classee = (&main.issue, main.verdict, main.texte());
                assert_eq!(classee, (&None, Some((Verdict::Classee, Foi::Constate)), texte.as_str()), "{id}");
            }
        }
    }
    // a et b assemblées, c classée, d sautée.
    let resume: Vec<String> = vague
        .mains
        .values()
        .map(|main| match (&main.issue, main.verdict) {
            (Some(Issue::Assemblee), _) => "assemblée".into(),
            (Some(Issue::Refusee { code, .. }), _) => format!("refusée {code}"),
            (None, verdict) => format!("{verdict:?}"),
        })
        .collect();
    assert_eq!(resume, ["assemblée", "assemblée", "Some((Classee, Constate))", "refusée 7011"]);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_losange_s_assemble_dans_l_ordre() {
    let racine = racine("losange");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    // a ouvre le losange, b et c en dépendent, d se referme sur les deux.
    let plan = plan(&[
        "plan 1",
        "objectif : Un losange",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        "depend : a",
        "    Écrire b",
        "tache c",
        "depend : a",
        "    Écrire c",
        "tache d",
        "depend : b c",
        "    Écrire d",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    for (id, issue) in &issues {
        assemblee(id, issue);
    }
    // Les fusions, dans l'ordre où le fil les note, sont un ordre topologique du plan : a d'abord, b et c ensuite dans
    // l'ordre où elles arrivent, d à la fin.
    let evenements = fil(&kraken);
    let fusions = fusions(&evenements);
    let ordre: Vec<usize> = fusions.iter().map(|(main, _)| *main).collect();
    assert!(est_topologique(&plan, &ordre), "les fusions suivent les dépendances : {ordre:?}");
    // La branche porte les mêmes fusions dans le même ordre : leurs titres, en suivant le premier parent depuis la
    // base.
    let titres = git(&depot, &["log", "--first-parent", "--reverse", "--format=%s", &format!("{base}..{branche}")]);
    let sur_la_branche: Vec<usize> =
        titres.lines().map(|vu| 1 + plan.taches.iter().position(|tache| tache.id == vu).unwrap()).collect();
    assert_eq!(sur_la_branche, ordre, "la branche porte les fusions dans l'ordre du fil : {titres}");
    // Elle a les quatre travaux.
    let travaux: Vec<String> = ids.iter().map(|id| format!("{id}.txt")).collect();
    assert_eq!(fichiers(&depot, &branche), travaux);
    // Chaque bac part de la tête de la vague à l'instant de son départ : a de la base, b et c de la fusion de a, d de
    // la dernière des fusions de b et de c, et son bac a le travail de a, de b et de c.
    let bac = |k: usize| kraken.join("bacs").join(ids[k - 1]);
    let parent = |k: usize| git(&bac(k), &["rev-parse", "HEAD^1"]);
    let fusion_de = |k: usize| fusions.iter().find(|(main, _)| *main == k).map(|(_, sha)| sha.clone()).unwrap();
    assert_eq!(parent(1), base, "a part de la base");
    assert_eq!((parent(2), parent(3)), (fusion_de(1), fusion_de(1)), "b et c partent de la fusion de a");
    assert_eq!(parent(4), fusions[2].1, "d part de la dernière fusion de b et de c");
    for k in 1..=3 {
        assert!(bac(4).join(format!("{}.txt", ids[k - 1])).is_file(), "le bac de d a le travail de {}", ids[k - 1]);
    }
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn deux_independantes_en_conflit() {
    let racine = racine("conflit");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_commune(&racine), DELAI);
    // a et b écrivent chacune leur commun.txt, de la même base : elles se heurtent. c dépend des deux.
    let plan = plan(&[
        "plan 1",
        "objectif : Deux tâches écrivent le même fichier",
        "preuve : exit 0",
        "tache a",
        "    Écrire commun.txt",
        "tache b",
        "    Écrire commun.txt autrement",
        "tache c",
        "depend : a b",
        "    Après a et b",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    // La première arrivée gagne, et ce test n'en suppose aucune : l'une est assemblée, l'autre refusée en 7007.
    let (gagnante, perdante) = match (&issues[0].1, &issues[1].1) {
        (Ok(_), Err(_)) => (1, 2),
        (Err(_), Ok(_)) => (2, 1),
        autre => panic!("l'une assemblée, l'autre refusée : {autre:?}"),
    };
    assemblee(ids[gagnante - 1], &issues[gagnante - 1].1);
    let conflit = refusee(ids[perdante - 1], &issues[perdante - 1].1, 7007);
    assert!(conflit.contains("commun.txt"), "le refus nomme le fichier en conflit : {conflit}");
    // Leur dépendante est sautée, sans bac ni branche.
    refusee(ids[2], &issues[2].1, 7011);
    assert!(!kraken.join("bacs").join(ids[2]).exists(), "c n'a pas de bac");
    assert_eq!(git(&depot, &["branch", "--list", &format!("kraken/{}", ids[2])]), "", "ni de branche");
    // Une seule fusion au fil : celle de la gagnante, dont le commun.txt est dans la branche. La perdante garde le
    // sien dans son bac, et la branche n'a pas bougé pour elle.
    let evenements = fil(&kraken);
    let fusions = fusions(&evenements);
    assert_eq!(fusions.iter().map(|(main, _)| *main).collect::<Vec<_>>(), [gagnante], "{evenements:?}");
    assert_eq!(git(&depot, &["rev-parse", &branche]), fusions[0].1);
    assert_eq!(git(&depot, &["show", &format!("{branche}:commun.txt")]), ids[gagnante - 1]);
    let chez_la_perdante = fs::read_to_string(kraken.join("bacs").join(ids[perdante - 1]).join("commun.txt"));
    assert_eq!(chez_la_perdante.unwrap().trim(), ids[perdante - 1], "le bac de la perdante garde son fichier");
    // Le refus noté au fil : le conflit sur la tête de la gagnante, la preuve de la vague n'ayant pas tourné.
    let tache = |k: usize| plan.taches[k - 1].id.as_str();
    let note = evenements.iter().find_map(|evenement| match evenement {
        Evenement::Refus { code: 7007, preuve, verif, .. } => Some((preuve, verif)),
        _ => None,
    });
    let Some((preuve, verif)) = note else { panic!("le refus 7007 est au fil : {evenements:?}") };
    let attendu = format!("la fusion de {} sur la tête {} est en conflit : commun.txt", tache(perdante), fusions[0].1);
    assert_eq!((preuve, verif), (&attendu, &None));
    assert_eq!(histoire_de(&evenements, gagnante), prise_du_premier_coup(tache(gagnante), "fusion"));
    assert_eq!(histoire_de(&evenements, perdante), prise_du_premier_coup(tache(perdante), "refus 7007"));
    assert_eq!(histoire_de(&evenements, 3), ["tâche c", "refus 7011"]);
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_preuve_de_vague_rouge_laisse_la_branche_a_sa_base() {
    let racine = racine("vague-rouge");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_commune(&racine), DELAI);
    // La preuve de la vague passe sur la base. a joue la sienne, verte, mais son travail, fusionné, rend rouge celle de
    // la vague. b dépend de a.
    let de_la_vague = format!("preuve : {SANS_COMMUN}");
    let plan = plan(&[
        "plan 1",
        "objectif : Une fusion qui rend rouge la preuve de la vague",
        de_la_vague.as_str(),
        "tache a",
        "preuve : exit 0",
        "    Écrire commun.txt",
        "tache b",
        "depend : a",
        "    Après a",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    let rouge = refusee(ids[0], &issues[0].1, 7008);
    assert!(rouge.contains("commun.txt vu"), "le refus porte la sortie de la preuve de la vague : {rouge}");
    refusee(ids[1], &issues[1].1, 7011);
    assert!(!kraken.join("bacs").join(ids[1]).exists(), "b n'a pas de bac");
    // La branche est restée à sa base, avec son worktree, et le fil ne note aucune fusion : il dit la base où la preuve
    // de la vague a passé, puis le refus de a, qui porte la fin de la sortie de cette preuve.
    assert_eq!(git(&depot, &["rev-parse", &branche]), base);
    worktree_a_la_tete(&depot, &kraken, &branche);
    let evenements = fil(&kraken);
    assert!(fusions(&evenements).is_empty(), "aucune fusion n'est notée : {evenements:?}");
    let attendu = [
        "vague 2",
        "assemblage",
        "tâche a",
        "tâche b",
        "bail 1",
        "début 1",
        "fin 1 Vert Constate",
        "verdict Prise Constate",
        "refus 7008",
        "refus 7011",
    ];
    assert_eq!(histoire(&evenements), attendu);
    let (vague, preuve) = (vague_du_fil(&kraken), SANS_COMMUN.to_string());
    let assemblage = Evenement::Assemblage { vague, branche: branche.clone(), base: base.clone(), preuve };
    assert_eq!(evenements[1], assemblage);
    let Some(Evenement::Refus { verif, .. }) =
        evenements.iter().find(|evenement| matches!(evenement, Evenement::Refus { code: 7008, .. }))
    else {
        panic!("le refus 7008 est au fil : {evenements:?}")
    };
    assert_eq!(verif.as_deref().map(str::trim), Some("commun.txt vu"));
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_relance_reste_seule_et_porte_un_jeton_neuf() {
    let racine = racine("relance-plan");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    // La preuve de a est rouge la première fois qu'elle se joue dans son bac, et verte ensuite : a est relancée, une
    // fois. b dépend d'elle.
    let sienne = format!("preuve : {ROUGE_PUIS_VERTE}");
    let plan = plan(&[
        "plan 1",
        "objectif : Une relance",
        "preuve : exit 0",
        "tache a",
        sienne.as_str(),
        "    Écrire a",
        "tache b",
        "depend : a",
        "    Écrire b",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    let pris = assemblee(ids[0], &issues[0].1);
    assemblee(ids[1], &issues[1].1);
    // Le début 2 suit la fin 1 : la relance ne part qu'après le message de la tentative d'avant, et elle part seule.
    let evenements = fil(&kraken);
    let relance = [
        "tâche a",
        "bail 1",
        "début 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "fin 2 Vert Constate",
        "verdict Prise Constate",
        "fusion",
    ];
    assert_eq!(histoire_de(&evenements, 1), relance);
    assert_eq!(histoire_de(&evenements, 2), prise_du_premier_coup("b", "fusion"));
    // b ne part qu'après la fusion de a, donc après sa seconde tentative.
    let fusion_de_a = evenements.iter().position(|e| matches!(e, Evenement::Fusion { main: 1, .. })).unwrap();
    let bail_de_b = evenements.iter().position(|e| matches!(e, Evenement::Bail { main: 2, .. })).unwrap();
    assert!(fusion_de_a < bail_de_b, "b ne part qu'après l'assemblage de a");
    // Un jeton neuf et une échéance neuve à la seconde tentative.
    let baux: Vec<(usize, &str)> = evenements
        .iter()
        .filter_map(|evenement| match evenement {
            Evenement::Bail { main: 1, tentative, echeance, .. } => Some((*tentative, echeance.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(baux.iter().map(|(tentative, _)| *tentative).collect::<Vec<_>>(), [1, 2], "{baux:?}");
    assert!(baux[0].1 <= baux[1].1, "l'échéance de la tentative 2 est la sienne, prise après celle de la 1 : {baux:?}");
    // La prise vient de la tentative 2 : sa fin est la verte, son texte est la sortie prise, et sa main a reçu l'erreur
    // de la première en contexte.
    let vagues = etat(&evenements);
    let main = &vagues[0].mains[&1];
    let couleurs: Vec<_> = main.tentatives.values().map(|tentative| tentative.couleur).collect();
    assert_eq!(couleurs, [Some((Couleur::Rouge, Foi::Constate)), Some((Couleur::Vert, Foi::Constate))]);
    assert_eq!(main.echeances.keys().copied().collect::<Vec<_>>(), [1, 2]);
    assert_eq!((main.verdict, &main.issue), (Some((Verdict::Prise, Foi::Constate)), &Some(Issue::Assemblee)));
    assert_eq!(main.texte(), pris.sortie, "la prise est la sortie de la tentative 2");
    let consigne = fs::read_to_string(kraken.join("bacs").join(ids[0]).join(format!("{}.txt", ids[0]))).unwrap();
    let erreur = format!("vérification « {ROUGE_PUIS_VERTE} » rouge");
    assert!(consigne.contains("Tentative pr") && consigne.contains(&erreur), "la consigne de la 2 : {consigne}");
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_bac_impossible_refuse_sa_tache_et_laisse_sa_trace() {
    let racine = racine("bac-impossible");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    // b et c dépendent de a, d de b. Le bac de b ne se posera pas : le test prend son dossier pendant que la main de a
    // attend. c continue.
    let plan = plan(&[
        "plan 1",
        "objectif : Un bac qui ne se pose pas",
        "preuve : exit 0",
        "tache a",
        "    Attendre",
        "tache b",
        "depend : a",
        "    Dans un bac qui ne se pose pas",
        "tache c",
        "depend : a",
        "    Dans un bac qui se pose",
        "tache d",
        "depend : b",
        "    Après b",
    ]);
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        attendre("la marque de a", vague, || marques(&kraken).len() == 1);
        ecrire(&bac_de(&kraken, &vague_du_fil(&kraken), 2), "pris.txt", "pris");
        for bac in marques(&kraken) {
            liberer(&bac);
        }
        attendre("la marque de c", vague, || marques(&kraken).len() == 2);
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    assemblee(ids[0], &issues[0].1);
    let refus = refusee(ids[1], &issues[1].1, 7001);
    assert!(refus.contains("git worktree add"), "le refus nomme la commande de git : {refus}");
    assert!(refus.contains(&format!("kraken/{}", ids[1])), "et la branche qui ne s'est pas posée : {refus}");
    assemblee(ids[2], &issues[2].1);
    refusee(ids[3], &issues[3].1, 7011);
    // La trace : le fil a la tâche de b et son refus, sans bail ni début ; d, qui en dépend, est sautée. Le dossier
    // pris est resté tel quel, et b n'a pas de worktree. Sa branche n'est pas jugée : `git worktree add -b` la pose
    // avant de contrôler le dossier, et elle peut rester sur le départ de b.
    let evenements = fil(&kraken);
    assert_eq!(histoire_de(&evenements, 2), ["tâche b", "refus 7001"]);
    assert_eq!(histoire_de(&evenements, 4), ["tâche d", "refus 7011"]);
    let vagues = etat(&evenements);
    let b = &vagues[0].mains[&2];
    assert_eq!(b.tache, Some(("b".to_string(), vec!["a".to_string()])));
    assert!(b.tentatives.is_empty() && b.verdict.is_none(), "b n'a jamais commencé : {b:?}");
    let Some(Issue::Refusee { code, regle, preuve, correction }) = &b.issue else { panic!("{:?}", b.issue) };
    assert_eq!(code, &7001);
    assert_eq!(&refus_en_clair(*code, regle, preuve, correction), issues[1].1.as_ref().unwrap_err());
    let bac_de_b = bac_de(&kraken, &vague_du_fil(&kraken), 2);
    assert_eq!(fs::read_to_string(bac_de_b.join("pris.txt")).unwrap(), "pris");
    assert!(!git(&depot, &["worktree", "list", "--porcelain"]).contains(ids[1]), "b n'a pas de worktree");
    // Les autres continuent : la branche a le travail de a et de c, et rien de b ni de d.
    let sur_la_branche = fichiers(&depot, &branche);
    assert!(sur_la_branche.contains(&format!("{}.txt", ids[0])) && sur_la_branche.contains(&format!("{}.txt", ids[2])));
    assert!(sur_la_branche.iter().all(|f| f.starts_with(ids[0]) || f.starts_with(ids[2])), "{sur_la_branche:?}");
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_plan_dont_une_main_touche_claude() {
    let racine = racine("plan-claude");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    // a est rouge et touche .claude/ : elle n'est pas relancée. b est verte et le touche aussi : elle n'est pas figée.
    // c ne touche à rien : elle est assemblée. d dépend de a et de b : elle est sautée.
    let plan = plan(&[
        "plan 1",
        "objectif : Des mains qui touchent .claude/",
        "preuve : exit 0",
        "tache a",
        "preuve : exit 1",
        "    Échouer en touchant .claude/",
        "tache b",
        "    Réussir en touchant .claude/",
        "tache c",
        "    Ne rien toucher",
        "tache d",
        "depend : a b",
        "    Après a et b",
    ]);
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        attendre("les trois marques", vague, || marques(&kraken).len() == 3);
        // Ce que la main aurait écrit sous un .claude/ : le test le pose pendant qu'elle attend. a et b le touchent.
        for k in [1, 2] {
            ecrire(&bac_de(&kraken, &vague_du_fil(&kraken), k), ".claude/settings.json", "{}");
        }
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    for k in [1, 2] {
        let refus = refusee(ids[k - 1], &issues[k - 1].1, 7006);
        assert!(refus.contains(".claude/settings.json"), "le refus nomme le fichier : {refus}");
    }
    assemblee(ids[2], &issues[2].1);
    refusee(ids[3], &issues[3].1, 7011);
    // Ni relance pour a (une seule tentative), ni gel ni fusion pour b ; d n'a pas de bac.
    let evenements = fil(&kraken);
    let a = ["tâche a", "bail 1", "début 1", "fin 1 Rouge Constate", "refus 7006"];
    assert_eq!(histoire_de(&evenements, 1), a);
    assert_eq!(histoire_de(&evenements, 2), prise_du_premier_coup("b", "refus 7006"));
    assert_eq!(histoire_de(&evenements, 3), prise_du_premier_coup("c", "fusion"));
    assert_eq!(histoire_de(&evenements, 4), ["tâche d", "refus 7011"]);
    assert!(!kraken.join("bacs").join(ids[3]).exists(), "d n'a pas de bac");
    let vague = vague_du_fil(&kraken);
    let consigne = fs::read_to_string(bac_de(&kraken, &vague, 1).join(format!("{}.txt", ids[0]))).unwrap();
    assert!(!consigne.contains("Tentative pr"), "la main de a n'a eu que sa première consigne : {consigne}");
    for k in [1, 2] {
        let bac = bac_de(&kraken, &vague, k);
        assert_eq!(git(&bac, &["rev-parse", "HEAD"]), base, "aucun commit dans le bac de {}", ids[k - 1]);
        assert_eq!(git(&bac, &["diff", "--cached", "--name-only"]), "", "rien d'ajouté à son index");
        assert!(bac.join(".claude/settings.json").is_file(), "et le .claude/ y est resté");
    }
    // La branche n'a que le travail de c.
    let sur_la_branche = fichiers(&depot, &branche);
    assert!(sur_la_branche.iter().all(|f| f.starts_with(ids[2])), "{sur_la_branche:?}");
    assert!(sur_la_branche.contains(&format!("{}.txt", ids[2])));
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_competence_imbriquee_fait_refuser_sa_tache() {
    let racine = racine("competence");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let plan = plan(&[
        "plan 1",
        "objectif : Une compétence sous un dossier",
        "preuve : exit 0",
        "tache a",
        "    Réussir en posant une compétence",
        "tache b",
        "depend : a",
        "    Après a",
    ]);
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        attendre("la marque", vague, || marques(&kraken).len() == 1);
        // Claude Code charge les compétences de tout `<dossier>/.claude/skills` : la forme `.claude` de la racine seule
        // laisserait passer celle-ci. Le test la pose pendant que la main attend.
        let bac = bac_de(&kraken, &vague_du_fil(&kraken), 1);
        ecrire(&bac, "sous/.claude/skills/k/SKILL.md", "---\nname: k\ndescription: Une compétence\n---\n");
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    let refus = refusee(ids[0], &issues[0].1, 7006);
    assert!(refus.contains(" : sous/.claude/skills/k/SKILL.md"), "le refus nomme la compétence : {refus}");
    refusee(ids[1], &issues[1].1, 7011);
    assert!(!kraken.join("bacs").join(ids[1]).exists(), "b n'a pas de bac");
    // a était verte, mais son bac n'est ni figé ni assemblé, et la branche est restée à sa base.
    let evenements = fil(&kraken);
    assert_eq!(histoire_de(&evenements, 1), prise_du_premier_coup("a", "refus 7006"));
    let bac = bac_de(&kraken, &vague_du_fil(&kraken), 1);
    assert_eq!(git(&bac, &["rev-parse", "HEAD"]), base, "aucun commit dans le bac");
    assert_eq!(git(&bac, &["diff", "--cached", "--name-only"]), "", "rien d'ajouté à son index");
    assert!(bac.join("sous/.claude/skills/k/SKILL.md").is_file());
    assert_eq!(git(&depot, &["rev-parse", &branche]), base);
    assert!(fusions(&evenements).is_empty(), "aucune fusion n'est notée");
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn l_etat_de_claude_code_ne_fait_pas_refuser_une_tache() {
    let racine = racine("etat-claude");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let plan = plan(&["plan 1", "objectif : L'état de Claude Code", "preuve : exit 0", "tache a", "    Déléguer"]);
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        attendre("la marque", vague, || marques(&kraken).len() == 1);
        // Ce que Claude Code écrit lui-même, hors de git, pendant que la main travaille : le worktree d'un sous-agent
        // isolé, et un point de reprise sous un dossier. Le test les pose pendant que la main attend.
        let bac = bac_de(&kraken, &vague_du_fil(&kraken), 1);
        git(&bac, &["worktree", "add", "-q", "--detach", ".claude/worktrees/x"]);
        ecrire(&bac, ".claude/worktrees/x/f", "travail du sous-agent");
        ecrire(&bac, "sous/.claude/checkpoints/c", "point de reprise");
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    let (id, issue) = &issues[0];
    assemblee(id, issue);
    assert_eq!(histoire_de(&fil(&kraken), 1), prise_du_premier_coup("a", "fusion"));
    // La branche n'a rien de cet état : aucun chemin sous un .claude/, aucun dépôt imbriqué (mode 160000). Il est resté
    // dans le bac.
    let arbre = git(&depot, &["ls-tree", "-r", &branche]);
    assert!(!arbre.to_lowercase().contains(".claude"), "rien de l'état de Claude Code dans la branche : {arbre}");
    assert!(!arbre.lines().any(|entree| entree.starts_with("160000")), "aucun dépôt imbriqué : {arbre}");
    assert_eq!(fichiers(&depot, &branche).len(), 3, "la consigne, la marque et la libération de la main : {arbre}");
    let bac = bac_de(&kraken, &vague_du_fil(&kraken), 1);
    assert!(bac.join(".claude/worktrees/x/f").is_file() && bac.join("sous/.claude/checkpoints/c").is_file());
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_vague_ne_laisse_rien_derriere() {
    let racine = racine("rien-derriere");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let courante = git(&depot, &["symbolic-ref", "HEAD"]);
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    // Toutes les issues dans une vague : a assemblée, b relancée puis assemblée, c classée, d sautée.
    let sienne = format!("preuve : {ROUGE_PUIS_VERTE}");
    let plan = plan(&[
        "plan 1",
        "objectif : Une vague de toutes les issues",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        sienne.as_str(),
        "    Écrire b",
        "tache c",
        "preuve : exit 3",
        "    Échouer",
        "tache d",
        "depend : c",
        "    Après c",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    assemblee(ids[0], &issues[0].1);
    assemblee(ids[1], &issues[1].1);
    let classee = issues[2].1.as_ref().unwrap_err();
    assert!(!classee.starts_with("refusée (") && classee.contains("exit 3"), "c est classée : {classee}");
    refusee(ids[3], &issues[3].1, 7011);
    // Le dépôt visé et le dossier de Kraken ne gardent que ce que Kraken a posé : un bac et une branche par tâche
    // partie (pas pour d), le worktree et la branche de la vague, et pas un fichier de plus. La branche n'a que le
    // travail des deux assemblées (et la marque que la preuve de b a posée dans son bac) : la classée reste dehors.
    rien_derriere(&racine, &base);
    worktree_a_la_tete(&depot, &kraken, &branche);
    let mut travaux = vec![format!("{}.txt", ids[0]), format!("{}.txt", ids[1]), "marque".to_string()];
    travaux.sort();
    assert_eq!(fichiers(&depot, &branche), travaux, "la classée et la sautée restent dehors");
    let vague = vague_du_fil(&kraken);
    let mut bacs: Vec<String> = ids[..3].iter().map(|id| id.to_string()).collect();
    bacs.push(format!("vague-{vague}"));
    bacs.sort();
    // Le dossier temporaire de chaque main qui a joué est à côté de son bac, hors de son worktree.
    let mut presents = bacs.clone();
    let temporaire = |id: &&str| temporaire_de(&kraken.join("bacs").join(id)).unwrap();
    presents.extend(ids[..3].iter().map(|id| temporaire(id).file_name().unwrap().to_string_lossy().into_owned()));
    presents.sort();
    assert_eq!(noms(&kraken.join("bacs")), presents, "d n'a pas de bac, ni de dossier temporaire");
    let mut branches: Vec<String> = bacs.iter().map(|bac| format!("refs/heads/kraken/{bac}")).collect();
    branches.push(courante);
    branches.sort();
    let vues = git(&depot, &["for-each-ref", "--format=%(refname)"]);
    assert_eq!(vues.lines().collect::<Vec<_>>(), branches, "une branche par bac, celle de la vague, et la sienne");
    let worktrees = git(&depot, &["worktree", "list", "--porcelain"]);
    assert_eq!(worktrees.matches("worktree ").count(), 1 + bacs.len(), "le dépôt, et un worktree par bac");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn des_independantes_partent_ensemble_chacune_dans_son_bac_sur_la_base() {
    let racine = racine("ensemble");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let plan = plan(&[
        "plan 1",
        "objectif : Trois indépendantes, puis une dépendante",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        "    Écrire b",
        "tache c",
        "    Écrire c",
        "tache d",
        "depend : a b",
        "    Écrire d d'après a et b",
    ]);
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        // Les trois mains attendent à la fois : elles tournent ensemble, chacune dans son bac, posé sur la base.
        attendre("les trois marques", vague, || marques(&kraken).len() == 3);
        let v = vague_du_fil(&kraken);
        for k in 1..=3 {
            let bac = bac_de(&kraken, &v, k);
            assert_eq!(git(&bac, &["rev-parse", "HEAD"]), base, "le bac de la tâche {k} part de la base");
            assert_eq!(git(&bac, &["branch", "--show-current"]), format!("kraken/main-{v}-{k}"));
        }
        // d attend a et b : ni bac, ni bail, ni début, et rien n'est encore assemblé.
        assert!(!bac_de(&kraken, &v, 4).exists(), "d n'a pas de bac");
        assert_eq!(histoire_de(&fil(&kraken), 4), ["tâche d"]);
        assert_eq!(git(&depot, &["rev-parse", &format!("kraken/vague-{v}")]), base, "la branche est à sa base");
        for bac in marques(&kraken) {
            liberer(&bac);
        }
        // d part dès que a et b sont assemblées.
        attendre("la marque de d", vague, || marques(&kraken).len() == 4);
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    for (id, issue) in &issues {
        assemblee(id, issue);
    }
    let evenements = fil(&kraken);
    let rang = |garde: &dyn Fn(&Evenement) -> bool| evenements.iter().position(garde).unwrap();
    let bail_de_d = rang(&|evenement| matches!(evenement, Evenement::Bail { main: 4, .. }));
    for k in [1, 2] {
        let fusion = rang(&|evenement| matches!(evenement, Evenement::Fusion { main, .. } if *main == k));
        assert!(fusion < bail_de_d, "d ne part qu'après l'assemblage de la tâche {k}");
    }
    let bac_de_d = kraken.join("bacs").join(ids[3]);
    for id in &ids[..2] {
        assert!(bac_de_d.join(format!("{id}.txt")).is_file(), "le bac de d a le travail de {id}");
    }
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_ouverture_non_notee_retire_le_worktree_et_la_branche_de_la_vague() {
    let racine = racine("ouverture");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    // Un dossier à la place du fil : la première note de la vague ne s'écrit pas, alors que son assemblage est posé.
    let chemin = kraken.join("fil.jsonl");
    fs::create_dir_all(&chemin).unwrap();
    let plan = plan(&["plan 1", "objectif : o", "preuve : exit 0", "tache a", "    Écrire a"]);
    let refus = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap_err();
    assert_eq!(refus.code, Code::EntreeSortie);
    assert!(refus.preuve.contains(&chemin.display().to_string()), "la preuve nomme le fil : {refus}");
    // Rien n'en reste sans trace : ni la branche de la vague, ni son worktree, ni un bac.
    assert_eq!(git(&depot, &["branch", "--list", "kraken/*"]), "", "ni branche");
    assert_eq!(git(&depot, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1, "ni worktree");
    assert_eq!(noms(&kraken.join("bacs")), Vec::<String>::new(), "ni bac");
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_bac_dont_git_ne_repond_plus_est_refuse_en_7009() {
    let racine = racine("git-muet");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let plan = plan(&[
        "plan 1",
        "objectif : Un bac que git ne lit plus",
        "preuve : exit 0",
        "tache a",
        "    Casser son bac",
        "tache b",
        "depend : a",
        "    Après a",
    ]);
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        attendre("la marque", vague, || marques(&kraken).len() == 1);
        // Le git-dir du bac, que Kraken a lu à sa pose : effacé, plus aucun geste de Kraken ne répond dans ce bac,
        // qui ne suit pas son `.git`. Le test l'ôte pendant que la main attend.
        let bac = bac_de(&kraken, &vague_du_fil(&kraken), 1);
        fs::remove_dir_all(depot.join(".git").join("worktrees").join(bac.file_name().unwrap())).unwrap();
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    // Une sortie vide ne vaut « rien sous .claude » que si git a réussi : a est refusée, jamais assemblée.
    let refus = refusee(ids[0], &issues[0].1, 7009);
    assert!(refus.contains("git diff --name-only"), "le refus nomme la commande de git qui a raté : {refus}");
    refusee(ids[1], &issues[1].1, 7011);
    let evenements = fil(&kraken);
    assert_eq!(histoire_de(&evenements, 1), prise_du_premier_coup("a", "refus 7009"));
    assert!(fusions(&evenements).is_empty(), "aucune fusion n'est notée");
    assert_eq!(git(&depot, &["rev-parse", &branche]), base);
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_branche_deplacee_apres_la_note_defait_la_fusion_en_7009() {
    let racine = racine("branche-deplacee");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let plan = plan(&[
        "plan 1",
        "objectif : Une branche qu'un autre déplace",
        "preuve : echo vert",
        "tache a",
        "    Écrire a",
        "tache b",
        "depend : a",
        "    Après a",
    ]);
    let mut autre = String::new();
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        attendre("la marque", vague, || marques(&kraken).len() == 1);
        // Un autre déplace la branche de la vague pendant que la main attend : un commit à côté de la base.
        // L'update-ref de la vague, qui attend la tête qu'elle connaît, ne peut plus la faire avancer, une fois la
        // fusion notée.
        let (arbre, identite) = (format!("{base}^{{tree}}"), ["-c", "user.name=t", "-c", "user.email=t@t"]);
        autre = git(&depot, &[&identite[..], &["commit-tree", &arbre, "-p", &base, "-m", "ailleurs"]].concat());
        git(&depot, &["update-ref", &format!("refs/heads/kraken/vague-{}", vague_du_fil(&kraken)), &autre]);
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    let refus = refusee(ids[0], &issues[0].1, 7009);
    assert!(refus.contains("est notée, mais"), "le refus dit que la fusion était notée : {refus}");
    refusee(ids[1], &issues[1].1, 7011);
    // Le fil a la fusion, puis le refus qui la suit : le rejeu prend le dernier, et la tâche n'est pas assemblée.
    let evenements = fil(&kraken);
    let attendu = [prise_du_premier_coup("a", "fusion"), vec!["refus 7009".to_string()]].concat();
    assert_eq!(histoire_de(&evenements, 1), attendu);
    let Some(Evenement::Refus { verif, .. }) =
        evenements.iter().find(|evenement| matches!(evenement, Evenement::Refus { code: 7009, .. }))
    else {
        panic!("le refus 7009 est au fil : {evenements:?}")
    };
    assert_eq!(verif.as_deref().map(str::trim), Some("vert"), "la preuve de la vague avait tourné");
    let vagues = etat(&evenements);
    assert!(matches!(&vagues[0].mains[&1].issue, Some(Issue::Refusee { code: 7009, .. })), "{vagues:?}");
    // La branche est restée où l'autre l'a mise ; le worktree de la vague est revenu à la tête que Kraken connaissait.
    assert_eq!(git(&depot, &["rev-parse", &branche]), autre);
    let worktree = kraken.join("bacs").join(branche.strip_prefix("kraken/").unwrap());
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), base);
    assert_eq!(git(&worktree, &["status", "--porcelain", "--untracked-files=all"]), "", "propre");
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_retour_rate_ferme_l_assemblage_de_la_vague() {
    let racine = racine("ferme");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    // a et b sont indépendantes, c dépend de a.
    let plan = plan(&[
        "plan 1",
        "objectif : Un assemblage qui se ferme",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        "    Écrire b",
        "tache c",
        "depend : a",
        "    Après a",
    ]);
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        attendre("les deux marques", vague, || marques(&kraken).len() == 2);
        // Un verrou dans le répertoire git du worktree de la vague, que le test pose pendant que les mains attendent :
        // le retour à la tête (`reset --hard`) ne peut pas se faire.
        let worktree = kraken.join("bacs").join(format!("vague-{}", vague_du_fil(&kraken)));
        let verrou = PathBuf::from(git(&worktree, &["rev-parse", "--absolute-git-dir"])).join("index.lock");
        fs::write(verrou, "").unwrap();
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    // La première prise voit le retour raté, et ferme l'assemblage ; l'autre le trouve fermé. Le test n'en suppose
    // aucune.
    let refus = [refusee(ids[0], &issues[0].1, 7010), refusee(ids[1], &issues[1].1, 7010)];
    assert!(refus.iter().all(|texte| texte.contains("ne revient pas à sa tête")), "{refus:?}");
    assert_eq!(refus.iter().filter(|texte| texte.contains("est fermé")).count(), 1, "{refus:?}");
    refusee(ids[2], &issues[2].1, 7011);
    // La branche n'a pas bougé, et aucune fusion n'est notée. Le verrou ôté, le worktree est à sa tête, propre.
    let evenements = fil(&kraken);
    assert!(fusions(&evenements).is_empty(), "aucune fusion n'est notée : {evenements:?}");
    assert_eq!(histoire_de(&evenements, 3), ["tâche c", "refus 7011"]);
    assert_eq!(git(&depot, &["rev-parse", &branche]), base);
    let worktree = kraken.join("bacs").join(branche.strip_prefix("kraken/").unwrap());
    let verrou = PathBuf::from(git(&worktree, &["rev-parse", "--absolute-git-dir"])).join("index.lock");
    fs::remove_file(verrou).unwrap();
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_depot_hostile_ne_voit_aucun_geste_de_la_vague() {
    let racine = racine("hostile");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let (crochets, moniteur) = hostile(&racine, &depot);
    // Les témoins marchent : un worktree ordinaire joue post-checkout et reference-transaction, un git status ordinaire
    // lance le programme de core.fsmonitor.
    let ordinaire = racine.join("ordinaire");
    git(&depot, &["worktree", "add", "-q", "--detach", &ordinaire.to_string_lossy(), &base]);
    git(&depot, &["status"]);
    let joues = fs::read_to_string(&crochets).unwrap();
    assert!(joues.contains("post-checkout") && joues.contains("reference-transaction"), "témoin : {joues:?}");
    assert_ne!(fs::read_to_string(&moniteur).unwrap(), "", "core.fsmonitor joue d'ordinaire");
    git(&depot, &["worktree", "remove", "--force", &ordinaire.to_string_lossy()]);
    // La fenêtre, de la remise à vide des témoins à leur lecture : seule la vague y joue git, car ni les fausses
    // mains ni les preuves ne le lancent.
    fs::write(&crochets, "").unwrap();
    fs::write(&moniteur, "").unwrap();
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    let plan = plan(&[
        "plan 1",
        "objectif : Un dépôt hostile",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        "depend : a",
        "    Écrire b",
    ]);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    let (joues, du_moniteur) = (fs::read_to_string(&crochets).unwrap(), fs::read_to_string(&moniteur).unwrap());
    let noms_joues: BTreeSet<&str> = joues.lines().collect();
    assert!(noms_joues.is_empty(), "des crochets du dépôt visé ont joué : {noms_joues:?}");
    assert_eq!(du_moniteur, "", "le programme de core.fsmonitor du dépôt visé a joué");
    // Malgré user.name, user.email et commit.gpgsign du dépôt, les deux gels et les deux fusions sont de Kraken, non
    // signés.
    let branche = branche.expect("la branche d'une vague à plan");
    for (id, issue) in &issues {
        assemblee(id, issue);
    }
    let auteurs = git(&depot, &["log", "--format=%an <%ae>|%cn <%ce>", &format!("{base}..{branche}")]);
    assert_eq!(auteurs.lines().count(), 4, "deux gels et deux fusions : {auteurs}");
    assert!(auteurs.lines().all(|ligne| ligne == "kraken <kraken@invalid>|kraken <kraken@invalid>"), "{auteurs}");
    for commit in git(&depot, &["rev-list", &format!("{base}..{branche}")]).lines() {
        let brut = git(&depot, &["cat-file", "commit", commit]);
        assert!(!brut.contains("gpgsig"), "{commit} n'est pas signé : {brut}");
    }
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_fil_injoignable_arrete_aussi_un_plan() {
    let racine = racine("injoignable-plan");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let (chemin, cote) = (kraken.join("fil.jsonl"), racine.join("cote"));
    // a et b sont indépendantes, c dépend de a. Quand leurs mains attendent, le fil se casse (un dossier prend la place
    // du fichier), puis elles sont libérées : la première note qui suit rate, et rien ne s'assemble.
    let plan = plan(&[
        "plan 1",
        "objectif : Un fil qui se casse",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        "    Écrire b",
        "tache c",
        "depend : a",
        "    Après a",
    ]);
    let mut avant = Vec::new();
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        attendre("les deux marques", vague, || marques(&kraken).len() == 2);
        fs::rename(&chemin, &cote).unwrap();
        fs::create_dir(&chemin).unwrap();
        avant = fs::read(&cote).unwrap();
        tout_liberer(&kraken, vague);
    });
    let branche = branche.expect("la branche d'une vague à plan");
    // Trois issues, chacune le refus qui a arrêté la vague, dont la preuve nomme le fil ; a et b gardent la sortie de
    // leur main, et c n'est jamais partie.
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    for (id, issue) in &issues {
        let texte = refusee(id, issue, 7012);
        assert!(texte.contains(&chemin.display().to_string()), "sa preuve nomme le fil : {texte}");
    }
    for k in [0, 1] {
        let texte = issues[k].1.as_ref().unwrap_err();
        assert!(texte.contains(&format!("sortie de {}", ids[k])), "la sortie de sa main : {texte}");
    }
    assert!(!kraken.join("bacs").join(ids[2]).exists(), "c n'a pas de bac");
    // Remis en place, le fil ne porte que les notes d'avant la casse : ni une fusion, ni un départ de plus.
    fs::remove_dir(&chemin).unwrap();
    fs::rename(&cote, &chemin).unwrap();
    assert!(fs::read(&chemin).unwrap() == avant, "rien n'est écrit au fil après la casse");
    let attendu = ["vague 3", "assemblage", "tâche a", "tâche b", "tâche c", "bail 1", "début 1", "bail 1", "début 1"];
    assert_eq!(histoire(&fil(&kraken)), attendu);
    assert_eq!(git(&depot, &["rev-parse", &branche]), base, "la branche n'a pas bougé");
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

/// Le plan d'une tâche `a` qui peut changer ces `fichiers`, et seulement eux.
fn plan_de_a(fichiers: &str) -> Plan {
    let fichiers = format!("fichiers : {fichiers}");
    plan(&["plan 1", "objectif : Écrire dedans.txt", "preuve : exit 0", "tache a", fichiers.as_str(), "    Écrire a"])
}

/// La consigne que la fausse main de `main_hors_fichiers` a gardée à sa tentative `n`, dans `racine`, son dossier,
/// en fins de ligne `\n`.
fn consigne_de(racine: &Path, n: usize) -> String {
    let octets = fs::read(racine.join(format!("entree-{n}.txt"))).unwrap();
    String::from_utf8_lossy(&octets).replace("\r\n", "\n")
}

/// L'erreur, texte de la fin rouge de la tentative `n` de la tâche 1 : le refus qui l'a rougie.
fn fin_de(evenements: &[Evenement], n: usize) -> String {
    let fin = |evenement: &Evenement| match evenement {
        Evenement::Fin { tentative, texte, .. } if *tentative == n => Some(texte.clone()),
        _ => None,
    };
    evenements.iter().find_map(fin).unwrap_or_else(|| panic!("la fin de la tentative {n} : {evenements:?}"))
}

#[test]
fn une_main_hors_de_ses_fichiers_est_rouge_se_corrige_a_la_relance_et_passe() {
    let racine = racine("hors-fichiers");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_hors_fichiers(&racine, true), DELAI);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan_de_a("dedans.txt"), BAIL).unwrap();
    let branche = branche.expect("la branche d'une vague à plan");
    assemblee(&issues[0].0, &issues[0].1);
    // Rouge à la première tentative, verte à la relance, puis prise et assemblée.
    let evenements = fil(&kraken);
    let attendu = [
        "tâche a",
        "bail 1",
        "début 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "fin 2 Vert Constate",
        "verdict Prise Constate",
        "fusion",
    ];
    assert_eq!(histoire_de(&evenements, 1), attendu);
    // L'erreur de la première est le refus 7013 en trois lignes : son code et sa règle, les chemins fautifs et ceux de
    // `fichiers`, ce qu'il faut faire. Ce n'est pas une tâche refusée : elle ne commence pas par `refusée (`.
    let erreur = fin_de(&evenements, 1);
    let lignes: Vec<&str> = erreur.lines().collect();
    let [code, preuve, correction] = lignes[..] else { panic!("trois lignes : {erreur}") };
    assert_eq!(code, format!("7013 vague.hors_fichiers : {}", Code::HorsFichiers.regle()));
    assert_eq!(preuve, r#"la main de la tâche a a changé, hors de ses fichiers ("dedans.txt") : "dehors.txt""#);
    assert!(correction.starts_with("Remets chacun de ces chemins") && correction.contains("`fichiers`"), "{erreur}");
    // La relance part dans le même bac, et reçoit cette erreur en contexte, avec les chemins fautifs et la règle ;
    // la première consigne n'en portait pas.
    assert!(!consigne_de(&racine, 1).contains("Tentative pr"));
    assert!(consigne_de(&racine, 2).contains(&format!("Tentative précédente échouée :\n{erreur}")));
    let bac = bac_de(&kraken, &vague_du_fil(&kraken), 1);
    assert!(!bac.join("dehors.txt").exists(), "la main a ôté dehors.txt de son bac");
    // Elle est assemblée, et seul son fichier est dans la branche de la vague.
    assert_eq!(fichiers(&depot, &branche), ["dedans.txt"]);
    assert_eq!(fusions(&evenements).len(), 1);
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_main_qui_ne_se_corrige_pas_est_classee_et_son_bac_reste() {
    let racine = racine("hors-fichiers-obstinee");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_hors_fichiers(&racine, false), DELAI);
    let (branche, issues) = lancer(&depot, &kraken, &profil, &plan_de_a("dedans.txt"), BAIL).unwrap();
    let branche = branche.expect("la branche d'une vague à plan");
    // Classée, et non refusée : son erreur est le refus 7013 de sa seconde tentative, avec ses chemins.
    let erreur = issues[0].1.as_ref().unwrap_err();
    assert!(erreur.starts_with(&format!("7013 vague.hors_fichiers : {}\n", Code::HorsFichiers.regle())), "{erreur}");
    assert!(erreur.contains(r#"hors de ses fichiers ("dedans.txt") : "dehors.txt""#), "{erreur}");
    let evenements = fil(&kraken);
    let attendu = [
        "tâche a",
        "bail 1",
        "début 1",
        "fin 1 Rouge Constate",
        "bail 2",
        "début 2",
        "fin 2 Rouge Constate",
        "verdict Classee Constate",
    ];
    assert_eq!(histoire_de(&evenements, 1), attendu);
    let vagues = etat(&evenements);
    let main = &vagues[0].mains[&1];
    let classee = (main.verdict, &main.issue, main.texte());
    assert_eq!(classee, (Some((Verdict::Classee, Foi::Constate)), &None, erreur.as_str()), "le fil la rejoue classée");
    // La relance a reçu l'erreur de la première, dans le même bac, et n'a rien corrigé.
    assert!(consigne_de(&racine, 2).contains(&fin_de(&evenements, 1)));
    // Son bac reste, avec ce que la main y a laissé, sans commit ; la branche de la vague n'a pas bougé.
    let bac = bac_de(&kraken, &vague_du_fil(&kraken), 1);
    assert!(bac.join("dedans.txt").is_file() && bac.join("dehors.txt").is_file(), "le bac est gardé");
    assert_eq!(git(&bac, &["rev-parse", "HEAD"]), base, "aucun commit dans le bac");
    assert_eq!(git(&depot, &["rev-parse", &branche]), base, "la branche de la vague reste à sa base");
    assert!(fusions(&evenements).is_empty(), "aucune fusion n'est notée");
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_casse_autre_que_celle_de_git_ne_couvre_pas_le_travail() {
    let racine = racine("hors-fichiers-casse");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    // La main écrit `dedans.txt` et `dehors.txt` ; `fichiers` écrit `Dedans.txt` : git rend `dedans.txt`, qui n'est
    // pas couvert, sous Windows comme ailleurs. `dehors.txt`, écrit comme git le rend, l'est.
    let profil = Profil::faux(main_hors_fichiers(&racine, true), DELAI);
    let (_, issues) = lancer(&depot, &kraken, &profil, &plan_de_a("Dedans.txt dehors.txt"), BAIL).unwrap();
    let erreur = issues[0].1.as_ref().unwrap_err();
    assert!(erreur.starts_with("7013 vague.hors_fichiers : "), "{erreur}");
    assert!(erreur.contains(r#"hors de ses fichiers ("Dedans.txt", "dehors.txt") : "dedans.txt""#), "{erreur}");
    // La relance a ôté dehors.txt, mais dedans.txt, écrit sous sa casse, reste hors de `Dedans.txt` : classée.
    assert_eq!(histoire_de(&fil(&kraken), 1).last().map(String::as_str), Some("verdict Classee Constate"));
    let _ = fs::remove_dir_all(&racine);
}

/// Les mains vivantes que dit le fil : une de plus à chaque `main.debut`, une de moins à chaque `main.fin`. Rend le
/// plus qu'il y en ait eu ensemble, ce qu'il en reste à la fin, et les mains dans l'ordre de leurs débuts. Le fil
/// note le début d'une main avant qu'elle ne parte et sa fin après son retour : il en compte au moins autant qu'il y
/// en a.
fn mains_vivantes(evenements: &[Evenement]) -> (usize, usize, Vec<usize>) {
    let (mut vivantes, mut plus, mut departs) = (0_usize, 0_usize, Vec::new());
    for evenement in evenements {
        match evenement {
            Evenement::Debut { main, .. } => {
                vivantes += 1;
                plus = plus.max(vivantes);
                departs.push(*main);
            }
            Evenement::Fin { .. } => vivantes -= 1,
            _ => {}
        }
    }
    (plus, vivantes, departs)
}

#[test]
fn six_independantes_ne_font_jamais_plus_de_quatre_mains_a_la_fois() {
    let racine = racine("quatre-mains");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let plan = plan(&[
        "plan 1",
        "objectif : Six indépendantes",
        "preuve : exit 0",
        "tache a",
        "    Écrire a",
        "tache b",
        "    Écrire b",
        "tache c",
        "    Écrire c",
        "tache d",
        "    Écrire d",
        "tache e",
        "    Écrire e",
        "tache f",
        "    Écrire f",
    ]);
    // Ce que le test voit pendant que les quatre premières mains attendent, jugé une fois la vague finie : un plafond
    // qui cède ne laisse pas des mains attendre leurs 115 s.
    let mut vu = (0, true, Vec::new());
    let (branche, issues) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        // Quatre mains partent ensemble, les quatre premières du plan, chacune dans son bac. On leur laisse le temps
        // de faire partir les deux autres, si le plafond ne les retenait pas.
        attendre("les quatre marques", vague, || marques(&kraken).len() == 4);
        thread::sleep(Duration::from_secs(1));
        let v = vague_du_fil(&kraken);
        let sans_bac = !bac_de(&kraken, &v, 5).exists() && !bac_de(&kraken, &v, 6).exists();
        vu = (marques(&kraken).len(), sans_bac, histoire_de(&fil(&kraken), 5));
        // Une main qui finit cède sa place à la suivante du plan : la cinquième part.
        liberer(&bac_de(&kraken, &v, 1));
        attendre("la marque de e", vague, || marques(&kraken).contains(&bac_de(&kraken, &v, 5)));
        tout_liberer(&kraken, vague);
    });
    assert_eq!(vu.0, 4, "quatre mains tournent, la cinquième attend");
    assert!(vu.1, "ni bac pour e, ni pour f");
    assert_eq!(vu.2, ["tâche e"], "e n'a ni bail ni début");
    let branche = branche.expect("la branche d'une vague à plan");
    for (id, issue) in &issues {
        assemblee(id, issue);
    }
    // Le fil dit les mains vivantes : jamais plus de quatre, et elles partent dans l'ordre du plan.
    let (plus, restent, departs) = mains_vivantes(&fil(&kraken));
    assert_eq!((plus, restent), (4, 0));
    assert_eq!(departs, [1, 2, 3, 4, 5, 6]);
    worktree_a_la_tete(&depot, &kraken, &branche);
    rien_derriere(&racine, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn dix_copies_rendent_dix_resultats_sans_plus_de_quatre_mains_a_la_fois() {
    let racine = racine("dix-copies");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let profil = Profil::faux(main_d_attente(&racine), DELAI);
    let plan = Plan::copies("objectif", None, 10);
    let mut tournent = 0;
    let (_, mains) = pendant_la_vague(&depot, &kraken, &profil, &plan, |vague| {
        // Quatre copies partent et attendent ; on leur laisse le temps de faire partir les autres, si le plafond ne les
        // retenait pas. Puis chaque copie qui se pose est libérée, et la suivante part à son tour.
        attendre("les quatre marques", vague, || marques(&kraken).len() == 4);
        thread::sleep(Duration::from_secs(1));
        tournent = marques(&kraken).len();
        tout_liberer(&kraken, vague);
    });
    assert_eq!(tournent, 4, "quatre copies tournent, les six autres attendent");
    // Dix résultats, chacun pris (sans vérification, sur la seule ligne de la main), plus lentement que d'un coup.
    assert_eq!(mains.len(), 10);
    for (id, issue) in &mains {
        assert!(issue.is_ok(), "{id} : {issue:?}");
    }
    // Jamais plus de quatre mains vivantes, et chaque copie part à son tour, dans l'ordre du plan.
    let (plus, restent, departs) = mains_vivantes(&fil(&kraken));
    assert_eq!((plus, restent), (4, 0));
    assert_eq!(departs, (1..=10).collect::<Vec<_>>());
    let _ = fs::remove_dir_all(&racine);
}

/// Un programme que le système lance lui-même comme `claude`, qui lui passe ses arguments : il note le modèle que
/// lui donne `--model` dans `modeles-<bac>.txt` de `dir`, hors de son dossier temporaire, qui se recrée vide à chaque
/// tentative, puis sort en erreur sans répondre. Rend son chemin.
fn main_qui_note_son_modele(dir: &Path) -> PathBuf {
    let d = dir.to_string_lossy();
    let (nom, corps) = if cfg!(windows) {
        let corps = format!(
            "@echo off\r\nfor %%I in (.) do set bac=%%~nxI\r\n:boucle\r\nif \"%~1\"==\"\" exit /b 1\r\n\
             if \"%~1\"==\"--model\" goto note\r\nshift\r\ngoto boucle\r\n:note\r\n\
             >>\"{d}\\modeles-%bac%.txt\" echo %~2\r\nexit /b 1\r\n"
        );
        ("modele.cmd", corps)
    } else {
        let corps = format!(
            "#!/bin/sh\nbac=$(basename \"$(pwd)\")\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"--model\" ]; \
             then\n    echo \"$2\" >> \"{d}/modeles-$bac.txt\"\n    exit 1\n  fi\n  shift\ndone\nexit 1\n"
        );
        ("modele.sh", corps)
    };
    let chemin = dir.join(nom);
    fs::write(&chemin, corps).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&chemin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    chemin
}

#[test]
fn chaque_main_a_le_modele_de_sa_tache_puis_celui_de_sa_relance() {
    let racine = racine("modeles");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    // Le profil dit `profil-modele` puis `profil-relance` ; a donne les siens, b seulement le premier, c aucun.
    let programme = main_qui_note_son_modele(&racine);
    let texte = format!(
        "sorte = claude\nprogramme = {}\nmodele = profil-modele\nrelance = profil-relance\n",
        programme.display()
    );
    let profil = profil_declare(&texte, &kraken, &depot);
    let plan = plan(&[
        "plan 1",
        "objectif : Trois tâches, trois modèles",
        "preuve : exit 0",
        "tache a",
        "modele : tache-a",
        "relance : relance-a",
        "    Tâche a",
        "tache b",
        "modele : tache-b",
        "    Tâche b",
        "tache c",
        "    Tâche c",
    ]);
    let (_, issues) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap();
    // Le programme ne répond pas : chaque main est rouge deux fois, puis classée. Seul compte le modèle qu'elle a reçu.
    for (id, issue) in &issues {
        let erreur = issue.as_ref().unwrap_err();
        assert!(!erreur.starts_with("refusée ("), "{id} est classée : {erreur}");
    }
    let v = vague_du_fil(&kraken);
    let recus = |k: usize| -> Vec<String> {
        let bac = bac_de(&kraken, &v, k);
        let nom = bac.file_name().expect("le bac a un nom").to_string_lossy();
        let notes = fs::read_to_string(racine.join(format!("modeles-{nom}.txt"))).unwrap();
        notes.lines().map(|ligne| ligne.trim().to_string()).collect()
    };
    // Le `--model` de chaque main : celui de sa tâche à la première tentative, sa relance à la seconde, et à défaut
    // de l'un ou de l'autre celui du profil.
    assert_eq!(recus(1), ["tache-a", "relance-a"]);
    assert_eq!(recus(2), ["tache-b", "profil-relance"]);
    assert_eq!(recus(3), ["profil-modele", "profil-relance"]);
    // Le `main.lancement` de chaque tentative note le même modèle.
    let lancements = |evenement: Evenement| match evenement {
        Evenement::Lancement { main, tentative, modele, .. } => Some((main, tentative, modele)),
        _ => None,
    };
    let mut notes: Vec<(usize, usize, String)> = fil(&kraken).into_iter().filter_map(lancements).collect();
    notes.sort();
    let attendu = [
        (1, 1, "tache-a"),
        (1, 2, "relance-a"),
        (2, 1, "tache-b"),
        (2, 2, "profil-relance"),
        (3, 1, "profil-modele"),
        (3, 2, "profil-relance"),
    ];
    assert_eq!(notes, attendu.map(|(main, tentative, modele)| (main, tentative, modele.to_string())));
    // Le profil de la vague n'a pas bougé : ses modèles sont ceux de son fichier.
    let demande = |relance: bool| profil.demande(&kraken, None, relance).declaration.expect("une main bornée").modele;
    assert_eq!((demande(false), demande(true)), ("profil-modele".to_string(), "profil-relance".to_string()));
    let _ = fs::remove_dir_all(&racine);
}

/// Une fausse main qui regarde si Kraken a posé `prepare.txt` dans son bac avant qu'elle parte : elle écrit `vu` ou
/// `pas-vu` dans `vu-<bac>.txt`, puis finit sur `PREUVE: vert`. Elle n'agit que dans son bac.
fn main_qui_regarde(dir: &Path) -> Vec<String> {
    if cfg!(windows) {
        let script = dir.join("regarde.bat");
        fs::write(
            &script,
            "@echo off\r\n\
             for %%I in (.) do set bac=%%~nxI\r\n\
             findstr \"^\" > nul\r\n\
             if exist prepare.txt (echo vu> vu-%bac%.txt) else (echo pas-vu> vu-%bac%.txt)\r\n\
             echo PREUVE: vert\r\n",
        )
        .unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("regarde.sh");
        fs::write(
            &script,
            "bac=$(basename \"$(pwd)\")\n\
             cat > /dev/null\n\
             if [ -f prepare.txt ]; then echo vu > \"vu-$bac.txt\"; else echo pas-vu > \"vu-$bac.txt\"; fi\n\
             echo 'PREUVE: vert'\n",
        )
        .unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

#[test]
fn la_preparation_pose_son_fichier_dans_le_bac_avant_la_main_et_son_refus_la_retient() {
    let racine = racine("preparation");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_qui_regarde(&racine), DELAI);
    // Trois copies : la préparation pose `prepare.txt` dans le bac de la première et de la troisième, et refuse celui
    // de la deuxième.
    let preparer = |bac: &Path| -> Result<(), Refus> {
        if nom(bac).ends_with("-2") {
            return Err(Refus::new(Code::GitImpossible, "la préparation de la main 2 est ratée", "Relance."));
        }
        ecrire(bac, "prepare.txt", "posé par Kraken");
        Ok(())
    };
    let liens = Liens { preparation: Some(&preparer), ..Liens::default() };
    let plan = Plan::copies("Regarder", None, 3);
    let menee = lancer_avec(&depot, &kraken, &profil, &plan, BAIL, &base, liens).unwrap();
    let vu = |k: usize| {
        let bac = bac_de(&kraken, &menee.id, k);
        fs::read_to_string(bac.join(format!("vu-{}.txt", nom(&bac)))).map(|texte| texte.trim().to_string())
    };
    // Les mains 1 et 3 ont vu le fichier à leur départ, et sont prises ; la main 2 n'est pas partie.
    assert_eq!((vu(1).ok().as_deref(), vu(3).ok().as_deref()), (Some("vu"), Some("vu")));
    assert!(vu(2).is_err(), "la main 2 n'est pas partie");
    let [(_, premiere), (_, deuxieme), (_, troisieme)] = &menee.issues[..] else { panic!("{:?}", menee.issues) };
    assert!(premiere.is_ok() && troisieme.is_ok(), "{:?}", menee.issues);
    let refus = deuxieme.as_ref().unwrap_err();
    assert!(refus.starts_with("refusée (7009) : "), "{refus}");
    assert!(refus.contains("la préparation de la main 2 est ratée"), "{refus}");
    // Son refus est noté, et rien de plus de sa tâche : ni bail ni début. Son bac demeure, avec ce que git y a posé.
    let evenements = fil(&kraken);
    assert_eq!(histoire_de(&evenements, 2), ["refus 7009"]);
    assert!(bac_de(&kraken, &menee.id, 2).is_dir());
    // Les mains 1 et 3 ont leur histoire ordinaire, sans que la préparation y paraisse.
    for k in [1, 3] {
        assert_eq!(histoire_de(&evenements, k), ["bail 1", "début 1", "fin 1 Vert Declare", "verdict Prise Declare"]);
    }
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_vague_qui_en_corrige_une_autre_note_son_lien_apres_son_assemblage_et_avant_ses_taches() {
    let racine = racine("corrige");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_de_vague(&racine), DELAI);
    let plan = plan(&["plan 1", "objectif : Corriger", "preuve : exit 0", "tache correction", "    Corriger"]);
    let liens = Liens { decoupage: Some("d"), corrige: Some("v"), ..Liens::default() };
    let menee = lancer_avec(&depot, &kraken, &profil, &plan, BAIL, &base, liens).unwrap();
    assemblee("correction", &menee.issues[0].1);
    // Dans l'ordre : l'ouverture, l'assemblage, le découpage qu'elle joue, la vague qu'elle corrige, ses tâches.
    let evenements = fil(&kraken);
    assert_eq!(&histoire(&evenements)[..5], ["vague 1", "assemblage", "joue", "correction", "tâche correction"]);
    let lien = Evenement::Correction { vague: menee.id.clone(), corrige: "v".into() };
    assert_eq!(evenements.iter().filter(|e| **e == lien).count(), 1);
    // Le rejeu en garde la vague corrigée.
    let vagues = etat(&evenements);
    assert_eq!(vagues[0].corrige.as_deref(), Some("v"));
    // Sans lien de correction, la vague n'en note aucun.
    let sans = lancer_sur(&depot, &kraken, &profil, &plan, BAIL, &base).unwrap();
    let sans =
        fil(&kraken).into_iter().filter(|e| e.vague() == sans.id).any(|e| matches!(e, Evenement::Correction { .. }));
    assert!(!sans);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_vague_de_relecteur_note_son_lien_apres_son_ouverture_et_avant_ses_mains() {
    let racine = racine("relit");
    let (depot, kraken) = (depot(&racine), racine.join("kraken"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = Profil::faux(main_qui_regarde(&racine), DELAI);
    let plan = Plan::copies("Relire", None, 1);
    let liens = Liens { relit: Some("v"), ..Liens::default() };
    let menee = lancer_avec(&depot, &kraken, &profil, &plan, BAIL, &base, liens).unwrap();
    // Dans l'ordre : l'ouverture, la vague que relit son relecteur, puis sa main.
    let evenements = fil(&kraken);
    assert_eq!(&histoire(&evenements)[..3], ["vague 1", "relecteur", "bail 1"]);
    let lien = Evenement::Relecteur { vague: menee.id.clone(), relue: "v".into() };
    assert_eq!(evenements.iter().filter(|e| **e == lien).count(), 1);
    // Ce n'est pas une relecture : aucun `vague.relecture` ne se note.
    assert!(!evenements.iter().any(|e| matches!(e, Evenement::Relecture { .. })));
    // Le rejeu en garde la vague relue.
    let vagues = etat(&evenements);
    assert_eq!(vagues[0].relit.as_deref(), Some("v"));
    // Sans lien de relecteur, la vague n'en note aucun.
    let sans = lancer_sur(&depot, &kraken, &profil, &plan, BAIL, &base).unwrap();
    let sans =
        fil(&kraken).into_iter().filter(|e| e.vague() == sans.id).any(|e| matches!(e, Evenement::Relecteur { .. }));
    assert!(!sans);
    let _ = fs::remove_dir_all(&racine);
}

// L'usage noté au fil et le quota du jour.
mod quota {
    use super::*;
    use crate::branchements::dossier::lire;
    use crate::coeur::profil;
    use std::ffi::OsString;

    /// Un profil `claude` lu de ce texte, sans fichier ni bornes : ce que `usages_de` lui demande, son `prix`.
    fn profil_lu(texte: &str) -> Profil {
        profil::lire("essai", texte, None, None).unwrap()
    }

    /// Un profil à `api`, qui a un prix pour `deepseek-flash`, 0,15 $ l'entrée et 0,60 $ la sortie par million.
    const AVEC_PRIX: &str = "sorte = claude\napi = https://api.exemple.test/anthropic\ncle = CLE_ESSAI\n\
                         prix = deepseek-flash 0.15 0.60\n";

    /// Des jetons : entrée, sortie, cache écrit, cache lu, réflexion.
    fn jetons(entree: u64, sortie: u64, cache_ecrit: u64, cache_lu: u64, reflexion: u64) -> Jetons {
        Jetons { entree, sortie, cache_ecrit, cache_lu, reflexion }
    }

    /// L'usage d'un modèle, de la base que Claude Code a dite.
    fn modele_de(modele: &str, jetons: Jetons, micro_dollars: u64, base: Option<&str>) -> UsageModele {
        UsageModele { modele: modele.into(), jetons, micro_dollars, base: base.map(Into::into) }
    }

    /// Le constat d'une main, de cette source de clé et de cet usage.
    fn constat_de(source_cle: Option<&str>, jetons: Jetons, micro_dollars: u64, modeles: Vec<UsageModele>) -> Constat {
        let usage = Usage { jetons, micro_dollars, duree_ms: 900, tours: 2, modeles };
        Constat { usage, source_cle: source_cle.map(Into::into), ..Constat::default() }
    }

    /// Ce que les lignes d'usage disent : le modèle, la dépense et la base de chacune.
    fn dit(lignes: &[Evenement]) -> Vec<(String, u64, Base)> {
        let mots = |ligne: &Evenement| match ligne {
            Evenement::Usage { modele, micro_dollars, base, .. } => (modele.clone(), *micro_dollars, *base),
            autre => panic!("une ligne d'usage : {autre:?}"),
        };
        lignes.iter().map(mots).collect()
    }

    #[test]
    fn l_usage_d_une_main_au_forfait_garde_le_cout_que_claude_code_annonce() {
        let haiku = modele_de("claude-haiku-5-5", jetons(10, 20, 30, 40, 50), 2500, Some("list"));
        let sonnet = modele_de("claude-sonnet-5-5", jetons(1, 2, 3, 4, 5), 700, Some("list"));
        let constat = constat_de(Some("none"), jetons(11, 22, 33, 44, 55), 3200, vec![haiku, sonnet]);
        let lignes = usages_de("v", 1, 1, "sonnet", &profil_lu("sorte = claude\n"), &constat);
        // Une ligne par modèle du détail, dans son ordre, et jamais le total.
        assert_eq!(
            dit(&lignes),
            [
                ("claude-haiku-5-5".to_string(), 2500, Base::Forfait),
                ("claude-sonnet-5-5".to_string(), 700, Base::Forfait)
            ]
        );
        let Evenement::Usage { vague, main, tentative, entree, sortie, cache_ecrit, cache_lu, reflexion, .. } =
            &lignes[0]
        else {
            panic!("{lignes:?}")
        };
        assert_eq!((vague.as_str(), *main, *tentative), ("v", 1, 1));
        assert_eq!((*entree, *sortie, *cache_ecrit, *cache_lu, *reflexion), (10, 20, 30, 40, 50));
    }

    #[test]
    fn le_cout_de_claude_code_ne_passe_jamais_hors_du_forfait_ou_d_une_base_list() {
        let profil = profil_lu("sorte = claude\n");
        // Sous l'abonnement, une base qui n'est pas `list`, ou absente : inconnu, à zéro.
        let inconnue = modele_de("a", jetons(1, 1, 0, 0, 0), 900, Some("unknown"));
        let absente = modele_de("b", jetons(1, 1, 0, 0, 0), 900, None);
        let constat = constat_de(Some("none"), jetons(2, 2, 0, 0, 0), 1800, vec![inconnue, absente]);
        let lignes = usages_de("v", 1, 1, "sonnet", &profil, &constat);
        assert_eq!(dit(&lignes), [("a".to_string(), 0, Base::Inconnu), ("b".to_string(), 0, Base::Inconnu)]);
        // Une base `list` sous une clé, non sous l'abonnement : ce n'est pas un forfait, et sans prix, c'est inconnu.
        let liste = modele_de("c", jetons(1, 1, 0, 0, 0), 900, Some("list"));
        let constat = constat_de(Some("ANTHROPIC_API_KEY"), jetons(1, 1, 0, 0, 0), 900, vec![liste.clone()]);
        assert_eq!(dit(&usages_de("v", 1, 1, "sonnet", &profil, &constat)), [("c".to_string(), 0, Base::Inconnu)]);
        // Sans `apiKeySource` lu non plus.
        let constat = constat_de(None, jetons(1, 1, 0, 0, 0), 900, vec![liste]);
        assert_eq!(dit(&usages_de("v", 1, 1, "sonnet", &profil, &constat)), [("c".to_string(), 0, Base::Inconnu)]);
    }

    #[test]
    fn sous_un_profil_a_api_le_forfait_que_la_main_declare_ne_vaut_pas() {
        // Une ligne qui se dit `none` et `list` : sous un `api`, elle est estimée au prix du profil, sinon inconnue.
        let ligne = modele_de("deepseek-flash", jetons(1_000_000, 0, 0, 0, 0), 900, Some("list"));
        let constat = constat_de(Some("none"), jetons(1_000_000, 0, 0, 0, 0), 900, vec![ligne]);
        let lignes = usages_de("v", 1, 1, "sonnet", &profil_lu(AVEC_PRIX), &constat);
        assert_eq!(dit(&lignes), [("deepseek-flash".to_string(), 150_000, Base::Estime)]);
        let sans_prix = "sorte = claude\napi = https://api.exemple.test/anthropic\ncle = CLE_ESSAI\n";
        let lignes = usages_de("v", 1, 1, "sonnet", &profil_lu(sans_prix), &constat);
        assert_eq!(dit(&lignes), [("deepseek-flash".to_string(), 0, Base::Inconnu)]);
    }

    #[test]
    fn un_resultat_perime_note_son_usage_juste_avant_sa_perimee_et_il_compte_pour_le_quota() {
        let racine = racine("perimee-usage");
        let (depot, kraken) = (depot(&racine), racine.join("kraken"));
        fs::create_dir_all(&kraken).unwrap();
        let texte = format!("{AVEC_PRIX}modele = deepseek-flash\n");
        let mut profil = profil_declare(&texte, &kraken, &depot);
        profil.fournir(|nom| (nom == "CLE_ESSAI").then(|| OsString::from("leurre"))).unwrap();
        let plan = Plan::copies("objectif", None, 1);
        let mut vague = Vague {
            id: "v".into(),
            base: "base".into(),
            depot: &depot,
            profil: &profil,
            plan: &plan,
            duree: BAIL,
            fil: Fil::seance(&kraken, "humain"),
            assemblage: None,
            juge: None,
            preparation: None,
            baux: Baux::new(),
            rangs: HashMap::new(),
            taches: vec![Etat::Attente],
            en_vol: vec![(1, 1)],
            arret: None,
        };
        // Le bail de la tentative 1 a expiré : sa tâche est refusée, et son résultat, qui vient, est périmé.
        vague.baux.ouvrir(1, Instant::now(), Duration::ZERO).unwrap();
        assert_eq!(vague.baux.expirer(Instant::now() + Duration::from_secs(1)), [(1, 1)]);
        // Elle a dépensé 1 000 000 de jetons d'entrée, à 0,15 $ le million.
        let jeton = jetons(1_000_000, 0, 0, 0, 0);
        let modele = modele_de("deepseek-flash", jeton, 0, None);
        let constat = constat_de(Some("ANTHROPIC_API_KEY"), jeton, 0, vec![modele]);
        let pris = Pris { sortie: "PREUVE: vert".into(), prouve: true };
        let message = Message { main: 1, tentative: 1, rendu: (Some(constat), None, Ok(pris)) };
        let (envoi, _recu) = mpsc::channel();
        thread::scope(|s| vague.recevoir(s, &envoi, message));
        let evenements = fil(&kraken);
        assert_eq!(histoire(&evenements), ["usage 1 deepseek-flash Estime", "périmée 1 Vert Constate"]);
        let jour = &instant(maintenant())[..10];
        assert_eq!(depense_du_jour(&kraken, jour).unwrap(), 150_000, "sa dépense compte pour le quota");
        let _ = fs::remove_dir_all(&racine);
    }

    #[test]
    fn l_usage_d_un_profil_a_api_s_estime_au_prix_du_profil() {
        let profil = profil_lu(AVEC_PRIX);
        // 12 000 d'entrée et 40 000 de cache lu à 0,15 $ le million : 7 800 ; 800 de sortie à 0,60 $ : 480, la
        // réflexion (300) étant déjà dans la sortie.
        let flash = modele_de("deepseek-flash", jetons(12_000, 800, 0, 40_000, 300), 123, Some("unknown"));
        let autre = modele_de("autre-modele", jetons(5, 5, 5, 5, 5), 456, None);
        let constat =
            constat_de(Some("ANTHROPIC_API_KEY"), jetons(12_005, 805, 5, 40_005, 305), 579, vec![flash, autre]);
        let lignes = usages_de("v", 2, 2, "deepseek-flash", &profil, &constat);
        // Le coût de Claude Code (123, 456) n'est pas repris ; un modèle sans prix prend celui de la tentative :
        // 15 jetons d'entrée et de cache à 0,15 $, 5 de sortie à 0,60 $ : 5,25, soit 6 au micro-dollar supérieur.
        assert_eq!(
            dit(&lignes),
            [("deepseek-flash".to_string(), 8280, Base::Estime), ("autre-modele".to_string(), 6, Base::Estime)]
        );
        // Sans prix pour l'un ni pour l'autre : inconnu, à zéro.
        let lignes = usages_de("v", 2, 2, "sonnet", &profil, &constat);
        assert_eq!(
            dit(&lignes),
            [("deepseek-flash".to_string(), 8280, Base::Estime), ("autre-modele".to_string(), 0, Base::Inconnu)]
        );
        let sans = profil_lu("sorte = claude\n");
        let lignes = usages_de("v", 2, 2, "sonnet", &sans, &constat);
        assert_eq!(
            dit(&lignes),
            [("deepseek-flash".to_string(), 0, Base::Inconnu), ("autre-modele".to_string(), 0, Base::Inconnu)]
        );
        // Cinq prix : chaque compteur au sien, la réflexion pour rien.
        let cinq = profil_lu(&AVEC_PRIX.replace("0.15 0.60", "1 2 0.5 1.5"));
        let tout = modele_de("deepseek-flash", jetons(1_000, 100, 10, 10_000, 7), 0, None);
        let constat = constat_de(Some("ANTHROPIC_API_KEY"), jetons(1_000, 100, 10, 10_000, 7), 0, vec![tout]);
        assert_eq!(
            dit(&usages_de("v", 1, 1, "x", &cinq, &constat)),
            [("deepseek-flash".to_string(), 6_215, Base::Estime)]
        );
        // Le cache écrit est au prix d'entrée : 1 000 000 jetons de cache écrit coûtent 150 000.
        let ecrit = modele_de("deepseek-flash", jetons(0, 0, 1_000_000, 0, 0), 0, None);
        let constat = constat_de(Some("ANTHROPIC_API_KEY"), jetons(0, 0, 1_000_000, 0, 0), 0, vec![ecrit]);
        assert_eq!(
            dit(&usages_de("v", 1, 1, "x", &profil, &constat)),
            [("deepseek-flash".to_string(), 150_000, Base::Estime)]
        );
        // Arrondi au micro-dollar supérieur : un quota ne se sous-estime pas.
        let un = modele_de("deepseek-flash", jetons(1, 0, 0, 0, 0), 0, None);
        let constat = constat_de(Some("ANTHROPIC_API_KEY"), jetons(1, 0, 0, 0, 0), 0, vec![un]);
        assert_eq!(
            dit(&usages_de("v", 1, 1, "x", &profil, &constat)),
            [("deepseek-flash".to_string(), 1, Base::Estime)]
        );
    }

    #[test]
    fn sans_detail_le_total_s_ecrit_sous_le_modele_de_la_tentative() {
        let total = constat_de(Some("ANTHROPIC_API_KEY"), jetons(1_000_000, 0, 0, 0, 0), 77, Vec::new());
        // Le modèle de la tentative a un prix : estimé.
        let lignes = usages_de("v", 1, 2, "deepseek-flash", &profil_lu(AVEC_PRIX), &total);
        assert_eq!(dit(&lignes), [("deepseek-flash".to_string(), 150_000, Base::Estime)]);
        let Evenement::Usage { tentative, entree, .. } = &lignes[0] else { panic!("{lignes:?}") };
        assert_eq!((*tentative, *entree), (2, 1_000_000));
        // Il n'en a pas : inconnu, le total de Claude Code (77) non repris.
        let lignes = usages_de("v", 1, 1, "sonnet", &profil_lu(AVEC_PRIX), &total);
        assert_eq!(dit(&lignes), [("sonnet".to_string(), 0, Base::Inconnu)]);
        // Sous l'abonnement, un total sans base `list` ne peut pas être un forfait.
        let abonnement = constat_de(Some("none"), jetons(1, 1, 0, 0, 0), 77, Vec::new());
        let lignes = usages_de("v", 1, 1, "sonnet", &profil_lu("sorte = claude\n"), &abonnement);
        assert_eq!(dit(&lignes), [("sonnet".to_string(), 0, Base::Inconnu)]);
    }

    #[test]
    fn une_tentative_a_l_usage_vide_n_ecrit_rien() {
        let profil = profil_lu(AVEC_PRIX);
        assert!(usages_de("v", 1, 1, "sonnet", &profil, &Constat::default()).is_empty());
        // La durée et les tours ne sont pas un usage.
        let vide = constat_de(Some("none"), Jetons::default(), 0, Vec::new());
        assert!(usages_de("v", 1, 1, "sonnet", &profil, &vide).is_empty());
    }

    #[test]
    fn un_nom_de_modele_que_le_fil_ne_lirait_pas_s_ecrit_point_d_interrogation() {
        let louche = modele_de("é t", jetons(1, 1, 0, 0, 0), 0, None);
        let vide = modele_de("", jetons(1, 1, 0, 0, 0), 0, None);
        let constat = constat_de(Some("none"), jetons(2, 2, 0, 0, 0), 0, vec![louche, vide]);
        let lignes = usages_de("v", 1, 1, "sonnet", &profil_lu("sorte = claude\n"), &constat);
        assert_eq!(dit(&lignes), [("?".to_string(), 0, Base::Inconnu), ("?".to_string(), 0, Base::Inconnu)]);
    }

    #[test]
    fn l_usage_note_au_fil_se_relit() {
        let racine = racine("usage-releve");
        let kraken = racine.join("kraken");
        fs::create_dir_all(&kraken).unwrap();
        let forfait = constat_de(
            Some("none"),
            jetons(10, 20, 30, 40, 50),
            2500,
            vec![modele_de("claude-haiku-5-5", jetons(10, 20, 30, 40, 50), 2500, Some("list"))],
        );
        let estime = constat_de(
            Some("ANTHROPIC_API_KEY"),
            jetons(12_000, 800, 0, 40_000, 300),
            0,
            vec![modele_de("deepseek-flash", jetons(12_000, 800, 0, 40_000, 300), 0, None)],
        );
        let inconnu = constat_de(Some("none"), jetons(7, 8, 9, 10, 11), 0, Vec::new());
        let mut ecrites = Vec::new();
        let seance = Fil::seance(&kraken, "humain");
        for (profil, constat) in [
            (profil_lu("sorte = claude\n"), forfait),
            (profil_lu(AVEC_PRIX), estime),
            (profil_lu("sorte = claude\n"), inconnu),
        ] {
            for ligne in usages_de("v", 1, 1, "sonnet", &profil, &constat) {
                seance.noter(&ligne).unwrap();
                ecrites.push(ligne);
            }
        }
        let Lecture { notes, ecarts } = lire(&kraken).unwrap();
        assert!(ecarts.is_empty(), "{ecarts:?}");
        let relues: Vec<Evenement> = notes.iter().map(|note| note.evenement.clone()).collect();
        assert_eq!(relues, ecrites);
        assert_eq!(
            dit(&relues),
            [
                ("claude-haiku-5-5".to_string(), 2500, Base::Forfait),
                ("deepseek-flash".to_string(), 8280, Base::Estime),
                ("sonnet".to_string(), 0, Base::Inconnu)
            ]
        );
        // Déclarées, à l'origine de la séance.
        assert!(notes.iter().all(|note| (note.origine.as_str(), note.origine_foi) == ("humain", Foi::Declare)));
        let _ = fs::remove_dir_all(&racine);
    }

    /// Le fichier du quota du dossier de Kraken.
    fn poser_le_quota(kraken: &Path, texte: &str) {
        fs::create_dir_all(kraken.join("profils")).unwrap();
        fs::write(kraken.join("profils").join("quota"), texte).unwrap();
    }

    /// Une dépense déjà au fil, d'une autre vague, aujourd'hui.
    fn depense(kraken: &Path, micro_dollars: u64, base: Base) {
        fs::create_dir_all(kraken).unwrap();
        let note = Evenement::Usage {
            vague: "amorce".into(),
            main: 1,
            tentative: 1,
            modele: "m".into(),
            entree: 1,
            sortie: 1,
            cache_ecrit: 0,
            cache_lu: 0,
            reflexion: 0,
            micro_dollars,
            base,
        };
        Fil::seance(kraken, "humain").noter(&note).unwrap();
    }

    /// Ce que le fil dit de la vague de la main brute qui part ou non : son histoire, sans les usages d'amorce.
    fn histoire_de_la_vague(kraken: &Path) -> Vec<String> {
        let evenements: Vec<Evenement> = fil(kraken).into_iter().filter(|e| e.vague() != "amorce").collect();
        histoire(&evenements)
    }

    /// Lance une copie d'une main brute qui finit vert, dans un dépôt neuf, et rend son issue et l'histoire.
    fn lancer_une_main(nom: &str, avant: impl FnOnce(&Path)) -> (Result<Pris, String>, Vec<String>, String) {
        let racine = racine(nom);
        let (depot, kraken) = (depot(&racine), racine.join("kraken"));
        avant(&kraken);
        let profil = Profil::faux(main_de_vague(&racine), DELAI);
        let (id, issue) =
            lancer(&depot, &kraken, &profil, &Plan::copies("objectif", None, 1), BAIL).unwrap().1.remove(0);
        let histoire = histoire_de_la_vague(&kraken);
        let _ = fs::remove_dir_all(&racine);
        (issue, histoire, id)
    }

    #[test]
    fn une_depense_du_jour_qui_atteint_le_quota_refuse_le_depart_et_se_note() {
        let racine = racine("quota-atteint");
        let (depot, kraken) = (depot(&racine), racine.join("kraken"));
        poser_le_quota(&kraken, "1.00\n");
        depense(&kraken, 600_000, Base::Estime);
        depense(&kraken, 400_000, Base::Estime);
        let profil = Profil::faux(main_de_vague(&racine), DELAI);
        let (id, issue) =
            lancer(&depot, &kraken, &profil, &Plan::copies("objectif", None, 1), BAIL).unwrap().1.remove(0);
        let texte = refusee(&id, &issue, 6030);
        assert!(
            texte.contains("1.000000 $") && texte.contains("quota"),
            "la preuve dit la dépense et le quota : {texte}"
        );
        // Le refus est au fil, sans bail ni début : la main n'est pas partie.
        assert_eq!(histoire_de_la_vague(&kraken), ["vague 1", "refus 6030"]);
        let refus = fil(&kraken).into_iter().find(|e| matches!(e, Evenement::Refus { .. })).unwrap();
        let Evenement::Refus { vague, main, code, regle, preuve, .. } = refus else { unreachable!() };
        assert_eq!((vague.as_str(), main, code), (id.rsplit_once('-').unwrap().0.trim_start_matches("main-"), 1, 6030));
        assert_eq!(regle, Code::Quota.regle());
        assert!(preuve.contains("atteint le quota du jour"), "{preuve}");
        let _ = fs::remove_dir_all(&racine);
    }

    #[test]
    fn le_forfait_et_l_inconnu_ne_comptent_pas_pour_le_quota() {
        // 5 $ au forfait, 0 en inconnu, et 0,999999 $ estimés : sous un quota de 1 $, la main part et prend.
        let (issue, histoire, _) = lancer_une_main("quota-forfait", |kraken| {
            poser_le_quota(kraken, "1\n");
            depense(kraken, 5_000_000, Base::Forfait);
            depense(kraken, 0, Base::Inconnu);
            depense(kraken, 999_999, Base::Estime);
        });
        assert!(issue.is_ok(), "{issue:?}");
        assert_eq!(histoire, ["vague 1", "bail 1", "début 1", "fin 1 Vert Declare", "verdict Prise Declare"]);
    }

    #[test]
    fn sans_fichier_quota_rien_ne_change() {
        let (issue, histoire, _) = lancer_une_main("quota-absent", |kraken| depense(kraken, u64::MAX, Base::Estime));
        assert!(issue.is_ok(), "{issue:?}");
        assert_eq!(histoire, ["vague 1", "bail 1", "début 1", "fin 1 Vert Declare", "verdict Prise Declare"]);
    }

    #[test]
    fn un_quota_mal_forme_refuse_le_depart() {
        for mauvais in ["beaucoup\n", "0\n", "1.234\n", "-1\n", "1\n2\n", ""] {
            let racine = racine("quota-mal-forme");
            let (depot, kraken) = (depot(&racine), racine.join("kraken"));
            poser_le_quota(&kraken, mauvais);
            let profil = Profil::faux(main_de_vague(&racine), DELAI);
            let plan = Plan::copies("objectif", None, 1);
            let (id, issue) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap().1.remove(0);
            let texte = refusee(&id, &issue, 6030);
            assert!(texte.contains("quota"), "{mauvais:?} : {texte}");
            assert_eq!(histoire_de_la_vague(&kraken), ["vague 1", "refus 6030"], "{mauvais:?}");
            let _ = fs::remove_dir_all(&racine);
        }
    }

    /// Lance une copie d'un profil à `api` (celui de `texte`) sous `quota`, qui ne part pas faute de programme, et
    /// rend son issue et l'histoire de sa vague.
    fn lancer_a_api(nom: &str, texte: &str, quota: Option<&str>) -> (Result<Pris, String>, Vec<String>, String) {
        let racine = racine(nom);
        let (depot, kraken) = (depot(&racine), racine.join("kraken"));
        if let Some(quota) = quota {
            poser_le_quota(&kraken, quota);
        }
        let mut profil = profil_declare(texte, &kraken, &depot);
        profil.fournir(|nom| (nom == "CLE_ESSAI").then(|| OsString::from("leurre"))).unwrap();
        let (id, issue) =
            lancer(&depot, &kraken, &profil, &Plan::copies("objectif", None, 1), BAIL).unwrap().1.remove(0);
        let histoire = histoire_de_la_vague(&kraken);
        let _ = fs::remove_dir_all(&racine);
        (issue, histoire, id)
    }

    #[test]
    fn un_profil_a_api_sans_prix_pour_son_modele_ne_part_pas_sous_quota() {
        let sans_prix = "sorte = claude\nprogramme = kraken-claude-introuvable\n\
                     api = https://api.exemple.test/anthropic\ncle = CLE_ESSAI\nprix = autre-modele 1 2\n";
        let (issue, histoire, id) = lancer_a_api("quota-sans-prix", sans_prix, Some("100\n"));
        let texte = refusee(&id, &issue, 6030);
        assert!(texte.contains("sonnet") && texte.contains("api.exemple.test") && texte.contains("prix"), "{texte}");
        assert_eq!(histoire, ["vague 1", "refus 6030"]);
        // Sans fichier quota, rien ne change : la main part, et ne tient pas faute de programme.
        let (issue, histoire, _) = lancer_a_api("quota-sans-prix-libre", sans_prix, None);
        assert!(issue.unwrap_err().starts_with("lancement impossible de kraken-claude-introuvable"));
        assert_eq!(&histoire[..2], ["vague 1", "bail 1"]);
        // Avec un prix pour le modèle (`sonnet` par défaut), sous un quota que rien n'atteint : la main part.
        let avec_prix = sans_prix.replace("autre-modele 1 2", "sonnet 3 15");
        let (issue, histoire, _) = lancer_a_api("quota-avec-prix", &avec_prix, Some("100\n"));
        assert!(issue.unwrap_err().starts_with("lancement impossible de kraken-claude-introuvable"));
        assert_eq!(&histoire[..2], ["vague 1", "bail 1"]);
    }

    #[test]
    fn la_relance_d_une_vague_dont_la_depense_atteint_le_quota_ne_part_pas() {
        // La première tentative écrit au fil sa dépense estimée, 2 $, puis finit rouge : la relance ne part pas
        // sous un quota de 1 $. Sa dépense vient d'un `main.usage` que la main écrit elle-même : les mains
        // brutes n'en rendent pas.
        let racine = racine("quota-relance");
        let (depot, kraken) = (depot(&racine), racine.join("kraken"));
        poser_le_quota(&kraken, "1\n");
        // La ligne d'usage, à l'instant d'aujourd'hui, que la main recopie au bout du fil.
        let instant = instant(maintenant());
        let ligne = format!(
            concat!(
                r#"{{"v":2,"seance":"main","seq":1,"instant":"{}","origine":"humain","origine_foi":"declare","#,
                r#""type":"main.usage","vague":"v","main":1,"tentative":1,"modele":"m","entree":1,"sortie":1,"#,
                r#""cache_ecrit":0,"cache_lu":0,"reflexion":0,"micro_dollars":2000000,"base":"estime","#,
                r#""foi":"declare"}}"#,
            ),
            instant
        );
        let note = racine.join("usage.jsonl");
        fs::write(&note, format!("{ligne}\n")).unwrap();
        let fil_jsonl = kraken.join("fil.jsonl");
        let (nom, corps) = if cfg!(windows) {
            let corps = format!(
                "@echo off\r\nfindstr \"^\" > nul\r\ntype \"{}\" >> \"{}\"\r\necho PREUVE: rouge\r\n",
                note.display(),
                fil_jsonl.display()
            );
            ("depense.bat", corps)
        } else {
            let corps = format!(
                "cat > /dev/null\ncat '{}' >> '{}'\necho 'PREUVE: rouge'\n",
                pour_sh(&note),
                pour_sh(&fil_jsonl)
            );
            ("depense.sh", corps)
        };
        let main = racine.join(nom);
        fs::write(&main, corps).unwrap();
        let argv = if cfg!(windows) {
            vec![main.to_string_lossy().into_owned()]
        } else {
            vec!["sh".into(), main.to_string_lossy().into_owned()]
        };
        let profil = Profil::faux(argv, DELAI);
        let plan = Plan::copies("objectif", None, 1);
        let (id, issue) = lancer(&depot, &kraken, &profil, &plan, BAIL).unwrap().1.remove(0);
        refusee(&id, &issue, 6030);
        let evenements = fil(&kraken);
        let attendu = ["vague 1", "bail 1", "début 1", "usage 1 m Estime", "fin 1 Rouge Constate", "refus 6030"];
        assert_eq!(histoire(&evenements), attendu, "la tentative 2 n'a ni bail ni début");
        let _ = fs::remove_dir_all(&racine);
    }
}
