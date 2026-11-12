use super::*;
use crate::essais::mains::{bac_jetable, profil_borne};
use std::fs;

// Les lignes de Claude Code relevées le 1/11, réduites aux champs que Kraken lit.
const GET_SETTINGS: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id"#,
    r#"":"get_settings","response":{"applied":{"model":"claude-sonnet-5-5"},""#,
    r#"effective":{"disableAllHooks":true,"autoMemoryEnabled":false,"sandbox""#,
    r#":{"enabled":true,"failIfUnavailable":false,"allowUnsandboxedCommands":"#,
    r#"false,"filesystem":{"denyRead":["~/","/srv/lydite"],"allowRead":["/srv/lydite/bacs/main-1","#,
    r#""/srv/depot/.git"]},"network":{"allowedDomains":[],"strictAllowlist":true}}},"source"#,
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
    Attendu { reglages, modele: "sonnet", effort: None, mcp: None }
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
fn l_effort_applique_se_juge_quand_il_est_declare_et_sinon_ne_se_compare_a_rien() {
    let applique = |effort: &str| GET_SETTINGS.replace(r#""applied":{"#, &format!(r#""applied":{{{effort}"#));
    let avec = |settings: &str| {
        let mut reponses = rangees(&JUSTES);
        reponses.extend(rangees(&[settings]));
        reponses
    };
    let declare = |effort: &'static str| Attendu { effort: Some(effort), ..attendu(REGLAGES) };
    let ecart = |texte: &str| Err(format!("hand: settings not applied: get_settings : {texte}"));
    // L'effort appliqué est le déclaré : pris, pour chacune des valeurs que Claude Code accepte.
    for effort in ["low", "medium", "high", "xhigh", "max"] {
        let dit = applique(&format!(r#""effort":"{effort}","#));
        assert_eq!(juger_reponses(&declare(effort), &avec(&dit)).1, Ok(()), "{effort}");
    }
    // Un autre, un absent, un mal typé : l'écart, au texte exact, que le déclaré soit plus haut ou plus bas.
    let dit_low = applique(r#""effort":"low","#);
    assert_eq!(juger_reponses(&declare("high"), &avec(&dit_low)).1, ecart(r#"applied.effort "low", declared "high""#));
    assert_eq!(
        juger_reponses(&declare("low"), &avec(&applique(r#""effort":"high","#))).1,
        ecart(r#"applied.effort "high", declared "low""#)
    );
    assert_eq!(
        juger_reponses(&declare("high"), &rangees(&JUSTES)).1,
        ecart(r#"applied.effort absent, declared "high""#)
    );
    assert_eq!(
        juger_reponses(&declare("high"), &avec(&applique(r#""effort":3,"#))).1,
        ecart(r#"applied.effort 3, declared "high""#)
    );
    // Avec l'écart du modèle, dans l'ordre : le modèle, puis l'effort, joints par « ; ».
    let deux = dit_low.replace("claude-sonnet-5-5", "claude-opus-5-5");
    assert_eq!(
        juger_reponses(&declare("high"), &avec(&deux)).1,
        ecart(r#"applied.model "claude-opus-5-5", declared "sonnet" ; applied.effort "low", declared "high""#)
    );
    // Sans effort déclaré, rien ne se compare : ni l'absent, ni celui que Claude Code a pris de lui-même.
    for dit in [GET_SETTINGS.to_string(), dit_low, applique(r#""effort":"max","#)] {
        assert_eq!(juger_reponses(&attendu(REGLAGES), &avec(&dit)).1, Ok(()), "{dit}");
    }
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
                r#"get_settings : error "claude-settings.json" at "disableAllHooks" : "Ex"#,
                r#"pected boolean, but received string", declared without error"#,
            ),
        ),
        (
            REGLAGES.to_string(),
            settings(flag, ""),
            r#"get_settings : sources ["userSettings"], declared "flagSettings" among them"#,
        ),
        (
            REGLAGES.to_string(),
            settings(r#""disableAllHooks":true"#, r#""disableAllHooks":false"#),
            "get_settings : effective.disableAllHooks false, declared true",
        ),
        (
            REGLAGES.to_string(),
            settings(r#""autoMemoryEnabled":false,"#, ""),
            "get_settings : effective.autoMemoryEnabled absent, declared false",
        ),
        (
            REGLAGES.to_string(),
            settings(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#),
            "get_settings : effective.sandbox.failIfUnavailable true, declared false",
        ),
        (
            REGLAGES.to_string(),
            settings("claude-sonnet-5-5", "claude-opus-5-5"),
            r#"get_settings : applied.model "claude-opus-5-5", declared "sonnet""#,
        ),
        (
            deny("Edit(//c/lydite/fil.jsonl)"),
            LIST_PERMISSION_RULES.to_string(),
            r#"list_permission_rules : deny "Edit(//c/lydite/fil.jsonl)" declared, absent"#,
        ),
        (
            deny("Read(~/.ssh/**)"),
            LIST_PERMISSION_RULES.to_string(),
            r#"list_permission_rules : deny "Read(~/.ssh/**)" declared, seen as "deny" at "userSettings""#,
        ),
        (
            REGLAGES.to_string(),
            LIST_PERMISSION_RULES.replace(r#""behavior":"allow""#, r#""behavior":"deny""#),
            r#"list_permission_rules : allow "WebSearch" declared, seen as "deny" at "flagSettings""#,
        ),
        (
            REGLAGES.to_string(),
            GET_HOOKS_LISTING.replace(r#""allDisabled":true"#, r#""allDisabled":false"#),
            "get_hooks_listing : policy.allDisabled false, declared true",
        ),
        (REGLAGES.to_string(), en_erreur("get_settings"), r#"get_settings : requested, returned as "error" : "…""#),
        (
            REGLAGES.to_string(),
            en_erreur("list_permission_rules"),
            r#"list_permission_rules : requested, returned as "error" : "…""#,
        ),
        (
            REGLAGES.to_string(),
            en_erreur("get_hooks_listing"),
            r#"get_hooks_listing : requested, returned as "error" : "…""#,
        ),
    ];
    for (reglages, changee, dit) in &cas {
        let (mur_actif, jugement) = juger_reponses(&attendu(reglages), &avec(changee));
        assert_eq!(jugement, Err(format!("hand: settings not applied: {dit}")), "{changee}");
        assert_eq!(mur_actif, Some(false), "le mur se constate même sur un écart : {changee}");
    }
    // Un mur exigé, que Claude Code dit inactif.
    let exige = REGLAGES.replace(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#);
    let reponses = avec(&settings(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#));
    let inactif =
        "hand: settings not applied: get_sandbox_dialog : enabled false, declared required (failIfUnavailable true)";
    assert_eq!(juger_reponses(&attendu(&exige), &reponses), (Some(false), Err(inactif.into())));
    // Une réponse qui manque.
    let mut reponses = rangees(&JUSTES);
    reponses.remove("get_hooks_listing");
    let manque = "hand: settings not applied: get_hooks_listing : requested, no reply";
    assert_eq!(juger_reponses(&attendu(REGLAGES), &reponses), (Some(false), Err(manque.into())));
    // Le `--settings` tombé du 1/11, sans `flagSettings` et avec son erreur, puis une autre demande en écart :
    // chaque écart à la suite du nom de sa demande, tous joints par « ; ».
    let mut reponses = avec(&settings(flag, "").replace(r#""sources""#, erreurs));
    reponses.extend(rangees(&[&GET_HOOKS_LISTING.replace(r#""allDisabled":true"#, r#""allDisabled":false"#)]));
    let tombe = concat!(
        r#"hand: settings not applied: get_settings : sources ["userSettings"], declared "flagSettings" among them"#,
        r#" ; error "claude-settings.json" at "disableAllHooks" : "Expected boolean, but received string", declared"#,
        r#" without error ; get_hooks_listing : policy.allDisabled false, declared true"#,
    );
    assert_eq!(juger_reponses(&attendu(REGLAGES), &reponses).1, Err(tombe.into()));
    // Un `--settings` que Kraken aurait mal écrit ne se juge pas : la main ne part pas.
    let (mur_actif, jugement) = juger_reponses(&attendu("{"), &rangees(&JUSTES));
    assert_eq!(mur_actif, Some(false));
    assert!(jugement.unwrap_err().starts_with("hand: settings not applied: declared --settings unreadable : "));
}

#[test]
fn les_commandes_exclues_du_bac_a_sable_se_jugent_telles_que_declarees() {
    // `get_settings` dans la forme relevée le 1/11 au soir sous Windows : après `network`, son `sandbox` effectif
    // porte la preuve prêtée en `excludedCommands`. Le `--settings` la déclare juste après `allowUnsandboxedCommands`.
    let settings = GET_SETTINGS.replace(
        r#""strictAllowlist":true}}"#,
        concat!(r#""strictAllowlist":true},"excludedCommands":["findstr /x bonjour bon"#, r#"jour.txt"]}"#,),
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
        (declare.clone(), settings.replace(une, deux), "[2 items], declared [1 item]"),
        (declare.replace(une, deux), settings.clone(), "[1 item], declared [2 items]"),
        (declare.clone(), tombee, "absent, declared [1 item]"),
        (REGLAGES.to_string(), settings.clone(), "[1 item], declared absent"),
    ];
    for (reglages, changee, dit) in &cas {
        let (mur_actif, jugement) = juger_reponses(&attendu(reglages), &avec(changee));
        let ecart = format!("hand: settings not applied: get_settings : effective.sandbox.excludedCommands {dit}");
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
            r#"permissionMode "default", declared "acceptEdits""#,
        ),
        (
            OUVERTURE.replace(r#""Write"]"#, r#""Write","CronCreate"]"#),
            r#"tools carries "CronCreate", declared in deny"#,
        ),
        (mcp, "mcp_servers [1 item], declared []"),
    ];
    for (ligne, dit) in &cas {
        let attendu = Err(format!("hand: settings not applied: at startup, {dit}"));
        assert_eq!(juger_ouverture(&demande, &ouverture(ligne)), (Some("2.1.312".into()), attendu), "{ligne}");
    }
    // Sans `tools` ni version : un écart, et rien de constaté.
    let nue = ouverture(r#"{"type":"system","subtype":"init","mcp_servers":[],"permissionMode":"acceptEdits"}"#);
    let sans_outils = "hand: settings not applied: at startup, tools absent, declared a list without the deny names";
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
            r#"mcp_servers.name "autre", declared among ["docs", "lore"]"#,
        ),
        (
            r#"[{"name":"Docs","status":"connected"},{"status":"connected"}]"#,
            concat!(
                r#"mcp_servers.name "Docs", declared among ["docs", "lore"] ; "#,
                r#"mcp_servers.name absent, declared among ["docs", "lore"]"#,
            ),
        ),
    ];
    for (serveurs, dit) in cas {
        let ecart = Err(format!("hand: settings not applied: at startup, {dit}"));
        assert_eq!(juger_ouverture(&mcp, &avec(serveurs)), (Some("2.1.312".into()), ecart), "{serveurs}");
    }
    // Une ouverture sans la liste des serveurs : un écart aussi.
    let sans_liste = ouverture(&OUVERTURE.replace(r#""mcp_servers":[],"#, ""));
    let ecart = concat!(
        r#"hand: settings not applied: at startup, mcp_servers absent, "#,
        r#"declared a list of servers among ["docs", "lore"]"#,
    );
    assert_eq!(juger_ouverture(&mcp, &sans_liste), (Some("2.1.312".into()), Err(ecart.into())));
    // Sans `mcp`, un serveur arrête toujours la main, même en attente, même au nom qu'un autre profil déclare.
    let docs = avec(r#"[{"name":"docs","status":"pending"}]"#);
    let ecart = "hand: settings not applied: at startup, mcp_servers [1 item], declared []";
    assert_eq!(juger_ouverture(&attendu(REGLAGES), &docs), (Some("2.1.312".into()), Err(ecart.into())));
    // Un `mcpServers` vide ne déclare aucun serveur : un serveur arrête la main.
    let aucun = Attendu { mcp: Some(&[]), ..attendu(REGLAGES) };
    let ecart = r#"hand: settings not applied: at startup, mcp_servers.name "docs", declared among []"#;
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
    assert_eq!(ecart, Err(r#"hand: settings not applied: at startup, tools carries "Edit", declared in deny"#.into()));
}

#[test]
fn le_resultat_donne_la_sortie() {
    let fin = |ligne: &str| match lire_ligne(ligne.as_bytes()) {
        Ok(Ligne::Resultat(fin)) => fin,
        autre => panic!("un résultat : {ligne} donne {autre:?}"),
    };
    assert_eq!(resultat(&fin(RESULTAT)), Ok("fait\nPREUVE: vert".into()));
    let en_erreur = RESULTAT.replace(r#""is_error":false"#, r#""is_error":true"#);
    assert_eq!(resultat(&fin(&en_erreur)), Err("hand: result in error (success) : fait\nPREUVE: vert".into()));
    assert_eq!(resultat(&fin(SANS_MUR)), Err("hand: result in error (error_during_execution)".into()));
}

#[test]
fn une_coupure_de_l_api_se_reconnait_a_son_statut() {
    let avec = |is_error: &str, statut: &str| {
        let ligne = format!(
            concat!(
                r#"{{"type":"result","subtype":"success","is_error":{is_error},"api_error_status":{statut},"#,
                r#""num_turns":1,"result":"API Error"}}"#,
            ),
            is_error = is_error,
            statut = statut,
        );
        ligne_json(&ligne)
    };
    // Le fournisseur surchargé, une limite de débit, une erreur 5xx, le délai de la requête : des coupures.
    for statut in [408u16, 429, 500, 503, 529, 599] {
        let ligne = avec("true", &statut.to_string());
        assert_eq!(coupure(&ligne), Some(statut), "{statut}");
        // C'est aussi un résultat en erreur, que `resultat` dit comme avant.
        assert_eq!(resultat(&ligne), Err("hand: result in error (success) : API Error".into()), "{statut}");
    }
    // Un autre statut, ou aucun, reste une erreur.
    for statut in ["400", "401", "403", "404", "407", "499", "null", "\"529\"", "-1", "529.5"] {
        assert_eq!(coupure(&avec("true", statut)), None, "{statut}");
    }
    assert_eq!(
        coupure(&ligne_json(&RESULTAT.replace(r#""is_error":false"#, r#""is_error":true"#))),
        None,
        "sans statut"
    );
    assert_eq!(coupure(&ligne_json(SANS_MUR)), None, "sans statut");
    // Sans `is_error`, un statut ne fait pas une coupure.
    assert_eq!(coupure(&avec("false", "529")), None);
    assert_eq!(coupure(&ligne_json(RESULT_ABONNEMENT)), None, "la ligne réelle d'une main qui réussit");
}

#[test]
fn le_plafond_de_depense_se_reconnait_a_son_subtype() {
    let depense = ligne_json(
        r#"{"type":"result","subtype":"error_max_budget_usd","is_error":true,"num_turns":3,"total_cost_usd":1.2}"#,
    );
    assert!(plafond_atteint(&depense));
    // Un résultat en erreur que `resultat` dit comme les autres, et qui n'est pas une coupure de l'API.
    assert_eq!(resultat(&depense), Err("hand: result in error (error_max_budget_usd)".into()));
    assert_eq!(coupure(&depense), None);
    // Une main qui réussit, un autre résultat en erreur, ou un autre mot dans le texte, ne sont pas un plafond.
    assert!(!plafond_atteint(&ligne_json(RESULTAT)));
    assert!(!plafond_atteint(&ligne_json(SANS_MUR)));
    let texte = RESULTAT.replace("fait", "error_max_budget_usd");
    assert!(!plafond_atteint(&ligne_json(&texte)), "le texte du résultat ne décide pas");
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
    let illisible = |raison: &str| Err(format!("hand: settings not applied: unreadable line : {raison}"));
    assert_eq!(lire_ligne(&octets), illisible("invalid UTF-8 after byte 10"));
    assert_eq!(
        lire_ligne(&GET_SETTINGS.as_bytes()[..7]),
        illisible("at character 8: expected `:`, saw the end of the line")
    );
    assert_eq!(lire_ligne(b""), illisible("at character 1: expected a value, saw the end of the line"));
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
            ecart.starts_with("hand: settings not applied: get_settings : effective.sandbox.credentials.envVars "),
            "{ecart}"
        );
    }
}

#[test]
fn les_regles_des_identifiants_s_additionnent_d_une_source_a_l_autre() {
    let cle = r#"{"name":"ANTHROPIC_API_KEY","mode":"deny"}"#;
    let jeton = r#"{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}"#;
    let declare = REGLAGES.replace(
        r#""strictAllowlist":true}"#,
        &format!(r#""strictAllowlist":true}},"credentials":{{"envVars":[{cle},{jeton}]}}"#),
    );
    let jugement = |effectif: Option<&str>| {
        let apres = effectif.map_or_else(
            || r#""strictAllowlist":true}"#.to_string(),
            |liste| format!(r#""strictAllowlist":true}},"credentials":{{"envVars":{liste}}}"#),
        );
        let settings = GET_SETTINGS.replace(r#""strictAllowlist":true}"#, &apres);
        juger_reponses(&attendu(&declare), &avec(&settings)).1
    };
    // L'effectif égal au déclaré, puis celui de la VM : userSettings, puis flagSettings.
    assert_eq!(jugement(Some(&format!("[{cle},{jeton}]"))), Ok(()));
    assert_eq!(jugement(Some(&format!("[{cle},{jeton},{cle},{jeton}]"))), Ok(()));
    // Un refus de plus, pour une autre variable, ne rouvre rien.
    let autre = r#"{"name":"AUTRE","mode":"deny"}"#;
    assert_eq!(jugement(Some(&format!("[{cle},{jeton},{autre}]"))), Ok(()));
    // Une entrée déclarée qui manque, la même variable en `allow`, ou pas de liste : un écart.
    let allow = r#"{"name":"ANTHROPIC_API_KEY","mode":"allow"}"#;
    for effectif in [Some(format!("[{cle}]")), Some(format!("[{cle},{jeton},{allow}]")), Some("{}".to_string()), None] {
        let ecart = jugement(effectif.as_deref()).unwrap_err();
        assert!(
            ecart.starts_with("hand: settings not applied: get_settings : effective.sandbox.credentials.envVars "),
            "{ecart}"
        );
    }
    let ecart = jugement(Some(&format!("[{cle}]"))).unwrap_err();
    assert!(ecart.ends_with("[1 item], declared [2 items]"), "{ecart}");
}

/// Le jugement de `get_settings` sur ces réglages déclarés, l'effectif étant `GET_SETTINGS` où `de` devient `en`.
fn jugement_du_mur(declare: &str, de: &str, en: &str) -> Result<(), String> {
    let settings = GET_SETTINGS.replace(de, en);
    assert!(de == en || settings != GET_SETTINGS, "{de} n'est pas dans GET_SETTINGS");
    juger_reponses(&attendu(declare), &avec(&settings)).1
}

#[test]
fn une_allow_write_de_l_humain_au_releve_d_une_vraie_main_est_un_ecart() {
    // Le relevé réduit : l'`allowWrite` posée par l'humain dans userSettings arrive à l'effectif.
    let fermes = r#""denyRead":["~/","/srv/lydite"]"#;
    let avec_la_cle = GET_SETTINGS.replace(fermes, &format!(r#""allowWrite":["/srv/ecrit"],{fermes}"#));
    assert_ne!(avec_la_cle, GET_SETTINGS);
    let releve = avec_la_cle.replace(
        r#"{"source":"userSettings","settings":{}}"#,
        r#"{"source":"userSettings","settings":{"sandbox":{"filesystem":{"allowWrite":["/srv/ecrit"]}}}}"#,
    );
    assert_ne!(releve, avec_la_cle);
    let ecart = juger_reponses(&attendu(REGLAGES), &avec(&releve)).1.unwrap_err();
    assert_eq!(
        ecart,
        concat!(
            "hand: settings not applied: get_settings : ",
            r#"effective.sandbox.filesystem.allowWrite : entry "/srv/ecrit" outside the declared list, "#,
            r#"seen at "userSettings" : remove it from "#,
            "~/.claude/settings.json (or the settings.json in the folder CLAUDE_CONFIG_DIR names) to launch Lydite",
        )
    );
}

// Le `get_settings` d'une vraie main, tel quel : Claude Code 2.1.317, Linux, relevée le 11/11/2026, avec une
// `sandbox.filesystem.allowWrite` posée par l'humain dans `~/.claude/settings.json`, chemins remplacés par `/srv/…`.
// Une ligne en un seul morceau, que `concat!` recolle : aucune ligne du source ne passe 120 caractères.
const RELEVE_2_1_317: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_settings","response":{"#,
    r#""effective":{"sandbox":{"filesystem":{"allowWrite":["/srv/ecrit"],"denyRead":["~/","/srv/lydite"],"#,
    r#""allowRead":["/srv/lydite/bacs/main-1","/srv/depot/.git","/srv/outils/lydite","#,
    r#""~/.cargo","~/.rustup","~/outils"]},"credentials":{"envVars":["#,
    r#"{"name":"ANTHROPIC_API_KEY","mode":"deny"},{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"},"#,
    r#"{"name":"ANTHROPIC_API_KEY","mode":"deny"},{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]},"#,
    r#""enabled":true,"failIfUnavailable":true,"allowUnsandboxedCommands":false,"#,
    r#""network":{"allowedDomains":[],"strictAllowlist":true}},"permissions":{"#,
    r#""allow":["Skill","Workflow","ExitPlanMode","Bash(test -f Cargo.toml)","PowerShell(test -f Cargo.toml)"],"#,
    r#""deny":["CronCreate","CronDelete","CronList","ScheduleWakeup","RemoteTrigger","SendMessage","ListAgents","#,
    r#""PushNotification","SendUserFile","Artifact","ArtifactComments","ArtifactData","ArtifactCheck","#,
    r#""DesignSync","SendToProject","SendToProjectContext","ShareOnboardingGuide","AskUserQuestion","WebSearch","#,
    r#""Edit(//srv/lydite/fil.jsonl)","Edit(//srv/lydite/profils/**)","Read(//srv/lydite/srt-*.json)","#,
    r#""Edit(//srv/lydite/srt-*.json)","Read(//srv/claude/.credentials.json)","#,
    r#""Edit(//srv/claude/.credentials.json)","Edit(//srv/depot/.git/**)","Bash(git commit:*)","#,
    r#""Bash(git commit *)","PowerShell(git commit:*)","PowerShell(git commit *)","#,
    r#""Edit(//srv/lydite/bacs/main-1/.agents/skills/**)"]},"disableAllHooks":true,"autoMemoryEnabled":false},"#,
    r#""sources":[{"source":"userSettings","settings":{"sandbox":{"filesystem":{"allowWrite":["/srv/ecrit"]},"#,
    r#""credentials":{"envVars":[{"name":"ANTHROPIC_API_KEY","mode":"deny"},"#,
    r#"{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}}}},{"source":"flagSettings","settings":{"permissions":{"#,
    r#""allow":["Skill","Workflow","ExitPlanMode","Bash(test -f Cargo.toml)","PowerShell(test -f Cargo.toml)"],"#,
    r#""deny":["CronCreate","CronDelete","CronList","ScheduleWakeup","RemoteTrigger","SendMessage","ListAgents","#,
    r#""PushNotification","SendUserFile","Artifact","ArtifactComments","ArtifactData","ArtifactCheck","#,
    r#""DesignSync","SendToProject","SendToProjectContext","ShareOnboardingGuide","AskUserQuestion","WebSearch","#,
    r#""Edit(//srv/lydite/fil.jsonl)","Edit(//srv/lydite/profils/**)","Read(//srv/lydite/srt-*.json)","#,
    r#""Edit(//srv/lydite/srt-*.json)","Read(//srv/claude/.credentials.json)","#,
    r#""Edit(//srv/claude/.credentials.json)","Edit(//srv/depot/.git/**)","Bash(git commit:*)","#,
    r#""Bash(git commit *)","PowerShell(git commit:*)","PowerShell(git commit *)","#,
    r#""Edit(//srv/lydite/bacs/main-1/.agents/skills/**)"]},"disableAllHooks":true,"sandbox":{"enabled":true,"#,
    r#""failIfUnavailable":true,"allowUnsandboxedCommands":false,"#,
    r#""network":{"allowedDomains":[],"strictAllowlist":true},"filesystem":{"denyRead":["~/","/srv/lydite"],"#,
    r#""allowRead":["/srv/lydite/bacs/main-1","/srv/depot/.git","/srv/outils/lydite","#,
    r#""~/.cargo","~/.rustup","~/outils"]},"credentials":{"envVars":["#,
    r#"{"name":"ANTHROPIC_API_KEY","mode":"deny"},{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}},"#,
    r#""autoMemoryEnabled":false}}],"applied":{"model":"claude-fable-5-2","effort":"medium","advisor":null,"#,
    r#""ultracode":false,"ultracodeRequested":false,"ultracodeAvailable":true,"ultrathinkAvailable":false}}}}"#,
);

#[test]
fn le_releve_d_une_vraie_main_ne_donne_qu_l_ecart_de_l_allow_write_de_l_humain() {
    // Son `--settings` déclaré : les réglages de la source `flagSettings` du relevé, pris dans le relevé même.
    let (_, fin) = RELEVE_2_1_317.split_once(r#"{"source":"flagSettings","settings":"#).unwrap();
    // Après les réglages, la source se ferme (`}`), puis la liste des sources (`]`).
    let (declare, _) = fin.split_once(r#"}],"applied""#).unwrap();
    assert!(declare.starts_with(r#"{"permissions":"#), "{declare}");
    assert!(declare.ends_with(r#""autoMemoryEnabled":false}"#), "{declare}");
    let reponses = rangees(&[RELEVE_2_1_317]);
    let corps = champ(&reponses["get_settings"], "response");
    assert!(corps.is_some());
    let declare = json::document(declare).unwrap();
    // Le modèle `fable` est appliqué comme `claude-fable-5-2`.
    let ecarts = reglages(corps, &declare, "fable", None);
    assert_eq!(
        ecarts,
        [concat!(
            r#"effective.sandbox.filesystem.allowWrite : entry "/srv/ecrit" outside the declared list, "#,
            r#"seen at "userSettings" : remove it from "#,
            "~/.claude/settings.json (or the settings.json in the folder CLAUDE_CONFIG_DIR names) to launch Lydite",
        )]
    );
}

#[test]
fn l_ecart_d_une_entree_de_liste_ouvrante_dit_sa_source_et_ou_la_retirer() {
    let lus = r#""allowRead":["/srv/lydite/bacs/main-1","/srv/depot/.git"]"#;
    let effectif = r#""allowRead":["/srv/lydite/bacs/main-1","/srv/depot/.git","/srv/autre"]"#;
    let entree = r#"effective.sandbox.filesystem.allowRead : entry "/srv/autre" outside the declared list"#;
    let fichier = "~/.claude/settings.json (or the settings.json in the folder CLAUDE_CONFIG_DIR names)";
    let prefixe = format!("hand: settings not applied: get_settings : {entree}");
    // Les sources du relevé, `source` de plus à leur suite.
    let juge = |source: &str| {
        let sources = format!(r#""settings":{{}}}}{source}]"#);
        let settings = GET_SETTINGS.replace(lus, effectif).replace(r#""settings":{}}]"#, &sources);
        assert_ne!(settings, GET_SETTINGS);
        juger_reponses(&attendu(REGLAGES), &avec(&settings)).1.unwrap_err()
    };
    let porte = |source: &str| {
        format!(r#"{{"source":"{source}","settings":{{"sandbox":{{"filesystem":{{"allowRead":["/srv/autre"]}}}}}}}}"#)
    };
    // Sans source lisible : les sources ne portent pas l'entrée.
    let sans = ": remove it from the settings of the source that adds it to launch Lydite";
    assert_eq!(juge(""), format!("{prefixe}{sans}"));
    // Une source qui la porte : son nom, et son fichier pour `userSettings`.
    let de_l_utilisateur = format!(r#", seen at "userSettings" : remove it from {fichier} to launch Lydite"#);
    assert_eq!(juge(&format!(",{}", porte("userSettings"))), format!("{prefixe}{de_l_utilisateur}"));
    // Une autre source : son nom seul. Deux sources : toutes deux, dans l'ordre des sources.
    let d_une_autre = concat!(
        r#", seen at "projectSettings" : remove it from "#,
        r#"the settings of "projectSettings" to launch Lydite"#,
    );
    assert_eq!(juge(&format!(",{}", porte("projectSettings"))), format!("{prefixe}{d_une_autre}"));
    let de_deux = concat!(
        r#", seen at "projectSettings", "localSettings" : remove it from "#,
        r#"the settings of "projectSettings" and the settings of "localSettings" to launch Lydite"#,
    );
    let deux_sources = format!(",{},{}", porte("projectSettings"), porte("localSettings"));
    assert_eq!(juge(&deux_sources), format!("{prefixe}{de_deux}"));
}

#[test]
fn l_ecart_d_une_feuille_non_jugee_dit_sa_source_et_ou_la_retirer() {
    let socle = r#""strictAllowlist":true}"#;
    let feuille = r#"effective.sandbox.network.allowLocalBinding true, declared absent"#;
    let fichier = "~/.claude/settings.json (or the settings.json in the folder CLAUDE_CONFIG_DIR names)";
    let prefixe = format!("hand: settings not applied: get_settings : {feuille}");
    // L'effectif porte la feuille ; `userSettings` porte `porte`.
    let juge = |porte: &str| {
        let settings = GET_SETTINGS.replace(socle, r#""strictAllowlist":true,"allowLocalBinding":true}"#);
        let settings = settings.replace(
            r#"{"source":"userSettings","settings":{}}"#,
            &format!(r#"{{"source":"userSettings","settings":{porte}}}"#),
        );
        assert_ne!(settings, GET_SETTINGS);
        juger_reponses(&attendu(REGLAGES), &avec(&settings)).1.unwrap_err()
    };
    // Sans source lisible : la source ne porte pas la valeur, ou porte une autre valeur.
    let sans = format!("{prefixe}: remove it from the settings of the source that adds it to launch Lydite");
    assert_eq!(juge("{}"), sans);
    assert_eq!(juge(r#"{"sandbox":{"network":{"allowLocalBinding":false}}}"#), sans);
    // La source qui porte la valeur : son nom, et son fichier pour `userSettings`.
    assert_eq!(
        juge(r#"{"sandbox":{"network":{"allowLocalBinding":true}}}"#),
        format!(r#"{prefixe}, seen at "userSettings" : remove it from {fichier} to launch Lydite"#)
    );
}

#[test]
fn une_feuille_du_bac_qu_aucune_liste_ne_juge_est_un_ecart_sauf_vide_ou_fermante() {
    let socle = r#""strictAllowlist":true}"#;
    let reseau = |ajout: &str| jugement_du_mur(REGLAGES, socle, &format!(r#""strictAllowlist":true,{ajout}}}"#));
    let ecart_de = |ajout: &str, dit: &str| {
        let ecart = reseau(ajout).unwrap_err();
        assert_eq!(
            ecart,
            format!(
                "hand: settings not applied: get_settings : effective.sandbox.{dit}, declared absent\
                 : remove it from the settings of the source that adds it to launch Lydite"
            )
        );
    };
    // Une clé inconnue posée : un écart qui dit son chemin et sa valeur.
    ecart_de(r#""allowLocalBinding":true"#, "network.allowLocalBinding true");
    ecart_de(r#""httpProxyPort":8080"#, "network.httpProxyPort 8080");
    ecart_de(r#""inconnue":["a"]"#, "network.inconnue [1 item]");
    // On descend dans les objets : l'écart nomme la feuille.
    ecart_de(r#""inconnue":{"profonde":{"cle":"x"}}"#, r#"network.inconnue.profonde.cle "x""#);
    let haut = jugement_du_mur(
        REGLAGES,
        r#""sandbox":{"enabled":true"#,
        r#""sandbox":{"enableWeakerNestedSandbox":true,"enabled":true"#,
    );
    assert_eq!(
        haut,
        Err(concat!(
            "hand: settings not applied: get_settings : ",
            "effective.sandbox.enableWeakerNestedSandbox true, declared absent",
            ": remove it from the settings of the source that adds it to launch Lydite",
        )
        .to_string())
    );
    // `false`, `[]`, `{}` et les trois clés qui n'ouvrent pas le bac passent.
    for passe in [
        r#""allowLocalBinding":false"#,
        r#""inconnue":[]"#,
        r#""inconnue":{}"#,
        r#""inconnue":{"profonde":{}}"#,
        r#""deniedDomains":["x.test"]"#,
    ] {
        assert_eq!(reseau(passe), Ok(()), "{passe}");
    }
    let fichiers = r#""denyRead":["~/","/srv/lydite"]"#;
    for passe in [r#""denyWrite":["/srv/lu"],"#, r#""denyWrite":[],"#] {
        assert_eq!(jugement_du_mur(REGLAGES, fichiers, &format!("{passe}{fichiers}")), Ok(()), "{passe}");
    }
    let haut = r#""sandbox":{"enabled":true"#;
    assert_eq!(jugement_du_mur(REGLAGES, haut, r#""sandbox":{"autoAllowBashIfSandboxed":true,"enabled":true"#), Ok(()));
}

#[test]
fn les_listes_qui_ouvrent_le_mur_se_jugent_par_leurs_entrees_effectives() {
    let lus = r#""allowRead":["/srv/lydite/bacs/main-1","/srv/depot/.git"]"#;
    // Une entrée de plus à l'effectif, qu'une autre source ajouterait : un écart qui nomme la clé et l'entrée.
    let ecart =
        jugement_du_mur(REGLAGES, lus, r#""allowRead":["/srv/lydite/bacs/main-1","/srv/depot/.git","/srv/autre"]"#)
            .unwrap_err();
    assert_eq!(
        ecart,
        concat!(
            "hand: settings not applied: get_settings : ",
            r#"effective.sandbox.filesystem.allowRead : entry "/srv/autre" outside the declared list"#,
            ": remove it from the settings of the source that adds it to launch Lydite",
        )
    );
    // Une entrée de moins ferme davantage le mur, et une clé absente de l'effectif vaut une liste vide : pris.
    assert_eq!(jugement_du_mur(REGLAGES, lus, r#""allowRead":["/srv/lydite/bacs/main-1"]"#), Ok(()));
    assert_eq!(jugement_du_mur(REGLAGES, &format!(",{lus}"), ""), Ok(()));
    // Une liste qui n'en est pas une n'est pas lue comme vide : un écart.
    let ecart = jugement_du_mur(REGLAGES, lus, r#""allowRead":"tout""#).unwrap_err();
    assert!(ecart.contains(r#"effective.sandbox.filesystem.allowRead "tout", declared a list"#), "{ecart}");
    // Les domaines du réseau suivent la même règle.
    let declare = REGLAGES.replace(r#""allowedDomains":[]"#, r#""allowedDomains":["a.test"]"#);
    let domaines = r#""allowedDomains":[]"#;
    assert_eq!(jugement_du_mur(&declare, domaines, r#""allowedDomains":["a.test"]"#), Ok(()));
    assert_eq!(jugement_du_mur(&declare, domaines, domaines), Ok(()));
    for effectif in [r#"["a.test","b.test"]"#, r#"["b.test"]"#] {
        let ecart = jugement_du_mur(&declare, domaines, &format!(r#""allowedDomains":{effectif}"#)).unwrap_err();
        assert!(
            ecart.contains(r#"effective.sandbox.network.allowedDomains : entry "b.test" outside the declared list"#),
            "{ecart}"
        );
    }
    // Rien de déclaré : toute entrée effective est de trop.
    let ecart = jugement_du_mur(REGLAGES, domaines, r#""allowedDomains":["b.test"]"#).unwrap_err();
    assert!(ecart.contains(r#"allowedDomains : entry "b.test" outside the declared list"#), "{ecart}");
}

#[test]
fn la_liste_qui_ferme_le_mur_garde_chaque_entree_declaree() {
    let fermes = r#""denyRead":["~/","/srv/lydite"]"#;
    // Une entrée déclarée qui manque : un écart qui nomme la clé et l'entrée.
    let ecart = jugement_du_mur(REGLAGES, fermes, r#""denyRead":["~/"]"#).unwrap_err();
    assert_eq!(
        ecart,
        concat!(
            "hand: settings not applied: get_settings : ",
            r#"effective.sandbox.filesystem.denyRead : entry "/srv/lydite" declared, absent"#,
        )
    );
    // Une clé absente, ou qui n'est pas une liste, ne garde rien.
    assert!(jugement_du_mur(REGLAGES, &format!("{fermes},"), "").is_err());
    assert!(jugement_du_mur(REGLAGES, fermes, r#""denyRead":"~/""#).is_err());
    // Une entrée de plus ferme davantage le mur : pas un écart.
    assert_eq!(jugement_du_mur(REGLAGES, fermes, r#""denyRead":["~/","/srv/lydite","/etc"]"#), Ok(()));
    assert_eq!(jugement_du_mur(REGLAGES, fermes, fermes), Ok(()));
}

/// Les réponses justes, avec cette règle de plus dans `list_permission_rules`.
fn avec_regle(comportement: &str, source: &str, regle: &str) -> BTreeMap<String, Json> {
    let plus = format!(r#"{{"behavior":"{comportement}","source":"{source}","rule":"{regle}"}}]"#);
    let fin = r#""rule":"Read(~/.ssh/**)"}"#;
    let liste = LIST_PERMISSION_RULES.replace(&format!("{fin}]"), &format!("{fin},{plus}"));
    assert!(liste != LIST_PERMISSION_RULES);
    avec(&liste)
}

/// Les mêmes réponses au mur exigé, que Claude Code dit actif, avec le `--settings` qui le déclare.
fn au_mur_exige(mut reponses: BTreeMap<String, Json>) -> (String, BTreeMap<String, Json>) {
    let declare = REGLAGES.replace(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#);
    let settings = GET_SETTINGS.replace(r#""failIfUnavailable":false"#, r#""failIfUnavailable":true"#);
    let dialogue = GET_SANDBOX_DIALOG.replace(r#""enabled":false"#, r#""enabled":true"#);
    assert!(declare != REGLAGES && settings != GET_SETTINGS && dialogue != GET_SANDBOX_DIALOG);
    reponses.extend(rangees(&[&settings, &dialogue]));
    (declare, reponses)
}

fn ecart_de_regle(regle: &str, source: &str) -> String {
    let lieu = match source {
        "userSettings" => {
            "~/.claude/settings.json (or the settings.json in the folder CLAUDE_CONFIG_DIR names)".to_string()
        }
        autre => format!("the settings of \"{autre}\""),
    };
    format!(
        "hand: settings not applied: list_permission_rules : allow \"{regle}\" seen at \"{source}\", not declared : \
         remove this rule from {lieu} to launch Lydite. \
         A rule on Bash or PowerShell stays allowed \
         under a profile that requires the wall (`mur = exige` or `mur = natif`)"
    )
}

#[test]
fn l_ecart_d_une_regle_allow_de_user_settings_se_dit_en_entier() {
    let reponses = avec_regle("allow", "userSettings", "Read(/srv/gh/**)");
    let dit = concat!(
        "hand: settings not applied: list_permission_rules : ",
        r#"allow "Read(/srv/gh/**)" seen at "userSettings", not declared : "#,
        "remove this rule from ~/.claude/settings.json (or the settings.json in the folder CLAUDE_CONFIG_DIR names) ",
        "to launch Lydite. A rule on Bash or PowerShell stays allowed ",
        "under a profile that requires the wall (`mur = exige` or `mur = natif`)",
    );
    assert_eq!(juger_reponses(&attendu(REGLAGES), &reponses).1, Err(dit.to_string()));
    // Une autre source : son nom seul, pas un fichier que Lydite ne connaît pas.
    let reponses = avec_regle("allow", "projectSettings", "Read(/srv/gh/**)");
    let ecart = juger_reponses(&attendu(REGLAGES), &reponses).1.unwrap_err();
    let attendu_ici = concat!(
        r#"seen at "projectSettings", not declared : "#,
        r#"remove this rule from the settings of "projectSettings" to launch"#,
    );
    assert!(ecart.contains(attendu_ici), "{ecart}");
    assert!(!ecart.contains("~/.claude"), "{ecart}");
}

#[test]
fn une_regle_allow_non_declaree_d_une_autre_source_est_un_ecart() {
    for regle in ["Read(/srv/gh/**)", "WebFetch", "mcp__serveur__outil", "Glob", "Skill"] {
        let ecart = ecart_de_regle(regle, "userSettings");
        // Au mur demandé : un écart qui nomme la règle et sa source.
        let (_, jugement) = {
            let reponses = avec_regle("allow", "userSettings", regle);
            juger_reponses(&attendu(REGLAGES), &reponses)
        };
        assert_eq!(jugement, Err(ecart.clone()));
        // Au mur exigé : de même, l'exception ne vaut que pour Bash et PowerShell.
        let (declare, reponses) = au_mur_exige(avec_regle("allow", "userSettings", regle));
        assert_eq!(juger_reponses(&attendu(&declare), &reponses).1, Err(ecart));
    }
    // Une règle non déclarée vue à `flagSettings` même, un écart aussi.
    let reponses = avec_regle("allow", "flagSettings", "Read(/etc/**)");
    assert_eq!(juger_reponses(&attendu(REGLAGES), &reponses).1, Err(ecart_de_regle("Read(/etc/**)", "flagSettings")));
}

#[test]
fn une_regle_allow_bash_ou_powershell_n_est_un_ecart_qu_au_mur_seulement_demande() {
    for regle in ["Bash", "Bash(git status:*)", "PowerShell", "PowerShell(Get-Date)"] {
        let (declare, reponses) = au_mur_exige(avec_regle("allow", "userSettings", regle));
        assert_eq!(juger_reponses(&attendu(&declare), &reponses), (Some(true), Ok(())), "{regle}");
        let reponses = avec_regle("allow", "userSettings", regle);
        assert_eq!(
            juger_reponses(&attendu(REGLAGES), &reponses).1,
            Err(ecart_de_regle(regle, "userSettings")),
            "{regle}"
        );
    }
    // Un outil dont le nom ne fait que commencer par `Bash` n'est pas `Bash`.
    let (declare, reponses) = au_mur_exige(avec_regle("allow", "userSettings", "BashOutput"));
    assert_eq!(juger_reponses(&attendu(&declare), &reponses).1, Err(ecart_de_regle("BashOutput", "userSettings")));
}

#[test]
fn une_regle_declaree_vue_a_flag_settings_ou_une_regle_deny_d_une_autre_source_n_est_pas_un_ecart() {
    // Les réponses justes portent déjà `WebSearch` en `allow` à `flagSettings`, déclarée.
    assert_eq!(juger_reponses(&attendu(REGLAGES), &rangees(&JUSTES)), (Some(false), Ok(())));
    // Une règle `deny` ou `ask` d'une autre source ferme ou ne répond à rien : pris.
    for comportement in ["deny", "ask"] {
        let reponses = avec_regle(comportement, "userSettings", "Read(/srv/gh/**)");
        assert_eq!(juger_reponses(&attendu(REGLAGES), &reponses), (Some(false), Ok(())), "{comportement}");
    }
}

#[test]
fn les_dossiers_de_plus_des_outils_de_fichiers_sont_un_ecart() {
    let avec_dossiers = |dossiers: &str| {
        let settings = GET_SETTINGS.replace(
            r#""effective":{"#,
            &format!(r#""effective":{{"permissions":{{"additionalDirectories":{dossiers}}},"#),
        );
        assert!(settings != GET_SETTINGS);
        juger_reponses(&attendu(REGLAGES), &avec(&settings)).1
    };
    let ecart = avec_dossiers(r#"["/srv/gh"]"#).unwrap_err();
    assert_eq!(
        ecart,
        concat!(
            "hand: settings not applied: get_settings : ",
            r#"effective.permissions.additionalDirectories : directory "/srv/gh" outside the declared list (none) : "#,
            "remove this entry from permissions.additionalDirectories in ",
            "the settings of the source that adds it to launch Lydite",
        )
    );
    // La source qui porte le dossier, quand `sources` le dit : son fichier, pour `userSettings`.
    let de_l_utilisateur = GET_SETTINGS
        .replace(
            r#"{"source":"userSettings","settings":{}}"#,
            r#"{"source":"userSettings","settings":{"permissions":{"additionalDirectories":["/srv/gh"]}}}"#,
        )
        .replace(r#""effective":{"#, r#""effective":{"permissions":{"additionalDirectories":["/srv/gh"]},"#);
    assert!(de_l_utilisateur.contains(r#""settings":{"permissions""#));
    let ecart = juger_reponses(&attendu(REGLAGES), &avec(&de_l_utilisateur)).1.unwrap_err();
    assert_eq!(
        ecart,
        concat!(
            "hand: settings not applied: get_settings : ",
            r#"effective.permissions.additionalDirectories : directory "/srv/gh" outside the declared list (none), "#,
            r#"seen at "userSettings" : remove this entry from permissions.additionalDirectories in "#,
            "~/.claude/settings.json (or the settings.json in the folder CLAUDE_CONFIG_DIR names) to launch Lydite",
        )
    );
    // Chaque entrée est nommée ; une valeur qui n'est pas une liste n'est pas lue comme vide.
    let ecart = avec_dossiers(r#"["/a","/b"]"#).unwrap_err();
    assert!(ecart.contains(r#"directory "/a" outside"#) && ecart.contains(r#"directory "/b" outside"#), "{ecart}");
    let ecart = avec_dossiers(r#""tout""#).unwrap_err();
    assert!(ecart.contains(r#"additionalDirectories "tout", declared an empty list"#), "{ecart}");
    // Une liste vide, et une clé absente (les réponses justes), ne sont pas un écart.
    assert_eq!(avec_dossiers("[]"), Ok(()));
    assert_eq!(juger_reponses(&attendu(REGLAGES), &rangees(&JUSTES)).1, Ok(()));
}

/// Les chemins des feuilles de ce JSON sous `chemin` : un objet se parcourt, tout autre valeur, liste comprise, est
/// une feuille.
fn feuilles(json: &Json, chemin: &str, sortie: &mut Vec<String>) {
    match json {
        Json::Objet(objet) => {
            for (cle, valeur) in objet {
                feuilles(valeur, &format!("{chemin}.{cle}"), sortie);
            }
        }
        _ => sortie.push(chemin.to_string()),
    }
}

#[test]
fn chaque_cle_sandbox_du_profil_est_jugee_ou_nommee_non_jugee() {
    let (dir, bac) = bac_jetable("cles-jugees");
    let mut ecrites = Vec::new();
    // Le mur demandé avec une preuve prêtée (`excludedCommands` s'écrit), puis le mur exigé, avec un réseau.
    let profils = [
        ("sorte = claude\n", Some("cargo test && git status")),
        ("sorte = claude\nmur = exige\nreseau = exemple.test\n", None),
    ];
    for (texte, verifier) in profils {
        let declaration = profil_borne(texte, &dir).demande(&bac, verifier, false).declaration.unwrap();
        let declare = json::document(&declaration.reglages).unwrap();
        feuilles(champ(&declare, "sandbox").unwrap(), "sandbox", &mut ecrites);
    }
    let _ = fs::remove_dir_all(&dir);
    assert!(ecrites.iter().any(|cle| cle == "sandbox.excludedCommands"), "{ecrites:?}");
    assert!(ecrites.iter().any(|cle| cle == "sandbox.filesystem.allowRead"), "{ecrites:?}");
    let jugees: Vec<&str> = MUR.iter().chain(&OUVRANT).chain([&VARIABLES, &FERMANT]).copied().collect();
    for cle in &ecrites {
        let nommee = NON_JUGEES.iter().any(|(nom, raison)| *nom == cle.as_str() && !raison.is_empty());
        assert!(jugees.contains(&cle.as_str()) || nommee, "{cle} s'écrit dans --settings sans être jugée ni nommée");
    }
}
