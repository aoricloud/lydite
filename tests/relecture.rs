//! La relecture, jouée sur le binaire lui-même : `lydite remarques <fichier>` seul, puis `lydite lancer <profil> --plan
//! <fichier>` d'un plan dont une tâche est critique. Chaque essai lance `kraken` sur un dépôt git jetable qui a un
//! commit, avec un dossier de Kraken jetable hors de lui et une fausse `claude` que le profil d'essai nomme :
//! `fausse-claude.cmd` sous Windows, `fausse-claude.sh` ailleurs. C'est Kraken qui la lance. Elle parle en stream-json
//! : elle prend son modèle au quinzième argument de sa ligne de commande (celui qui suit `--model`), répond aux quatre
//! demandes de contrôle par `controle-<modele>.jsonl`, recopie son entrée dans son dossier temporaire, puis :
//!
//! - si sa consigne commence la correction (`Corrige au plus petit`), elle garde sa consigne dans `correction.txt` et
//!   écrit `sortie/correction.txt` dans son bac ;
//! - si sa consigne nomme `REMARQUES.lydite`, elle est le relecteur, à sa tentative `n` (1, puis 2 une fois posée la
//!   marque `relecteur-1`) : elle garde son `PATH` dans `path-relecteur-<n>.txt` et le `DIFF.lydite` de son bac dans
//!   `diff-<n>.txt` (et son `PREUVE.lydite` dans `preuve-lydite-<n>.txt`), copie `remarques-<n>.lydite` en
//!   `REMARQUES.lydite` à la racine de son bac, puis joue elle-même
//!   `lydite remarques REMARQUES.lydite`, que son `PATH` doit trouver, et en garde la sortie dans `preuve-<n>.txt` et
//!   le code dans `code-<n>.txt` ;
//! - sinon, elle est la main d'une tâche : elle écrit sa consigne dans `sortie/<bac>.txt` de son bac si elle dit «
//!   critique », dans `autre/<bac>.txt` sinon.
//!
//! Elle finit par l'ouverture et le résultat de `fin.jsonl`. Tout ce qu'elle garde est à côté d'elle. Le profil d'essai
//! exige le mur, donne `haiku` aux tâches, `sonnet` à la relance, et `opus` au relecteur : trois modèles, trois
//! fichiers de contrôle, dont la liste des règles porte celles de toute main. Aucun test ne pose de variable dans son
//! propre processus (l'environnement va à l'enfant seul, par `Command::env`), ne lance un vrai `claude` ou un vrai
//! `gh`, ni ne cherche à sortir d'un bac.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// La preuve de la vague, que Kraken prête aux mains des tâches et de la correction, et celle qu'il prête au relecteur.
const VERIFIER: &str = "git status";
const PREUVE: &str = "lydite remarques REMARQUES.lydite";

/// Le plan des essais : a, critique, écrit sous `sortie/` ; b, qui ne l'est pas, sous `autre/`.
const PLAN: &str = "plan 1\nobjectif : Ecrire deux fichiers\npreuve : git status\n\n\
                    tache a\nfichiers : sortie/\ncritique : oui\n    Ecrire le fichier critique\n\n\
                    tache b\nfichiers : autre/\n    Ecrire l'autre fichier\n";

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
// réseau. `MODELE` est le modèle de la main.
const GET_SETTINGS: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_settings","response":{"applied":"#,
    r#"{"model":"MODELE"},"effective":{"disableAllHooks":true,"autoMemoryEnabled":false,"sandbox":{"enabled":true,"f"#,
    r#"ailIfUnavailable":true,"allowUnsandboxedCommands":false,"network":{"allowedDomains":[],"strictAllowlist":true"#,
    r#"},"credentials":{"envVars":[{"name":"ANTHROPIC_API_KEY","mode":"deny"},"#,
    r#"{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}}},"#,
    r#""sources":[{"source":"userSettings","settings":{}},{"source":"flagSettings","settings":{}}]}}}"#,
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
/// entre parenthèses : un `PATH` qui porte une parenthèse les fermerait. Le modèle est le quinzième argument : il est
/// `%9` après six `shift`.
const FAUSSE_WINDOWS: &[&str] = &[
    "@echo off",
    "rem La fausse claude des essais de la relecture.",
    r#"set "ici=%~dp0""#,
    "for %%I in (.) do set bac=%%~nxI",
    "shift",
    "shift",
    "shift",
    "shift",
    "shift",
    "shift",
    "set modele=%9",
    // La règle des leçons nomme le bac de la main : `@BAC@` y est remplacé par sa règle.
    // PowerShell rend le dossier courant en nom long, la règle de Lydite le garde tel quel : `%CD%` aussi.
    r#"set "bac_cd=%CD%""#,
    concat!(
        r#"powershell -NoProfile -Command "$p=$env:bac_cd; "#,
        r#"$r='//'+$p.Substring(0,1).ToLower()+$p.Substring(2).Replace('\','/'); "#,
        r#"[Console]::Out.Write((Get-Content -Raw '%ici%controle-%modele%.jsonl').Replace('@BAC@',$r))""#,
    ),
    r#"findstr "^" > "%TEMP%\entree.jsonl""#,
    r#"findstr /l /c:"Corrige au plus petit" "%TEMP%\entree.jsonl" > nul"#,
    "if not errorlevel 1 goto correction",
    r#"findstr /l /c:"REMARQUES.lydite" "%TEMP%\entree.jsonl" > nul"#,
    "if not errorlevel 1 goto relecteur",
    "set dossier=autre",
    r#"findstr /l /c:"critique" "%TEMP%\entree.jsonl" > nul"#,
    "if not errorlevel 1 set dossier=sortie",
    "if not exist %dossier% mkdir %dossier%",
    r#"copy /y "%TEMP%\entree.jsonl" "%dossier%\%bac%.txt" > nul"#,
    "goto fin",
    ":correction",
    r#"copy /y "%TEMP%\entree.jsonl" "%ici%correction.txt" > nul"#,
    "if not exist sortie mkdir sortie",
    r#"echo corrige> sortie\correction.txt"#,
    "goto fin",
    ":relecteur",
    "set n=1",
    r#"if exist "%ici%relecteur-1" set n=2"#,
    r#"type nul > "%ici%relecteur-%n%""#,
    r#"set PATH > "%ici%path-relecteur-%n%.txt""#,
    r#"if exist DIFF.lydite copy /y DIFF.lydite "%ici%diff-%n%.txt" > nul"#,
    r#"if exist PREUVE.lydite copy /y PREUVE.lydite "%ici%preuve-lydite-%n%.txt" > nul"#,
    r#"if exist "%ici%remarques-%n%.lydite" copy /y "%ici%remarques-%n%.lydite" REMARQUES.lydite > nul"#,
    r#"lydite remarques REMARQUES.lydite > "%ici%preuve-%n%.txt" 2>&1"#,
    // La redirection d'abord : `echo 0> f` redirigerait l'entrée, et écrirait l'état d'echo sur la sortie.
    r#">"%ici%code-%n%.txt" echo %errorlevel%"#,
    ":fin",
    r#"type "%ici%fin.jsonl""#,
    "exit /b 0",
];

/// La fausse claude des autres systèmes : le modèle est le quinzième argument.
const FAUSSE_UNIX: &[&str] = &[
    "#!/bin/sh",
    "# La fausse claude des essais de la relecture.",
    r#"ici=$(dirname "$0")"#,
    r#"bac=$(basename "$(pwd)")"#,
    "shift 14",
    r#"modele="$1""#,
    // La règle des leçons nomme le bac de la main : `@BAC@` y est remplacé par sa règle.
    r#"sed "s|@BAC@|/$(pwd)|g" "$ici/controle-$modele.jsonl""#,
    r#"cat > "$TMPDIR/entree.jsonl""#,
    r#"if grep -q 'Corrige au plus petit' "$TMPDIR/entree.jsonl"; then"#,
    r#"cp "$TMPDIR/entree.jsonl" "$ici/correction.txt""#,
    "mkdir -p sortie",
    "echo corrige > sortie/correction.txt",
    r#"elif grep -q 'REMARQUES\.lydite' "$TMPDIR/entree.jsonl"; then"#,
    "n=1",
    r#"if [ -f "$ici/relecteur-1" ]; then n=2; fi"#,
    r#": > "$ici/relecteur-$n""#,
    r#"printf 'PATH=%s\n' "$PATH" > "$ici/path-relecteur-$n.txt""#,
    r#"if [ -f DIFF.lydite ]; then cp DIFF.lydite "$ici/diff-$n.txt"; fi"#,
    r#"if [ -f PREUVE.lydite ]; then cp PREUVE.lydite "$ici/preuve-lydite-$n.txt"; fi"#,
    r#"if [ -f "$ici/remarques-$n.lydite" ]; then cp "$ici/remarques-$n.lydite" REMARQUES.lydite; fi"#,
    r#"lydite remarques REMARQUES.lydite > "$ici/preuve-$n.txt" 2>&1"#,
    r#"echo $? > "$ici/code-$n.txt""#,
    "else",
    "dossier=autre",
    r#"if grep -q 'critique' "$TMPDIR/entree.jsonl"; then dossier=sortie; fi"#,
    r#"mkdir -p "$dossier""#,
    r#"cp "$TMPDIR/entree.jsonl" "$dossier/$bac.txt""#,
    "fi",
    r#"cat "$ici/fin.jsonl""#,
    "exit 0",
];

/// Le faux srt de Windows, que `LYDITE_SRT` nomme : il saute `--settings <fichier>`, puis joue la preuve de `sh -c
/// <preuve>` par la coquille de Windows, où `sh` peut manquer, et `true`, l'épreuve du mur, sort en 0.
const FAUX_SRT_WINDOWS: &[&str] =
    &["@echo off", "rem Le faux srt des essais.", r#"if "%~5"=="true" exit /b 0"#, "%~5", "exit /b %errorlevel%"];

/// Le faux srt des autres systèmes : il saute `--settings <fichier>`, puis joue `sh -c <preuve>`.
const FAUX_SRT_UNIX: &[&str] = &["#!/bin/sh", "# Le faux srt des essais.", "shift 2", r#"exec "$@""#];

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    // Un nom court : Claude Code met ses sockets de pont dans son dossier temporaire (`<dossier
    // temporaire>/claude-http-<16 hex>.sock`), et un chemin de socket tient en 107 octets.
    let dir = std::env::temp_dir().join(format!("kr-{nom}-{}", std::process::id()));
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

/// Le texte d'un fichier de remarques : ces lignes, chacune finie par `\n`.
fn texte(lignes: &[&str]) -> String {
    lignes.iter().map(|ligne| format!("{ligne}\n")).collect()
}

/// Des remarques justes, dont une bloquante.
fn remarques_justes() -> String {
    texte(&[
        "remarques 2",
        "",
        "remarque 1",
        "fichier : sortie/a.txt",
        "ligne : 3",
        "bloquante : oui",
        "    Le test ne peut pas echouer.",
        "    Ajouter une assertion.",
        "",
        "remarque 2",
        "fichier : sortie/a.txt",
        "bloquante : non",
        "    Un nom peu clair.",
        "",
        "rejoue",
        "    La preuve de la vague est verte.",
        "",
        "non rejoue",
        "    Le réseau n'est pas joué.",
    ])
}

/// La valeur du `PATH` que la fausse claude a gardée : la ligne `PATH=<valeur>`, le nom en toute casse.
fn path_garde(fichier: &Path) -> String {
    let octets = fs::read(fichier).unwrap_or_else(|e| panic!("{} : {e}", fichier.display()));
    let texte = String::from_utf8_lossy(&octets);
    let ligne = texte.lines().find(|ligne| ligne.get(..5).is_some_and(|nom| nom.eq_ignore_ascii_case("PATH=")));
    ligne.map(|ligne| ligne[5..].to_string()).unwrap_or_else(|| panic!("aucun PATH dans {texte}"))
}

/// Les lignes du fil qui sont de ce type, dans l'ordre du fil.
fn lignes_de<'a>(fil: &'a str, genre: &str) -> Vec<&'a str> {
    let type_ = format!(r#""type":"{genre}""#);
    fil.lines().filter(|ligne| ligne.contains(&type_)).collect()
}

/// Les ids des vagues que le fil ouvre, dans l'ordre.
fn vagues(fil: &str) -> Vec<String> {
    let id = |ligne: &str| {
        let debut = ligne.find(r#""vague":""#).expect("une ouverture nomme sa vague") + 9;
        ligne[debut..].split('"').next().unwrap().to_string()
    };
    lignes_de(fil, "vague.ouverte").into_iter().map(id).collect()
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
    /// Un essai dont le profil dit `relecteur = <relecteur>`, ou ne dit rien du relecteur s'il n'en donne pas.
    fn nouveau(nom: &str, relecteur: Option<&str>) -> Essai {
        let racine = neuf(nom);
        let (depot, main) = (racine.join("depot"), racine.join("main"));
        fs::create_dir(&depot).unwrap();
        fs::create_dir(&main).unwrap();
        git(&depot, &["init", "-q"]);
        git(&depot, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let essai = Essai { dossier: racine.join("lydite"), racine, depot, main };
        essai.poser_la_fausse_claude();
        essai.accepter_le_profil(relecteur);
        essai.poser_les_reponses();
        fs::write(essai.racine.join("plan.lydite"), PLAN).unwrap();
        essai
    }

    /// La fausse claude, écrite puis rendue exécutable : c'est Kraken qui la lance, pas le test. Puis, de même, le faux
    /// srt, à la racine de l'essai.
    fn poser_la_fausse_claude(&self) {
        let (nom, lignes, srt, fin) = if cfg!(windows) {
            ("fausse-claude.cmd", FAUSSE_WINDOWS, FAUX_SRT_WINDOWS, "\r\n")
        } else {
            ("fausse-claude.sh", FAUSSE_UNIX, FAUX_SRT_UNIX, "\n")
        };
        for (script, lignes) in [(self.main.join(nom), lignes), (self.faux_srt(), srt)] {
            fs::write(&script, lignes.iter().map(|ligne| format!("{ligne}{fin}")).collect::<String>()).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
    }

    /// Le faux srt de l'essai, que `kraken` nomme en `LYDITE_SRT` : sous le mur exigé, Kraken rejoue toute preuve par
    /// lui.
    fn faux_srt(&self) -> PathBuf {
        self.racine.join(if cfg!(windows) { "faux-srt.cmd" } else { "faux-srt.sh" })
    }

    /// Le profil `essai` : la fausse claude, `haiku` aux tâches, `sonnet` à la relance (donc à la correction), le
    /// modèle qu'on donne au relecteur ou, sans lui, le défaut du profil (`opus`), le mur exigé ; puis son acceptation.
    fn accepter_le_profil(&self, relecteur: Option<&str>) {
        let fausse = self.main.join(if cfg!(windows) { "fausse-claude.cmd" } else { "fausse-claude.sh" });
        let relecteur = relecteur.map(|modele| format!("relecteur = {modele}\n")).unwrap_or_default();
        let texte = format!(
            "sorte = claude\nprogramme = {}\nmodele = haiku\nrelance = sonnet\n{relecteur}mur = exige\n\
             delai_main = 120\ndelai_preuve = 60\n",
            fausse.display()
        );
        fs::create_dir_all(self.dossier.join("profils")).unwrap();
        fs::write(self.dossier.join("profils").join("essai.profil"), texte).unwrap();
        let sortie = self.kraken().args(["accepter", "essai"]).output().unwrap();
        assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    }

    /// Ce que la fausse claude répond à toute main : les quatre réponses de contrôle, au modèle de la main (un fichier
    /// par modèle), dont la liste des règles porte celles de toutes les mains, puis l'ouverture et un résultat vert.
    fn poser_les_reponses(&self) {
        let mut allow: Vec<String> = ["Skill", "Workflow", "ExitPlanMode"].map(String::from).into();
        for pretee in [PREUVE, VERIFIER] {
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
        deny.extend(
            ["Bash(git commit:*)", "Bash(git commit *)", "PowerShell(git commit:*)", "PowerShell(git commit *)"]
                .map(String::from),
        );
        // Les leçons, protégées de toute main : la fausse claude met le bac de la main à la place de `@BAC@`.
        deny.push("Edit(@BAC@/.agents/skills/**)".into());
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
        for modele in ["haiku", "sonnet", "opus", "claude-opus-5-5"] {
            let reglages = GET_SETTINGS.replace("MODELE", modele);
            let controle = [reglages.as_str(), &regles, GET_HOOKS_LISTING, GET_SANDBOX_DIALOG].join("\n") + "\n";
            fs::write(self.main.join(format!("controle-{modele}.jsonl")), controle).unwrap();
        }
        fs::write(self.main.join("fin.jsonl"), format!("{OUVERTURE}\n{RESULTAT}\n")).unwrap();
    }

    /// `kraken`, prêt à lancer sans qu'aucune vraie main ne puisse partir ni rien de l'humain ne soit lu : le dossier
    /// de Kraken, un `claude` qui n'existe pas pour le profil intégré, le faux srt, et les leurres du dossier personnel
    /// et de `CLAUDE_CONFIG_DIR`, jamais créés.
    fn kraken(&self) -> Command {
        let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
        let personnel = self.racine.join("personnel");
        commande
            .current_dir(&self.racine)
            .env("LYDITE_DOSSIER", &self.dossier)
            .env("LYDITE_CLAUDE", self.racine.join("claude-absent"))
            .env("LYDITE_SRT", self.faux_srt())
            .env("USERPROFILE", &personnel)
            .env("HOME", personnel)
            .env("CLAUDE_CONFIG_DIR", self.racine.join("config"));
        commande
    }

    /// Les remarques que la fausse claude pose à sa tentative `n` de relecteur.
    fn remarques(&self, n: usize, texte: &str) {
        fs::write(self.main.join(format!("remarques-{n}.lydite")), texte).unwrap();
    }

    /// `lydite lancer essai --plan plan.lydite --depot <depot>`.
    fn lancer(&self) -> Output {
        let mut commande = self.kraken();
        commande.args(["lancer", "essai", "--plan"]).arg(self.racine.join("plan.lydite")).arg("--depot");
        commande.arg(&self.depot).output().unwrap()
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

    /// `lydite etat`.
    fn etat(&self) -> Output {
        self.kraken().arg("etat").output().unwrap()
    }
}

/// Un fichier de remarques sur le disque, dans un dossier neuf, que `lydite remarques` lit.
fn remarques_sur_disque(nom: &str, octets: &[u8]) -> (PathBuf, PathBuf) {
    let dir = neuf(nom);
    let fichier = dir.join("REMARQUES.lydite");
    fs::write(&fichier, octets).unwrap();
    (dir, fichier)
}

#[test]
fn kraken_remarques_sort_en_0_sur_un_fichier_juste_et_ne_fait_rien_d_autre() {
    let sans_remarque = b"remarques 2\n\nrejoue\n    Rien de rejoue.\n\nnon rejoue\n    Rien.\n".to_vec();
    for (nom, octets) in [("juste", remarques_justes().into_bytes()), ("vide", sans_remarque.clone())] {
        let (dir, fichier) = remarques_sur_disque(&format!("remarques-{nom}"), &octets);
        let sortie = Command::new(env!("CARGO_BIN_EXE_lydite")).arg("remarques").arg(&fichier).output().unwrap();
        assert_eq!(sortie.status.code(), Some(0), "{nom} : {}", dit(&sortie));
        assert!(sortie.stdout.is_empty() && sortie.stderr.is_empty(), "rien d'autre qu'un code : {}", dit(&sortie));
        // Il lit, il n'écrit pas : le dossier ne porte que le fichier.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        assert_eq!(fs::read(&fichier).unwrap(), octets);
        let _ = fs::remove_dir_all(&dir);
    }
    // Un chemin relatif part du dossier courant.
    let (dir, _) = remarques_sur_disque("remarques-relatif", &sans_remarque);
    let sortie = Command::new(env!("CARGO_BIN_EXE_lydite"))
        .current_dir(&dir)
        .args(["remarques", "REMARQUES.lydite"])
        .output()
        .unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn kraken_remarques_sort_en_2_sur_un_refus_de_quatre_lignes_sans_panique() {
    let (juste, trop) = (remarques_justes(), vec![b'x'; 1_000_001]);
    let cas: Vec<(&str, Vec<u8>, &str)> = vec![
        ("vide", Vec::new(), "7015 vague.remarques_illisibles"),
        ("plan", b"plan 1\n".to_vec(), "7015 vague.remarques_illisibles"),
        ("forme-1", b"remarques 1\n".to_vec(), "7015 vague.remarques_illisibles"),
        ("forme-zero", b"remarques 0\n".to_vec(), "7015 vague.remarques_illisibles"),
        ("bloc-sous-1", b"remarques 1\nrejoue\n    a\n".to_vec(), "7015 vague.remarques_illisibles"),
        ("sans-texte", b"remarques 1\nremarque 1\nfichier : a\nbloquante : oui\n".to_vec(), "7015"),
        ("hors-numero", juste.replace("remarque 2", "remarque 3").into_bytes(), "7015"),
        ("hors-ordre", juste.replace("ligne : 3\nbloquante : oui", "bloquante : oui\nligne : 3").into_bytes(), "7015"),
        ("utf16", b"\xFF\xFEr\x00e\x00".to_vec(), "7015"),
        ("pas-utf8", b"remarques 1\n\xC3(\n".to_vec(), "7015"),
        ("lourd", trop, "7015"),
        ("binaire", (0..=255u8).collect(), "7015"),
    ];
    for (nom, octets, code) in cas {
        let (dir, fichier) = remarques_sur_disque(&format!("refus-{nom}"), &octets);
        let sortie = Command::new(env!("CARGO_BIN_EXE_lydite")).arg("remarques").arg(&fichier).output().unwrap();
        let erreur = String::from_utf8_lossy(&sortie.stderr);
        assert_eq!(sortie.status.code(), Some(2), "{nom} : {}", dit(&sortie));
        assert!(sortie.stdout.is_empty() && !erreur.contains("panicked"), "{nom} : {}", dit(&sortie));
        let lignes: Vec<&str> = erreur.lines().collect();
        assert_eq!(lignes.len(), 4, "{nom} : {erreur}");
        assert!(lignes[0].starts_with(&format!("lydite : {code}")), "{nom} : {erreur}");
        for (ligne, debut) in lignes[1..].iter().zip(["  règle : ", "  preuve : ", "  correction : "]) {
            assert!(ligne.starts_with(debut) && ligne.len() > debut.len(), "{nom} : {erreur}");
        }
        let _ = fs::remove_dir_all(&dir);
    }
    // Un fichier qu'on ne lit pas, absent ou dossier : le refus 6006, qui nomme son chemin, et sort en 2 aussi.
    let dir = neuf("refus-absent");
    for chemin in [dir.join("absent.lydite"), dir.clone()] {
        let sortie = Command::new(env!("CARGO_BIN_EXE_lydite")).arg("remarques").arg(&chemin).output().unwrap();
        let erreur = String::from_utf8_lossy(&sortie.stderr);
        assert_eq!(sortie.status.code(), Some(2), "{}", dit(&sortie));
        assert!(erreur.starts_with("lydite : 6006 lancer.entree_sortie\n"), "{erreur}");
        assert!(erreur.contains(&format!("lecture des remarques {}", chemin.display())), "{erreur}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn kraken_remarques_veut_un_fichier_et_un_seul() {
    let (dir, fichier) = remarques_sur_disque("remarques-ligne", b"remarques 1\n");
    let cas: Vec<(Vec<&std::ffi::OsStr>, &str)> = vec![
        (vec![], "`remarques` : il manque le fichier des remarques"),
        (vec!["--decoupe".as_ref(), fichier.as_os_str()], "« --decoupe » est un drapeau"),
        (vec![fichier.as_os_str(), "autre".as_ref()], "« autre » le suit"),
    ];
    for (args, vu) in cas {
        let sortie = Command::new(env!("CARGO_BIN_EXE_lydite")).arg("remarques").args(&args).output().unwrap();
        let erreur = String::from_utf8_lossy(&sortie.stderr);
        assert_eq!(sortie.status.code(), Some(2), "{args:?} : {}", dit(&sortie));
        assert!(erreur.starts_with("lydite : 1006 contrat.commande_mal_formee\n"), "{erreur}");
        assert!(erreur.contains(vu) && erreur.contains("lydite remarques <file>"), "{erreur}");
    }
    // Le mot est dans la liste fermée que le refus 1005 donne.
    let sortie = Command::new(env!("CARGO_BIN_EXE_lydite")).arg("Remarques").output().unwrap();
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert!(erreur.starts_with("lydite : 1005 contrat.commande_inconnue\n"), "{erreur}");
    assert!(erreur.contains("lydite remarques <file>"), "{erreur}");
    let _ = fs::remove_dir_all(&dir);
}

/// Les modèles des tentatives de la vague `vague`, d'après ses `main.lancement`, dans l'ordre du fil et joints par des
/// virgules.
fn modeles_de(fil: &str, vague: &str) -> String {
    let siens =
        lignes_de(fil, "main.lancement").into_iter().filter(|ligne| ligne.contains(&format!(r#""vague":"{vague}""#)));
    let modele = |ligne: &str| {
        let debut = ligne.find(r#""modele":""#).unwrap() + 10;
        ligne[debut..].split('"').next().unwrap().to_string()
    };
    siens.map(modele).collect::<Vec<_>>().join(",")
}

#[test]
fn le_travail_critique_se_relit_et_sa_correction_s_assemble() {
    let essai = Essai::nouveau("relue", Some("claude-opus-5-5"));
    essai.remarques(1, &remarques_justes());
    let sortie = essai.lancer();
    assert_eq!(sortie.status.code(), Some(0), "{}", dit(&sortie));
    let fil = essai.fil();
    let ouvertes = vagues(&fil);
    let [v, r, c] = &ouvertes[..] else { panic!("la vague, le relecteur, la correction : {fil}") };
    // Sur stderr : les tâches, puis la relecture et la correction assemblée, d'où la vague finit.
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    let t2 = {
        let fusions = lignes_de(&fil, "vague.fusion");
        let derniere = fusions.last().unwrap();
        let debut = derniere.find(r#""sha":""#).unwrap() + 7;
        derniere[debut..].split('"').next().unwrap().to_string()
    };
    assert!(erreur.contains(&format!("relecture prise : main-{r}-1 · 2 remarques, dont 1 bloquantes\n")), "{erreur}");
    assert!(erreur.contains(&format!("correction assemblée : main-{c}-1 · la vague finit à {t2}\n")), "{erreur}");
    // La relecture est au fil, avec le texte du relecteur ; la correction est liée à la vague relue.
    let notee = lignes_de(&fil, "vague.relecture");
    let [notee] = &notee[..] else { panic!("{fil}") };
    assert!(
        notee.contains(&format!(r#""vague":"{r}","relue":"{v}","n":2,"bloquantes":1,"remarques":"remarques 2"#)),
        "{notee}"
    );
    assert!(fil.contains(&format!(r#""type":"vague.correction","vague":"{c}","corrige":"{v}""#)), "{fil}");
    // Les modèles : `modele` aux tâches, la clé `relecteur` du profil au relecteur (ici une valeur qui n'est pas son
    // défaut), `relance` à la correction.
    let modeles = (modeles_de(&fil, v), modeles_de(&fil, r), modeles_de(&fil, c));
    assert_eq!(modeles, ("haiku,haiku".to_string(), "claude-opus-5-5".to_string(), "sonnet".to_string()));
    // Le relecteur a joué lui-même `lydite remarques REMARQUES.lydite`, que son PATH trouve : sans rien dire, code 0.
    assert_eq!((essai.garde("code-1.txt").trim(), essai.garde("preuve-1.txt")), ("0", String::new()));
    let separateur = if cfg!(windows) { ';' } else { ':' };
    let (tete, _) = path_garde(&essai.main.join("path-relecteur-1.txt"))
        .split_once(separateur)
        .map(|(t, r)| (t.to_string(), r.to_string()))
        .expect("un PATH de plusieurs dossiers");
    let exe = Path::new(env!("CARGO_BIN_EXE_lydite")).parent().unwrap();
    assert_eq!(fs::canonicalize(tete).unwrap(), fs::canonicalize(exe).unwrap());
    // Son bac portait le diff du seul travail critique, et la correction a reçu la seule remarque bloquante.
    let diff = essai.garde("diff-1.txt");
    assert!(diff.contains(&format!("diff --git a/sortie/main-{v}-1.txt b/sortie/main-{v}-1.txt")), "{diff}");
    assert!(!diff.contains("autre/"), "{diff}");
    // Et la preuve que Lydite a rejouée sur la tête relue : la commande de la vague, le commit, la sortie.
    let tete_relue = {
        let siennes: Vec<&str> =
            lignes_de(&fil, "vague.fusion").into_iter().filter(|l| l.contains(&format!(r#""vague":"{v}""#))).collect();
        let derniere = siennes.last().unwrap();
        let debut = derniere.find(r#""sha":""#).unwrap() + 7;
        derniere[debut..].split('"').next().unwrap().to_string()
    };
    let preuve = essai.garde("preuve-lydite-1.txt");
    let entete =
        format!("Proof command: {VERIFIER}\nReplayed green by Lydite on commit: {tete_relue}\nOutput (last 4000");
    assert!(preuve.starts_with(&entete), "{preuve}");
    let correction = essai.garde("correction.txt");
    assert!(correction.contains("sortie/a.txt, ligne 3 : Le test ne peut pas echouer."), "{correction}");
    assert!(!correction.contains("Un nom peu clair"), "{correction}");
    // La branche de la vague a avancé à la tête de la correction, qui porte son fichier ; le bac du relecteur est
    // retiré.
    let branche = format!("lydite/vague-{v}");
    assert_eq!(git(&essai.depot, &["rev-parse", &branche]), t2);
    let arbre = git(&essai.depot, &["ls-tree", "-r", "--name-only", &branche]);
    assert_eq!(arbre, format!("autre/main-{v}-2.txt\nsortie/correction.txt\nsortie/main-{v}-1.txt"));
    assert!(!essai.dossier.join("bacs").join(format!("main-{r}-1")).exists(), "le bac du relecteur est retiré");
    assert_eq!(git(&essai.depot, &["branch", "--list", &format!("lydite/main-{r}-1")]), "");
    // `lydite etat` montre la chaîne entière et sort en 0.
    let etat = essai.etat();
    let montre = String::from_utf8_lossy(&etat.stdout);
    assert_eq!(etat.status.code(), Some(0), "{montre}");
    assert!(montre.contains("Relecture : 2 remarques, dont 1 bloquantes\n"), "{montre}");
    assert!(montre.contains(&format!("Correction : main-{c}-1 correction : assemblée")), "{montre}");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn un_relecteur_sans_fichier_classe_la_relecture_et_rien_ne_se_pousse() {
    // Le relecteur n'écrit rien : sa propre preuve échoue (6006, code 2), et celle de Kraken aussi (7014), deux fois.
    // Le profil ne dit rien de `relecteur` : ses deux tentatives partent sur le défaut, opus.
    let essai = Essai::nouveau("classee", None);
    let sortie = essai.lancer();
    assert_eq!(sortie.status.code(), Some(1), "{}", dit(&sortie));
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert!(erreur.contains("lydite : 7016 vague.relecture_manquee\n"), "{erreur}");
    assert!(erreur.contains("est classée : main-"), "{erreur}");
    assert!(erreur.contains("7014 vague.bac_touche"), "{erreur}");
    assert!(!erreur.contains("relecture prise"), "{erreur}");
    assert_eq!(essai.garde("code-1.txt").trim(), "2");
    assert!(essai.garde("preuve-1.txt").starts_with("lydite : 6006 lancer.entree_sortie\n"));
    // Ni relecture ni correction au fil, mais le refus 7016 de la forge, noté sur la vague relue.
    let fil = essai.fil();
    let ouvertes = vagues(&fil);
    let [v, r] = &ouvertes[..] else { panic!("la vague et le relecteur : {fil}") };
    assert_eq!(modeles_de(&fil, r), "opus,opus");
    assert!(!fil.contains(r#""type":"vague.relecture""#) && !fil.contains(r#""type":"vague.correction""#), "{fil}");
    let refus = lignes_de(&fil, "forge.refus");
    let [refus] = &refus[..] else { panic!("{fil}") };
    assert!(refus.contains(&format!(r#""vague":"{v}","code":7016"#)), "{refus}");
    // La branche de la vague est restée à sa tête, et `lydite etat` sort en 1.
    let tete = git(&essai.depot, &["rev-parse", &format!("lydite/vague-{v}")]);
    let fusions = lignes_de(&fil, "vague.fusion");
    assert!(fusions.last().unwrap().contains(&format!(r#""sha":"{tete}""#)), "{fil}");
    assert_eq!(essai.etat().status.code(), Some(1));
    let _ = fs::remove_dir_all(&essai.racine);
}
