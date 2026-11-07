//! Le dialogue de Kraken avec une main Claude Code en stream-json, sans processus ni E/S : ce qu'il lui écrit, et le
//! jugement de ce qu'elle lui répond. `jouer` (claude.rs) écrit, lit, attend et tue ; ce qui se décide sur une ligne
//! se décide ici.
//!
//! La main part en `claude -p --input-format stream-json --output-format stream-json --verbose`, avec `--settings`,
//! `--model` et ses autres drapeaux : un message JSON par ligne, sur son entrée comme sur sa sortie. Le dialogue :
//!
//! 1. Kraken écrit les quatre demandes de contrôle de [`DEMANDES`] ([`demandes`]), une ligne chacune, l'id valant le
//!    sous-type : `{"type":"control_request","request_id":"get_settings","request":{"subtype":"get_settings"}}`, puis
//!    de même `list_permission_rules`, `get_hooks_listing` et `get_sandbox_dialog`. Rien ne tourne avant la consigne :
//!    la main ne fait que répondre, en 1 à 1,4 s.
//! 2. Une réponse, `{"type":"control_response","response":{"subtype":"success","request_id":"<id>","response":{…}}}`,
//!    s'apparie à sa demande par son id, dans n'importe quel ordre ([`lire_ligne`]). Une réponse en erreur porte
//!    `"subtype":"error"`, et `error` au lieu de `response`.
//! 3. [`juger_reponses`] les juge contre le JSON de `--settings` que Kraken a passé : `get_settings` sans erreur, avec
//!    la source `flagSettings`, les réglages effectifs qui gardent la main (crochets et mémoire automatique coupés, le
//!    mur tel que déclaré, ses `excludedCommands` compris : absents au déclaré, ils le sont à l'effectif, et une autre
//!    source qui en ajoute est un écart) et le modèle déclaré ; `list_permission_rules` avec chaque règle déclarée, à
//!    la source `flagSettings` ; `get_hooks_listing` avec tous les crochets coupés ; `get_sandbox_dialog`, interne à
//!    Claude Code, qui dit le mur actif ou non, et dont le silence n'est pas un écart, sauf un mur exigé qu'il dit
//!    inactif.
//! 4. Sans écart, Kraken écrit la consigne ([`message`]), puis ferme l'entrée de la main. Au moindre écart, la main
//!    meurt sans sa consigne : `main : réglages non pris : <écarts>`, chaque écart en une ligne qui dit le déclaré
//!    contre le constaté, les écarts joints par « ; ».
//! 5. L'ouverture, `{"type":"system","subtype":"init",…}`, n'arrive qu'après la consigne, 0,4 s après :
//!    [`juger_ouverture`] y juge le mode `acceptEdits`, les outils fermés et les serveurs MCP, et en tire la version.
//!    Avant elle, une ligne illisible est un écart, et toute autre ligne s'ignore.
//! 6. Après l'ouverture, seule compte la ligne `{"type":"result",…}` : [`resultat`] en tire la sortie de la main, où se
//!    lit la ligne `PREUVE:`, ou son erreur.
//!
//! Ces formes sont celles de Claude Code 2.1.312 (Windows) et 2.1.316 (WSL), sauf la réponse en erreur, qu'aucune
//! main n'a rendue. Une forme qui change est un écart : la main meurt, et l'écart le dit.
//! Les bornes de temps, la mise à mort et ce que la main écrit sur stderr sont à `jouer`.

use std::collections::BTreeMap;
use std::str;

use crate::coeur::json::{self, Json};

/// Les quatre demandes de contrôle, dans l'ordre où Kraken les écrit. Chacune est le sous-type de sa demande et son id.
pub(crate) const DEMANDES: [&str; 4] =
    ["get_settings", "list_permission_rules", "get_hooks_listing", "get_sandbox_dialog"];

/// Les réglages du mur, que `get_settings` rend effectifs tels que déclarés, `excludedCommands` compris : les commandes
/// qui sortent du bac à sable, que Kraken ne déclare que pour la preuve prêtée sous un mur demandé. Absentes au
/// déclaré, elles le sont à l'effectif : une autre source qui en ajoute est un écart.
const MUR: [&str; 5] = [
    "sandbox.enabled",
    "sandbox.failIfUnavailable",
    "sandbox.allowUnsandboxedCommands",
    "sandbox.excludedCommands",
    "sandbox.network.strictAllowlist",
];

/// Le début de l'erreur d'une main tuée sur un écart, avant sa consigne ou à son ouverture.
const NON_PRIS: &str = "main : réglages non pris : ";

/// Ce que Kraken a passé à la main, contre quoi se juge ce qu'elle répond.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Attendu<'a> {
    /// Le JSON de `--settings`, à l'octet.
    pub(crate) reglages: &'a str,
    /// Le modèle de `--model` : un alias (`sonnet`) ou un nom complet (`claude-sonnet-5-5`).
    pub(crate) modele: &'a str,
    /// Les noms des serveurs du `mcpServers` de `--mcp-config`, `None` sans lui : un serveur de l'ouverture hors de ces
    /// noms est un écart, quel que soit son statut (`pending` compris) ; sans eux, `mcp_servers` est vide.
    pub(crate) mcp: Option<&'a [String]>,
}

/// Ce que Kraken a lu de la main, même quand il l'a tuée sur un écart : la version de Claude Code, tirée de son
/// ouverture, et son mur, tiré de `get_sandbox_dialog`. `None` : rien ne l'a dit. Constaté veut dire ce que Claude Code
/// a répondu, pas la tenue du mur.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Constat {
    pub(crate) version: Option<String>,
    pub(crate) mur_actif: Option<bool>,
    /// L'usage de la main : celui de sa ligne `result`, ou le cumul de ses lignes `assistant` si elle est tuée avant.
    pub(crate) usage: Usage,
    /// `apiKeySource` de l'ouverture : `none` est la connexion de l'abonnement, une autre valeur une clé.
    pub(crate) source_cle: Option<String>,
}

/// Les jetons d'une main, ou d'un de ses modèles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Jetons {
    pub(crate) entree: u64,
    pub(crate) sortie: u64,
    pub(crate) cache_ecrit: u64,
    pub(crate) cache_lu: u64,
    pub(crate) reflexion: u64,
}

/// L'usage d'un modèle : ses jetons, le coût que Claude Code annonce en micro-dollars, et sa base (`costBasis` :
/// `list`, `unknown`, ou absente).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UsageModele {
    pub(crate) modele: String,
    pub(crate) jetons: Jetons,
    pub(crate) micro_dollars: u64,
    pub(crate) base: Option<String>,
}

/// L'usage d'une main : ses totaux, puis le détail par modèle, dans l'ordre des noms. Des entiers et des mots, rien
/// d'autre. Vide quand Kraken n'a rien lu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    pub(crate) jetons: Jetons,
    pub(crate) micro_dollars: u64,
    pub(crate) duree_ms: u64,
    pub(crate) tours: u64,
    pub(crate) modeles: Vec<UsageModele>,
}

impl Usage {
    /// Vrai si l'usage n'a ni jeton, ni dépense, ni modèle (la durée et les tours n'y comptent pas).
    pub(crate) fn est_vide(&self) -> bool {
        self.jetons == Jetons::default() && self.micro_dollars == 0 && self.modeles.is_empty()
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Ligne {
    /// Une réponse de contrôle : l'id de sa demande, et son objet `response` (`subtype`, `request_id`, puis `response`
    /// ou `error`), que [`juger_reponses`] lit.
    Reponse(String, Json),
    /// L'ouverture, `{"type":"system","subtype":"init",…}`, entière.
    Ouverture(Json),
    /// Le résultat, `{"type":"result",…}`, entier.
    Resultat(Json),
    /// Une ligne `{"type":"assistant",…}`, entière : son `message.usage` se cumule ([`cumuler`]).
    Assistant(Json),
    /// Toute autre ligne lisible, que Kraken ignore.
    Autre,
}

pub(crate) fn demandes() -> String {
    DEMANDES
        .iter()
        .map(|id| format!(r#"{{"type":"control_request","request_id":"{id}","request":{{"subtype":"{id}"}}}}"#) + "\n")
        .collect()
}

/// La consigne en message de l'humain, sur une ligne finie par `\n`, la consigne écrite par [`json::chaine`].
pub(crate) fn message(consigne: &str) -> String {
    let contenu = json::chaine(consigne);
    format!(
        r#"{{"type":"user","message":{{"role":"user","content":{contenu}}},"parent_tool_use_id":null,"session_id":""}}"#
    ) + "\n"
}

/// Range une ligne de la sortie de la main, lue sans son `\n`. Illisible, en UTF-8 ou en JSON, elle est un écart, et
/// l'erreur rendue est entière : `jouer` la rend avant l'ouverture, et l'ignore après.
pub(crate) fn lire_ligne(octets: &[u8]) -> Result<Ligne, String> {
    let illisible = |raison: String| format!("{NON_PRIS}ligne illisible : {raison}");
    let texte =
        str::from_utf8(octets).map_err(|e| illisible(format!("UTF-8 invalide après l'octet {}", e.valid_up_to())))?;
    let ligne = json::document(texte).map_err(illisible)?;
    Ok(match (texte_de(&ligne, "type"), texte_de(&ligne, "subtype")) {
        (Some("control_response"), _) => match (texte_de(&ligne, "response.request_id"), champ(&ligne, "response")) {
            (Some(id), Some(reponse)) => Ligne::Reponse(id.into(), reponse.clone()),
            _ => Ligne::Autre,
        },
        (Some("system"), Some("init")) => Ligne::Ouverture(ligne),
        (Some("result"), _) => Ligne::Resultat(ligne),
        (Some("assistant"), _) => Ligne::Assistant(ligne),
        _ => Ligne::Autre,
    })
}

/// Juge les quatre réponses, rangées par l'id de leur demande, contre le `--settings` déclaré. Rend le mur constaté,
/// même sur un écart : le booléen `enabled` d'une réponse `success` à `get_sandbox_dialog`, `None` sinon. Puis le
/// jugement : `Ok`, ou l'erreur qui dit tous les écarts, chacun à la suite du nom de sa demande, dans l'ordre de
/// [`DEMANDES`] : `main : réglages non pris : get_settings : <écart> ; <écart> ; list_permission_rules : <écart>`.
pub(crate) fn juger_reponses(
    attendu: &Attendu,
    reponses: &BTreeMap<String, Json>,
) -> (Option<bool>, Result<(), String>) {
    let succes = |reponse: &Json| texte_de(reponse, "subtype") == Some("success");
    let dialogue = reponses.get("get_sandbox_dialog").filter(|reponse| succes(reponse));
    let mur_actif = match dialogue.and_then(|reponse| champ(reponse, "response.enabled")) {
        Some(Json::Booleen(actif)) => Some(*actif),
        _ => None,
    };
    let declare = match json::document(attendu.reglages) {
        Ok(declare) => declare,
        Err(raison) => return (mur_actif, Err(format!("{NON_PRIS}--settings déclaré illisible : {raison}"))),
    };
    let mut groupes = Vec::new();
    for id in DEMANDES {
        let ecarts = match reponses.get(id) {
            None => vec!["demandée, sans réponse".to_string()],
            Some(reponse) if !succes(reponse) => match id {
                // Interne à Claude Code, `get_sandbox_dialog` en erreur ne dit rien du mur : ce n'est pas un écart.
                "get_sandbox_dialog" => Vec::new(),
                _ => vec![rendue_en_erreur(reponse)],
            },
            Some(reponse) => {
                let corps = champ(reponse, "response");
                match id {
                    "get_settings" => reglages(corps, &declare, attendu.modele),
                    "list_permission_rules" => regles(corps, &declare),
                    "get_hooks_listing" => crochets(corps),
                    _ => mur(mur_actif, &declare),
                }
            }
        };
        if !ecarts.is_empty() {
            groupes.push(format!("{id} : {}", ecarts.join(" ; ")));
        }
    }
    (mur_actif, jugement(&groupes, NON_PRIS))
}

/// Juge l'ouverture contre ce que Kraken a passé : le mode `acceptEdits` exactement (`manual` et `default` s'y
/// affichent tous deux `default`), aucun nom nu de `deny` parmi `tools`, et `mcp_servers` vide sans `mcp`, ou, avec
/// lui, chaque serveur au nom d'une clé du `mcpServers` déclaré, quel que soit son statut. Rend la version constatée,
/// `claude_code_version`, même sur un écart. Puis le jugement : `Ok`, ou l'erreur qui dit tous les écarts,
/// `main : réglages non pris : à l'ouverture, <écart> ; <écart>`.
pub(crate) fn juger_ouverture(attendu: &Attendu, ouverture: &Json) -> (Option<String>, Result<(), String>) {
    let version = texte_de(ouverture, "claude_code_version").map(str::to_string);
    let mut ecarts = Vec::new();
    if texte_de(ouverture, "permissionMode") != Some("acceptEdits") {
        ecarts.push(format!("permissionMode {}, déclaré \"acceptEdits\"", montre(champ(ouverture, "permissionMode"))));
    }
    let declare = json::document(attendu.reglages);
    let fermes: Vec<&str> = match &declare {
        Ok(declare) => {
            textes(champ(declare, "permissions.deny")).into_iter().filter(|nom| !nom.contains('(')).collect()
        }
        Err(raison) => {
            ecarts.push(format!("--settings déclaré illisible : {raison}"));
            Vec::new()
        }
    };
    match champ(ouverture, "tools") {
        Some(Json::Liste(outils)) => {
            for nom in fermes {
                if outils.iter().any(|outil| matches!(outil, Json::Texte(vu) if vu == nom)) {
                    ecarts.push(format!("tools porte {nom:?}, déclaré en deny"));
                }
            }
        }
        autre => ecarts.push(format!("tools {}, déclaré une liste sans les noms de deny", montre(autre))),
    }
    let serveurs = champ(ouverture, "mcp_servers");
    match (attendu.mcp, serveurs) {
        (None, Some(Json::Liste(serveurs))) if serveurs.is_empty() => {}
        (None, _) => ecarts.push(format!("mcp_servers {}, déclaré []", montre(serveurs))),
        // Un serveur déclaré passe, quel que soit son statut : il peut être encore `pending`.
        (Some(noms), Some(Json::Liste(serveurs))) => {
            for nom in serveurs.iter().map(|serveur| champ(serveur, "name")) {
                if !matches!(nom, Some(Json::Texte(nom)) if noms.contains(nom)) {
                    ecarts.push(format!("mcp_servers.name {}, déclaré parmi {noms:?}", montre(nom)));
                }
            }
        }
        (Some(noms), autre) => {
            ecarts.push(format!("mcp_servers {}, déclaré une liste de serveurs parmi {noms:?}", montre(autre)));
        }
    }
    (version, jugement(&ecarts, &format!("{NON_PRIS}à l'ouverture, ")))
}

/// La sortie de la main : le texte `result` de sa ligne de résultat, vide s'il manque. Un résultat en erreur
/// (`is_error` vrai, ou un `subtype` autre que `success`) rend `main : résultat en erreur (<subtype>) : <result>`,
/// sans ` : <result>` s'il n'a pas de texte.
pub(crate) fn resultat(ligne: &Json) -> Result<String, String> {
    let sortie = texte_de(ligne, "result").unwrap_or_default();
    let en_erreur = champ(ligne, "is_error") == Some(&Json::Booleen(true));
    if texte_de(ligne, "subtype") == Some("success") && !en_erreur {
        return Ok(sortie.to_string());
    }
    let subtype = texte_de(ligne, "subtype").map_or_else(|| montre(champ(ligne, "subtype")), str::to_string);
    let texte = if sortie.is_empty() { String::new() } else { format!(" : {sortie}") };
    Err(format!("main : résultat en erreur ({subtype}){texte}"))
}

/// `apiKeySource` de l'ouverture : `none` est la connexion de l'abonnement, une autre valeur une clé.
pub(crate) fn source_cle(ouverture: &Json) -> Option<String> {
    texte_de(ouverture, "apiKeySource").map(str::to_string)
}

/// L'usage d'une ligne `result` : les totaux de `usage` et `total_cost_usd`, `duration_ms`, `num_turns`, puis le détail
/// de `modelUsage`, dans l'ordre des noms. Un champ absent ou mal typé vaut zéro, ou « absent » pour une base : jamais
/// une erreur, et jamais un effet sur le verdict.
pub(crate) fn usage_du_resultat(ligne: &Json) -> Usage {
    let modeles = match champ(ligne, "modelUsage") {
        Some(Json::Objet(modeles)) => modeles
            .iter()
            .map(|(nom, modele)| UsageModele {
                modele: nom.clone(),
                jetons: Jetons {
                    entree: entier(modele, "inputTokens"),
                    sortie: entier(modele, "outputTokens"),
                    cache_ecrit: entier(modele, "cacheCreationInputTokens"),
                    cache_lu: entier(modele, "cacheReadInputTokens"),
                    reflexion: entier(modele, "thinkingTokens"),
                },
                micro_dollars: micro_dollars(modele, "costUSD"),
                base: texte_de(modele, "costBasis").map(str::to_string),
            })
            .collect(),
        _ => Vec::new(),
    };
    Usage {
        jetons: Jetons {
            entree: entier(ligne, "usage.input_tokens"),
            sortie: entier(ligne, "usage.output_tokens"),
            cache_ecrit: entier(ligne, "usage.cache_creation_input_tokens"),
            cache_lu: entier(ligne, "usage.cache_read_input_tokens"),
            reflexion: entier(ligne, "usage.output_tokens_details.thinking_tokens"),
        },
        micro_dollars: micro_dollars(ligne, "total_cost_usd"),
        duree_ms: entier(ligne, "duration_ms"),
        tours: entier(ligne, "num_turns"),
        modeles,
    }
}

/// Les messages de l'API déjà lus d'une main, par `message.id` : leur modèle et leurs jetons. Claude Code écrit un même
/// message en plusieurs lignes `assistant`, une par bloc de contenu, qui portent le même id et le même usage.
pub(crate) type Messages = BTreeMap<String, (Option<String>, Jetons)>;

/// Reprend dans `cumul` l'usage d'une ligne `assistant` : les quatre compteurs de `message.usage`, aux totaux et, sous
/// le nom `message.model`, au détail (créé à sa place dans l'ordre des noms). Le dernier usage de chaque `message.id`
/// compte seul, et `cumul` est la somme de ces usages : un message écrit en trois lignes ne compte qu'une fois. Une
/// ligne sans `message.id` compte pour elle seule. Une ligne sans `message.usage` n'ajoute rien.
pub(crate) fn cumuler(cumul: &mut Usage, vus: &mut Messages, ligne: &Json) {
    if !matches!(champ(ligne, "message.usage"), Some(Json::Objet(_))) {
        return;
    }
    let jetons = Jetons {
        entree: entier(ligne, "message.usage.input_tokens"),
        sortie: entier(ligne, "message.usage.output_tokens"),
        cache_ecrit: entier(ligne, "message.usage.cache_creation_input_tokens"),
        cache_lu: entier(ligne, "message.usage.cache_read_input_tokens"),
        reflexion: 0,
    };
    let modele = texte_de(ligne, "message.model").map(str::to_string);
    // Sans id, une clé que nul id ne porte : un octet nul ne se lit pas dans un id.
    let cle = texte_de(ligne, "message.id").map_or_else(|| format!("\0{}", vus.len()), str::to_string);
    vus.insert(cle, (modele, jetons));
    cumul.jetons = Jetons::default();
    cumul.modeles.clear();
    for (modele, jetons) in vus.values() {
        ajouter(&mut cumul.jetons, jetons);
        if let Some(nom) = modele {
            let place = match cumul.modeles.binary_search_by(|vu| vu.modele.as_str().cmp(nom)) {
                Ok(place) => place,
                Err(place) => {
                    cumul.modeles.insert(place, UsageModele { modele: nom.clone(), ..UsageModele::default() });
                    place
                }
            };
            ajouter(&mut cumul.modeles[place].jetons, jetons);
        }
    }
}

fn ajouter(total: &mut Jetons, de: &Jetons) {
    total.entree = total.entree.saturating_add(de.entree);
    total.sortie = total.sortie.saturating_add(de.sortie);
    total.cache_ecrit = total.cache_ecrit.saturating_add(de.cache_ecrit);
    total.cache_lu = total.cache_lu.saturating_add(de.cache_lu);
    total.reflexion = total.reflexion.saturating_add(de.reflexion);
}

/// Le nombre à ce chemin, `None` s'il manque, n'est pas un nombre, n'est pas fini ou est négatif.
fn flottant(json: &Json, chemin: &str) -> Option<f64> {
    match champ(json, chemin) {
        Some(Json::Nombre(nombre)) => nombre.parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0),
        _ => None,
    }
}

/// Un entier à ce chemin, arrondi au plus proche ; zéro s'il manque ou est mal typé.
fn entier(json: &Json, chemin: &str) -> u64 {
    flottant(json, chemin).map_or(0, |n| n.round() as u64)
}

/// Un coût en dollars à ce chemin, en micro-dollars arrondis à l'entier le plus proche ; zéro s'il manque.
fn micro_dollars(json: &Json, chemin: &str) -> u64 {
    flottant(json, chemin).map_or(0, |n| (n * 1_000_000.0).round() as u64)
}

/// Les écarts de `get_settings` : une source `flagSettings`, aucune erreur, les réglages effectifs qui gardent la main,
/// et le modèle appliqué, égal au déclaré ou qui en est le nom complet (`sonnet` donne `claude-sonnet-5-5`).
fn reglages(corps: Option<&Json>, declare: &Json, modele: &str) -> Vec<String> {
    let vu = |chemin: &str| corps.and_then(|corps| champ(corps, chemin));
    let mut ecarts = Vec::new();
    let flag = |source: &Json| texte_de(source, "source") == Some("flagSettings");
    match vu("sources") {
        Some(Json::Liste(sources)) if sources.iter().any(flag) => {}
        Some(Json::Liste(sources)) => {
            let noms: Vec<String> = sources.iter().map(|source| montre(champ(source, "source"))).collect();
            ecarts.push(format!("sources [{}], déclaré \"flagSettings\" parmi elles", noms.join(", ")));
        }
        autre => ecarts.push(format!("sources {}, déclaré \"flagSettings\" parmi elles", montre(autre))),
    }
    match vu("errors") {
        None => {}
        Some(Json::Liste(erreurs)) => {
            for erreur in erreurs {
                let de = |cle| montre(champ(erreur, cle));
                ecarts.push(format!("erreur {} à {} : {}, déclaré sans erreur", de("file"), de("path"), de("message")));
            }
        }
        autre => ecarts.push(format!("errors {}, déclaré []", montre(autre))),
    }
    // Ce qui garde la main : les crochets et la mémoire automatique coupés, le mur tel que déclaré.
    let mut egal = |chemin: &str, declare: Option<&Json>| {
        let effectif = vu(&format!("effective.{chemin}"));
        if effectif != declare {
            ecarts.push(format!("effective.{chemin} {}, déclaré {}", montre(effectif), montre(declare)));
        }
    };
    egal("disableAllHooks", Some(&Json::Booleen(true)));
    egal("autoMemoryEnabled", Some(&Json::Booleen(false)));
    for chemin in MUR {
        egal(chemin, champ(declare, chemin));
    }
    let applique = vu("applied.model");
    let pris =
        matches!(applique, Some(Json::Texte(nom)) if nom == modele || nom.starts_with(&format!("claude-{modele}-")));
    if !pris {
        ecarts.push(format!("applied.model {}, déclaré {modele:?}", montre(applique)));
    }
    ecarts
}

/// Les écarts de `list_permission_rules` : chaque règle déclarée, d'`allow` puis de `deny`, y est sous la même chaîne,
/// avec ce comportement, à la source `flagSettings`.
fn regles(corps: Option<&Json>, declare: &Json) -> Vec<String> {
    let lues = corps.and_then(|corps| champ(corps, "state.rules"));
    let Some(Json::Liste(vues)) = lues else {
        return vec![format!("state.rules {}, déclaré une liste de règles", montre(lues))];
    };
    let mut ecarts = Vec::new();
    for comportement in ["allow", "deny"] {
        for regle in textes(champ(declare, &format!("permissions.{comportement}"))) {
            let memes: Vec<&Json> = vues.iter().filter(|vue| texte_de(vue, "rule") == Some(regle)).collect();
            let prise = |vue: &&Json| {
                texte_de(vue, "behavior") == Some(comportement) && texte_de(vue, "source") == Some("flagSettings")
            };
            if memes.iter().any(prise) {
                continue;
            }
            let vu = if memes.is_empty() {
                "absente".to_string()
            } else {
                let ou: Vec<String> = memes
                    .iter()
                    .map(|vue| format!("en {} à {}", montre(champ(vue, "behavior")), montre(champ(vue, "source"))))
                    .collect();
                format!("vue {}", ou.join(", "))
            };
            ecarts.push(format!("{comportement} {regle:?} déclarée, {vu}"));
        }
    }
    ecarts
}

/// L'écart de `get_hooks_listing` : les crochets ne sont pas tous coupés.
fn crochets(corps: Option<&Json>) -> Vec<String> {
    let coupes = corps.and_then(|corps| champ(corps, "policy.allDisabled"));
    if coupes == Some(&Json::Booleen(true)) {
        Vec::new()
    } else {
        vec![format!("policy.allDisabled {}, déclaré true", montre(coupes))]
    }
}

/// L'écart de `get_sandbox_dialog` : un mur exigé (`failIfUnavailable` vrai au déclaré) que Claude Code dit inactif.
fn mur(mur_actif: Option<bool>, declare: &Json) -> Vec<String> {
    let exige = champ(declare, "sandbox.failIfUnavailable") == Some(&Json::Booleen(true));
    if exige && mur_actif == Some(false) {
        vec!["enabled false, déclaré exigé (failIfUnavailable true)".to_string()]
    } else {
        Vec::new()
    }
}

/// L'écart d'une réponse qui n'est pas `success` : son `subtype`, puis son `error` s'il en a un.
fn rendue_en_erreur(reponse: &Json) -> String {
    let subtype = montre(champ(reponse, "subtype"));
    match champ(reponse, "error") {
        Some(erreur) => format!("demandée, rendue en {subtype} : {}", montre(Some(erreur))),
        None => format!("demandée, rendue en {subtype}"),
    }
}

/// `Ok` sans écart ; sinon l'erreur qui les dit tous après `debut`, joints par « ; ».
fn jugement(ecarts: &[String], debut: &str) -> Result<(), String> {
    if ecarts.is_empty() { Ok(()) } else { Err(format!("{debut}{}", ecarts.join(" ; "))) }
}

/// La valeur au bout de ce chemin de clés, séparées par des points, à travers des objets.
fn champ<'a>(json: &'a Json, chemin: &str) -> Option<&'a Json> {
    chemin.split('.').try_fold(json, |json, cle| match json {
        Json::Objet(objet) => objet.get(cle),
        _ => None,
    })
}

fn texte_de<'a>(json: &'a Json, chemin: &str) -> Option<&'a str> {
    match champ(json, chemin) {
        Some(Json::Texte(texte)) => Some(texte.as_str()),
        _ => None,
    }
}

/// Les textes d'une liste, ses autres valeurs sautées ; aucun si ce n'est pas une liste.
fn textes(liste: Option<&Json>) -> Vec<&str> {
    let Some(Json::Liste(liste)) = liste else { return Vec::new() };
    liste
        .iter()
        .filter_map(|valeur| match valeur {
            Json::Texte(texte) => Some(texte.as_str()),
            _ => None,
        })
        .collect()
}

/// Ce qu'une valeur lue montre dans un écart, sur une ligne : un texte entre guillemets, échappé comme en Rust ; un
/// nombre tel qu'écrit ; `true`, `false` ou `null` ; `[]`, `{}`, la taille d'une liste, `un objet` ; `absent` faute
/// de valeur.
fn montre(valeur: Option<&Json>) -> String {
    match valeur {
        None => "absent".into(),
        Some(Json::Nul) => "null".into(),
        Some(Json::Booleen(b)) => b.to_string(),
        Some(Json::Nombre(nombre)) => nombre.clone(),
        Some(Json::Texte(texte)) => format!("{texte:?}"),
        Some(Json::Liste(liste)) => match liste.len() {
            0 => "[]".into(),
            1 => "[1 élément]".into(),
            n => format!("[{n} éléments]"),
        },
        Some(Json::Objet(objet)) if objet.is_empty() => "{}".into(),
        Some(Json::Objet(_)) => "un objet".into(),
    }
}

#[cfg(test)]
mod tests;
