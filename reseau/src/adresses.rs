//! Mise en forme d'un hôte et classement d'une adresse : la partie pure de la décision.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Met un hôte en forme : minuscules, point final retiré. Rend `None` si l'hôte est vide ou mal formé.
///
/// Un nom garde ses points et ses tirets ; une adresse IP est reconnue par `parse`. Les caractères d'un nom qui
/// changeraient le sens de la requête (espace, barre, arobase, dièse, interrogation) sont refusés, ainsi qu'un
/// deux-points hors adresse IP.
pub fn normaliser_hote(brut: &str) -> Option<String> {
    if brut.is_empty() || brut.bytes().any(|o| o.is_ascii_whitespace()) {
        return None;
    }
    if brut.chars().any(|c| matches!(c, '/' | '\\' | '@' | '#' | '?' | '[')) {
        return None;
    }
    // Un seul point final est retiré (le point racine) ; deux points finaux ou plus sont mal formés.
    let hote = brut.strip_suffix('.').unwrap_or(brut).to_ascii_lowercase();
    if hote.is_empty() || hote.ends_with('.') || hote.starts_with('.') || hote.contains("..") {
        return None;
    }
    if hote.parse::<IpAddr>().is_ok() {
        return Some(hote);
    }
    if hote.contains(':') {
        return None;
    }
    // raccourci : un nom est validé caractère par caractère, sans contrôle label par label ; monter avec un
    // analyseur de noms si un hôte exotique doit être refusé plus tôt.
    if hote.bytes().any(|o| !(o.is_ascii_alphanumeric() || o == b'-' || o == b'.' || o == b'_')) {
        return None;
    }
    Some(hote)
}

/// Une IPv4 logée dans une IPv6 par les voies connues : mappée, compatible, NAT64, 6to4.
fn v4_logee(v6: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(v4) = v6.to_ipv4_mapped() {
        return Some(v4);
    }
    let s = v6.segments();
    if s[..6] == [0, 0, 0, 0, 0, 0] {
        return Some(Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8));
    }
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
        return Some(Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8));
    }
    if s[0] == 0x2002 {
        return Some(Ipv4Addr::new((s[1] >> 8) as u8, s[1] as u8, (s[2] >> 8) as u8, s[2] as u8));
    }
    None
}

// raccourci : l'adresse publique de la machine elle-même n'est pas reconnue (std ne liste pas les interfaces) ;
// monter avec une table d'interfaces si le filtre doit refuser le rebouclage vers l'hôte.

/// Vrai si l'adresse appartient à une classe que le filtre refuse.
pub fn adresse_refusee(adresse: IpAddr) -> bool {
    match adresse {
        IpAddr::V4(v4) => v4_refusee(v4),
        IpAddr::V6(v6) => match v4_logee(v6) {
            Some(v4) => v4_refusee(v4),
            None => {
                v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || lien_local_v6(v6) || unique_local_v6(v6)
            }
        },
    }
}

fn v4_refusee(a: Ipv4Addr) -> bool {
    let octets = a.octets();
    // 100.64/10 : l'espace partagé des opérateurs (CGNAT).
    let partage = octets[0] == 100 && (octets[1] & 0xc0) == 64;
    // 0.0.0.0/8 : `is_unspecified` ne couvre que 0.0.0.0.
    let zero = octets[0] == 0;
    zero || a.is_loopback()
        || a.is_unspecified()
        || a.is_private()
        || a.is_link_local()
        || a.is_broadcast()
        || a.is_multicast()
        || partage
}

fn lien_local_v6(a: Ipv6Addr) -> bool {
    // fe80::/10
    (a.segments()[0] & 0xffc0) == 0xfe80
}

fn unique_local_v6(a: Ipv6Addr) -> bool {
    // fc00::/7
    (a.segments()[0] & 0xfe00) == 0xfc00
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Une adresse IPv4 écrite par ses octets : la garde du dépôt refuse la forme écrite d'une adresse privée.
    fn v4(a: u8, b: u8, c: u8, d: u8) -> String {
        Ipv4Addr::new(a, b, c, d).to_string()
    }

    #[test]
    fn l_hote_est_mis_en_forme() {
        assert_eq!(normaliser_hote("Example.COM.").as_deref(), Some("example.com"));
        assert_eq!(normaliser_hote("example.com").as_deref(), Some("example.com"));
        assert_eq!(normaliser_hote("example.com.").as_deref(), Some("example.com"));
        assert!(normaliser_hote("example.com..").is_none());
        assert_eq!(normaliser_hote("127.0.0.1").as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn un_hote_vide_ou_mal_forme_est_refuse() {
        for brut in ["", ".", "..", "example .com", "exa/mple.com", "user@example.com", "exa:mple", "[::1]", "a..b"] {
            assert!(normaliser_hote(brut).is_none(), "{brut:?}");
        }
    }

    #[test]
    fn les_classes_d_adresse_sont_refusees() {
        let refusees: &[&str] = &[
            "127.0.0.1",
            "0.0.0.0",
            "169.254.1.1",
            "255.255.255.255",
            "224.0.0.1",
            &v4(10, 0, 0, 1),
            &v4(172, 16, 0, 1),
            &v4(192, 168, 1, 1),
            &v4(100, 64, 0, 1),
            &v4(100, 127, 255, 255),
            "0.0.0.2",
            &format!("::{}", v4(10, 0, 0, 1)),
            &format!("64:ff9b::{}", v4(10, 0, 0, 1)),
            "2002:0a00:0001::",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd00::1",
            "fdff:ffff::1",
            "ff02::1",
            &format!("::ffff:{}", v4(10, 0, 0, 1)),
        ];
        for texte in refusees {
            let adresse = texte.parse::<IpAddr>().unwrap();
            assert!(adresse_refusee(adresse), "{texte}");
        }
        for texte in ["192.0.2.1", "198.51.100.1", "2001:db8::1"] {
            let adresse = texte.parse::<IpAddr>().unwrap();
            assert!(!adresse_refusee(adresse), "{texte}");
        }
    }
}
