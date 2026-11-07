use super::*;
use std::collections::HashSet;

/// L'empreinte de ces octets, en hexadécimal.
fn empreinte(octets: &[u8]) -> String {
    hex(&sha256(octets))
}

/// `n` fois la lettre « a » : les octets de `head -c <n> /dev/zero | tr '\0' a`.
fn des_a(n: usize) -> Vec<u8> {
    vec![b'a'; n]
}

#[test]
fn les_vecteurs_de_fips_180_4_se_retrouvent() {
    // Les messages d'essai de la norme. Chaque attendu est la sortie de la commande écrite au-dessus de lui, jouée
    // une fois par `sha256sum` sur les octets du message, sans fin de ligne ; il est aussi la valeur des exemples
    // de FIPS 180-4.
    let cas: [(&[u8], &str); 4] = [
        // printf '' | sha256sum
        (b"", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        // printf 'abc' | sha256sum
        (b"abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        // 448 bits, 56 octets : le reste qui force un second bloc de bourrage.
        // printf '%s' 'abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq' | sha256sum
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        // 896 bits, 112 octets : un bloc entier, puis 48 octets de reste.
        // printf '%s' '<le message ci-dessous>' | sha256sum
        (
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmn\
              hijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
    ];
    for (message, attendu) in cas {
        assert_eq!(empreinte(message), attendu, "{} octets", message.len());
    }
    // Un million de « a » : 15 625 blocs. head -c 1000000 /dev/zero | tr '\0' a | sha256sum
    assert_eq!(empreinte(&des_a(1_000_000)), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
}

#[test]
fn les_longueurs_de_bord_de_bloc_se_retrouvent() {
    // De part et d'autre du bourrage : à 55 octets, 0x80 et la longueur tiennent encore dans le bloc ; à 56, ils en
    // veulent un second ; à 63, le reste est plein à un octet près ; à 64, un bloc entier sans reste ; à 65, un
    // octet de reste. Chaque attendu : head -c <n> /dev/zero | tr '\0' a | sha256sum
    let cas = [
        (55, "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"),
        (56, "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"),
        (63, "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34"),
        (64, "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"),
        (65, "635361c48bb9eab14198e76ea8ab7f1a41685d6ad62aa9146d301d4f17eb0ae0"),
    ];
    for (n, attendu) in cas {
        assert_eq!(empreinte(&des_a(n)), attendu, "{n} octets");
    }
    // Aucune longueur ne fait paniquer, et chacune a son empreinte : de 0 à 300 octets, chaque reste de bloc et les
    // deux formes de bourrage, plusieurs fois.
    let toutes: HashSet<String> = (0..=300).map(|n| empreinte(&des_a(n))).collect();
    assert_eq!(toutes.len(), 301);
}

#[test]
fn l_empreinte_s_ecrit_en_64_minuscules() {
    // `{}`, sans fin de ligne : l'empreinte que tests/profil.rs attendra. printf '{}' | sha256sum
    let accolades = empreinte(b"{}");
    assert_eq!(accolades, "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a");
    assert_eq!(accolades.len(), 64);
    // Deux caractères par octet, zéro de tête compris, et en minuscules : de 0x00 à 0x1f, puis de 0xe0 à 0xff,
    // chaque chiffre hexadécimal passe à la première place et à la seconde.
    let bas: [u8; 32] = std::array::from_fn(|i| i as u8);
    let haut: [u8; 32] = std::array::from_fn(|i| 0xe0 + i as u8);
    assert_eq!(hex(&bas), "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
    assert_eq!(hex(&haut), "e0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
    // Et toute empreinte, ni plus courte ni plus longue, ne porte que des chiffres et des minuscules de a à f.
    for n in 0..=130 {
        let texte = empreinte(&des_a(n));
        let juste = texte.len() == 64 && texte.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
        assert!(juste, "{n} octets : {texte}");
    }
}
