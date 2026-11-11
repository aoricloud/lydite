//! Les tests de la main Claude Code : son processus, son dialogue et son dossier temporaire.
use super::*;
use crate::coeur::json::{self, Json};
use crate::coeur::protocole::{Jetons, Usage, UsageModele};
use crate::essais::mains::{DELAI, arbre, bac_jetable, profil_borne};
use crate::essais::{ARBRE, effacer, marque, meurt};

// Les lignes de Claude Code réduites aux champs que Kraken lit : celles des tests de protocole.rs,
// sauf `WebSearch`, que l'ouverture n'a plus dans `tools` : un profil la ferme d'office.
const GET_SETTINGS: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_settings","response""#,
    r#":{"applied":{"model":"claude-sonnet-5-5"},"effective":{"disableAllHooks":true,"autoMemoryEnabled""#,
    r#":false,"sandbox":{"enabled":true,"failIfUnavailable":false,"allowUnsandboxedCommands":false,"#,
    r#""filesystem":{"denyRead":["~/","/srv/lydite"],"allowRead":["/srv/lydite/bacs/main-1","/srv/depot/.git"]},"#,
    r#""network":{"allowedDomains":[],"strictAllowlist":true},"credentials":{"envVars":[{"name":"#,
    r#""ANTHROPIC_API_KEY","mode":"deny"},{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}}},"#,
    r#""sources":[{"source":"userSettings","#,
    r#""settings":{}},{"source":"flagSettings","settings":{}}]}}}"#,
);
const LIST_PERMISSION_RULES: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"list_permission_rules","#,
    r#""response":{"state":{"rules":[{"behavior":"allow","source":"flagSettings","rule":"WebSearch"},"#,
    r#"{"behavior":"deny","source":"flagSettings","rule":"CronCreate"},{"behavior":"deny","source""#,
    r#":"userSettings","rule":"Read(~/.ssh/**)"}]}}}}"#,
);
const GET_HOOKS_LISTING: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_hooks_listing","#,
    r#""response":{"policy":{"disabledByPolicy":false,"managedOnly":false,"pluginOnly":false,"allDisabled""#,
    r#":true,"policyHookCount":0},"hooks":[],"events":[],"eventCatalog":[]}}}"#,
);
const GET_SANDBOX_DIALOG: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_sandbox_dialog","#,
    r#""response":{"supported":false,"unsupported_reason":"platform","enabled":false,"enabled_in_settings""#,
    r#":true,"unavailable_reason":"sandbox is enabled but the Windows sandbox is not active on this session"#,
    r#" (feature gate off)","mode":"regular"}}}"#,
);
const OUVERTURE: &str = concat!(
    r#"{"type":"system","subtype":"init","cwd":"/srv/bac","tools":["Task","Bash","Edit","Read","Skill","#,
    r#""WebFetch","Workflow","Write"],"mcp_servers":[],"model":"claude-sonnet-5-5","permissionMode""#,
    r#":"acceptEdits","claude_code_version":"2.1.312"}"#,
);
const RESULTAT: &str =
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"fait\nPREUVE: vert"}"#;
/// La seule ligne d'une main au mur exigé, sans mur : elle sort en code 1, avant ses réponses.
const SANS_MUR: &str = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":0}"#;
/// Les quatre réponses justes, dans l'ordre des demandes.
const JUSTES: [&str; 4] = [GET_SETTINGS, LIST_PERMISSION_RULES, GET_HOOKS_LISTING, GET_SANDBOX_DIALOG];
/// Un `--settings` de la forme que Kraken passe, au mur demandé, réduit aux deux règles des réponses relevées.
const REGLAGES: &str = concat!(
    r#"{"sandbox":{"enabled":true,"failIfUnavailable":false,"allowUnsandboxedCommands":false,"filesystem""#,
    r#":{"denyRead":["~/","/srv/lydite"],"allowRead":["/srv/lydite/bacs/main-1","/srv/depot/.git"]},"#,
    r#""network":{"allowedDomains":[],"strictAllowlist":true},"credentials":{"envVars":[{"name":"#,
    r#""ANTHROPIC_API_KEY","mode":"deny"},{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}},"#,
    r#""permissions":{"allow":["WebSearch"],"deny""#,
    r#":["CronCreate"]},"disableAllHooks":true,"autoMemoryEnabled":false}"#,
);

/// Ce que Kraken passe à une main sur ces réglages : `--model sonnet`, sans `mcp`.
fn attendu() -> Attendu<'static> {
    Attendu { reglages: REGLAGES, modele: "sonnet", effort: None, mcp: None }
}

/// Un geste d'une fausse main Claude.
enum Geste<'a> {
    /// Lit une ligne de son entrée et la recopie dans `entree.txt`, au dossier courant.
    Lire,
    /// Écrit cette ligne sur sa sortie, telle quelle.
    Dire(&'a str),
    /// Recopie le reste de son entrée au bout d'`entree.txt`, jusqu'à sa fin.
    Recopier,
    /// Écrit cette ligne sur ses erreurs.
    Plaindre(&'a str),
    /// Dort 30 s.
    Dormir,
    /// Sort sur ce code.
    Sortir(u8),
}

/// Une fausse main Claude qui joue ces gestes, un script écrit dans `dir` : un `.bat` sous Windows, lancé par `sh`
/// ailleurs. Rend son argv.
fn fausse_claude(dir: &Path, gestes: &[Geste]) -> Vec<String> {
    let windows = cfg!(windows);
    let lignes = gestes.iter().map(|geste| match geste {
        Geste::Lire if windows => "set /p demande=\r\n>entree.txt echo %demande%".to_string(),
        Geste::Lire => r#"IFS= read -r demande; printf '%s\n' "$demande" > entree.txt"#.to_string(),
        Geste::Dire(ligne) if windows => format!("echo {ligne}"),
        Geste::Dire(ligne) => format!("printf '%s\\n' '{ligne}'"),
        Geste::Recopier if windows => r#"findstr "^" >> entree.txt"#.to_string(),
        Geste::Recopier => "cat >> entree.txt".to_string(),
        Geste::Plaindre(ligne) if windows => format!(">&2 echo {ligne}"),
        Geste::Plaindre(ligne) => format!("printf '%s\\n' '{ligne}' >&2"),
        Geste::Dormir if windows => "ping -n 31 127.0.0.1 >nul".to_string(),
        Geste::Dormir => "sleep 30".to_string(),
        Geste::Sortir(code) => format!("exit {code}"),
    });
    if windows {
        let script = dir.join("claude.bat");
        let texte: String = std::iter::once("@echo off".to_string()).chain(lignes).map(|l| l + "\r\n").collect();
        fs::write(&script, texte).unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("claude.sh");
        fs::write(&script, lignes.map(|l| l + "\n").collect::<String>()).unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Ce que la fausse main a recopié de son entrée, en fins de ligne `\n`.
fn entree(bac: &Path) -> String {
    String::from_utf8_lossy(&fs::read(bac.join("entree.txt")).unwrap()).replace("\r\n", "\n")
}

/// La première demande de contrôle, que la fausse main lit avant de répondre.
fn premiere_demande() -> String {
    protocole::demandes().lines().next().unwrap().to_string()
}

/// La réponse de `list_permission_rules` d'une main fidèle à ces réglages déclarés : chaque règle de `allow` puis de
/// `deny`, à la source `flagSettings`, sous la même chaîne.
fn regles_fideles(reglages: &str) -> String {
    let Ok(Json::Objet(racine)) = json::document(reglages) else { panic!("{reglages}") };
    let Some(Json::Objet(permissions)) = racine.get("permissions") else { panic!("{reglages}") };
    let mut regles = Vec::new();
    for comportement in ["allow", "deny"] {
        let Some(Json::Liste(liste)) = permissions.get(comportement) else { panic!("{reglages}") };
        for regle in liste {
            let Json::Texte(regle) = regle else { panic!("{regle:?}") };
            let regle = json::chaine(regle);
            regles.push(format!(r#"{{"behavior":"{comportement}","source":"flagSettings","rule":{regle}}}"#));
        }
    }
    let rules = regles.join(",");
    format!(
        concat!(
            r#"{{"type":"control_response","response":{{"subtype":"success","request_id":"list_permission_rules","#,
            r#""response":{{"state":{{"rules":[{rules}]}}}}}}}}"#,
        ),
        rules = rules,
    )
}

#[test]
fn la_main_trop_lente_est_tuee_avec_ses_enfants() {
    let (dir, bac) = bac_jetable("main-lente");
    let (constat, sortie) = jouer(&arbre(&dir), &bac, "consigne", Duration::from_secs(20), None, false, None);
    let erreur = sortie.unwrap_err();
    assert!(matches!(erreur, ErreurMain::Tuee(_)), "tuée à son délai, par son type : {erreur:?}");
    let erreur = erreur.texte();
    assert!(erreur.starts_with("hand: timed out (20 s)"), "{erreur}");
    assert_eq!(constat, None, "une main brute ne rend pas de constat");
    let pid = marque(&bac, Duration::ZERO);
    assert!(meurt(pid), "le petit-enfant {pid} survit à la main tuée");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn ce_que_la_main_verte_laisse_dans_son_bac_meurt_et_le_reste_vit() {
    let (dir, bac) = bac_jetable("main-orpheline");
    // Une fausse main qui laisse un `sleep` en arrière-plan dans son bac, détaché de ses tuyaux, et sort verte.
    let script = dir.join("orpheline.sh");
    fs::write(&script, "sleep 30 </dev/null >/dev/null 2>&1 &\necho $! > marque\necho fait\n").unwrap();
    // Un processus du même compte hors du bac : un frère du bac, dans `dir`.
    let mut dehors = Command::new("sleep").arg("30").current_dir(&dir).spawn().unwrap();
    let argv = vec!["sh".to_string(), script.to_string_lossy().into_owned()];
    let sortie = jouer(&argv, &bac, "", DELAI, None, false, None).1;
    assert_eq!(sortie.as_deref(), Ok("fait\n"), "la main reste verte");
    let pid = marque(&bac, Duration::ZERO);
    assert!(meurt(pid), "le sleep {pid} laissé dans le bac survit à la fin de la tentative");
    assert!(
        fs::metadata(format!("/proc/{}", dehors.id())).is_ok(),
        "le processus hors du bac a été tué avec les restes"
    );
    assert!(dehors.try_wait().unwrap().is_none(), "le processus hors du bac est sorti");
    dehors.kill().unwrap();
    dehors.wait().unwrap();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_main_sans_programme_est_une_erreur() {
    let (dir, bac) = bac_jetable("sans-programme");
    let demande = attendu();
    for attendu in [None, Some(&demande)] {
        let (constat, sortie) = jouer(&[], &bac, "consigne", DELAI, attendu, false, None);
        let erreur = sortie.unwrap_err();
        assert!(matches!(erreur, ErreurMain::Autre(_)), "une main qui ne part pas n'est pas tuée : {erreur:?}");
        let erreur = erreur.texte();
        assert!(erreur.starts_with("hand: no program to start"), "{erreur}");
        assert_eq!(constat, attendu.map(|_| Constat::default()), "rien ne s'est lu");
    }
    assert!(!temporaire_de(&bac).unwrap().exists(), "rien n'est parti, rien n'est créé");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_main_a_son_dossier_temporaire_a_cote_de_son_bac() {
    let (dir, bac) = bac_jetable("temporaire");
    // La main dit son dossier temporaire, et `reste` s'il y trouve le fichier qu'elle y laisse ensuite.
    let main: Vec<String> = if cfg!(windows) {
        let script = dir.join("temporaire.bat");
        let corps = "@echo off\r\necho %TEMP%\r\necho %TMP%\r\nif exist \"%TEMP%\\reste\" echo reste\r\n\
                     type nul > \"%TEMP%\\reste\"\r\n";
        fs::write(&script, corps).unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let corps = r#"printf '%s\n' "$TMPDIR"; if [ -e "$TMPDIR/reste" ]; then echo reste; fi; : > "$TMPDIR/reste""#;
        vec!["sh".into(), "-c".into(), corps.into()]
    };
    let temporaire = temporaire_de(&bac).unwrap();
    assert_eq!(temporaire.parent(), Some(dir.as_path()), "à côté du bac");
    let attendu = vec![temporaire.to_string_lossy().into_owned(); if cfg!(windows) { 2 } else { 1 }];
    // Deux tentatives dans le même bac, recréé vide avant chacune comme sous un mur exigé : chacune le trouve sans
    // ce que l'autre y a laissé.
    for _ in 0..2 {
        assert_eq!(super::temporaire(&bac, true), Ok(Some(temporaire.clone())));
        let vu = jouer(&main, &bac, "consigne", DELAI, None, false, None).1.unwrap();
        assert_eq!(vu.lines().map(str::trim_end).collect::<Vec<_>>(), attendu);
        assert!(temporaire.join("reste").is_file());
    }
    // Une relance sous un mur demandé ne vide pas le dossier : elle y trouve ce que la tentative d'avant a laissé.
    let vu = jouer(&main, &bac, "consigne", DELAI, None, false, None).1.unwrap();
    assert_eq!(vu.lines().map(str::trim_end).next_back(), Some("reste"));
    assert_eq!(fs::read_dir(&bac).unwrap().count(), 0, "rien n'est créé dans le bac");
    // Un bac absent reste une erreur : la main n'y part pas, et ni lui ni son dossier temporaire ne se créent.
    let absent = dir.join("absent");
    let erreur = jouer(&main, &absent, "consigne", DELAI, None, false, None).1.unwrap_err().texte();
    assert!(erreur.starts_with("cannot start "), "{erreur}");
    assert!(!absent.exists() && !temporaire_de(&absent).unwrap().exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_main_claude_rend_son_resultat_et_son_constat() {
    let (dir, bac) = bac_jetable("claude");
    let mut gestes = vec![Geste::Lire];
    gestes.extend(JUSTES.map(Geste::Dire));
    gestes.extend([Geste::Recopier, Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT)]);
    let consigne = r#"Ecris "bonjour" dans C:\bac"#;
    let (constat, sortie) = jouer(&fausse_claude(&dir, &gestes), &bac, consigne, DELAI, Some(&attendu()), false, None);
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()), "la sortie est le champ result");
    assert_eq!(
        constat,
        Some(Constat {
            version: Some("2.1.312".into()),
            mur_actif: Some(false),
            usage: Usage { tours: 1, ..Usage::default() },
            ..Constat::default()
        })
    );
    // La main a lu les demandes, puis, ses quatre réponses jugées, la consigne en message, en dernier.
    let lu = entree(&bac);
    assert!(lu.starts_with(&premiere_demande()), "{lu}");
    assert!(lu.ends_with(&protocole::message(consigne)), "la consigne part en dernier : {lu}");
    let _ = fs::remove_dir_all(&dir);
}

/// Une ouverture de l'abonnement : `apiKeySource` à `none`.
fn ouverture_abonnement() -> String {
    OUVERTURE.replace(r#""model""#, r#""apiKeySource":"none","model""#)
}

/// Une ligne `assistant` : un modèle et ses quatre compteurs, `entree` en entrée.
fn assistant(entree: u64) -> String {
    format!(
        concat!(
            r#"{{"type":"assistant","message":{{"model":"claude-haiku-5-5","usage":{{"input_tokens":{},"#,
            r#""output_tokens":3,"cache_creation_input_tokens":5,"cache_read_input_tokens":7}}}}}}"#,
        ),
        entree
    )
}

/// Le cumul de deux lignes `assistant` d'entrée 1 et 2.
fn cumul_de_deux() -> Usage {
    let jetons = Jetons { entree: 3, sortie: 6, cache_ecrit: 10, cache_lu: 14, reflexion: 0 };
    let haiku = UsageModele { modele: "claude-haiku-5-5".into(), jetons, ..UsageModele::default() };
    Usage { jetons, modeles: vec![haiku], ..Usage::default() }
}

#[test]
fn une_main_claude_remplit_son_usage_avec_assistant_puis_result() {
    let (dir, bac) = bac_jetable("usage");
    let (premiere, seconde, ouverture) = (assistant(1), assistant(2), ouverture_abonnement());
    let resultat = RESULTAT.replace(
        r#""num_turns":1,"#,
        concat!(
            r#""num_turns":2,"duration_ms":900,"total_cost_usd":0.0025,"usage":{"input_tokens":10,"#,
            r#""output_tokens":20,"cache_creation_input_tokens":30,"cache_read_input_tokens":40,"#,
            r#""output_tokens_details":{"thinking_tokens":50}},"modelUsage":{"claude-haiku-5-5":{"#,
            r#""inputTokens":10,"outputTokens":20,"cacheCreationInputTokens":30,"cacheReadInputTokens":40,"#,
            r#""thinkingTokens":50,"costUSD":0.0025,"costBasis":"list"}},"#,
        ),
    );
    let mut gestes = vec![Geste::Lire];
    gestes.extend(JUSTES.map(Geste::Dire));
    gestes.extend([Geste::Recopier, Geste::Dire(&ouverture), Geste::Dire(&premiere), Geste::Dire(&seconde)]);
    gestes.push(Geste::Dire(&resultat));
    let (constat, sortie) =
        jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()), "l'usage ne change pas la sortie");
    let Some(constat) = constat else { panic!("un constat") };
    assert_eq!(constat.source_cle.as_deref(), Some("none"));
    let jetons = Jetons { entree: 10, sortie: 20, cache_ecrit: 30, cache_lu: 40, reflexion: 50 };
    let haiku =
        UsageModele { modele: "claude-haiku-5-5".into(), jetons, micro_dollars: 2500, base: Some("list".into()) };
    let attendu = Usage { jetons, micro_dollars: 2500, duree_ms: 900, tours: 2, modeles: vec![haiku] };
    assert_eq!(constat.usage, attendu, "le résultat remplace le cumul");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_result_sans_usage_garde_le_cumul_des_lignes_assistant() {
    let (dir, bac) = bac_jetable("usage-vide");
    let (premiere, seconde, ouverture) = (assistant(1), assistant(2), ouverture_abonnement());
    let mut gestes = vec![Geste::Lire];
    gestes.extend(JUSTES.map(Geste::Dire));
    gestes.extend([Geste::Recopier, Geste::Dire(&ouverture), Geste::Dire(&premiere), Geste::Dire(&seconde)]);
    let resultat = RESULTAT.replace(r#""num_turns":1,"#, r#""num_turns":3,"duration_ms":700,"#);
    gestes.push(Geste::Dire(&resultat));
    let (constat, sortie) =
        jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()));
    let Some(constat) = constat else { panic!("un constat") };
    let attendu = Usage { duree_ms: 700, tours: 3, ..cumul_de_deux() };
    assert_eq!(constat.usage, attendu, "un result sans usage garde le cumul, et prend sa durée et ses tours");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_main_claude_tuee_avant_son_result_garde_le_cumul_de_ses_lignes_assistant() {
    let (dir, bac) = bac_jetable("usage-partiel");
    let (premiere, seconde, ouverture) = (assistant(1), assistant(2), ouverture_abonnement());
    let mut gestes = vec![Geste::Lire];
    gestes.extend(JUSTES.map(Geste::Dire));
    gestes.extend([Geste::Recopier, Geste::Dire(&ouverture), Geste::Dire(&premiere), Geste::Dire(&seconde)]);
    gestes.push(Geste::Sortir(1));
    let (constat, sortie) =
        jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    assert!(sortie.is_err(), "pas de result : {sortie:?}");
    let Some(constat) = constat else { panic!("un constat") };
    assert_eq!(constat.usage, cumul_de_deux());
    assert_eq!(constat.source_cle.as_deref(), Some("none"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_main_claude_dont_les_reglages_n_ont_pas_pris_est_tuee_avant_la_consigne() {
    let (dir, bac) = bac_jetable("non-pris");
    let crochets = GET_SETTINGS.replace(r#""disableAllHooks":true"#, r#""disableAllHooks":false"#);
    let mut gestes = vec![Geste::Lire, Geste::Dire(&crochets)];
    gestes.extend(JUSTES[1..].iter().map(|juste| Geste::Dire(juste)));
    gestes.extend([Geste::Recopier, Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT)]);
    let (constat, sortie) =
        jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    let erreur = sortie.unwrap_err().texte();
    let ecart = "main : réglages non pris : get_settings : effective.disableAllHooks false, déclaré true";
    assert!(erreur.starts_with(ecart), "{erreur}");
    assert_eq!(
        constat,
        Some(Constat { version: None, mur_actif: Some(false), ..Constat::default() }),
        "le mur se constate quand même"
    );
    let lu = entree(&bac);
    assert!(lu.starts_with(&premiere_demande()), "la main a lu ses demandes : {lu}");
    assert!(!lu.contains(r#""type":"user""#), "et pas sa consigne : {lu}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_main_claude_a_l_ouverture_fautive_est_tuee() {
    let (dir, bac) = bac_jetable("ouverture");
    let defaut = OUVERTURE.replace(r#""permissionMode":"acceptEdits""#, r#""permissionMode":"default""#);
    let mut gestes = vec![Geste::Lire];
    gestes.extend(JUSTES.map(Geste::Dire));
    gestes.extend([Geste::Recopier, Geste::Dire(&defaut), Geste::Dormir, Geste::Dire(RESULTAT)]);
    let debut = Instant::now();
    let (constat, sortie) =
        jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    let ecart = r#"main : réglages non pris : à l'ouverture, permissionMode "default", déclaré "acceptEdits""#;
    // Sous Windows, `taskkill /T` tue d'abord les enfants (ici `ping`) : la racine peut sortir d'elle-même avant son
    // tour, et le code 255 de taskkill ajoute alors « ; kill: … » à l'erreur, alors que l'arbre est mort.
    let erreur = sortie.unwrap_err().texte();
    assert!(erreur == ecart || erreur.starts_with(&format!("{ecart}; kill: ")), "{erreur}");
    assert_eq!(
        constat,
        Some(Constat { version: Some("2.1.312".into()), mur_actif: Some(false), ..Constat::default() })
    );
    assert!(debut.elapsed() < Duration::from_secs(20), "tuée à son ouverture, pas au bout de son sommeil");
    effacer(&dir);
}

#[test]
fn une_main_claude_qui_sort_avant_ses_reponses_le_dit() {
    // La forme du mur exigé sans mur, le 1/11 : une ligne `result` en erreur, une plainte sur stderr, le code 1.
    let (dir, bac) = bac_jetable("sans-mur");
    let plainte = "sandbox required but unavailable, refusing to start";
    let gestes = [Geste::Lire, Geste::Dire(SANS_MUR), Geste::Plaindre(plainte), Geste::Sortir(1)];
    let (constat, sortie) =
        jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    let statut = if cfg!(windows) { "exit code: 1" } else { "exit status: 1" };
    let dit = format!("hand: exit before its control responses ({statut}): {plainte}");
    assert_eq!(sortie, Err(ErreurMain::Autre(dit)));
    assert_eq!(constat, Some(Constat::default()), "rien ne s'est lu");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_main_claude_muette_est_tuee_a_la_borne() {
    // Sous un délai d'une seconde, le délai tombe avant la borne des réponses : c'est son erreur.
    let (dir, bac) = bac_jetable("muette");
    let main = fausse_claude(&dir, &[Geste::Lire, Geste::Dormir]);
    let debut = Instant::now();
    let (constat, sortie) = jouer(&main, &bac, "consigne", Duration::from_secs(1), Some(&attendu()), false, None);
    let erreur = sortie.unwrap_err();
    assert!(matches!(erreur, ErreurMain::Tuee(_)), "tuée au délai, par son type : {erreur:?}");
    let erreur = erreur.texte();
    assert!(erreur.starts_with("hand: timed out (1 s)"), "{erreur}");
    assert_eq!(constat, Some(Constat::default()));
    assert!(debut.elapsed() < Duration::from_secs(20), "tuée au délai, pas au bout de son sommeil");
    effacer(&dir);
}

#[test]
fn les_reglages_d_un_vrai_profil_se_jugent_pris_par_une_main_fidele() {
    // Ce que `Profil::demande` écrit et ce que protocole.rs juge se rencontrent ici : une main qui répond juste aux
    // réglages réels d'un profil, mur exigé et vérification prêtée compris, est prise telle quelle.
    let (dir, bac) = bac_jetable("fidele");
    let profil = profil_borne("sorte = claude\nmur = exige\nreseau = exemple.test\noutils = SendMessage\n", &dir);
    let verifier = Some("cargo test && git status");
    let declaration = profil.demande(&bac, verifier, false).declaration.expect("une main Claude bornée");
    let permis = concat!(
        r#""allow":["Skill","Workflow","ExitPlanMode","WebFetch(domain:exemple.test)","SendMessage","#,
        r#""Bash(cargo test)","PowerShell(cargo test)","Bash(git status)","PowerShell(git status)"],"deny":["#,
    );
    assert!(declaration.reglages.contains(permis), "{}", declaration.reglages);
    // WebSearch fermée au bout des sortants, puis les gardes du dossier de Kraken, du fichier de connexion de chaque
    // leurre et du git commun : la main fidèle les retrouve aussi.
    let regle =
        |chemin: PathBuf| crate::coeur::profil::regle_de_chemin(&chemin.to_string_lossy(), cfg!(windows)).unwrap();
    let (kraken, config, claude) =
        (regle(dir.join("lydite")), regle(dir.join("config")), regle(dir.join("personnel").join(".claude")));
    let commun = regle(dir.join("depot").join(".git"));
    let bac_regle = regle(bac.clone());
    let fermes = format!(
        concat!(
            r#""AskUserQuestion","WebSearch","Edit({kraken}/fil.jsonl)","Edit({kraken}/profils/**)","#,
            r#""Read({kraken}/srt-*.json)","Edit({kraken}/srt-*.json)","#,
            r#""Read({config}/.credentials.json)","Edit({config}/.credentials.json)","#,
            r#""Read({claude}/.credentials.json)","Edit({claude}/.credentials.json)","#,
            r#""Edit({commun}/**)","Bash(git commit:*)","Bash(git commit *)","#,
            r#""PowerShell(git commit:*)","PowerShell(git commit *)","Edit({bac}/.agents/skills/**)"]"#,
        ),
        bac = bac_regle,
        kraken = kraken,
        config = config,
        claude = claude,
        commun = commun,
    );
    assert!(declaration.reglages.contains(&fermes), "{}", declaration.reglages);
    let attendu = Attendu {
        reglages: &declaration.reglages,
        modele: &declaration.modele,
        effort: declaration.effort.as_deref(),
        mcp: declaration.mcp.as_deref(),
    };
    // Une main fidèle rend le `filesystem` déclaré, tel quel, à l'effectif.
    let systeme = |texte: &str| {
        let debut = texte.find(r#""filesystem":"#).unwrap();
        (debut, debut + texte[debut..].find(r#","network""#).unwrap())
    };
    let (debut, fin) = systeme(&declaration.reglages);
    let (de, a) = systeme(GET_SETTINGS);
    let mut settings = GET_SETTINGS.to_string();
    settings.replace_range(de..a, &declaration.reglages[debut..fin]);
    let settings = settings.replace(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#);
    let mur = GET_SANDBOX_DIALOG.replace(r#""enabled":false"#, r#""enabled":true"#);
    let regles = regles_fideles(&declaration.reglages);
    let mut gestes = vec![Geste::Lire];
    gestes.extend([settings.as_str(), regles.as_str(), GET_HOOKS_LISTING, mur.as_str()].map(Geste::Dire));
    gestes.extend([Geste::Recopier, Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT)]);
    let (constat, sortie) = jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu), false, None);
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()));
    assert_eq!(
        constat,
        Some(Constat {
            version: Some("2.1.312".into()),
            mur_actif: Some(true),
            usage: Usage { tours: 1, ..Usage::default() },
            ..Constat::default()
        })
    );
    assert!(entree(&bac).ends_with(&protocole::message("consigne")), "la consigne est partie");
    // Une règle déclarée que la main ne retrouve pas la fait tuer : le constat ne se contente pas du mur.
    let sans_edit: Vec<&str> = regles.split("},{").filter(|regle| !regle.contains("fil.jsonl")).collect();
    let regles = sans_edit.join("},{");
    let mut gestes = vec![Geste::Lire];
    gestes.extend([settings.as_str(), regles.as_str(), GET_HOOKS_LISTING, mur.as_str()].map(Geste::Dire));
    gestes.extend([Geste::Recopier, Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT)]);
    let (_, sortie) = jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu), false, None);
    let erreur = sortie.unwrap_err().texte();
    assert!(erreur.starts_with("main : réglages non pris : list_permission_rules : deny \"Edit("), "{erreur}");
    assert!(erreur.contains("fil.jsonl)\" déclarée, absente"), "{erreur}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_main_qui_sort_en_erreur_rend_son_statut_et_sa_sortie() {
    // Une main qui écrit sur ses deux sorties puis sort en erreur 3 : l'erreur dit le statut, stderr, puis stdout.
    let main: Vec<String> = if cfg!(windows) {
        vec!["cmd".into(), "/C".into(), "echo sortie& echo erreur>&2& exit 3".into()]
    } else {
        vec!["sh".into(), "-c".into(), "echo sortie; echo erreur >&2; exit 3".into()]
    };
    let (dir, bac) = bac_jetable("erreur");
    let erreur = jouer(&main, &bac, "consigne", DELAI, None, false, None).1.unwrap_err().texte();
    let statut = if cfg!(windows) { "exit code: 3" } else { "exit status: 3" };
    assert_eq!(erreur.replace("\r\n", "\n"), format!("{statut}; erreur\nsortie\n"));
    let _ = fs::remove_dir_all(&dir);
}

/// Une main brute qui finit tout de suite, en écrivant « ok ».
fn rapide() -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd".into(), "/C".into(), "echo ok".into()]
    } else {
        vec!["sh".into(), "-c".into(), "echo ok".into()]
    }
}

#[test]
fn un_bac_sans_nom_n_a_pas_de_dossier_temporaire() {
    // La racine du système est un dossier qui existe, mais elle n'a pas de nom : son dossier temporaire ne se forme
    // pas, et la main ne part pas, brute ou déclarée.
    let racine = env::current_dir().unwrap().ancestors().last().unwrap().to_path_buf();
    let erreur = temporaire(&racine, false).unwrap_err();
    assert!(erreur.contains("has no name") && erreur.contains(&racine.display().to_string()), "{erreur}");
    let demande = attendu();
    for attendu in [None, Some(&demande)] {
        let (constat, sortie) = jouer(&rapide(), &racine, "consigne", DELAI, attendu, false, None);
        assert_eq!(sortie.unwrap_err().texte(), erreur);
        assert_eq!(constat, attendu.map(|_| Constat::default()), "rien ne s'est lu");
    }
}

#[test]
fn l_erreur_du_delai_garde_son_debut_malgre_une_longue_erreur() {
    // La main écrit 12 000 caractères sur ses erreurs, puis dort. `queue` coupe la fin de ses erreurs à 4000
    // caractères, jamais le début de l'erreur : le test y lit « hand: timed out », brute ou déclarée. Le délai est
    // celui de l'arbre : sous charge, la coquille doit avoir tout écrit avant qu'il tombe, et un délai plus court
    // est réservé à la main muette.
    let (dir, bac) = bac_jetable("delai-long");
    let ligne = "x".repeat(200);
    let mut gestes: Vec<Geste> = (0..60).map(|_| Geste::Plaindre(&ligne)).collect();
    gestes.push(Geste::Dormir);
    let main = fausse_claude(&dir, &gestes);
    let demande = attendu();
    for attendu in [None, Some(&demande)] {
        let (_, sortie) = jouer(&main, &bac, "consigne", ARBRE, attendu, false, None);
        let erreur = sortie.unwrap_err().texte();
        assert!(erreur.starts_with("hand: timed out (10 s)"), "{}", &erreur[..erreur.len().min(80)]);
        let longueur = erreur.chars().count();
        assert!((4000..=4040).contains(&longueur), "la fin de ses erreurs, coupée à 4000 : {longueur} caractères");
        assert!(erreur.contains(&"x".repeat(200)), "la fin de ses erreurs reste");
    }
    effacer(&dir);
}

#[test]
fn un_delai_demesure_ne_fait_pas_paniquer() {
    // Un profil accepte `delai_main = 18446744073709551615` : l'échéance se borne à cent ans au lieu de déborder.
    let profil = crate::coeur::profil::lire("essai", "sorte = claude\ndelai_main = 18446744073709551615\n", None, None);
    assert_eq!(profil.unwrap().delai_main(), Duration::from_secs(u64::MAX));
    let _ = echeance(Duration::MAX);
    let (dir, bac) = bac_jetable("demesure");
    let delai = Duration::from_secs(u64::MAX);
    let (constat, sortie) = jouer(&rapide(), &bac, "consigne", delai, None, false, None);
    assert_eq!((constat, sortie), (None, Ok("ok".to_string() + if cfg!(windows) { "\r\n" } else { "\n" })));
    let main = fidele(&dir, [Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT)]);
    let (_, sortie) = jouer(&main, &bac, "consigne", delai, Some(&attendu()), false, None);
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()));
    let _ = fs::remove_dir_all(&dir);
}

/// Une fausse main Claude fidèle aux réponses justes, puis ces gestes de plus : elle lit sa première demande, répond,
/// recopie le reste de son entrée, et fait `apres`.
fn fidele<'a>(dir: &Path, apres: impl IntoIterator<Item = Geste<'a>>) -> Vec<String> {
    let mut gestes = vec![Geste::Lire];
    gestes.extend(JUSTES.map(Geste::Dire));
    gestes.push(Geste::Recopier);
    gestes.extend(apres);
    fausse_claude(dir, &gestes)
}

#[test]
fn une_main_qui_sort_en_erreur_apres_son_resultat_est_rouge() {
    let (dir, bac) = bac_jetable("sortie-en-erreur");
    let statut = if cfg!(windows) { "exit code: 3" } else { "exit status: 3" };
    // Un résultat juste, puis le code 3 et une plainte : la main est rouge, et dit son statut et sa plainte.
    let gestes = [Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT), Geste::Plaindre("boum"), Geste::Sortir(3)];
    let (constat, sortie) = jouer(&fidele(&dir, gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    assert_eq!(sortie, Err(ErreurMain::Autre(format!("hand: exited with an error ({statut}): boum"))));
    assert_eq!(
        constat,
        Some(Constat {
            version: Some("2.1.312".into()),
            mur_actif: Some(false),
            usage: Usage { tours: 1, ..Usage::default() },
            ..Constat::default()
        })
    );
    // Un résultat en erreur l'emporte, qu'elle sorte en 0 ou en 3 : l'erreur du résultat, pas celle de sa sortie.
    let en_erreur = RESULTAT.replace(r#""is_error":false"#, r#""is_error":true"#);
    for code in [0, 3] {
        let gestes = [Geste::Dire(OUVERTURE), Geste::Dire(&en_erreur), Geste::Sortir(code)];
        let (_, sortie) = jouer(&fidele(&dir, gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
        let dit = ErreurMain::Autre("main : résultat en erreur (success) : fait\nPREUVE: vert".into());
        assert_eq!(sortie, Err(dit), "code {code}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_resultat_en_erreur_de_l_api_est_une_coupure_dans_le_constat() {
    let (dir, bac) = bac_jetable("coupure");
    let en_erreur = |statut: &str| {
        RESULTAT
            .replace(r#""is_error":false"#, &format!(r#""is_error":true,"api_error_status":{statut}"#))
            .replace("fait\\nPREUVE: vert", "API Error")
    };
    // 529 (le fournisseur surchargé) est une coupure, quel que soit le code de sortie de la main ; 401 reste une
    // erreur.
    for (statut, code, coupure) in [("529", 0, Some(529)), ("429", 3, Some(429)), ("401", 0, None), ("null", 0, None)] {
        let gestes = [Geste::Dire(OUVERTURE), Geste::Dire(&en_erreur(statut)), Geste::Sortir(code)];
        let (constat, sortie) = jouer(&fidele(&dir, gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
        let dit = ErreurMain::Autre("main : résultat en erreur (success) : API Error".into());
        assert_eq!(sortie, Err(dit), "{statut} code {code}");
        assert_eq!(constat.unwrap().coupure, coupure, "{statut} code {code}");
    }
    // Une main qui réussit n'a pas de coupure.
    let gestes = [Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT)];
    let (constat, sortie) = jouer(&fidele(&dir, gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()));
    assert_eq!(constat.unwrap().coupure, None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_resultat_au_plafond_de_depense_est_lu_au_constat() {
    let (dir, bac) = bac_jetable("plafond");
    let depense = RESULTAT.replace(r#""subtype":"success""#, r#""subtype":"error_max_budget_usd""#);
    let depense = depense.replace(r#""is_error":false"#, r#""is_error":true"#);
    for code in [0, 1] {
        let gestes = [Geste::Dire(OUVERTURE), Geste::Dire(&depense), Geste::Sortir(code)];
        let (constat, sortie) = jouer(&fidele(&dir, gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
        assert!(sortie.is_err(), "code {code} : {sortie:?}");
        let constat = constat.unwrap();
        assert!(constat.plafond, "code {code}");
        assert_eq!(constat.coupure, None, "code {code}");
    }
    // Une main qui réussit n'a pas atteint son plafond.
    let gestes = [Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT)];
    let (constat, _) = jouer(&fidele(&dir, gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    assert!(!constat.unwrap().plafond);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_main_qui_ne_sort_pas_apres_son_resultat_est_tuee_et_son_resultat_compte() {
    // Après `result`, la main a 10 s pour sortir ; sinon son arbre meurt, et le résultat compte.
    let (dir, bac) = bac_jetable("ne-sort-pas");
    let gestes = [Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT), Geste::Dormir];
    let debut = Instant::now();
    let (constat, sortie) = jouer(&fidele(&dir, gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    let duree = debut.elapsed();
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()));
    assert_eq!(
        constat,
        Some(Constat {
            version: Some("2.1.312".into()),
            mur_actif: Some(false),
            usage: Usage { tours: 1, ..Usage::default() },
            ..Constat::default()
        })
    );
    assert!(duree >= Duration::from_secs(10), "elle a 10 s pour sortir : tuée après {duree:?}");
    assert!(duree < Duration::from_secs(28), "tuée à la fin de ses 10 s, pas au bout de son sommeil : {duree:?}");
    effacer(&dir);
}

#[test]
fn avant_son_ouverture_une_ligne_illisible_tue_la_main_et_ensuite_elle_s_ignore() {
    let (dir, bac) = bac_jetable("illisible");
    // Avant l'ouverture, une ligne qui n'est pas du JSON est un écart : la main meurt avant sa consigne.
    let gestes = [Geste::Lire, Geste::Dire("pas du json"), Geste::Dormir];
    let debut = Instant::now();
    let (constat, sortie) =
        jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    let erreur = sortie.unwrap_err().texte();
    assert!(erreur.starts_with("main : réglages non pris : ligne illisible : "), "{erreur}");
    assert_eq!(constat, Some(Constat::default()));
    assert!(debut.elapsed() < Duration::from_secs(20), "tuée à sa ligne, pas au bout de son sommeil");
    // Après l'ouverture, seule compte la ligne du résultat : la même ligne s'ignore.
    let gestes = [Geste::Dire(OUVERTURE), Geste::Dire("pas du json"), Geste::Dire(RESULTAT)];
    let (_, sortie) = jouer(&fidele(&dir, gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()));
    effacer(&dir);
}

#[test]
fn les_reponses_s_apparient_par_leur_id_dans_n_importe_quel_ordre() {
    // Les quatre réponses à l'envers, après une ligne lisible qui n'est ni une réponse ni l'ouverture : elle s'ignore.
    let (dir, bac) = bac_jetable("desordre");
    let mut gestes = vec![Geste::Lire, Geste::Dire(r#"{"type":"system","subtype":"status"}"#)];
    gestes.extend(JUSTES.iter().rev().map(|juste| Geste::Dire(juste)));
    gestes.extend([Geste::Recopier, Geste::Dire(OUVERTURE), Geste::Dire(RESULTAT)]);
    let (constat, sortie) =
        jouer(&fausse_claude(&dir, &gestes), &bac, "consigne", DELAI, Some(&attendu()), false, None);
    assert_eq!(sortie, Ok("fait\nPREUVE: vert".into()));
    assert_eq!(
        constat,
        Some(Constat {
            version: Some("2.1.312".into()),
            mur_actif: Some(false),
            usage: Usage { tours: 1, ..Usage::default() },
            ..Constat::default()
        })
    );
    assert!(entree(&bac).ends_with(&protocole::message("consigne")), "la consigne est partie");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_main_ne_recoit_pas_l_epuration_de_l_environnement_des_commandes_avec_ou_sans_fournisseur() {
    let (dir, bac) = bac_jetable("epuration");
    let liste: Vec<String> =
        if cfg!(windows) { vec!["cmd".into(), "/C".into(), "set".into()] } else { vec!["env".into()] };
    let api = "https://api.exemple.test/anthropic";
    let fourni = Fournisseur { api, cle: "leurre-de-la-cle", variable: "CLE_ESSAI" };
    for fournisseur in [None, Some(fourni)] {
        let sortie = jouer(&liste, &bac, "", DELAI, None, false, fournisseur).1.unwrap();
        let vues: Vec<_> = sortie
            .lines()
            .filter_map(|ligne| ligne.split_once('='))
            .filter(|(nom, _)| nom.eq_ignore_ascii_case("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"))
            .collect();
        assert!(vues.is_empty(), "{fournisseur:?} : {sortie}");
    }
    let _ = fs::remove_dir_all(&dir);
}
