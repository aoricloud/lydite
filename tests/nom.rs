//! Le test du nom : le programme s'appelle Lydite partout où l'utilisateur le voit. Aucun littéral de chaîne de `src/`
//! (hors les fichiers `tests.rs` et les dossiers `tests/`) ne dit « kraken », en toute casse, et l'usage du programme
//! dit `lydite` et jamais `kraken`. Les commentaires et les identifiants gardent le nom interne.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Un littéral de chaîne : le rang de son octet de départ (après le guillemet ouvrant) et son contenu tel qu'écrit.
#[derive(Debug, PartialEq)]
struct Litteral {
    debut: usize,
    texte: String,
}

/// Les littéraux de chaîne d'un source Rust : les chaînes ordinaires, d'octets et brutes, sans les commentaires ni
/// les caractères (`'"'` n'ouvre rien).
fn litteraux(source: &str) -> Vec<Litteral> {
    let o = source.as_bytes();
    let (mut i, mut trouves) = (0, Vec::new());
    let ident = |j: usize| j > 0 && (o[j - 1].is_ascii_alphanumeric() || o[j - 1] == b'_');
    while i < o.len() {
        if o[i..].starts_with(b"//") {
            while i < o.len() && o[i] != b'\n' {
                i += 1;
            }
        } else if o[i..].starts_with(b"/*") {
            let mut profondeur = 0;
            while i < o.len() {
                if o[i..].starts_with(b"/*") {
                    profondeur += 1;
                    i += 2;
                } else if o[i..].starts_with(b"*/") {
                    profondeur -= 1;
                    i += 2;
                    if profondeur == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if (o[i] == b'r' || o[i..].starts_with(b"br")) && !ident(i) && brute(o, i).is_some() {
            let (debut, fin_texte, fin) = brute(o, i).unwrap();
            trouves.push(Litteral { debut, texte: source[debut..fin_texte].to_string() });
            i = fin;
        } else if o[i] == b'"' || (o[i..].starts_with(b"b\"") && !ident(i)) {
            let debut = if o[i] == b'"' { i + 1 } else { i + 2 };
            let mut j = debut;
            while j < o.len() && o[j] != b'"' {
                j += if o[j] == b'\\' { 2 } else { 1 };
            }
            let fin = j.min(o.len());
            trouves.push(Litteral { debut, texte: source[debut..fin].to_string() });
            i = fin + 1;
        } else if o[i] == b'\'' {
            i = apres_apostrophe(source, i);
        } else {
            i += 1;
        }
    }
    trouves
}

/// Une chaîne brute (`r"…"`, `r#"…"#`, `br"…"`) qui commence en `i` : (début du texte, fin du texte, fin du littéral).
fn brute(o: &[u8], i: usize) -> Option<(usize, usize, usize)> {
    let mut j = if o[i] == b'b' { i + 2 } else { i + 1 };
    let dieses = o[j..].iter().take_while(|&&c| c == b'#').count();
    j += dieses;
    if o.get(j) != Some(&b'"') {
        return None;
    }
    let debut = j + 1;
    let mut fermeture = vec![b'"'];
    fermeture.extend(std::iter::repeat_n(b'#', dieses));
    let fin_texte = (debut..o.len()).find(|&k| o[k..].starts_with(&fermeture))?;
    Some((debut, fin_texte, fin_texte + fermeture.len()))
}

/// Le rang après une apostrophe en `i` : celui d'un caractère (`'a'`, `'"'`, `'\''`, `'\u{e9}'`) ou, pour une durée de
/// vie (`'a`), juste après l'apostrophe.
fn apres_apostrophe(source: &str, i: usize) -> usize {
    let reste = &source[i + 1..];
    let mut caracteres = reste.char_indices();
    match caracteres.next() {
        Some((_, '\\')) => match reste.find('\'').map(|k| k + 1) {
            // `'\''` : l'apostrophe échappée ne ferme pas.
            Some(k) if reste[..k].ends_with("\\'") && reste[k..].starts_with('\'') => i + 1 + k + 1,
            Some(k) => i + 1 + k,
            None => i + 1,
        },
        Some((_, c)) if reste[c.len_utf8()..].starts_with('\'') => i + 1 + c.len_utf8() + 1,
        _ => i + 1,
    }
}

/// Les fautes d'un source : « fichier:ligne » de chaque littéral qui dit kraken, en toute casse.
fn fautes(nom: &str, source: &str) -> Vec<String> {
    let mut fautes = Vec::new();
    for litteral in litteraux(source) {
        if let Some(rang) = litteral.texte.to_lowercase().find("kraken") {
            let ligne =
                source[..litteral.debut].matches('\n').count() + litteral.texte[..rang].matches('\n').count() + 1;
            fautes.push(format!("{nom}:{ligne}"));
        }
    }
    fautes
}

/// Les fichiers `.rs` de `dossier`, sans les fichiers `tests.rs` ni les dossiers `tests/`.
fn sources(dossier: &Path, trouves: &mut Vec<PathBuf>) {
    for entree in fs::read_dir(dossier).unwrap() {
        let chemin = entree.unwrap().path();
        let nom = chemin.file_name().unwrap().to_string_lossy().to_string();
        if chemin.is_dir() {
            if nom != "tests" {
                sources(&chemin, trouves);
            }
        } else if nom.ends_with(".rs") && nom != "tests.rs" {
            trouves.push(chemin);
        }
    }
}

#[test]
fn aucun_litteral_de_src_ne_dit_kraken() {
    let racine = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut fichiers = Vec::new();
    sources(&racine.join("src"), &mut fichiers);
    assert!(!fichiers.is_empty());
    fichiers.sort();
    let mut tous = Vec::new();
    for fichier in fichiers {
        let nom = fichier.strip_prefix(racine).unwrap().display().to_string().replace('\\', "/");
        tous.extend(fautes(&nom, &fs::read_to_string(&fichier).unwrap()));
    }
    assert!(tous.is_empty(), "des littéraux disent kraken :\n{}", tous.join("\n"));
}

#[test]
fn l_usage_dit_lydite_et_jamais_kraken() {
    let sortie = Command::new(env!("CARGO_BIN_EXE_lydite")).output().unwrap();
    let dit = format!("{}{}", String::from_utf8_lossy(&sortie.stdout), String::from_utf8_lossy(&sortie.stderr));
    assert!(dit.contains("lydite lancer"), "{dit}");
    assert!(!dit.to_lowercase().contains("kraken"), "{dit}");
}

fn textes(source: &str) -> Vec<String> {
    litteraux(source).into_iter().map(|l| l.texte).collect()
}

#[test]
fn un_commentaire_qui_dit_kraken_ne_compte_pas() {
    let source = "// kraken ici\n/* kraken /* et là */ encore kraken */\n/// Kraken\nlet a = 1; // \"kraken\"\n";
    assert_eq!(fautes("f.rs", source), Vec::<String>::new());
    assert!(litteraux(source).is_empty());
}

#[test]
fn une_chaine_qui_dit_kraken_compte_avec_sa_ligne() {
    let source = "fn a() {}\nlet a = \"un Kraken\";\nlet b = \"ok\";\nlet c = \"KRAKEN\";\n";
    assert_eq!(fautes("f.rs", source), ["f.rs:2", "f.rs:4"]);
}

#[test]
fn une_chaine_sur_plusieurs_lignes_compte() {
    let source = "let a = \"début \\\n    puis kraken\";\nlet b = \"x\\\n    y\";\n";
    assert_eq!(textes(source), ["début \\\n    puis kraken", "x\\\n    y"]);
    assert_eq!(fautes("f.rs", source), ["f.rs:2"]);
}

#[test]
fn une_chaine_brute_et_une_chaine_d_octets_comptent() {
    let source = "let a = r#\"un \"kraken\" brut\"#;\nlet b = b\"kraken\";\nlet c = br\"kraken\";\nlet d = r\"ok\";\n";
    assert_eq!(textes(source), ["un \"kraken\" brut", "kraken", "kraken", "ok"]);
    assert_eq!(fautes("f.rs", source), ["f.rs:1", "f.rs:2", "f.rs:3"]);
}

#[test]
fn un_guillemet_echappe_ne_ferme_pas_la_chaine() {
    let source = "let a = \"dit \\\"kraken\\\" ici\";\n";
    assert_eq!(textes(source), ["dit \\\"kraken\\\" ici"]);
    assert_eq!(fautes("f.rs", source), ["f.rs:1"]);
}

#[test]
fn un_caractere_guillemet_n_ouvre_rien() {
    let source = "let a = '\"';\nlet b = \"ok\";\nlet c = '\\'';\nlet d = '\\\"';\nlet e = \"kraken\";\n";
    assert_eq!(textes(source), ["ok", "kraken"]);
    assert_eq!(fautes("f.rs", source), ["f.rs:5"]);
}

#[test]
fn une_duree_de_vie_n_ouvre_rien() {
    let source = "fn a<'a>(x: &'a str) -> &'a str { x }\nlet b = \"kraken\";\n";
    assert_eq!(textes(source), ["kraken"]);
}

#[test]
fn un_identifiant_qui_finit_par_r_n_ouvre_pas_de_chaine_brute() {
    let source = "let ar = 1; let b = \"ok\"; let br = 2;\n";
    assert_eq!(textes(source), ["ok"]);
}
