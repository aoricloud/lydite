use super::*;
use crate::coeur::fil::lot;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

/// Les blocs de mille : le premier numéro de chacun, et le préfixe des noms qu'il porte.
const BLOCS: &[(u16, &str)] = &[(1000, "contrat"), (6000, "lancer"), (7000, "vague"), (8000, "forge")];

/// Le bloc 1000 du contrat v2 de passerelle, copié en entier : Kraken n'en construit que six lignes.
const COPIE_DU_BLOC_1000: &[(u16, &str)] = &[
    (1001, "contrat.json_illisible"),
    (1002, "contrat.version_differente"),
    (1003, "contrat.poignee_attendue"),
    (1004, "contrat.poignee_repetee"),
    (1005, "contrat.commande_inconnue"),
    (1006, "contrat.commande_mal_formee"),
    (1007, "contrat.demande_en_cours"),
    (1008, "contrat.evenement_inconnu"),
    (1009, "contrat.evenement_mal_forme"),
    (1010, "contrat.sequence_rompue"),
    (1011, "contrat.flux_incoherent"),
];

/// La table publiée. Ce test ne se modifie que pour **ajouter** une ligne, au bout de son bloc : changer un
/// numéro ou un nom existant casse ceux qui les lisent.
const TABLE: &[(u16, &str)] = &[
    // --- 1000 : contrat ---
    (1001, "contrat.json_illisible"),
    (1002, "contrat.version_differente"),
    (1005, "contrat.commande_inconnue"),
    (1006, "contrat.commande_mal_formee"),
    (1009, "contrat.evenement_mal_forme"),
    (1010, "contrat.sequence_rompue"),
    // --- 6000 : lancer ---
    (6001, "lancer.profil_inconnu"),
    (6002, "lancer.depot_introuvable"),
    (6003, "lancer.bac_impossible"),
    (6004, "lancer.dossier_relatif"),
    (6005, "lancer.dossier_introuvable"),
    (6006, "lancer.entree_sortie"),
    (6007, "lancer.profil_nom_refuse"),
    (6008, "lancer.profil_illisible"),
    (6009, "lancer.profil_ligne_refusee"),
    (6010, "lancer.profil_incomplet"),
    (6011, "lancer.profil_non_accepte"),
    (6012, "lancer.profil_change"),
    (6013, "lancer.profil_sans_fichier"),
    (6014, "lancer.preuve_vide"),
    (6015, "lancer.dossier_sous_le_depot"),
    (6016, "lancer.plan_version"),
    (6017, "lancer.plan_encodage"),
    (6018, "lancer.plan_ligne"),
    (6019, "lancer.plan_identifiant"),
    (6020, "lancer.plan_dependance"),
    (6021, "lancer.plan_blanc"),
    (6022, "lancer.base_rouge"),
    (6023, "lancer.dossier_chemin_refuse"),
    (6024, "lancer.plan_chemin"),
    (6025, "lancer.plan_borne"),
    (6026, "lancer.plan_entete"),
    (6027, "lancer.fournisseur"),
    (6028, "lancer.mur_refuse"),
    (6029, "lancer.issue_sans_mur"),
    (6030, "lancer.quota"),
    (6031, "lancer.lecon_mal_formee"),
    (6032, "lancer.dossier_pris"),
    (6033, "lancer.effort_non_permis"),
    (6034, "lancer.memoire_texte_absent"),
    (6035, "lancer.memoire_empreinte_fausse"),
    (6036, "lancer.fichier_pose_refuse"),
    (6038, "lancer.rapport_refuse"),
    // --- 7000 : vague ---
    (7001, "vague.bac_impossible"),
    (7002, "vague.main_active"),
    (7003, "vague.resultat_perime"),
    (7004, "vague.main_expiree"),
    (7005, "vague.main_tuee"),
    (7006, "vague.claude_touche"),
    (7007, "vague.conflit"),
    (7008, "vague.preuve_rouge"),
    (7009, "vague.git_impossible"),
    (7010, "vague.assemblage_ferme"),
    (7011, "vague.sautee"),
    (7012, "vague.fil_injoignable"),
    (7013, "vague.hors_fichiers"),
    (7014, "vague.bac_touche"),
    (7015, "vague.remarques_illisibles"),
    (7016, "vague.relecture_manquee"),
    (7017, "vague.bloquantes_restantes"),
    (7018, "vague.chemin_protege"),
    (7019, "vague.plafond_atteint"),
    (7020, "vague.bloquante_hors_de_portee"),
    (7021, "vague.ouverture_executable"),
    (7022, "vague.empreinte_differente"),
    // --- 8000 : forge ---
    (8001, "forge.distant_illisible"),
    (8002, "forge.base_inconnue"),
    (8003, "forge.gh_injoignable"),
    (8004, "forge.rien_a_pousser"),
    (8005, "forge.publication_refusee"),
    (8006, "forge.poussee_refusee"),
    (8007, "forge.pr_impossible"),
    (8008, "forge.ci_muette"),
    (8009, "forge.a_reintegrer"),
    (8010, "forge.pr_changee"),
    (8011, "forge.ci_rouge"),
    (8012, "forge.issue_illisible"),
    (8013, "forge.issue_trop_longue"),
];

#[test]
fn table_figee() {
    let reelle: Vec<(u16, &str)> = Code::TOUS.iter().map(|c| (c.numero(), c.nom())).collect();
    assert_eq!(reelle, TABLE, "un code publié a changé : on ajoute au bout de son bloc, on ne renumérote pas");
}

#[test]
fn blocs_tenus() {
    // Chaque code tombe dans un bloc et porte le préfixe de celui-là.
    for code in Code::TOUS {
        let numero = code.numero();
        let (_, prefixe) = BLOCS
            .iter()
            .find(|(base, _)| (base + 1..base + 1000).contains(&numero))
            .unwrap_or_else(|| panic!("{code} n'est dans aucun bloc"));
        assert_eq!(code.nom().split('.').next(), Some(*prefixe), "{code}");
    }
    // Les blocs se suivent sans se chevaucher.
    assert!(BLOCS.windows(2).all(|w| w[0].0 + 1000 <= w[1].0));
    // Numéros et noms uniques ; la liste suit l'ordre des numéros.
    assert_eq!(Code::TOUS.iter().map(|c| c.numero()).collect::<HashSet<_>>().len(), Code::TOUS.len());
    assert_eq!(Code::TOUS.iter().map(|c| c.nom()).collect::<HashSet<_>>().len(), Code::TOUS.len());
    assert!(Code::TOUS.windows(2).all(|w| w[0].numero() < w[1].numero()));
}

#[test]
fn copie_du_bloc_1000() {
    for code in Code::TOUS {
        let (numero, nom) = (code.numero(), code.nom());
        if (1000..6000).contains(&numero) || (9000..10000).contains(&numero) {
            let copie = COPIE_DU_BLOC_1000.contains(&(numero, nom));
            assert!(copie, "{code} n'est pas une ligne du bloc 1000 de passerelle");
        } else {
            assert!((6000..9000).contains(&numero), "{code} : un code de ce dépôt vit dans 6000 à 8999");
        }
    }
}

#[test]
fn un_numero_rend_son_code() {
    // Chaque code se retrouve par son numéro ; un numéro que Kraken ne construit pas, de passerelle ou d'un autre
    // écrivain du fil, n'en rend aucun.
    for code in Code::TOUS {
        assert_eq!(Code::de_numero(code.numero()), Some(*code));
    }
    for numero in [0, 1003, 6999, 9999] {
        assert_eq!(Code::de_numero(numero), None, "{numero}");
    }
}

#[test]
fn chaque_code_a_sa_regle() {
    for code in Code::TOUS {
        let regle = code.regle();
        let phrase = !regle.trim().is_empty() && regle.ends_with('.') && !regle.contains('\n');
        assert!(phrase, "{code} : une phrase non vide, pas {regle:?}");
    }
}

/// L'en-tête de `docs/refusals.md` : ce qui précède la table.
const ENTETE_DE_LA_PAGE: &str = "# Lydite refusals

A refusal that stops Lydite before any wave exits with code 2, on four lines: `lydite : <code> <name>`, then `rule:`, \
`seen:` and `fix:`. The same codes name the refusals of a wave, which `lydite status` shows. A code and its name never \
change; a rule may be reworded. The codes come in four blocks of a thousand:

- 1000, `contrat`: the command line and the thread.
- 6000, `lancer`: everything before a wave opens (the command line, the profile, the repository, Lydite's folder and \
the plan).
- 7000, `vague`: the hands and the assembly, then the files of each hand, the splitter, the reviewer and the \
correction that follows its review.
- 8000, `forge`: the pull request and the CI, before and after the wave.

| Code | Name | Rule |
|---|---|---|
";

/// Le texte de `docs/refusals.md`, tiré de `Code::TOUS` : l'en-tête, puis une rangée par code, dans l'ordre.
fn page_des_refus() -> String {
    let mut page = ENTETE_DE_LA_PAGE.to_string();
    for code in Code::TOUS {
        let regle = code.regle().replace('|', "\\|");
        page.push_str(&format!("| {} | `{}` | {regle} |\n", code.numero(), code.nom()));
    }
    page
}

#[test]
fn la_page_des_refus_suit_les_codes() {
    let chemin = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs").join("refusals.md");
    let attendu = page_des_refus();
    // Un retour chariot que git aurait posé sous Windows ne change pas la page.
    let ecrite = fs::read_to_string(&chemin).unwrap_or_default().replace("\r\n", "\n");
    assert!(
        ecrite == attendu,
        "{} ne suit pas Code::TOUS ; voici le texte attendu en entier, à recopier :\n{attendu}",
        chemin.display()
    );
}

#[test]
fn le_refus_s_affiche_en_quatre_lignes() {
    let regle = Code::ProfilInconnu.regle();
    let refus = Refus::new(Code::ProfilInconnu, "profil « x »", "Nomme un profil que Lydite connaît.");
    let attendu = [
        "lydite : 6001 lancer.profil_inconnu".to_string(),
        format!("  rule: {regle}"),
        "  seen: profil « x »".to_string(),
        "  fix: Nomme un profil que Lydite connaît.".to_string(),
    ];
    assert_eq!(refus.to_string(), attendu.join("\n"));
    // La sortie de git tient sur plusieurs lignes : elle se met bout à bout, et le refus reste à quatre lignes.
    let git = "fatal: la branche existe déjà\r\nhint: la renommer\n\n";
    let refus = Refus::new(Code::BacImpossible, git, "Retire l'ancien bac.\nPuis relance.");
    let texte = refus.to_string();
    assert_eq!(texte.lines().count(), 4, "{texte}");
    assert!(texte.contains("\n  seen: fatal: la branche existe déjà ; hint: la renommer\n"), "{texte}");
    assert!(texte.ends_with("\n  fix: Retire l'ancien bac. ; Puis relance."), "{texte}");
}

#[test]
fn une_erreur_d_e_s_devient_6006_avec_son_message() {
    // Celui qui pose l'erreur y met le chemin ou le programme, et garde son `kind()` jusqu'à la conversion.
    let erreur = io::Error::new(io::ErrorKind::PermissionDenied, "fil.jsonl : accès refusé");
    let refus = Refus::from(erreur);
    assert_eq!((refus.code.numero(), refus.code.nom()), (6006, "lancer.entree_sortie"));
    assert_eq!(refus.preuve, "fil.jsonl : accès refusé");
    assert_eq!(
        refus.correction,
        "Check that the path, or the program of the proof, exists and that Lydite can read, write or run it, \
         then retry."
    );
    // `?` convertit seul : un vrai fichier absent donne le message de l'erreur du système.
    let lire = |chemin: &Path| -> Result<Vec<u8>, Refus> { Ok(fs::read(chemin)?) };
    let absent = env::temp_dir().join(format!("lydite-test-erreur-{}", lot())).join("fil.jsonl");
    let refus = lire(&absent).unwrap_err();
    assert_eq!(refus.code, Code::EntreeSortie);
    assert_eq!(refus.preuve, fs::read(&absent).unwrap_err().to_string());
}

#[cfg(debug_assertions)]
#[test]
fn un_refus_sans_preuve_ou_sans_correction_n_enseigne_pas() {
    // `Refus::new` le refuse (par une assertion de débogage) : une preuve vide n'a rien vu, une correction vide ne
    // dit rien à faire, et des blancs ne valent pas mieux.
    for (preuve, correction) in [("", "Corrige."), (" \n\t", "Corrige."), ("Vu.", ""), ("Vu.", "\r\n ")] {
        let refus = std::panic::catch_unwind(|| Refus::new(Code::ProfilInconnu, preuve, correction));
        assert!(refus.is_err(), "{preuve:?} et {correction:?} devaient être refusées");
    }
    // Les deux dites, le refus se construit.
    assert_eq!(Refus::new(Code::ProfilInconnu, "Vu.", "Corrige.").correction, "Corrige.");
}

#[test]
fn nommee_garde_son_kind_et_nomme_ce_qu_elle_visait() {
    let erreur = nommee(io::Error::from(io::ErrorKind::PermissionDenied), "git dans /depot");
    assert_eq!(erreur.kind(), io::ErrorKind::PermissionDenied);
    assert!(erreur.to_string().starts_with("git dans /depot : "), "{erreur}");
}

#[test]
fn un_refus_affiche_n_a_aucun_caractere_de_controle() {
    let refus = Refus::new(Code::Conflit, "a\u{1b}[31m\tb\u{202e}\nc", "d\u{7}\r\ne");
    let affiche = refus.to_string();
    assert!(affiche.contains("  seen: a\\u{1b}[31m\\tb\\u{202e} ; c\n"), "{affiche:?}");
    assert!(affiche.ends_with("  fix: d\\u{7} ; e"), "{affiche:?}");
    assert!(!affiche.chars().any(|c| c.is_control() && c != '\n'), "{affiche:?}");
    // Le refus garde ses champs bruts.
    assert!(refus.preuve.contains('\u{1b}'));
}

#[test]
fn la_queue_garde_les_4000_derniers_caracteres() {
    // Des caractères de deux octets : la coupe compte des caractères, pas des octets. Un assert! dit les longueurs :
    // un assert_eq! imprimerait deux textes de 4000 caractères.
    let juste = "é".repeat(4000);
    let gardes = |texte: &str| queue(texte).chars().count();
    assert!(queue(&juste) == juste, "4000 caractères tiennent tels quels : {} gardés", gardes(&juste));
    let trop = format!("A{juste}");
    assert!(queue(&trop) == juste, "le 4001e, en tête, tombe : {} gardés", gardes(&trop));
    assert_eq!(queue("court"), "court");
    assert_eq!(queue(""), "");
}
