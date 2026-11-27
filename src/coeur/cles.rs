//! La seule liste des formes de clés de Lydite : [`forme_de_cle`] rend le nom de la première forme que porte une ligne.
//! Le relevé des points à justifier et le refus de publication (8005) la lisent, et aucun des deux n'a sa propre liste.

/// Le début d'un en-tête PEM : cinq tirets, `BEGIN`, un blanc.
const DEBUT_PEM: &str = "-----BEGIN ";
/// La fin d'un en-tête de clé privée PEM : les mots qui la disent, puis cinq tirets.
const FIN_PEM: &str = "PRIVATE KEY-----";

/// `octets[i]` est au début de la ligne ou après un caractère qui n'est ni une lettre ASCII, ni un chiffre, ni `_`.
fn a_sa_frontiere(octets: &[u8], i: usize) -> bool {
    i == 0 || !(octets[i - 1].is_ascii_alphanumeric() || octets[i - 1] == b'_')
}

/// `texte` porte `prefixe` à une frontière, suivi d'au moins `longueur` caractères que `permis` accepte : une
/// occurrence suffit, la première n'empêche pas les suivantes.
fn porte_un_prefixe(texte: &str, prefixe: &str, permis: impl Fn(u8) -> bool, longueur: usize) -> bool {
    let octets = texte.as_bytes();
    texte.match_indices(prefixe).any(|(i, _)| {
        a_sa_frontiere(octets, i) && octets[i + prefixe.len()..].iter().take_while(|c| permis(**c)).count() >= longueur
    })
}

/// `texte` porte, à une frontière, `AKIA` ou `ASIA` suivi d'exactement 16 majuscules ou chiffres ASCII, puis de la
/// fin de la ligne ou d'un caractère qui n'est ni une lettre ASCII ni un chiffre.
fn porte_un_identifiant_aws(texte: &str) -> bool {
    let octets = texte.as_bytes();
    ["AKIA", "ASIA"].iter().any(|prefixe| {
        texte.match_indices(prefixe).any(|(i, _)| {
            let corps = &octets[i + prefixe.len()..];
            let seize = corps.len() >= 16 && corps[..16].iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
            seize && a_sa_frontiere(octets, i) && !corps.get(16).is_some_and(|c| c.is_ascii_alphanumeric())
        })
    })
}

/// Le nom montré de la première forme de clé de la liste que porte la ligne ajoutée `texte`, s'il y en a une.
// raccourci : une forme hors de la liste (Slack, Stripe, Google, un jeton JWT, un mot de passe, une valeur longue après
// `api_key =`, un bloc de clé privée PGP dont l'en-tête ne finit pas par les mots de la forme PEM suivis de cinq
// tirets) n'est pas vue, monter si un fournisseur dont le code tient les clés manque à la liste
// raccourci : une clé coupée sur deux lignes ou encodée (base64, hexadécimal) n'est pas vue, monter si une main en
// écrit ainsi
// raccourci : une fausse clé d'essai ou un exemple de documentation qui ont la forme sont relevés et refusés, c'est
// voulu : Lydite ne juge pas
pub(crate) fn forme_de_cle(texte: &str) -> Option<&'static str> {
    let cle = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    let alphanumerique = |c: u8| c.is_ascii_alphanumeric();
    if porte_un_prefixe(texte, "sk-ant-", cle, 20) {
        return Some("Anthropic key");
    }
    if porte_un_prefixe(texte, "sk-", cle, 30) {
        return Some("sk- key");
    }
    let jeton = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"].iter().any(|p| porte_un_prefixe(texte, p, alphanumerique, 30));
    if jeton || porte_un_prefixe(texte, "github_pat_", |c| alphanumerique(c) || c == b'_', 30) {
        return Some("GitHub token");
    }
    if porte_un_identifiant_aws(texte) {
        return Some("AWS access key ID");
    }
    if porte_un_prefixe(texte, "K00", |c| alphanumerique(c) || c == b'/' || c == b'+', 20) {
        return Some("Backblaze key");
    }
    let pem = texte.find(DEBUT_PEM).is_some_and(|i| texte[i + DEBUT_PEM.len()..].contains(FIN_PEM));
    pem.then_some("PEM private key")
}
