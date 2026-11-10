//! Le rendu de `lydite etat` : la chaîne des vagues montrées, et le code de sortie qu'elle vaut.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use crate::branchements::dossier::{lire, lire_avec_sceaux, sonder};
use crate::coeur::erreur::Refus;
use crate::coeur::fil::{
    self, Evenement, Foi, Lecture, Main, Note, SceauLu, Vague, Verdict, correction_de, etat, relecture_de,
};
use crate::coeur::texte;
use crate::conduite::publication::recu_de;
use crate::conduite::tentative::IMPOSSIBLE;

/// `lydite etat` dans ce dossier de Kraken : les écarts du fil sur stderr, un refus chacun, puis la chaîne de la
/// dernière vague sur stdout (voir `montrees`), vue à l'heure de la lecture. Un fil absent est un fil vide. Le premier
/// sceau (`vague.sceau`) qui ne tient plus se nomme sur stderr (sa vague, ses octets), et la sortie est 2, comme pour
/// un fil qu'on ne lit pas, sans l'état. Le sceau montre une ligne changée ou retirée avant lui ; il ne montre ni une
/// ligne ajoutée après lui, ni un fil réécrit avec des sceaux refaits.
pub(crate) fn etat_dans(dossier: &Path) -> Result<ExitCode, Refus> {
    rendre_dans(dossier, false)
}

/// Le temps entre deux sondes du dossier de `lydite attendre`.
pub(crate) const PAUSE_DE_L_ATTENTE: Duration = Duration::from_secs(2);

/// Ce que `lydite attendre` laisse à un lancement pour ouvrir sa vague quand le dossier n'est pas encore tenu.
pub(crate) const DELAI_D_OUVERTURE: Duration = Duration::from_secs(60);

/// `lydite attendre` : attend tant qu'un lancement tient le dossier ([`sonder`]), puis dit la dernière vague comme
/// `lydite etat`. Le code est celui de `etat` (0 menée à bout et verte, 1 sinon), avec un 1 de plus pour une vague
/// finie sans son sceau de fin (un lancement tué), et 2 pour un fil illisible ou un sceau rompu. Si le dossier n'est
/// pas tenu quand elle part, elle laisse `delai` (en pauses de `pause`) à une vague pour s'ouvrir, au fil ou par le
/// dossier tenu ; sans l'un ni l'autre, elle le dit sur stderr et sort en 2. Tant que le dossier est tenu, elle ne
/// compte rien : elle attend la fin.
pub(crate) fn attendre_dans(dossier: &Path, delai: Duration, pause: Duration) -> Result<ExitCode, Refus> {
    let pauses = delai.as_millis() / pause.as_millis().max(1);
    attendre_avec(dossier, u32::try_from(pauses).unwrap_or(u32::MAX), &mut || std::thread::sleep(pause))
}

/// [`attendre_dans`] avec `pauses` sondes de grâce et la `pause` entre deux sondes en paramètre, pour que les tests
/// n'attendent pas en temps réel.
pub(crate) fn attendre_avec(dossier: &Path, pauses: u32, pause: &mut dyn FnMut()) -> Result<ExitCode, Refus> {
    let ouvertes = |dossier: &Path| -> Result<Vec<String>, Refus> {
        let notes = lire(dossier)?.notes;
        Ok(notes
            .into_iter()
            .filter_map(|note| match note.evenement {
                Evenement::Vague { vague, .. } => Some(vague),
                _ => None,
            })
            .collect())
    };
    let avant = ouvertes(dossier)?;
    let (mut vu_tenu, mut restantes) = (false, pauses);
    loop {
        if sonder(dossier)? {
            vu_tenu = true;
            pause();
            continue;
        }
        if vu_tenu || ouvertes(dossier)?.iter().any(|id| !avant.contains(id)) {
            return rendre_dans(dossier, true);
        }
        if restantes == 0 {
            eprintln!(
                "aucune vague neuve au fil, et aucun lancement ne tient le dossier {} : il n'y a rien à attendre",
                dossier.display()
            );
            return Ok(ExitCode::from(2));
        }
        restantes -= 1;
        pause();
    }
}

/// `etat_dans`, et avec `sceau_exige` la règle de `attendre` : la dernière vague montrée doit porter son sceau de fin.
fn rendre_dans(dossier: &Path, sceau_exige: bool) -> Result<ExitCode, Refus> {
    let (Lecture { notes, ecarts }, issues, sceaux, rompu) = lire_avec_sceaux(dossier)?;
    for (_, refus) in &ecarts {
        eprintln!("{refus}");
    }
    // Un sceau qui ne correspond plus : une ligne d'avant lui a changé ou disparu. Le fil ne se lit plus comme sûr.
    if let Some(sceau) = rompu {
        eprintln!(
            "le fil ne correspond plus au sceau de la vague {}, noté à {} octets : une ligne d'avant lui a changé ou \
             disparu",
            sceau.vague, sceau.octets
        );
        return Ok(ExitCode::from(2));
    }
    let evenements: Vec<Evenement> = notes.iter().map(|note| note.evenement.clone()).collect();
    let mut vagues = etat(&evenements);
    fil::porter_issues(&mut vagues, &issues);
    let maintenant = fil::instant(fil::maintenant());
    println!("{}", if vagues.is_empty() { aucune_vague(dossier) } else { rendu_avec(&vagues, &notes, &maintenant) });
    let montrees = montrees(&vagues);
    if sceau_exige && sans_sceau_de_fin(&montrees, &sceaux) {
        eprintln!(
            "la dernière vague n'a pas son sceau de fin (vague.sceau) : le lancement a fini sans l'écrire, tué sans \
             doute"
        );
        return Ok(ExitCode::FAILURE);
    }
    Ok(if toutes_menees_a_bout(&montrees, &vagues) { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Un lancement scelle la vague qu'il mène à chacune de ses sorties : aucune des vagues montrées n'a son sceau, c'est
/// qu'il a été tué (ou que le sceau ne s'est pas écrit). Un découpage seul (`--observer`) ne scelle rien et n'en doit
/// pas : la dernière vague montrée qui n'est qu'un découpage pris n'est pas dite sans sceau.
fn sans_sceau_de_fin(montrees: &[&Vague], sceaux: &[SceauLu]) -> bool {
    let Some(derniere) = montrees.last() else {
        return false;
    };
    derniere.plan_pris.is_none() && !montrees.iter().any(|vague| sceaux.iter().any(|lu| lu.sceau.vague == vague.id))
}

/// Ce que `lydite etat` dit sans vague : « aucune vague », et l'ancien fil à tabulations (version 1) s'il reste seul,
/// qu'il ne lit pas.
fn aucune_vague(dossier: &Path) -> String {
    let m1 = dossier.join("fil");
    if m1.exists() {
        format!("aucune vague au fil v2 ; le fil de M1 n'est plus lu : {}", m1.display())
    } else {
        "aucune vague".into()
    }
}

fn vague_de<'a>(vagues: &'a [Vague], id: &str) -> Option<&'a Vague> {
    vagues.iter().find(|vague| vague.id == id)
}

fn relecteur_de<'a>(vagues: &'a [Vague], id: &str) -> Option<&'a Vague> {
    vagues.iter().rev().find(|vague| vague.relit.as_deref() == Some(id))
}

/// La vague que `lydite etat` montre à la place de `vague` : une relecture, prise ou classée, ou une correction se
/// montre dans la vague qu'elle relit ou corrige. Le parcours suit les liens du fil, d'au plus autant de pas que le fil
/// a de vagues : un fil dont les liens bouclent ne fait pas boucler Kraken.
fn principale<'a>(vagues: &'a [Vague], vague: &'a Vague) -> &'a Vague {
    let mut courante = vague;
    for _ in vagues {
        let relue = courante.relecture.as_ref().map(|relecture| relecture.relue.as_str());
        let visee = courante.corrige.as_deref().or(relue).or(courante.relit.as_deref());
        match visee.and_then(|id| vague_de(vagues, id)) {
            Some(autre) => courante = autre,
            None => break,
        }
    }
    courante
}

/// Les vagues que `lydite etat` montre, en partant de la dernière du fil et en remontant ses liens : la dernière,
/// ramenée à sa vague principale (`principale`), puis celle qu'elle relance. Le découpage, la relecture et la
/// correction d'une vague se disent dans son texte (`bloc`), non comme des vagues de plus. Une vague déjà montrée
/// n'est pas reprise : des liens qui bouclent s'arrêtent là.
fn montrees(vagues: &[Vague]) -> Vec<&Vague> {
    let mut chaine: Vec<&Vague> = Vec::new();
    let mut courante = vagues.last();
    while let Some(vague) = courante.map(|vague| principale(vagues, vague)) {
        if chaine.iter().any(|vue| vue.id == vague.id) {
            break;
        }
        chaine.push(vague);
        courante = vague.relance_de.as_deref().and_then(|id| vague_de(vagues, id));
    }
    chaine.reverse();
    chaine
}

/// Le texte de `lydite etat` : le bloc de chaque vague montrée (voir `montrees`), séparés par une ligne vide, ou
/// « aucune vague ».
#[cfg(test)]
pub(crate) fn rendu(vagues: &[Vague], maintenant: &str) -> String {
    rendu_avec(vagues, &[], maintenant)
}

/// Le texte de `lydite etat` avec le reçu de chaque vague montrée, lu dans les `notes` du fil (voir `recu_de`).
pub(crate) fn rendu_avec(vagues: &[Vague], notes: &[Note], maintenant: &str) -> String {
    let blocs: Vec<String> = montrees(vagues).into_iter().map(|vague| bloc(vague, vagues, notes, maintenant)).collect();
    if blocs.is_empty() { "aucune vague".into() } else { blocs.join("\n\n") }
}

/// Le texte d'une vague : son objectif et sa branche, puis ses mains, sa relecture, sa correction, sa forge et enfin
/// son reçu (`ligne_du_recu`).
/// Un découpage seul, observé ou dont aucune vague ne joue le plan, ne dit que sa propre ligne.
fn bloc(vague: &Vague, vagues: &[Vague], notes: &[Note], maintenant: &str) -> String {
    if let Some(taches) = vague.plan_pris {
        let ligne = ligne_du_decoupage(vague, taches, maintenant);
        return std::iter::once(ligne).chain([ligne_du_recu(vagues, notes, &vague.id)]).collect::<Vec<_>>().join("\n");
    }
    let decoupage = vague.decoupage.iter().map(|id| match vague_de(vagues, id) {
        Some(d) => match d.plan_pris {
            Some(taches) => ligne_du_decoupage(d, taches, maintenant),
            None => format!("Découpage : {id} · sans plan au fil"),
        },
        None => format!("Découpage : {id} · absent du fil"),
    });
    let entete = vague.assemblage.iter().map(|(branche, preuve)| {
        format!("branche : {} · preuve : {}", texte::en_ligne(branche), texte::en_ligne(preuve))
    });
    let relecture = match (relecture_de(vagues, &vague.id), relecteur_de(vagues, &vague.id)) {
        (Some(relecture), _) => {
            Some(format!("Relecture : {} remarques, dont {} bloquantes", relecture.n, relecture.bloquantes))
        }
        (None, Some(relecteur)) => Some(ligne_du_relecteur(relecteur, maintenant)),
        (None, None) => None,
    };
    let correction = correction_de(vagues, &vague.id).into_iter().flat_map(|correction| {
        lignes_des_mains(correction, maintenant).into_iter().map(|ligne| format!("Correction : {ligne}"))
    });
    let forge = vague.forge.iter().flat_map(|forge| {
        let ci = match forge.ci {
            Some(fil::Couleur::Vert) => "CI verte (constatée)",
            Some(fil::Couleur::Rouge) => "CI rouge (constatée)",
            None => "CI en attente",
        };
        let pr = forge.pr.iter().map(move |pr| format!("PR {} · {} · {ci}", pr.numero, texte::en_ligne(&pr.url)));
        let refus = forge.refus.iter().map(|clair| texte::en_ligne(clair.lines().next().unwrap_or_default()));
        pr.chain(refus)
    });
    // L'objectif d'une vague née d'une issue est `Issue #<n>` ; sans ouverture au fil, son `vague.issue` le redit.
    let objectif = match vague.issue {
        Some(numero) if vague.objectif.is_empty() => format!("Issue #{numero}"),
        _ => texte::en_lignes(&vague.objectif),
    };
    let toutes = decoupage.chain(std::iter::once(objectif)).chain(entete);
    let toutes = toutes.chain(lignes_des_mains(vague, maintenant)).chain(relecture).chain(correction).chain(forge);
    toutes.chain([ligne_du_recu(vagues, notes, &vague.id)]).collect::<Vec<_>>().join("\n")
}

/// La ligne de reçu de la vague `id` (`Recu::ligne`) : elle compte la vague, son découpage, ses relectures, ses
/// corrections et ses relances, de proche en proche, même quand une vague liée est montrée à part. Sans usage au fil,
/// le coût y est « inconnu ».
fn ligne_du_recu(vagues: &[Vague], notes: &[Note], id: &str) -> String {
    let recu = recu_de(vagues, notes, id);
    // Les réglages de mesure, quand il y en a, se disent sur une ligne à part sous le coût.
    std::iter::once(recu.ligne()).chain(recu.ligne_de_mesure()).collect::<Vec<_>>().join("\n")
}

/// La ligne d'un relecteur que le fil lie à une vague, sans relecture prise : `Relecture : main-<id>-1 · classée (7016)
/// : <dernière ligne>` si sa main est classée, quelle que soit la foi, sinon `Relecture : main-<id>-1 · <état>`.
fn ligne_du_relecteur(relecteur: &Vague, maintenant: &str) -> String {
    let main = relecteur.mains.get(&1);
    let etat = match main {
        Some(main) if matches!(main.verdict, Some((Verdict::Classee, _))) => {
            format!("classée (7016) : {}", derniere_ligne(main.texte()))
        }
        _ => etat_de(main, None, maintenant),
    };
    format!("Relecture : main-{}-1 · {etat}", relecteur.id)
}

/// La ligne d'un découpage qui a pris un plan de `taches` tâches : `Découpage : <k> tâches · <main> · prise
/// (constatée)`. Une prise que Kraken n'a pas rejouée porte son mot, comme dans une ligne de main.
fn ligne_du_decoupage(decoupage: &Vague, taches: usize, maintenant: &str) -> String {
    let main = decoupage.mains.get(&1);
    let etat = match main.and_then(|main| main.verdict) {
        Some((Verdict::Prise, Foi::Constate)) => "prise (constatée)".to_string(),
        _ => etat_de(main, None, maintenant),
    };
    format!("Découpage : {taches} tâches · main-{}-1 · {etat}", decoupage.id)
}

/// Les lignes des mains d'une vague, de 1 à n (et au-delà si le fil en nomme davantage) : `main-<vague>-<n>`, l'id de
/// sa tâche si la vague a un plan, son état (`etat_de`), puis le mot de son mur et celui de son fournisseur.
fn lignes_des_mains(vague: &Vague, maintenant: &str) -> Vec<String> {
    let plus_grande = vague.mains.last_key_value().map_or(0, |(n, _)| *n);
    // Une vague à plan : les ids de ses tâches assemblées, de quoi dire ce qu'une tâche attend encore.
    let assemblees: Option<std::collections::HashSet<&str>> = vague.assemblage.is_some().then(|| {
        let taches = vague.mains.values().filter(|main| matches!(main.issue, Some(fil::Issue::Assemblee)));
        taches.filter_map(|main| main.tache.as_ref()).map(|(id, _)| id.as_str()).collect()
    });
    let ligne = |n: usize| {
        let main = vague.mains.get(&n);
        let id = main.and_then(|main| main.tache.as_ref()).map_or_else(String::new, |(id, _)| format!(" {id}"));
        let etat = etat_de(main, assemblees.as_ref(), maintenant);
        format!("main-{}-{n}{id} : {etat}{}{}", vague.id, mot_du_mur(main), mot_du_fournisseur(main))
    };
    (1..=vague.n.max(plus_grande)).map(ligne).collect()
}

/// Le mot du mur de la dernière tentative d'une main, précédé de « · » pour finir sa ligne ; rien sans lancement ni
/// constat. Le constat qui dit le mur actif ou non l'emporte : « constaté » veut dire ce que Claude Code a répondu,
/// pas la tenue du mur. À défaut, le lancement dit ce que le profil demande : « déclaré ». Rien ne se lit de la
/// plateforme.
fn mot_du_mur(main: Option<&Main>) -> String {
    let Some((_, tentative)) = main.and_then(|main| main.tentatives.last_key_value()) else {
        return String::new();
    };
    let mot = match (tentative.mur_actif, tentative.mur) {
        (Some(true), _) => "mur actif (constaté)",
        (Some(false), _) => "mur inactif (constaté)",
        (None, Some(fil::Mur::Exige)) => "mur exigé (déclaré)",
        (None, Some(fil::Mur::Demande)) => "mur demandé, non exigé (déclaré)",
        (None, None) => return String::new(),
    };
    format!(" · {mot}")
}

/// Le fournisseur de la dernière tentative d'une main, précédé de « · » pour finir sa ligne ; rien sans
/// `main.fournisseur`. Il est toujours déclaré : le profil nomme le serveur où la main parle, et Kraken n'a pas lu ce
/// qu'il répond. Une main déclarée `sonnet` sur un autre fournisseur ne passe donc pas pour du Claude.
fn mot_du_fournisseur(main: Option<&Main>) -> String {
    let derniere = main.and_then(|main| main.tentatives.last_key_value());
    let fournisseur = derniere.and_then(|(_, tentative)| tentative.fournisseur.as_deref());
    fournisseur.map_or_else(String::new, |hote| format!(" · fournisseur {} (déclaré)", texte::en_ligne(hote)))
}

/// Ce qu'une main affiche, dans cet ordre de préséance : « assemblée » ; son refus, par la première ligne de
/// `refus_en_clair` ; son verdict, « prise, en attente d'assemblage » dans une vague à plan ; « sans nouvelles depuis
/// <échéance> (déduit) », si sa dernière tentative n'a pas fini et que son échéance est passée à `maintenant` ; « en
/// cours » avec sa tentative ; « en attente », de ses dépendances non encore assemblées dans une vague à plan. Une
/// classée affiche la dernière ligne de son erreur, et un verdict déclaré porte son mot.
fn etat_de(main: Option<&Main>, assemblees: Option<&std::collections::HashSet<&str>>, maintenant: &str) -> String {
    let Some(main) = main else {
        return "en attente".into();
    };
    let declaree = |foi| if foi == Foi::Declare { " déclarée, non prouvée" } else { "" };
    match (&main.issue, main.verdict) {
        (Some(fil::Issue::Assemblee), _) if assemblees.is_some() => "assemblée".into(),
        (Some(fil::Issue::Refusee { code, regle, preuve, correction }), _) => {
            let clair = fil::refus_en_clair(*code, regle, preuve, correction);
            texte::en_ligne(clair.lines().next().unwrap_or_default())
        }
        (_, Some((Verdict::Impossible, _))) => format!("{IMPOSSIBLE}{}", derniere_ligne(main.texte())),
        (_, Some((Verdict::Classee, foi))) => format!("classée{} : {}", declaree(foi), derniere_ligne(main.texte())),
        (_, Some((Verdict::Prise, foi))) => {
            let attente = if assemblees.is_some() { ", en attente d'assemblage" } else { "" };
            format!("prise{}{attente}", declaree(foi))
        }
        (_, None) => match main.tentatives.last_key_value() {
            // Sans fin à son échéance, une main n'est pas dite expirée : Kraken a pu s'arrêter, et n'en sait rien.
            Some((tentative, lue)) => match main.echeances.get(tentative) {
                Some(echeance) if lue.couleur.is_none() && echeance.as_str() <= maintenant => {
                    format!("sans nouvelles depuis {echeance} (déduit)")
                }
                _ => format!("en cours, tentative {tentative}"),
            },
            None => {
                let depend = main.tache.iter().flat_map(|(_, depend)| depend);
                let manque: Vec<&str> =
                    depend.map(String::as_str).filter(|id| assemblees.is_some_and(|a| !a.contains(id))).collect();
                if manque.is_empty() { "en attente".into() } else { format!("en attente de {}", manque.join(", ")) }
            }
        },
    }
}

/// La dernière ligne non vide d'un texte venu d'une main, ses contrôles écrits (`coeur::texte`).
fn derniere_ligne(brut: &str) -> String {
    texte::en_ligne(brut.lines().map(str::trim).rfind(|ligne| !ligne.is_empty()).unwrap_or_default())
}

/// Toutes les mains de la vague ont pris, et chaque prise est constatée : la vague en compte n, de 1 à n, et chacune a
/// pris sur la vérification que Kraken a rejouée. Une prise déclarée n'y compte pas.
fn toutes_prises(vague: &Vague) -> bool {
    let prise = |n: usize| vague.mains.get(&n).and_then(|main| main.verdict) == Some((Verdict::Prise, Foi::Constate));
    vague.n > 0 && vague.mains.len() == vague.n && (1..=vague.n).all(prise)
}

/// Toutes les tâches de la vague à plan sont assemblées : la vague en compte n, de 1 à n, et l'issue de chacune est
/// l'assemblage que Kraken a noté (`vague.fusion`). Une prise n'y compte pas : elle attend encore son assemblage.
fn toutes_assemblees(vague: &Vague) -> bool {
    let assemblee =
        |n: usize| vague.mains.get(&n).is_some_and(|main| matches!(main.issue, Some(fil::Issue::Assemblee)));
    vague.n > 0 && vague.mains.len() == vague.n && (1..=vague.n).all(assemblee)
}

/// Ce que le code 0 de `lydite etat` demande de la vague : toutes ses tâches assemblées si elle a un plan ; sinon, pour
/// des copies, toutes ses mains prises et constatées.
fn menee_a_bout(vague: &Vague) -> bool {
    if vague.assemblage.is_some() { toutes_assemblees(vague) } else { toutes_prises(vague) }
}

/// Ce que le code 0 de `lydite etat` demande, en plus de `menee_a_bout`, d'une vague qui joue un plan découpé ou
/// qu'une relecture a relue : le découpage qu'elle joue a un plan au fil, et sa main est prise et constatée ; une
/// correction qu'elle a est assemblée ; un relecteur qui la relit a rendu sa relecture ; et ses remarques bloquantes,
/// s'il y en a, ont leur correction. Une vague sans lien de découpage ni de relecture n'a rien de plus à faire.
fn satellites_a_bout(vague: &Vague, vagues: &[Vague]) -> bool {
    let pris = |d: &Vague| d.plan_pris.is_some() && toutes_prises(d);
    let decoupage_pris = vague.decoupage.as_deref().is_none_or(|id| vague_de(vagues, id).is_some_and(pris));
    let bloquantes = relecture_de(vagues, &vague.id).map_or(0, |relecture| relecture.bloquantes);
    let corrigee = match correction_de(vagues, &vague.id) {
        Some(correction) => toutes_assemblees(correction),
        None => bloquantes == 0,
    };
    let relue = relecture_de(vagues, &vague.id).is_some() || relecteur_de(vagues, &vague.id).is_none();
    decoupage_pris && relue && corrigee
}

/// Ce que le code 0 de `lydite etat` demande des vagues montrées (voir `montrees`), et rien n'étant affirmé s'il n'y en
/// a aucune. Chacune est menée à bout, et ses liens de découpage, de relecture et de correction aussi
/// (`satellites_a_bout`). Si la dernière a des notes de forge, de plus : aucune n'a de refus de forge, et la dernière
/// CI lue est verte, celle de la dernière vague qui en a une. Une vague sans note de forge garde la règle d'une vague
/// à plan.
fn toutes_menees_a_bout(montrees: &[&Vague], vagues: &[Vague]) -> bool {
    let a_bout = montrees.iter().all(|vague| menee_a_bout(vague) && satellites_a_bout(vague, vagues));
    let Some(derniere) = montrees.last() else {
        return true;
    };
    if derniere.forge.is_none() {
        return a_bout;
    }
    let forges: Vec<_> = montrees.iter().filter_map(|vague| vague.forge.as_ref()).collect();
    let sans_refus = forges.iter().all(|forge| forge.refus.is_none());
    let verte = forges.iter().rev().find_map(|forge| forge.ci) == Some(fil::Couleur::Vert);
    a_bout && sans_refus && verte
}

#[cfg(test)]
mod tests;
