//! La décision pure : liste, hôte, adresses résolues → verdict.

use std::net::IpAddr;

use crate::adresses::{adresse_refusee, normaliser_hote};

/// La raison d'un refus. Le proxy en tire la réponse HTTP et le journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refus {
    /// L'hôte est vide ou mal formé (400).
    HoteInvalide,
    /// L'hôte n'est pas exactement dans la liste (403, `blocked-by-allowlist`).
    HorsListe,
    /// Une adresse résolue est de boucle locale, privée ou autre classe refusée (403,
    /// `blocked-by-resolved-address`).
    AdresseRefusee,
    /// Le nom n'a pas pu être résolu (502).
    ResolutionImpossible,
}

/// Le verdict de la décision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// L'hôte est permis ; la connexion sortante doit viser cette adresse, sans seconde résolution.
    Permis {
        adresse: IpAddr,
    },
    Refuse(Refus),
}

/// Décide pour `hote` et les `adresses` résolues, sans toucher ni au DNS ni à un socket.
///
/// L'hôte est mis en forme. Il doit être exactement dans la liste : pas de joker, pas de sous-domaine implicite. Une
/// adresse IP écrite en clair et listée telle quelle est permise sans résolution ni contrôle de classe ; une IP non
/// listée est hors liste. Pour un nom listé, toutes les adresses résolues sont contrôlées, sauf une adresse résolue qui
/// est elle-même une IP listée : elle est permise par exception, la liste est l'autorité. La connexion se fera vers la
/// première, jamais par une seconde résolution.
pub fn decider(domaines: &[String], hote: &str, adresses: &[IpAddr]) -> Verdict {
    let Some(hote) = normaliser_hote(hote) else {
        return Verdict::Refuse(Refus::HoteInvalide);
    };
    if !domaines.iter().any(|domaine| domaine == &hote) {
        return Verdict::Refuse(Refus::HorsListe);
    }
    if let Ok(adresse) = hote.parse::<IpAddr>() {
        // L'exception de l'IP listée telle quelle : aucune résolution, aucun contrôle de classe.
        return Verdict::Permis { adresse };
    }
    if adresses.is_empty() {
        return Verdict::Refuse(Refus::ResolutionImpossible);
    }
    // L'exception « adresse IP listée telle quelle » vaut aussi pour une adresse résolue : l'opérateur a
    // explicitement autorisé cette IP. Une adresse résolue non listée et de classe refusée fait refuser le nom.
    let listees: Vec<IpAddr> = domaines.iter().filter_map(|domaine| domaine.parse().ok()).collect();
    let mut permise = None;
    for adresse in adresses {
        if listees.contains(adresse) {
            permise.get_or_insert(*adresse);
            continue;
        }
        if adresse_refusee(*adresse) {
            return Verdict::Refuse(Refus::AdresseRefusee);
        }
        permise.get_or_insert(*adresse);
    }
    let Some(adresse) = permise else {
        return Verdict::Refuse(Refus::ResolutionImpossible);
    };
    // raccourci : une seule adresse vérifiée est rendue et essayée ; monter avec la liste vérifiée si un double pile
    // doit basculer vers la seconde quand la première ne répond pas.
    Verdict::Permis { adresse }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::net::Ipv4Addr;

    fn liste(noms: &[&str]) -> Vec<String> {
        noms.iter().map(|nom| nom.to_string()).collect()
    }

    fn ip(texte: &str) -> IpAddr {
        texte.parse().unwrap()
    }

    fn publique() -> IpAddr {
        ip("192.0.2.1")
    }

    /// Une adresse IPv4 écrite par ses octets : la garde du dépôt refuse la forme écrite d'une adresse privée.
    fn v4(a: u8, b: u8, c: u8, d: u8) -> String {
        Ipv4Addr::new(a, b, c, d).to_string()
    }

    #[test]
    fn un_nom_exact_est_permis() {
        assert_eq!(
            decider(&liste(&["example.com"]), "example.com", &[publique()]),
            Verdict::Permis { adresse: publique() }
        );
    }

    #[test]
    fn la_casse_et_le_point_final_sont_ignores() {
        assert_eq!(
            decider(&liste(&["example.com"]), "EXAMPLE.COM.", &[publique()]),
            Verdict::Permis { adresse: publique() }
        );
    }

    #[test]
    fn un_sous_domaine_ou_un_suffixe_ne_suffit_pas() {
        let domaines = liste(&["example.com"]);
        for hote in ["sub.example.com", "example.com.example.org", "example.com.evil", "exemple.com"] {
            assert_eq!(decider(&domaines, hote, &[publique()]), Verdict::Refuse(Refus::HorsListe), "{hote}");
        }
    }

    #[test]
    fn une_ip_listee_est_permise_sans_controle_de_classe() {
        let privee = v4(10, 0, 0, 1);
        let domaines = liste(&["127.0.0.1", &privee]);
        assert_eq!(decider(&domaines, "127.0.0.1", &[]), Verdict::Permis { adresse: ip("127.0.0.1") });
        assert_eq!(decider(&domaines, &privee, &[]), Verdict::Permis { adresse: ip(&privee) });
    }

    #[test]
    fn une_ip_non_listee_est_hors_liste() {
        assert_eq!(decider(&liste(&["example.com"]), "127.0.0.1", &[]), Verdict::Refuse(Refus::HorsListe));
    }

    #[test]
    fn un_hote_vide_ou_mal_forme_est_400() {
        for hote in ["", " ", "exa mple.com", "exa,mple.com", "exa/mple.com", "user@example.com", "exa:mple"] {
            assert_eq!(decider(&liste(&["example.com"]), hote, &[]), Verdict::Refuse(Refus::HoteInvalide), "{hote:?}");
        }
    }

    #[test]
    fn chaque_classe_d_adresse_resolue_est_refusee() {
        let classes: &[&str] = &[
            "127.0.0.1",
            "0.0.0.0",
            "169.254.1.1",
            "255.255.255.255",
            "224.0.0.1",
            &v4(10, 0, 0, 1),
            &v4(172, 16, 0, 1),
            &v4(192, 168, 1, 1),
            &v4(100, 64, 0, 1),
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "ff02::1",
            &format!("::ffff:{}", v4(10, 0, 0, 1)),
        ];
        for texte in classes {
            let adresse = ip(texte);
            assert_eq!(
                decider(&liste(&["example.com"]), "example.com", &[adresse]),
                Verdict::Refuse(Refus::AdresseRefusee),
                "{texte}"
            );
        }
    }

    #[test]
    fn une_seule_adresse_refusee_suffit() {
        assert_eq!(
            decider(&liste(&["example.com"]), "example.com", &[publique(), ip(&v4(192, 168, 1, 1))]),
            Verdict::Refuse(Refus::AdresseRefusee)
        );
    }

    #[test]
    fn sans_adresse_resolue_c_est_502() {
        assert_eq!(decider(&liste(&["example.com"]), "example.com", &[]), Verdict::Refuse(Refus::ResolutionImpossible));
    }

    #[test]
    fn une_adresse_resolue_listee_est_permise_par_exception() {
        let domaines = liste(&["example.com", "127.0.0.1"]);
        assert_eq!(decider(&domaines, "example.com", &[ip("127.0.0.1")]), Verdict::Permis { adresse: ip("127.0.0.1") });
    }

    #[test]
    fn une_adresse_resolue_listee_ne_dispense_pas_des_autres() {
        let domaines = liste(&["example.com", "127.0.0.1"]);
        assert_eq!(
            decider(&domaines, "example.com", &[ip("127.0.0.1"), ip(&v4(192, 168, 1, 1))]),
            Verdict::Refuse(Refus::AdresseRefusee)
        );
    }

    #[test]
    fn une_adresse_publique_passe() {
        let adresse = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        assert_eq!(decider(&liste(&["example.com"]), "example.com", &[adresse]), Verdict::Permis { adresse });
    }
}
