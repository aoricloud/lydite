use super::*;
use crate::branchements::bac::{bac, sans_claude};
use crate::coeur::empreinte::{hex, sha256};
use crate::essais::{depot, ecrire, etat_de_claude, git, pour_sh, racine, script};
use std::fs;
use std::process::Command;
use std::time::Duration;

const DELAI: Duration = Duration::from_secs(30);

/// La preuve de la vague : verte tant qu'aucun fichier `rouge` n'est là, rouge et bavarde ensuite. Elle ne lance
/// pas git.
const PREUVE: &str = if cfg!(windows) {
    "if exist rouge (echo rouge vue& exit 1) else (echo vert)"
} else {
    "if [ -f rouge ]; then echo rouge vue; exit 1; fi; echo vert"
};

/// Une racine neuve à ce test : elle, le dépôt visé, le dossier de Kraken (que le premier geste crée) et la base.
fn vise(nom: &str) -> (PathBuf, PathBuf, PathBuf, String) {
    let racine = racine(nom);
    let (depot, kraken) = (depot(&racine), racine.join("lydite"));
    let base = git(&depot, &["rev-parse", "HEAD"]);
    (racine, depot, kraken, base)
}

/// Comme [`vise`], mais la base suit `src/a.txt`, `src/b.txt` et `doc/c.txt`, et ignore `target/`.
fn vise_avec_fichiers(nom: &str) -> (PathBuf, PathBuf, PathBuf, String) {
    let (racine, depot, kraken, _) = vise(nom);
    for (chemin, contenu) in
        [("src/a.txt", "a\n"), ("src/b.txt", "b\n"), ("doc/c.txt", "c\n"), (".gitignore", "target/\n")]
    {
        ecrire(&depot, chemin, contenu);
    }
    git(&depot, &["add", "-A"]);
    git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "fichiers"]);
    let base = git(&depot, &["rev-parse", "HEAD"]);
    (racine, depot, kraken, base)
}

/// Comme [`vise`], mais la base suit un `.claude/settings.json`, que la main peut réécrire après `sans_claude`.
fn vise_avec_claude_suivi(nom: &str) -> (PathBuf, PathBuf, PathBuf, String) {
    let (racine, depot, kraken, _) = vise(nom);
    ecrire(&depot, ".claude/settings.json", "{}");
    git(&depot, &["add", "-A"]);
    git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "réglage suivi"]);
    let base = git(&depot, &["rev-parse", "HEAD"]);
    (racine, depot, kraken, base)
}

/// `chemins_changes` d'un profil sans chemin protégé : les tests qui ne parlent pas des butées d'un chemin protégé
/// n'ont rien à lui passer. Ce nom masque celui de `super`, que les tests de butées appellent par `super::`.
fn chemins_changes(bac: &Path, depart: &str) -> Result<Vec<String>, Refus> {
    super::chemins_changes(bac, depart, &[])
}

/// `fichiers_tenus` d'un profil sans chemin protégé, comme [`chemins_changes`].
fn fichiers_tenus(bac: &Path, depart: &str, id: &str, fichiers: &[String]) -> Result<(), Refus> {
    super::fichiers_tenus(bac, depart, id, fichiers, &[])
}

/// Ces chemins, en liste de textes.
fn chemins(chemins: &[&str]) -> Vec<String> {
    chemins.iter().map(|chemin| chemin.to_string()).collect()
}

fn profil() -> Profil {
    Profil::faux(vec!["lydite-main-inutile".into()], DELAI)
}

/// Le bac de la tâche `id`, posé par le test sur `depart`, où la main a écrit `fichiers` (chemin, contenu).
fn prise(depot: &Path, kraken: &Path, id: &str, depart: &str, fichiers: &[(&str, &str)]) -> PathBuf {
    let bac = bac(depot, kraken, id, depart).unwrap();
    for (chemin, contenu) in fichiers {
        ecrire(&bac, chemin, contenu);
    }
    bac
}

/// Les fichiers d'un commit, triés.
fn fichiers(depot: &Path, commit: &str) -> Vec<String> {
    git(depot, &["ls-tree", "-r", "--name-only", commit]).lines().map(str::to_string).collect()
}

/// Les noms de ce que contient un dossier, triés.
fn noms(dir: &Path) -> Vec<String> {
    let mut noms: Vec<String> =
        fs::read_dir(dir).unwrap().map(|entree| entree.unwrap().file_name().to_string_lossy().into_owned()).collect();
    noms.sort();
    noms
}

/// Les journaux de la preuve, `verif-*.tmp`, qui restent sous `dir`, à toute profondeur.
fn journaux(dir: &Path) -> Vec<PathBuf> {
    let mut restes = Vec::new();
    for entree in fs::read_dir(dir).unwrap() {
        let chemin = entree.unwrap().path();
        let nom = chemin.file_name().unwrap().to_string_lossy().into_owned();
        if chemin.is_dir() {
            restes.extend(journaux(&chemin));
        } else if nom.starts_with("verif-") && nom.ends_with(".tmp") {
            restes.push(chemin);
        }
    }
    restes
}

/// Le worktree de la vague `v` : à sa tête, détaché et propre ?
fn a_sa_tete(kraken: &Path, tete: &str) {
    let worktree = kraken.join("bacs").join("vague-v");
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), tete);
    assert_eq!(git(&worktree, &["branch", "--show-current"]), "", "détaché");
    assert_eq!(git(&worktree, &["status", "--porcelain", "--untracked-files=all"]), "", "propre");
    let fusion =
        Command::new("git").current_dir(&worktree).args(["rev-parse", "-q", "--verify", "MERGE_HEAD"]).output();
    assert!(!fusion.unwrap().status.success(), "aucune fusion en cours");
}

#[test]
fn retirer_la_vague_efface_son_temporaire() {
    let (racine, depot, kraken, base) = vise("retrait-temporaire");
    let profil = profil();
    let assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let temporaire = temporaire_de(&kraken.join("bacs").join("vague-v")).unwrap();
    ecrire(&temporaire, "reste.txt", "r");
    assemblage.retirer().unwrap();
    assert!(!kraken.join("bacs").join("vague-v").exists());
    assert!(!temporaire.exists());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn eprouver_la_base_efface_son_temporaire() {
    let (racine, depot, kraken, base) = vise("base-temporaire");
    // Sous un mur exigé, par un faux srt qui le tient, le rejeu de la preuve crée le dossier temporaire du worktree.
    let mut exige = crate::coeur::profil::lire("essai", "sorte = claude\nmur = exige\n", None, None).unwrap();
    exige.borner(&kraken, &depot.join(".git"), None, None, None).unwrap();
    exige.avec_srt(crate::essais::faux_srt(&racine, true));
    fs::create_dir_all(&kraken).unwrap();
    eprouver_la_base(&depot, &kraken, &exige, &base, "exit 0").unwrap();
    assert!(racine.join("srt").join("forme.txt").is_file(), "la preuve est passée par srt");
    assert_eq!(noms(&kraken.join("bacs")), Vec::<String>::new());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn deux_taches_disjointes_s_assemblent() {
    let (racine, depot, kraken, base) = vise("disjointes");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    assert_eq!((assemblage.branche(), assemblage.tete()), ("lydite/vague-v", base.as_str()));
    assert_eq!(git(&depot, &["rev-parse", "lydite/vague-v"]), base);
    a_sa_tete(&kraken, &base);
    let mut notes = Vec::new();
    for (k, id, fichier) in [(1, "a", "a.txt"), (2, "b", "b.txt")] {
        let bac = prise(&depot, &kraken, id, &base, &[(fichier, id)]);
        let (verif, issue) = assemblage.fusionner(&bac, &base, id, None, k, |sha, verif| {
            notes.push((sha.to_string(), verif.trim().to_string()));
            Ok(())
        });
        let sha = issue.unwrap();
        assert_eq!(verif.as_deref().map(str::trim), Some("vert"));
        // La branche avance sur la fusion notée, dont le gel est le second parent ; le worktree la suit.
        assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (sha.as_str(), sha.clone()));
        assert_eq!(notes.last(), Some(&(sha.clone(), "vert".to_string())));
        for commit in [sha.clone(), format!("{sha}^2")] {
            assert_eq!(git(&depot, &["log", "-1", "--format=%s", &commit]), id);
        }
        a_sa_tete(&kraken, &sha);
    }
    assert_eq!(notes.len(), 2);
    assert_eq!(fichiers(&depot, "lydite/vague-v"), ["a.txt", "b.txt"]);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_gel_et_la_fusion_ne_portent_que_l_id() {
    let (racine, depot, kraken, base) = vise("id-seul");
    // Le worktree de la vague lit la configuration du dépôt : merge.log y ajouterait la liste des commits fusionnés.
    git(&depot, &["config", "merge.log", "true"]);
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, "exit 0").unwrap();
    // L'objectif n'entre plus dans l'assemblage : le texte du découpeur ne vit que dans le travail du bac.
    let bac = prise(&depot, &kraken, "t", &base, &[("TEXTE-DU-DECOUPEUR.txt", "TEXTE-DU-DECOUPEUR")]);
    let sha = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| Ok(())).1.unwrap();
    for commit in [sha.clone(), format!("{sha}^2")] {
        let message = git(&depot, &["log", "-1", "--format=%B", &commit]);
        assert_eq!(message.trim(), "t", "{commit}");
        assert_eq!(git(&depot, &["log", "-1", "--format=%s", &commit]), "t", "{commit}");
    }
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_conflit_laisse_la_vague_a_sa_tete() {
    let (racine, depot, kraken, base) = vise("conflit");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let premiere = prise(&depot, &kraken, "a", &base, &[("commun.txt", "a\n"), ("a.txt", "a")]);
    let tete = assemblage.fusionner(&premiere, &base, "a", None, 1, |_, _| Ok(())).1.unwrap();
    // La seconde écrit le même fichier autrement : elle n'entre pas, et la vague reste à sa tête.
    let seconde = prise(&depot, &kraken, "b", &base, &[("commun.txt", "b\n"), ("b.txt", "b")]);
    let mut notee = false;
    let (verif, issue) = assemblage.fusionner(&seconde, &base, "b", None, 2, |_, _| {
        notee = true;
        Ok(())
    });
    let refus = issue.unwrap_err();
    assert_eq!(refus.code, Code::Conflit);
    assert_eq!(refus.preuve, format!("the merge of b on head {tete} is in conflict: commun.txt"));
    assert!(!refus.correction.trim().is_empty());
    assert_eq!((verif, notee), (None, false), "ni preuve rejouée, ni note");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (tete.as_str(), tete.clone()));
    a_sa_tete(&kraken, &tete);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_preuve_rouge_ne_fait_pas_avancer_la_branche() {
    let (racine, depot, kraken, base) = vise("rouge");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let bac = prise(&depot, &kraken, "r", &base, &[("rouge", "r")]);
    let (verif, issue) =
        assemblage.fusionner(&bac, &base, "r", None, 1, |_, _| panic!("une preuve rouge ne se note pas"));
    let refus = issue.unwrap_err();
    assert_eq!(refus.code, Code::PreuveRouge);
    assert!(refus.preuve.starts_with("the wave's proof, replayed after the merge of r: "), "{refus}");
    assert_eq!(verif.as_deref().map(str::trim), Some("rouge vue"), "le verif de la preuve rouge");
    assert!(!refus.correction.trim().is_empty());
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    let _ = fs::remove_dir_all(&racine);
}

/// Rend le dépôt visé hostile, par sa configuration locale : ses crochets `pre-commit` (qui sort en 1),
/// `post-checkout` et `reference-transaction` ajoutent leur nom au témoin des crochets ; `core.fsmonitor` nomme un
/// programme qui écrit le sien ; `commit.gpgsign` vaut true, avec un `gpg.program` absent ; `user.name` et
/// `user.email` nomment un autre que Kraken. Rend les deux témoins.
fn hostile(racine: &Path, depot: &Path) -> (PathBuf, PathBuf) {
    let (crochets, temoin) = (depot.join(".git").join("hooks"), racine.join("crochets.txt"));
    let (programme, temoin_du_moniteur) = (racine.join("fsmonitor.sh"), racine.join("fsmonitor.txt"));
    for (nom, fin) in [("pre-commit", "\nexit 1"), ("post-checkout", ""), ("reference-transaction", "")] {
        script(&crochets.join(nom), &format!("echo {nom} >> '{}'{fin}", pour_sh(&temoin)));
    }
    script(&programme, &format!("echo fsmonitor >> '{}'", pour_sh(&temoin_du_moniteur)));
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
    (temoin, temoin_du_moniteur)
}

#[test]
fn le_gel_et_la_fusion_portent_l_identite_de_kraken() {
    let (racine, depot, kraken, base) = vise("identite");
    hostile(&racine, &depot);
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, "exit 0").unwrap();
    let bac = prise(&depot, &kraken, "t", &base, &[("travail.txt", "fait")]);
    let sha = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| Ok(())).1.unwrap();
    // La fusion et le gel, son second parent : auteur et committeur Kraken, sans signature, malgré user.name,
    // user.email et commit.gpgsign du dépôt visé.
    assert_eq!(git(&depot, &["rev-parse", &format!("{sha}^1")]), base);
    for commit in [sha.clone(), format!("{sha}^2")] {
        let qui = git(&depot, &["log", "-1", "--format=%an <%ae>|%cn <%ce>|%s", &commit]);
        assert_eq!(qui, "lydite <lydite@invalid>|lydite <lydite@invalid>|t", "{commit}");
        let brut = git(&depot, &["cat-file", "commit", &commit]);
        assert!(!brut.contains("gpgsig"), "{commit} n'est pas signé : {brut}");
    }
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn aucun_geste_de_la_vague_ne_joue_un_crochet_du_depot() {
    let (racine, depot, kraken, base) = vise("crochets");
    let (crochets, moniteur) = hostile(&racine, &depot);
    // Les témoins marchent : pre-commit fait échouer un commit ordinaire, un worktree ordinaire joue post-checkout
    // et reference-transaction, un git status ordinaire lance le programme de core.fsmonitor.
    let ordinaire = Command::new("git").current_dir(&depot).args(["commit", "-q", "--allow-empty", "-m", "o"]).output();
    assert!(!ordinaire.unwrap().status.success(), "pre-commit refuse le commit ordinaire");
    let bac = racine.join("bac");
    git(&depot, &["worktree", "add", "-q", "-b", "temoin", &bac.to_string_lossy(), &base]);
    git(&depot, &["status"]);
    let joues = fs::read_to_string(&crochets).unwrap();
    for nom in ["pre-commit", "post-checkout", "reference-transaction"] {
        assert!(joues.contains(nom), "{nom} joue d'ordinaire : {joues:?}");
    }
    assert_ne!(fs::read_to_string(&moniteur).unwrap(), "", "core.fsmonitor joue d'ordinaire");
    ecrire(&bac, "travail.txt", "fait");
    // La fenêtre, de la remise à vide des témoins à leur lecture : seule la vague y joue git. Sa preuve ne le lance
    // pas.
    fs::write(&crochets, "").unwrap();
    fs::write(&moniteur, "").unwrap();
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, "exit 0").unwrap();
    let (_, issue) = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| Ok(()));
    let temoins = (fs::read_to_string(&crochets).unwrap(), fs::read_to_string(&moniteur).unwrap());
    assert_eq!(temoins, (String::new(), String::new()), "ni ouvrir, ni le gel, ni la fusion, ni update-ref");
    let sha = issue.unwrap();
    assert_eq!(git(&depot, &["rev-parse", "lydite/vague-v"]), sha);
    assert_eq!(fichiers(&depot, &sha), ["travail.txt"]);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_bac_sans_changement_s_assemble_sans_commit() {
    let (racine, depot, kraken, base) = vise("sans-changement");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let bac = prise(&depot, &kraken, "t", &base, &[]);
    let mut notee = None;
    let (_, issue) = assemblage.fusionner(&bac, &base, "t", None, 1, |sha, _| {
        notee = Some(sha.to_string());
        Ok(())
    });
    // Assemblée et notée sur la tête, qui ne bouge pas : ni gel, ni fusion.
    assert_eq!(issue.unwrap(), base);
    assert_eq!(notee, Some(base.clone()));
    assert_eq!(git(&bac, &["rev-parse", "HEAD"]), base, "aucun commit dans le bac");
    assert_eq!(git(&depot, &["rev-parse", "lydite/vague-v"]), base, "ni dans la branche");
    a_sa_tete(&kraken, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_bac_qui_a_change_de_branche_s_assemble_avec_son_travail() {
    let (racine, depot, kraken, base) = vise("autre-branche");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let bac = prise(&depot, &kraken, "t", &base, &[("commite.txt", "1")]);
    // La main quitte sa branche, commite sur une autre, et laisse un fichier de plus.
    git(&bac, &["switch", "-q", "-c", "ailleurs"]);
    git(&bac, &["add", "commite.txt"]);
    git(&bac, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "commit de la main"]);
    ecrire(&bac, "laisse.txt", "2");
    let sha = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| Ok(())).1.unwrap();
    assert_eq!(fichiers(&depot, &sha), ["commite.txt", "laisse.txt"]);
    // La fusion a pris le HEAD du bac, gelé sur `ailleurs` ; la branche du bac n'a pas bougé.
    assert_eq!(git(&depot, &["rev-parse", &format!("{sha}^2")]), git(&depot, &["rev-parse", "ailleurs"]));
    assert_eq!(git(&depot, &["rev-parse", "lydite/t"]), base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_bac_qui_touche_claude_n_est_pas_fige() {
    let (racine, depot, kraken, base) = vise("claude");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let bac = prise(&depot, &kraken, "t", &base, &[("travail.txt", "fait"), (".claude/settings.json", "{}")]);
    let (verif, issue) = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| panic!("rien ne se note"));
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::ClaudeTouche, None));
    assert!(refus.preuve.ends_with(": .claude/settings.json"), "{refus}");
    // Ni figé (aucun commit, rien d'ajouté), ni assemblé : la branche à sa tête.
    assert_eq!(git(&bac, &["rev-parse", "HEAD"]), base);
    assert_eq!(git(&bac, &["diff", "--cached", "--name-only"]), "");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_claude_commite_puis_ote_n_est_ni_fige_ni_fusionne() {
    let (racine, depot, kraken, base) = vise("claude-commite");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let bac = prise(&depot, &kraken, "t", &base, &[("travail.txt", "fait"), (".claude/settings.json", "{}")]);
    // La main commite le travail et le réglage, puis ôte le réglage par un simple effacement que le gel commiterait :
    // l'arbre du bac n'a plus de .claude, mais la fusion ferait entrer dans la branche le commit qui le porte.
    git(&bac, &["add", "-A"]);
    git(&bac, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "commit de la main"]);
    fs::remove_file(bac.join(".claude/settings.json")).unwrap();
    let (verif, issue) = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| panic!("rien ne se note"));
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::ClaudeTouche, None));
    assert!(refus.preuve.ends_with(": .claude/settings.json"), "{refus}");
    // Ni figé (le bac n'a que le commit de la main, et l'effacement n'est pas ajouté), ni assemblé : la branche à sa
    // tête.
    assert_eq!(git(&bac, &["rev-list", "--count", &format!("{base}..HEAD")]), "1");
    assert_eq!(git(&bac, &["diff", "--cached", "--name-only"]), "");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_claude_ecrit_apres_sans_claude_est_le_refus_7006_au_gel() {
    let (racine, depot, kraken, base) = vise_avec_claude_suivi("claude-apres-sans-claude");
    let profil = profil();
    let assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    // La main a rendu sa tentative, `sans_claude` a passé, puis un processus qu'elle a laissé derrière elle réécrit le
    // réglage suivi de la base : le gel le prendrait. Avec des `fichiers`, `chemins_geles` écarte tout `.claude` et le
    // laisserait passer ; sans, il ne tourne même pas.
    let avec_fichiers = prise(&depot, &kraken, "a", &base, &[("src/a.txt", "a\n")]);
    assert_eq!(sans_claude(&avec_fichiers, &base), Ok(()), "sans_claude passe avant l'écriture");
    ecrire(&avec_fichiers, ".claude/settings.json", r#"{"hooks":{}}"#);
    let permis = chemins(&["src/"]);
    let (verif, issue) = assemblage.assembler(
        &avec_fichiers,
        &base,
        "a",
        Some(&permis),
        1,
        &[],
        |_| Ok(()),
        |_, _| Ok(()),
        |_, _| panic!("un .claude au gel ne se note pas"),
    );
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::ClaudeTouche, None));
    assert!(refus.preuve.ends_with(r#": ".claude/settings.json""#), "{refus}");
    assert!(refus.correction.contains("the merge did not happen"), "{refus}");
    // Le gel a eu lieu et porte le chemin ; ni fusion, ni note : la branche et le worktree restent à leur tête.
    assert_eq!(git(&avec_fichiers, &["rev-list", "--count", &format!("{base}..HEAD")]), "1");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    // Sans `fichiers`, le gel est jugé de même : une vague sans plan reste fermée à ce `.claude`.
    let sans_fichiers = prise(&depot, &kraken, "b", &base, &[("b.txt", "b")]);
    assert_eq!(sans_claude(&sans_fichiers, &base), Ok(()), "sans_claude passe avant l'écriture");
    ecrire(&sans_fichiers, ".claude/settings.json", r#"{"hooks":{}}"#);
    let (verif, issue) = assemblage.assembler(
        &sans_fichiers,
        &base,
        "b",
        None,
        2,
        &[],
        |_| Ok(()),
        |_, _| Ok(()),
        |_, _| panic!("un .claude au gel ne se note pas"),
    );
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::ClaudeTouche, None));
    assert!(refus.preuve.ends_with(r#": ".claude/settings.json""#), "{refus}");
    assert_eq!(git(&sans_fichiers, &["rev-list", "--count", &format!("{base}..HEAD")]), "1");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_claude_commite_puis_ote_apres_sans_claude_est_le_refus_7006_au_gel() {
    let (racine, depot, kraken, base) = vise("claude-commite-puis-ote");
    let profil = profil();
    let assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    // Après `sans_claude`, un processus resté derrière la main crée un `.claude`, le commite, puis l'ôte : l'arbre du
    // gel vaudra celui de la base, mais son histoire porte le commit, et la fusion l'apporterait à la branche.
    let avec_fichiers = prise(&depot, &kraken, "a", &base, &[("src/a.txt", "a\n")]);
    assert_eq!(sans_claude(&avec_fichiers, &base), Ok(()), "sans_claude passe avant le commit");
    ecrire(&avec_fichiers, ".claude/settings.json", r#"{"hooks":{}}"#);
    commiter(&avec_fichiers);
    fs::remove_file(avec_fichiers.join(".claude/settings.json")).unwrap();
    let permis = chemins(&["src/"]);
    let (verif, issue) = assemblage.assembler(
        &avec_fichiers,
        &base,
        "a",
        Some(&permis),
        1,
        &[],
        |_| Ok(()),
        |_, _| Ok(()),
        |_, _| panic!("un .claude au gel ne se note pas"),
    );
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::ClaudeTouche, None));
    assert!(refus.preuve.ends_with(r#": ".claude/settings.json""#), "{refus}");
    // Le gel a eu lieu : son arbre ne dit plus rien de `.claude`, mais son histoire le porte.
    assert_eq!(git(&avec_fichiers, &["rev-list", "--count", &format!("{base}..HEAD")]), "2");
    assert_eq!(git(&avec_fichiers, &["diff", "--name-only", &base, "HEAD", "--", ".claude"]), "");
    assert!(
        git(&avec_fichiers, &["log", "--format=", "--name-only", &format!("{base}..HEAD"), "--", ".claude"])
            .contains(".claude/settings.json")
    );
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    // Sans `fichiers`, le même commit puis le même retrait : l'histoire est jugée de même.
    let sans_fichiers = prise(&depot, &kraken, "b", &base, &[("b.txt", "b")]);
    assert_eq!(sans_claude(&sans_fichiers, &base), Ok(()), "sans_claude passe avant le commit");
    ecrire(&sans_fichiers, ".claude/settings.json", r#"{"hooks":{}}"#);
    commiter(&sans_fichiers);
    fs::remove_file(sans_fichiers.join(".claude/settings.json")).unwrap();
    let (verif, issue) = assemblage.assembler(
        &sans_fichiers,
        &base,
        "b",
        None,
        2,
        &[],
        |_| Ok(()),
        |_, _| Ok(()),
        |_, _| panic!("un .claude au gel ne se note pas"),
    );
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::ClaudeTouche, None));
    assert!(refus.preuve.ends_with(r#": ".claude/settings.json""#), "{refus}");
    assert_eq!(git(&sans_fichiers, &["diff", "--name-only", &base, "HEAD", "--", ".claude"]), "");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_gel_laisse_l_etat_de_claude_code_hors_de_la_branche() {
    let (racine, depot, kraken, base) = vise("etat");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let bac = prise(&depot, &kraken, "t", &base, &[("travail.txt", "fait"), ("sous/travail.txt", "fait")]);
    etat_de_claude(&bac);
    let sha = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| Ok(())).1.unwrap();
    // La tâche est assemblée, sans rien de l'état de Claude Code : ni ses fichiers, ni ses sous-agents en dépôts
    // imbriqués. Cet état reste dans le bac, hors de git.
    assert_eq!(git(&depot, &["rev-parse", "lydite/vague-v"]), sha);
    assert_eq!(fichiers(&depot, &sha), ["sous/travail.txt", "travail.txt"]);
    let arbre = git(&depot, &["ls-tree", "-r", &sha]);
    assert!(!arbre.lines().any(|entree| entree.starts_with("160000")), "aucun dépôt imbriqué : {arbre}");
    assert!(bac.join(".claude/worktrees/x/f").is_file() && bac.join("sous/.claude/worktrees/y/f").is_file());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_fusion_non_notee_est_defaite() {
    let (racine, depot, kraken, base) = vise("non-notee");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let bac = prise(&depot, &kraken, "t", &base, &[("travail.txt", "fait")]);
    let fil = "note au fil /lydite/fil.jsonl : disque plein";
    let (verif, issue) = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| Err(io::Error::other(fil)));
    let refus = issue.unwrap_err();
    assert_eq!(refus.code, Code::FilInjoignable);
    assert!(refus.preuve.starts_with("the merge of t (") && refus.preuve.ends_with(fil), "le fil : {refus}");
    assert!(!refus.correction.trim().is_empty());
    assert_eq!(verif.as_deref().map(str::trim), Some("vert"), "la preuve a tourné");
    // Défaite : la branche et le worktree de la vague à la base, sans le travail ; l'assemblage reste ouvert.
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    assert_eq!(assemblage.fermeture, None);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_retour_rate_ferme_l_assemblage() {
    let (racine, depot, kraken, base) = vise("retour");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let worktree = kraken.join("bacs").join("vague-v");
    let verrou = PathBuf::from(git(&worktree, &["rev-parse", "--absolute-git-dir"])).join("index.lock");
    // La note rate après avoir posé index.lock dans le git-dir du worktree de la vague : le retour qui défait la
    // fusion ne peut pas se faire.
    let bac = prise(&depot, &kraken, "a", &base, &[("a.txt", "a")]);
    let (_, issue) = assemblage.fusionner(&bac, &base, "a", None, 1, |_, _| {
        fs::write(&verrou, "").unwrap();
        Err(io::Error::other("note ratée"))
    });
    let refus = issue.unwrap_err();
    assert_eq!(refus.code, Code::AssemblageFerme);
    assert!(refus.preuve.starts_with(&format!("the wave's worktree does not return to its head {base}")), "{refus}");
    assert!(refus.preuve.contains("7012 vague.fil_injoignable") && refus.preuve.contains("git reset"), "{refus}");
    assert!(refus.preuve.contains(": 128)"), "reset --hard sort en 128 : {refus}");
    assert!(!refus.correction.trim().is_empty());
    // Fermé : la prise suivante est refusée d'emblée, et la branche est restée à sa tête.
    let suivante = prise(&depot, &kraken, "b", &base, &[("b.txt", "b")]);
    let (verif, issue) = assemblage.fusionner(&suivante, &base, "b", None, 2, |_, _| panic!("rien ne se note"));
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::AssemblageFerme, None));
    assert!(refus.preuve.starts_with("the assembly of wave v is closed, and b does not enter it: "), "{refus}");
    assert_eq!(git(&depot, &["rev-parse", "lydite/vague-v"]), base);
    assert_eq!(git(&suivante, &["rev-parse", "HEAD"]), base, "la suivante n'est pas figée");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_base_rouge_n_ouvre_rien() {
    let (racine, depot, kraken, base) = vise("base-rouge");
    let rouge = if cfg!(windows) { "echo base rouge& exit 1" } else { "echo base rouge; exit 1" };
    let Err(refus) = ouvrir(&depot, &kraken, &profil(), "v", &base, rouge) else { panic!("une base rouge s'ouvre") };
    assert_eq!(refus.code, Code::BaseRouge);
    assert!(refus.preuve.starts_with(&format!("the wave's proof, played on the base {base}: ")), "{refus}");
    assert!(refus.preuve.contains("base rouge"), "la sortie de la preuve : {refus}");
    assert!(!refus.correction.trim().is_empty());
    // Ni worktree, ni branche, ni journal.
    assert!(!kraken.join("bacs").join("vague-v").exists());
    assert_eq!(git(&depot, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1);
    assert_eq!(git(&depot, &["branch", "--list", "lydite/*"]), "");
    assert_eq!(journaux(&racine), Vec::<PathBuf>::new());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_geste_rate_avant_la_preuve_est_6006_et_ne_retire_que_le_sien() {
    let (racine, depot, kraken, base) = vise("6006");
    let profil = profil();
    let ouverte = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    // Sa branche existe : la même vague ne se rouvre pas, et rien de la première ne se retire.
    let Err(refus) = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE) else { panic!("la branche existe") };
    assert_eq!(refus.code, Code::EntreeSortie);
    assert!(refus.preuve.starts_with("git branch lydite/vague-v "), "la commande : {refus}");
    assert!(refus.correction.contains("lydite/vague-v"), "{refus}");
    assert_eq!(git(&depot, &["rev-parse", ouverte.branche()]), base);
    a_sa_tete(&kraken, &base);
    // Son dossier est pris : le worktree ne se pose pas, et la branche que ce geste a posée se retire.
    ecrire(&kraken, "bacs/vague-w/pris.txt", "pris");
    let Err(refus) = ouvrir(&depot, &kraken, &profil, "w", &base, PREUVE) else { panic!("le dossier est pris") };
    assert_eq!(refus.code, Code::EntreeSortie);
    assert!(refus.preuve.starts_with("git worktree add -q --detach "), "la commande : {refus}");
    assert_eq!(git(&depot, &["branch", "--list", "lydite/vague-w"]), "");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_depot_vise_ne_voit_que_des_branches() {
    let (racine, depot, kraken, base) = vise("branches");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let verte = prise(&depot, &kraken, "a", &base, &[("a.txt", "a")]);
    assemblage.fusionner(&verte, &base, "a", None, 1, |_, _| Ok(())).1.unwrap();
    let rouge = prise(&depot, &kraken, "r", &base, &[("rouge", "r")]);
    let refus = assemblage.fusionner(&rouge, &base, "r", None, 2, |_, _| Ok(())).1.unwrap_err();
    assert_eq!(refus.code, Code::PreuveRouge);
    // Rien dans le dossier du dépôt visé hors de .git, rien de suivi ni de non suivi ; des branches à Kraken.
    assert_eq!(git(&depot, &["status", "--porcelain", "--untracked-files=all"]), "");
    assert_eq!(noms(&depot), [".git"]);
    let branches = git(&depot, &["for-each-ref", "--format=%(refname)", "refs/heads/lydite"]);
    assert_eq!(
        branches.lines().collect::<Vec<_>>(),
        ["refs/heads/lydite/a", "refs/heads/lydite/r", "refs/heads/lydite/vague-v"]
    );
    // Aucun journal de la preuve ne reste, ni dans le dossier de Kraken, ni ailleurs.
    assert_eq!(noms(&kraken), ["bacs"]);
    assert_eq!(journaux(&racine), Vec::<PathBuf>::new());
    let _ = fs::remove_dir_all(&racine);
}

/// Commite dans le bac tout ce que la main y a changé.
fn commiter(bac: &Path) {
    git(bac, &["add", "-A"]);
    git(bac, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "commit de la main"]);
}

#[test]
fn les_chemins_changes_disent_le_suivi_le_non_suivi_les_renommes_et_les_commits_mais_pas_l_ignore() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("changes");
    // Un bac que la main n'a pas touché n'a rien de changé.
    let propre = prise(&depot, &kraken, "propre", &base, &[]);
    assert_eq!(chemins_changes(&propre, &base), Ok(Vec::new()));
    // Un modifié, un effacé, un renommé par git (sous ses deux noms), un ajouté à l'index, deux non suivis.
    let bac =
        prise(&depot, &kraken, "t", &base, &[("src/b.txt", "autre\n"), ("ajoute.txt", "1"), ("sous/neuf.txt", "2")]);
    fs::remove_file(bac.join("doc/c.txt")).unwrap();
    git(&bac, &["mv", "src/a.txt", "src/renomme.txt"]);
    git(&bac, &["add", "ajoute.txt"]);
    // Ce que le gel ne prend pas ne compte pas : un fichier ignoré, et l'état de Claude Code ou tout dossier
    // `.claude`, à 7006.
    for ailleurs in ["target/x.o", ".claude/worktrees/x/f", "sous/.Claude/settings.json"] {
        ecrire(&bac, ailleurs, "ailleurs");
    }
    let attendu = ["ajoute.txt", "doc/c.txt", "sous/neuf.txt", "src/a.txt", "src/b.txt", "src/renomme.txt"];
    assert_eq!(chemins_changes(&bac, &base).unwrap(), attendu);
    // Un renommé sans git (un effacé et un non suivi) se dit de même sous ses deux noms.
    let deplace = prise(&depot, &kraken, "deplace", &base, &[]);
    fs::rename(deplace.join("src/b.txt"), deplace.join("src/deplace.txt")).unwrap();
    assert_eq!(chemins_changes(&deplace, &base).unwrap(), ["src/b.txt", "src/deplace.txt"]);
    // Ce que la main a commité compte, bien que `git status` n'en dise plus rien ; puis ce qu'elle change encore.
    let commite = prise(&depot, &kraken, "commite", &base, &[("src/b.txt", "autre\n"), ("commite.txt", "1")]);
    commiter(&commite);
    assert_eq!(git(&commite, &["status", "--porcelain"]), "");
    assert_eq!(chemins_changes(&commite, &base).unwrap(), ["commite.txt", "src/b.txt"]);
    ecrire(&commite, "src/a.txt", "encore\n");
    ecrire(&commite, "src/b.txt", "encore\n");
    assert_eq!(chemins_changes(&commite, &base).unwrap(), ["commite.txt", "src/a.txt", "src/b.txt"]);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_chemin_hors_des_fichiers_est_le_refus_7013() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("hors-fichiers");
    let bac = prise(&depot, &kraken, "t", &base, &[("src/b.txt", "autre\n"), ("src/neuf/x.txt", "1")]);
    // Ce que `fichiers` couvre passe : un chemin, ou un dossier fini par `/` et tout ce qu'il contient, et un chemin
    // de trop qui n'a pas changé n'y fait rien.
    for permis in [&["src/"][..], &["src/b.txt", "src/neuf/"], &["src/b.txt", "src/neuf/x.txt", "doc/c.txt"]] {
        assert_eq!(fichiers_tenus(&bac, &base, "t", &chemins(permis)), Ok(()), "{permis:?}");
    }
    // Ce qu'elle ne couvre pas est le refus 7013, qui cite les chemins de trop puis ceux de `fichiers`, et dit quoi
    // faire.
    let refus = fichiers_tenus(&bac, &base, "t", &chemins(&["src/b.txt"])).unwrap_err();
    assert_eq!(refus.code, Code::HorsFichiers);
    let preuve = r#"the hand of task t changed, outside its files ("src/b.txt"): "src/neuf/x.txt""#;
    assert_eq!(refus.preuve, preuve);
    assert!(refus.correction.contains("Put each of these paths back") && refus.correction.contains("`fichiers`"));
    // Un dossier sans `/` final n'est que ce nom, qui ne couvre rien dessous ; un dossier n'en couvre pas un autre
    // dont le nom commence de même ; des `fichiers` vides ne couvrent rien.
    for faux in [&["src"][..], &["src/neuf", "src/b.txt"], &["sr/", "src/n/"], &[]] {
        let refus = fichiers_tenus(&bac, &base, "t", &chemins(faux)).unwrap_err();
        assert_eq!(refus.code, Code::HorsFichiers, "{faux:?}");
    }
    // La comparaison est exacte, octet à octet : la casse compte, sous Windows comme ailleurs. Les `fichiers` d'une
    // autre casse que celle de git ne couvrent pas le travail ; un fichier d'une autre casse que les `fichiers`, sur
    // le disque, non plus.
    let les_deux = r#""src/b.txt", "src/neuf/x.txt""#;
    let autres_casses = [(&["SRC/"][..], les_deux), (&["src/B.txt", "src/neuf/"], r#""src/b.txt""#)];
    for (faux, hors) in autres_casses.into_iter().chain([(&["Src/b.txt", "src/Neuf/"][..], les_deux)]) {
        let refus = fichiers_tenus(&bac, &base, "t", &chemins(faux)).unwrap_err();
        assert_eq!(refus.code, Code::HorsFichiers, "{faux:?}");
        assert!(refus.preuve.ends_with(&format!("): {hors}")), "{faux:?} : {refus}");
    }
    let disque = prise(&depot, &kraken, "casse", &base, &[("Dedans.txt", "1")]);
    assert_eq!(fichiers_tenus(&disque, &base, "t", &chemins(&["Dedans.txt"])), Ok(()));
    let refus = fichiers_tenus(&disque, &base, "t", &chemins(&["dedans.txt"])).unwrap_err();
    assert_eq!(refus.preuve, r#"the hand of task t changed, outside its files ("dedans.txt"): "Dedans.txt""#);
    // Un dossier d'une autre casse est un autre dossier là où le disque distingue la casse. Sous Windows, il retombe
    // sur le dossier qui existe, et git rend `src/x.txt`, sous la casse qu'il connaît : `src/` le couvre.
    let dossier = prise(&depot, &kraken, "dossier", &base, &[("SRC/x.txt", "1")]);
    let issue = fichiers_tenus(&dossier, &base, "t", &chemins(&["src/"]));
    if cfg!(windows) {
        assert_eq!(issue, Ok(()));
    } else {
        let refus = issue.unwrap_err();
        assert!(refus.preuve.ends_with(r#": "SRC/x.txt""#), "{refus}");
    }
    // Un renommé, un effacé et un non suivi comptent ; un fichier ignoré, non.
    let travail = prise(&depot, &kraken, "travail", &base, &[("neuf.txt", "1"), ("target/x.o", "ignoré")]);
    fs::remove_file(travail.join("doc/c.txt")).unwrap();
    git(&travail, &["mv", "src/a.txt", "src/renomme.txt"]);
    let refus = fichiers_tenus(&travail, &base, "t", &chemins(&["src/"])).unwrap_err();
    assert!(refus.preuve.ends_with(r#": "doc/c.txt", "neuf.txt""#), "l'effacé et le non suivi : {refus}");
    let tout = chemins(&["src/", "doc/", "neuf.txt"]);
    assert_eq!(fichiers_tenus(&travail, &base, "t", &tout), Ok(()), "ni le renommé de src/, ni l'ignoré");
    let refus = fichiers_tenus(&travail, &base, "t", &chemins(&["doc/", "neuf.txt", "src/renomme.txt"])).unwrap_err();
    assert!(refus.preuve.ends_with(r#": "src/a.txt""#), "le renommé se dit sous son ancien nom : {refus}");
    let refus = fichiers_tenus(&travail, &base, "t", &chemins(&["doc/", "neuf.txt", "src/a.txt"])).unwrap_err();
    assert!(refus.preuve.ends_with(r#": "src/renomme.txt""#), "et sous son nouveau : {refus}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_chemin_ecrit_apres_la_fin_de_la_tentative_est_refuse_au_gel() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("gel-hors-fichiers");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let permis = chemins(&["src/"]);
    // Une première tâche dans ses fichiers passe, et la branche avance : c'est la tête que le refus suivant laisse.
    let couverte = prise(&depot, &kraken, "a", &base, &[("src/b.txt", "autre\n")]);
    let tete = assemblage.fusionner(&couverte, &base, "a", Some(&permis), 1, |_, _| Ok(())).1.unwrap();
    assert_eq!(git(&depot, &["diff", "--name-only", &base, &tete]), "src/b.txt");
    // La main a rendu sa tentative, puis un fichier hors de ses `fichiers` est apparu dans son bac, comme l'écrirait
    // un processus qu'elle a laissé derrière elle : le gel le prend, et le jugement du gel le refuse, sans relance.
    let bac = prise(&depot, &kraken, "t", &base, &[("src/a.txt", "encore\n"), ("hors.txt", "tard")]);
    let (verif, issue) = assemblage
        .fusionner(&bac, &base, "t", Some(&permis), 2, |_, _| panic!("un chemin hors des fichiers ne se note pas"));
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::HorsFichiers, None));
    assert!(refus.preuve.ends_with(r#": "hors.txt""#), "{refus}");
    assert!(refus.correction.contains("the merge did not happen"), "{refus}");
    assert!(refus.correction.contains("stay at their head"), "{refus}");
    // Le gel a eu lieu, et porte le chemin de trop.
    assert_eq!(git(&bac, &["rev-list", "--count", &format!("{base}..HEAD")]), "1");
    assert_eq!(
        git(&bac, &["show", "--name-only", "--format=", "HEAD"]).lines().collect::<Vec<_>>(),
        ["hors.txt", "src/a.txt"]
    );
    // Ni fusion, ni note : la branche et le worktree de la vague restent à leur tête.
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (tete.as_str(), tete.clone()));
    a_sa_tete(&kraken, &tete);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_refus_7013_cite_vingt_chemins_au_plus() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("beaucoup");
    let bac = prise(&depot, &kraken, "t", &base, &[]);
    for i in 0..25 {
        ecrire(&bac, &format!("lot/f{i:02}.txt"), "1");
    }
    let refus = fichiers_tenus(&bac, &base, "t", &chemins(&["src/"])).unwrap_err();
    let vingt: Vec<String> = (0..20).map(|i| format!("\"lot/f{i:02}.txt\"")).collect();
    assert_eq!(
        refus.preuve,
        format!("the hand of task t changed, outside its files (\"src/\"): {}, and 5 more", vingt.join(", "))
    );
    // Une liste de `fichiers` qui déborde se cite de même, et un chemin avec un caractère de contrôle s'écrit.
    let plus_de_vingt: Vec<String> = (0..21).map(|i| format!("d{i}/")).collect();
    assert!(citer(&plus_de_vingt).ends_with(r#""d19/", and 1 more"#));
    assert_eq!(citer(&chemins(&["a\nb", "\u{1b}[2J", "é.txt"])), r#""a\nb", "\u{1b}[2J", "é.txt""#);
    assert_eq!(citer(&[]), "");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn git_qui_ne_dit_pas_ce_que_la_main_a_change_est_le_refus_7009() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("liste-ratee");
    let bac = prise(&depot, &kraken, "t", &base, &[("src/b.txt", "autre\n")]);
    // Un bac absent, où git ne part pas, et un départ inconnu, que git refuse : jamais un bac propre, ni un chemin
    // couvert par défaut.
    let refus = fichiers_tenus(&racine.join("absent"), &base, "t", &chemins(&["src/"])).unwrap_err();
    assert_eq!(refus.code, Code::GitImpossible);
    assert!(refus.preuve.starts_with("cannot start git status "), "{refus}");
    let refus = fichiers_tenus(&bac, &"0".repeat(40), "t", &chemins(&["src/"])).unwrap_err();
    assert_eq!(refus.code, Code::GitImpossible);
    assert!(refus.preuve.starts_with("git diff --name-only -z --no-renames 0000"), "{refus}");
    assert!(!refus.correction.trim().is_empty());
    // Le même bac, avec un départ connu, passe.
    assert_eq!(fichiers_tenus(&bac, &base, "t", &chemins(&["src/"])), Ok(()));
    let _ = fs::remove_dir_all(&racine);
}

/// Pose une butée du mur : un fichier vide en lecture seule.
fn butee(bac: &Path, chemin: &str) {
    ecrire(bac, chemin, "");
    let fichier = bac.join(chemin);
    let mut droits = fs::metadata(&fichier).unwrap().permissions();
    droits.set_readonly(true);
    fs::set_permissions(&fichier, droits).unwrap();
}

#[test]
fn le_gel_epargne_les_butees_du_mur() {
    let (racine, depot, kraken, base) = vise("gel-butees");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let bac = prise(&depot, &kraken, "t", &base, &[("travail.txt", "fait")]);
    for nom in ["settings.json", "settings.local.json", "skills", "loop.md"] {
        butee(&bac, &format!(".claude/{nom}"));
    }
    assert_eq!(sans_claude(&bac, &base), Ok(()));
    let (_, issue) = assemblage.fusionner(&bac, &base, "t", None, 1, |_, _| Ok(()));
    issue.unwrap();
    // Le commit du gel ne porte que le vrai changement.
    assert_eq!(git(&bac, &["diff", "--name-only", &base, "HEAD"]), "travail.txt");
    assert_ne!(assemblage.tete(), base.as_str());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_gel_refuse_un_fichier_non_vide_ou_inscriptible_sous_claude() {
    for (nom, contenu, lecture_seule) in [("non-vide", "x", true), ("inscriptible", "", false)] {
        let (racine, depot, kraken, base) = vise(&format!("gel-pas-butee-{nom}"));
        let profil = profil();
        let assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
        let bac = prise(&depot, &kraken, "t", &base, &[("travail.txt", "fait")]);
        butee(&bac, ".claude/loop.md");
        assert_eq!(sans_claude(&bac, &base), Ok(()));
        let fichier = bac.join(".claude/loop.md");
        let mut droits = fs::metadata(&fichier).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        droits.set_readonly(false);
        fs::set_permissions(&fichier, droits.clone()).unwrap();
        fs::write(&fichier, contenu).unwrap();
        droits.set_readonly(lecture_seule);
        fs::set_permissions(&fichier, droits).unwrap();
        let (verif, issue) = assemblage.assembler(
            &bac,
            &base,
            "t",
            None,
            1,
            &[],
            |_| Ok(()),
            |_, _| Ok(()),
            |_, _| panic!("un .claude au gel ne se note pas"),
        );
        let refus = issue.unwrap_err();
        assert_eq!((refus.code, verif), (Code::ClaudeTouche, None), "{nom}");
        assert!(refus.preuve.ends_with(r#": ".claude/loop.md""#), "{refus}");
        let _ = fs::remove_dir_all(&racine);
    }
}

/// Rend le fichier `chemin` du bac inscriptible ou en lecture seule.
fn regler_ecriture(bac: &Path, chemin: &str, inscriptible: bool) {
    let fichier = bac.join(chemin);
    let mut droits = fs::metadata(&fichier).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    droits.set_readonly(!inscriptible);
    fs::set_permissions(&fichier, droits).unwrap();
}

#[test]
fn une_butee_a_un_chemin_protege_absent_n_est_ni_7013_ni_7018_et_le_gel_ne_la_commite_pas() {
    let cas = [
        ("lecons", profil(), ".agents/skills"),
        ("dossier", profil().avec_proteges(&["vendor/"]), "vendor"),
        ("fichier", profil().avec_proteges(&["secret.env"]), "secret.env"),
    ];
    for (nom, profil, chemin) in cas {
        let (racine, depot, kraken, base) = vise_avec_fichiers(&format!("butee-protegee-{nom}"));
        let proteges = profil.proteges();
        let permis = chemins(&["src/"]);
        let bac = prise(&depot, &kraken, "t", &base, &[("src/a.txt", "fait\n")]);
        butee(&bac, chemin);
        // Git voit la butée, comme un non suivi : sans la tolérance, elle serait un chemin changé.
        let vu = git(&bac, &["status", "--porcelain", "--untracked-files=all"]);
        assert!(vu.lines().any(|ligne| ligne == format!("?? {chemin}")), "{nom} : {vu}");
        let mut avec = vec!["src/a.txt".to_string(), chemin.to_string()];
        avec.sort();
        assert_eq!(super::chemins_changes(&bac, &base, &[]).unwrap(), avec, "{nom}");
        let refus = super::fichiers_tenus(&bac, &base, "t", &permis, &[]).unwrap_err();
        assert_eq!(refus.code, Code::HorsFichiers, "{nom}");
        // Avec les chemins protégés du profil : ni 7018, ni 7013, ni chemin changé.
        assert_eq!(proteges_tenus(&bac, &base, "t", proteges), Ok(()), "{nom}");
        assert_eq!(super::fichiers_tenus(&bac, &base, "t", &permis, proteges), Ok(()), "{nom}");
        assert_eq!(super::chemins_changes(&bac, &base, proteges).unwrap(), ["src/a.txt"], "{nom}");
        // Le gel ne la commite pas : la branche de la vague reçoit le vrai changement, et lui seul.
        let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
        let sha = assemblage.fusionner(&bac, &base, "t", Some(&permis), 1, |_, _| Ok(())).1.unwrap();
        assert_eq!(git(&bac, &["diff", "--name-only", &base, "HEAD"]), "src/a.txt", "{nom}");
        assert!(!fichiers(&depot, &sha).iter().any(|fichier| fichier.starts_with(chemin)), "{nom}");
        let _ = fs::remove_dir_all(&racine);
    }
}

#[test]
fn un_fichier_non_vide_ou_inscriptible_a_un_chemin_protege_reste_le_refus_7018() {
    for (nom, contenu, lecture_seule) in [("non-vide", "x", true), ("inscriptible", "", false)] {
        let (racine, depot, kraken, base) = vise_avec_fichiers(&format!("protege-pas-butee-{nom}"));
        let profil = profil();
        let permis = chemins(&["src/"]);
        let bac = prise(&depot, &kraken, "t", &base, &[("src/a.txt", "fait\n"), (".agents/skills", contenu)]);
        regler_ecriture(&bac, ".agents/skills", !lecture_seule);
        // À la fin de la tentative, avant les `fichiers`.
        let refus = proteges_tenus(&bac, &base, "t", profil.proteges()).unwrap_err();
        assert_eq!(refus.code, Code::CheminProtege, "{nom}");
        assert!(refus.preuve.ends_with(r#": ".agents/skills""#), "{nom} : {refus}");
        // Au gel : la tâche est refusée, rien n'est noté.
        let assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
        let (verif, issue) = assemblage.assembler(
            &bac,
            &base,
            "t",
            Some(&permis),
            1,
            &[],
            |_| Ok(()),
            |_, _| Ok(()),
            |_, _| panic!("un chemin protégé ne se note pas"),
        );
        let refus = issue.unwrap_err();
        assert_eq!((refus.code, verif), (Code::CheminProtege, None), "{nom}");
        assert!(refus.preuve.ends_with(r#": ".agents/skills""#), "{nom} : {refus}");
        let _ = fs::remove_dir_all(&racine);
    }
}

#[test]
fn un_fichier_vide_en_lecture_seule_a_un_chemin_non_protege_reste_un_chemin_change() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("butee-non-protegee");
    let profil = profil();
    let permis = chemins(&["src/"]);
    let bac = prise(&depot, &kraken, "t", &base, &[("src/a.txt", "fait\n")]);
    butee(&bac, ".bashrc");
    let attendu = [".bashrc", "src/a.txt"];
    assert_eq!(super::chemins_changes(&bac, &base, profil.proteges()).unwrap(), attendu);
    // Il n'est pas protégé : pas de 7018 ; mais il n'est pas dans les `fichiers` de la tâche : 7013.
    assert_eq!(proteges_tenus(&bac, &base, "t", profil.proteges()), Ok(()));
    let refus = super::fichiers_tenus(&bac, &base, "t", &permis, profil.proteges()).unwrap_err();
    assert_eq!(refus.code, Code::HorsFichiers);
    assert!(refus.preuve.ends_with(r#": ".bashrc""#), "{refus}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_chemin_protege_qui_existait_au_depart_et_que_la_main_change_reste_le_refus_7018() {
    let (racine, depot, kraken, _) = vise("protege-present");
    ecrire(&depot, "secret.env", "x");
    git(&depot, &["add", "-A"]);
    git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "secret"]);
    let base = git(&depot, &["rev-parse", "HEAD"]);
    let profil = profil().avec_proteges(&["secret.env"]);
    // La main le vide et lui ôte l'écriture : il a la forme d'une butée, mais il existait au départ.
    let bac = prise(&depot, &kraken, "t", &base, &[("secret.env", "")]);
    regler_ecriture(&bac, "secret.env", false);
    let refus = proteges_tenus(&bac, &base, "t", profil.proteges()).unwrap_err();
    assert_eq!(refus.code, Code::CheminProtege);
    assert!(refus.preuve.ends_with(r#": "secret.env""#), "{refus}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_chemin_protege_change_est_le_refus_7018_a_la_fin_de_la_tentative() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("protege-tentative");
    let bac = prise(&depot, &kraken, "t", &base, &[("src/b.txt", "autre\n"), ("src/neuf/x.txt", "1")]);
    // Sans chemin protégé, rien n'est jugé, et git n'est pas même interrogé.
    assert_eq!(proteges_tenus(&bac, &base, "t", &[]), Ok(()));
    assert_eq!(proteges_tenus(&racine.join("absent"), &"0".repeat(40), "t", &[]), Ok(()));
    // Un fichier protégé change : le refus 7018, avec la preuve et la correction de la tentative.
    let refus = proteges_tenus(&bac, &base, "t", &chemins(&["src/b.txt"])).unwrap_err();
    assert_eq!(refus.code, Code::CheminProtege);
    let preuve = r#"the hand of task t changed paths that its profile protects: "src/b.txt""#;
    assert_eq!(refus.preuve, preuve);
    assert!(refus.correction.contains("a protected path changes only through its profile"), "{refus}");
    // Un dossier protégé couvre ce qui est sous lui, jusqu'au non suivi, et rien de ses voisins de même préfixe.
    let refus = proteges_tenus(&bac, &base, "t", &chemins(&["src/"])).unwrap_err();
    assert_eq!(refus.code, Code::CheminProtege);
    assert!(refus.preuve.ends_with(r#": "src/b.txt", "src/neuf/x.txt""#), "{refus}");
    assert_eq!(proteges_tenus(&bac, &base, "t", &chemins(&["sr/", "src/neuf", "doc/", "src/a.txt"])), Ok(()));
    // Un chemin protégé effacé est changé ; commité, de même.
    let efface = prise(&depot, &kraken, "efface", &base, &[]);
    fs::remove_file(efface.join("doc/c.txt")).unwrap();
    let refus = proteges_tenus(&efface, &base, "t", &chemins(&["doc/c.txt"])).unwrap_err();
    assert!(refus.preuve.ends_with(r#": "doc/c.txt""#), "{refus}");
    let commite = prise(&depot, &kraken, "commite", &base, &[("src/a.txt", "commité\n")]);
    commiter(&commite);
    let refus = proteges_tenus(&commite, &base, "t", &chemins(&["src/a.txt"])).unwrap_err();
    assert_eq!(refus.code, Code::CheminProtege);
    // Un git qui ne répond pas est le refus 7009, pas un « rien changé ».
    let refus = proteges_tenus(&racine.join("absent"), &base, "t", &chemins(&["src/"])).unwrap_err();
    assert_eq!(refus.code, Code::GitImpossible);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn les_lecons_sont_protegees_meme_pour_un_profil_sans_proteges() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("protege-lecons");
    let profil = profil();
    assert_eq!(profil.proteges(), [".agents/skills/"]);
    // À la fin de la tentative : une leçon changée est le refus 7018 ; un fichier voisin est libre.
    let lecon = prise(&depot, &kraken, "lecon", &base, &[(".agents/skills/x/SKILL.md", "pas une lecon\n")]);
    let refus = proteges_tenus(&lecon, &base, "t", profil.proteges()).unwrap_err();
    assert_eq!(refus.code, Code::CheminProtege);
    assert!(refus.preuve.ends_with(r#": ".agents/skills/x/SKILL.md""#), "{refus}");
    let voisin = prise(&depot, &kraken, "voisin", &base, &[(".agents/autre", "libre\n"), (".agents/skillset/y", "1")]);
    assert_eq!(proteges_tenus(&voisin, &base, "t", profil.proteges()), Ok(()));
    // Au gel : la tâche est refusée, rien n'est fusionné ; le voisin s'assemble.
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let (verif, issue) = assemblage.assembler(
        &lecon,
        &base,
        "lecon",
        None,
        1,
        &[],
        |_| Ok(()),
        |_, _| Ok(()),
        |_, _| panic!("une leçon changée ne se note pas"),
    );
    assert_eq!((issue.unwrap_err().code, verif), (Code::CheminProtege, None));
    assert_eq!(assemblage.tete(), base);
    let sha = assemblage.fusionner(&voisin, &base, "voisin", None, 2, |_, _| Ok(())).1.unwrap();
    assert!(fichiers(&depot, &sha).contains(&".agents/autre".to_string()));
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_chemin_protege_au_gel_refuse_la_tache_et_rien_n_est_fusionne() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("protege-gel");
    let profil = Profil::faux(vec!["lydite-main-inutile".into()], DELAI).avec_proteges(&["src/b.txt", "doc/"]);
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let a_la_tete = |assemblage: &Assemblage<'_>| {
        assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
        a_sa_tete(&kraken, &base);
    };
    // Des `fichiers` qui couvrent le chemin protégé ne le sauvent pas : le protégé l'emporte.
    let couvrant = prise(&depot, &kraken, "a", &base, &[("src/a.txt", "ok\n"), ("src/b.txt", "autre\n")]);
    let permis = chemins(&["src/"]);
    let (verif, issue) = assemblage.assembler(
        &couvrant,
        &base,
        "a",
        Some(&permis),
        1,
        &[],
        |_| Ok(()),
        |_, _| Ok(()),
        |_, _| panic!("un chemin protégé ne se note pas"),
    );
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::CheminProtege, None));
    assert_eq!(refus.preuve, r#"the hand of task a changed paths that its profile protects: "src/b.txt""#);
    assert!(refus.correction.starts_with("The task is refused and nothing of it is merged. Restore these paths"));
    assert!(refus.correction.contains("a protected path changes only through its profile, which the human accepts."));
    a_la_tete(&assemblage);
    // Une tâche sans `fichiers` reste bornée par `proteges`, et un dossier protégé couvre ce qui est sous lui.
    let libre = prise(&depot, &kraken, "b", &base, &[("neuf.txt", "1"), ("doc/profond/d.txt", "2")]);
    let (verif, issue) = assemblage.assembler(
        &libre,
        &base,
        "b",
        None,
        2,
        &[],
        |_| Ok(()),
        |_, _| Ok(()),
        |_, _| panic!("un chemin protégé ne se note pas"),
    );
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::CheminProtege, None));
    assert!(refus.preuve.ends_with(r#": "doc/profond/d.txt""#), "{refus}");
    a_la_tete(&assemblage);
    // Par `fusionner`, la tête et la branche reviennent de même, et la tâche refusée ne laisse rien derrière.
    let (_, issue) = assemblage.fusionner(&libre, &base, "b", None, 2, |_, _| panic!("rien ne se note"));
    assert_eq!(issue.unwrap_err().code, Code::CheminProtege);
    a_la_tete(&assemblage);
    // Un travail qui ne touche aucun chemin protégé s'assemble comme avant, avec ou sans `fichiers`.
    let bon = prise(&depot, &kraken, "c", &base, &[("src/a.txt", "bon\n")]);
    let sha = assemblage.fusionner(&bon, &base, "c", Some(&permis), 3, |_, _| Ok(())).1.unwrap();
    assert_eq!(assemblage.tete(), sha);
    let propre = prise(&depot, &kraken, "d", &sha, &[("neuf2.txt", "2")]);
    assert!(assemblage.fusionner(&propre, &sha, "d", None, 4, |_, _| Ok(())).1.is_ok());
    let _ = fs::remove_dir_all(&racine);
}

/// Le SHA-256 de ces octets, en hexadécimaux.
fn sha(octets: &str) -> String {
    hex(&sha256(octets.as_bytes()))
}

/// Comme [`vise`], mais la base dit `* text=auto` : git y normalise les fins de ligne `\r\n` en `\n` à l'ajout, et le
/// commit gelé garde d'autres octets que le disque.
fn vise_normalisant(nom: &str) -> (PathBuf, PathBuf, PathBuf, String) {
    let (racine, depot, kraken, _) = vise(nom);
    ecrire(&depot, ".gitattributes", "* text=auto\n");
    git(&depot, &["add", "-A"]);
    git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "normalise"]);
    let base = git(&depot, &["rev-parse", "HEAD"]);
    (racine, depot, kraken, base)
}

/// Le gel de la tâche `id` (numéro `k`) sous ces épingles, sans note de fusion ni d'empreintes à attendre : le refus,
/// ou le sha de la fusion et ce que la note des empreintes a reçu.
fn geler_sous_epingles(
    assemblage: &mut Assemblage<'_>,
    bac: &Path,
    base: &str,
    (id, fichiers, k): (&str, Option<&[String]>, usize),
    epingles: &[Epingle<'_>],
) -> Result<(String, Vec<String>), Refus> {
    let mut notees = Vec::new();
    let (_, issue) = assemblage.fusionner_et_noter_le_gel(
        bac,
        base,
        id,
        fichiers,
        k,
        epingles,
        |_| Ok(()),
        |_, lues| {
            notees = lues;
            Ok(())
        },
        |_, _| Ok(()),
    );
    issue.map(|sha| (sha, notees))
}

#[test]
fn un_fichier_epingle_que_git_garde_sous_d_autres_octets_que_le_disque_est_le_refus_7022_au_gel() {
    let (racine, depot, kraken, base) = vise_normalisant("empreinte-normalisee");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let (disque, garde) = (sha("a\r\nb\r\n"), sha("a\nb\n"));
    assert_ne!(disque, garde);
    let permis = chemins(&["src/"]);
    // Épinglé par l'empreinte des octets du disque (`\r\n`) : git garde `\n`, le gel refuse, la branche ne bouge pas.
    let bac = prise(&depot, &kraken, "a", &base, &[("src/crlf.txt", "a\r\nb\r\n")]);
    let pins = [Epingle { tache: "a", fichier: "src/crlf.txt", empreinte: &disque }];
    let refus = geler_sous_epingles(&mut assemblage, &bac, &base, ("a", Some(&permis), 1), &pins).unwrap_err();
    assert_eq!(refus.code.numero(), 7022, "{refus}");
    for vu in ["task a", "src/crlf.txt", disque.as_str(), garde.as_str()] {
        assert!(refus.preuve.contains(vu), "{vu} : {refus}");
    }
    assert!(refus.correction.contains("fresh wave") && refus.correction.contains("bytes git stores"), "{refus}");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    // Épinglé par l'empreinte de ce que git garde : la tâche est assemblée, et la note dit son épingle.
    let bac = prise(&depot, &kraken, "b", &base, &[("src/crlf.txt", "a\r\nb\r\n")]);
    let pins = [Epingle { tache: "b", fichier: "src/crlf.txt", empreinte: &garde }];
    let (sha_fusion, notees) =
        geler_sous_epingles(&mut assemblage, &bac, &base, ("b", Some(&permis), 2), &pins).unwrap();
    assert_eq!(notees, [format!("src/crlf.txt {garde}")]);
    assert_eq!(assemblage.tete(), sha_fusion);
    assert_eq!(git(&depot, &["show", &format!("{sha_fusion}:src/crlf.txt")]), "a\nb");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_tache_sans_fichiers_qui_a_une_epingle_est_controlee_au_gel() {
    let (racine, depot, kraken, base) = vise("empreinte-sans-fichiers");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let attendue = sha("x\n");
    let pins_de = |id| [Epingle { tache: id, fichier: "x.txt", empreinte: &attendue }];
    // Le fichier épinglé manque : la tâche n'a ni `fichiers` ni chemin protégé, et le gel la refuse en 7022.
    let bac = prise(&depot, &kraken, "t", &base, &[("autre.txt", "y\n")]);
    let refus = geler_sous_epingles(&mut assemblage, &bac, &base, ("t", None, 1), &pins_de("t")).unwrap_err();
    assert_eq!(refus.code.numero(), 7022, "{refus}");
    assert!(refus.preuve.contains("x.txt") && refus.preuve.contains(&attendue), "{refus}");
    assert!(refus.preuve.contains("git stores no such file"), "{refus}");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    // Un autre octet : refusé de même, la preuve cite l'empreinte vue.
    let bac = prise(&depot, &kraken, "u", &base, &[("x.txt", "z\n")]);
    let refus = geler_sous_epingles(&mut assemblage, &bac, &base, ("u", None, 2), &pins_de("u")).unwrap_err();
    assert_eq!(refus.code.numero(), 7022, "{refus}");
    assert!(refus.preuve.contains(&sha("z\n")), "{refus}");
    // Le bon octet : assemblé.
    let bac = prise(&depot, &kraken, "v", &base, &[("x.txt", "x\n")]);
    let (_, notees) = geler_sous_epingles(&mut assemblage, &bac, &base, ("v", None, 3), &pins_de("v")).unwrap();
    assert_eq!(notees, [format!("x.txt {attendue}")]);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_gel_qui_change_le_fichier_epingle_d_une_autre_tache_est_refuse_en_7022() {
    let (racine, depot, kraken, base) = vise_avec_fichiers("empreinte-d-une-autre");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    // `autre` épingle `src/a.txt`, que la base porte déjà sous cette empreinte.
    let attendue = sha("a\n");
    let pins = [Epingle { tache: "autre", fichier: "src/a.txt", empreinte: &attendue }];
    let permis = chemins(&["src/"]);
    // Une tâche qui change ce fichier est refusée, bien qu'il soit dans ses `fichiers` ; la preuve nomme les deux.
    let bac = prise(&depot, &kraken, "t", &base, &[("src/a.txt", "changé\n")]);
    let refus = geler_sous_epingles(&mut assemblage, &bac, &base, ("t", Some(&permis), 1), &pins).unwrap_err();
    assert_eq!(refus.code.numero(), 7022, "{refus}");
    for vu in ["task t", "task autre", "src/a.txt", attendue.as_str(), sha("changé\n").as_str()] {
        assert!(refus.preuve.contains(vu), "{vu} : {refus}");
    }
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    // Sans `fichiers` non plus, rien ne passe.
    let bac = prise(&depot, &kraken, "u", &base, &[("src/a.txt", "encore\n")]);
    let refus = geler_sous_epingles(&mut assemblage, &bac, &base, ("u", None, 2), &pins).unwrap_err();
    assert_eq!(refus.code.numero(), 7022, "{refus}");
    // Une tâche qui ne touche pas ce fichier n'a rien à voir avec l'épingle d'une autre : assemblée, et sans note.
    let bac = prise(&depot, &kraken, "w", &base, &[("src/b.txt", "autre\n")]);
    let (_, notees) = geler_sous_epingles(&mut assemblage, &bac, &base, ("w", Some(&permis), 3), &pins).unwrap();
    assert!(notees.is_empty(), "{notees:?}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_note_des_empreintes_qui_ne_s_ecrit_pas_est_le_refus_7012_et_rien_n_est_fusionne() {
    let (racine, depot, kraken, base) = vise("empreinte-note-ratee");
    let profil = profil();
    let mut assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    let attendue = sha("x\n");
    let pins = [Epingle { tache: "t", fichier: "x.txt", empreinte: &attendue }];
    let bac = prise(&depot, &kraken, "t", &base, &[("x.txt", "x\n")]);
    let (verif, issue) = assemblage.fusionner_et_noter_le_gel(
        &bac,
        &base,
        "t",
        None,
        1,
        &pins,
        |_| Ok(()),
        |_, _| Err(io::Error::other("fil en lecture seule")),
        |_, _| panic!("rien ne se note après une note ratée"),
    );
    let refus = issue.unwrap_err();
    assert_eq!((refus.code, verif), (Code::FilInjoignable, None), "{refus}");
    assert!(refus.preuve.contains("fil en lecture seule"), "{refus}");
    assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
    a_sa_tete(&kraken, &base);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_fichier_qui_s_execute_a_l_ouverture_au_gel_est_le_refus_7021_et_la_branche_ne_bouge_pas() {
    let (racine, depot, kraken, base) = vise("ouverture-au-gel");
    let profil = profil();
    let assemblage = ouvrir(&depot, &kraken, &profil, "v", &base, PREUVE).unwrap();
    // La main écrit, à la racine de son bac, un fichier que l'éditeur ou l'agent lit à la prochaine ouverture ; la
    // tâche a ou non des `fichiers`, qui couvrent même le chemin : le gel est refusé de même, et rien ne se fusionne.
    let permis = chemins(&["src/", ".vscode/", ".mcp.json"]);
    let cas: [(&str, Option<&[String]>, &str, bool); 4] = [
        ("a", Some(&permis), ".vscode/tasks.json", false),
        ("b", None, ".mcp.json", false),
        ("c", None, ".vscode/tasks.json", true),
        ("d", Some(&permis), ".mcp.json", true),
    ];
    for (k, (id, fichiers, chemin, commite_puis_ote)) in cas.into_iter().enumerate() {
        let bac = prise(&depot, &kraken, id, &base, &[("src/a.txt", "a\n"), (chemin, "{}")]);
        if commite_puis_ote {
            // Commité puis ôté : l'arbre du gel n'en dit rien, mais la fusion apporterait le commit.
            commiter(&bac);
            fs::remove_file(bac.join(chemin)).unwrap();
        }
        let (verif, issue) = assemblage.assembler(
            &bac,
            &base,
            id,
            fichiers,
            k + 1,
            &[],
            |_| Ok(()),
            |_, _| Ok(()),
            |_, _| panic!("rien ne se note"),
        );
        let refus = issue.unwrap_err();
        assert_eq!((refus.code.numero(), verif), (7021, None), "{refus}");
        assert!(refus.preuve.contains(chemin), "{refus}");
        assert!(refus.correction.contains("the merge did not happen"), "{refus}");
        assert_eq!((assemblage.tete(), git(&depot, &["rev-parse", "lydite/vague-v"])), (base.as_str(), base.clone()));
        a_sa_tete(&kraken, &base);
    }
    let _ = fs::remove_dir_all(&racine);
}
