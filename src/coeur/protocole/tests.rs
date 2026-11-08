use super::*;

// Les lignes de Claude Code relevées le 1/11, réduites aux champs que Kraken lit.
const GET_SETTINGS: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id"#,
    r#"":"get_settings","response":{"applied":{"model":"claude-sonnet-5-5"},""#,
    r#"effective":{"disableAllHooks":true,"autoMemoryEnabled":false,"sandbox""#,
    r#":{"enabled":true,"failIfUnavailable":false,"allowUnsandboxedCommands":"#,
    r#"false,"network":{"allowedDomains":[],"strictAllowlist":true}}},"source"#,
    r#"s":[{"source":"userSettings","settings":{}},{"source":"flagSettings",""#,
    r#"settings":{}}]}}}"#,
);
const LIST_PERMISSION_RULES: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id"#,
    r#"":"list_permission_rules","response":{"state":{"rules":[{"behavior":"a"#,
    r#"llow","source":"flagSettings","rule":"WebSearch"},{"behavior":"deny",""#,
    r#"source":"flagSettings","rule":"CronCreate"},{"behavior":"deny","source"#,
    r#"":"userSettings","rule":"Read(~/.ssh/**)"}]}}}}"#,
);
const GET_HOOKS_LISTING: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id"#,
    r#"":"get_hooks_listing","response":{"policy":{"disabledByPolicy":false,""#,
    r#"managedOnly":false,"pluginOnly":false,"allDisabled":true,"policyHookCo"#,
    r#"unt":0},"hooks":[],"events":[],"eventCatalog":[]}}}"#,
);
const GET_SANDBOX_DIALOG: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id"#,
    r#"":"get_sandbox_dialog","response":{"supported":false,"unsupported_reas"#,
    r#"on":"platform","enabled":false,"enabled_in_settings":true,"unavailable"#,
    r#"_reason":"sandbox is enabled but the Windows sandbox is not active on "#,
    r#"this session (feature gate off)","mode":"regular"}}}"#,
);
const OUVERTURE: &str = concat!(
    r#"{"type":"system","subtype":"init","cwd":"/srv/bac","tools":["Task","Ba"#,
    r#"sh","Edit","Read","Skill","WebFetch","WebSearch","Workflow","Write"],""#,
    r#"mcp_servers":[],"model":"claude-sonnet-5-5","permissionMode":"acceptEd"#,
    r#"its","claude_code_version":"2.1.312"}"#,
);
const RESULTAT: &str =
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"fait\nPREUVE: vert"}"#;
/// La seule ligne d'une main au mur exigé, sans mur : elle sort en code 1, avant ses réponses.
const SANS_MUR: &str = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":0}"#;
/// Les quatre réponses justes, dans l'ordre des demandes.
const JUSTES: [&str; 4] = [GET_SETTINGS, LIST_PERMISSION_RULES, GET_HOOKS_LISTING, GET_SANDBOX_DIALOG];

/// Un `--settings` de la forme que Kraken passe, au mur demandé, réduit aux deux règles des réponses relevées.
const REGLAGES: &str = concat!(
    r#"{"sandbox":{"enabled":true,"failIfUnavailable":false,"allowUnsandboxed"#,
    r#"Commands":false,"filesystem":{"denyRead":["~/","/srv/lydite"],"allowRe"#,
    r#"ad":["/srv/lydite/bacs/main-1","/srv/depot/.git"]},"network":{"allowed"#,
    r#"Domains":[],"strictAllowlist":true}},"permissions":{"allow":["WebSearc"#,
    r#"h"],"deny":["CronCreate"]},"disableAllHooks":true,"autoMemoryEnabled":"#,
    r#"false}"#,
);

/// La réponse en erreur à la demande `id` : la seule forme qu'aucune main n'a rendue le 1/11.
fn en_erreur(id: &str) -> String {
    format!(r#"{{"type":"control_response","response":{{"subtype":"error","request_id":"{id}","error":"…"}}}}"#)
}

/// Ce que Kraken passe à une main sur ces réglages : `--model sonnet`, sans `mcp`.
fn attendu(reglages: &str) -> Attendu<'_> {
    Attendu { reglages, modele: "sonnet", mcp: None }
}

/// Les réponses rangées par id, comme `jouer` les range : chaque ligne passée par `lire_ligne`.
fn rangees(lignes: &[&str]) -> BTreeMap<String, Json> {
    let reponse = |ligne: &&str| match lire_ligne(ligne.as_bytes()) {
        Ok(Ligne::Reponse(id, reponse)) => (id, reponse),
        autre => panic!("une réponse : {ligne} donne {autre:?}"),
    };
    lignes.iter().map(reponse).collect()
}

/// Les réponses justes, celle de même id que `changee` remplacée par elle.
fn avec(changee: &str) -> BTreeMap<String, Json> {
    let mut reponses = rangees(&JUSTES);
    reponses.extend(rangees(&[changee]));
    reponses
}

/// L'ouverture de cette ligne, rangée par `lire_ligne`.
fn ouverture(ligne: &str) -> Json {
    match lire_ligne(ligne.as_bytes()) {
        Ok(Ligne::Ouverture(ouverture)) => ouverture,
        autre => panic!("une ouverture : {ligne} donne {autre:?}"),
    }
}

#[test]
fn les_demandes_et_le_message_s_ecrivent_a_l_octet() {
    assert_eq!(
        demandes(),
        concat!(
            r#"{"type":"control_request","request_id":"get_settings","request":{"subtype":"get_settings"}}"#,
            "\n",
            concat!(
                r#"{"type":"control_request","request_id":"list_permission_rules","reques"#,
                r#"t":{"subtype":"list_permission_rules"}}"#,
            ),
            "\n",
            r#"{"type":"control_request","request_id":"get_hooks_listing","request":{"subtype":"get_hooks_listing"}}"#,
            "\n",
            concat!(
                r#"{"type":"control_request","request_id":"get_sandbox_dialog","request":"#,
                r#"{"subtype":"get_sandbox_dialog"}}"#,
            ),
            "\n",
        )
    );
    // Un guillemet, une barre inverse, un retour à la ligne et un accent : la consigne passe par `json::chaine`.
    let consigne = "Écris \"bonjour\" dans C:\\bac\nPuis prouve-le.";
    assert_eq!(
        message(consigne),
        concat!(
            concat!(
                r#"{"type":"user","message":{"role":"user","content":"Écris \"bonjour\" d"#,
                r#"ans C:\\bac\nPuis prouve-le."},"parent_tool_use_id":null,"session_id":"#,
                r#"""}"#,
            ),
            "\n",
        )
    );
    // Chaque ligne se relit en document, et la consigne revient telle quelle.
    for ligne in demandes().lines() {
        assert!(json::document(ligne).is_ok(), "{ligne}");
    }
    let relu = json::document(message(consigne).trim_end()).unwrap();
    assert_eq!(texte_de(&relu, "message.content"), Some(consigne));
}

#[test]
fn des_reglages_pris_rendent_le_mur() {
    let demande = attendu(REGLAGES);
    // Sous Windows le 1/11, le mur n'est pas actif, et Claude Code le dit.
    assert_eq!(juger_reponses(&demande, &rangees(&JUSTES)), (Some(false), Ok(())));
    let actif = GET_SANDBOX_DIALOG.replace(r#""enabled":false"#, r#""enabled":true"#);
    assert_eq!(juger_reponses(&demande, &avec(&actif)), (Some(true), Ok(())));
    // `enabled` absent ou d'un autre type : rien n'est constaté, et ce n'est pas un écart.
    let muets = [
        GET_SANDBOX_DIALOG.replace(r#""enabled":false,"#, ""),
        GET_SANDBOX_DIALOG.replace(r#""enabled":false"#, r#""enabled":"false""#),
    ];
    for muet in &muets {
        assert_eq!(juger_reponses(&demande, &avec(muet)), (None, Ok(())), "{muet}");
    }
    // Le modèle se déclare en alias ou en nom complet.
    let complet = Attendu { modele: "claude-sonnet-5-5", ..demande };
    assert_eq!(juger_reponses(&complet, &rangees(&JUSTES)), (Some(false), Ok(())));
    // `get_sandbox_dialog` en erreur ne constate rien, et ce n'est pas un écart, sous le mur demandé comme sous le
    // mur exigé, dont `get_settings` rend `failIfUnavailable` vrai.
    let erreur = en_erreur("get_sandbox_dialog");
    assert_eq!(juger_reponses(&demande, &avec(&erreur)), (None, Ok(())));
    let exige = REGLAGES.replace(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#);
    let mut reponses = avec(&GET_SETTINGS.replace(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#));
    reponses.extend(rangees(&[&erreur]));
    assert_eq!(juger_reponses(&attendu(&exige), &reponses), (None, Ok(())));
    // Exigé et dit actif : pris.
    reponses.extend(rangees(&[&actif]));
    assert_eq!(juger_reponses(&attendu(&exige), &reponses), (Some(true), Ok(())));
}

#[test]
fn chaque_ecart_des_reponses_est_dit() {
    let settings = |de: &str, en: &str| GET_SETTINGS.replace(de, en);
    let erreurs = concat!(
        r#""errors":[{"file":"claude-settings.json","path":"disableAllHooks","mes"#,
        r#"sage":"Expected boolean, but received string"}],"sources""#,
    );
    let flag = r#",{"source":"flagSettings","settings":{}}"#;
    let deny = |plus: &str| REGLAGES.replace(r#""deny":["CronCreate"]"#, &format!(r#""deny":["CronCreate","{plus}"]"#));
    // Chaque cas : les réglages déclarés, la réponse changée qui remplace la juste de même id, puis l'écart entier,
    // qui dit le déclaré contre le constaté.
    let cas = [
        (
            REGLAGES.to_string(),
            settings(r#""sources""#, erreurs),
            concat!(
                r#"get_settings : erreur "claude-settings.json" à "disableAllHooks" : "Ex"#,
                r#"pected boolean, but received string", déclaré sans erreur"#,
            ),
        ),
        (
            REGLAGES.to_string(),
            settings(flag, ""),
            r#"get_settings : sources ["userSettings"], déclaré "flagSettings" parmi elles"#,
        ),
        (
            REGLAGES.to_string(),
            settings(r#""disableAllHooks":true"#, r#""disableAllHooks":false"#),
            "get_settings : effective.disableAllHooks false, déclaré true",
        ),
        (
            REGLAGES.to_string(),
            settings(r#""autoMemoryEnabled":false,"#, ""),
            "get_settings : effective.autoMemoryEnabled absent, déclaré false",
        ),
        (
            REGLAGES.to_string(),
            settings(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#),
            "get_settings : effective.sandbox.failIfUnavailable true, déclaré false",
        ),
        (
            REGLAGES.to_string(),
            settings("claude-sonnet-5-5", "claude-opus-5-5"),
            r#"get_settings : applied.model "claude-opus-5-5", déclaré "sonnet""#,
        ),
        (
            deny("Edit(//c/lydite/fil.jsonl)"),
            LIST_PERMISSION_RULES.to_string(),
            r#"list_permission_rules : deny "Edit(//c/lydite/fil.jsonl)" déclarée, absente"#,
        ),
        (
            deny("Read(~/.ssh/**)"),
            LIST_PERMISSION_RULES.to_string(),
            r#"list_permission_rules : deny "Read(~/.ssh/**)" déclarée, vue en "deny" à "userSettings""#,
        ),
        (
            REGLAGES.to_string(),
            LIST_PERMISSION_RULES.replace(r#""behavior":"allow""#, r#""behavior":"deny""#),
            r#"list_permission_rules : allow "WebSearch" déclarée, vue en "deny" à "flagSettings""#,
        ),
        (
            REGLAGES.to_string(),
            GET_HOOKS_LISTING.replace(r#""allDisabled":true"#, r#""allDisabled":false"#),
            "get_hooks_listing : policy.allDisabled false, déclaré true",
        ),
        (REGLAGES.to_string(), en_erreur("get_settings"), r#"get_settings : demandée, rendue en "error" : "…""#),
        (
            REGLAGES.to_string(),
            en_erreur("list_permission_rules"),
            r#"list_permission_rules : demandée, rendue en "error" : "…""#,
        ),
        (
            REGLAGES.to_string(),
            en_erreur("get_hooks_listing"),
            r#"get_hooks_listing : demandée, rendue en "error" : "…""#,
        ),
    ];
    for (reglages, changee, dit) in &cas {
        let (mur_actif, jugement) = juger_reponses(&attendu(reglages), &avec(changee));
        assert_eq!(jugement, Err(format!("main : réglages non pris : {dit}")), "{changee}");
        assert_eq!(mur_actif, Some(false), "le mur se constate même sur un écart : {changee}");
    }
    // Un mur exigé, que Claude Code dit inactif.
    let exige = REGLAGES.replace(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#);
    let reponses = avec(&settings(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#));
    let inactif =
        "main : réglages non pris : get_sandbox_dialog : enabled false, déclaré exigé (failIfUnavailable true)";
    assert_eq!(juger_reponses(&attendu(&exige), &reponses), (Some(false), Err(inactif.into())));
    // Une réponse qui manque.
    let mut reponses = rangees(&JUSTES);
    reponses.remove("get_hooks_listing");
    let manque = "main : réglages non pris : get_hooks_listing : demandée, sans réponse";
    assert_eq!(juger_reponses(&attendu(REGLAGES), &reponses), (Some(false), Err(manque.into())));
    // Le `--settings` tombé du 1/11, sans `flagSettings` et avec son erreur, puis une autre demande en écart :
    // chaque écart à la suite du nom de sa demande, tous joints par « ; ».
    let mut reponses = avec(&settings(flag, "").replace(r#""sources""#, erreurs));
    reponses.extend(rangees(&[&GET_HOOKS_LISTING.replace(r#""allDisabled":true"#, r#""allDisabled":false"#)]));
    let tombe = concat!(
        r#"main : réglages non pris : get_settings : sources ["userSettings"], déclaré "flagSettings" parmi elles"#,
        r#" ; erreur "claude-settings.json" à "disableAllHooks" : "Expected boolean, but received string", déclaré"#,
        r#" sans erreur ; get_hooks_listing : policy.allDisabled false, déclaré true"#,
    );
    assert_eq!(juger_reponses(&attendu(REGLAGES), &reponses).1, Err(tombe.into()));
    // Un `--settings` que Kraken aurait mal écrit ne se juge pas : la main ne part pas.
    let (mur_actif, jugement) = juger_reponses(&attendu("{"), &rangees(&JUSTES));
    assert_eq!(mur_actif, Some(false));
    assert!(jugement.unwrap_err().starts_with("main : réglages non pris : --settings déclaré illisible : "));
}

#[test]
fn les_commandes_exclues_du_bac_a_sable_se_jugent_telles_que_declarees() {
    // `get_settings` dans la forme relevée le 1/11 au soir sous Windows : après `network`, son `sandbox` effectif
    // porte `filesystem`, le bac en `/srv/lydite/bacs/main-1`, puis la preuve prêtée en `excludedCommands`. Le
    // `--settings` la déclare juste après `allowUnsandboxedCommands`.
    let settings = GET_SETTINGS.replace(
        r#""strictAllowlist":true}}"#,
        concat!(
            r#""strictAllowlist":true},"filesystem":{"denyRead":["~/"],"allowRead":[""#,
            r#"/srv/lydite/bacs/main-1"]},"excludedCommands":["findstr /x bonjour bon"#,
            r#"jour.txt"]}"#,
        ),
    );
    let declare = REGLAGES.replace(
        r#""allowUnsandboxedCommands":false,"#,
        r#""allowUnsandboxedCommands":false,"excludedCommands":["findstr /x bonjour bonjour.txt"],"#,
    );
    assert!(settings != GET_SETTINGS && declare != REGLAGES);
    // L'effectif égal au déclaré : pris. Absentes de part et d'autre, comme dans les réponses justes : pris aussi.
    assert_eq!(juger_reponses(&attendu(&declare), &avec(&settings)), (Some(false), Ok(())));
    assert_eq!(juger_reponses(&attendu(REGLAGES), &rangees(&JUSTES)), (Some(false), Ok(())));
    // Une de plus, qu'une autre source ajouterait ; une qui manque ; la clé tombée de l'effectif ; la clé à
    // l'effectif sans rien de déclaré : chaque fois un écart, qui dit l'effectif contre le déclaré.
    let (une, deux) = (r#""findstr /x bonjour bonjour.txt"]"#, r#""findstr /x bonjour bonjour.txt","cargo test"]"#);
    let tombee = settings.replace(r#","excludedCommands":["findstr /x bonjour bonjour.txt"]"#, "");
    let cas = [
        (declare.clone(), settings.replace(une, deux), "[2 éléments], déclaré [1 élément]"),
        (declare.replace(une, deux), settings.clone(), "[1 élément], déclaré [2 éléments]"),
        (declare.clone(), tombee, "absent, déclaré [1 élément]"),
        (REGLAGES.to_string(), settings.clone(), "[1 élément], déclaré absent"),
    ];
    for (reglages, changee, dit) in &cas {
        let (mur_actif, jugement) = juger_reponses(&attendu(reglages), &avec(changee));
        let ecart = format!("main : réglages non pris : get_settings : effective.sandbox.excludedCommands {dit}");
        assert_eq!(jugement, Err(ecart), "{reglages}\n{changee}");
        assert_eq!(mur_actif, Some(false), "le mur se constate même sur un écart : {changee}");
    }
}

#[test]
fn l_ouverture_se_juge() {
    let demande = attendu(REGLAGES);
    assert_eq!(juger_ouverture(&demande, &ouverture(OUVERTURE)), (Some("2.1.312".into()), Ok(())));
    // Chaque écart, et la version, constatée même alors.
    let mcp = OUVERTURE.replace(r#""mcp_servers":[]"#, r#""mcp_servers":[{}]"#);
    let cas = [
        (
            OUVERTURE.replace(r#""permissionMode":"acceptEdits""#, r#""permissionMode":"default""#),
            r#"permissionMode "default", déclaré "acceptEdits""#,
        ),
        (OUVERTURE.replace(r#""Write"]"#, r#""Write","CronCreate"]"#), r#"tools porte "CronCreate", déclaré en deny"#),
        (mcp, "mcp_servers [1 élément], déclaré []"),
    ];
    for (ligne, dit) in &cas {
        let attendu = Err(format!("main : réglages non pris : à l'ouverture, {dit}"));
        assert_eq!(juger_ouverture(&demande, &ouverture(ligne)), (Some("2.1.312".into()), attendu), "{ligne}");
    }
    // Sans `tools` ni version : un écart, et rien de constaté.
    let nue = ouverture(r#"{"type":"system","subtype":"init","mcp_servers":[],"permissionMode":"acceptEdits"}"#);
    let sans_outils = "main : réglages non pris : à l'ouverture, tools absent, déclaré une liste sans les noms de deny";
    assert_eq!(juger_ouverture(&demande, &nue), (None, Err(sans_outils.into())));
}

#[test]
fn les_serveurs_mcp_se_jugent_par_leur_nom() {
    // L'ouverture juste, à ces serveurs près.
    let avec =
        |serveurs: &str| ouverture(&OUVERTURE.replace(r#""mcp_servers":[]"#, &format!(r#""mcp_servers":{serveurs}"#)));
    let declares = ["docs".to_string(), "lore".to_string()];
    let mcp = Attendu { mcp: Some(&declares), ..attendu(REGLAGES) };
    let pris = (Some("2.1.312".to_string()), Ok(()));
    // Un serveur déclaré passe, quel que soit son statut, `pending` compris ; un serveur déclaré peut manquer.
    let justes = [
        r#"[{"name":"docs","status":"pending"}]"#,
        r#"[{"name":"lore","status":"failed"},{"name":"docs","status":"connected"}]"#,
        "[]",
    ];
    for serveurs in justes {
        assert_eq!(juger_ouverture(&mcp, &avec(serveurs)), pris, "{serveurs}");
    }
    // Un serveur dont le nom n'est pas une clé du `mcpServers` déclaré arrête la main, quel que soit son statut, à
    // la casse près ; un serveur sans nom aussi. Chacun est un écart, qui dit les noms déclarés.
    let cas = [
        (
            r#"[{"name":"docs","status":"pending"},{"name":"autre","status":"pending"}]"#,
            r#"mcp_servers.name "autre", déclaré parmi ["docs", "lore"]"#,
        ),
        (
            r#"[{"name":"Docs","status":"connected"},{"status":"connected"}]"#,
            concat!(
                r#"mcp_servers.name "Docs", déclaré parmi ["docs", "lore"] ; "#,
                r#"mcp_servers.name absent, déclaré parmi ["docs", "lore"]"#,
            ),
        ),
    ];
    for (serveurs, dit) in cas {
        let ecart = Err(format!("main : réglages non pris : à l'ouverture, {dit}"));
        assert_eq!(juger_ouverture(&mcp, &avec(serveurs)), (Some("2.1.312".into()), ecart), "{serveurs}");
    }
    // Une ouverture sans la liste des serveurs : un écart aussi.
    let sans_liste = ouverture(&OUVERTURE.replace(r#""mcp_servers":[],"#, ""));
    let ecart = concat!(
        r#"main : réglages non pris : à l'ouverture, mcp_servers absent, "#,
        r#"déclaré une liste de serveurs parmi ["docs", "lore"]"#,
    );
    assert_eq!(juger_ouverture(&mcp, &sans_liste), (Some("2.1.312".into()), Err(ecart.into())));
    // Sans `mcp`, un serveur arrête toujours la main, même en attente, même au nom qu'un autre profil déclare.
    let docs = avec(r#"[{"name":"docs","status":"pending"}]"#);
    let ecart = "main : réglages non pris : à l'ouverture, mcp_servers [1 élément], déclaré []";
    assert_eq!(juger_ouverture(&attendu(REGLAGES), &docs), (Some("2.1.312".into()), Err(ecart.into())));
    // Un `mcpServers` vide ne déclare aucun serveur : un serveur arrête la main.
    let aucun = Attendu { mcp: Some(&[]), ..attendu(REGLAGES) };
    let ecart = r#"main : réglages non pris : à l'ouverture, mcp_servers.name "docs", déclaré parmi []"#;
    assert_eq!(juger_ouverture(&aucun, &docs), (Some("2.1.312".into()), Err(ecart.into())));
}

#[test]
fn un_refus_d_ecriture_n_est_pas_un_outil_ferme_a_l_ouverture() {
    // Les gardes du dossier de Kraken sont des règles de `deny` comme les sortants, mais ce ne sont pas des noms
    // d'outils : l'ouverture, dont `tools` porte `Read` et `Edit`, n'est pas un écart.
    let gardes = concat!(
        r#""deny":["CronCreate","Edit(//c/lydite/fil.jsonl)","Edit(//c/lydite/profils/**)","#,
        r#""Read(//c/lydite/srt-*.json)","Edit(//c/lydite/srt-*.json)"]"#,
    );
    let reglages = REGLAGES.replace(r#""deny":["CronCreate"]"#, gardes);
    assert_ne!(reglages, REGLAGES);
    assert_eq!(juger_ouverture(&attendu(&reglages), &ouverture(OUVERTURE)), (Some("2.1.312".into()), Ok(())));
    // Un `Edit` nu en `deny`, lui, retire l'outil : si `tools` le porte encore, c'est un écart.
    let nu = REGLAGES.replace(r#""deny":["CronCreate"]"#, r#""deny":["Edit"]"#);
    let (version, ecart) = juger_ouverture(&attendu(&nu), &ouverture(OUVERTURE));
    assert_eq!(version.as_deref(), Some("2.1.312"));
    assert_eq!(ecart, Err(r#"main : réglages non pris : à l'ouverture, tools porte "Edit", déclaré en deny"#.into()));
}

#[test]
fn le_resultat_donne_la_sortie() {
    let fin = |ligne: &str| match lire_ligne(ligne.as_bytes()) {
        Ok(Ligne::Resultat(fin)) => fin,
        autre => panic!("un résultat : {ligne} donne {autre:?}"),
    };
    assert_eq!(resultat(&fin(RESULTAT)), Ok("fait\nPREUVE: vert".into()));
    let en_erreur = RESULTAT.replace(r#""is_error":false"#, r#""is_error":true"#);
    assert_eq!(resultat(&fin(&en_erreur)), Err("main : résultat en erreur (success) : fait\nPREUVE: vert".into()));
    assert_eq!(resultat(&fin(SANS_MUR)), Err("main : résultat en erreur (error_during_execution)".into()));
}

#[test]
fn chaque_ligne_se_range() {
    // Les réponses, par l'id de leur demande, avec leur objet `response` ; l'ouverture et les résultats, entiers.
    for (juste, id) in JUSTES.iter().zip(DEMANDES) {
        let Ok(Ligne::Reponse(lu, reponse)) = lire_ligne(juste.as_bytes()) else { panic!("{juste}") };
        let rangee = (lu.as_str(), texte_de(&reponse, "request_id"), texte_de(&reponse, "subtype"));
        assert_eq!(rangee, (id, Some(id), Some("success")));
    }
    let erreur = en_erreur("get_settings");
    let Ok(Ligne::Reponse(id, reponse)) = lire_ligne(erreur.as_bytes()) else { panic!("{erreur}") };
    assert_eq!((id.as_str(), texte_de(&reponse, "error")), ("get_settings", Some("…")));
    assert_eq!(lire_ligne(OUVERTURE.as_bytes()), Ok(Ligne::Ouverture(json::document(OUVERTURE).unwrap())));
    assert_eq!(lire_ligne(format!("{OUVERTURE}\r").as_bytes()), lire_ligne(OUVERTURE.as_bytes()), "en CRLF");
    for fin in [RESULTAT, SANS_MUR] {
        assert_eq!(lire_ligne(fin.as_bytes()), Ok(Ligne::Resultat(json::document(fin).unwrap())));
    }
    // Toute autre ligne lisible s'ignore : ce que Kraken écrit, une réponse sans id, un autre message `system`, un
    // document qui n'est pas un objet.
    let (consigne, sans_id, statut) = (
        message("x"),
        GET_HOOKS_LISTING.replace(r#""request_id":"get_hooks_listing","#, ""),
        OUVERTURE.replace(r#""subtype":"init""#, r#""subtype":"status""#),
    );
    let ecrites = demandes();
    for autre in ecrites.lines().chain([consigne.trim_end(), &sans_id, &statut, "[]"]) {
        assert_eq!(lire_ligne(autre.as_bytes()), Ok(Ligne::Autre), "{autre}");
    }
    // Une ligne illisible est un écart, dit en entier : un octet hors de l'UTF-8, un document coupé, une ligne vide.
    let mut octets = GET_SETTINGS.as_bytes().to_vec();
    octets.insert(10, 0xFF);
    let illisible = |raison: &str| Err(format!("main : réglages non pris : ligne illisible : {raison}"));
    assert_eq!(lire_ligne(&octets), illisible("UTF-8 invalide après l'octet 10"));
    assert_eq!(
        lire_ligne(&GET_SETTINGS.as_bytes()[..7]),
        illisible("au caractère 8 : attendu `:`, vu la fin de la ligne")
    );
    assert_eq!(lire_ligne(b""), illisible("au caractère 1 : attendu une valeur, vu la fin de la ligne"));
}

#[test]
fn le_constat_garde_ce_que_la_main_a_dit_meme_sur_un_ecart() {
    // `--model opus` déclaré pour une main sur Sonnet, puis une ouverture en mode `default` : deux écarts, et
    // pourtant la version et le mur se constatent.
    let (mur_actif, reponses) = juger_reponses(&Attendu { modele: "opus", ..attendu(REGLAGES) }, &rangees(&JUSTES));
    assert!(reponses.is_err());
    let defaut = OUVERTURE.replace(r#""permissionMode":"acceptEdits""#, r#""permissionMode":"default""#);
    let (version, ouvre) = juger_ouverture(&attendu(REGLAGES), &ouverture(&defaut));
    assert!(ouvre.is_err());
    let constat = Constat { version, mur_actif, ..Constat::default() };
    assert_eq!((constat.version.as_deref(), constat.mur_actif), (Some("2.1.312"), Some(false)));
}

// Deux lignes `result` réelles, relevées le 07/11 sur Claude Code 2.1.317.
const RESULT_ABONNEMENT: &str = include_str!("exemples/result-abonnement.json");
const RESULT_DEEPSEEK: &str = include_str!("exemples/result-deepseek.json");

fn ligne_json(ligne: &str) -> Json {
    json::document(ligne.trim_end()).unwrap()
}

#[test]
fn l_usage_se_lit_des_lignes_result_reelles() {
    let jetons = Jetons { entree: 2, sortie: 4, cache_ecrit: 11436, cache_lu: 11382, reflexion: 0 };
    let haiku =
        UsageModele { modele: "claude-haiku-5-5".into(), jetons, micro_dollars: 2403, base: Some("list".into()) };
    let attendu = Usage { jetons, micro_dollars: 2403, duree_ms: 1114, tours: 1, modeles: vec![haiku] };
    assert_eq!(usage_du_resultat(&ligne_json(RESULT_ABONNEMENT)), attendu);
    let jetons = Jetons { entree: 11234, sortie: 2, ..Jetons::default() };
    let flash =
        UsageModele { modele: "deepseek-flash".into(), jetons, micro_dollars: 44976, base: Some("unknown".into()) };
    let attendu = Usage { jetons, micro_dollars: 44976, duree_ms: 1049, tours: 1, modeles: vec![flash] };
    assert_eq!(usage_du_resultat(&ligne_json(RESULT_DEEPSEEK)), attendu);
    // Le résultat se juge comme avant : l'usage n'y change rien.
    assert_eq!(resultat(&ligne_json(RESULT_DEEPSEEK)), Ok("OK".into()));
}

#[test]
fn l_usage_absent_ou_mal_type_est_vide() {
    assert_eq!(usage_du_resultat(&ligne_json(RESULTAT)), Usage { tours: 1, ..Usage::default() });
    let faux = concat!(
        r#"{"type":"result","usage":"x","modelUsage":{"m":{"inputTokens":"2","costUSD":-1,"costBasis":3}},"#,
        r#""num_turns":null}"#,
    );
    let usage = usage_du_resultat(&ligne_json(faux));
    assert_eq!(usage.modeles, vec![UsageModele { modele: "m".into(), ..UsageModele::default() }]);
    assert_eq!(Usage { modeles: Vec::new(), ..usage }, Usage::default());
}

#[test]
fn l_ouverture_dit_sa_source_de_cle() {
    assert_eq!(source_cle(&ligne_json(OUVERTURE)), None);
    let abonnement = OUVERTURE.replace(r#""model""#, r#""apiKeySource":"none","model""#);
    assert_eq!(source_cle(&ligne_json(&abonnement)).as_deref(), Some("none"));
    let cle = OUVERTURE.replace(r#""model""#, r#""apiKeySource":"ANTHROPIC_API_KEY","model""#);
    assert_eq!(source_cle(&ligne_json(&cle)).as_deref(), Some("ANTHROPIC_API_KEY"));
}

#[test]
fn deux_lignes_assistant_se_cumulent() {
    let assistant = |modele: &str, entree: u32| {
        let usage = format!(
            concat!(
                r#"{{"input_tokens":{entree},"output_tokens":5,"cache_creation_input_tokens":7,"#,
                r#""cache_read_input_tokens":11}}"#,
            ),
            entree = entree,
        );
        format!(r#"{{"type":"assistant","message":{{"model":"{modele}","usage":{usage}}}}}"#)
    };
    let (mut cumul, mut vus) = (Usage::default(), Messages::new());
    // Sans `message.id`, chaque ligne compte pour elle seule.
    for ligne in [assistant("m-b", 1), assistant("m-a", 2), assistant("m-b", 3)] {
        let Ok(Ligne::Assistant(lue)) = lire_ligne(ligne.as_bytes()) else { panic!("{ligne}") };
        cumuler(&mut cumul, &mut vus, &lue);
    }
    // Une ligne sans usage n'ajoute rien.
    cumuler(&mut cumul, &mut vus, &ligne_json(r#"{"type":"assistant","message":{"model":"m-c"}}"#));
    assert_eq!(cumul.jetons, Jetons { entree: 6, sortie: 15, cache_ecrit: 21, cache_lu: 33, reflexion: 0 });
    let noms: Vec<&str> = cumul.modeles.iter().map(|m| m.modele.as_str()).collect();
    assert_eq!(noms, ["m-a", "m-b"]);
    assert_eq!(cumul.modeles[1].jetons, Jetons { entree: 4, sortie: 10, cache_ecrit: 14, cache_lu: 22, reflexion: 0 });
}

#[test]
fn deux_lignes_assistant_de_meme_id_ne_comptent_qu_une_fois() {
    let assistant = |id: &str, entree: u32| {
        let usage = format!(r#"{{"input_tokens":{entree},"output_tokens":5}}"#);
        format!(r#"{{"type":"assistant","message":{{"id":"{id}","model":"m","usage":{usage}}}}}"#)
    };
    let (mut cumul, mut vus) = (Usage::default(), Messages::new());
    let mut jouer = |ligne: String| {
        let Ok(Ligne::Assistant(lue)) = lire_ligne(ligne.as_bytes()) else { panic!("{ligne}") };
        cumuler(&mut cumul, &mut vus, &lue);
    };
    // Un message en deux blocs : deux lignes de même id, la dernière faisant foi.
    jouer(assistant("msg_1", 3));
    jouer(assistant("msg_1", 4));
    // Deux ids différents s'additionnent.
    jouer(assistant("msg_2", 10));
    let jetons = Jetons { entree: 14, sortie: 10, ..Jetons::default() };
    assert_eq!(cumul.jetons, jetons);
    assert_eq!(cumul.modeles.len(), 1);
    assert_eq!(cumul.modeles[0].jetons, jetons);
}

#[test]
fn un_usage_est_vide_sans_jeton_ni_depense_ni_modele() {
    assert!(Usage::default().est_vide());
    assert!(Usage { duree_ms: 900, tours: 2, ..Usage::default() }.est_vide(), "la durée et les tours n'y comptent pas");
    let jeton = Jetons { reflexion: 1, ..Jetons::default() };
    assert!(!Usage { jetons: jeton, ..Usage::default() }.est_vide());
    assert!(!Usage { micro_dollars: 1, ..Usage::default() }.est_vide());
    assert!(!Usage { modeles: vec![UsageModele::default()], ..Usage::default() }.est_vide());
}

#[test]
fn les_regles_des_identifiants_se_jugent_telles_que_declarees() {
    let regles = concat!(
        r#","credentials":{"envVars":[{"name":"ANTHROPIC_API_KEY","mode":"deny"},"#,
        r#"{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}"#,
    );
    let apres = format!(r#""strictAllowlist":true}}{regles}"#);
    let declare = REGLAGES.replace(r#""strictAllowlist":true}"#, &apres);
    let settings = GET_SETTINGS.replace(r#""strictAllowlist":true}"#, &apres);
    assert!(declare != REGLAGES && settings != GET_SETTINGS);
    assert_eq!(juger_reponses(&attendu(&declare), &avec(&settings)), (Some(false), Ok(())));
    // Absentes de l'effectif, ou autres : un écart, et la main meurt sans sa consigne.
    let autres = settings.replace(r#""name":"ANTHROPIC_AUTH_TOKEN""#, r#""name":"AUTRE""#);
    for effectif in [GET_SETTINGS.to_string(), autres] {
        let (_, jugement) = juger_reponses(&attendu(&declare), &avec(&effectif));
        let ecart = jugement.unwrap_err();
        assert!(
            ecart.starts_with("main : réglages non pris : get_settings : effective.sandbox.credentials.envVars "),
            "{ecart}"
        );
    }
}
