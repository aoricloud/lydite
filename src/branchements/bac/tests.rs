use super::*;
use crate::branchements::git::{decrire, faire};
use crate::coeur::fil::lot;
#[cfg(windows)]
use crate::essais::effacer_avant;
use crate::essais::{depot_neuf as depot, ecrire, effacer, etat_de_claude, git, kraken, neuf, pour_sh, script};
use std::cell::RefCell;
use std::fs;
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn le_bac_est_hors_du_depot() {
    let depot = depot("hors");
    let kraken = neuf("hors-lydite"); // pas encore créé : le premier bac le crée
    let pose = bac(&depot, &kraken, "main-1", "HEAD").unwrap();
    assert_eq!(pose, kraken.join("bacs").join("main-1"));
    assert!(pose.is_dir() && !pose.starts_with(&depot));
    assert_eq!(git(&pose, &["branch", "--show-current"]), "lydite/main-1");
    assert_eq!(git(&depot, &["status", "--porcelain"]), "", "le dépôt visé ne voit rien du bac");
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn un_bac_part_de_son_depart() {
    let (depot, kraken) = (depot("depart"), neuf("depart-lydite"));
    let premier = git(&depot, &["rev-parse", "HEAD"]);
    git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "second"]);
    let second = git(&depot, &["rev-parse", "HEAD"]);
    // Un sha, ou le HEAD du dépôt : chaque bac en part, sur sa branche à lui.
    for (id, depart, attendu) in [("main-1", premier.as_str(), &premier), ("main-2", "HEAD", &second)] {
        let pose = bac(&depot, &kraken, id, depart).unwrap();
        assert_eq!(&git(&pose, &["rev-parse", "HEAD"]), attendu, "{id}");
        assert_eq!(git(&pose, &["branch", "--show-current"]), format!("lydite/{id}"));
    }
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

/// Un processus qui a `dir` pour dossier courant et y vit `secondes` : il pose `pret` avant tout, et le test
/// l'attend.
fn tenir(dir: &Path, secondes: u32) -> Child {
    let (programme, commande) = if cfg!(windows) {
        ("cmd", ["/C".to_string(), format!("type nul > pret & ping -n {} 127.0.0.1 >nul", secondes + 1)])
    } else {
        ("sh", ["-c".to_string(), format!(": > pret; sleep {secondes}")])
    };
    let tenant = Command::new(programme)
        .args(commande)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let debut = Instant::now();
    while !dir.join("pret").exists() {
        assert!(debut.elapsed() < Duration::from_secs(30), "le processus n'a pas posé pret dans {}", dir.display());
        thread::sleep(Duration::from_millis(20));
    }
    tenant
}

#[test]
fn effacer_attend_le_processus_qui_tient_le_dossier() {
    let dir = neuf("effacer");
    fs::create_dir_all(&dir).unwrap();
    let mut tenant = tenir(&dir, 2);
    // Sous Windows, un processus qui a le dossier pour dossier courant l'empêche de s'effacer ; ailleurs, non.
    assert_eq!(fs::remove_dir_all(&dir).is_err(), cfg!(windows));
    effacer(&dir);
    assert!(!dir.exists());
    tenant.wait().unwrap();
}

#[cfg(windows)]
#[test]
fn effacer_ne_se_tait_pas_quand_le_dossier_ne_s_efface_pas() {
    let dir = neuf("effacer-tenu");
    fs::create_dir_all(&dir).unwrap();
    let mut tenant = tenir(&dir, 4);
    let panique = std::panic::catch_unwind(|| effacer_avant(&dir, Duration::from_millis(300)));
    let message = panique.expect_err("le dossier tenu ne s'efface pas en 300 ms");
    let message = message.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.starts_with(&format!("le dossier {} ne s'efface pas : ", dir.display())), "{message}");
    tenant.wait().unwrap();
    effacer(&dir);
}

#[test]
fn un_bac_ne_joue_aucun_crochet_du_depot() {
    let (depot, kraken, temoin) = (depot("crochets"), neuf("crochets-lydite"), neuf("crochets-temoin"));
    // Deux crochets du dépôt visé, que `git worktree add` joue : chacun ajoute son nom au témoin. Le dépôt les
    // nomme lui-même par son core.hooksPath, qu'un réglage global ne remplace pas.
    let crochets = depot.join(".git").join("hooks");
    for nom in ["post-checkout", "reference-transaction"] {
        script(&crochets.join(nom), &format!("echo {nom} >> '{}'", pour_sh(&temoin)));
    }
    git(&depot, &["config", "core.hooksPath", &pour_sh(&crochets)]);
    // Le témoin marche : un `git worktree add` ordinaire joue les deux.
    let ordinaire = neuf("crochets-ordinaire");
    git(&depot, &["worktree", "add", "-q", "-b", "temoin", &ordinaire.to_string_lossy()]);
    let joues = fs::read_to_string(&temoin).unwrap_or_default();
    assert!(joues.contains("post-checkout") && joues.contains("reference-transaction"), "témoin : {joues:?}");
    fs::write(&temoin, "").unwrap();
    bac(&depot, &kraken, "main-1", "HEAD").unwrap();
    assert_eq!(fs::read_to_string(&temoin).unwrap(), "", "le bac ne joue aucun crochet du dépôt visé");
    for dir in [depot, kraken, ordinaire] {
        let _ = fs::remove_dir_all(dir);
    }
    let _ = fs::remove_file(&temoin);
}

/// Un bac neuf du dépôt sous `kraken`, posé sur le HEAD du dépôt : son chemin, et son départ en sha.
fn pose(depot: &Path, kraken: &Path) -> (PathBuf, String) {
    let depart = git(depot, &["rev-parse", "HEAD"]);
    (bac(depot, kraken, &format!("main-{}", lot()), &depart).unwrap(), depart)
}

/// Le refus 7006 du bac où la main a `fait` quelque chose, et les seuls noms `vus` dans sa preuve, à leur place.
fn touche(depot: &Path, kraken: &Path, fait: impl FnOnce(&Path), vus: &[&str]) {
    let (bac, depart) = pose(depot, kraken);
    fait(&bac);
    let refus = sans_claude(&bac, &depart).unwrap_err();
    assert_eq!(refus.code, Code::ClaudeTouche, "{vus:?} : {refus}");
    let preuve = format!("la main a touché un dossier .claude dans son bac {} : {}", bac.display(), vus.join(", "));
    assert_eq!(refus.preuve, preuve);
    assert!(!refus.correction.trim().is_empty());
}

/// Un commit de test dans `dir`, de tout ce qui y a changé.
fn commit(dir: &Path) {
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "commit de la main"]);
}

/// Un commit de test dans `dir`, de ce qui est déjà dans l'index et rien d'autre.
fn commit_de_l_index(dir: &Path, message: &str) {
    git(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", message]);
}

#[test]
fn sans_claude_voit_suivi_non_suivi_ignore_commite_efface_et_autre_casse() {
    let (depot, kraken) = (depot("claude"), neuf("claude-lydite"));
    // Le dépôt suit .claude/settings.json et ignore *.local.json, comme le settings.local.json de Claude Code.
    ecrire(&depot, ".claude/settings.json", "{}");
    ecrire(&depot, ".gitignore", "*.local.json\n");
    commit(&depot);
    // Rien de touché, ou un nom qui n'est pas `.claude` : le bac passe.
    let (bac, depart) = pose(&depot, &kraken);
    ecrire(&bac, ".claude.bak", "x");
    ecrire(&bac, "sous/.claude.bak/settings.json", "{}");
    assert_eq!(sans_claude(&bac, &depart), Ok(()));
    // Chaque geste de la main, dans un bac à lui, est vu et nommé.
    touche(&depot, &kraken, |bac| ecrire(bac, ".claude/settings.json", "{\"modifie\":1}"), &[".claude/settings.json"]);
    touche(
        &depot,
        &kraken,
        |bac| fs::remove_file(bac.join(".claude/settings.json")).unwrap(),
        &[".claude/settings.json"],
    );
    let commite = |bac: &Path| {
        ecrire(bac, ".claude/commands/c.md", "commande");
        commit(bac);
    };
    touche(&depot, &kraken, commite, &[".claude/commands/c.md"]);
    touche(&depot, &kraken, |bac| ecrire(bac, ".claude/agents/a.md", "agent"), &[".claude/agents/a.md"]);
    touche(&depot, &kraken, |bac| ecrire(bac, ".claude/settings.local.json", "{}"), &[".claude/settings.local.json"]);
    touche(&depot, &kraken, |bac| ecrire(bac, "sous/.Claude/settings.json", "{}"), &["sous/.Claude/settings.json"]);
    // Git qui ne part pas (un bac absent) ou qui refuse (un départ inconnu) : le refus 7009, jamais un bac propre.
    let absent = sans_claude(&neuf("claude-absent"), &depart).unwrap_err();
    assert_eq!(absent.code, Code::GitImpossible);
    assert!(absent.preuve.starts_with("lancement impossible de git diff --name-only "), "{absent}");
    let inconnu = sans_claude(&bac, &"0".repeat(40)).unwrap_err();
    assert_eq!(inconnu.code, Code::GitImpossible);
    assert!(inconnu.preuve.starts_with("git diff --name-only 0000"), "{inconnu}");
    assert!(inconnu.preuve.contains(&bac.display().to_string()), "le dossier du geste : {inconnu}");
    assert!(!inconnu.correction.trim().is_empty());
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn sans_claude_voit_un_claude_stage_puis_efface() {
    let (depot, kraken) = (depot("stage-puis-efface"), neuf("stage-puis-efface-lydite"));
    // Un .claude stagé que la main efface ensuite du worktree : `git diff` n'en dit plus rien, `git log` non plus,
    // et `git ls-files --others` le tait, car l'index le suit. Seul `git diff --cached` le nomme. Sans quoi le gel,
    // dont l'exclusion ne retire pas une entrée déjà stagée, le commiterait.
    let stage_puis_efface = |bac: &Path| {
        ecrire(bac, ".claude/scheduled_tasks.json", "{}");
        git(bac, &["add", "-f", "--", ".claude/scheduled_tasks.json"]);
        fs::remove_file(bac.join(".claude/scheduled_tasks.json")).unwrap();
    };
    touche(&depot, &kraken, stage_puis_efface, &[".claude/scheduled_tasks.json"]);
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn sans_claude_voit_un_claude_imbrique() {
    let (depot, kraken) = (depot("imbrique"), neuf("imbrique-lydite"));
    // Claude Code charge les compétences d'un <dossier>/.claude/skills imbriqué : un .claude sous un dossier compte.
    for chemin in ["sous/.claude/skills/k/SKILL.md", "sous/.claude/settings.json"] {
        touche(&depot, &kraken, |bac| ecrire(bac, chemin, "x"), &[chemin]);
    }
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn sans_claude_voit_un_claude_commite_puis_ote() {
    let (depot, kraken) = (depot("commite-ote"), neuf("commite-ote-lydite"));
    // Un .claude commité, puis ôté : l'arbre redevient celui du départ, mais le commit reste dans l'histoire que la
    // fusion ferait entrer dans la branche de la vague. Seul `git log` le dit.
    for chemin in [".claude/settings.json", "sous/.claude/skills/k/SKILL.md"] {
        let commite = |bac: &Path| {
            ecrire(bac, chemin, "x");
            commit(bac);
        };
        // Ôté par un second commit, ou par un simple effacement que le gel commiterait.
        let par_commit = |bac: &Path| {
            commite(bac);
            fs::remove_file(bac.join(chemin)).unwrap();
            commit(bac);
        };
        touche(&depot, &kraken, par_commit, &[chemin]);
        let par_effacement = |bac: &Path| {
            commite(bac);
            fs::remove_file(bac.join(chemin)).unwrap();
        };
        touche(&depot, &kraken, par_effacement, &[chemin]);
    }
    // Le worktree d'un sous-agent, commité (un gitlink, mode 160000) puis retiré de l'index : il reste dans le bac,
    // hors de git, où l'état de Claude Code passe, mais le premier commit porte le .claude/. Commité, cet état est
    // refusé.
    let gitlink = |bac: &Path| {
        etat_de_claude(bac);
        git(bac, &["add", "-f", ".claude/worktrees/x"]);
        commit_de_l_index(bac, "worktree commité");
        git(bac, &["rm", "-q", "--cached", ".claude/worktrees/x"]);
        commit_de_l_index(bac, "worktree retiré");
    };
    touche(&depot, &kraken, gitlink, &[".claude/worktrees/x"]);
    // Le départ compte : un .claude que le départ porte déjà n'est le fait d'aucune main.
    ecrire(&depot, ".claude/settings.json", "{}");
    commit(&depot);
    let (bac, depart) = pose(&depot, &kraken);
    ecrire(&bac, "travail.txt", "fait");
    commit(&bac);
    assert_eq!(sans_claude(&bac, &depart), Ok(()));
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

/// Pose une butée du mur : un fichier vide en lecture seule.
fn butee(bac: &Path, chemin: &str) {
    ecrire(bac, chemin, "");
    regler_ecriture(&bac.join(chemin), false);
}

/// Rend un fichier inscriptible ou en lecture seule.
fn regler_ecriture(fichier: &Path, inscriptible: bool) {
    let mut droits = fs::metadata(fichier).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    droits.set_readonly(!inscriptible);
    fs::set_permissions(fichier, droits).unwrap();
}

#[test]
fn sans_claude_laisse_passer_les_butees_du_mur() {
    let (depot, kraken) = (depot("butees"), neuf("butees-lydite"));
    let (bac, depart) = pose(&depot, &kraken);
    for nom in ["settings.json", "settings.local.json", "skills", "scheduled_tasks.json", "loop.md"] {
        butee(&bac, &format!(".claude/{nom}"));
    }
    assert_eq!(sans_claude(&bac, &depart), Ok(()));
    // Une butée à laquelle la main ajoute un octet, ou qu'elle rend inscriptible, est refusée.
    let avec_octet = |bac: &Path| {
        butee(bac, ".claude/hooks");
        regler_ecriture(&bac.join(".claude/hooks"), true);
        fs::write(bac.join(".claude/hooks"), "x").unwrap();
        regler_ecriture(&bac.join(".claude/hooks"), false);
    };
    touche(&depot, &kraken, avec_octet, &[".claude/hooks"]);
    let inscriptible = |bac: &Path| {
        butee(bac, ".claude/settings.json");
        regler_ecriture(&bac.join(".claude/settings.json"), true);
    };
    touche(&depot, &kraken, inscriptible, &[".claude/settings.json"]);
    // Un fichier vide inscriptible est refusé.
    touche(&depot, &kraken, |bac| ecrire(bac, ".claude/loop.md", ""), &[".claude/loop.md"]);
    // Un dossier qui porte un fichier est refusé.
    touche(&depot, &kraken, |bac| ecrire(bac, ".claude/hooks/h.sh", ""), &[".claude/hooks/h.sh"]);
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

/// Un commit qui écrit `chemin`, posé hors de la branche du bac, sur un HEAD détaché : rend son sha, le bac étant
/// revenu sur sa branche. Aucune branche n'est nommée, car celles du dépôt sont partagées entre ses bacs.
fn commit_a_cote(bac: &Path, chemin: &str) -> String {
    let branche = git(bac, &["branch", "--show-current"]);
    git(bac, &["switch", "-q", "--detach"]);
    ecrire(bac, chemin, "x");
    commit(bac);
    let autre = git(bac, &["rev-parse", "HEAD"]);
    git(bac, &["switch", "-q", &branche]);
    autre
}

#[test]
fn sans_claude_voit_un_claude_venu_d_une_fusion() {
    let (depot, kraken) = (depot("fusion"), neuf("fusion-lydite"));
    // La fusion `ours` garde l'arbre du bac, sans le .claude de l'autre côté : par défaut, git ne suit alors que le
    // côté dont la fusion a l'arbre, et ne montre pas le commit qui porte ce .claude. `-m` et `--full-history` suivent
    // les deux. Le côté du bac a un commit de la main, sans quoi son parent serait le départ, que `<départ>..HEAD`
    // exclut.
    let ours = |bac: &Path| {
        ecrire(bac, "travail.txt", "fait");
        commit(bac);
        let autre = commit_a_cote(bac, ".claude/settings.json");
        git(bac, &["-c", "user.name=t", "-c", "user.email=t@t", "merge", "-q", "--no-ff", "-s", "ours", &autre]);
    };
    touche(&depot, &kraken, ours, &[".claude/settings.json"]);
    // Une fusion qui apporte seule un .claude, puis le perd par un effacement : aucun commit ordinaire ne le porte, et
    // `git log` ne dit rien d'une fusion sans `-m`.
    let seule = |bac: &Path| {
        let autre = commit_a_cote(bac, "autre.txt");
        git(bac, &["-c", "user.name=t", "-c", "user.email=t@t", "merge", "-q", "--no-ff", "--no-commit", &autre]);
        ecrire(bac, ".claude/settings.json", "{}");
        git(bac, &["add", "-A"]);
        commit_de_l_index(bac, "fusion qui apporte un .claude");
        fs::remove_file(bac.join(".claude/settings.json")).unwrap();
    };
    touche(&depot, &kraken, seule, &[".claude/settings.json"]);
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn sans_claude_voit_l_entree_claude_elle_meme() {
    let (depot, kraken) = (depot("entree"), neuf("entree-lydite"));
    // Un fichier nommé .claude, à la racine ou sous un dossier, en toute casse : `**/.claude/**` veut un nom sous
    // .claude/ et ne le prend pas. Un lien nommé ainsi mène au dossier qu'il vise : voir le test du lien.
    for chemin in [".claude", "sous/.claude", "sous/.Claude"] {
        touche(&depot, &kraken, |bac| ecrire(bac, chemin, "reglages"), &[chemin]);
    }
    // Un lien commité, ce que git range en mode 120000 sur tout système : posé dans l'index, puis commité, sans qu'il
    // soit dans le bac. Seul `git log` le dit.
    let lien = |bac: &Path| {
        ecrire(bac, "cible", "reglages");
        let cible = git(bac, &["hash-object", "-w", "cible"]);
        fs::remove_file(bac.join("cible")).unwrap();
        git(bac, &["update-index", "--add", "--cacheinfo", &format!("120000,{cible},.claude")]);
        commit_de_l_index(bac, "lien commité");
    };
    touche(&depot, &kraken, lien, &[".claude"]);
    // Un nom qui n'est que le préfixe de .claude n'est pas lui.
    let (bac, depart) = pose(&depot, &kraken);
    ecrire(&bac, ".claude.bak", "x");
    ecrire(&bac, "sous/.claudex", "x");
    assert_eq!(sans_claude(&bac, &depart), Ok(()));
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

/// Un lien symbolique nommé `.claude` : Claude Code lit `.claude/settings.json` à travers lui, et git ne le liste que
/// comme l'entrée `.claude`.
#[cfg(unix)]
#[test]
fn sans_claude_voit_un_lien_nomme_claude() {
    use std::os::unix::fs::symlink;
    let (depot, kraken) = (depot("lien"), neuf("lien-lydite"));
    // Le lien mène à un dossier ordinaire, `reglages`, qui porte le réglage : rien sous lui n'est un .claude.
    let lier = |bac: &Path, lien: &str, cible: &str| {
        ecrire(bac, "reglages/settings.json", "{}");
        fs::create_dir_all(bac.join(lien).parent().unwrap()).unwrap();
        symlink(cible, bac.join(lien)).unwrap();
    };
    let liens = [(".claude", "reglages"), ("sous/.claude", "../reglages"), ("sous/.Claude", "../reglages")];
    for (lien, cible) in liens {
        touche(&depot, &kraken, |bac| lier(bac, lien, cible), &[lien]);
    }
    // Commité, puis ôté : seul le commit le dit.
    let commite = |bac: &Path| {
        lier(bac, ".claude", "reglages");
        commit(bac);
        fs::remove_file(bac.join(".claude")).unwrap();
    };
    touche(&depot, &kraken, commite, &[".claude"]);
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn l_etat_de_claude_code_passe_et_rien_d_autre() {
    let (depot, kraken) = (depot("etat"), neuf("etat-lydite"));
    let (bac, depart) = pose(&depot, &kraken);
    etat_de_claude(&bac);
    assert_eq!(sans_claude(&bac, &depart), Ok(()), "l'état de Claude Code, hors de git, passe");
    // À côté de cet état, la mémoire de projet, un nom qui n'en est que le préfixe, ou cet état suivi par git : le
    // refus 7006, qui ne nomme que lui.
    for chemin in [".claude/agent-memory/t/MEMORY.md", ".claude/agent-memory-locale/x"] {
        let fait = |bac: &Path| {
            etat_de_claude(bac);
            ecrire(bac, chemin, "x");
        };
        touche(&depot, &kraken, fait, &[chemin]);
    }
    let commite = |bac: &Path| {
        etat_de_claude(bac);
        git(bac, &["add", ".claude/first-run"]);
        git(bac, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "état commité"]);
    };
    touche(&depot, &kraken, commite, &[".claude/first-run"]);
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn sans_claude_ne_verifie_aucune_signature() {
    let (depot, kraken, temoin, gpg) = (depot("gpg"), neuf("gpg-lydite"), neuf("gpg-temoin"), neuf("gpg-faux"));
    // Un faux gpg : il lit son entrée jusqu'au bout, comme le vrai (git l'y écrit ; sorti avant, il fait échouer la
    // signature sous Linux, « gpg failed to sign the data »), signe de rien, trouve toute signature bonne et note
    // chaque appel dans le témoin. Le dépôt visé le nomme (gpg.program) et demande à `git log` d'afficher les
    // signatures (log.showSignature) : le programme que nomme le dépôt tournerait hors du mur, sur le commit signé
    // d'une main.
    script(
        &gpg,
        &format!(
            "while read -r _; do :; done\n\
             echo \"$@\" >> '{}'\n\
             case \"$*\" in\n\
             *--verify*) echo '[GNUPG:] GOODSIG 0 faux';;\n\
             *) echo '[GNUPG:] SIG_CREATED ' >&2; echo '-----BEGIN PGP SIGNATURE-----'; echo; echo 'ZmF1eA=='; \
             echo '-----END PGP SIGNATURE-----';;\n\
             esac",
            pour_sh(&temoin)
        ),
    );
    git(&depot, &["config", "gpg.program", &pour_sh(&gpg)]);
    git(&depot, &["config", "log.showSignature", "true"]);
    // Le commit signé de la main touche un .claude : `git log`, limité à ce chemin, ne montre (et ne vérifie) que lui.
    let (bac, depart) = pose(&depot, &kraken);
    ecrire(&bac, ".claude/settings.json", "{}");
    git(&bac, &["add", "-A"]);
    git(&bac, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-S", "-m", "signé"]);
    // Le témoin marche : un `git log` ordinaire du bac vérifie la signature.
    fs::write(&temoin, "").unwrap();
    git(&bac, &["log", "--format=", "--name-only", &format!("{depart}..HEAD")]);
    assert!(fs::read_to_string(&temoin).unwrap().contains("--verify"), "gpg joue d'ordinaire");
    fs::write(&temoin, "").unwrap();
    let refus = sans_claude(&bac, &depart).unwrap_err();
    assert_eq!(refus.code, Code::ClaudeTouche, "{refus}");
    assert_eq!(fs::read_to_string(&temoin).unwrap(), "", "sans_claude ne lance pas le gpg du dépôt visé");
    for dir in [depot, kraken] {
        let _ = fs::remove_dir_all(dir);
    }
    let _ = fs::remove_file(&temoin);
    let _ = fs::remove_file(&gpg);
}

#[test]
fn retirer_un_bac_ne_perd_aucun_travail() {
    let (depot, kraken) = (depot("retirer"), neuf("retirer-lydite"));
    let depart = git(&depot, &["rev-parse", "HEAD"]);
    // Un bac propre, son dossier temporaire et sa branche, à la valeur lue : tout se retire.
    let propre = bac(&depot, &kraken, "main-1", &depart).unwrap();
    let temporaire = temporaire_de(&propre).unwrap();
    ecrire(&temporaire, "x", "temporaire");
    assert_eq!(retirer(&depot, &propre, Some(("lydite/main-1", &depart)), faire), Ok(()));
    assert!(!propre.exists() && !temporaire.exists());
    assert_eq!(git(&depot, &["branch", "--list", "lydite/main-1"]), "");
    // Un bac qui garde un fichier non suivi : git le refuse, et rien ne bouge, ni le fichier, ni le dossier
    // temporaire, ni la branche.
    let sale = bac(&depot, &kraken, "main-2", &depart).unwrap();
    ecrire(&sale, "travail.txt", "nulle part ailleurs");
    let temporaire = temporaire_de(&sale).unwrap();
    ecrire(&temporaire, "x", "temporaire");
    let vu = retirer(&depot, &sale, Some(("lydite/main-2", &depart)), faire).unwrap_err();
    assert!(vu.starts_with("git worktree remove "), "le refus nomme le geste : {vu}");
    assert!(sale.join("travail.txt").is_file() && temporaire.is_dir());
    assert_eq!(git(&depot, &["rev-parse", "lydite/main-2"]), depart);
    // Une branche qui a bougé depuis sa lecture reste, une fois son worktree retiré.
    let parti = bac(&depot, &kraken, "main-3", &depart).unwrap();
    ecrire(&parti, "suite.txt", "commité");
    commit(&parti);
    let suite = git(&parti, &["rev-parse", "HEAD"]);
    let vu = retirer(&depot, &parti, Some(("lydite/main-3", &depart)), faire).unwrap_err();
    assert!(vu.starts_with("git update-ref -d refs/heads/lydite/main-3 "), "{vu}");
    assert!(!parti.exists());
    assert_eq!(git(&depot, &["rev-parse", "lydite/main-3"]), suite);
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn retirer_joue_ses_deux_gestes_par_le_lanceur_qu_on_lui_donne() {
    let (depot, kraken) = (depot("lanceur"), neuf("lanceur-lydite"));
    let depart = git(&depot, &["rev-parse", "HEAD"]);
    // Un lanceur qui note chaque geste avant de le jouer : il voit le retrait du worktree, puis celui de la branche, et
    // seulement eux.
    let propre = bac(&depot, &kraken, "main-1", &depart).unwrap();
    let vus = RefCell::new(Vec::new());
    let noteur = |geste: Command| {
        vus.borrow_mut().push(decrire(&geste));
        faire(geste)
    };
    assert_eq!(retirer(&depot, &propre, Some(("lydite/main-1", &depart)), noteur), Ok(()));
    let vus = vus.into_inner();
    assert_eq!(vus.len(), 2, "{vus:?}");
    assert!(vus[0].starts_with(&format!("git worktree remove {} ", propre.display())), "{vus:?}");
    assert!(vus[1].starts_with(&format!("git update-ref -d refs/heads/lydite/main-1 {depart} ")), "{vus:?}");
    assert!(!propre.exists() && git(&depot, &["branch", "--list", "lydite/main-1"]).is_empty());
    // Un lanceur qui refuse le premier geste : rien ne bouge, et `retirer` rend ce que le lanceur a dit.
    let refuse = bac(&depot, &kraken, "main-2", &depart).unwrap();
    let vu = retirer(&depot, &refuse, Some(("lydite/main-2", &depart)), |_| Err("lanceur muet".into()));
    assert_eq!(vu, Err("lanceur muet".to_string()));
    assert!(refuse.is_dir());
    assert_eq!(git(&depot, &["rev-parse", "lydite/main-2"]), depart);
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn retirer_un_bac_au_chemin_long_efface_aussi_ce_que_le_depot_ignore() {
    let (depot, kraken) = (depot("long"), neuf("long-lydite"));
    // Le dépôt ignore `cible/`, comme le `target` d'un projet cargo : git ne le compte pas pour du travail à perdre, et
    // l'efface avec le bac.
    ecrire(&depot, ".gitignore", "cible/\n");
    commit(&depot);
    let depart = git(&depot, &["rev-parse", "HEAD"]);
    let long = bac(&depot, &kraken, "main-1", &depart).unwrap();
    // Un fichier dont le chemin dépasse 260 caractères, la limite de git sous Windows sans `core.longpaths` (« Filename
    // too long ») ; ailleurs, aucune limite ne s'y applique, et le même test passe tel quel.
    let mut profond = long.join("cible");
    while profond.as_os_str().len() <= 300 {
        profond.push("d".repeat(40));
    }
    fs::create_dir_all(&profond).unwrap();
    fs::write(profond.join("objet.o"), "compilé").unwrap();
    assert_eq!(retirer(&depot, &long, Some(("lydite/main-1", &depart)), faire), Ok(()));
    assert!(!long.exists(), "le bac est parti avec ce qu'il ignorait");
    assert_eq!(git(&depot, &["branch", "--list", "lydite/main-1"]), "");
    let _ = fs::remove_dir_all(&depot);
    let _ = fs::remove_dir_all(&kraken);
}

#[test]
fn un_bac_qui_ne_se_pose_pas_est_refuse_en_7001() {
    let (nu, kraken) = (neuf("nu"), neuf("nu-lydite"));
    fs::create_dir_all(&nu).unwrap();
    let refus = bac(&nu, &kraken, "main-1", "HEAD").unwrap_err();
    assert_eq!(refus.code, Code::VagueBacImpossible);
    assert!(refus.preuve.starts_with("git worktree add"), "la preuve nomme la commande de git : {refus}");
    assert!(refus.preuve.contains(&format!("dans {} a échoué", nu.display())), "et son dossier : {refus}");
    assert!(refus.correction.contains("lydite/main-1"), "la correction nomme la branche : {refus}");
    let _ = fs::remove_dir_all(&nu);
}

#[test]
fn le_dossier_temporaire_est_le_debut_du_sha256_du_nom() {
    // Le voisin du bac, les 12 premiers chiffres hexadécimaux du SHA-256 de son nom suivis de `.tmp`, quel que soit
    // le dossier parent ; les vecteurs de SHA-256 de « a » et de « foobar » le figent.
    let bacs = Path::new("k").join("bacs");
    assert_eq!(temporaire_de(&bacs.join("foobar")), Some(bacs.join("c3ab8ff13720.tmp")));
    assert_eq!(temporaire_de(&bacs.join("a")), Some(bacs.join("ca978112ca1b.tmp")));
    let nom = "main-1794357585769_1201474_10-2";
    let complet = hex(&sha256(nom.as_bytes()));
    assert_eq!(temporaire_de(&bacs.join(nom)), Some(bacs.join(format!("{}.tmp", &complet[..12]))));
}

#[test]
fn deux_depots_partagent_le_dossier_sans_se_heurter() {
    let (a, b, kraken) = (depot("a"), depot("b"), neuf("partage"));
    // Deux lancements tirés coup sur coup (souvent dans la même ms) vers deux dépôts, un seul dossier de Kraken.
    let (id_a, id_b) = (format!("main-{}-1", lot()), format!("main-{}-1", lot()));
    let (bac_a, bac_b) = (bac(&a, &kraken, &id_a, "HEAD").unwrap(), bac(&b, &kraken, &id_b, "HEAD").unwrap());
    assert_ne!(bac_a, bac_b);
    assert_eq!(git(&bac_a, &["branch", "--show-current"]), format!("lydite/{id_a}"));
    assert_eq!(git(&bac_b, &["branch", "--show-current"]), format!("lydite/{id_b}"));
    for dir in [a, b, kraken] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn une_erreur_de_git_nomme_son_programme_et_son_depot() {
    // Un dépôt absent : git ne part pas. Le refus 7001 dit quel programme, dans quel dépôt.
    let (depot, kraken) = (neuf("absent"), neuf("absent-lydite"));
    let refus = bac(&depot, &kraken, "main-1", "HEAD").unwrap_err();
    assert_eq!(refus.code, Code::VagueBacImpossible);
    assert!(refus.preuve.contains("git"), "le programme : {refus}");
    assert!(refus.preuve.contains(&depot.display().to_string()), "le dépôt : {refus}");
}

#[test]
fn les_reglages_du_bac_sont_empreints() {
    let bac = kraken("reglages-bac");
    let claude_dir = bac.join(".claude");
    // Aucun : la chaîne vide.
    assert_eq!(reglages_du_bac(&bac).unwrap(), "");
    fs::create_dir(&claude_dir).unwrap();
    assert_eq!(reglages_du_bac(&bac).unwrap(), "");
    // Un : son chemin dans le bac, puis le SHA-256 de ses octets. printf '{}' | sha256sum
    fs::write(claude_dir.join("settings.local.json"), "{}").unwrap();
    let local = ".claude/settings.local.json 44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a";
    assert_eq!(reglages_du_bac(&bac).unwrap(), local);
    // Deux : settings.json d'abord, un par ligne.
    fs::write(claude_dir.join("settings.json"), r#"{"model":"opus"}"#).unwrap();
    let partage = format!(".claude/settings.json {}", hex(&sha256(br#"{"model":"opus"}"#)));
    assert_eq!(reglages_du_bac(&bac).unwrap(), format!("{partage}\n{local}"));
    // Un fichier présent qui ne se lit pas (un dossier à sa place) : une erreur qui le nomme et garde son kind().
    fs::remove_file(claude_dir.join("settings.json")).unwrap();
    fs::create_dir(claude_dir.join("settings.json")).unwrap();
    let erreur = reglages_du_bac(&bac).unwrap_err();
    assert_eq!(erreur.kind(), fs::read(claude_dir.join("settings.json")).unwrap_err().kind());
    assert!(erreur.to_string().contains(&claude_dir.join("settings.json").display().to_string()), "{erreur}");
    let _ = fs::remove_dir_all(&bac);
}

#[test]
fn des_reglages_qui_ne_sont_pas_la_ne_sont_pas_une_erreur() {
    // Un `.claude` qui est un fichier, un bac qui n'existe pas : aucun fichier de réglages n'y est. Seul un fichier
    // présent qui ne se lit pas est une erreur.
    let bac = kraken("reglages-absents");
    fs::write(bac.join(".claude"), "x").unwrap();
    assert_eq!(reglages_du_bac(&bac).unwrap(), "");
    assert_eq!(reglages_du_bac(&bac.join("absent")).unwrap(), "");
    let _ = fs::remove_dir_all(&bac);
}
