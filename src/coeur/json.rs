//! Du JSON écrit et lu à la main avec la std seule (RFC 8259, UTF-8) : la forme plate d'une ligne du fil v2, et le
//! document entier, imbriqué, d'une ligne de Claude Code en stream-json.
//!
//! [`ecrire`] écrit un objet dont les valeurs sont des chaînes, des entiers positifs, des booléens et `null`
//! ([`Valeur`]), seule forme du fil v2. [`lire`] est plus large, au plus petit : une valeur imbriquée (objet, tableau)
//! ou un autre nombre est lue jusqu'au bout puis oubliée, rendue `None`, pour que l'enveloppe d'une ligne se lise
//! toujours, même quand un écrivain a imbriqué. [`document`], par le même lecteur, garde tout ([`Json`]) : chaque
//! valeur imbriquée, chaque nombre tel qu'écrit ; une clé redite y est refusée à tout niveau.
//!
//! Une liste s'écrit dans une chaîne, un élément par ligne (`\n`) ; la chaîne vide veut dire aucun élément.

// raccourci : l'écrivain n'écrit que des objets plats, seule forme du fil v2, et un document se lit tout en
// mémoire, ses nombres gardés en texte ; monter vers serde_json avec une version du fil qui imbriquerait, ou
// quand un document devra se lire en flux, se convertir en types ou faire calculer ses nombres

use std::collections::BTreeMap;

/// Ce qu'une ligne du fil v2 porte. `Entier` : un entier positif (0 compris), sans signe, sans zéro de tête, sans
/// décimale ni exposant, qui tient en `u64`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Valeur {
    Texte(String),
    Entier(u64),
    Booleen(bool),
    Nul,
}

/// Un document JSON lu par [`document`], chaque valeur gardée. `Nombre` est le nombre tel qu'il est écrit, sans
/// conversion ; `Objet` range ses membres par clé.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Nul,
    Booleen(bool),
    Nombre(String),
    Texte(String),
    Liste(Vec<Json>),
    Objet(BTreeMap<String, Json>),
}

/// Les niveaux d'imbrication d'une ligne ou d'un document, le sien compris : l'objet de la ligne, ou le premier objet
/// ou tableau du document, est le premier.
const PROFONDEUR: usize = 64;

/// Un objet plat sur une ligne, sans blanc : les clés dans l'ordre donné, chaque clé et chaque texte passés par
/// [`chaine`]. Les clés doivent être distinctes : [`lire`] refuse une clé redite.
pub(crate) fn ecrire(champs: &[(&str, Valeur)]) -> String {
    let mut ligne = String::from("{");
    for (i, (cle, valeur)) in champs.iter().enumerate() {
        if i > 0 {
            ligne.push(',');
        }
        ligne.push_str(&chaine(cle));
        ligne.push(':');
        match valeur {
            Valeur::Texte(texte) => ligne.push_str(&chaine(texte)),
            Valeur::Entier(n) => ligne.push_str(&n.to_string()),
            Valeur::Booleen(b) => ligne.push_str(if *b { "true" } else { "false" }),
            Valeur::Nul => ligne.push_str("null"),
        }
    }
    ligne.push('}');
    ligne
}

/// Une chaîne JSON, guillemets compris. S'échappent le guillemet, la barre oblique inverse et les contrôles de U+0000 à
/// U+001F : `\n`, `\r`, `\t`, `\b` et `\f` en deux caractères, les autres en `\u00xx`. Tout le reste passe tel quel,
/// accents, `/` et U+007F compris.
pub(crate) fn chaine(texte: &str) -> String {
    let mut json = String::with_capacity(texte.len() + 2);
    json.push('"');
    for c in texte.chars() {
        match c {
            '"' => json.push_str(r#"\""#),
            '\\' => json.push_str(r"\\"),
            '\n' => json.push_str(r"\n"),
            '\r' => json.push_str(r"\r"),
            '\t' => json.push_str(r"\t"),
            '\u{8}' => json.push_str(r"\b"),
            '\u{c}' => json.push_str(r"\f"),
            c if c < ' ' => json.push_str(&format!(r"\u{:04x}", u32::from(c))),
            c => json.push(c),
        }
    }
    json.push('"');
    json
}

/// Lit une ligne : un objet JSON, ses valeurs par clé. Une valeur plate se rend `Some` ; toute autre valeur bien
/// formée (un objet, un tableau, un nombre qui n'est pas un entier de `u64` sans signe, sans décimale ni exposant)
/// est lue jusqu'au bout puis oubliée : `None`.
///
/// Sont refusés, avec le caractère fautif : une clé redite au premier niveau (comparée une fois décodée), ce que la
/// RFC 8259 n'écrit pas (nombre à zéro de tête, virgule finale, contrôle brut ou échappement inconnu dans une chaîne,
/// substitut seul), plus de 64 niveaux d'imbrication et toute suite après le `}` hors blancs. Les blancs sont ceux de
/// la RFC : espace, tabulation, `\n` et `\r`.
pub(crate) fn lire(ligne: &str) -> Result<BTreeMap<String, Option<Valeur>>, String> {
    let mut lecteur = Lecteur { texte: ligne, pos: 0, redites: 1 };
    lecteur.blancs();
    lecteur.passer(b'{', "an object `{`")?;
    let objet = lecteur.objet(1)?;
    lecteur.blancs();
    if lecteur.pos < ligne.len() {
        return Err(lecteur.refus("the end of the line after `}`"));
    }
    Ok(objet.into_iter().map(|(cle, valeur)| (cle, plate(valeur))).collect())
}

/// Lit un document entier : une valeur JSON (un objet, un tableau ou une valeur seule) entourée de blancs, chaque
/// valeur gardée. Sont refusés, comme par [`lire`] et avec le caractère fautif, ce que la RFC 8259 n'écrit pas, plus de
/// 64 niveaux d'imbrication et toute suite après la valeur hors blancs ; et une clé redite, à tout niveau.
pub(crate) fn document(ligne: &str) -> Result<Json, String> {
    let mut lecteur = Lecteur { texte: ligne, pos: 0, redites: PROFONDEUR };
    lecteur.blancs();
    let document = lecteur.valeur(0)?;
    lecteur.blancs();
    if lecteur.pos < ligne.len() {
        return Err(lecteur.refus("the end of the document"));
    }
    Ok(document)
}

/// Ce que [`lire`] rend d'une valeur : elle-même si elle est plate, `None` si elle est imbriquée ou si c'est un nombre
/// qui n'est pas un entier de `u64` sans signe, sans décimale ni exposant.
fn plate(valeur: Json) -> Option<Valeur> {
    match valeur {
        Json::Texte(texte) => Some(Valeur::Texte(texte)),
        // `parse` ne prend que des chiffres seuls dans `u64` : ni signe, ni décimale, ni exposant, ni dépassement.
        Json::Nombre(nombre) => nombre.parse().ok().map(Valeur::Entier),
        Json::Booleen(b) => Some(Valeur::Booleen(b)),
        Json::Nul => Some(Valeur::Nul),
        Json::Liste(_) | Json::Objet(_) => None,
    }
}

/// Où en est la lecture d'une ligne : `pos` est un octet de `texte`, jamais au-delà de sa fin. Chaque pas avance sur de
/// l'ASCII ou sur un bloc fini par de l'ASCII, donc `pos` reste sur une frontière de caractère. Une clé redite est
/// refusée dans un objet de niveau `redites` au plus : 1 pour [`lire`], qui oublie l'imbriqué, tous pour [`document`].
struct Lecteur<'a> {
    texte: &'a str,
    pos: usize,
    redites: usize,
}

impl Lecteur<'_> {
    fn octet(&self) -> Option<u8> {
        self.texte.as_bytes().get(self.pos).copied()
    }

    fn commence(&self, motif: &str) -> bool {
        self.texte.as_bytes().get(self.pos..).is_some_and(|reste| reste.starts_with(motif.as_bytes()))
    }

    fn blancs(&mut self) {
        while matches!(self.octet(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn passer(&mut self, c: u8, attendu: &str) -> Result<(), String> {
        if self.octet() != Some(c) {
            return Err(self.refus(attendu));
        }
        self.pos += 1;
        Ok(())
    }

    /// Le rang, à partir de 1, du caractère qui commence à l'octet `pos`.
    fn rang(&self, pos: usize) -> usize {
        self.texte.char_indices().take_while(|&(i, _)| i < pos).count() + 1
    }

    /// Un refus qui dit ce qu'il fallait, où, et ce qui s'y trouvait.
    fn refus(&self, attendu: &str) -> String {
        let vu = match self.texte.get(self.pos..).and_then(|reste| reste.chars().next()) {
            Some(c) => format!("{c:?}"),
            None => "the end of the line".into(),
        };
        format!("at character {}: expected {attendu}, saw {vu}", self.rang(self.pos))
    }

    /// Ce qui suit un ouvrant déjà passé, jusqu'à `fermant` : des éléments que `element` lit, séparés par des virgules.
    fn suite(&mut self, fermant: u8, mut element: impl FnMut(&mut Self) -> Result<(), String>) -> Result<(), String> {
        self.blancs();
        if self.octet() == Some(fermant) {
            self.pos += 1;
            return Ok(());
        }
        loop {
            self.blancs();
            element(self)?;
            self.blancs();
            match self.octet() {
                Some(b',') => self.pos += 1,
                Some(c) if c == fermant => {
                    self.pos += 1;
                    return Ok(());
                }
                _ => return Err(self.refus(&format!("`,` or `{}`", char::from(fermant)))),
            }
        }
    }

    /// Les membres d'un objet de niveau `niveau`, son `{` passé, jusqu'à son `}`. Plus bas que le niveau `redites`, une
    /// clé redite garde sa dernière valeur.
    fn objet(&mut self, niveau: usize) -> Result<BTreeMap<String, Json>, String> {
        let mut objet = BTreeMap::new();
        self.suite(b'}', |l| {
            let (cle, valeur) = l.membre(niveau)?;
            if niveau <= l.redites && objet.contains_key(&cle) {
                return Err(format!("repeated key: {cle:?}"));
            }
            objet.insert(cle, valeur);
            Ok(())
        })?;
        Ok(objet)
    }

    /// `"clé" : valeur`, `niveau` étant celui de l'objet qui la porte.
    fn membre(&mut self, niveau: usize) -> Result<(String, Json), String> {
        self.passer(b'"', "a key (a string)")?;
        let cle = self.lire_chaine()?;
        self.blancs();
        self.passer(b':', "`:`")?;
        self.blancs();
        Ok((cle, self.valeur(niveau)?))
    }

    /// Une valeur, `niveau` étant celui de l'objet ou du tableau qui la porte, 0 pour la valeur du document.
    fn valeur(&mut self, niveau: usize) -> Result<Json, String> {
        match self.octet() {
            Some(b'"') => {
                self.pos += 1;
                Ok(Json::Texte(self.lire_chaine()?))
            }
            Some(b'{' | b'[') if niveau >= PROFONDEUR => {
                Err(format!("more than {PROFONDEUR} levels of nesting, at character {}", self.rang(self.pos)))
            }
            Some(b'{') => {
                self.pos += 1;
                Ok(Json::Objet(self.objet(niveau + 1)?))
            }
            Some(b'[') => {
                self.pos += 1;
                let mut liste = Vec::new();
                self.suite(b']', |l| {
                    liste.push(l.valeur(niveau + 1)?);
                    Ok(())
                })?;
                Ok(Json::Liste(liste))
            }
            Some(b'-' | b'0'..=b'9') => self.nombre(),
            Some(b't') => self.litteral("true", Json::Booleen(true)),
            Some(b'f') => self.litteral("false", Json::Booleen(false)),
            Some(b'n') => self.litteral("null", Json::Nul),
            _ => Err(self.refus("a value")),
        }
    }

    fn litteral(&mut self, mot: &str, valeur: Json) -> Result<Json, String> {
        if !self.commence(mot) {
            return Err(self.refus(&format!("`{mot}`")));
        }
        self.pos += mot.len();
        Ok(valeur)
    }

    /// Un nombre de la grammaire JSON, tel qu'il est écrit.
    fn nombre(&mut self) -> Result<Json, String> {
        let debut = self.pos;
        if self.octet() == Some(b'-') {
            self.pos += 1;
        }
        match self.octet() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.chiffres()?,
            _ => return Err(self.refus("a digit")),
        }
        if self.octet() == Some(b'.') {
            self.pos += 1;
            self.chiffres()?;
        }
        if matches!(self.octet(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.octet(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.chiffres()?;
        }
        Ok(Json::Nombre(self.texte[debut..self.pos].into()))
    }

    fn chiffres(&mut self) -> Result<(), String> {
        let debut = self.pos;
        while self.octet().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.pos == debut { Err(self.refus("a digit")) } else { Ok(()) }
    }

    /// Le texte d'une chaîne dont le guillemet ouvrant est passé, guillemet fermant compris.
    fn lire_chaine(&mut self) -> Result<String, String> {
        let mut texte = String::new();
        loop {
            let debut = self.pos;
            while self.octet().is_some_and(|b| b != b'"' && b != b'\\' && b >= b' ') {
                self.pos += 1;
            }
            texte.push_str(&self.texte[debut..self.pos]);
            match self.octet() {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(texte);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    texte.push(self.echappement()?);
                }
                Some(controle) => {
                    let (vu, rang) = (char::from(controle), self.rang(self.pos));
                    return Err(format!("raw control {vu:?} in a string (to write as \\u00xx), at character {rang}"));
                }
                None => return Err(self.refus("a closing `\"`")),
            }
        }
    }

    /// Le caractère qu'écrit l'échappement dont la barre oblique inverse est passée.
    fn echappement(&mut self) -> Result<char, String> {
        let c = match self.octet() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.pos += 1;
                return self.unicode();
            }
            _ => return Err(self.refus(r#"an escape (`\` then `"`, `\`, `/`, `b`, `f`, `n`, `r`, `t` or `u`)"#)),
        };
        self.pos += 1;
        Ok(c)
    }

    /// Le caractère de `\uXXXX`, `\u` passé. Un substitut haut (D800 à DBFF) veut son substitut bas (DC00 à DFFF) juste
    /// après, lui aussi en `\uXXXX` : la paire fait un caractère hors du plan de base. Un substitut seul est refusé.
    fn unicode(&mut self) -> Result<char, String> {
        let debut = self.pos - 2;
        let haut = self.hex4()?;
        let seul = |lecteur: &Self| format!("lone surrogate \\u{haut:04x}, at character {}", lecteur.rang(debut));
        let mut code = haut;
        if (0xD800..0xDC00).contains(&haut) {
            // 0 : pas de `\u` derrière, donc pas de moitié basse.
            let bas = if self.commence(r"\u") {
                self.pos += 2;
                self.hex4()?
            } else {
                0
            };
            if !(0xDC00..0xE000).contains(&bas) {
                return Err(seul(self));
            }
            code = 0x10000 + ((haut - 0xD800) << 10) + (bas - 0xDC00);
        }
        // Un substitut bas seul n'est pas un caractère : `from_u32` le refuse.
        char::from_u32(code).ok_or_else(|| seul(self))
    }

    /// Quatre chiffres hexadécimaux, majuscules ou minuscules.
    fn hex4(&mut self) -> Result<u32, String> {
        let mut code = 0;
        for _ in 0..4 {
            let Some(chiffre) = self.octet().and_then(|b| char::from(b).to_digit(16)) else {
                return Err(self.refus("a hexadecimal digit"));
            };
            code = code * 16 + chiffre;
            self.pos += 1;
        }
        Ok(code)
    }
}

#[cfg(test)]
mod tests;
