//! Le contrat de la ligne de commande, joué sur le binaire lui-même.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Un argument qui n'est pas de l'UTF-8 : un substitut seul sous Windows, l'octet 0xFF ailleurs.
fn non_utf8() -> OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xD800])
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xFF])
    }
}

/// Le binaire, dont le dossier personnel et `CLAUDE_CONFIG_DIR` vont à des leurres sous le dossier temporaire, jamais
/// créés, d'où Kraken tire les dossiers de connexion de Claude : posés sur l'enfant seul, ils ne touchent pas le
/// processus du test, et le vrai dossier personnel de l'humain n'est jamais lu. Un test qui veut un autre `USERPROFILE`
/// le pose après.
fn kraken() -> Command {
    let leurres = std::env::temp_dir().join(format!("lydite-contrat-leurres-{}", std::process::id()));
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande
        .env("USERPROFILE", leurres.join("personnel"))
        .env("HOME", leurres.join("personnel"))
        .env("CLAUDE_CONFIG_DIR", leurres.join("config"))
        .env_remove("GH_CONFIG_DIR")
        .env_remove("XDG_CONFIG_HOME");
    commande
}

/// Un refus lu sur stderr : sa première ligne, `lydite : <numéro> <nom>`, puis la règle, la preuve et la correction,
/// sans leur étiquette.
#[derive(Debug)]
struct Refus {
    code: String,
    regle: String,
    preuve: String,
    correction: String,
}

/// Joue la commande. Elle sort en 2, sans panique et sans rien sur stdout, avec un refus de quatre lignes sur stderr.
/// Rend ces quatre lignes.
fn lire_refus(commande: &mut Command) -> Refus {
    let sortie = commande.output().unwrap();
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(2), "{commande:?} : {erreur}");
    assert!(!erreur.contains("panicked"), "{commande:?} : {erreur}");
    assert!(sortie.stdout.is_empty(), "{commande:?} écrit sur stdout : {}", String::from_utf8_lossy(&sortie.stdout));
    let lignes: Vec<&str> = erreur.lines().collect();
    assert_eq!(lignes.len(), 4, "{commande:?} : {erreur}");
    let etiquettes = ["  rule: ", "  seen: ", "  fix: "];
    for (ligne, debut) in lignes[1..].iter().zip(etiquettes) {
        assert!(ligne.starts_with(debut) && ligne.len() > debut.len(), "{commande:?} : {erreur}");
    }
    let [regle, preuve, correction] = [0, 1, 2].map(|i| lignes[i + 1][etiquettes[i].len()..].to_string());
    Refus { code: lignes[0].to_string(), regle, preuve, correction }
}

/// Comme `lire_refus`, mais ne rend que la première ligne, `lydite : <numéro> <nom>`.
fn refus(commande: &mut Command) -> String {
    lire_refus(commande).code
}

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lydite-contrat-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Joue git dans ce dossier, avec une identité de passage : le dépôt jetable n'a pas de configuration.
fn git(dir: &Path, args: &[&str]) {
    let identite = ["-c", "user.name=t", "-c", "user.email=t@t"];
    let sortie = Command::new("git").current_dir(dir).args(identite).args(args).output().unwrap();
    assert!(sortie.status.success(), "git {args:?} : {}", String::from_utf8_lossy(&sortie.stderr));
}

/// Un dépôt git neuf, vide et sans commit, dans le dossier temporaire du système : assez pour que git le connaisse.
fn depot_git(nom: &str) -> PathBuf {
    let dir = neuf(nom);
    git(&dir, &["init", "-q"]);
    dir
}

/// Un dépôt git neuf avec un commit, où un bac se pose.
fn depot_commite(nom: &str) -> PathBuf {
    let dir = depot_git(nom);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    dir
}

/// Les lignes du fil qui sont de ce type, dans l'ordre du fil.
fn lignes_de<'a>(fil: &'a str, genre: &str) -> Vec<&'a str> {
    let type_ = format!(r#""type":"{genre}""#);
    fil.lines().filter(|ligne| ligne.contains(&type_)).collect()
}

/// Les noms de ce que contient un dossier, triés.
fn noms(dir: &Path) -> Vec<String> {
    let mut noms: Vec<String> =
        fs::read_dir(dir).unwrap().map(|entree| entree.unwrap().file_name().to_string_lossy().into_owned()).collect();
    noms.sort();
    noms
}

/// La règle d'un refus est une phrase : non vide, finie par un point.
fn assert_regle(refus: &Refus) {
    let regle = refus.regle.trim();
    assert!(!regle.is_empty() && regle.ends_with('.'), "la règle est une phrase : {refus:?}");
}

/// La correction d'un refus de la ligne de commande donne l'usage, en mots anglais : les trois commandes de la liste
/// fermée, `launch` avec ses trois drapeaux.
fn assert_usage(refus: &Refus) {
    let lancer = r#"lydite launch <profile> "<objective>" [--n N] [--verify "<command>"] [--repo <path>]"#;
    for morceau in [lancer, "lydite accept <profile>", "lydite profile <profile>", "lydite status", "lydite wait"] {
        assert!(refus.correction.contains(morceau), "la correction donne l'usage ({morceau}) : {refus:?}");
    }
}

#[test]
fn un_argument_non_utf8_ne_panique_pas() {
    let mauvais = non_utf8();
    // Comme mot de commande, ce n'est pas une commande ; là où il faut un texte, la ligne est mal formée.
    assert_eq!(refus(kraken().arg(&mauvais)), "lydite : 1005 contrat.commande_inconnue");
    let mal_formee = "lydite : 1006 contrat.commande_mal_formee";
    assert_eq!(refus(kraken().arg("lancer").arg(&mauvais).arg("objectif")), mal_formee);
    assert_eq!(refus(kraken().args(["lancer", "claude"]).arg(&mauvais)), mal_formee);
    assert_eq!(refus(kraken().args(["lancer", "claude", "objectif", "--verifier"]).arg(&mauvais)), mal_formee);
}

/// `kraken`, prêt à lancer sans qu'aucune vraie main ne puisse partir ni rien de l'humain ne soit lu : le dossier de
/// Kraken donné (un dossier jetable), le dépôt visé pour dossier courant, un `claude` qui n'existe pas, et les leurres
/// de `kraken()` pour les dossiers de connexion de Claude.
fn kraken_dans(dossier: &Path, depot: &Path) -> Command {
    let mut commande = kraken();
    commande.current_dir(depot).env("LYDITE_DOSSIER", dossier).env("LYDITE_CLAUDE", "lydite-claude-introuvable");
    commande
}

#[test]
fn toute_sortie_en_2_est_un_refus_en_quatre_lignes() {
    // Pas de commande, une commande sans ses arguments : la ligne se refuse avant toute main.
    assert_eq!(refus(&mut kraken()), "lydite : 1006 contrat.commande_mal_formee");
    assert_eq!(refus(kraken().arg("lancer")), "lydite : 1006 contrat.commande_mal_formee");
    // `attendre` ne prend aucun argument, comme `etat` : le refus tombe avant toute attente.
    assert_eq!(refus(kraken().args(["attendre", "x"])), "lydite : 1006 contrat.commande_mal_formee");
    // Un profil inconnu : dans un dépôt et un dossier de Kraken jetables, car le dépôt se lit avant le profil, et le
    // profil dans le dossier de Kraken.
    let (depot, dossier) = (depot_git("quatre-depot"), neuf("quatre-lydite"));
    let inconnu = refus(kraken_dans(&dossier, &depot).args(["lancer", "inconnu", "x"]));
    assert_eq!(inconnu, "lydite : 6001 lancer.profil_inconnu");
    // `etat` aussi : un dossier de Kraken relatif est refusé avant de lire quoi que ce soit.
    assert_eq!(refus(kraken().arg("etat").env("LYDITE_DOSSIER", "relatif")), "lydite : 6004 lancer.dossier_relatif");
    for dir in [&depot, &dossier] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn sans_commande_le_refus_enseigne() {
    // Rien à lire : la ligne ne nomme aucune commande. C'est le refus 1006, avant tout dossier et toute main.
    let refus = lire_refus(&mut kraken());
    assert_eq!(refus.code, "lydite : 1006 contrat.commande_mal_formee");
    assert_regle(&refus);
    assert!(refus.preuve.contains("no command"), "la preuve dit ce qui a été vu : {refus:?}");
    assert_usage(&refus);
}

#[test]
fn une_commande_inconnue_est_refusee() {
    // Un mot hors de la liste fermée, quel qu'il soit, un drapeau, une casse fausse ou le mot vide compris : le refus
    // 1005, dont la preuve nomme le mot et dont la correction donne l'usage.
    for mot in ["bonjour", "--bonjour", "ETAT", "lancer2", ""] {
        let refus = lire_refus(kraken().arg(mot));
        assert_eq!(refus.code, "lydite : 1005 contrat.commande_inconnue", "{mot:?}");
        assert_regle(&refus);
        assert!(refus.preuve.contains(&format!("« {mot} »")), "la preuve nomme le mot {mot:?} : {refus:?}");
        assert_usage(&refus);
    }
}

#[test]
fn kraken_etat_lit_un_fil_demesure_sans_paniquer() {
    // Un fil écrit par un autre programme : des lignes v2 valides dont un `n` ou un `main` tient en u64, mais ferait
    // écrire à `lydite etat` une ligne par main, jusqu'à la panique (code 101) ou la mémoire pleine (mort brutale). Le
    // lecteur les refuse en 1009 : chacune se dit sur stderr, et la lecture va au bout.
    let dossier = neuf("demesure");
    let ligne = |seq: u64, corps: &str| {
        format!(
            concat!(
                r#"{{"v":2,"seance":"s","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","origine":"humain","origine"#,
                r#"_foi":"declare",{corps}}}"#,
            ),
            seq = seq,
            corps = corps,
        )
    };
    let ouverture = |seq: u64, n: &str| {
        let corps = format!(
            r#""type":"vague.ouverte","vague":"v","n":{n},"depot":"/srv/depot","verifier":null,"objectif":"o""#
        );
        ligne(seq, &corps)
    };
    let debut =
        |seq: u64, main: &str| ligne(seq, &format!(r#""type":"main.debut","vague":"v","main":{main},"tentative":1"#));
    let lignes = [
        ouverture(1, "18446744073709551615"),
        ouverture(2, "1000000000000"),
        debut(3, "18446744073709551615"),
        debut(4, "0"),
    ];
    fs::write(dossier.join("fil.jsonl"), lignes.join("\n") + "\n").unwrap();
    let sortie = kraken().arg("etat").env("LYDITE_DOSSIER", &dossier).output().unwrap();
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    // Aucune vague lisible : rien n'est affirmé, le code 0.
    assert_eq!(sortie.status.code(), Some(0), "{erreur}");
    assert_eq!(String::from_utf8_lossy(&sortie.stdout).trim_end(), "no wave");
    // Quatre lignes refusées, chacune en un refus de quatre lignes.
    let lignes_d_erreur: Vec<&str> = erreur.lines().collect();
    assert_eq!(lignes_d_erreur.len(), 16, "{erreur}");
    for refus in lignes_d_erreur.chunks(4) {
        assert_eq!(refus[0], "lydite : 1009 contrat.evenement_mal_forme", "{erreur}");
        assert!(refus[2].contains("hors de son domaine"), "{erreur}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_profil_inconnu_est_refuse_avant_tout_dossier() {
    // La ligne est entière, avec tous ses drapeaux : seul le profil manque. Le dossier courant est neuf et vide ; le
    // dépôt visé est un `git init` neuf, car git doit le connaître avant que le profil se lise ; le dossier de Kraken,
    // que l'enfant reçoit seul, est neuf et vide : on voit que rien n'y est posé.
    let (courant, depot, dossier) = (neuf("profil-courant"), depot_git("profil-depot"), neuf("profil-lydite"));
    let refus = lire_refus(
        kraken()
            .current_dir(&courant)
            .env("LYDITE_DOSSIER", &dossier)
            .args(["lancer", "inconnu", "objectif", "--n", "3", "--verifier", "exit 0", "--depot"])
            .arg(&depot),
    );
    assert_eq!(refus.code, "lydite : 6001 lancer.profil_inconnu");
    assert_regle(&refus);
    assert!(refus.preuve.contains("\"inconnu\""), "la preuve nomme le profil : {refus:?}");
    assert!(refus.correction.contains("claude"), "la correction nomme les profils connus : {refus:?}");
    // Avant tout dossier : ni bac, ni fil, ni rien d'autre, ni dans le dépôt visé (que son `.git`), ni dans le dossier
    // courant, ni dans le dossier de Kraken.
    assert_eq!(noms(&depot), [".git"]);
    for dir in [&courant, &dossier] {
        assert_eq!(noms(dir), Vec::<String>::new(), "{} n'est plus vide", dir.display());
    }
    for dir in [&courant, &depot, &dossier] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn le_dossier_et_le_depot_se_lisent_avant_le_profil() {
    // Le dossier de Kraken d'abord : relatif (6004), il l'emporte sur un dépôt absent et sur un profil inconnu.
    let absent = std::env::temp_dir().join(format!("lydite-contrat-absent-{}", std::process::id()));
    let _ = fs::remove_dir_all(&absent);
    let ligne = ["lancer", "inconnu", "objectif", "--depot"];
    let relatif = refus(kraken().args(ligne).arg(&absent).env("LYDITE_DOSSIER", "relatif"));
    assert_eq!(relatif, "lydite : 6004 lancer.dossier_relatif");
    // Puis le dépôt, avant le profil : absent (6002), puis hors git (6003) ; le profil inconnu n'est pas lu.
    let (nu, dossier) = (neuf("ordre-nu"), neuf("ordre-lydite"));
    let refus_absent = lire_refus(kraken().args(ligne).arg(&absent).env("LYDITE_DOSSIER", &dossier));
    assert_eq!(refus_absent.code, "lydite : 6002 lancer.depot_introuvable");
    assert!(refus_absent.preuve.contains(&absent.display().to_string()), "{refus_absent:?}");
    let refus_nu = lire_refus(kraken().args(ligne).arg(&nu).env("LYDITE_DOSSIER", &dossier));
    assert_eq!(refus_nu.code, "lydite : 6003 lancer.bac_impossible");
    // Rien n'est posé : ni dans le dépôt hors git, ni dans le dossier de Kraken, ni à la place du dépôt absent.
    assert_eq!(noms(&nu), Vec::<String>::new());
    assert_eq!(noms(&dossier), Vec::<String>::new());
    assert!(!absent.exists());
    for dir in [&nu, &dossier] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn un_drapeau_a_la_place_du_profil_ou_de_l_objectif_est_refuse() {
    // Sans ce refus, `lancer claude --verifier x` lancerait une main sur l'objectif « --verifier ». Le dépôt et le
    // dossier de Kraken sont jetables, et le `claude` n'existe pas : si le refus manquait, aucune vraie main ne
    // partirait.
    let (depot, dossier) = (depot_commite("drapeau-depot"), neuf("drapeau-lydite"));
    let cas: [(&[&str], &str); 4] = [
        (&["lancer", "claude", "--verifier", "x"], "the objective is missing"),
        (&["lancer", "claude", "--n", "3"], "the objective is missing"),
        (&["lancer", "--n", "3", "obj"], "the profile and the objective are missing"),
        (&["lancer", "--depot", "d", "--n", "3"], "the profile and the objective are missing"),
    ];
    for (mots, manque) in cas {
        let refus = lire_refus(kraken_dans(&dossier, &depot).args(mots));
        assert_eq!(refus.code, "lydite : 1006 contrat.commande_mal_formee", "{mots:?}");
        assert_regle(&refus);
        assert!(refus.preuve.contains(manque), "la preuve dit ce qui manque ({manque}) : {refus:?}");
        assert_usage(&refus);
    }
    assert_eq!(noms(&dossier), Vec::<String>::new(), "aucun dossier, aucun bac, aucun fil");
    assert_eq!(noms(&depot), [".git"]);
    for dir in [&depot, &dossier] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn une_preuve_blanche_est_refusee_avant_tout_dossier() {
    // Rejouée, une commande blanche réussirait toujours : toute main serait prouvée.
    let (depot, dossier) = (depot_commite("blanche-depot"), neuf("blanche-lydite"));
    for blanche in ["", "   ", "\t"] {
        let refus =
            lire_refus(kraken_dans(&dossier, &depot).args(["lancer", "claude", "objectif", "--verifier", blanche]));
        assert_eq!(refus.code, "lydite : 6014 lancer.preuve_vide", "{blanche:?}");
        assert_regle(&refus);
        assert!(refus.preuve.contains("--verify") && refus.preuve.contains("blank"), "{refus:?}");
        assert!(refus.correction.contains("--verify"), "{refus:?}");
    }
    assert_eq!(noms(&dossier), Vec::<String>::new(), "aucun dossier, aucun bac, aucun fil");
    assert_eq!(noms(&depot), [".git"]);
    for dir in [&depot, &dossier] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn le_dossier_de_kraken_est_hors_du_depot_et_s_ecrit_dans_une_regle() {
    let depot = depot_commite("regle-depot");
    // Sous le dépôt visé : le refus 6015, qui nomme le dossier. Rien n'est posé.
    let sous = depot.join("lydite");
    let ligne = ["lancer", "claude", "objectif"];
    let refus = lire_refus(kraken_dans(&sous, &depot).args(ligne));
    assert_eq!(refus.code, "lydite : 6015 lancer.dossier_sous_le_depot");
    assert_regle(&refus);
    assert!(refus.preuve.contains(&sous.display().to_string()), "la preuve nomme le dossier : {refus:?}");
    assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus:?}");
    assert_eq!(noms(&depot), [".git"]);
    // Hors du dépôt, mais à un chemin qu'une règle de permission ne sait pas écrire (une parenthèse) : le refus 6023,
    // qui nomme le chemin, avant tout bac.
    let parent = neuf("regle-parent");
    let rare = parent.join("k(1)");
    let refus = lire_refus(kraken_dans(&rare, &depot).args(ligne));
    assert_eq!(refus.code, "lydite : 6023 lancer.dossier_chemin_refuse");
    assert_regle(&refus);
    assert!(refus.preuve.contains("k(1)"), "la preuve nomme le chemin : {refus:?}");
    assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus:?}");
    assert_eq!(noms(&parent), Vec::<String>::new(), "rien n'est posé");
    for dir in [&depot, &parent] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn accepter_refuse_ce_qui_n_a_pas_de_fichier() {
    let (depot, dossier) = (depot_git("sans-fichier-depot"), neuf("sans-fichier-lydite"));
    let accepter = |nom: &str| lire_refus(kraken_dans(&dossier, &depot).args(["accepter", nom]));
    // Le profil intégré n'a pas d'empreinte à garder (6013) ; un nom sans fichier ni profil intégré est inconnu (6001)
    // ; un nom qui sortirait de `profils` est refusé (6007).
    let integre = accepter("claude");
    assert_eq!(integre.code, "lydite : 6013 lancer.profil_sans_fichier");
    assert_regle(&integre);
    assert!(integre.preuve.contains("claude.profil"), "la preuve nomme le fichier attendu : {integre:?}");
    assert_eq!(accepter("hermes").code, "lydite : 6001 lancer.profil_inconnu");
    assert_eq!(accepter("../x").code, "lydite : 6007 lancer.profil_nom_refuse");
    // Une ligne mal formée : un nom, rien d'autre ; un dossier de Kraken relatif est refusé avant de lire quoi que ce
    // soit.
    let mal_formee = "lydite : 1006 contrat.commande_mal_formee";
    assert_eq!(lire_refus(kraken().arg("accepter")).code, mal_formee);
    assert_usage(&lire_refus(kraken().args(["accepter", "a", "b"])));
    let relatif = refus(kraken().args(["accepter", "claude"]).env("LYDITE_DOSSIER", "relatif"));
    assert_eq!(relatif, "lydite : 6004 lancer.dossier_relatif");
    // Rien n'est créé : accepter ne pose que l'empreinte d'un fichier qui existe.
    assert_eq!(noms(&dossier), Vec::<String>::new());
    for dir in [&depot, &dossier] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn accepter_garde_l_empreinte_que_lancer_exige_et_note_au_fil() {
    let (depot, dossier) = (depot_commite("accepter-depot"), neuf("accepter-lydite"));
    let profils = dossier.join("profils");
    fs::create_dir(&profils).unwrap();
    // Un profil dont le programme n'existe pas : la main se déclare et ne part pas. Son empreinte est le SHA-256 de ses
    // octets (printf '…' | sha256sum), avec ou sans la ligne vide ajoutée plus bas.
    let texte =
        "# la main de veille\nsorte = claude\nprogramme = lydite-claude-introuvable\nmodele = haiku\nmur = demande\n";
    let (empreinte, autre) = (
        "b124a44f5b06b4956bf6be45f6912d818a4cce4cf0a050d68aecc06e35841964",
        "7115b63925d912523588bee3c181ef064f8150d4ae023cb9d2d51b81f4418d42",
    );
    let (fichier, garde) = (profils.join("veille.profil"), profils.join("veille.empreinte"));
    fs::write(&fichier, texte).unwrap();
    let lancer = || kraken_dans(&dossier, &depot).args(["lancer", "veille", "objectif"]).output().unwrap();
    // Pas accepté : le refus 6011 dit comment, avant tout bac et tout fil.
    let refus = lire_refus(kraken_dans(&dossier, &depot).args(["lancer", "veille", "objectif"]));
    assert_eq!(refus.code, "lydite : 6011 lancer.profil_non_accepte");
    assert!(refus.correction.contains("lydite accepter veille"), "{refus:?}");
    assert_eq!(noms(&dossier), ["profils"]);
    // `lydite accepter veille` montre le texte et l'empreinte, et garde l'empreinte à côté du profil.
    let sortie = kraken_dans(&dossier, &depot).args(["accepter", "veille"]).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{sortie:?}");
    assert!(sortie.stderr.is_empty(), "{sortie:?}");
    let dit = String::from_utf8_lossy(&sortie.stdout);
    assert_eq!(dit, format!("{}\nfingerprint: {empreinte}\n", texte.trim_end()));
    assert_eq!(fs::read_to_string(&garde).unwrap(), empreinte);
    // Accepté, le profil est lancé : ses deux mains échouent à partir (le programme n'existe pas), et la vague le dit.
    let sortie = lancer();
    assert_eq!(sortie.status.code(), Some(1), "{sortie:?}");
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert!(erreur.contains("échec classé : main-"), "{erreur}");
    assert!(erreur.contains("cannot start lydite-claude-introuvable"), "{erreur}");
    // Le fil compte les lancements par profil et par empreinte : un par tentative, noté avant la main, avec son modèle.
    // Un constat suit chacun, vide, car rien n'a été lu d'une main qui n'est pas partie.
    let fil = fs::read_to_string(dossier.join("fil.jsonl")).unwrap();
    let lancements = lignes_de(&fil, "main.lancement");
    assert_eq!(lancements.len(), 2, "{fil}");
    for (ligne, modele) in lancements.iter().zip(["haiku", "sonnet"]) {
        for champ in [
            r#""profil":"veille""#.to_string(),
            format!(r#""empreinte":"{empreinte}""#),
            format!(r#""modele":"{modele}""#),
            r#""mur":"demande""#.to_string(),
            r#""foi":"declare""#.to_string(),
        ] {
            assert!(ligne.contains(&champ), "{champ} manque : {ligne}");
        }
    }
    let constats = lignes_de(&fil, "main.constat");
    assert_eq!(constats.len(), 2, "{fil}");
    assert!(constats.iter().all(|l| l.contains(r#""version":null,"mur_actif":null,"foi":"constate""#)), "{constats:?}");
    // `lydite etat` dit le mur de la dernière tentative : ce que le profil demandait, déclaré, car rien n'a été
    // constaté.
    let etat = kraken_dans(&dossier, &depot).arg("etat").output().unwrap();
    assert_eq!(etat.status.code(), Some(1), "une main classée donne 1 : {etat:?}");
    let etat = String::from_utf8_lossy(&etat.stdout);
    assert!(etat.contains("set aside: ") && etat.contains(" · wall requested, not required (declared)"), "{etat}");
    // Le profil change, d'une ligne vide : le refus 6012 dit l'empreinte acceptée et celle qu'il lit, et rien n'est
    // noté.
    fs::write(&fichier, format!("{texte}\n")).unwrap();
    let avant = fs::read(dossier.join("fil.jsonl")).unwrap();
    let refus = lire_refus(kraken_dans(&dossier, &depot).args(["lancer", "veille", "objectif"]));
    assert_eq!(refus.code, "lydite : 6012 lancer.profil_change");
    assert!(refus.preuve.contains(empreinte) && refus.preuve.contains(autre), "{refus:?}");
    assert!(refus.correction.contains("lydite accepter veille"), "{refus:?}");
    assert_eq!(fs::read(dossier.join("fil.jsonl")).unwrap(), avant, "le fil n'a pas bougé");
    // Réaccepté, il passe de nouveau, avec sa nouvelle empreinte.
    let sortie = kraken_dans(&dossier, &depot).args(["accepter", "veille"]).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{sortie:?}");
    assert_eq!(fs::read_to_string(&garde).unwrap(), autre);
    assert_eq!(lancer().status.code(), Some(1), "le profil réaccepté est lancé");
    for dir in [&depot, &dossier] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[cfg(windows)]
#[test]
fn sous_windows_le_dossier_est_userprofile_point_kraken() {
    // Le fil de M1 posé dans <USERPROFILE>\.lydite : `etat` le nomme, donc lit ce dossier-là.
    let profil = neuf("userprofile");
    fs::create_dir_all(profil.join(".lydite")).unwrap();
    fs::write(profil.join(".lydite").join("fil"), "").unwrap();
    let sortie = kraken().arg("etat").env_remove("LYDITE_DOSSIER").env("USERPROFILE", &profil).output().unwrap();
    let attendu = profil.join(".lydite").join("fil").display().to_string();
    assert!(String::from_utf8_lossy(&sortie.stdout).contains(&attendu), "{sortie:?}");
    let relatif = kraken().arg("etat").env_remove("LYDITE_DOSSIER").env("USERPROFILE", "relatif").output().unwrap();
    assert_eq!(relatif.status.code(), Some(2), "{relatif:?}");
    assert!(
        String::from_utf8_lossy(&relatif.stderr).starts_with("lydite : 6005 lancer.dossier_introuvable"),
        "{relatif:?}"
    );
    let _ = fs::remove_dir_all(&profil);
}

/// Écrit le plan `nom` dans `dir`, ses lignes finies par `\n`, et rend son chemin.
fn plan_dans(dir: &Path, nom: &str, lignes: &[&str]) -> PathBuf {
    let chemin = dir.join(nom);
    fs::write(&chemin, lignes.iter().map(|ligne| format!("{ligne}\n")).collect::<String>()).unwrap();
    chemin
}

#[test]
fn kraken_plan_dit_les_dependances_ajoutees_sans_rien_lancer() {
    let dir = neuf("plan-juste");
    // Un plan découpé, à ses clés : b change src/b.rs, que le dossier src/ de a couvre ; c ne partage rien.
    let chemin = plan_dans(
        &dir,
        "PLAN.lydite",
        &[
            "plan 1",
            "objectif : Trois fichiers",
            "preuve : cargo test",
            "",
            "tache a",
            "fichiers : src/",
            "modele : sonnet",
            "    Écrire src/a.rs.",
            "tache b",
            "fichiers : src/b.rs",
            "modele : haiku",
            "    Écrire src/b.rs.",
            "tache c",
            "fichiers : docs/",
            "modele : haiku",
            "    Écrire docs/c.md.",
        ],
    );
    // Lu à la main ou en plan découpé, par un chemin relatif au dossier courant ou absolu. `lydite plan` ne lit ni le
    // dossier de Kraken, ni un dépôt : un dossier relatif, que `lancer` refuserait en 6004, ne le gêne pas.
    for (drapeaux, fichier) in [(&[][..], Path::new("PLAN.lydite")), (&["--decoupe"][..], chemin.as_path())] {
        let sortie = kraken()
            .env("LYDITE_DOSSIER", "relatif")
            .current_dir(&dir)
            .arg("plan")
            .args(drapeaux)
            .arg(fichier)
            .output()
            .unwrap();
        let erreur = String::from_utf8_lossy(&sortie.stderr);
        assert_eq!(sortie.status.code(), Some(0), "{drapeaux:?} : {erreur}");
        assert!(erreur.is_empty(), "{erreur}");
        assert_eq!(String::from_utf8_lossy(&sortie.stdout), "b attend a : elles partagent src/b.rs\n");
    }
    // Rien n'est posé à côté du plan.
    assert_eq!(noms(&dir), ["PLAN.lydite"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn kraken_plan_refuse_un_plan_faux_par_son_refus() {
    let dir = neuf("plan-faux");
    let entete = ["plan 1", "objectif : o", "preuve : cargo test"];
    // Chaque plan faux : ses drapeaux, ses tâches, le code de son refus et un bout de sa preuve.
    let cas: [(&[&str], &[&str], &str, &str); 5] = [
        (
            &[],
            &["tache a", "fichiers : ../voisin/x", "    x"],
            "lydite : 6024 lancer.plan_chemin",
            "ligne 5 « fichiers : ../voisin/x » : le chemin « ../voisin/x » sort du dépôt par `..`",
        ),
        (&[], &["tache a", "fichiers : .git/hooks/", "    x"], "lydite : 6024 lancer.plan_chemin", "est sous `.git/`"),
        (
            &["--decoupe"],
            &["tache a", "modele : haiku", "    x"],
            "lydite : 6025 lancer.plan_borne",
            "la tâche « a » n'a pas de clé `fichiers`",
        ),
        (&["--decoupe"], &["tache a", "fichiers : src/", "    x"], "lydite : 6025 lancer.plan_borne", "`modele`"),
        (&[], &["tache a", "modele : Opus", "    x"], "lydite : 6018 lancer.plan_ligne", "le modèle « Opus »"),
    ];
    for (i, (drapeaux, taches, code, bout)) in cas.into_iter().enumerate() {
        let chemin = plan_dans(&dir, &format!("plan-{i}.txt"), &[&entete[..], taches].concat());
        let refus = lire_refus(kraken().arg("plan").args(drapeaux).arg(&chemin));
        assert_eq!(refus.code, code, "plan {i}");
        assert_regle(&refus);
        assert!(refus.preuve.contains(bout), "le plan {i} devait dire « {bout} » : {refus:?}");
    }
    // Une tâche sans `fichiers` ni `modele` : refusée avec `--decoupe`, lue sans, comme `--plan` la lit.
    let sans_cles = plan_dans(&dir, "sans-cles.txt", &[&entete[..], &["tache a", "    x"]].concat());
    assert_eq!(refus(kraken().args(["plan", "--decoupe"]).arg(&sans_cles)), "lydite : 6025 lancer.plan_borne");
    let sortie = kraken().arg("plan").arg(&sans_cles).output().unwrap();
    assert_eq!((sortie.status.code(), sortie.stdout.is_empty(), sortie.stderr.is_empty()), (Some(0), true, true));
    // Un fichier absent : 6006, qui nomme son chemin.
    let absent = dir.join("absent.txt");
    let refus_absent = lire_refus(kraken().arg("plan").arg(&absent));
    assert_eq!(refus_absent.code, "lydite : 6006 lancer.entree_sortie");
    assert!(refus_absent.preuve.contains(&absent.display().to_string()), "{refus_absent:?}");
    assert!(refus_absent.correction.contains("lydite plan"), "{refus_absent:?}");
    // Une ligne de commande fausse : 1006, avant toute lecture.
    for mots in [&["plan"][..], &["plan", "--decoupe"], &["plan", "a.txt", "b.txt"], &["plan", "--n", "1"]] {
        assert_eq!(refus(kraken().args(mots)), "lydite : 1006 contrat.commande_mal_formee", "{mots:?}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn kraken_plan_refuse_un_fichier_de_plus_de_1_mo() {
    let dir = neuf("plan-lourd");
    // Une tâche dont l'objectif est une ligne immense, qui mène le fichier à la taille voulue.
    let debut = "plan 1\nobjectif : o\npreuve : cargo test\ntache a\n    ";
    let de_taille = |taille: usize| format!("{debut}{}\n", "x".repeat(taille - debut.len() - 1));
    let pile = dir.join("pile.txt");
    fs::write(&pile, de_taille(1_000_000)).unwrap();
    let sortie = kraken().arg("plan").arg(&pile).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", String::from_utf8_lossy(&sortie.stderr));
    // Un octet de plus, ou trois fois plus : le refus 6025, sans lire le fichier au-delà de la borne.
    for taille in [1_000_001, 3_000_000] {
        let lourd = dir.join(format!("lourd-{taille}.txt"));
        fs::write(&lourd, de_taille(taille)).unwrap();
        let refus = lire_refus(kraken().arg("plan").arg(&lourd));
        assert_eq!(refus.code, "lydite : 6025 lancer.plan_borne");
        assert_eq!(refus.preuve, "le plan dépasse 1 Mo : plus de 1000000 octets");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn kraken_plan_ne_lit_pas_au_dela_de_1_mo() {
    // `/dev/zero` ne finit jamais : une lecture sans borne n'en reviendrait pas. Kraken en lit un octet de plus que 1
    // Mo, et refuse.
    let refus = lire_refus(kraken().args(["plan", "/dev/zero"]));
    assert_eq!(refus.code, "lydite : 6025 lancer.plan_borne");
    assert_eq!(refus.preuve, "le plan dépasse 1 Mo : plus de 1000000 octets");
}

/// Une leçon juste, nommée `nom`.
fn lecon_juste(nom: &str) -> String {
    format!(
        "---\nname: {nom}\ndescription: Une règle.\n---\n\n## What happened\nA.\n\n## Why\nB.\n\n\
         ## How it is checked\nC.\n\n## Pitfall\nD.\n\n## Files\n\n## Falls when\nE.\n"
    )
}

#[test]
fn kraken_lecons_sort_en_0_sans_rien_dire_ou_en_2_sur_le_refus_de_la_premiere_fausse() {
    let depot = neuf("lecons");
    let sans_rien_dire = |depot: &Path| {
        let sortie = kraken().arg("lecons").arg(depot).output().unwrap();
        assert_eq!(sortie.status.code(), Some(0), "{}", String::from_utf8_lossy(&sortie.stderr));
        assert!(sortie.stdout.is_empty() && sortie.stderr.is_empty(), "{sortie:?}");
    };
    // Sans `.agents/skills`, aucune leçon.
    sans_rien_dire(&depot);
    let skills = depot.join(".agents").join("skills");
    for nom in ["juste", "fausse"] {
        fs::create_dir_all(skills.join(nom)).unwrap();
    }
    fs::write(skills.join("juste").join("SKILL.md"), lecon_juste("juste")).unwrap();
    sans_rien_dire(&depot);
    // Une juste et une fausse (sans `## Why`) : le refus 6031, avec le fichier et la ligne.
    let sans_why = lecon_juste("fausse").replace("## Why\nB.\n\n", "");
    fs::write(skills.join("fausse").join("SKILL.md"), sans_why).unwrap();
    let refus = lire_refus(kraken().arg("lecons").arg(&depot));
    assert_eq!(refus.code, "lydite : 6031 lancer.lecon_mal_formee");
    let fichier = skills.join("fausse").join("SKILL.md");
    assert!(refus.preuve.starts_with(&format!("{} : line 9: ", fichier.display())), "{}", refus.preuve);
    assert!(refus.preuve.contains("`## Why` is missing"), "{}", refus.preuve);
    let _ = fs::remove_dir_all(&depot);
}
