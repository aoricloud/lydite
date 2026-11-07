use super::*;
use crate::branchements::bac::temporaire_de;
use crate::branchements::processus::{retire_de_l_env, retire_de_la_preuve};
use crate::essais::mains::{DELAI, bac_jetable, profil_borne};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;

#[test]
fn l_environnement_de_session_est_retire() {
    // Les marques de session, les jetons, les moyens de s'authentifier comme l'humain et les variables de git qui
    // nomment un dépôt, ses objets, sa configuration ou son programme partent de la main et de sa preuve, quelle que
    // soit la casse ; les fichiers de configuration de git de l'humain, `MCP_*` et le reste passent aux deux.
    let retires = [
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_MESSAGING_SOCKET",
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "CLAUDE_EFFORT",
        "CLAUDE",
        "claude_code_entrypoint",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "gh_token",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
        "github_enterprise_token",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_NAMESPACE",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_EXEC_PATH",
        "GIT_SSH_COMMAND",
        "GIT_SSH",
        "GIT_ASKPASS",
        "SSH_ASKPASS",
        "SSH_AUTH_SOCK",
        "git_dir",
        "ssh_auth_sock",
    ];
    for nom in retires {
        assert!(retire_de_l_env(nom), "{nom} part de la main");
        assert!(retire_de_la_preuve(nom), "{nom} part de la preuve");
    }
    let gardes = [
        "MCP_TIMEOUT",
        "PATH",
        "HOME",
        "XCLAUDE",
        "GH_HOST",
        "GITHUB_ACTIONS",
        "GIT_AUTHOR_NAME",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_KEY_0",
        "GIT_CONFIG_VALUE_0",
    ];
    for nom in gardes {
        assert!(!retire_de_l_env(nom), "{nom} passe à la main");
        assert!(!retire_de_la_preuve(nom), "{nom} passe à la preuve");
    }
    // La main et sa preuve listent ce qu'elles reçoivent : rien de ce que leur règle retire de l'environnement de ce
    // test, et tout le reste.
    let (dir, bac) = bac_jetable("env");
    let liste: Vec<String> =
        if cfg!(windows) { vec!["cmd".into(), "/C".into(), "set".into()] } else { vec!["env".into()] };
    let main = jouer(&liste, &bac, "", DELAI, None, false, None).1.unwrap();
    let profil = Profil::faux(vec!["x".into()], DELAI);
    let commande = if cfg!(windows) { "set > env.txt" } else { "env > env.txt" };
    rejouer(&profil, commande, &bac, &dir.join("journal.tmp")).1.unwrap();
    let preuve = String::from_utf8_lossy(&fs::read(bac.join("env.txt")).unwrap()).into_owned();
    let retire_de_la_main: fn(&str) -> bool = retire_de_l_env;
    for (qui, vu, retire) in [("la main", main, retire_de_la_main), ("la preuve", preuve, retire_de_la_preuve)] {
        let noms: Vec<String> =
            vu.lines().filter_map(|ligne| ligne.split_once('=')).map(|(nom, _)| nom.to_ascii_uppercase()).collect();
        // Les variables cachées de Windows, `=C:` et ses pareilles, ne se listent pas.
        let lisibles = env::vars_os().map(|(nom, _)| nom.to_string_lossy().to_ascii_uppercase());
        for nom in lisibles.filter(|nom| !nom.starts_with('=')) {
            assert_eq!(noms.contains(&nom), !retire(&nom), "{qui} et {nom}");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_main_d_un_fournisseur_recoit_son_url_et_sa_cle_et_perd_le_jeton_herite() {
    // Le test ne pose rien dans son processus : il compare ce que la main voit à ce qu'il a. Sans fournisseur, les
    // `ANTHROPIC_*` passent tels quels. Avec, l'URL de l'API et la clé remplacent `ANTHROPIC_BASE_URL` et
    // `ANTHROPIC_API_KEY`, un `ANTHROPIC_AUTH_TOKEN` hérité est retiré, et le reste de l'environnement ne change
    // pas.
    let (dir, bac) = bac_jetable("fournisseur");
    let liste: Vec<String> =
        if cfg!(windows) { vec!["cmd".into(), "/C".into(), "set".into()] } else { vec!["env".into()] };
    let vues = |fournisseur| -> BTreeMap<String, String> {
        let sortie = jouer(&liste, &bac, "", DELAI, None, false, fournisseur).1.unwrap();
        let lignes = sortie.lines().filter_map(|ligne| ligne.split_once('='));
        lignes.map(|(nom, valeur)| (nom.to_ascii_uppercase(), valeur.to_string())).collect()
    };
    let connexion = ["ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];
    let sans = vues(None);
    for nom in connexion {
        assert_eq!(sans.get(nom).map(String::as_str), env::var(nom).ok().as_deref(), "{nom} passe tel quel");
    }
    let (api, cle) = ("https://api.exemple.test/anthropic", "leurre-de-la-cle");
    let avec = vues(Some((api, cle, "CLE_ESSAI")));
    assert_eq!(avec["ANTHROPIC_BASE_URL"], api);
    assert_eq!(avec["ANTHROPIC_API_KEY"], cle);
    assert!(!avec.contains_key("ANTHROPIC_AUTH_TOKEN"), "le jeton hérité de l'humain ne va pas chez le fournisseur");
    let sans_connexion = |mut variables: BTreeMap<String, String>| {
        connexion.iter().for_each(|nom| drop(variables.remove(*nom)));
        variables
    };
    assert_eq!(sans_connexion(avec), sans_connexion(sans), "le reste de l'environnement est celui de la main");
    // La preuve que Kraken rejoue, pour le même profil, ne reçoit ni l'URL ni la clé.
    let texte = "sorte = claude\napi = https://api.exemple.test/anthropic\ncle = CLE_ESSAI\n";
    let mut profil = profil_borne(texte, &dir);
    profil.fournir(|_| Some(OsString::from(cle))).unwrap();
    assert_eq!(profil.fournisseur(), Some((api, cle)));
    let commande = if cfg!(windows) { "set > env.txt" } else { "env > env.txt" };
    rejouer(&profil, commande, &bac, &dir.join("journal.tmp")).1.unwrap();
    let preuve = String::from_utf8_lossy(&fs::read(bac.join("env.txt")).unwrap()).into_owned();
    let noms = preuve.lines().filter_map(|ligne| ligne.split_once('=')).map(|(nom, _)| nom.to_ascii_uppercase());
    assert!(!noms.into_iter().any(|nom| nom.starts_with("ANTHROPIC_")), "{preuve}");
    assert!(!preuve.contains(cle) && !preuve.contains(api), "{preuve}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tenter_ne_rend_de_constat_que_d_une_main_declaree() {
    let (dir, bac) = bac_jetable("tenter");
    let journal = dir.join("journal.tmp");
    // Une main Claude que `borner` n'a pas bornée ne part pas, et ne déclare rien : pas de constat, et rien n'est
    // créé à côté de son bac.
    let libre = crate::coeur::profil::lire("essai", "sorte = claude\n", None, None).unwrap();
    let (constat, _, verdict) = tenter(&libre, &bac, "objectif", None, None, &journal, None);
    assert_eq!(constat, None);
    assert!(verdict.unwrap_err().starts_with("main : aucun programme à lancer"));
    assert!(!temporaire_de(&bac).unwrap().exists());
    // Une main brute ne déclare rien : l'erreur de son départ, sans constat.
    let brute = Profil::faux(vec!["lydite-main-introuvable".into()], DELAI);
    let (constat, verif, verdict) = tenter(&brute, &bac, "objectif", Some("exit 0"), None, &journal, None);
    assert_eq!((constat, verif), (None, None));
    assert!(verdict.unwrap_err().starts_with("lancement impossible de lydite-main-introuvable"));
    // Une main Claude déclare : Kraken rend le constat de ce qu'il en a lu, ici rien, et l'erreur de son départ. Un
    // constat vide est un constat : la vague le note, car la main était déclarée.
    let claude = profil_borne("sorte = claude\nprogramme = lydite-claude-introuvable\n", &dir);
    let (constat, verif, verdict) = tenter(&claude, &bac, "objectif", Some("exit 0"), Some("échec"), &journal, None);
    assert_eq!((constat, verif), (Some(Constat::default()), None));
    assert!(verdict.unwrap_err().starts_with("lancement impossible de lydite-claude-introuvable"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_consigne_garde_ses_octets() {
    // Sans vérification, la main joue elle-même sa preuve et finit sur PREUVE.
    assert_eq!(
        consigne("Écrire bonjour.txt", None, None, false),
        "Écrire bonjour.txt\n\nQuand tu as fini, joue toi-même le test qui prouve ton travail. La dernière ligne \
         de ta réponse est exactement PREUVE: vert si le test passe, sinon PREUVE: rouge."
    );
    // Avec une vérification que la main ne peut pas jouer, Kraken la joue : la consigne la nomme.
    assert_eq!(
        consigne("Écrire bonjour.txt", Some("cargo test"), None, false),
        "Écrire bonjour.txt\n\nQuand tu auras fini, Lydite jouera dans ton dossier : cargo test. \
         Ton travail est pris si elle réussit."
    );
    // À la relance, l'erreur de la tentative d'avant s'intercale avant la preuve.
    assert_eq!(
        consigne("Écrire bonjour.txt", None, Some("pas de PREUVE\nen dernière ligne"), false),
        "Écrire bonjour.txt\n\nTentative précédente échouée :\npas de PREUVE\nen dernière ligne\n\n\
         Quand tu as fini, joue toi-même le test qui prouve ton travail. La dernière ligne \
         de ta réponse est exactement PREUVE: vert si le test passe, sinon PREUVE: rouge."
    );
    assert_eq!(
        consigne("Écrire bonjour.txt", Some("cargo test"), Some("vérification rouge"), false),
        "Écrire bonjour.txt\n\nTentative précédente échouée :\nvérification rouge\n\n\
         Quand tu auras fini, Lydite jouera dans ton dossier : cargo test. \
         Ton travail est pris si elle réussit."
    );
    // Une vérification prêtée à la main : elle la joue elle-même, chaque morceau seul et tel quel, jusqu'à ce
    // qu'elle passe, puis Kraken la rejoue.
    assert_eq!(
        consigne("Écrire bonjour.txt", Some("cargo test"), None, true),
        "Écrire bonjour.txt\n\nQuand tu auras fini, joue toi-même dans ton dossier : cargo test. Lance chaque \
         morceau seul et tel qu'il est écrit, dans l'ordre (les morceaux sont ceux que joint ` && ` ; une \
         commande sans ` && ` en a un seul), sans rien y ajouter avant ni après, ni `;`, ni `|`, ni \
         redirection : c'est seul ainsi qu'un morceau t'est permis. Reprends ton \
         travail jusqu'à ce qu'elle réussisse : Lydite la rejouera ensuite, et ton travail est pris si elle \
         réussit."
    );
    assert_eq!(
        consigne("Écrire bonjour.txt", Some("cargo test"), Some("vérification rouge"), true),
        "Écrire bonjour.txt\n\nTentative précédente échouée :\nvérification rouge\n\n\
         Quand tu auras fini, joue toi-même dans ton dossier : cargo test. Lance chaque \
         morceau seul et tel qu'il est écrit, dans l'ordre (les morceaux sont ceux que joint ` && ` ; une \
         commande sans ` && ` en a un seul), sans rien y ajouter avant ni après, ni `;`, ni `|`, ni \
         redirection : c'est seul ainsi qu'un morceau t'est permis. Reprends ton \
         travail jusqu'à ce qu'elle réussisse : Lydite la rejouera ensuite, et ton travail est pris si elle \
         réussit."
    );
    // Une preuve à deux morceaux s'écrit entière, telle que l'humain l'a donnée, et la même phrase la suit.
    assert_eq!(
        consigne("Écrire bonjour.txt", Some("cargo fmt --check && cargo test"), None, true),
        "Écrire bonjour.txt\n\nQuand tu auras fini, joue toi-même dans ton dossier : cargo fmt --check && \
         cargo test. Lance chaque morceau seul et tel qu'il est écrit, dans l'ordre (les morceaux sont ceux que \
         joint ` && ` ; une commande sans ` && ` en a un seul), sans rien y ajouter avant ni après, ni `;`, ni \
         `|`, ni redirection : c'est seul ainsi qu'un morceau t'est permis. Reprends ton travail jusqu'à ce \
         qu'elle réussisse : Lydite la rejouera ensuite, et ton travail est pris si elle réussit."
    );
}
