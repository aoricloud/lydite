//! Les profils en fichier, joués sur le binaire lui-même : `lydite accepter` et `lydite lancer` dans un dépôt git
//! jetable, avec un dossier de Kraken jetable hors de lui.
//!
//! Les refus sortent en 2, chacun avec son code de contrat, avant tout bac et tout fil. Les lancements partent sur une
//! fausse `claude`, `fausse-claude.cmd` sous Windows et `fausse-claude.sh` ailleurs : le profil d'essai la nomme pour
//! `programme`, et c'est Kraken qui la lance, pas le test. Elle ne lit pas le JSON de Kraken. À côté d'elle, elle écrit
//! ses arguments (`args.txt`) et son environnement (`env.txt`), répond par le fichier `controle-N.jsonl` de sa
//! tentative, recopie son entrée jusqu'à la fin dans `entree.jsonl`, puis écrit l'ouverture et le résultat de
//! `fin-N.jsonl`. Sa tentative est la 2 si `marque` existe, la 1 sinon, et la première pose `marque` en partant, tuée
//! ou non : le résultat de la première est en erreur, celui de la seconde réussit. Si `changer-le-profil` existe, elle
//! ajoute une ligne de commentaire au profil `veille` en partant : un profil qui change pendant la vague.
//!
//! Aucun test ne pose de variable dans son propre processus : l'environnement va à l'enfant seul, par `Command::env`.
//! Aucun ne lance un vrai `claude` ou un vrai `gh`, et aucun ne cherche à sortir d'un bac.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// L'objectif de tous les essais : de l'ASCII, car la fausse main le recopie dans un fichier.
const OBJECTIF: &str = "Ecrire bonjour.txt";

/// La vérification des lancements qui réussissent : elle passe dans tout dépôt, et la main peut la jouer elle-même.
const VERIFIER: &str = "git status";

/// Le prêt de cette vérification : une règle pour Bash, une pour PowerShell.
const PRET: [&str; 2] = ["Bash(git status)", "PowerShell(git status)"];

/// Ce que ce prêt sort du bac à sable de Claude Code sous un mur demandé : son seul morceau, la vérification.
const EXCLUES: [&str; 1] = [VERIFIER];

/// La preuve que Kraken prête au découpeur de `--decouper`, d'un seul morceau.
const DECOUPE: &str = "lydite plan --decoupe PLAN.lydite";

/// Le réseau du profil d'essai : un seul domaine.
const RESEAU: [&str; 1] = ["exemple.test"];

/// L'API du fournisseur des essais, et son hôte, que le fil note : ni la fausse claude ni aucun test ne la joint.
const API: &str = "https://api.exemple.test/anthropic";
const HOTE: &str = "api.exemple.test";

/// La variable d'environnement qui tient la clé du fournisseur des essais, et cette clé : un leurre que Kraken ne doit
/// écrire nulle part, ni au fil, ni sur stderr, ni dans un refus.
const VARIABLE: &str = "LYDITE_ESSAI_CLE";
const LEURRE: &str = "leurre-de-la-cle-4f9a";

/// Ce que l'humain a chez lui du côté d'Anthropic : son jeton, sa clé et son URL. Sur Kraken seul, ils passent à la
/// main sans fournisseur, et un fournisseur les remplace ou les retire.
const HUMAIN: [(&str, &str); 3] = [
    ("ANTHROPIC_AUTH_TOKEN", "jeton-de-l-humain"),
    ("ANTHROPIC_API_KEY", "cle-de-l-humain"),
    ("ANTHROPIC_BASE_URL", "https://humain.exemple.test"),
];

/// Les sortants que le profil ferme par défaut, dans l'ordre du contrat, copiés à la main : un nom que le code
/// changerait de place ou de lettre ferait un écart.
const FERMES: [&str; 19] = [
    "CronCreate",
    "CronDelete",
    "CronList",
    "ScheduleWakeup",
    "RemoteTrigger",
    "SendMessage",
    "ListAgents",
    "PushNotification",
    "SendUserFile",
    "Artifact",
    "ArtifactComments",
    "ArtifactData",
    "ArtifactCheck",
    "DesignSync",
    "SendToProject",
    "SendToProjectContext",
    "ShareOnboardingGuide",
    "AskUserQuestion",
    "WebSearch",
];

/// Les quatre demandes de contrôle que Kraken écrit d'abord à la main, une ligne chacune, l'id valant le sous-type.
const DEMANDES: [&str; 4] = [
    r#"{"type":"control_request","request_id":"get_settings","request":{"subtype":"get_settings"}}"#,
    r#"{"type":"control_request","request_id":"list_permission_rules","request":{"subtype":"list_permission_rules"}}"#,
    r#"{"type":"control_request","request_id":"get_hooks_listing","request":{"subtype":"get_hooks_listing"}}"#,
    r#"{"type":"control_request","request_id":"get_sandbox_dialog","request":{"subtype":"get_sandbox_dialog"}}"#,
];

// Les lignes de Claude Code relevées le 1/11, réduites aux champs que Kraken lit. Un `<mot>` est ce que l'essai change.
const GET_SETTINGS: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_settings","response":{"applied":"#,
    r#"{"model":"<modele>"},"effective":{"disableAllHooks":true,"autoMemoryEnabled":false,"sandbox":{"enabled":true,"#,
    r#""failIfUnavailable":<exige>,"allowUnsandboxedCommands":false,<exclues>"network":{"allowedDomains":["exemple.t"#,
    r#"est"],"strictAllowlist":true}}},"sources":[{"source":"userSettings","settings":{}},{"source":"flagSettings",""#,
    r#"settings":{}}]}}}"#,
);
/// Le `get_settings` du `--settings` tombé du 1/11 : une valeur mal typée le fait ignorer, sans `flagSettings` parmi
/// les sources, et Claude Code dit pourquoi dans `errors`.
const GET_SETTINGS_TOMBE: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_settings","response":{"applied":"#,
    r#"{"model":"<modele>"},"effective":{"sandbox":{"enabled":true}},"sources":[{"source":"userSettings","settings":"#,
    r#"{}}],"errors":[{"file":"claude-settings.json","path":"disableAllHooks","message":"Expected boolean, but recei"#,
    r#"ved string"}]}}}"#,
);
const LIST_PERMISSION_RULES: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"list_permission_rules","response":{""#,
    r#"state":{"rules":[{"behavior":"deny","source":"userSettings","rule":"Read(~/.ssh/**)"},<regles>]}}}}"#,
);
const GET_HOOKS_LISTING: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_hooks_listing","response":{"poli"#,
    r#"cy":{"disabledByPolicy":false,"managedOnly":false,"pluginOnly":false,"allDisabled":true,"policyHookCount":0},"#,
    r#""hooks":[],"events":[],"eventCatalog":[]}}}"#,
);
const GET_SANDBOX_DIALOG: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_sandbox_dialog","response":{"sup"#,
    r#"ported":false,"unsupported_reason":"platform","enabled":<actif>,"enabled_in_settings":true,"unavailable_reaso"#,
    r#"n":"essai","mode":"regular"}}}"#,
);
/// L'ouverture d'une main dont le profil ferme `WebSearch` d'office : Claude Code retire de `tools` un outil refusé.
const OUVERTURE: &str = concat!(
    r#"{"type":"system","subtype":"init","cwd":"bac","tools":["Task","Bash","Edit","Read","Skill","WebFetch","Workfl"#,
    r#"ow","Write"],"mcp_servers":[],"model":"<modele>","permissionMode":"acceptEdits","claude_code_version":"9.9.9-"#,
    r#"fausse"}"#,
);
const ECHEC: &str =
    r#"{"type":"result","subtype":"success","is_error":true,"num_turns":1,"result":"Je n'ai pas pu finir"}"#;
const REUSSITE: &str = concat!(
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"bonjour.txt ecrit\nPREUVE: vert"#,
    r#""}"#,
);

/// La version que la fausse claude dit dans son ouverture, que le constat du fil reprend.
const VERSION: &str = "9.9.9-fausse";

/// La fausse claude de Windows. Que de l'ASCII : la coquille lit le script dans la page de code du système. `%*` garde
/// la ligne de commande telle que Rust l'écrit pour un fichier de commandes (jamais `%~1`, qui ôterait les guillemets).
const FAUSSE_WINDOWS: &[&str] = &[
    "@echo off",
    "rem La fausse claude des essais : elle rejoue ce que le test a pose a cote d'elle.",
    r#"set "ici=%~dp0""#,
    r#"echo %*>>"%ici%args.txt""#,
    r#"set>"%ici%env.txt""#,
    r#"if exist "%ici%changer-le-profil" echo # change>>"%ici%..\lydite\profils\veille.profil""#,
    r#"if exist "%ici%marque" (set n=2) else (set n=1)"#,
    r#"if %n%==1 type nul>"%ici%marque""#,
    r#"type nul>>"%ici%entree.jsonl""#,
    r#"type "%ici%controle-%n%.jsonl""#,
    // L'entrée se recopie ligne à ligne, chaque ligne écrite aussitôt : `findstr` garde sa sortie jusqu'à sa fin et la
    // perd si l'arbre de la main est tué, on ne verrait alors rien de ce qu'elle a lu. `set /p` rend en morceaux une
    // ligne qui déborde ce qu'une lecture du tuyau a apporté (un Ko au plus) : `recoller` les recolle.
    "setlocal enabledelayedexpansion",
    ":lire",
    r#"set "ligne=""#,
    r#"set /p "ligne=""#,
    "if not defined ligne goto lu",
    r#">>"%ici%entree.jsonl" echo(!ligne!"#,
    "goto lire",
    ":lu",
    "endlocal",
    r#"type "%ici%fin-%n%.jsonl""#,
    "exit /b 0",
];

/// La fausse claude des autres systèmes : une ligne par argument dans `args.txt`, puis `---`.
const FAUSSE_UNIX: &[&str] = &[
    "#!/bin/sh",
    "# La fausse claude des essais : elle rejoue ce que le test a pose a cote d'elle.",
    r#"ici=$(dirname "$0")"#,
    r#"printf '%s\n' "$@" >> "$ici/args.txt""#,
    r#"echo '---' >> "$ici/args.txt""#,
    r#"env > "$ici/env.txt""#,
    r#"if [ -e "$ici/changer-le-profil" ]; then echo '# change' >> "$ici/../lydite/profils/veille.profil"; fi"#,
    r#"if [ -e "$ici/marque" ]; then n=2; else n=1; fi"#,
    r#"if [ "$n" = 1 ]; then : > "$ici/marque"; fi"#,
    r#": >> "$ici/entree.jsonl""#,
    r#"cat "$ici/controle-$n.jsonl""#,
    r#"cat >> "$ici/entree.jsonl""#,
    r#"cat "$ici/fin-$n.jsonl""#,
    "exit 0",
];

/// Le faux srt de Windows, que `LYDITE_SRT` nomme : il saute `--settings <fichier>`, puis joue la preuve de `sh -c
/// <preuve>` par la coquille de Windows, où `sh` peut manquer, et `true`, l'épreuve du mur, sort en 0.
const FAUX_SRT_WINDOWS: &[&str] =
    &["@echo off", "rem Le faux srt des essais.", r#"if "%~5"=="true" exit /b 0"#, "%~5", "exit /b %errorlevel%"];

/// Le faux srt des autres systèmes : il saute `--settings <fichier>`, puis joue `sh -c <preuve>`.
const FAUX_SRT_UNIX: &[&str] = &["#!/bin/sh", "# Le faux srt des essais.", "shift 2", r#"exec "$@""#];

fn kraken() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lydite"))
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
    assert_eq!(sortie.status.code(), Some(2), "{commande:?} : {}", dit(&sortie));
    assert!(!erreur.contains("panicked"), "{commande:?} : {erreur}");
    assert!(sortie.stdout.is_empty(), "{commande:?} écrit sur stdout : {}", dit(&sortie));
    let lignes: Vec<&str> = erreur.lines().collect();
    assert_eq!(lignes.len(), 4, "{commande:?} : {erreur}");
    let etiquettes = ["  règle : ", "  preuve : ", "  correction : "];
    for (ligne, debut) in lignes[1..].iter().zip(etiquettes) {
        assert!(ligne.starts_with(debut) && ligne.len() > debut.len(), "{commande:?} : {erreur}");
    }
    let [regle, preuve, correction] = [0, 1, 2].map(|i| lignes[i + 1][etiquettes[i].len()..].to_string());
    Refus { code: lignes[0].to_string(), regle, preuve, correction }
}

/// Ce qu'une commande a rendu, pour le message d'un échec.
fn dit(sortie: &Output) -> String {
    let (sortant, erreur) = (String::from_utf8_lossy(&sortie.stdout), String::from_utf8_lossy(&sortie.stderr));
    format!("code {:?}\nstdout : {sortant}\nstderr : {erreur}", sortie.status.code())
}

/// Ce que `lydite etat` a dit, sans la durée de la ligne de reçu : elle suit l'horloge, et ne se fige pas.
fn etat_sans_duree(sortie: &Output) -> String {
    let texte = String::from_utf8_lossy(&sortie.stdout);
    let lignes: Vec<String> = texte.trim_end().lines().map(sans_duree).collect();
    lignes.join("\n")
}

/// Une ligne de reçu (`Coût : … · 2 mains, … · 3 s`) sans sa durée finale ; toute autre ligne, telle quelle.
fn sans_duree(ligne: &str) -> String {
    let duree = |fin: &str| {
        let mut mots = fin.split(' ');
        let nombre = mots.next().is_some_and(|mot| !mot.is_empty() && mot.chars().all(|c| c.is_ascii_digit()));
        nombre && (fin.ends_with(" s") || fin.ends_with(" min"))
    };
    match ligne.rsplit_once(" · ") {
        Some((debut, fin)) if ligne.starts_with("Coût : ") && duree(fin) => debut.to_string(),
        _ => ligne.to_string(),
    }
}

/// La règle d'un refus est une phrase : non vide, finie par un point.
fn assert_regle(refus: &Refus) {
    let regle = refus.regle.trim();
    assert!(!regle.is_empty() && regle.ends_with('.'), "la règle est une phrase : {refus:?}");
}

/// La correction d'un refus de la ligne de commande donne l'usage de `lancer`, avec ses trois drapeaux.
fn assert_usage(refus: &Refus) {
    let usage = r#"lydite lancer <profil> "<objectif>" [--n N] [--verifier "<commande>"] [--depot <chemin>]"#;
    assert!(refus.correction.contains(usage), "la correction donne l'usage : {refus:?}");
}

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    // Un nom court : Claude Code met ses sockets de pont dans son dossier temporaire (`<dossier
    // temporaire>/claude-http-<16 hex>.sock`), et un chemin de socket tient en 107 octets.
    let dir = std::env::temp_dir().join(format!("kp-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Joue git dans ce dossier, avec une identité de passage : le dépôt jetable n'a pas de configuration. Rend sa sortie.
fn git(dir: &Path, args: &[&str]) -> String {
    let identite = ["-c", "user.name=t", "-c", "user.email=t@t"];
    let sortie = Command::new("git").current_dir(dir).args(identite).args(args).output().unwrap();
    assert!(sortie.status.success(), "git {args:?} : {}", String::from_utf8_lossy(&sortie.stderr));
    String::from_utf8_lossy(&sortie.stdout).trim().to_string()
}

/// Les noms de ce que contient un dossier, triés.
fn noms(dir: &Path) -> Vec<String> {
    let mut noms: Vec<String> =
        fs::read_dir(dir).unwrap().map(|entree| entree.unwrap().file_name().to_string_lossy().into_owned()).collect();
    noms.sort();
    noms
}

/// Le dépôt visé n'a rien reçu : son `.git` et les `dossiers` que le test y a posés, aucun worktree, aucune branche de
/// bac.
fn depot_intact(depot: &Path, dossiers: &[&str]) {
    let mut attendus = vec![".git"];
    attendus.extend(dossiers);
    attendus.sort_unstable();
    assert_eq!(noms(depot), attendus, "le dépôt visé ne porte que ce que le test y a posé");
    assert!(!depot.join(".git").join("worktrees").exists(), "git a posé un worktree");
    assert_eq!(git(depot, &["branch", "--list", "lydite/*"]), "", "une branche de bac existe");
}

/// Le dossier de Kraken n'a ni fil ni bac : un refus de `preparer` ou d'`options` laisse tout intact.
fn sans_bac_ni_fil(dossier: &Path) {
    assert!(!dossier.join("fil.jsonl").exists(), "un fil a été ouvert dans {}", dossier.display());
    assert!(!dossier.join("bacs").exists(), "un bac a été posé dans {}", dossier.display());
}

/// Une chaîne JSON, guillemets compris, pour les textes de l'essai : le guillemet et la barre inverse s'échappent.
fn json_chaine(texte: &str) -> String {
    let mut json = String::from("\"");
    for c in texte.chars() {
        match c {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            c => json.push(c),
        }
    }
    json.push('"');
    json
}

/// Un tableau JSON compact de chaînes.
fn tableau(chaines: &[String]) -> String {
    format!("[{}]", chaines.iter().map(|chaine| json_chaine(chaine)).collect::<Vec<_>>().join(","))
}

/// La clé `excludedCommands` d'un `sandbox` et sa virgule, écrites ici à la main : les commandes qui sortent du bac à
/// sable, rien s'il n'y en a pas.
fn cle_exclues(exclues: &[&str]) -> String {
    let exclues: Vec<String> = exclues.iter().map(|commande| commande.to_string()).collect();
    if exclues.is_empty() { String::new() } else { format!(r#""excludedCommands":{},"#, tableau(&exclues)) }
}

/// Le dossier en règle de permission de Claude Code, écrit ici à la main : `//` puis le chemin absolu en forme POSIX,
/// `C:\x` donnant `//c/x` sous Windows et `/x` donnant `//x` ailleurs.
fn regle_du_dossier(dossier: &Path) -> String {
    let chemin = dossier.to_str().expect("un dossier d'essai en UTF-8");
    if cfg!(windows) {
        format!("//{}{}", chemin[..1].to_ascii_lowercase(), chemin[2..].replace('\\', "/"))
    } else {
        format!("/{chemin}")
    }
}

/// Les règles de permission d'un profil, copiées à la main dans la forme du contrat. `allow` : les permis d'office, un
/// `WebFetch` par domaine de `reseau`, les sortants que `outils` déclare dans l'ordre de `FERMES`, ses serveurs
/// `mcp__…` dans l'ordre où il les écrit, puis ce que le prêt de la preuve ajoute. `deny` : les sortants que `outils`
/// ne déclare pas, les quatre gardes du dossier de Kraken de l'essai (fil, profils, puis `Read` et `Edit` des réglages
/// de srt), `Read` et `Edit` du fichier de connexion de chaque leurre que `Essai::kraken` pose, `CLAUDE_CONFIG_DIR`
/// puis le `.claude` du dossier personnel, puis l'`Edit` du git commun du dépôt visé.
fn regles(essai: &Essai, reseau: &[&str], outils: &[&str], pret: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut allow: Vec<String> = ["Skill", "Workflow", "ExitPlanMode"].map(String::from).into();
    allow.extend(reseau.iter().map(|domaine| format!("WebFetch(domain:{domaine})")));
    allow.extend(FERMES.iter().filter(|ferme| outils.contains(ferme)).map(|ferme| ferme.to_string()));
    allow.extend(outils.iter().filter(|outil| outil.starts_with("mcp__")).map(|outil| outil.to_string()));
    allow.extend(pret.iter().map(|regle| regle.to_string()));
    let regle = regle_du_dossier(&essai.dossier);
    let gardes = [
        format!("Edit({regle}/fil.jsonl)"),
        format!("Edit({regle}/profils/**)"),
        format!("Read({regle}/srt-*.json)"),
        format!("Edit({regle}/srt-*.json)"),
    ];
    let fermes = FERMES.iter().filter(|ferme| !outils.contains(ferme)).map(|ferme| ferme.to_string());
    let commun = git(&essai.depot, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let commun = format!("Edit({}/**)", regle_du_dossier(Path::new(&commun)));
    (allow, fermes.chain(gardes).chain(essai.connexion()).chain([commun]).collect())
}

/// Les quatre réponses de contrôle d'une main fidèle au `--settings` déclaré : au modèle `modele`, le mur exigé ou
/// demandé et ses commandes `exclues`, que Claude Code dit actif ou non, et chaque règle déclarée vue à la source
/// `flagSettings`.
fn reponses(
    modele: &str,
    exige: bool,
    exclues: &[&str],
    actif: bool,
    allow: &[String],
    deny: &[String],
) -> [String; 4] {
    let vues: Vec<String> = [("allow", allow), ("deny", deny)]
        .iter()
        .flat_map(|(comportement, liste)| {
            liste.iter().map(move |regle| {
                format!(r#"{{"behavior":"{comportement}","source":"flagSettings","rule":{}}}"#, json_chaine(regle))
            })
        })
        .collect();
    [
        GET_SETTINGS
            .replace("<modele>", modele)
            .replace("<exige>", &exige.to_string())
            .replace("<exclues>", &cle_exclues(exclues)),
        LIST_PERMISSION_RULES.replace("<regles>", &vues.join(",")),
        GET_HOOKS_LISTING.to_string(),
        GET_SANDBOX_DIALOG.replace("<actif>", &actif.to_string()),
    ]
}

/// La ligne de commande telle que la fausse claude de Windows l'a écrite (`%*`), d'où l'on tire l'argv : Rust met entre
/// guillemets un argument qui porte autre chose que des lettres, des chiffres et quelques signes, et double chaque
/// guillemet du sien. Les arguments se coupent sur les blancs hors guillemets.
fn argv_de_windows(ligne: &str) -> Vec<String> {
    let (mut argv, mut courant, mut dedans) = (Vec::new(), String::new(), false);
    for c in ligne.trim_end().chars() {
        match c {
            '"' => {
                dedans = !dedans;
                courant.push(c);
            }
            ' ' if !dedans => argv.push(std::mem::take(&mut courant)),
            c => courant.push(c),
        }
    }
    argv.push(courant);
    let defaire = |arg: String| match arg.strip_prefix('"').and_then(|arg| arg.strip_suffix('"')) {
        Some(interieur) => interieur.replace("\"\"", "\""),
        None => arg,
    };
    argv.into_iter().map(defaire).collect()
}

/// L'argv d'une tentative de la fausse claude, tel que Kraken le lui passe après son nom : `-p`, les formats, le mode,
/// `--strict-mcp-config`, `--mcp-config` si le profil a un `mcp`, `--setting-sources user`, `--model`, puis
/// `--settings` et le JSON en dernier. Jamais de `--tools`.
fn argv_attendu(modele: &str, mcp: Option<&str>, reglages: &str) -> Vec<String> {
    let debut = ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"];
    let mode = ["--permission-mode", "acceptEdits", "--permission-prompts", "none", "--strict-mcp-config"];
    let fin = ["--setting-sources", "user", "--model", modele, "--settings", reglages];
    let mcp = mcp.into_iter().flat_map(|mcp| ["--mcp-config", mcp]);
    debut.into_iter().chain(mode).chain(mcp).chain(fin).map(String::from).collect()
}

/// Les lignes du fil qui sont de ce type, dans l'ordre du fil.
fn lignes_de<'a>(fil: &'a str, genre: &str) -> Vec<&'a str> {
    let type_ = format!(r#""type":"{genre}""#);
    fil.lines().filter(|ligne| ligne.contains(&type_)).collect()
}

/// Le type d'une ligne du fil.
fn genre(ligne: &str) -> &str {
    let debut = ligne.find(r#""type":""#).expect("une ligne du fil porte son type") + 8;
    ligne[debut..].split('"').next().unwrap()
}

/// Ce qu'une chaîne JSON de `texte` écrit entre ses guillemets : un morceau de ligne du fil à y chercher.
fn dans_une_chaine(texte: &str) -> String {
    let chaine = json_chaine(texte);
    chaine[1..chaine.len() - 1].to_string()
}

/// Les lignes que la fausse claude a recopiées de son entrée. Sous Windows, `set /p` rend en morceaux une ligne qui
/// déborde ce qu'une lecture du tuyau a apporté, un Ko au plus, selon l'instant où elle arrive, et la fausse claude
/// recopie chaque morceau sur sa ligne : ceux d'un même objet JSON se recollent, octet pour octet. Ailleurs, `cat` ne
/// coupe rien, et rien ne se recolle.
fn recoller(brut: &[u8]) -> Vec<String> {
    let mut lignes: Vec<Vec<u8>> = Vec::new();
    for ligne in brut.split_inclusive(|&octet| octet == b'\n') {
        let ligne = ligne.strip_suffix(b"\n").unwrap_or(ligne);
        let ligne = ligne.strip_suffix(b"\r").unwrap_or(ligne);
        match lignes.last_mut() {
            Some(morceau) if cfg!(windows) && !ferme(morceau) => morceau.extend_from_slice(ligne),
            _ => lignes.push(ligne.to_vec()),
        }
    }
    lignes.iter().map(|ligne| String::from_utf8_lossy(ligne).into_owned()).collect()
}

/// Ces octets ferment chaque objet et chaque tableau JSON qu'ils ouvrent, hors des chaînes : une ligne de JSON entière,
/// et non le premier morceau d'une ligne coupée.
fn ferme(json: &[u8]) -> bool {
    let (mut profondeur, mut dans_chaine, mut echappe) = (0i64, false, false);
    for &octet in json {
        match octet {
            _ if echappe => echappe = false,
            b'\\' if dans_chaine => echappe = true,
            b'"' => dans_chaine = !dans_chaine,
            b'{' | b'[' if !dans_chaine => profondeur += 1,
            b'}' | b']' if !dans_chaine => profondeur -= 1,
            _ => {}
        }
    }
    profondeur <= 0 && !dans_chaine
}

/// Un essai : un dépôt git jetable avec un commit, un dossier de Kraken jetable que le premier lancement crée, et à
/// côté la fausse claude, ce qu'elle répond et ce qu'elle note, tout sous une racine neuve à lui.
struct Essai {
    racine: PathBuf,
    depot: PathBuf,
    dossier: PathBuf,
    main: PathBuf,
}

impl Essai {
    fn nouveau(nom: &str) -> Essai {
        let racine = neuf(nom);
        let (depot, main) = (racine.join("depot"), racine.join("main"));
        fs::create_dir(&depot).unwrap();
        fs::create_dir(&main).unwrap();
        git(&depot, &["init", "-q"]);
        git(&depot, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let essai = Essai { dossier: racine.join("lydite"), racine, depot, main };
        essai.poser_la_fausse_claude();
        essai
    }

    /// La fausse claude, un fichier de commandes sous Windows et un script ailleurs.
    fn fausse_claude(&self) -> PathBuf {
        self.main.join(if cfg!(windows) { "fausse-claude.cmd" } else { "fausse-claude.sh" })
    }

    /// Le faux srt de l'essai, à sa racine, que `kraken` nomme en `LYDITE_SRT` : sous le mur exigé, Kraken rejoue toute
    /// preuve par lui.
    fn faux_srt(&self) -> PathBuf {
        self.racine.join(if cfg!(windows) { "faux-srt.cmd" } else { "faux-srt.sh" })
    }

    /// L'écrit, le ferme, puis le rend exécutable : c'est Kraken qui la lance, pas le test. De même le faux srt.
    fn poser_la_fausse_claude(&self) {
        let (lignes, srt, fin) =
            if cfg!(windows) { (FAUSSE_WINDOWS, FAUX_SRT_WINDOWS, "\r\n") } else { (FAUSSE_UNIX, FAUX_SRT_UNIX, "\n") };
        for (script, lignes) in [(self.fausse_claude(), lignes), (self.faux_srt(), srt)] {
            fs::write(&script, lignes.iter().map(|ligne| format!("{ligne}{fin}")).collect::<String>()).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
    }

    /// `kraken`, prêt à lancer sans qu'aucune vraie main ne puisse partir ni rien de l'humain ne soit lu : le dossier
    /// de Kraken, le dépôt visé pour dossier courant, un `claude` qui n'existe pas pour le profil intégré, le faux srt,
    /// et les leurres d'où Kraken tire les dossiers de connexion de Claude, le dossier personnel et
    /// `CLAUDE_CONFIG_DIR`.
    fn kraken(&self) -> Command {
        let mut commande = kraken();
        commande
            .current_dir(&self.depot)
            .env("LYDITE_DOSSIER", &self.dossier)
            .env("LYDITE_CLAUDE", "lydite-claude-introuvable")
            .env("LYDITE_SRT", self.faux_srt())
            .env("USERPROFILE", self.personnel())
            .env("HOME", self.personnel())
            .env("CLAUDE_CONFIG_DIR", self.config());
        commande
    }

    /// Le dossier personnel de l'essai, que `kraken` pose sur l'enfant en `USERPROFILE` et `HOME` : un leurre sous la
    /// racine, jamais créé, dont le `.claude` est le second dossier de connexion de Claude.
    fn personnel(&self) -> PathBuf {
        self.racine.join("personnel")
    }

    /// Le dossier de connexion de Claude de l'essai, que `kraken` pose sur l'enfant en `CLAUDE_CONFIG_DIR` : un leurre
    /// sous la racine, jamais créé.
    fn config(&self) -> PathBuf {
        self.racine.join("config")
    }

    /// Les règles de `deny` qui ferment aux outils de la main le fichier de connexion de Claude de chaque leurre,
    /// écrites ici à la main : `Read` puis `Edit`, pour `CLAUDE_CONFIG_DIR`, puis pour le `.claude` du dossier
    /// personnel.
    fn connexion(&self) -> Vec<String> {
        let leurres = [self.config(), self.personnel().join(".claude")].map(|leurre| regle_du_dossier(&leurre));
        let fermer = |regle: &String| ["Read", "Edit"].map(|outil| format!("{outil}({regle}/.credentials.json)"));
        leurres.iter().flat_map(fermer).collect()
    }

    /// Écrit le profil `nom` : la fausse claude pour programme, Haiku puis Sonnet, le réseau d'`exemple.test`, des
    /// délais qui bornent un essai qui se perdrait, puis `plus`. Rend son texte.
    fn profil(&self, nom: &str, plus: &str) -> String {
        let texte = format!(
            "# La main d'essai : la fausse claude, Haiku puis Sonnet.\nsorte = claude\nprogramme = {}\nmodele = haiku\n\
             relance = sonnet\nreseau = exemple.test\ndelai_main = 120\ndelai_preuve = 60\n{plus}",
            self.fausse_claude().display()
        );
        fs::create_dir_all(self.dossier.join("profils")).unwrap();
        fs::write(self.dossier.join("profils").join(format!("{nom}.profil")), &texte).unwrap();
        texte
    }

    /// `lydite accepter <nom>`.
    fn accepter(&self, nom: &str) -> Output {
        self.kraken().args(["accepter", nom]).output().unwrap()
    }

    /// Écrit le profil `nom` et l'accepte. Rend son texte et son empreinte, que `lydite accepter` a dite.
    fn profil_accepte(&self, nom: &str, plus: &str) -> (String, String) {
        let texte = self.profil(nom, plus);
        let sortie = self.accepter(nom);
        assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
        let affiche = String::from_utf8_lossy(&sortie.stdout).into_owned();
        let empreinte = affiche.trim_end().rsplit_once("\nempreinte : ").expect("l'empreinte est dite").1.to_string();
        (texte, empreinte)
    }

    /// `lydite lancer <nom> <objectif>` et `plus`.
    fn lancer(&self, nom: &str, plus: &[&str]) -> Output {
        self.kraken().args(["lancer", nom, OBJECTIF]).args(plus).output().unwrap()
    }

    /// Le lancement des essais qui réussissent : la vérification `git status`, que la main peut jouer elle-même.
    fn lancer_et_verifier(&self, nom: &str) -> Output {
        self.lancer(nom, &["--verifier", VERIFIER])
    }

    /// Pose ce que la fausse claude répondra à ses deux tentatives, juste, pour le profil d'essai : Haiku, dont le
    /// résultat est en erreur, puis Sonnet, qui réussit. `pret` est ce que le profil prête de la vérification, `PRET`
    /// ou rien. Le mur est demandé : le prêt y sort aussi la vérification du bac à sable (`EXCLUES`). Claude Code dit
    /// le mur inactif, comme sous Windows le 1/11.
    fn repondre_juste(&self, pret: &[&str]) {
        let (allow, deny) = regles(self, &RESEAU, &[], pret);
        let exclues: &[&str] = if pret.is_empty() { &[] } else { &EXCLUES };
        self.repondre_a(false, exclues, false, &allow, &deny);
    }

    /// Pose ce que la fausse claude répondra à ses deux tentatives, fidèle à ces règles : Haiku, dont le résultat est
    /// en erreur, puis Sonnet, qui réussit. Le mur est exigé ou demandé par le profil, avec ses commandes `exclues`,
    /// actif ou non pour Claude Code.
    fn repondre_a(&self, exige: bool, exclues: &[&str], actif: bool, allow: &[String], deny: &[String]) {
        let tentatives = [(1, "claude-haiku-5-5", ECHEC), (2, "claude-sonnet-5-5", REUSSITE)];
        for (n, modele, resultat) in tentatives {
            self.repondre(n, &reponses(modele, exige, exclues, actif, allow, deny));
            self.conclure(n, modele, resultat);
        }
    }

    /// Pose les quatre réponses de contrôle de la tentative `n`.
    fn repondre(&self, n: u8, reponses: &[String; 4]) {
        fs::write(self.main.join(format!("controle-{n}.jsonl")), reponses.join("\n") + "\n").unwrap();
    }

    /// Pose l'ouverture, au modèle `modele`, et le résultat de la tentative `n`.
    fn conclure(&self, n: u8, modele: &str, resultat: &str) {
        self.conclure_par(n, &OUVERTURE.replace("<modele>", modele), resultat);
    }

    /// Pose cette ouverture et ce résultat à la tentative `n`.
    fn conclure_par(&self, n: u8, ouverture: &str, resultat: &str) {
        fs::write(self.main.join(format!("fin-{n}.jsonl")), format!("{ouverture}\n{resultat}\n")).unwrap();
    }

    /// Le fil du dossier de Kraken.
    fn fil(&self) -> String {
        fs::read_to_string(self.dossier.join("fil.jsonl")).unwrap()
    }

    /// La dernière vague du fil, lue à son ouverture.
    fn vague(&self) -> String {
        let fil = self.fil();
        let ouverture = lignes_de(&fil, "vague.ouverte").pop().expect("le fil ouvre une vague");
        let debut = ouverture.find(r#""vague":""#).unwrap() + 9;
        ouverture[debut..].split('"').next().unwrap().to_string()
    }

    /// Le bac de la première main de la dernière vague.
    fn bac(&self) -> PathBuf {
        self.dossier.join("bacs").join(format!("main-{}-1", self.vague()))
    }

    /// Le fichier que la fausse claude a écrit à côté d'elle, `None` s'il n'est pas là.
    fn note(&self, nom: &str) -> Option<String> {
        fs::read(self.main.join(nom)).ok().map(|octets| String::from_utf8_lossy(&octets).into_owned())
    }

    /// L'argv de chaque tentative de la fausse claude, dans l'ordre : ce que Kraken lui a passé après son nom.
    fn argv_recus(&self) -> Vec<Vec<String>> {
        let brut = self.note("args.txt").expect("la fausse claude est partie : args.txt existe");
        if cfg!(windows) {
            return brut.lines().map(argv_de_windows).collect();
        }
        let mut tentatives = vec![Vec::new()];
        for ligne in brut.lines() {
            match ligne {
                "---" => tentatives.push(Vec::new()),
                arg => tentatives.last_mut().unwrap().push(arg.to_string()),
            }
        }
        tentatives.pop(); // ce qui suit le dernier `---`
        tentatives
    }

    /// Les `--settings` que Kraken a passés, un par tentative.
    fn reglages_recus(&self) -> Vec<String> {
        self.argv_recus().into_iter().map(|argv| argv.last().cloned().unwrap()).collect()
    }

    /// Ce que la fausse claude a lu sur son entrée, tentative après tentative, une ligne par élément (voir `recoller`).
    fn entree(&self) -> Vec<String> {
        let brut = fs::read(self.main.join("entree.jsonl")).expect("la fausse claude est partie : entree.jsonl existe");
        recoller(&brut)
    }

    /// Les réglages que Kraken doit passer à la première main de la dernière vague, écrits ici à la main dans la forme
    /// du contrat : le mur exigé ou demandé, les commandes `exclues` du bac à sable, les chemins du dossier, du bac et
    /// du git commun, les domaines de `reseau` et les règles de `regles`.
    fn reglages_attendus(
        &self,
        exige: bool,
        exclues: &[&str],
        reseau: &[&str],
        allow: &[String],
        deny: &[String],
    ) -> String {
        let commun = git(&self.depot, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);
        let lus = [self.bac().to_string_lossy().into_owned(), commun];
        let fermes = ["~/".to_string(), self.dossier.to_string_lossy().into_owned()];
        let domaines: Vec<String> = reseau.iter().map(|domaine| domaine.to_string()).collect();
        format!(
            concat!(
                r#"{{"sandbox":{{"enabled":true,"failIfUnavailable":{exige},"allowUnsandboxedCommands":false,"#,
                r#"{exclues}"filesystem":{{"denyRead":{fermes},"allowRead":{lus}}},"#,
                r#""network":{{"allowedDomains":{domaines},"strictAllowlist":true}}}},"#,
                r#""permissions":{{"allow":{allow},"deny":{deny}}},"disableAllHooks":true,"autoMemoryEnabled":false}}"#,
            ),
            exige = exige,
            exclues = cle_exclues(exclues),
            fermes = tableau(&fermes),
            lus = tableau(&lus),
            domaines = tableau(&domaines),
            allow = tableau(allow),
            deny = tableau(deny),
        )
    }

    /// Rien n'est parti : ni la fausse claude, ni fil, ni bac, et le dépôt n'a rien reçu.
    fn rien_ne_part(&self) {
        assert!(self.note("args.txt").is_none(), "la fausse claude est partie : {:?}", self.note("args.txt"));
        sans_bac_ni_fil(&self.dossier);
        depot_intact(&self.depot, &[]);
    }

    fn nettoyer(&self) {
        let _ = fs::remove_dir_all(&self.racine);
    }
}

#[test]
fn un_dossier_de_kraken_sous_le_depot_est_refuse_avant_tout_bac() {
    let mut essai = Essai::nouveau("sous-depot");
    // Sous la tête du dépôt, à plusieurs étages, dans son .git, ou sous un sous-dossier d'où l'on lance : git dit où
    // est la tête du dépôt, et le dossier de Kraken n'y est pas.
    let courant = essai.depot.join("sous");
    fs::create_dir(&courant).unwrap();
    let dossiers = [
        essai.depot.join("lydite"),
        essai.depot.join("a").join("b").join("lydite"),
        essai.depot.join(".git").join("lydite"),
        courant.join("lydite"),
    ];
    for dossier in dossiers {
        essai.dossier = dossier.clone();
        let refus = lire_refus(essai.kraken().current_dir(&courant).args(["lancer", "claude", OBJECTIF]));
        assert_eq!(refus.code, "lydite : 6015 lancer.dossier_sous_le_depot", "{}", dossier.display());
        assert_regle(&refus);
        assert!(refus.preuve.contains(&dossier.display().to_string()), "la preuve nomme le dossier : {refus:?}");
        assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus:?}");
        assert!(!dossier.exists(), "rien n'est posé à {}", dossier.display());
    }
    // Un `..` est refusé de même, où qu'il mène : même hors du dépôt, ce dossier-là n'est pas celui qu'on croit.
    essai.dossier = essai.racine.join("lydite").join("..").join("ailleurs");
    let refus = lire_refus(essai.kraken().args(["lancer", "claude", OBJECTIF]));
    assert_eq!(refus.code, "lydite : 6015 lancer.dossier_sous_le_depot");
    assert!(refus.preuve.ends_with("porte « .. »"), "{refus:?}");
    assert!(!essai.racine.join("ailleurs").exists() && !essai.racine.join("lydite").exists());
    depot_intact(&essai.depot, &["sous"]);
    essai.nettoyer();
}

#[test]
fn un_verifier_blanc_est_refuse() {
    // Rejouée, une commande blanche réussirait toujours : toute main serait prouvée. Le profil est accepté et sa fausse
    // main prête, pour qu'on voie qu'elle ne part pas.
    let essai = Essai::nouveau("blanc");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    for blanche in ["", " ", "\t", "  \t ", " \n "] {
        let refus = lire_refus(essai.kraken().args(["lancer", "veille", OBJECTIF, "--verifier", blanche]));
        assert_eq!(refus.code, "lydite : 6014 lancer.preuve_vide", "{blanche:?}");
        assert_regle(&refus);
        assert!(refus.preuve.contains("--verifier") && refus.preuve.contains("une commande blanche"), "{refus:?}");
        assert!(refus.correction.contains("--verifier"), "{refus:?}");
    }
    // Un --verifier blanc après d'autres drapeaux est refusé de même.
    let ligne = ["lancer", "veille", OBJECTIF, "--n", "2", "--verifier", " "];
    assert_eq!(lire_refus(essai.kraken().args(ligne)).code, "lydite : 6014 lancer.preuve_vide");
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn un_drapeau_a_la_place_de_l_objectif_ne_lance_rien() {
    // Sans ce refus, `lancer veille --verifier x` lancerait une main sur l'objectif « --verifier ». Le profil est
    // accepté et sa fausse main prête : aucune ne part.
    let essai = Essai::nouveau("drapeau");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    let cas: [(&[&str], &str); 7] = [
        (&["lancer", "veille", "--verifier", "x"], "il manque l'objectif"),
        (&["lancer", "veille", "--n", "3"], "il manque l'objectif"),
        (&["lancer", "veille", "--depot", "d", "--n", "3"], "il manque l'objectif"),
        (&["lancer", "--n", "3", OBJECTIF], "il manque le profil et l'objectif"),
        (&["lancer", "--verifier", VERIFIER, "--n", "3"], "il manque le profil et l'objectif"),
        (&["lancer", "--depot", "d", "--n", "3"], "il manque le profil et l'objectif"),
        (&["lancer"], "il manque le profil et l'objectif"),
    ];
    for (mots, manque) in cas {
        let refus = lire_refus(essai.kraken().args(mots));
        assert_eq!(refus.code, "lydite : 1006 contrat.commande_mal_formee", "{mots:?}");
        assert_regle(&refus);
        assert!(refus.preuve.contains(manque), "la preuve dit ce qui manque ({manque}) : {refus:?}");
        assert_usage(&refus);
    }
    // Un mot qui n'est pas un drapeau de `lancer` reste un objectif : « --help » est refusé plus loin, par le profil.
    let refus = lire_refus(essai.kraken().args(["lancer", "inconnu", "--help"]));
    assert_eq!(refus.code, "lydite : 6001 lancer.profil_inconnu");
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn un_profil_inconnu_est_refuse() {
    let essai = Essai::nouveau("inconnu");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    // La ligne est entière, avec tous ses drapeaux : seul le profil manque. Un autre profil existe, accepté, mais ce
    // n'est pas celui qu'on nomme.
    let ligne = ["lancer", "inconnu", OBJECTIF, "--n", "3", "--verifier", VERIFIER];
    let refus = lire_refus(essai.kraken().args(ligne));
    assert_eq!(refus.code, "lydite : 6001 lancer.profil_inconnu");
    assert_regle(&refus);
    assert!(refus.preuve.contains("« inconnu »") && refus.preuve.contains("inconnu.profil"), "{refus:?}");
    assert!(refus.correction.contains("lydite accepter inconnu") && refus.correction.contains("claude"), "{refus:?}");
    // Le nom d'un profil qui sortirait de `profils` est refusé avant toute lecture.
    let refus = lire_refus(essai.kraken().args(["lancer", "../veille", OBJECTIF]));
    assert_eq!(refus.code, "lydite : 6007 lancer.profil_nom_refuse");
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn un_depot_absent_est_refuse() {
    let essai = Essai::nouveau("absent");
    essai.profil_accepte("veille", "");
    // Un dépôt absent, puis un fichier à la place du dépôt : le dépôt visé est un dossier qui existe, vérifié avant
    // tout geste git.
    let (absent, fichier) = (essai.racine.join("absent"), essai.racine.join("fichier"));
    fs::write(&fichier, "x").unwrap();
    for depot in [&absent, &fichier] {
        let refus = lire_refus(essai.kraken().args(["lancer", "veille", OBJECTIF, "--depot"]).arg(depot));
        assert_eq!(refus.code, "lydite : 6002 lancer.depot_introuvable", "{}", depot.display());
        assert_regle(&refus);
        assert!(refus.preuve.contains(&depot.display().to_string()), "la preuve nomme le dépôt : {refus:?}");
        assert!(refus.correction.contains("--depot"), "{refus:?}");
    }
    assert!(!absent.exists(), "un dépôt absent ne se crée pas");
    assert_eq!(fs::read_to_string(&fichier).unwrap(), "x");
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn un_dossier_hors_git_est_refuse() {
    let essai = Essai::nouveau("hors-git");
    essai.profil_accepte("veille", "");
    let nu = essai.racine.join("nu");
    fs::create_dir(&nu).unwrap();
    let refus = lire_refus(essai.kraken().args(["lancer", "veille", OBJECTIF, "--depot"]).arg(&nu));
    assert_eq!(refus.code, "lydite : 6003 lancer.bac_impossible");
    assert_regle(&refus);
    assert!(refus.preuve.starts_with(&format!("git rev-parse dans {} a échoué (", nu.display())), "{refus:?}");
    assert!(refus.correction.contains("--depot"), "{refus:?}");
    // Le dossier hors git reste vide ; Kraken n'y a posé ni bac ni fil, pas plus que dans son dossier.
    assert_eq!(noms(&nu), Vec::<String>::new());
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn accepter_un_nom_inconnu_ou_integre_est_refuse() {
    let essai = Essai::nouveau("accepter");
    let accepter = |nom: &str| lire_refus(essai.kraken().args(["accepter", nom]));
    // Le profil intégré n'a pas d'empreinte à garder : 6013. Un nom sans fichier ni profil intégré est inconnu : 6001.
    let integre = accepter("claude");
    assert_eq!(integre.code, "lydite : 6013 lancer.profil_sans_fichier");
    assert_regle(&integre);
    assert!(integre.preuve.contains("claude.profil") && integre.preuve.contains("profil intégré"), "{integre:?}");
    let inconnu = accepter("hermes");
    assert_eq!(inconnu.code, "lydite : 6001 lancer.profil_inconnu");
    assert_regle(&inconnu);
    assert!(inconnu.preuve.contains("hermes.profil"), "{inconnu:?}");
    assert!(inconnu.correction.contains("lydite accepter hermes"), "{inconnu:?}");
    // Un nom qui sortirait de `profils` est refusé (6007) ; un dossier de Kraken qui n'est pas absolu l'est aussi
    // (6004).
    assert_eq!(accepter("../x").code, "lydite : 6007 lancer.profil_nom_refuse");
    let relatif = lire_refus(essai.kraken().args(["accepter", "claude"]).env("LYDITE_DOSSIER", "relatif"));
    assert_eq!(relatif.code, "lydite : 6004 lancer.dossier_relatif");
    // Accepter n'écrit que l'empreinte d'un fichier qui existe : ici, rien, pas même le dossier de Kraken.
    assert!(!essai.dossier.exists(), "{}", essai.dossier.display());
    essai.nettoyer();
}

#[test]
fn un_profil_faux_est_refuse_a_l_acceptation_comme_au_lancement() {
    // Un profil qui ne se lit pas, ou qui ne dit pas ce qu'il faut, ne s'accepte pas : aucune empreinte n'est écrite.
    // Au lancement, c'est son fichier qui est refusé, avant la question de son empreinte (6011).
    let essai = Essai::nouveau("profil-faux");
    let profils = essai.dossier.join("profils");
    fs::create_dir_all(&profils).unwrap();
    let cas: [(&str, &[u8], &str, &str); 6] = [
        (
            "budget",
            b"sorte = claude\nbudget = 5\n",
            "6009 lancer.profil_ligne_refusee",
            "budget.profil:2 : « budget = 5 » : aucun budget",
        ),
        (
            "decoupeur",
            b"sorte = claude\ndecoupeur = Opus\n",
            "6009 lancer.profil_ligne_refusee",
            "decoupeur.profil:2 : « decoupeur = Opus » : un modèle s'écrit en [a-z0-9.-]",
        ),
        (
            "relecteur",
            b"sorte = claude\nrelecteur = Opus\n",
            "6009 lancer.profil_ligne_refusee",
            "relecteur.profil:2 : « relecteur = Opus » : un modèle s'écrit en [a-z0-9.-]",
        ),
        (
            "relecteur-redit",
            b"sorte = claude\nrelecteur = opus\nrelecteur = haiku\n",
            "6009 lancer.profil_ligne_refusee",
            "relecteur-redit.profil:3 : « relecteur = haiku » : chaque clé s'écrit une fois",
        ),
        ("sans-sorte", b"modele = opus\n", "6010 lancer.profil_incomplet", "sans-sorte.profil ne dit pas sa sorte"),
        ("utf16", b"\xFF\xFEs\x00", "6008 lancer.profil_illisible", "utf16.profil : UTF-8 invalide après l'octet 0"),
    ];
    for (nom, octets, code, dans_la_preuve) in cas {
        fs::write(profils.join(format!("{nom}.profil")), octets).unwrap();
        for ligne in [vec!["accepter", nom], vec!["lancer", nom, OBJECTIF]] {
            let refus = lire_refus(essai.kraken().args(&ligne));
            assert_eq!(refus.code, format!("lydite : {code}"), "{ligne:?}");
            assert_regle(&refus);
            assert!(refus.preuve.contains(dans_la_preuve), "{dans_la_preuve} : {refus:?}");
            assert!(refus.correction.contains(&format!("lydite accepter {nom}")), "{refus:?}");
        }
        assert!(!profils.join(format!("{nom}.empreinte")).exists(), "{nom} n'a pas d'empreinte");
    }
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn un_dossier_de_kraken_au_chemin_refuse_ne_lance_rien() {
    // Hors du dépôt, mais à un chemin qu'une règle de permission ne sait pas écrire : une parenthèse, un crochet, une
    // accolade. Sans ce refus, les règles `Edit(...)` qui gardent le fil et les profils tomberaient seules, en silence.
    let mut essai = Essai::nouveau("chemin");
    essai.repondre_juste(&PRET);
    let mut dossiers = vec![essai.racine.join("k(1)"), essai.racine.join("k[x]"), essai.racine.join("k{x}")];
    // Sous Windows, un chemin sans lettre de lecteur (ici le préfixe `\\?\`) ne s'écrit pas non plus.
    if cfg!(windows) {
        dossiers.push(PathBuf::from(format!(r"\\?\{}", essai.racine.join("lydite").display())));
    }
    for dossier in dossiers {
        essai.dossier = dossier.clone();
        essai.profil_accepte("veille", "");
        // Le profil de fichier, accepté, comme le profil intégré : aucun ne part.
        for profil in ["veille", "claude"] {
            let refus = lire_refus(essai.kraken().args(["lancer", profil, OBJECTIF, "--verifier", VERIFIER]));
            assert_eq!(refus.code, "lydite : 6023 lancer.dossier_chemin_refuse", "{profil} dans {}", dossier.display());
            assert_regle(&refus);
            assert!(refus.preuve.contains(&dossier.display().to_string()), "la preuve nomme le chemin : {refus:?}");
            assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus:?}");
        }
        assert!(essai.note("args.txt").is_none(), "la fausse claude est partie dans {}", dossier.display());
        assert_eq!(noms(&dossier), ["profils"], "ni bac ni fil dans {}", dossier.display());
        depot_intact(&essai.depot, &[]);
    }
    essai.nettoyer();
}

#[test]
fn git_qui_ne_se_lance_pas_est_refuse_en_6006() {
    // Le dépôt existe, mais git ne se trouve pas dans le chemin de recherche de Kraken : le refus nomme le programme et
    // le dépôt. Le chemin de recherche est celui de l'enfant seul, dirigé vers un dossier vide.
    let essai = Essai::nouveau("sans-git");
    essai.profil_accepte("veille", "");
    let vide = essai.racine.join("vide");
    fs::create_dir(&vide).unwrap();
    let lancer = ["lancer", "veille", OBJECTIF, "--depot"];
    let refus = lire_refus(essai.kraken().env("PATH", &vide).args(lancer).arg(&essai.depot));
    assert_eq!(refus.code, "lydite : 6006 lancer.entree_sortie");
    assert_regle(&refus);
    let nomme = format!("lancement impossible de git dans {}", essai.depot.display());
    assert!(refus.preuve.starts_with(&nomme), "la preuve nomme le programme et le dépôt : {refus:?}");
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn un_profil_non_accepte_ne_part_pas_puis_part_une_fois_accepte() {
    let essai = Essai::nouveau("accepte");
    essai.repondre_juste(&PRET);
    let texte = essai.profil("veille", "");
    // Pas accepté : le refus 6011 dit comment, avant tout bac et tout fil, et la fausse claude ne part pas.
    let refus = lire_refus(essai.kraken().args(["lancer", "veille", OBJECTIF, "--verifier", VERIFIER]));
    assert_eq!(refus.code, "lydite : 6011 lancer.profil_non_accepte");
    assert_regle(&refus);
    assert!(refus.correction.contains("lydite accepter veille"), "{refus:?}");
    essai.rien_ne_part();
    assert_eq!(noms(&essai.dossier), ["profils"]);
    // `lydite accepter veille` affiche le texte et l'empreinte, et garde l'empreinte à côté du profil.
    let sortie = essai.accepter("veille");
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    assert!(sortie.stderr.is_empty(), "{}", dit(&sortie));
    let affiche = String::from_utf8_lossy(&sortie.stdout).into_owned();
    let (corps, empreinte) = affiche.trim_end().rsplit_once("\nempreinte : ").expect("l'empreinte est dite");
    assert_eq!(corps, texte.trim_end(), "le texte du profil est dit");
    assert!(empreinte.len() == 64 && empreinte.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')), "{empreinte}");
    let garde = essai.dossier.join("profils").join("veille.empreinte");
    assert_eq!(fs::read_to_string(&garde).unwrap(), empreinte);
    // Accepté, le profil part : la fausse claude est lancée, deux fois, et la vague est prise.
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    assert_eq!(essai.argv_recus().len(), 2, "une tentative, puis sa relance");
    let (args, fil) = (essai.note("args.txt"), essai.fil());
    assert!(fil.contains(&format!(r#""empreinte":"{empreinte}""#)), "{fil}");
    // Changé, d'une ligne vide : le refus 6012 dit l'empreinte acceptée et celle qu'il lit, et rien ne bouge.
    fs::write(essai.dossier.join("profils").join("veille.profil"), format!("{texte}\n")).unwrap();
    let refus = lire_refus(essai.kraken().args(["lancer", "veille", OBJECTIF, "--verifier", VERIFIER]));
    assert_eq!(refus.code, "lydite : 6012 lancer.profil_change");
    assert_regle(&refus);
    assert!(refus.preuve.contains(&format!("empreinte acceptée « {empreinte} »")), "l'empreinte acceptée : {refus:?}");
    let lue = refus.preuve.rsplit_once("empreinte lue « ").and_then(|(_, lue)| lue.strip_suffix(" »"));
    let lue = lue.unwrap_or_else(|| panic!("l'empreinte lue : {refus:?}")).to_string();
    assert!(lue.len() == 64 && lue != empreinte, "une autre empreinte : {lue}");
    assert!(refus.correction.contains("lydite accepter veille"), "{refus:?}");
    assert_eq!(essai.note("args.txt"), args, "la fausse claude n'est pas repartie");
    assert_eq!(essai.fil(), fil, "le fil n'a pas bougé");
    // Réaccepté, il repart avec l'empreinte qu'il vient de dire. La fausse claude recommence à sa première tentative.
    let sortie = essai.accepter("veille");
    assert!(String::from_utf8_lossy(&sortie.stdout).trim_end().ends_with(&format!("empreinte : {lue}")));
    fs::remove_file(essai.main.join("marque")).unwrap();
    assert_eq!(essai.lancer_et_verifier("veille").status.code(), Some(0));
    assert_eq!(essai.argv_recus().len(), 4, "deux tentatives de plus");
    assert!(essai.fil().contains(&format!(r#""empreinte":"{lue}""#)));
    essai.nettoyer();
}

#[test]
fn sous_windows_les_morceaux_d_une_ligne_recopiee_se_recollent() {
    // `set /p` coupe où tombe la fin d'une lecture : au milieu d'un caractère, après une accolade, un crochet ou un
    // guillemet échappé d'une chaîne. Les morceaux d'un objet JSON se recollent octet pour octet ; une ligne entière,
    // ou qui n'est pas du JSON, reste seule.
    let message = r#"{"type":"user","message":{"content":"é } \" [ ]"},"session_id":""}"#;
    let (debut, fin) = message.as_bytes().split_at(message.find('é').unwrap() + 1);
    let brut = [DEMANDES[0].as_bytes(), debut, fin, b"pas du json".as_slice()].join(b"\r\n".as_slice());
    let lu = recoller(&brut);
    if cfg!(windows) {
        assert_eq!(lu, [DEMANDES[0], message, "pas du json"]);
    } else {
        assert_eq!(lu.len(), 4, "ailleurs, rien ne se recolle : {lu:?}");
    }
    // Chaque début strict de l'objet est un morceau ; l'objet entier ne l'est pas.
    assert!(ferme(message.as_bytes()));
    for n in 1..message.len() {
        assert!(!ferme(&message.as_bytes()[..n]), "{n} octets");
    }
}

#[test]
fn une_fausse_claude_recoit_ses_drapeaux_tentative_par_tentative() {
    let essai = Essai::nouveau("drapeaux");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    // Deux tentatives : Haiku, puis Sonnet à la relance. Chacune reçoit les mêmes drapeaux, dans cet ordre, et les
    // mêmes réglages, que la forme du contrat dit à l'octet ; `--model` change seul.
    let (allow, deny) = regles(&essai, &RESEAU, &[], &PRET);
    let reglages = essai.reglages_attendus(false, &EXCLUES, &RESEAU, &allow, &deny);
    let recus = essai.argv_recus();
    assert_eq!(recus.len(), 2, "{recus:?}");
    for (argv, modele) in recus.iter().zip(["haiku", "sonnet"]) {
        assert_eq!(argv, &argv_attendu(modele, None, &reglages), "tentative sur {modele}");
        for drapeau in ["--tools", "--mcp-config"] {
            assert!(!argv.iter().any(|arg| arg == drapeau), "{drapeau} ne part pas : {argv:?}");
        }
        let json = argv.last().unwrap();
        let attendu = ["WebFetch(domain:exemple.test)", "Bash(git status)", "PowerShell(git status)", "ListAgents"];
        for dedans in attendu.into_iter().chain([r#""autoMemoryEnabled":false"#]) {
            assert!(json.contains(dedans), "{dedans} manque dans les réglages : {json}");
        }
    }
    // Sur son entrée, la fausse claude a lu à chaque tentative les quatre demandes, puis le message de la consigne.
    let lu = essai.entree();
    assert_eq!(lu.len(), 10, "{lu:?}");
    let debut_du_message = r#"{"type":"user","message":{"role":"user","content":"Ecrire bonjour.txt\n\n"#;
    for (tentative, lignes) in lu.chunks(5).enumerate() {
        assert_eq!(lignes[..4], DEMANDES, "les quatre demandes d'abord, tentative {}", tentative + 1);
        let message = &lignes[4];
        assert!(message.starts_with(debut_du_message), "{message}");
        assert!(message.ends_with(r#""},"parent_tool_use_id":null,"session_id":""}"#), "{message}");
        assert!(message.contains(VERIFIER), "la consigne nomme la vérification : {message}");
        // Seule la relance porte l'erreur de la tentative d'avant.
        assert_eq!(message.contains("Tentative pr"), tentative == 1, "{message}");
    }
    assert!(lu[9].contains("Je n'ai pas pu finir"), "la relance porte l'erreur de la première : {}", lu[9]);
    essai.nettoyer();
}

#[test]
fn le_decoupeur_part_sur_la_cle_decoupeur_a_ses_deux_tentatives() {
    // Sans la clé, le découpeur de `--decouper` part sur opus ; avec, sur la valeur lue. Aux deux tentatives, quels que
    // soient `modele` et `relance` (haiku et sonnet dans le profil d'essai). Chaque cas : la clé, le `--model` attendu,
    // et le modèle que Claude Code dit appliquer.
    let cas = [
        ("", "opus", "claude-opus-5-5"),
        ("decoupeur = claude-sonnet-5-5\n", "claude-sonnet-5-5", "claude-sonnet-5-5"),
    ];
    for (i, (cle, modele, applique)) in cas.into_iter().enumerate() {
        let essai = Essai::nouveau(&format!("decoupeur-{i}"));
        // La fausse claude répond juste au découpeur, sur son modèle et avec sa preuve prêtée, qui sort du bac à sable
        // sous le mur demandé. Elle n'écrit pas de plan : le juge de Kraken la rougit deux fois (7014).
        let (allow, deny) =
            regles(&essai, &RESEAU, &[], &[&format!("Bash({DECOUPE})"), &format!("PowerShell({DECOUPE})")]);
        for n in [1, 2] {
            essai.repondre(n, &reponses(applique, false, &[DECOUPE], false, &allow, &deny));
            essai.conclure(n, applique, REUSSITE);
        }
        essai.profil_accepte("veille", cle);
        let sortie = essai.lancer("veille", &["--decouper", "--verifier", VERIFIER]);
        assert_eq!(sortie.status.code(), Some(1), "{}", dit(&sortie));
        let erreur = String::from_utf8_lossy(&sortie.stderr);
        let classe = erreur.contains("7014 vague.bac_touche") && erreur.ends_with("aucune vague ne part\n");
        assert!(classe, "{}", dit(&sortie));
        // Ses deux tentatives partent sur le modèle du découpeur, avec sa preuve prêtée, et leurs lancements le notent.
        let recus = essai.argv_recus();
        assert_eq!(recus.len(), 2, "{recus:?}");
        for argv in &recus {
            let i = argv.iter().position(|arg| arg == "--model").expect("--model s'écrit toujours");
            assert_eq!(argv[i + 1], modele, "{argv:?}");
            assert!(argv.last().unwrap().contains(&format!("Bash({DECOUPE})")), "{argv:?}");
        }
        let fil = essai.fil();
        let lancements = lignes_de(&fil, "main.lancement");
        assert_eq!(lancements.len(), 2, "{fil}");
        for ligne in lancements {
            assert!(ligne.contains(&format!(r#""modele":"{modele}""#)), "{ligne}");
        }
        essai.nettoyer();
    }
}

#[test]
fn le_fil_inscrit_le_lancement_et_le_constat_de_chaque_tentative() {
    let essai = Essai::nouveau("fil");
    essai.repondre_juste(&PRET);
    let (_, empreinte) = essai.profil_accepte("veille", "");
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let stderr = String::from_utf8_lossy(&sortie.stderr);
    assert!(stderr.contains(&format!("pris : main-{}-1", essai.vague())), "{}", dit(&sortie));
    // Dans chaque tentative : le bail et le début, le lancement avant que la main parte, le constat à son retour, puis
    // la fin.
    let fil = essai.fil();
    let genres: Vec<&str> = fil.lines().map(genre).collect();
    let tentative = ["main.bail", "main.debut", "main.lancement", "main.constat", "main.fin"];
    let attendu: Vec<&str> =
        ["vague.ouverte"].into_iter().chain(tentative).chain(tentative).chain(["main.verdict"]).collect();
    assert_eq!(genres, attendu, "{fil}");
    // Un lancement par tentative : l'empreinte acceptée, le modèle de la tentative, le mur demandé, et les réglages
    // exacts que la fausse claude a reçus. Il est déclaré, et porte l'origine de la séance.
    let recus = essai.reglages_recus();
    let lancements = lignes_de(&fil, "main.lancement");
    assert_eq!(lancements.len(), 2);
    for ((ligne, modele), reglages) in lancements.iter().zip(["haiku", "sonnet"]).zip(&recus) {
        let champs = [
            r#""origine":"humain","origine_foi":"declare","type":"main.lancement""#.to_string(),
            r#""profil":"veille""#.to_string(),
            format!(r#""empreinte":"{empreinte}""#),
            format!(r#""modele":"{modele}""#),
            r#""mur":"demande""#.to_string(),
            format!(r#""reglages":{}"#, json_chaine(reglages)),
            r#""reglages_depot":"","foi":"declare"}"#.to_string(),
        ];
        for champ in champs {
            assert!(ligne.contains(&champ), "{champ} manque : {ligne}");
        }
    }
    // Un constat par tentative : la version que la fausse claude a dite à son ouverture, et le mur que Claude Code a
    // dit inactif. Il est constaté, et porte l'origine de la séance.
    let constats = lignes_de(&fil, "main.constat");
    assert_eq!(constats.len(), 2);
    for ligne in constats {
        let champs = [
            r#""origine":"humain","origine_foi":"declare","type":"main.constat""#.to_string(),
            format!(r#""version":"{VERSION}","mur_actif":false,"foi":"constate"}}"#),
        ];
        for champ in champs {
            assert!(ligne.contains(&champ), "{champ} manque : {ligne}");
        }
    }
    // La première tentative est rouge : le résultat de la main est en erreur, et la vérification ne tourne pas. La
    // seconde est verte : Kraken a rejoué la vérification, et la vague est prise et constatée.
    let fins = lignes_de(&fil, "main.fin");
    let rouge = concat!(
        r#""couleur":"rouge","foi":"constate","texte":"main : résultat en erreur (success) : Je n'ai pas pu finir",""#,
        r#"verif":null}"#,
    );
    assert!(fins[0].contains(rouge), "{}", fins[0]);
    let vert = r#""couleur":"vert","foi":"constate","texte":"bonjour.txt ecrit\nPREUVE: vert","verif":""#;
    assert!(fins[1].contains(vert), "{}", fins[1]);
    let verdict = lignes_de(&fil, "main.verdict");
    assert!(verdict[0].ends_with(r#""verdict":"prise","foi":"constate"}"#), "{}", verdict[0]);
    essai.nettoyer();
}

#[test]
fn une_main_dont_les_reglages_n_ont_pas_pris_est_tuee_avant_la_consigne() {
    let essai = Essai::nouveau("non-pris");
    let (allow, deny) = regles(&essai, &RESEAU, &[], &PRET);
    // Le `get_settings` du `--settings` tombé du 1/11, à chaque tentative : les trois autres réponses sont justes.
    for (n, modele) in [(1, "claude-haiku-5-5"), (2, "claude-sonnet-5-5")] {
        let mut tombees = reponses(modele, false, &EXCLUES, false, &allow, &deny);
        tombees[0] = GET_SETTINGS_TOMBE.replace("<modele>", modele);
        essai.repondre(n, &tombees);
        essai.conclure(n, modele, REUSSITE);
    }
    essai.profil_accepte("veille", "");
    let sortie = essai.lancer_et_verifier("veille");
    // La vague est classée : le code 1, et l'erreur de chaque tentative dit que les réglages n'ont pas pris.
    assert_eq!(sortie.status.code(), Some(1), "{}", dit(&sortie));
    let stderr = String::from_utf8_lossy(&sortie.stderr);
    assert!(stderr.contains(&format!("échec classé : main-{}-1", essai.vague())), "{}", dit(&sortie));
    assert!(stderr.contains("main : réglages non pris : get_settings : "), "{}", dit(&sortie));
    // La consigne n'est jamais partie : la fausse claude n'a lu que ses demandes, si elle en a lu.
    assert!(essai.entree().iter().all(|ligne| !ligne.contains(r#""type":"user""#)), "{:?}", essai.entree());
    assert_eq!(essai.argv_recus().len(), 2, "lancée deux fois, tuée deux fois");
    let fil = essai.fil();
    let fins = lignes_de(&fil, "main.fin");
    assert_eq!(fins.len(), 2, "{fil}");
    for ligne in fins {
        let rouge = r#""couleur":"rouge","foi":"constate","texte":"main : réglages non pris : get_settings : "#;
        assert!(ligne.contains(rouge), "{ligne}");
        assert!(ligne.contains("flagSettings") && ligne.contains("claude-settings.json"), "{ligne}");
    }
    // Ce que Kraken a lu avant de tuer se note quand même : pas de version, faute d'ouverture, mais le mur que Claude
    // Code a dit inactif. Puis la main est classée.
    let constats = lignes_de(&fil, "main.constat");
    assert_eq!(constats.len(), 2);
    assert!(constats.iter().all(|ligne| ligne.contains(r#""version":null,"mur_actif":false,"foi":"constate"}"#)));
    let verdict = lignes_de(&fil, "main.verdict");
    assert!(verdict[0].ends_with(r#""verdict":"classee","foi":"constate"}"#), "{}", verdict[0]);
    // `lydite etat` le redit : classée, et le mur est constaté inactif même pour une main tuée.
    let etat = essai.kraken().arg("etat").output().unwrap();
    assert_eq!(etat.status.code(), Some(1), "{}", dit(&etat));
    let etat = String::from_utf8_lossy(&etat.stdout).into_owned();
    assert!(etat.contains(" : classée : main : réglages non pris : get_settings : "), "{etat}");
    let sans_recu = etat.trim_end().rsplit_once('\n').map_or("", |(reste, _)| reste);
    assert!(sans_recu.ends_with(" · mur inactif (constaté)"), "{etat}");
    assert!(etat.trim_end().lines().last().unwrap().starts_with("Coût : inconnu · "), "{etat}");
    essai.nettoyer();
}

#[test]
fn le_fil_inscrit_les_reglages_du_depot() {
    let essai = Essai::nouveau("reglages-depot");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    // `.claude/settings.json` est commité à `{}`, sans fin de ligne : aucune conversion de fin de ligne n'en change
    // l'empreinte, le SHA-256 des deux octets (printf '{}' | sha256sum).
    let empreinte = "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a";
    fs::create_dir(essai.depot.join(".claude")).unwrap();
    fs::write(essai.depot.join(".claude").join("settings.json"), "{}").unwrap();
    git(&essai.depot, &["add", ".claude/settings.json"]);
    git(&essai.depot, &["commit", "-q", "-m", "réglages du dépôt"]);
    assert_eq!(essai.lancer_et_verifier("veille").status.code(), Some(0));
    // Chaque lancement porte les réglages que le bac de la main avait au départ.
    let fil = essai.fil();
    let seul = format!(r#""reglages_depot":".claude/settings.json {empreinte}","foi":"declare"}}"#);
    let lancements = lignes_de(&fil, "main.lancement");
    assert_eq!(lancements.len(), 2);
    for ligne in &lancements {
        assert!(ligne.contains(&seul), "{ligne}");
    }
    // Deux fichiers de réglages au dépôt : `settings.json` d'abord, un par ligne ; la liste est une chaîne, ses lignes
    // séparées par `\n`. La fausse claude recommence à sa première tentative.
    fs::write(essai.depot.join(".claude").join("settings.local.json"), "{}").unwrap();
    git(&essai.depot, &["add", ".claude/settings.local.json"]);
    git(&essai.depot, &["commit", "-q", "-m", "réglages locaux"]);
    fs::remove_file(essai.main.join("marque")).unwrap();
    assert_eq!(essai.lancer_et_verifier("veille").status.code(), Some(0));
    let fil = essai.fil();
    let deux = format!(
        concat!(
            r#""reglages_depot":".claude/settings.json {empreinte}\n.claude/settings.local.json {empreinte}","foi":""#,
            r#"declare"}}"#,
        ),
        empreinte = empreinte,
    );
    let lancements = lignes_de(&fil, "main.lancement");
    assert_eq!(lancements.len(), 4);
    for ligne in &lancements[2..] {
        assert!(ligne.contains(&deux), "{ligne}");
    }
    essai.nettoyer();
}

#[test]
fn un_reglage_illisible_arrete_la_vague_avant_la_main() {
    let essai = Essai::nouveau("reglage-illisible");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    // Un dossier à la place de `.claude/settings.json`, au dépôt : chaque bac l'a, et il ne se lit pas comme un
    // fichier.
    let place = essai.depot.join(".claude").join("settings.json");
    fs::create_dir_all(&place).unwrap();
    fs::write(place.join("x"), "x").unwrap();
    git(&essai.depot, &["add", ".claude"]);
    git(&essai.depot, &["commit", "-q", "-m", "un dossier à la place des réglages"]);
    // La vague s'arrête comme sur une note ratée : le refus 7012 est l'issue de la main, refusée, et la vague ne rend
    // pas 0.
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(1), "{}", dit(&sortie));
    let stderr = String::from_utf8_lossy(&sortie.stderr);
    let reglages = essai.bac().join(".claude").join("settings.json");
    assert!(stderr.contains(&format!("refusée : main-{}-1\nrefusée (7012) : ", essai.vague())), "{stderr}");
    let preuve = format!("lecture des réglages {}", reglages.display());
    assert!(stderr.contains(&preuve), "la preuve nomme le fichier : {stderr}");
    // La main n'est pas partie : la vague s'est arrêtée au début de sa tentative, avant son lancement, sans fin ni
    // verdict.
    assert!(essai.note("args.txt").is_none(), "la fausse claude est partie");
    let fil = essai.fil();
    let genres: Vec<&str> = fil.lines().map(genre).collect();
    assert_eq!(genres, ["vague.ouverte", "main.bail", "main.debut"], "{fil}");
    let bacs = fs::read_dir(essai.dossier.join("bacs")).unwrap();
    let tmp: Vec<_> =
        bacs.map(|e| e.unwrap().file_name()).filter(|nom| nom.to_string_lossy().ends_with(".tmp")).collect();
    assert!(tmp.is_empty(), "son dossier temporaire n'a pas été créé : {tmp:?}");
    essai.nettoyer();
}

#[test]
fn l_environnement_de_session_n_atteint_pas_la_main() {
    let essai = Essai::nouveau("env");
    // La vérification dit ce que sa coquille reçoit. Ses caractères sortent de ceux qu'on prête : rien n'en est prêté.
    essai.repondre_juste(&[]);
    essai.profil_accepte("veille", "");
    // Les jetons, les variables de git qui nomment un dépôt, ses objets, la configuration d'un git parent ou son
    // programme, ici ceux du dépôt visé, comme les exporterait un de ses crochets qui lance Kraken, et les moyens de
    // s'authentifier comme l'humain : aucune ne passe, pas plus qu'une marque de session. Leurs valeurs ne gênent pas
    // git, que Kraken lance sous le même environnement.
    let jetons = ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"];
    let git_dir = essai.depot.join(".git");
    let depot = [
        ("GIT_DIR", git_dir.clone()),
        ("GIT_WORK_TREE", essai.depot.clone()),
        ("GIT_INDEX_FILE", git_dir.join("index")),
        ("GIT_COMMON_DIR", git_dir.clone()),
        ("GIT_OBJECT_DIRECTORY", git_dir.join("objects")),
        ("GIT_NAMESPACE", PathBuf::from("essai")),
        ("GIT_ALTERNATE_OBJECT_DIRECTORIES", git_dir.join("objects")),
        ("GIT_CONFIG_PARAMETERS", PathBuf::from("'core.abbrev'='12'")),
        ("GIT_CONFIG_COUNT", PathBuf::from("0")),
        ("GIT_EXEC_PATH", PathBuf::from(git(&essai.depot, &["--exec-path"]))),
    ];
    let authentification = [
        ("GIT_SSH_COMMAND", "ssh-essai"),
        ("GIT_SSH", "ssh-essai"),
        ("GIT_ASKPASS", "askpass-essai"),
        ("SSH_ASKPASS", "askpass-essai"),
        ("SSH_AUTH_SOCK", "agent-essai"),
    ];
    let otees: Vec<&str> = ["CLAUDE_CODE_ESSAI"]
        .into_iter()
        .chain(jetons)
        .chain(depot.iter().map(|(nom, _)| *nom))
        .chain(authentification.iter().map(|(nom, _)| *nom))
        .collect();
    // La connexion de la main à Claude, son dossier de réglages et ANTHROPIC_*, passe à elle seule, pas à sa preuve.
    // Les fichiers de configuration de git de l'humain, vides ici, et MCP_* passent à l'une comme à l'autre.
    let connexion = ["CLAUDE_CONFIG_DIR", "ANTHROPIC_ESSAI"];
    let (global, systeme) = (essai.racine.join("gitconfig-global"), essai.racine.join("gitconfig-systeme"));
    for fichier in [&global, &systeme] {
        fs::write(fichier, "").unwrap();
    }
    let communes = [
        ("MCP_ESSAI", "la".to_string()),
        ("GIT_CONFIG_GLOBAL", global.display().to_string()),
        ("GIT_CONFIG_SYSTEM", systeme.display().to_string()),
    ];
    // Le dossier de réglages que l'essai pose déjà à Kraken, s'il en pose un, sinon un dossier à lui.
    let mut kraken = essai.kraken();
    let config = (kraken.get_envs())
        .find(|(nom, _)| *nom == "CLAUDE_CONFIG_DIR")
        .and_then(|(_, valeur)| valeur)
        .map_or_else(|| essai.racine.join("config"), PathBuf::from);
    // La vérification dit, entre crochets, ce que sa coquille reçoit de chaque nom : rien de ceux que la preuve perd,
    // la valeur des autres. `cmd` laisse telle quelle une variable qui n'est pas posée.
    let lire = |nom: &str| if cfg!(windows) { format!("%{nom}%") } else { format!("${nom}") };
    let rien = |nom: &str| if cfg!(windows) { format!("%{nom}%") } else { String::new() };
    let perdus: Vec<&str> = otees.iter().copied().chain(connexion).collect();
    let lues: String = (perdus.iter().copied())
        .chain(communes.iter().map(|(nom, _)| *nom))
        .map(|nom| format!(" [{}]", lire(nom)))
        .collect();
    let vues = perdus.iter().map(|nom| format!(" [{}]", rien(nom)));
    let attendu: String = vues.chain(communes.iter().map(|(_, valeur)| format!(" [{valeur}]"))).collect();
    let (verifier, vu) = if cfg!(windows) {
        let temp = std::env::var("TEMP").expect("TEMP est posée sous Windows");
        (format!("echo ESSAI{lues} TEMP=%TEMP%"), format!("ESSAI{attendu} TEMP={temp}"))
    } else {
        let tmpdir = std::env::var("TMPDIR").unwrap_or_default();
        (format!(r#"echo "ESSAI{lues} TMPDIR=$TMPDIR""#), format!("ESSAI{attendu} TMPDIR={tmpdir}"))
    };
    // Les marques de session, les jetons et les variables de git sont posés sur Kraken seul ; le dossier de réglages de
    // Claude, ANTHROPIC_*, MCP_* et les fichiers de configuration de git de l'humain font partie de ce qui passe à la
    // main.
    let sortie = kraken
        .env("CLAUDECODE", "1")
        .env("CLAUDE_CODE_ESSAI", "session")
        .env("CLAUDE_EFFORT", "high")
        .envs(jetons.map(|jeton| (jeton, "jeton")))
        .envs(depot)
        .envs(authentification)
        .env("CLAUDE_CONFIG_DIR", &config)
        .env("ANTHROPIC_ESSAI", "ici")
        .envs(communes.clone())
        .args(["lancer", "veille", OBJECTIF, "--verifier", verifier.as_str()])
        .output()
        .unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    // La main : aucune variable CLAUDE* sauf CLAUDE_CONFIG_DIR, aucun jeton, aucun moyen de s'authentifier ni variable
    // de git qui nomme un dépôt, et le reste. Sous Windows, les noms ne tiennent pas à la casse.
    let variables = variables_de_la_main(&essai);
    let session: Vec<&String> =
        variables.keys().filter(|nom| nom.starts_with("CLAUDE") && *nom != "CLAUDE_CONFIG_DIR").collect();
    assert!(session.is_empty(), "des marques de session passent : {session:?}");
    for nom in &otees {
        assert!(!variables.contains_key(*nom), "{nom} passe");
    }
    assert_eq!(variables["CLAUDE_CONFIG_DIR"], config.display().to_string());
    assert_eq!(variables["ANTHROPIC_ESSAI"], "ici");
    for (nom, valeur) in &communes {
        assert_eq!(&variables[*nom], valeur, "{nom}");
    }
    // Son dossier temporaire est le voisin de son bac dans le dossier de Kraken, nommé des 12 premiers chiffres
    // hexadécimaux de l'empreinte FNV-1a sur 64 bits du nom du bac, puis `.tmp` (`bac::temporaire_de`) : pas celui de
    // Kraken.
    let bac = essai.bac();
    let nom = bac.file_name().unwrap().to_string_lossy().into_owned();
    let empreinte =
        nom.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, octet| (h ^ u64::from(octet)).wrapping_mul(0x100_0000_01b3));
    let temporaire = bac.with_file_name(format!("{:012x}.tmp", empreinte >> 16));
    assert!(temporaire.is_dir(), "{}", temporaire.display());
    let noms_du_dossier: &[&str] = if cfg!(windows) { &["TEMP", "TMP"] } else { &["TMPDIR"] };
    for nom in noms_du_dossier {
        assert_eq!(variables[*nom], temporaire.display().to_string(), "{nom}");
    }
    // La preuve que Kraken rejoue perd les mêmes variables et, de plus, la connexion de la main à Claude : ni
    // CLAUDE_CONFIG_DIR ni ANTHROPIC_ESSAI. Elle garde MCP_ESSAI, les fichiers de configuration de git de l'humain et
    // le dossier temporaire de Kraken.
    let fil = essai.fil();
    let fins = lignes_de(&fil, "main.fin");
    assert!(fins[1].contains(&dans_une_chaine(&vu)), "la preuve a reçu : {vu}\n{}", fins[1]);
    essai.nettoyer();
}

/// L'environnement que la fausse claude a écrit pour sa dernière tentative (`env.txt`), les noms en majuscules : sous
/// Windows, ils ne tiennent pas à la casse.
fn variables_de_la_main(essai: &Essai) -> BTreeMap<String, String> {
    let brut = essai.note("env.txt").expect("la fausse claude a écrit son environnement");
    brut.lines()
        .filter_map(|ligne| ligne.split_once('='))
        .map(|(nom, valeur)| (nom.to_ascii_uppercase(), valeur.to_string()))
        .collect()
}

/// Aucun fichier de ce dossier, à toute profondeur, ne porte ce texte.
fn rien_n_ecrit(dossier: &Path, texte: &str) {
    for entree in fs::read_dir(dossier).unwrap() {
        let chemin = entree.unwrap().path();
        if chemin.is_dir() {
            rien_n_ecrit(&chemin, texte);
        } else if let Ok(octets) = fs::read(&chemin) {
            assert!(!String::from_utf8_lossy(&octets).contains(texte), "{} porte « {texte} »", chemin.display());
        }
    }
}

#[test]
fn un_fournisseur_mal_donne_est_refuse_en_6027_a_l_acceptation_comme_au_lancement() {
    // Une clé seule, une URL qui n'est pas en https ou qui porte des identifiants, un nom de variable hors de [A-Z0-9_]
    // : le profil ne s'accepte pas, ne se lance pas, et le refus ne reprend rien de ce qu'il a lu sur la ligne fautive
    // (une clé collée dans `cle`, des identifiants dans `api`). Chaque cas : son nom, ses lignes après la sorte, ce que
    // la preuve dit, et ce que le refus ne doit pas dire.
    let essai = Essai::nouveau("fournisseur-faux");
    let profils = essai.dossier.join("profils");
    fs::create_dir_all(&profils).unwrap();
    let api = format!("api = {API}\n");
    let cle = format!("cle = {VARIABLE}\n");
    let cas: [(&str, String, &str, &str); 7] = [
        ("api-seule", api.clone(), ":2 : « api » est seule : un fournisseur se donne par api et cle ensemble", API),
        (
            "cle-seule",
            cle.clone(),
            ":2 : « cle » est seule : un fournisseur se donne par api et cle ensemble",
            VARIABLE,
        ),
        ("http", format!("api = http://hote-secret.exemple.test/x\n{cle}"), ":2 : « api » : ", "hote-secret"),
        (
            "identifiants",
            format!("api = https://utilisateur:mot-de-passe-secret@api.exemple.test/x\n{cle}"),
            ":2 : « api » : ",
            "mot-de-passe-secret",
        ),
        (
            "requete",
            format!("api = https://api.exemple.test/x?cle=secret-requete\n{cle}"),
            ":2 : « api » : ",
            "secret-requete",
        ),
        ("cle-collee", format!("{api}cle = {LEURRE}\n"), ":3 : « cle » : ", LEURRE),
        ("cle-minuscule", format!("{api}cle = lydite_essai_cle\n"), ":3 : « cle » : ", "lydite_essai_cle"),
    ];
    for (nom, lignes, dans_la_preuve, secret) in cas {
        fs::write(profils.join(format!("{nom}.profil")), format!("sorte = claude\n{lignes}")).unwrap();
        for ligne in [vec!["accepter", nom], vec!["lancer", nom, OBJECTIF]] {
            let refus = lire_refus(essai.kraken().args(&ligne));
            assert_eq!(refus.code, "lydite : 6027 lancer.fournisseur", "{nom} {ligne:?}");
            assert_regle(&refus);
            assert!(refus.preuve.contains(&format!("{nom}.profil{dans_la_preuve}")), "{nom} : {refus:?}");
            assert!(refus.correction.contains(&format!("lydite accepter {nom}")), "{nom} : {refus:?}");
            assert!(!format!("{refus:?}").contains(secret), "le refus reprend {secret} : {refus:?}");
        }
        assert!(!profils.join(format!("{nom}.empreinte")).exists(), "{nom} n'a pas d'empreinte");
    }
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn une_cle_absente_ou_vide_ne_lance_rien() {
    // Le profil est juste et accepté (`accepter` ne lit pas la clé), mais la variable que `cle` nomme n'est pas posée,
    // ou vide, ou blanche : le refus 6027 au lancement, avant tout bac et tout fil, qui nomme la variable sans dire de
    // valeur. La fausse claude, prête, ne part pas.
    let essai = Essai::nouveau("cle-absente");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", &format!("api = {API}\ncle = {VARIABLE}\n"));
    for valeur in [None, Some(""), Some("   ")] {
        let mut commande = essai.kraken();
        commande.args(["lancer", "veille", OBJECTIF, "--verifier", VERIFIER]);
        match valeur {
            Some(valeur) => commande.env(VARIABLE, valeur),
            None => commande.env_remove(VARIABLE),
        };
        let refus = lire_refus(&mut commande);
        assert_eq!(refus.code, "lydite : 6027 lancer.fournisseur", "{valeur:?}");
        assert_regle(&refus);
        let motif = if valeur.is_none() { "n'est pas posée" } else { "est vide" };
        let dit = format!("la variable d'environnement {VARIABLE}, que cle nomme, {motif}");
        assert!(refus.preuve.ends_with(&dit), "{valeur:?} : {refus:?}");
        assert!(refus.correction.starts_with(&format!("Pose {VARIABLE} dans l'environnement de Lydite")), "{refus:?}");
        assert!(refus.correction.contains(HOTE), "la correction nomme le fournisseur : {refus:?}");
    }
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn la_main_d_un_fournisseur_recoit_son_url_et_sa_cle_et_sa_preuve_ne_les_recoit_pas() {
    let essai = Essai::nouveau("fournisseur");
    // La preuve dit ce que sa coquille reçoit. Ses caractères sortent de ceux qu'on prête : rien n'en est prêté.
    essai.repondre_juste(&[]);
    let (_, empreinte) = essai.profil_accepte("veille", &format!("api = {API}\ncle = {VARIABLE}\n"));
    // Le jeton, la clé et l'URL d'Anthropic de l'humain, et la clé du fournisseur dans sa variable, sont sur Kraken
    // seul. Elle dit, entre crochets, ce que sa coquille reçoit de chaque nom : rien, `cmd` laissant telle quelle une
    // variable qui n'est pas posée.
    let noms = ["ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", VARIABLE];
    let lire = |nom: &str| if cfg!(windows) { format!("%{nom}%") } else { format!("${nom}") };
    let rien = |nom: &str| if cfg!(windows) { format!("%{nom}%") } else { String::new() };
    let lues: String = noms.iter().map(|nom| format!(" [{}]", lire(nom))).collect();
    let vues: String = noms.iter().map(|nom| format!(" [{}]", rien(nom))).collect();
    let verifier = if cfg!(windows) { format!("echo ESSAI{lues}") } else { format!(r#"echo "ESSAI{lues}""#) };
    let vu = format!("ESSAI{vues}");
    let lancer = ["lancer", "veille", OBJECTIF, "--verifier", verifier.as_str()];
    let sortie = essai.kraken().envs(HUMAIN).env(VARIABLE, LEURRE).args(lancer).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    // La main : l'URL et la clé du fournisseur à la place de celles de l'humain, et le jeton hérité retiré.
    let variables = variables_de_la_main(&essai);
    assert_eq!(variables["ANTHROPIC_BASE_URL"], API);
    assert_eq!(variables["ANTHROPIC_API_KEY"], LEURRE);
    assert!(!variables.contains_key(VARIABLE), "la main hérite de la variable que cle nomme");
    assert!(!variables.contains_key("ANTHROPIC_AUTH_TOKEN"), "le jeton de l'humain va chez le fournisseur");
    // Sa preuve n'a reçu ni l'une ni l'autre, ni la variable qui tient la clé.
    let fil = essai.fil();
    let fins = lignes_de(&fil, "main.fin");
    assert!(fins[1].contains(&dans_une_chaine(&vu)), "la preuve a reçu : {vu}\n{}", fins[1]);
    // Au fil, dans chaque tentative, le fournisseur déclaré suit le lancement ; il porte l'origine de la séance, comme
    // lui.
    let genres: Vec<&str> = fil.lines().map(genre).collect();
    let tentative = ["main.bail", "main.debut", "main.lancement", "main.fournisseur", "main.constat", "main.fin"];
    let attendu: Vec<&str> =
        ["vague.ouverte"].into_iter().chain(tentative).chain(tentative).chain(["main.verdict"]).collect();
    assert_eq!(genres, attendu, "{fil}");
    let fournisseurs = lignes_de(&fil, "main.fournisseur");
    assert_eq!(fournisseurs.len(), 2, "{fil}");
    for (ligne, tentative) in fournisseurs.iter().zip(1..) {
        let champs = [
            r#""origine":"humain","origine_foi":"declare","type":"main.fournisseur""#.to_string(),
            format!(r#""main":1,"tentative":{tentative},"fournisseur":"{HOTE}","foi":"declare"}}"#),
        ];
        for champ in champs {
            assert!(ligne.contains(&champ), "{champ} manque : {ligne}");
        }
    }
    // L'empreinte du lancement est celle du profil accepté : le nom de la variable y est, sa valeur n'y est pas.
    for ligne in lignes_de(&fil, "main.lancement") {
        assert!(ligne.contains(&format!(r#""empreinte":"{empreinte}""#)), "{ligne}");
    }
    // La clé n'est écrite nulle part de ce que Kraken écrit : ni au fil, ni dans son dossier, ni sur ses sorties.
    rien_n_ecrit(&essai.dossier, LEURRE);
    let (stdout, stderr) = (String::from_utf8_lossy(&sortie.stdout), String::from_utf8_lossy(&sortie.stderr));
    assert!(!stdout.contains(LEURRE) && !stderr.contains(LEURRE), "{}", dit(&sortie));
    // `lydite etat` dit le fournisseur, déclaré : une main déclarée `haiku` sur lui ne passe pas pour du Claude.
    let etat = essai.kraken().arg("etat").output().unwrap();
    assert_eq!(etat.status.code(), Some(0), "{}", dit(&etat));
    let ligne = format!("main-{}-1 : prise · mur inactif (constaté) · fournisseur {HOTE} (déclaré)", essai.vague());
    let attendu = format!("{OBJECTIF}\n{ligne}\nCoût : inconnu · 2 mains, 1 relance, 0 classée");
    assert_eq!(etat_sans_duree(&etat), attendu);
    // Une autre clé sous le même nom ne change pas le profil : il repart sans être réaccepté, avec la clé du moment.
    fs::remove_file(essai.main.join("marque")).unwrap();
    let sortie = essai.kraken().envs(HUMAIN).env(VARIABLE, "une-autre-cle").args(lancer).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    assert_eq!(variables_de_la_main(&essai)["ANTHROPIC_API_KEY"], "une-autre-cle");
    essai.nettoyer();
}

#[test]
fn la_variable_de_la_cle_peut_etre_une_variable_anthropic_et_la_main_n_a_que_la_cle_du_fournisseur() {
    // La documentation d'un fournisseur met souvent sa clé dans `ANTHROPIC_AUTH_TOKEN`, ou dans `ANTHROPIC_API_KEY` :
    // la clé qu'on lit n'est pas retirée avec le jeton hérité, et la main la reçoit sous `ANTHROPIC_API_KEY`, seule.
    for variable in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
        let essai = Essai::nouveau(&format!("fournisseur-{}", variable.to_ascii_lowercase()));
        essai.repondre_juste(&PRET);
        essai.profil_accepte("veille", &format!("api = {API}\ncle = {variable}\n"));
        let lancer = ["lancer", "veille", OBJECTIF, "--verifier", VERIFIER];
        let url = ("ANTHROPIC_BASE_URL", "https://humain.exemple.test");
        let sortie = essai.kraken().env(variable, LEURRE).env(url.0, url.1).args(lancer).output().unwrap();
        assert_eq!(sortie.status.code(), Some(0), "{variable} : {}", dit(&sortie));
        let variables = variables_de_la_main(&essai);
        assert_eq!(variables["ANTHROPIC_BASE_URL"], API, "{variable}");
        assert_eq!(variables["ANTHROPIC_API_KEY"], LEURRE, "{variable}");
        assert!(!variables.contains_key("ANTHROPIC_AUTH_TOKEN"), "{variable}");
        rien_n_ecrit(&essai.dossier, LEURRE);
        essai.nettoyer();
    }
}

#[test]
fn sans_fournisseur_la_main_garde_les_variables_de_l_humain_et_le_fil_n_a_pas_de_fournisseur() {
    // Sans `api` ni `cle`, rien ne change : la main reçoit le jeton, la clé et l'URL d'Anthropic tels que Kraken les a,
    // la variable que le profil ne nomme pas passe comme les autres, et le fil n'a ni fournisseur ni mot de fournisseur
    // à `lydite etat`.
    let essai = Essai::nouveau("sans-fournisseur");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    let lancer = ["lancer", "veille", OBJECTIF, "--verifier", VERIFIER];
    let sortie = essai.kraken().envs(HUMAIN).env(VARIABLE, LEURRE).args(lancer).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let variables = variables_de_la_main(&essai);
    for (nom, valeur) in HUMAIN {
        assert_eq!(variables[nom], valeur, "{nom}");
    }
    assert_eq!(variables[VARIABLE], LEURRE);
    let fil = essai.fil();
    assert!(lignes_de(&fil, "main.fournisseur").is_empty(), "{fil}");
    let etat = essai.kraken().arg("etat").output().unwrap();
    assert!(!String::from_utf8_lossy(&etat.stdout).contains("fournisseur"), "{}", dit(&etat));
    essai.nettoyer();
}

#[test]
fn la_main_porte_l_identite_de_kraken_et_non_celle_de_l_humain() {
    let essai = Essai::nouveau("identite");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    // Celle de l'humain, telle que son shell l'exporterait : posée sur Kraken seul, elle ne doit pas être celle des
    // commits de la main.
    let humain = [
        ("GIT_AUTHOR_NAME", "humain"),
        ("GIT_AUTHOR_EMAIL", "humain@exemple.test"),
        ("GIT_COMMITTER_NAME", "humain"),
        ("GIT_COMMITTER_EMAIL", "humain@exemple.test"),
    ];
    let lancer = ["lancer", "veille", OBJECTIF, "--verifier", VERIFIER];
    let sortie = essai.kraken().envs(humain).args(lancer).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    // L'identité de Kraken, celle qu'il donne à chacun de ses gestes git, écrite ici à la main.
    let kraken = [
        ("GIT_AUTHOR_NAME", "lydite"),
        ("GIT_AUTHOR_EMAIL", "lydite@invalid"),
        ("GIT_COMMITTER_NAME", "lydite"),
        ("GIT_COMMITTER_EMAIL", "lydite@invalid"),
    ];
    let variables = variables_de_la_main(&essai);
    for (nom, valeur) in kraken {
        assert_eq!(variables[nom], valeur, "{nom}");
    }
    essai.nettoyer();
}

#[test]
fn un_git_dir_herite_ne_donne_en_lecture_que_le_git_commun_du_depot_vise() {
    let essai = Essai::nouveau("git-dir");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    // L'autre dépôt, que `GIT_DIR` nomme à Kraken sur l'enfant seul, comme le ferait un crochet ou un alias `!` de git
    // qui le lance : hérité par le `git rev-parse` des racines, il donnerait son git commun pour celui du dépôt visé,
    // et `borner` le donnerait en lecture à la main.
    let autre = essai.racine.join("autre-depot");
    fs::create_dir(&autre).unwrap();
    git(&autre, &["init", "-q"]);
    git(&autre, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let lancer = ["lancer", "veille", OBJECTIF, "--verifier", VERIFIER];
    let sortie = essai.kraken().env("GIT_DIR", autre.join(".git")).args(lancer).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    // Chaque lancement donne en lecture le bac et le git commun du dépôt visé, tel que git le dit hors de tout GIT_DIR,
    // et le fil ne nomme rien de l'autre dépôt, qui n'a rien reçu.
    let (allow, deny) = regles(&essai, &RESEAU, &[], &PRET);
    let reglages = essai.reglages_attendus(false, &EXCLUES, &RESEAU, &allow, &deny);
    let fil = essai.fil();
    let lancements = lignes_de(&fil, "main.lancement");
    assert_eq!(lancements.len(), 2, "{fil}");
    for ligne in lancements {
        assert!(ligne.contains(&format!(r#""reglages":{}"#, json_chaine(&reglages))), "{reglages}\n{ligne}");
    }
    assert!(!fil.contains("autre-depot"), "le fil nomme l'autre dépôt : {fil}");
    depot_intact(&autre, &[]);
    essai.nettoyer();
}

#[test]
fn un_profil_qui_change_pendant_la_vague_n_est_pas_relu_avant_le_lancement_suivant() {
    // Toutes les mains d'un lancement partent du profil lu à ce moment-là. La fausse claude ajoute une ligne au fichier
    // du profil à chacune de ses tentatives : les deux lancements portent pourtant l'empreinte acceptée.
    let essai = Essai::nouveau("profil-change");
    essai.repondre_juste(&PRET);
    let (texte, empreinte) = essai.profil_accepte("veille", "");
    fs::write(essai.main.join("changer-le-profil"), "").unwrap();
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let fichier = essai.dossier.join("profils").join("veille.profil");
    let change = fs::read_to_string(&fichier).unwrap().replace("\r\n", "\n");
    assert_eq!(change, format!("{texte}# change\n# change\n"), "le fichier a changé à chaque tentative");
    let fil = essai.fil();
    let lancements = lignes_de(&fil, "main.lancement");
    assert_eq!(lancements.len(), 2);
    for ligne in lancements {
        assert!(ligne.contains(&format!(r#""empreinte":"{empreinte}""#)), "{ligne}");
    }
    // Le lancement suivant le lit : l'empreinte a changé, il est refusé en 6012, et la fausse claude ne repart pas.
    let args = essai.note("args.txt");
    let refus = lire_refus(essai.kraken().args(["lancer", "veille", OBJECTIF, "--verifier", VERIFIER]));
    assert_eq!(refus.code, "lydite : 6012 lancer.profil_change");
    assert_eq!(essai.note("args.txt"), args);
    essai.nettoyer();
}

#[test]
fn kraken_etat_dit_le_mur() {
    let essai = Essai::nouveau("etat");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    assert_eq!(essai.lancer_et_verifier("veille").status.code(), Some(0));
    // Claude Code a dit son mur inactif : `lydite etat` le dit constaté, et la vague prise donne le code 0.
    let etat = essai.kraken().arg("etat").output().unwrap();
    assert_eq!(etat.status.code(), Some(0), "{}", dit(&etat));
    let attendu = format!(
        "{OBJECTIF}\nmain-{}-1 : prise · mur inactif (constaté)\nCoût : inconnu · 2 mains, 1 relance, 0 classée",
        essai.vague()
    );
    assert_eq!(etat_sans_duree(&etat), attendu);
    essai.nettoyer();
}

/// Le SHA-256 du texte du profil intégré, écrit ici à la main : sa seule source est le texte du fichier de profil.rs.
/// printf 'sorte = claude\nmodele = sonnet\nrelance = sonnet\nmur = demande\ndelai_main = 3600\ndelai_preuve = 300\n' |
/// sha256sum
const EMPREINTE_INTEGREE: &str = "ee807c6e2a903e1409af8950a3e9adfc9ee30326360b0bec5b8f4c5d485e6704";

#[test]
fn le_profil_integre_part_sur_sonnet_et_s_inscrit_sans_etre_garde() {
    let essai = Essai::nouveau("integre");
    // Sans fichier, `claude` est le profil intégré, dont le programme est `LYDITE_CLAUDE` : ici la fausse claude. La
    // main par défaut part sur Sonnet, non sur le modèle par défaut du compte (Opus), à ses deux tentatives, sans
    // réseau.
    let (allow, deny) = regles(&essai, &[], &[], &PRET);
    for (n, resultat) in [(1, ECHEC), (2, REUSSITE)] {
        essai.repondre(n, &reponses("claude-sonnet-5-5", false, &EXCLUES, false, &allow, &deny));
        essai.conclure(n, "claude-sonnet-5-5", resultat);
    }
    let lancer = ["lancer", "claude", OBJECTIF, "--verifier", VERIFIER];
    let sortie = essai.kraken().env("LYDITE_CLAUDE", essai.fausse_claude()).args(lancer).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let reglages = essai.reglages_attendus(false, &EXCLUES, &[], &allow, &deny);
    assert_eq!(essai.argv_recus(), [argv_attendu("sonnet", None, &reglages), argv_attendu("sonnet", None, &reglages)]);
    // Le lancement inscrit le profil intégré avec l'empreinte de son texte, jamais gardée : aucun profil n'a été écrit.
    let fil = essai.fil();
    let lancements = lignes_de(&fil, "main.lancement");
    assert_eq!(lancements.len(), 2);
    let integre = format!(r#""profil":"claude","empreinte":"{EMPREINTE_INTEGREE}","modele":"sonnet","mur":"demande""#);
    for ligne in lancements {
        assert!(ligne.contains(&integre), "{ligne}");
    }
    assert_eq!(noms(&essai.dossier), ["bacs", "fil.jsonl"], "ni profil ni empreinte n'est écrit");
    essai.nettoyer();
}

#[test]
fn un_mur_exige_se_constate_actif_ou_la_main_est_tuee() {
    // Le profil exige le mur (`failIfUnavailable`) : la vérification prêtée reste dans le bac à sable, aucune commande
    // n'en sort. Claude Code le dit actif : la main est prise, le mur constaté actif.
    let essai = Essai::nouveau("mur-actif");
    essai.profil_accepte("veille", "mur = exige\n");
    let (allow, deny) = regles(&essai, &RESEAU, &[], &PRET);
    essai.repondre_a(true, &[], true, &allow, &deny);
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let reglages = essai.reglages_attendus(true, &[], &RESEAU, &allow, &deny);
    assert_eq!(essai.reglages_recus(), [reglages.clone(), reglages]);
    let fil = essai.fil();
    for ligne in lignes_de(&fil, "main.lancement") {
        assert!(ligne.contains(r#""mur":"exige""#), "{ligne}");
    }
    for ligne in lignes_de(&fil, "main.constat") {
        assert!(ligne.contains(r#""mur_actif":true,"foi":"constate"}"#), "{ligne}");
    }
    let etat = essai.kraken().arg("etat").output().unwrap();
    assert_eq!(etat.status.code(), Some(0), "{}", dit(&etat));
    let attendu = format!(
        "{OBJECTIF}\nmain-{}-1 : prise · mur actif (constaté)\nCoût : inconnu · 2 mains, 1 relance, 0 classée",
        essai.vague()
    );
    assert_eq!(etat_sans_duree(&etat), attendu);
    essai.nettoyer();

    // Il le dit inactif, alors que le profil l'exige : l'écart tue la main avant sa consigne, à chaque tentative. Le
    // mur inactif se constate quand même, et la main est classée.
    let essai = Essai::nouveau("mur-inactif");
    essai.profil_accepte("veille", "mur = exige\n");
    let (allow, deny) = regles(&essai, &RESEAU, &[], &PRET);
    essai.repondre_a(true, &[], false, &allow, &deny);
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(1), "{}", dit(&sortie));
    let fil = essai.fil();
    let fins = lignes_de(&fil, "main.fin");
    assert_eq!(fins.len(), 2, "{fil}");
    let ecart = "main : réglages non pris : get_sandbox_dialog : enabled false, déclaré exigé (failIfUnavailable true)";
    for ligne in fins {
        // La mise à mort peut suivre l'écart (« ␣; mise à mort : … »), si la main a fini avant d'être tuée.
        let debut = format!(r#""couleur":"rouge","foi":"constate","texte":"{ecart}"#);
        let fermee = format!("{debut}\"");
        let tuee = format!("{debut} ; mise à mort : ");
        assert!(ligne.contains(&fermee) || ligne.contains(&tuee), "{ligne}");
    }
    for ligne in lignes_de(&fil, "main.constat") {
        assert!(ligne.contains(r#""version":null,"mur_actif":false,"foi":"constate"}"#), "{ligne}");
    }
    assert!(essai.entree().iter().all(|ligne| !ligne.contains(r#""type":"user""#)), "{:?}", essai.entree());
    let etat = essai.kraken().arg("etat").output().unwrap();
    assert_eq!(etat.status.code(), Some(1), "{}", dit(&etat));
    let etat = etat_sans_duree(&etat);
    let attendu = " · mur inactif (constaté)\nCoût : inconnu · 2 mains, 1 relance, 1 classée";
    assert!(etat.contains(ecart) && etat.ends_with(attendu), "{etat}");
    essai.nettoyer();
}

#[test]
fn un_profil_declare_ses_sortants_et_son_mcp_a_la_fausse_claude() {
    let essai = Essai::nouveau("outils");
    let mcp = r#"{"mcpServers":{"docs":{"command":"docs-mcp"}}}"#;
    // Les sortants se déclarent dans l'ordre qu'on veut. Ils passent de `deny` à `allow` dans l'ordre du contrat,
    // suivis de leurs serveurs, et la configuration MCP part telle quelle, juste après `--strict-mcp-config`.
    essai.profil_accepte("veille", &format!("outils = ListAgents, SendMessage, mcp__docs\nmcp = {mcp}\n"));
    let (allow, deny) = regles(&essai, &RESEAU, &["ListAgents", "SendMessage", "mcp__docs"], &PRET);
    // L'ouverture porte le serveur que `mcp` déclare, encore en attente : il passe, quel que soit son statut.
    let attente = r#""mcp_servers":[{"name":"docs","status":"pending"}]"#;
    let ouverture = OUVERTURE.replace(r#""mcp_servers":[]"#, attente);
    for (n, modele, resultat) in [(1, "claude-haiku-5-5", ECHEC), (2, "claude-sonnet-5-5", REUSSITE)] {
        essai.repondre(n, &reponses(modele, false, &EXCLUES, false, &allow, &deny));
        essai.conclure_par(n, &ouverture.replace("<modele>", modele), resultat);
    }
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let reglages = essai.reglages_attendus(false, &EXCLUES, &RESEAU, &allow, &deny);
    let recus = essai.argv_recus();
    assert_eq!(recus.len(), 2, "{recus:?}");
    for (argv, modele) in recus.iter().zip(["haiku", "sonnet"]) {
        assert_eq!(argv, &argv_attendu(modele, Some(mcp), &reglages), "tentative sur {modele}");
    }
    // Dans l'ordre du contrat : SendMessage avant ListAgents, leur serveur ensuite, puis le prêt de la vérification ;
    // les deux sortants ne sont plus fermés.
    let permis =
        r#""ExitPlanMode","WebFetch(domain:exemple.test)","SendMessage","ListAgents","mcp__docs","Bash(git status)""#;
    assert!(reglages.contains(permis), "{reglages}");
    let ferme = r#""RemoteTrigger","PushNotification","SendUserFile""#;
    assert!(reglages.contains(ferme), "{reglages}");
    essai.nettoyer();
}

#[test]
fn le_fichier_de_connexion_de_claude_est_ferme_aux_outils_de_la_main() {
    // Kraken tire les dossiers de connexion de Claude de ce que `Essai::kraken` pose sur l'enfant : `<racine>/config`
    // pour `CLAUDE_CONFIG_DIR`, puis le `.claude` de `<racine>/personnel`, le dossier personnel. Rien n'y est lu.
    let essai = Essai::nouveau("connexion");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    let sortie = essai.lancer_et_verifier("veille");
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    // Chaque `main.lancement` dit, dans ses `reglages`, `Read` puis `Edit` du fichier de connexion de chacun, dans cet
    // ordre, juste après les gardes du dossier de Kraken, puis l'Edit du git commun au bout de deny ; et aucun autre
    // chemin de connexion.
    let (config, claude) = (regle_du_dossier(&essai.config()), regle_du_dossier(&essai.personnel().join(".claude")));
    let commun = git(&essai.depot, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let fin = [
        format!("Edit({}/srt-*.json)", regle_du_dossier(&essai.dossier)),
        format!("Read({config}/.credentials.json)"),
        format!("Edit({config}/.credentials.json)"),
        format!("Read({claude}/.credentials.json)"),
        format!("Edit({claude}/.credentials.json)"),
        format!("Edit({}/**)", regle_du_dossier(Path::new(&commun))),
    ];
    let attendu = dans_une_chaine(&format!("{}}}", &tableau(&fin)[1..]));
    let fil = essai.fil();
    let lancements = lignes_de(&fil, "main.lancement");
    assert_eq!(lancements.len(), 2, "{fil}");
    for ligne in lancements {
        assert!(ligne.contains(&attendu), "{attendu}\n{ligne}");
        assert_eq!(ligne.matches(".credentials.json").count(), 4, "{ligne}");
    }
    essai.nettoyer();
}

#[test]
fn un_dossier_de_connexion_au_chemin_refuse_ne_lance_rien() {
    // `CLAUDE_CONFIG_DIR` relatif, ou à un chemin qu'une règle de permission ne sait pas écrire : le refus 6023 au
    // lancement, avant tout bac et tout fil, qui nomme le chemin et dit de poser CLAUDE_CONFIG_DIR autrement. Le profil
    // est accepté et sa fausse main prête : elle ne part pas.
    let essai = Essai::nouveau("connexion-refusee");
    essai.repondre_juste(&PRET);
    essai.profil_accepte("veille", "");
    let lancer = ["lancer", "veille", OBJECTIF, "--verifier", VERIFIER];
    for config in [PathBuf::from("relatif"), essai.racine.join("c(1)")] {
        let refus = lire_refus(essai.kraken().env("CLAUDE_CONFIG_DIR", &config).args(lancer));
        assert_eq!(refus.code, "lydite : 6023 lancer.dossier_chemin_refuse", "{}", config.display());
        assert!(refus.regle.contains("son dossier ou un dossier de connexion de Claude"), "{refus:?}");
        assert!(refus.preuve.contains(&config.display().to_string()), "la preuve nomme le chemin : {refus:?}");
        assert!(refus.correction.starts_with("Pose CLAUDE_CONFIG_DIR"), "{refus:?}");
    }
    // Un dossier personnel à un tel chemin, sans CLAUDE_CONFIG_DIR : son `.claude` est nommé, et la correction dit de
    // poser CLAUDE_CONFIG_DIR, d'y connecter claude, puis de retirer l'ancien fichier de connexion.
    let personnel = essai.racine.join("compte (perso)");
    let mut sans_config = essai.kraken();
    sans_config.env_remove("CLAUDE_CONFIG_DIR").env("USERPROFILE", &personnel).env("HOME", &personnel);
    let refus = lire_refus(sans_config.args(lancer));
    assert_eq!(refus.code, "lydite : 6023 lancer.dossier_chemin_refuse");
    assert!(refus.preuve.contains(&personnel.join(".claude").display().to_string()), "{refus:?}");
    for etape in ["Pose CLAUDE_CONFIG_DIR", "connecte claude", "retire l'ancien fichier de connexion"] {
        assert!(refus.correction.contains(etape), "{etape} : {refus:?}");
    }
    // Avec CLAUDE_CONFIG_DIR, il n'est refusé que s'il garde un fichier de connexion, et la correction dit alors de le
    // retirer ou de le déplacer, puisque la connexion vit dans CLAUDE_CONFIG_DIR.
    fs::create_dir_all(personnel.join(".claude")).unwrap();
    fs::write(personnel.join(".claude").join(".credentials.json"), "").unwrap();
    let refus = lire_refus(essai.kraken().env("USERPROFILE", &personnel).env("HOME", &personnel).args(lancer));
    assert_eq!(refus.code, "lydite : 6023 lancer.dossier_chemin_refuse");
    assert!(refus.preuve.contains(&personnel.join(".claude").display().to_string()), "{refus:?}");
    for etape in ["Retire du .claude du dossier personnel", "ou déplace-le", "dans CLAUDE_CONFIG_DIR"] {
        assert!(refus.correction.contains(etape), "{etape} : {refus:?}");
    }
    // Une lecture qui recouvre un dossier de connexion, ici par `~/` : le refus 6009 au lancement, qui nomme sa ligne.
    essai.profil_accepte("lecteur", "lecture = ~/.claude\n");
    let refus = lire_refus(essai.kraken().args(["lancer", "lecteur", OBJECTIF, "--verifier", VERIFIER]));
    assert_eq!(refus.code, "lydite : 6009 lancer.profil_ligne_refusee");
    assert!(refus.preuve.contains("« lecture = ~/.claude » : une lecture ne recouvre pas"), "{refus:?}");
    assert!(refus.preuve.ends_with("et ~/.claude le recouvre"), "{refus:?}");
    essai.rien_ne_part();
    essai.nettoyer();
}

#[test]
fn sans_verification_la_prise_d_une_main_declaree_reste_declaree() {
    // Sans `--verifier`, la main seule dit `PREUVE: vert` : Kraken ne rejoue rien. La prise est déclarée, jamais donnée
    // pour un fait, et ne donne pas le code 0.
    let essai = Essai::nouveau("declaree");
    essai.repondre_juste(&[]);
    essai.profil_accepte("veille", "");
    let sortie = essai.lancer("veille", &[]);
    assert_eq!(sortie.status.code(), Some(1), "{}", dit(&sortie));
    let (stdout, stderr) = (String::from_utf8_lossy(&sortie.stdout), String::from_utf8_lossy(&sortie.stderr));
    assert_eq!(stdout.trim_end(), "bonjour.txt ecrit\nPREUVE: vert");
    let vague = essai.vague();
    assert!(stderr.contains(&format!("pris, déclaré : main-{vague}-1")), "{}", dit(&sortie));
    // Sans commande à jouer, rien n'est prêté, et la consigne demande à la main sa ligne de preuve.
    let (allow, deny) = regles(&essai, &RESEAU, &[], &[]);
    let reglages = essai.reglages_attendus(false, &[], &RESEAU, &allow, &deny);
    assert_eq!(essai.reglages_recus(), [reglages.clone(), reglages]);
    assert!(essai.entree()[4].contains("PREUVE: vert"), "{}", essai.entree()[4]);
    // La fin verte est déclarée, sans vérification, et porte l'origine de sa main ; le verdict est déclaré aussi.
    let fil = essai.fil();
    let fins = lignes_de(&fil, "main.fin");
    let origine = format!(r#""origine":"agent:main-{vague}-1","origine_foi":"declare","type":"main.fin""#);
    let vert = r#""couleur":"vert","foi":"declare","texte":"bonjour.txt ecrit\nPREUVE: vert","verif":null}"#;
    assert!(fins[1].contains(&origine) && fins[1].contains(vert), "{}", fins[1]);
    let verdict = lignes_de(&fil, "main.verdict");
    assert!(verdict[0].ends_with(r#""verdict":"prise","foi":"declare"}"#), "{}", verdict[0]);
    // `lydite etat` dit la prise déclarée, avec le mur constaté, et ne donne pas le code 0.
    let etat = essai.kraken().arg("etat").output().unwrap();
    assert_eq!(etat.status.code(), Some(1), "{}", dit(&etat));
    let ligne = format!("main-{vague}-1 : prise déclarée, non prouvée · mur inactif (constaté)");
    let attendu = format!("{OBJECTIF}\n{ligne}\nCoût : inconnu · 2 mains, 1 relance, 0 classée");
    assert_eq!(etat_sans_duree(&etat), attendu);
    essai.nettoyer();
}
