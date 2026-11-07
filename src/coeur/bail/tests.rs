use super::*;

/// La durée d'un bail de la vague de M3b : `delai_main` + `delai_preuve` + 60 s.
const DUREE: Duration = Duration::from_secs(3600 + 300 + 60);

/// Une nanoseconde : un instant juste avant une échéance en est à une nanoseconde.
const NANO: Duration = Duration::from_nanos(1);

/// Le refus que rend l'appel : de ce code, sa preuve commence par `debut` (la main, la tentative, l'état du
/// bail), sa preuve et sa correction ne sont pas vides, et il s'affiche en quatre lignes.
fn refus_de<T: fmt::Debug>(appel: Result<T, Refus>, code: Code, debut: &str) -> Refus {
    let refus = appel.expect_err(debut);
    assert_eq!(refus.code, code, "{refus}");
    assert!(refus.preuve.starts_with(debut), "la preuve commence par « {debut} » : {refus}");
    assert!(!refus.preuve.trim().is_empty() && !refus.correction.trim().is_empty(), "{refus}");
    assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    refus
}

#[test]
fn une_seconde_main_sur_une_tache_active_est_refusee() {
    let t0 = Instant::now();
    let mut baux = Baux::new();
    assert_eq!(baux.ouvrir(1, t0, DUREE), Ok(1));
    // Avant le message de sa tentative 1, la tâche 1 ne prend pas de seconde main : ni tout de suite, ni plus tard,
    // ni son échéance passée, tant que la vague n'a pas expiré son bail.
    let ouvert = "main 1 : le bail de sa tentative 1 est ouvert, pour 3960 s";
    for plus_tard in [Duration::ZERO, Duration::from_secs(5), DUREE * 2] {
        let refus = refus_de(baux.ouvrir(1, t0 + plus_tard, Duration::from_secs(1)), Code::MainActive, ouvert);
        assert!(refus.correction.contains("sa tentative 1"), "la correction nomme la tentative attendue : {refus}");
    }
    // Une autre tâche a son bail à elle.
    assert_eq!(baux.ouvrir(2, t0, DUREE), Ok(1));
    // Les refus n'ont rien changé : la tâche 1 garde sa tentative, son ouverture et sa durée.
    assert!(baux.expirer(t0 + DUREE - NANO).is_empty());
    assert_eq!(baux.expirer(t0 + DUREE), [(1, 1), (2, 1)]);
}

#[test]
fn une_main_expiree_ne_se_rouvre_pas() {
    let t0 = Instant::now();
    let mut baux = Baux::new();
    assert_eq!(baux.ouvrir(1, t0, DUREE), Ok(1));
    assert_eq!(baux.expirer(t0 + DUREE), [(1, 1)]);
    // Sa main peut tourner encore : la tâche ne repart jamais, à l'échéance comme bien après, pour toute durée.
    for (plus_tard, duree) in [(DUREE, DUREE), (DUREE * 10, Duration::from_secs(1))] {
        let ouverte = baux.ouvrir(1, t0 + plus_tard, duree);
        let refus = refus_de(ouverte, Code::MainActive, "main 1 : le bail de sa tentative 1 a expiré");
        assert!(refus.preuve.ends_with("sa main peut tourner encore"), "{refus}");
    }
    // Rien ne l'a rouverte : aucune tentative 2 n'a de bail, et le sien ne se ferme pas deux fois.
    let perime = Code::ResultatPerime;
    refus_de(baux.conclure(1, 2), perime, "main 1, tentative 2 : le bail de sa tentative 1 a expiré");
    assert!(baux.expirer(t0 + DUREE * 20).is_empty());
}

#[test]
fn un_resultat_perime_est_refuse() {
    let t0 = Instant::now();
    let (mut baux, perime) = (Baux::new(), Code::ResultatPerime);
    // Après l'expiration : le message de la main arrive trop tard.
    assert_eq!(baux.ouvrir(1, t0, DUREE), Ok(1));
    assert_eq!(baux.expirer(t0 + DUREE), [(1, 1)]);
    refus_de(baux.conclure(1, 1), perime, "main 1, tentative 1 : le bail de sa tentative 1 a expiré");
    // Conclu deux fois : le second message est périmé.
    assert_eq!(baux.ouvrir(2, t0, DUREE), Ok(1));
    assert_eq!(baux.conclure(2, 1), Ok(()));
    refus_de(baux.conclure(2, 1), perime, "main 2, tentative 1 : le bail de sa tentative 1 est conclu");
    // Une autre tentative que celle du bail ouvert : périmée, et le bail reste ouvert pour la sienne.
    assert_eq!(baux.ouvrir(3, t0, DUREE), Ok(1));
    for autre in [0, 2, usize::MAX] {
        let debut = format!("main 3, tentative {autre} : le bail de sa tentative 1 est ouvert");
        refus_de(baux.conclure(3, autre), perime, &debut);
    }
    assert_eq!(baux.conclure(3, 1), Ok(()));
    // Une main inconnue : aucun bail ne l'attend.
    let refus = refus_de(baux.conclure(4, 1), perime, "main 4, tentative 1 : aucun bail");
    assert!(refus.correction.contains("main.perimee"), "un résultat périmé se note sans être pris : {refus}");
}

#[test]
fn une_main_jamais_conclue_expire_seule() {
    let t0 = Instant::now();
    let mut baux = Baux::new();
    for main in 1..=3 {
        assert_eq!(baux.ouvrir(main, t0, DUREE), Ok(1));
    }
    // La main 2 rend son message : elle n'expire pas.
    assert_eq!(baux.conclure(2, 1), Ok(()));
    // Rien juste avant l'échéance ; à l'échéance, les deux autres, dans l'ordre des mains.
    assert!(baux.expirer(t0 + DUREE - NANO).is_empty());
    assert_eq!(baux.expirer(t0 + DUREE), [(1, 1), (3, 1)]);
    // Un bail ne se ferme qu'une fois ; et une vague qui regarde tard expire ce qui est échu depuis longtemps.
    assert_eq!(baux.ouvrir(4, t0 + DUREE, DUREE), Ok(1));
    assert_eq!(baux.expirer(t0 + DUREE * 5), [(4, 1)]);
}

#[test]
fn la_relance_ouvre_une_generation_neuve() {
    let t0 = Instant::now();
    let mut baux = Baux::new();
    assert_eq!(baux.ouvrir(1, t0, DUREE), Ok(1));
    assert_eq!(baux.conclure(1, 1), Ok(()));
    // Après le message de la tentative 1, la relance : la tentative 2, un jeton neuf et une échéance neuve.
    let relance = t0 + Duration::from_secs(60);
    assert_eq!(baux.ouvrir(1, relance, DUREE), Ok(2));
    // Le résultat de la tentative 1, redit, est périmé ; l'échéance de la 1 ne tombe pas sur la 2.
    let perime = Code::ResultatPerime;
    refus_de(baux.conclure(1, 1), perime, "main 1, tentative 1 : le bail de sa tentative 2 est ouvert");
    assert!(baux.expirer(t0 + DUREE).is_empty());
    assert!(baux.expirer(relance + DUREE - NANO).is_empty());
    assert_eq!(baux.expirer(relance + DUREE), [(1, 2)]);
    // Une tâche conclue repart autant de fois que la vague la relance, d'un jeton de plus à chaque fois.
    for tentative in 1..=3 {
        assert_eq!(baux.ouvrir(2, t0, DUREE), Ok(tentative));
        assert_eq!(baux.conclure(2, tentative), Ok(()));
    }
}

#[test]
fn chaque_bail_a_sa_duree() {
    let t0 = Instant::now();
    let (courte, longue) = (Duration::from_secs(10), Duration::from_secs(20));
    let mut baux = Baux::new();
    assert_eq!(baux.ouvrir(1, t0, longue), Ok(1));
    assert_eq!(baux.ouvrir(2, t0, courte), Ok(1));
    // Ouverts au même instant, chacun échoit au bout de sa durée.
    assert!(baux.expirer(t0 + courte - NANO).is_empty());
    assert_eq!(baux.expirer(t0 + courte), [(2, 1)]);
    assert!(baux.expirer(t0 + longue - NANO).is_empty());
    assert_eq!(baux.expirer(t0 + longue), [(1, 1)]);
    // La durée passe à chaque ouverture : la relance prend la sienne, pas celle de la tentative d'avant.
    assert_eq!(baux.ouvrir(3, t0, longue), Ok(1));
    assert_eq!(baux.conclure(3, 1), Ok(()));
    assert_eq!(baux.ouvrir(3, t0, courte), Ok(2));
    assert_eq!(baux.expirer(t0 + courte), [(3, 2)]);
    // Une durée qu'aucun instant n'atteint ne déborde pas : son bail n'échoit jamais, et sa preuve se dit.
    assert_eq!(baux.ouvrir(4, t0, Duration::MAX), Ok(1));
    assert!(baux.expirer(t0 + longue * 1000).is_empty());
    refus_de(baux.ouvrir(4, t0, courte), Code::MainActive, "main 4 : le bail de sa tentative 1 est ouvert, pour ");
}
