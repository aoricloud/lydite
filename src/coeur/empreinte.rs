//! L'empreinte d'un profil : le SHA-256 de FIPS 180-4, écrit à la main avec la std seule, car le dépôt n'a aucune
//! dépendance. [`sha256`] hache des octets ; [`hex`] écrit le résultat en 64 hexadécimaux minuscules.

// raccourci : SHA-256 à la main, tout en mémoire ; monter vers sha2 si de gros objets doivent s'empreindre

/// Les constantes des 64 rondes : les 32 premiers bits des parties fractionnaires des racines cubiques des 64 premiers
/// nombres premiers (§4.2.2 de la norme, dans sa disposition à huit par ligne).
#[rustfmt::skip]
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// L'état de départ : les 32 premiers bits des parties fractionnaires des racines carrées des 8 premiers nombres
/// premiers (§5.3.3).
const H0: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

/// Le SHA-256 de ces octets (FIPS 180-4, §6.2). Les blocs entiers de 64 octets se prennent dans l'entrée même ; le
/// reste, bourré, finit dans un tampon de deux blocs. Aucune entrée ne le fait paniquer.
pub(crate) fn sha256(octets: &[u8]) -> [u8; 32] {
    let mut etat = H0;
    let (blocs, reste) = octets.as_chunks::<64>();
    for bloc in blocs {
        compresser(&mut etat, bloc);
    }
    // Le bourrage (§5.1.1) : le reste, l'octet 0x80, des zéros, puis la longueur de l'entrée en bits sur huit octets,
    // grand-boutiste. Le reste fait 63 octets au plus : jusqu'à 55, ces pièces tiennent dans un bloc, sinon dans deux.
    let mut fin = [0; 128];
    fin[..reste.len()].copy_from_slice(reste);
    fin[reste.len()] = 0x80;
    let taille = if reste.len() < 56 { 64 } else { 128 };
    // Modulo 2^64 : aucune entrée n'atteint 2^61 octets, et `wrapping_mul` ne panique pas même à cette limite.
    let bits = (octets.len() as u64).wrapping_mul(8);
    fin[taille - 8..taille].copy_from_slice(&bits.to_be_bytes());
    for bloc in fin[..taille].as_chunks::<64>().0 {
        compresser(&mut etat, bloc);
    }
    let mut empreinte = [0; 32];
    for (morceau, mot) in empreinte.as_chunks_mut::<4>().0.iter_mut().zip(etat) {
        *morceau = mot.to_be_bytes();
    }
    empreinte
}

/// L'empreinte en 64 hexadécimaux minuscules : deux par octet, zéro de tête compris.
pub(crate) fn hex(empreinte: &[u8; 32]) -> String {
    empreinte.iter().map(|octet| format!("{octet:02x}")).collect()
}

/// Un bloc de 64 octets entre dans l'état (§6.2.2). Les additions se font modulo 2^32.
fn compresser(etat: &mut [u32; 8], bloc: &[u8; 64]) {
    // L'échéancier W : les 16 mots du bloc, grand-boutistes, puis 48 mots qui en découlent (σ0 et σ1 du §4.1.2).
    let mut w = [0u32; 64];
    for (mot, octets) in w.iter_mut().zip(bloc.as_chunks::<4>().0) {
        *mot = u32::from_be_bytes(*octets);
    }
    for t in 16..64 {
        let (x, y) = (w[t - 15], w[t - 2]);
        let sigma0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
        let sigma1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
        w[t] = w[t - 16].wrapping_add(sigma0).wrapping_add(w[t - 7]).wrapping_add(sigma1);
    }
    // Les 64 rondes sur les huit mots de travail ; Σ0, Σ1, Ch et Maj sont ceux du §4.1.2.
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *etat;
    for (k, mot) in K.iter().zip(w) {
        let grand_sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choix = (e & f) ^ (!e & g);
        let t1 = h.wrapping_add(grand_sigma1).wrapping_add(choix).wrapping_add(*k).wrapping_add(mot);
        let grand_sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majorite = (a & b) ^ (a & c) ^ (b & c);
        let t2 = grand_sigma0.wrapping_add(majorite);
        (h, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
    }
    for (mot, travail) in etat.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *mot = mot.wrapping_add(travail);
    }
}

#[cfg(test)]
mod tests;
