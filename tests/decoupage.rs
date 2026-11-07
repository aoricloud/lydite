//! Le découpage, joué sur le binaire lui-même : `kraken lancer <profil> "<objectif>" --decouper --verifier "<preuve>"`.
//! Chaque essai lance `kraken` sur un dépôt git jetable qui a un commit, avec un dossier de Kraken jetable hors de lui
//! et une fausse `claude` que le profil d'essai nomme : `fausse-claude.cmd` sous Windows, `fausse-claude.sh` ailleurs.
//! C'est Kraken qui la lance. Elle parle en stream-json : elle répond aux quatre demandes de contrôle par
//! `controle.jsonl`, recopie son entrée dans son dossier temporaire, puis :
//!
//! - si sa consigne nomme `PLAN.kraken`, elle est le découpeur, à sa tentative `n` (1, puis 2 une fois posée la marque
//!   `decoupe-1`) : elle garde sa consigne dans `consigne-decoupeur-<n>.jsonl` et son `PATH` dans
//!   `path-decoupeur-<n>.txt`, copie `plan-<n>.kraken` en `PLAN.kraken` à la racine de son bac, puis joue elle-même
//!   `kraken plan --decoupe PLAN.kraken`, que son `PATH` doit trouver, et en garde la sortie dans `preuve-<n>.txt` et
//!   le code dans `code-<n>.txt` ;
//! - si sa consigne nomme `REMARQUES.kraken`, elle est le relecteur : elle pose la marque `relecteur`, garde le
//!   `DIFF.kraken` de son bac dans `diff.txt`, et copie en `REMARQUES.kraken` le `remarques.kraken` que l'essai a posé
//!   à côté d'elle (`remarques 1`, aucune remarque) ;
//! - sinon, elle est la main d'une tâche : elle garde son `PATH` dans `path-<bac>.txt`, et écrit sa consigne dans
//!   `sortie/<bac>.txt` de son bac, que `fichiers : sortie/` couvre.
//!
//! Elle finit par l'ouverture et le résultat de `fin.jsonl`. Tout ce qu'elle garde est à côté d'elle.
//!
//! Les essais de `--issue` (M7) ont de plus un distant `origin` nu et local, et un faux `gh` ([`faux_gh`]) que
//! `KRAKEN_GH` nomme : il rend l'issue que l'essai pose, et suit la PR de la vague jusqu'à une CI verte.
//!
//! Le profil d'essai exige le mur et donne `haiku` à toutes ses mains, découpeur et relecteur compris (`libre`, le même
//! au mur seulement demandé) : les réponses de contrôle sont les mêmes pour toutes, sans `excludedCommands`, et la
//! liste des règles qu'elles disent porte celles de chacune, dont le prêt de sa preuve. Sous ce mur, Kraken rejoue
//! toute preuve par le faux srt que `KRAKEN_SRT` nomme, à la racine de l'essai. Aucun test ne pose de variable dans son
//! propre processus (l'environnement va à l'enfant seul, par `Command::env`), ne lance un vrai `claude` ou un vrai
//! `gh`, ni ne cherche à sortir d'un bac.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// L'objectif et la preuve que l'humain donne dans ces essais.
const OBJECTIF: &str = "Ecrire deux fichiers";
const VERIFIER: &str = "git status";

/// La preuve que Kraken prête au découpeur.
const PREUVE: &str = "kraken plan --decoupe PLAN.kraken";

/// La preuve que Kraken prête au relecteur.
const PREUVE_DU_RELECTEUR: &str = "kraken remarques REMARQUES.kraken";

/// L'URL d'`origin` dans les essais d'issue : `forge.test` ne se résout jamais, et `insteadOf` la renvoie au dépôt nu.
const URL: &str = "https://forge.test/p/d";

/// L'issue des essais, son titre et son corps.
const ISSUE: u64 = 7;
const TITRE: &str = "Ajouter deux fichiers de sortie";
const CORPS: &str = "Le corps de cette issue demande deux fichiers sous sortie.";

/// Les sortants que le profil ferme par défaut, dans l'ordre du contrat.
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

// Les lignes de Claude Code relevées le 1/11, réduites aux champs que Kraken lit, pour un mur exigé et actif, sans
// réseau et au modèle `haiku`.
const GET_SETTINGS: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_settings","response":{"applied":"#,
    r#"{"model":"haiku"},"effective":{"disableAllHooks":true,"autoMemoryEnabled":false,"sandbox":{"enabled":true,"fa"#,
    r#"ilIfUnavailable":true,"allowUnsandboxedCommands":false,"network":{"allowedDomains":[],"strictAllowlist":true}"#,
    r#"}},"sources":[{"source":"userSettings","settings":{}},{"source":"flagSettings","settings":{}}]}}}"#,
);
const GET_HOOKS_LISTING: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_hooks_listing","response":{"poli"#,
    r#"cy":{"disabledByPolicy":false,"managedOnly":false,"pluginOnly":false,"allDisabled":true,"policyHookCount":0},"#,
    r#""hooks":[],"events":[],"eventCatalog":[]}}}"#,
);
const GET_SANDBOX_DIALOG: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_sandbox_dialog","response":{"sup"#,
    r#"ported":true,"enabled":true,"enabled_in_settings":true,"mode":"regular"}}}"#,
);
const OUVERTURE: &str = concat!(
    r#"{"type":"system","subtype":"init","cwd":"bac","tools":["Task","Bash","Edit","Read","Skill","WebFetch","Workfl"#,
    r#"ow","Write"],"mcp_servers":[],"model":"haiku","permissionMode":"acceptEdits","claude_code_version":"9.9.9-fau"#,
    r#"sse"}"#,
);
const RESULTAT: &str =
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"fait\nPREUVE: vert"}"#;

/// La fausse claude de Windows. Que de l'ASCII : la coquille lit le script dans la page de code du système. Aucun bloc
/// entre parenthèses : un `PATH` qui porte une parenthèse les fermerait.
const FAUSSE_WINDOWS: &[&str] = &[
    "@echo off",
    "rem La fausse claude des essais du decoupage.",
    r#"set "ici=%~dp0""#,
    "for %%I in (.) do set bac=%%~nxI",
    r#"type "%ici%controle.jsonl""#,
    r#"findstr "^" > "%TEMP%\entree.jsonl""#,
    r#"findstr /l /c:"PLAN.kraken" "%TEMP%\entree.jsonl" > nul"#,
    "if errorlevel 1 goto tache",
    "set n=1",
    r#"if exist "%ici%decoupe-1" set n=2"#,
    r#"type nul > "%ici%decoupe-%n%""#,
    r#"copy /y "%TEMP%\entree.jsonl" "%ici%consigne-decoupeur-%n%.jsonl" > nul"#,
    r#"set PATH > "%ici%path-decoupeur-%n%.txt""#,
    r#"if exist "%ici%plan-%n%.kraken" copy /y "%ici%plan-%n%.kraken" PLAN.kraken > nul"#,
    r#"kraken plan --decoupe PLAN.kraken > "%ici%preuve-%n%.txt" 2>&1"#,
    // La redirection d'abord : `echo 0> f` redirigerait l'entrée, et écrirait l'état d'echo sur la sortie.
    r#">"%ici%code-%n%.txt" echo %errorlevel%"#,
    r#"type "%ici%fin.jsonl""#,
    "exit /b 0",
    ":tache",
    r#"findstr /l /c:"REMARQUES.kraken" "%TEMP%\entree.jsonl" > nul"#,
    "if not errorlevel 1 goto relecteur",
    r#"set PATH > "%ici%path-%bac%.txt""#,
    "if not exist sortie mkdir sortie",
    r#"copy /y "%TEMP%\entree.jsonl" "sortie\%bac%.txt" > nul"#,
    r#"type "%ici%fin.jsonl""#,
    "exit /b 0",
    ":relecteur",
    r#"type nul > "%ici%relecteur""#,
    r#"if exist DIFF.kraken copy /y DIFF.kraken "%ici%diff.txt" > nul"#,
    r#"copy /y "%ici%remarques.kraken" REMARQUES.kraken > nul"#,
    r#"type "%ici%fin.jsonl""#,
    "exit /b 0",
];

/// La fausse claude des autres systèmes.
const FAUSSE_UNIX: &[&str] = &[
    "#!/bin/sh",
    "# La fausse claude des essais du decoupage.",
    r#"ici=$(dirname "$0")"#,
    r#"bac=$(basename "$(pwd)")"#,
    r#"cat "$ici/controle.jsonl""#,
    r#"cat > "$TMPDIR/entree.jsonl""#,
    r#"if grep -q 'PLAN\.kraken' "$TMPDIR/entree.jsonl"; then"#,
    "n=1",
    r#"if [ -f "$ici/decoupe-1" ]; then n=2; fi"#,
    r#": > "$ici/decoupe-$n""#,
    r#"cp "$TMPDIR/entree.jsonl" "$ici/consigne-decoupeur-$n.jsonl""#,
    r#"printf 'PATH=%s\n' "$PATH" > "$ici/path-decoupeur-$n.txt""#,
    r#"if [ -f "$ici/plan-$n.kraken" ]; then cp "$ici/plan-$n.kraken" PLAN.kraken; fi"#,
    r#"kraken plan --decoupe PLAN.kraken > "$ici/preuve-$n.txt" 2>&1"#,
    r#"echo $? > "$ici/code-$n.txt""#,
    r#"elif grep -q 'REMARQUES\.kraken' "$TMPDIR/entree.jsonl"; then"#,
    r#": > "$ici/relecteur""#,
    r#"if [ -f DIFF.kraken ]; then cp DIFF.kraken "$ici/diff.txt"; fi"#,
    r#"cp "$ici/remarques.kraken" REMARQUES.kraken"#,
    "else",
    r#"printf 'PATH=%s\n' "$PATH" > "$ici/path-$bac.txt""#,
    "mkdir -p sortie",
    r#"cp "$TMPDIR/entree.jsonl" "sortie/$bac.txt""#,
    "fi",
    r#"cat "$ici/fin.jsonl""#,
    "exit 0",
];

/// Le faux srt de Windows, que `KRAKEN_SRT` nomme : il saute `--settings <fichier>`, puis joue la preuve de `sh -c
/// <preuve>` par la coquille de Windows, où `sh` peut manquer, et `true`, l'épreuve du mur, sort en 0.
const FAUX_SRT_WINDOWS: &[&str] =
    &["@echo off", "rem Le faux srt des essais.", r#"if "%~5"=="true" exit /b 0"#, "%~5", "exit /b %errorlevel%"];

/// Le faux srt des autres systèmes : il saute `--settings <fichier>`, puis joue `sh -c <preuve>`.
const FAUX_SRT_UNIX: &[&str] = &["#!/bin/sh", "# Le faux srt des essais.", "shift 2", r#"exec "$@""#];

/// Le faux srt qui ne tient pas le mur : il rend 1, quoi qu'on lui donne.
const SRT_SANS_MUR: &[&str] = if cfg!(windows) { &["@exit /b 1"] } else { &["#!/bin/sh", "exit 1"] };

/// Écrit ces lignes en script à ce chemin, dans les fins de ligne du système, puis le rend exécutable : c'est Kraken
/// qui le lance, pas le test.
fn script(chemin: &Path, lignes: &[&str]) {
    let fin = if cfg!(windows) { "\r\n" } else { "\n" };
    fs::write(chemin, lignes.iter().map(|ligne| format!("{ligne}{fin}")).collect::<String>()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(chemin, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// L'adresse de la PR que le faux `gh` ouvre.
const PR: &str = "https://forge.test/p/d/pull/12";

/// Le faux `gh` des essais, écrit dans `<racine>/gh` et que `KRAKEN_GH` nomme : il note ses arguments dans `appels.txt`
/// ; `issue view` rend `issue.json`, que l'essai pose ; `auth status` réussit ; `pr create` garde son corps
/// (`corps.txt`) et la branche de `--head`, puis rend l'adresse de la PR 12 ; `pr view <branche>` rend la PR sur la
/// tête que la branche a dans `depot` ; `pr view 12`, la PR ouverte, sans conflit, sur la tête de la branche gardée ;
/// `run list`, un run vert ; `api`, aucun statut. Rend son chemin.
fn faux_gh(racine: &Path, depot: &Path) -> PathBuf {
    let dir = racine.join("gh");
    fs::create_dir_all(&dir).unwrap();
    let (d, depot) = (dir.to_string_lossy(), depot.to_string_lossy());
    let runs = concat!(
        r#"[{"conclusion":"success","createdAt":"","name":"ci","startedAt":"","status":"completed","workflowName":"c"#,
        r#"i"}]"#,
    );
    let statuts = r#"{"state":"pending","statuses":[],"total_count":0}"#;
    let vue = |tete: &str| format!(r#"{{"headRefOid":"{tete}","number":12,"url":"{PR}"}}"#);
    let ci = |tete: &str| format!(r#"{{"headRefOid":"{tete}","mergeable":"MERGEABLE","state":"OPEN"}}"#);
    if cfg!(windows) {
        let chemin = dir.join("gh.cmd");
        let lignes = [
            "@echo off".to_string(),
            format!("set \"d={d}\""),
            ">>\"%d%\\appels.txt\" echo %*".into(),
            "if \"%1\"==\"issue\" type \"%d%\\issue.json\"& exit /b 0".into(),
            "if \"%1\"==\"auth\" exit /b 0".into(),
            format!("if \"%1\"==\"run\" echo {runs}& exit /b 0"),
            format!("if \"%1\"==\"api\" echo {statuts}& exit /b 0"),
            "if \"%2\"==\"create\" goto creer".into(),
            "if \"%3\"==\"12\" goto ci".into(),
            format!("for /f %%s in ('git -C \"{depot}\" rev-parse %3') do set tete=%%s"),
            format!("echo {}", vue("%tete%")),
            "exit /b 0".into(),
            ":creer".into(),
            "findstr \"^\" > \"%d%\\corps.txt\"".into(),
            ">\"%d%\\branche.txt\" echo %6".into(),
            format!("echo {PR}"),
            "exit /b 0".into(),
            ":ci".into(),
            "set /p branche=<\"%d%\\branche.txt\"".into(),
            format!("for /f %%s in ('git -C \"{depot}\" rev-parse %branche%') do set tete=%%s"),
            format!("echo {}", ci("%tete%")),
        ];
        fs::write(&chemin, lignes.map(|ligne| ligne + "\r\n").concat()).unwrap();
        chemin
    } else {
        let chemin = dir.join("gh");
        let lignes = [
            "#!/bin/sh".to_string(),
            format!("d='{d}'"),
            "echo \"$*\" >> \"$d/appels.txt\"".into(),
            "if [ \"$1\" = issue ]; then cat \"$d/issue.json\"; exit 0; fi".into(),
            "if [ \"$1\" = auth ]; then exit 0; fi".into(),
            format!("if [ \"$1\" = run ]; then echo '{runs}'; exit 0; fi"),
            format!("if [ \"$1\" = api ]; then echo '{statuts}'; exit 0; fi"),
            format!(
                "if [ \"$2\" = create ]; then cat > \"$d/corps.txt\"; echo \"$6\" > \"$d/branche.txt\"; \
                 echo '{PR}'; exit 0; fi"
            ),
            "if [ \"$3\" = 12 ]; then".into(),
            format!("tete=$(git -C '{depot}' rev-parse \"$(cat \"$d/branche.txt\")\")"),
            format!("echo '{}'", ci("'\"$tete\"'")),
            "else".into(),
            format!("tete=$(git -C '{depot}' rev-parse \"$3\")"),
            format!("echo '{}'", vue("'\"$tete\"'")),
            "fi".into(),
        ];
        let refs: Vec<&str> = lignes.iter().map(String::as_str).collect();
        script(&chemin, &refs);
        chemin
    }
}

/// L'issue que `gh issue view` rend : son numéro, son état, son titre, son corps et son adresse, en JSON.
fn issue_json(etat: &str, titre: &str, corps: &str) -> String {
    let (corps, titre) = (json_chaine(corps), json_chaine(titre));
    format!(r#"{{"body":{corps},"number":{ISSUE},"state":"{etat}","title":{titre},"url":"{URL}/issues/{ISSUE}"}}"#)
}

/// L'appel de `gh` qui lit l'issue.
fn vue_de_l_issue() -> String {
    format!("issue view {ISSUE} --json number,state,title,body,url --repo forge.test/p/d")
}

/// Le titre et le corps de l'issue ne paraissent pas dans ce texte.
fn sans_l_issue(quoi: &str, texte: &str) {
    assert!(!texte.contains(TITRE) && !texte.contains(CORPS), "{quoi} cite l'issue : {texte}");
}

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    // Un nom court : Claude Code met ses sockets de pont dans son dossier temporaire (`<dossier
    // temporaire>/claude-http-<16 hex>.sock`), et un chemin de socket tient en 107 octets.
    let dir = std::env::temp_dir().join(format!("kd-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Joue git dans ce dossier, avec une identité de passage, et rend sa sortie, rognée.
fn git(dir: &Path, args: &[&str]) -> String {
    let identite = ["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"];
    let sortie = Command::new("git").current_dir(dir).args(identite).args(args).output().unwrap();
    assert!(sortie.status.success(), "git {args:?} : {}", String::from_utf8_lossy(&sortie.stderr));
    String::from_utf8_lossy(&sortie.stdout).trim().to_string()
}

/// Une chaîne JSON, guillemets compris : le guillemet et la barre inverse s'échappent.
fn json_chaine(texte: &str) -> String {
    format!("\"{}\"", texte.replace('\\', "\\\\").replace('"', "\\\""))
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

/// Ce qu'une commande a rendu, pour le message d'un échec.
fn dit(sortie: &Output) -> String {
    let (sortant, erreur) = (String::from_utf8_lossy(&sortie.stdout), String::from_utf8_lossy(&sortie.stderr));
    format!("code {:?}\nstdout : {sortant}\nstderr : {erreur}", sortie.status.code())
}

/// Le texte d'un plan : ces lignes, chacune finie par `\n`.
fn texte(lignes: &[String]) -> String {
    lignes.iter().map(|ligne| format!("{ligne}\n")).collect()
}

/// L'en-tête que Kraken demande au découpeur dans ces essais.
fn entete() -> Vec<String> {
    vec!["plan 1".to_string(), format!("objectif : {OBJECTIF}"), format!("preuve : {VERIFIER}")]
}

/// Le plan juste des essais : a, puis b, qui partage `sortie/` avec elle et passe donc après elle.
fn plan_juste() -> String {
    let mut lignes = entete();
    for (id, objectif) in [("a", "Ecrire un fichier sous sortie"), ("b", "Ecrire un autre fichier sous sortie")] {
        lignes.extend(["".into(), format!("tache {id}"), "fichiers : sortie/".into(), "modele : haiku".into()]);
        lignes.push(format!("    {objectif}"));
    }
    texte(&lignes)
}

/// Un plan découpé de `n` tâches, chacune à ses clés, sous l'en-tête demandé.
fn plan_de(n: usize) -> String {
    let mut lignes = entete();
    for k in 1..=n {
        lignes.extend([format!("tache t{k}"), format!("fichiers : t{k}/"), "modele : haiku".into(), "    x".into()]);
    }
    texte(&lignes)
}

/// La valeur du `PATH` que la fausse claude a gardée : la ligne `PATH=<valeur>`, le nom en toute casse.
fn path_garde(fichier: &Path) -> String {
    let octets = fs::read(fichier).unwrap_or_else(|e| panic!("{} : {e}", fichier.display()));
    let texte = String::from_utf8_lossy(&octets);
    let ligne = texte.lines().find(|ligne| ligne.get(..5).is_some_and(|nom| nom.eq_ignore_ascii_case("PATH=")));
    ligne.map(|ligne| ligne[5..].to_string()).unwrap_or_else(|| panic!("aucun PATH dans {texte}"))
}

/// Les ids des vagues que le fil ouvre, dans l'ordre.
fn vagues(fil: &str) -> Vec<String> {
    let ouvertes = fil.lines().filter(|ligne| ligne.contains(r#""type":"vague.ouverte""#));
    let id = |ligne: &str| {
        let debut = ligne.find(r#""vague":""#).expect("une ouverture nomme sa vague") + 9;
        ligne[debut..].split('"').next().unwrap().to_string()
    };
    ouvertes.map(id).collect()
}

/// Un essai : un dépôt git jetable avec un commit, un dossier de Kraken jetable, et la fausse claude, que le profil
/// accepté `essai` nomme, ce qu'elle répond et ce qu'elle garde.
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
        let essai = Essai { dossier: racine.join("kraken"), racine, depot, main };
        faux_gh(&essai.racine, &essai.depot);
        essai.poser_la_fausse_claude();
        script(&essai.srt("faux-srt"), if cfg!(windows) { FAUX_SRT_WINDOWS } else { FAUX_SRT_UNIX });
        essai.accepter_le_profil();
        essai.poser_les_reponses();
        essai
    }

    /// La fausse claude, écrite puis rendue exécutable : c'est Kraken qui la lance, pas le test.
    fn poser_la_fausse_claude(&self) {
        let (nom, lignes, fin) = if cfg!(windows) {
            ("fausse-claude.cmd", FAUSSE_WINDOWS, "\r\n")
        } else {
            ("fausse-claude.sh", FAUSSE_UNIX, "\n")
        };
        fs::write(self.main.join("remarques.kraken"), "remarques 1\n").unwrap();
        let script = self.main.join(nom);
        fs::write(&script, lignes.iter().map(|ligne| format!("{ligne}{fin}")).collect::<String>()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    /// Le profil `essai` : la fausse claude, `haiku` partout, découpeur compris, le mur exigé ; puis son acceptation.
    fn accepter_le_profil(&self) {
        let fausse = self.main.join(if cfg!(windows) { "fausse-claude.cmd" } else { "fausse-claude.sh" });
        let texte = format!(
            "sorte = claude\nprogramme = {}\nmodele = haiku\nrelance = haiku\ndecoupeur = haiku\nrelecteur = haiku\n\
             mur = exige\ndelai_main = 120\ndelai_preuve = 60\n",
            fausse.display()
        );
        fs::create_dir_all(self.dossier.join("profils")).unwrap();
        // `libre`, le même profil au mur seulement demandé.
        for (nom, texte) in [("essai", texte.clone()), ("libre", texte.replace("mur = exige", "mur = demande"))] {
            fs::write(self.dossier.join("profils").join(format!("{nom}.profil")), texte).unwrap();
            let sortie = self.kraken().args(["accepter", nom]).output().unwrap();
            assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
        }
    }

    /// Ce que la fausse claude répond à toute main : les quatre réponses de contrôle, dont la liste des règles porte
    /// celles du découpeur et celles des tâches, puis l'ouverture et un résultat vert.
    fn poser_les_reponses(&self) {
        let mut allow: Vec<String> = ["Skill", "Workflow", "ExitPlanMode"].map(String::from).into();
        for pretee in [PREUVE, PREUVE_DU_RELECTEUR, VERIFIER] {
            allow.extend([format!("Bash({pretee})"), format!("PowerShell({pretee})")]);
        }
        let regle = regle_du_dossier(&self.dossier);
        let mut deny: Vec<String> = FERMES.map(String::from).into();
        deny.extend([format!("Edit({regle}/fil.jsonl)"), format!("Edit({regle}/profils/**)")]);
        deny.extend(["Read", "Edit"].map(|outil| format!("{outil}({regle}/srt-*.json)")));
        for leurre in [self.racine.join("config"), self.racine.join("personnel").join(".claude")] {
            let regle = regle_du_dossier(&leurre);
            deny.extend(["Read", "Edit"].map(|outil| format!("{outil}({regle}/.credentials.json)")));
        }
        let commun = git(&self.depot, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);
        deny.push(format!("Edit({}/**)", regle_du_dossier(Path::new(&commun))));
        let vues: Vec<String> = [("allow", &allow), ("deny", &deny)]
            .iter()
            .flat_map(|(comportement, regles)| {
                regles.iter().map(move |regle| {
                    format!(r#"{{"behavior":"{comportement}","source":"flagSettings","rule":{}}}"#, json_chaine(regle))
                })
            })
            .collect();
        let regles = format!(
            concat!(
                r#"{{"type":"control_response","response":{{"subtype":"success","request_id":"list_permission_rules""#,
                r#","response":{{"state":{{"rules":[{}]}}}}}}}}"#,
            ),
            vues.join(",")
        );
        let controle = [GET_SETTINGS, &regles, GET_HOOKS_LISTING, GET_SANDBOX_DIALOG].join("\n") + "\n";
        fs::write(self.main.join("controle.jsonl"), controle).unwrap();
        fs::write(self.main.join("fin.jsonl"), format!("{OUVERTURE}\n{RESULTAT}\n")).unwrap();
    }

    /// Le chemin du script de srt `nom` de l'essai, à sa racine : `<nom>.cmd` sous Windows, `<nom>.sh` ailleurs.
    fn srt(&self, nom: &str) -> PathBuf {
        self.racine.join(format!("{nom}.{}", if cfg!(windows) { "cmd" } else { "sh" }))
    }

    /// `kraken`, prêt à lancer sans qu'aucune vraie main ne puisse partir ni rien de l'humain ne soit lu : le dossier
    /// de Kraken, un `claude` qui n'existe pas pour le profil intégré, le faux srt de l'essai, et les leurres du
    /// dossier personnel et de `CLAUDE_CONFIG_DIR`, jamais créés.
    fn kraken(&self) -> Command {
        let mut commande = Command::new(env!("CARGO_BIN_EXE_kraken"));
        let personnel = self.racine.join("personnel");
        commande
            .current_dir(&self.racine)
            .env("KRAKEN_DOSSIER", &self.dossier)
            .env("KRAKEN_CLAUDE", self.racine.join("claude-absent"))
            .env("KRAKEN_SRT", self.srt("faux-srt"))
            .env("KRAKEN_GH", self.racine.join("gh").join(if cfg!(windows) { "gh.cmd" } else { "gh" }))
            .env("USERPROFILE", &personnel)
            .env("HOME", personnel)
            .env("CLAUDE_CONFIG_DIR", self.racine.join("config"));
        commande
    }

    /// Le plan que le découpeur écrit à sa tentative `n`.
    fn poser(&self, n: usize, texte: &str) {
        fs::write(self.main.join(format!("plan-{n}.kraken")), texte).unwrap();
    }

    /// `kraken lancer essai OBJECTIF --decouper --verifier <verifier> --depot <depot>`, et `plus`.
    fn lancer(&self, verifier: &str, plus: &[&str]) -> Output {
        let mut commande = self.kraken();
        commande.args(["lancer", "essai", OBJECTIF, "--decouper", "--verifier", verifier, "--depot"]);
        commande.arg(&self.depot).args(plus).output().unwrap()
    }

    /// Le fil du dossier de Kraken.
    fn fil(&self) -> String {
        fs::read_to_string(self.dossier.join("fil.jsonl")).unwrap()
    }

    /// Ce que la fausse claude a gardé à côté d'elle, en fins de ligne `\n`.
    fn garde(&self, nom: &str) -> String {
        let octets = fs::read(self.main.join(nom)).unwrap_or_else(|e| panic!("{nom} : {e}"));
        String::from_utf8_lossy(&octets).replace("\r\n", "\n")
    }

    /// `kraken etat`.
    fn etat(&self) -> Output {
        self.kraken().arg("etat").output().unwrap()
    }

    /// Le distant `origin` du dépôt : [`URL`], qu'`insteadOf` renvoie au dépôt nu `<racine>/distant.git`, où le `HEAD`
    /// du dépôt est poussé dans `main`.
    fn distant(&self) {
        let nu = self.racine.join("distant.git");
        git(&self.racine, &["init", "-q", "--bare", &nu.to_string_lossy()]);
        git(&self.depot, &["remote", "add", "origin", URL]);
        // Avec des `/` : git lit `C:/…` comme un chemin du poste sous Windows.
        git(&self.depot, &["config", &format!("url.{}.insteadOf", nu.to_string_lossy().replace('\\', "/")), URL]);
        git(&self.depot, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
    }

    /// L'issue que le faux `gh` rendra.
    fn poser_l_issue(&self, json: &str) {
        fs::write(self.racine.join("gh").join("issue.json"), json).unwrap();
    }

    /// `kraken lancer <profil> --issue ISSUE --decouper --verifier VERIFIER --depot <depot>`, et `plus`.
    fn lancer_l_issue(&self, profil: &str, plus: &[&str]) -> Output {
        let mut commande = self.kraken();
        let issue = ISSUE.to_string();
        commande.args(["lancer", profil, "--issue", &issue, "--decouper", "--verifier", VERIFIER, "--depot"]);
        commande.arg(&self.depot).args(plus).output().unwrap()
    }

    /// Les appels du faux `gh`, un par ligne, sans les guillemets que Windows ajoute à certains.
    fn appels(&self) -> Vec<String> {
        let texte = fs::read_to_string(self.racine.join("gh").join("appels.txt")).unwrap_or_default();
        texte.lines().map(|ligne| ligne.replace('"', "").trim().to_string()).collect()
    }

    /// Rien n'est parti : ni fil, ni bac, ni branche de Kraken, et la fausse claude n'a jamais tourné.
    fn rien_n_est_parti(&self) {
        assert!(!self.dossier.join("fil.jsonl").exists());
        let bacs = self.dossier.join("bacs");
        assert!(!bacs.exists() || fs::read_dir(&bacs).unwrap().count() == 0);
        assert_eq!(git(&self.depot, &["branch", "--list", "kraken/*"]), "");
        assert!(!self.main.join("decoupe-1").exists(), "aucun découpeur n'est parti");
    }
}

/// Le plan juste, sous l'en-tête que Kraken demande pour l'issue : l'objectif est `Issue #<n>`.
fn plan_de_l_issue() -> String {
    plan_juste().replace(&format!("objectif : {OBJECTIF}"), &format!("objectif : Issue #{ISSUE}"))
}

/// Le refus que Kraken a écrit : le code 2, rien sur stdout, quatre lignes sur stderr. Rend sa première ligne et sa
/// preuve.
fn lire_refus(sortie: &Output) -> (String, String) {
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(2), "{}", dit(sortie));
    assert!(sortie.stdout.is_empty(), "{}", dit(sortie));
    let lignes: Vec<&str> = erreur.lines().collect();
    assert_eq!(lignes.len(), 4, "{erreur}");
    let preuve = lignes[2].strip_prefix("  preuve : ").unwrap_or_else(|| panic!("{erreur}"));
    (lignes[0].to_string(), preuve.to_string())
}

#[test]
fn une_issue_sous_un_profil_sans_mur_exige_est_refusee_en_6029() {
    let essai = Essai::nouveau("i-6029");
    essai.distant();
    essai.poser_l_issue(&issue_json("OPEN", TITRE, CORPS));
    essai.poser(1, &plan_de_l_issue());
    let (code, preuve) = lire_refus(&essai.lancer_l_issue("libre", &[]));
    assert_eq!(code, "kraken : 6029 lancer.issue_sans_mur");
    assert!(preuve.contains("n'exige pas le mur"), "{preuve}");
    // L'issue n'est pas même lue : gh n'est pas lancé, et rien n'est parti.
    assert!(essai.appels().is_empty(), "{:?}", essai.appels());
    essai.rien_n_est_parti();
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn une_issue_fermee_est_refusee_en_8012() {
    let essai = Essai::nouveau("i-8012");
    essai.distant();
    essai.poser_l_issue(&issue_json("CLOSED", TITRE, CORPS));
    essai.poser(1, &plan_de_l_issue());
    let (code, preuve) = lire_refus(&essai.lancer_l_issue("essai", &[]));
    assert_eq!(code, "kraken : 8012 forge.issue_illisible");
    assert!(preuve.contains("n'est pas ouverte"), "{preuve}");
    assert_eq!(essai.appels(), [vue_de_l_issue()]);
    essai.rien_n_est_parti();
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn une_issue_trop_longue_est_refusee_en_8013() {
    let essai = Essai::nouveau("i-8013");
    essai.distant();
    essai.poser_l_issue(&issue_json("OPEN", TITRE, &"a".repeat(65_536)));
    essai.poser(1, &plan_de_l_issue());
    let (code, preuve) = lire_refus(&essai.lancer_l_issue("essai", &[]));
    assert_eq!(code, "kraken : 8013 forge.issue_trop_longue");
    assert!(preuve.contains("65536"), "{preuve}");
    essai.rien_n_est_parti();
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn une_issue_ouverte_se_decoupe_et_toutes_ses_taches_se_relisent() {
    let essai = Essai::nouveau("i-ouverte");
    essai.distant();
    essai.poser_l_issue(&issue_json("OPEN", TITRE, CORPS));
    essai.poser(1, &plan_de_l_issue());
    let sortie = essai.lancer_l_issue("essai", &[]);
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let fil = essai.fil();
    let [d, v, r] = &vagues(&fil)[..] else { panic!("trois vagues : {fil}") };
    // La consigne du découpeur cite le titre et le corps de l'issue, et lui demande l'objectif `Issue #<n>`.
    let consigne = essai.garde("consigne-decoupeur-1.jsonl");
    assert!(consigne.contains(&format!("Titre : {TITRE}")) && consigne.contains(CORPS), "{consigne}");
    assert!(consigne.contains(&format!("objectif : Issue #{ISSUE}")), "{consigne}");
    // Le plan pris a l'objectif `Issue #<n>`, et la vague note son issue dès son ouverture.
    let ouverte =
        fil.lines().find(|l| l.contains(r#""type":"vague.ouverte""#) && l.contains(&format!(r#""vague":"{v}""#)));
    assert!(ouverte.unwrap().contains(&format!(r#""objectif":"Issue #{ISSUE}""#)), "{fil}");
    let note = format!(r#""type":"vague.issue","vague":"{v}","issue":{ISSUE}}}"#);
    let rang =
        |cherche: &str| fil.lines().position(|l| l.contains(cherche)).unwrap_or_else(|| panic!("{cherche} : {fil}"));
    let lancement_de_v = fil
        .lines()
        .position(|l| l.contains(r#""type":"main.lancement""#) && l.contains(&format!(r#""vague":"{v}""#)))
        .unwrap();
    assert!(rang(&note) < lancement_de_v, "vague.issue se note avant tout départ : {fil}");
    // Les deux tâches, critiques sous une issue, sont relues : le relecteur a vu leur travail, et la relecture est
    // notée.
    assert!(essai.main.join("relecteur").exists(), "le relecteur est parti");
    let diff = essai.garde("diff.txt");
    for k in [1, 2] {
        assert!(diff.contains(&format!("sortie/main-{v}-{k}.txt")), "{diff}");
    }
    assert!(fil.contains(&format!(r#""type":"vague.relecteur","vague":"{r}","relue":"{v}""#)), "{fil}");
    assert!(
        fil.contains(&format!(r#""type":"vague.relecture","vague":"{r}","relue":"{v}","n":0,"bloquantes":0"#)),
        "{fil}"
    );
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert!(erreur.contains(&format!("découpage pris : main-{d}-1 · 2 tâches")), "{erreur}");
    assert!(erreur.contains(&format!("relecture prise : main-{r}-1 · 0 remarques, dont 0 bloquantes")), "{erreur}");
    // `kraken etat` montre l'objectif du plan, `Issue #<n>`, après la ligne du découpage, et sort en 0.
    let etat = essai.etat();
    let montre = String::from_utf8_lossy(&etat.stdout);
    assert_eq!(etat.status.code(), Some(0), "{montre}");
    assert!(
        montre.starts_with(&format!("Découpage : 2 tâches · main-{d}-1 · prise (constatée)\nIssue #{ISSUE}\n")),
        "{montre}"
    );
    assert!(montre.contains("Relecture : 0 remarques, dont 0 bloquantes"), "{montre}");
    // Le titre et le corps ne vont qu'au découpeur : ni au fil, ni sur stdout et stderr, ni dans `kraken etat`.
    sans_l_issue("le fil", &fil);
    sans_l_issue("stderr", &erreur);
    sans_l_issue("stdout", &String::from_utf8_lossy(&sortie.stdout));
    sans_l_issue("kraken etat", &montre);
    sans_l_issue("les erreurs de kraken etat", &String::from_utf8_lossy(&etat.stderr));
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn une_issue_decoupee_avec_pr_ouvre_une_pr_qui_la_ferme() {
    let essai = Essai::nouveau("i-pr");
    essai.distant();
    essai.poser_l_issue(&issue_json("OPEN", TITRE, CORPS));
    essai.poser(1, &plan_de_l_issue());
    let sortie = essai.lancer_l_issue("essai", &["--pr", "main"]);
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let fil = essai.fil();
    let [_, v, _] = &vagues(&fil)[..] else { panic!("trois vagues : {fil}") };
    assert!(fil.contains(&format!(r#""type":"vague.issue","vague":"{v}","issue":{ISSUE}}}"#)), "{fil}");
    // L'issue se lit d'abord, puis gh se dit connecté ; la PR s'ouvre contre main, titrée de l'objectif du plan.
    let appels = essai.appels();
    assert_eq!(appels[0], vue_de_l_issue(), "{appels:?}");
    assert_eq!(appels[1], "auth status --hostname forge.test", "{appels:?}");
    let cree = appels.iter().find(|appel| appel.starts_with("pr create")).unwrap_or_else(|| panic!("{appels:?}"));
    assert!(cree.contains(&format!("--base main --head kraken/vague-{v} --title Issue #{ISSUE}")), "{cree}");
    // Le corps de la PR ferme l'issue.
    let corps = fs::read_to_string(essai.racine.join("gh").join("corps.txt")).unwrap().replace("\r\n", "\n");
    assert!(corps.contains(&format!("\nCloses #{ISSUE}\n")), "{corps}");
    // Ni le corps de la PR, ni ce que gh reçoit, ni le fil, ni stderr ne citent le titre et le corps de l'issue.
    sans_l_issue("le corps de la PR", &corps);
    sans_l_issue("gh", &appels.join("\n"));
    sans_l_issue("le fil", &fil);
    sans_l_issue("stderr", &String::from_utf8_lossy(&sortie.stderr));
    sans_l_issue("kraken etat", &String::from_utf8_lossy(&essai.etat().stdout));
    assert!(fil.contains(&format!(r#""type":"forge.pr","vague":"{v}""#)), "{fil}");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn un_objectif_decoupe_par_une_main_se_construit_et_s_assemble() {
    let essai = Essai::nouveau("juste");
    essai.poser(1, &plan_juste());
    let sortie = essai.lancer(VERIFIER, &[]);
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let fil = essai.fil();
    let [d, v] = &vagues(&fil)[..] else { panic!("deux vagues : {fil}") };
    // Sur stderr : le découpage pris, puis chaque tâche assemblée et la branche de la vague.
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    let lignes = [
        format!("découpage pris : main-{d}-1 · 2 tâches"),
        format!("assemblée : main-{v}-1 (a)"),
        format!("assemblée : main-{v}-2 (b)"),
        format!("branche de la vague : kraken/vague-{v}"),
    ];
    assert_eq!(erreur.lines().collect::<Vec<_>>(), lignes, "{}", dit(&sortie));
    // Le découpeur a joué lui-même `kraken plan --decoupe PLAN.kraken`, trouvé par son PATH : la commande a lu le plan,
    // dit la dépendance que Kraken ajoute, et réussi.
    assert_eq!(essai.garde("code-1.txt").trim(), "0");
    assert_eq!(essai.garde("preuve-1.txt"), "b attend a : elles partagent sortie/\n");
    // Son PATH porte en tête le dossier de l'exécutable de Kraken, puis celui de Kraken tel quel, qu'ont les tâches.
    let separateur = if cfg!(windows) { ';' } else { ':' };
    let du_decoupeur = path_garde(&essai.main.join("path-decoupeur-1.txt"));
    let de_a = path_garde(&essai.main.join(format!("path-main-{v}-1.txt")));
    let (tete, reste) = du_decoupeur.split_once(separateur).expect("un PATH de plusieurs dossiers");
    let exe = Path::new(env!("CARGO_BIN_EXE_kraken")).parent().unwrap();
    assert_eq!(fs::canonicalize(tete).unwrap(), fs::canonicalize(exe).unwrap(), "{du_decoupeur}");
    assert_eq!(reste, de_a);
    assert_eq!(path_garde(&essai.main.join(format!("path-main-{v}-2.txt"))), de_a);
    // Le modèle du découpeur, la clé `decoupeur` du profil, est noté à son lancement.
    let lancement = fil.lines().find(|ligne| ligne.contains(r#""type":"main.lancement""#)).unwrap();
    assert!(lancement.contains(&format!(r#""vague":"{d}""#)) && lancement.contains(r#""modele":"haiku""#));
    // Au fil, le découpage et le lien de la vague à son découpage ; le bac du découpeur et sa branche sont retirés.
    assert!(fil.contains(&format!(r#""type":"vague.decoupage","vague":"{d}","taches":2,"#)), "{fil}");
    assert!(fil.contains(&format!(r#""type":"vague.joue","vague":"{v}","decoupage":"{d}""#)), "{fil}");
    let bac = essai.dossier.join("bacs").join(format!("main-{d}-1"));
    assert!(!bac.exists(), "le bac du découpeur est retiré");
    assert_eq!(git(&essai.depot, &["branch", "--list", &format!("kraken/main-{d}-1")]), "");
    // La branche de la vague porte le travail des deux tâches.
    let arbre = git(&essai.depot, &["ls-tree", "-r", "--name-only", &format!("kraken/vague-{v}")]);
    assert_eq!(arbre, format!("sortie/main-{v}-1.txt\nsortie/main-{v}-2.txt"));
    // `kraken etat` montre la chaîne, et sort en 0.
    let etat = essai.etat();
    let montre = String::from_utf8_lossy(&etat.stdout);
    assert_eq!(etat.status.code(), Some(0), "{montre}");
    assert!(
        montre.starts_with(&format!("Découpage : 2 tâches · main-{d}-1 · prise (constatée)\n{OBJECTIF}\n")),
        "{montre}"
    );
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn un_plan_faux_se_refait_juste_a_la_relance() {
    let essai = Essai::nouveau("relance");
    // `kraken plan --decoupe` ne juge pas l'en-tête : le découpeur le croit juste, Kraken le refuse (6026).
    essai.poser(1, &plan_juste().replace(&format!("objectif : {OBJECTIF}"), "objectif : Autre chose"));
    essai.poser(2, &plan_juste());
    let sortie = essai.lancer(VERIFIER, &[]);
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    assert_eq!((essai.garde("code-1.txt").trim(), essai.garde("code-2.txt").trim()), ("0", "0"));
    let fil = essai.fil();
    let rouge =
        fil.lines().find(|ligne| ligne.contains(r#""type":"main.fin""#) && ligne.contains(r#""couleur":"rouge""#));
    let rouge = rouge.unwrap_or_else(|| panic!("{fil}"));
    assert!(rouge.contains(r#""tentative":1"#) && rouge.contains("6026 lancer.plan_entete"), "{rouge}");
    assert_eq!(vagues(&fil).len(), 2, "le découpeur, puis la vague de son plan");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn un_plan_faux_deux_fois_classe_le_decoupage_sans_vague() {
    let essai = Essai::nouveau("classe");
    essai.poser(1, &plan_de(13));
    essai.poser(2, &plan_de(13));
    let sortie = essai.lancer(VERIFIER, &[]);
    assert_eq!(sortie.status.code(), Some(1), "{}", dit(&sortie));
    assert!(sortie.stdout.is_empty(), "{}", dit(&sortie));
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert!(erreur.ends_with("le découpage n'est pas pris : aucune vague ne part\n"), "{erreur}");
    assert!(erreur.contains("6025 lancer.plan_borne"), "{erreur}");
    // Le découpeur a vu sa propre preuve rouge, deux fois : `kraken plan --decoupe` refuse treize tâches (6025, code
    // 2).
    for n in [1, 2] {
        assert_eq!(essai.garde(&format!("code-{n}.txt")).trim(), "2");
        assert!(essai.garde(&format!("preuve-{n}.txt")).starts_with("kraken : 6025 lancer.plan_borne\n"));
    }
    // Une seule vague, celle du découpeur : ni découpage noté, ni vague de plan.
    let fil = essai.fil();
    assert_eq!(vagues(&fil).len(), 1, "{fil}");
    assert!(!fil.contains(r#""type":"vague.decoupage""#) && !fil.contains(r#""type":"vague.joue""#), "{fil}");
    assert_eq!(git(&essai.depot, &["branch", "--list", "kraken/vague-*"]), "");
    assert_eq!(essai.etat().status.code(), Some(1));
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn observer_ecrit_le_plan_sur_stdout_sans_rien_construire() {
    let essai = Essai::nouveau("observe");
    essai.poser(1, &plan_juste());
    let sortie = essai.lancer(VERIFIER, &["--observer"]);
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    // Le plan pris, et lui seul, sur stdout.
    assert_eq!(String::from_utf8_lossy(&sortie.stdout).replace("\r\n", "\n"), plan_juste());
    // Au fil, le découpeur et son découpage, et aucune vague de plan ; ni branche de vague, ni bac du découpeur.
    let fil = essai.fil();
    let [d] = &vagues(&fil)[..] else { panic!("une vague : {fil}") };
    assert!(fil.contains(&format!(r#""type":"vague.decoupage","vague":"{d}","taches":2,"#)), "{fil}");
    assert!(!fil.contains(r#""type":"vague.joue""#) && !fil.contains(r#""type":"vague.assemblage""#), "{fil}");
    assert_eq!(git(&essai.depot, &["branch", "--list", "kraken/*"]), "");
    // `kraken etat` ne dit que la ligne du découpage et celle de son reçu, et sort en 0. La durée du reçu suit
    // l'horloge : le test n'en fige que le début de la ligne.
    let etat = essai.etat();
    let montre = String::from_utf8_lossy(&etat.stdout);
    let lignes: Vec<&str> = montre.trim_end().lines().collect();
    let [ligne, recu] = &lignes[..] else { panic!("{montre}") };
    assert_eq!(etat.status.code(), Some(0));
    assert_eq!(*ligne, format!("Découpage : 2 tâches · main-{d}-1 · prise (constatée)"));
    assert!(recu.starts_with("Coût : inconnu · 1 main, 0 relance, 0 classée"), "{montre}");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn observer_echappe_le_controle_du_plan_et_le_fil_le_garde_tel_quel() {
    let essai = Essai::nouveau("observe-controle");
    // L'objectif d'une tâche porte un caractère de contrôle : le découpeur l'écrit tel quel dans le plan.
    let plan = plan_juste().replace("Ecrire un fichier sous sortie", "Ecrire\u{1b}[2J un fichier sous sortie");
    essai.poser(1, &plan);
    let sortie = essai.lancer(VERIFIER, &["--observer"]);
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    // Sur stdout, le plan avec son contrôle écrit `\u{1b}`, et non le caractère brut : le terminal ne peut pas être
    // piégé.
    let montre = String::from_utf8_lossy(&sortie.stdout).replace("\r\n", "\n");
    assert_eq!(montre, plan.replace('\u{1b}', r"\u{1b}"));
    // Au fil, le plan reste tel que le découpeur l'a écrit : le contrôle y prend la forme JSON du caractère brut
    // (`\u001b`), et non l'écriture de `sans_controle_en_lignes` (`\u{1b}`, que JSON écrirait `\\u{1b}`).
    let fil = essai.fil();
    let ligne = fil.lines().find(|ligne| ligne.contains(r#""type":"vague.decoupage""#)).expect("le découpage noté");
    assert!(ligne.contains(r#""plan":"plan 1\n"#), "{ligne}");
    assert!(ligne.contains(r"\u001b[2J"), "le fil a perdu le caractère brut : {ligne}");
    assert!(!ligne.contains(r"\\u{1b}"), "le fil a gardé l'écriture de stdout : {ligne}");
    // Comme le test voisin : une seule vague, ni plan joué ni branche, donc rien ne se construit.
    assert_eq!(vagues(&fil).len(), 1, "{fil}");
    assert!(!fil.contains(r#""type":"vague.joue""#) && !fil.contains(r#""type":"vague.assemblage""#), "{fil}");
    assert_eq!(git(&essai.depot, &["branch", "--list", "kraken/*"]), "");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn une_base_rouge_ne_lance_aucun_decoupeur() {
    let essai = Essai::nouveau("base-rouge");
    essai.poser(1, &plan_juste());
    let rouge = if cfg!(windows) { "echo base rouge& exit 1" } else { "echo base rouge; exit 1" };
    let sortie = essai.lancer(rouge, &[]);
    // Le refus 6022, en quatre lignes et en code 2, rien sur stdout.
    assert_eq!(sortie.status.code(), Some(2), "{}", dit(&sortie));
    assert!(sortie.stdout.is_empty(), "{}", dit(&sortie));
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    let lignes: Vec<&str> = erreur.lines().collect();
    assert_eq!(lignes.len(), 4, "{erreur}");
    assert_eq!(lignes[0], "kraken : 6022 lancer.base_rouge");
    assert!(lignes[2].contains("base rouge"), "{erreur}");
    // Aucune main n'est partie : ni fil, ni bac, ni branche, et la fausse claude n'a jamais tourné.
    assert!(!essai.dossier.join("fil.jsonl").exists());
    let bacs = essai.dossier.join("bacs");
    assert!(!bacs.exists() || fs::read_dir(&bacs).unwrap().count() == 0);
    assert_eq!(git(&essai.depot, &["branch", "--list", "kraken/*"]), "");
    assert!(!essai.main.join("decoupe-1").exists(), "aucun découpeur n'est parti");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn un_srt_qui_ne_joue_pas_true_refuse_le_mur_exige_avant_la_base_et_toute_main() {
    let essai = Essai::nouveau("sans-mur");
    essai.poser(1, &plan_juste());
    script(&essai.srt("srt-sans-mur"), SRT_SANS_MUR);
    let mut commande = essai.kraken();
    commande.env("KRAKEN_SRT", essai.srt("srt-sans-mur"));
    commande.args(["lancer", "essai", OBJECTIF, "--decouper", "--verifier", VERIFIER, "--depot"]);
    let sortie = commande.arg(&essai.depot).output().unwrap();
    // Le refus 6028, en quatre lignes et en code 2, rien sur stdout : avant l'épreuve de la base, qui serait 6022.
    assert_eq!(sortie.status.code(), Some(2), "{}", dit(&sortie));
    assert!(sortie.stdout.is_empty(), "{}", dit(&sortie));
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    let lignes: Vec<&str> = erreur.lines().collect();
    assert_eq!(lignes.len(), 4, "{erreur}");
    assert_eq!(lignes[0], "kraken : 6028 lancer.mur_refuse");
    assert!(lignes[1].starts_with("  règle : ") && lignes[3].starts_with("  correction : "), "{erreur}");
    assert!(lignes[2].contains("srt ne joue pas `true`"), "{erreur}");
    // Rien n'est parti : ni fil, ni bac, ni branche, ni réglages de srt, et la fausse claude n'a jamais tourné.
    assert!(!essai.dossier.join("fil.jsonl").exists());
    let bacs = essai.dossier.join("bacs");
    assert!(!bacs.exists() || fs::read_dir(&bacs).unwrap().count() == 0);
    let restes = fs::read_dir(&essai.dossier).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned());
    assert!(restes.filter(|nom| nom.ends_with(".json") || nom.ends_with(".tmp")).count() == 0);
    assert_eq!(git(&essai.depot, &["branch", "--list", "kraken/*"]), "");
    assert!(!essai.main.join("decoupe-1").exists(), "aucun découpeur n'est parti");
    let _ = fs::remove_dir_all(&essai.racine);
}
