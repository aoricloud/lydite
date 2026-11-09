//! Le fichier du relecteur, `REMARQUES` :
//!
//! ```text
//! remarques 1
//!
//! remarque 1
//! fichier : <chemin>
//! ligne : <entier>
//! bloquante : oui
//!     <le constat puis la correction, en lignes indentées>
//! ```
//!
//! - 1 Mo au plus ([`TAILLE_MAX`] octets), pesé avant d'être lu ; de l'UTF-8, un BOM de tête toléré ; les lignes se
//!   coupent sur `\n`, un `\r` final ôté ;
//! - la première ligne est `remarques 1` ou `remarques 2`, seule : en forme 1, un fichier qui ne porte qu'elle dit
//!   qu'il n'y a aucune remarque ;
//! - ensuite, en colonne 0, une ligne est vide (ou faite de blancs), `remarque <n>`, ou `<clé> : <valeur>`, les blancs
//!   ôtés autour du `:` ; une ligne indentée (espaces ou tabulations) est le texte de la remarque en cours ;
//! - les remarques sont numérotées 1, 2, 3… dans l'ordre, au plus [`NOMBRE_MAX`] (la borne des remarques d'une
//!   relecture au fil) ; chacune a ses clés dans cet ordre, chacune une fois : `fichier`, un chemin relatif à la
//!   racine du dépôt, écrit avec `/`, sans `..` ; `ligne`, facultative, un entier positif ; `bloquante`, `oui` ou
//!   `non` ; puis son texte, au moins une ligne indentée, dont l'indentation commune est ôtée et les lignes vides du
//!   milieu gardées.
//!
//! La forme 2, que le relecteur écrit, est la forme 1 sous `remarques 2`, puis, après la dernière remarque ou seuls,
//! deux blocs obligatoires dans cet ordre, `rejoue` puis `non rejoue` (en colonne 0, seuls sur leur ligne), chacun
//! suivi d'au moins une ligne indentée : le premier dit ce que montre la preuve rejouée ; le second, ce que le travail
//! affirme ou demande et que rien de rejoué ici ne montre (une autre plateforme, le réseau, un chemin qu'aucun test
//! ne joue). Un bloc manquant, vide, hors d'ordre ou en double est refusé. [`lire_fichier`] lit les deux formes,
//! avec les mêmes bornes et les mêmes refus ; [`Fichier::exiger_la_forme_2`] écarte la forme 1.
//!
//! Deux refus en disent plus que la forme ne l'exige, sans rien changer à ce qui passe ni à ce qui est refusé : une
//! première ligne `remarques <chiffres>` autre que `remarques 1` et `remarques 2`, et un bloc `rejoue` ou `non rejoue`
//! sous `remarques 1`, rappellent que le chiffre est la forme du fichier et non un nombre de remarques ; le refus de
//! [`Fichier::exiger_la_forme_2`] le redit.
//!
//! Toute faute est le refus 7015, qui nomme la ligne qu'il vise. Aucune entrée ne fait paniquer la lecture : un fichier
//! de main est une frontière de confiance, et le lire strictement est ce qui la garde.

use std::fmt::Display;
use std::iter;
use std::str;

use crate::coeur::erreur::{Code, Refus};
use crate::coeur::fil::NOMBRE_MAX;
use crate::coeur::plan::TAILLE_MAX;

/// La forme d'une remarque, que la consigne du relecteur porte, telle que l'en-tête de ce module la donne.
pub(crate) const FORME: &str = concat!(
    "remarques 2\n",
    "\n",
    "remarque 1\n",
    "fichier : <chemin>\n",
    "ligne : <entier>\n",
    "bloquante : oui\n",
    "    <le constat puis la correction, en lignes indentées>\n",
    "\n",
    "rejoue\n",
    "    <ce que montre la preuve rejouée par Lydite>\n",
    "\n",
    "non rejoue\n",
    "    <ce que le travail affirme ou demande et que rien de rejoué ici ne montre>",
);

/// Les clés d'une remarque, dans l'ordre où elles s'écrivent.
const CLES: [&str; 3] = ["fichier", "ligne", "bloquante"];

/// Les caractères d'une ligne qu'une preuve cite : au-delà, la citation est coupée.
const CITE_MAX: usize = 200;

/// La correction d'un fichier de remarques hors de sa forme.
const A_ECRIRE: &str = concat!(
    "Un fichier de remarques commence par `remarques 2` (ou `remarques 1`), puis chaque remarque s'écrit ",
    "`remarque <n>` (1, 2, 3… dans l'ordre), `fichier : <chemin>`, `ligne : <entier>` (facultative), ",
    "`bloquante : oui` ou `non`, puis son constat et sa correction en lignes indentées ; sous `remarques 2`, les ",
    "blocs `rejoue` puis `non rejoue` suivent, chacun d'au moins une ligne indentée."
);

/// Les deux blocs de la forme 2, dans l'ordre où ils s'écrivent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bloc {
    Rejoue,
    NonRejoue,
}

impl Bloc {
    fn nom(self) -> &'static str {
        match self {
            Bloc::Rejoue => "rejoue",
            Bloc::NonRejoue => "non rejoue",
        }
    }
}

/// Un fichier de remarques lu, dans l'une ou l'autre forme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fichier {
    /// 1 ou 2.
    pub(crate) forme: u8,
    pub(crate) remarques: Vec<Remarque>,
    /// Les lignes du bloc `rejoue`, sans leur indentation ; vide en forme 1.
    pub(crate) rejoue: Vec<String>,
    /// Les lignes du bloc `non rejoue`, sans leur indentation ; vide en forme 1.
    pub(crate) non_rejoue: Vec<String>,
}

/// Ce que disent les refus quand le chiffre de la première ligne a été pris pour un nombre de remarques.
const RAPPEL_FORME: &str = "Le chiffre de la première ligne est la forme du fichier, jamais un nombre de remarques : \
                            le relecteur écrit `remarques 2` quel que soit le nombre de ses remarques, même aucune.";

impl Fichier {
    /// Le fichier s'il est en forme 2, sinon le refus 7015 de sa première ligne.
    pub(crate) fn exiger_la_forme_2(self) -> Result<Self, Refus> {
        if self.forme == 2 {
            return Ok(self);
        }
        let premiere = Ligne { numero: 1, texte: "remarques 1" };
        let quoi = "le relecteur écrit la forme 2, `remarques 2`, avec ses blocs `rejoue` et `non rejoue`";
        let correction = format!(
            "Écris `remarques 2` en première ligne, puis tes remarques, puis les blocs `rejoue` et `non rejoue`, \
             chacun d'au moins une ligne indentée. {RAPPEL_FORME}"
        );
        Err(refus(premiere, quoi, correction))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Remarque {
    /// Le chemin que la remarque vise, relatif à la racine du dépôt.
    pub(crate) fichier: String,
    pub(crate) ligne: Option<u32>,
    /// Elle vaut une correction.
    pub(crate) bloquante: bool,
    /// Le constat puis la correction, en lignes : l'indentation commune ôtée.
    pub(crate) texte: String,
}

/// Une ligne du fichier : son numéro, à partir de 1, et son texte sans le `\r` final.
#[derive(Clone, Copy, Default)]
struct Ligne<'a> {
    numero: usize,
    texte: &'a str,
}

/// Lit un fichier de remarques dans ses octets : ses remarques dans l'ordre, ou le refus 7015 de la première faute
/// (voir l'en-tête du module pour la forme). Aucune entrée ne fait paniquer la lecture.
#[cfg(test)]
pub(crate) fn lire_remarques(octets: &[u8]) -> Result<Vec<Remarque>, Refus> {
    lire_fichier(octets).map(|fichier| fichier.remarques)
}

/// Lit un fichier de remarques, de la forme 1 ou 2 : ses remarques, et, en forme 2, ses deux blocs. Mêmes bornes et
/// mêmes refus que la lecture de ses seules remarques.
pub(crate) fn lire_fichier(octets: &[u8]) -> Result<Fichier, Refus> {
    if octets.len() > TAILLE_MAX {
        let preuve = format!("le fichier de remarques dépasse 1 Mo : plus de {TAILLE_MAX} octets");
        let correction = "Allège le fichier sous 1 Mo : garde les remarques qui comptent, chacune en peu de lignes.";
        return Err(Refus::new(Code::RemarquesIllisibles, preuve, correction));
    }
    let texte = en_utf8(octets)?;
    let coupees = texte.split('\n').map(|ligne| ligne.strip_suffix('\r').unwrap_or(ligne));
    let mut lignes = (1..).zip(coupees).map(|(numero, texte)| Ligne { numero, texte });
    let premiere = lignes.next().unwrap_or(Ligne { numero: 1, texte: "" });
    let forme = match premiere.texte {
        "remarques 1" => 1,
        "remarques 2" => 2,
        _ => {
            let mut correction = String::from(
                "Écris `remarques 2` seul en première ligne (ou `remarques 1`) : Lydite ne lit que ces formes, avant \
                 tout le reste.",
            );
            let chiffres = premiere.texte.strip_prefix("remarques ");
            if chiffres.is_some_and(|c| !c.is_empty() && c.bytes().all(|b| b.is_ascii_digit())) {
                correction.push_str(&format!(" {RAPPEL_FORME}"));
            }
            let quoi = "la première ligne d'un fichier de remarques est `remarques 1` ou `remarques 2`";
            return Err(refus(premiere, quoi, correction));
        }
    };
    let mut lecteur = Lecteur { forme, derniere: premiere, ..Lecteur::default() };
    for ligne in lignes {
        lecteur.derniere = ligne;
        lecteur.lire(ligne)?;
    }
    lecteur.finir()
}

#[derive(Default)]
struct Lecteur<'a> {
    forme: u8,
    /// La dernière ligne du fichier, que nomme un bloc qui manque.
    derniere: Ligne<'a>,
    finies: Vec<Remarque>,
    courante: Option<Brouillon<'a>>,
    /// Le bloc en cours (forme 2) : sa ligne d'en-tête, son nom, ses lignes.
    bloc: Option<(Ligne<'a>, Bloc, Vec<String>)>,
    rejoue: Option<Vec<String>>,
    non_rejoue: Option<Vec<String>>,
}

/// Une remarque en cours de lecture : ses clés, à mesure qu'elles paraissent, puis son texte.
struct Brouillon<'a> {
    /// Sa ligne `remarque <n>`.
    entete: Ligne<'a>,
    fichier: Option<&'a str>,
    ligne: Option<u32>,
    bloquante: Option<bool>,
    /// Ses lignes indentées, chacune avec le nombre de retours à la ligne qui la séparent de la précédente : 0 pour la
    /// première, puis 1 de plus par ligne vide entre elles.
    texte: Vec<(usize, &'a str)>,
    /// Les lignes vides vues depuis la dernière ligne indentée : elles ne comptent que si une autre ligne suit.
    vides: usize,
}

impl<'a> Lecteur<'a> {
    /// Lit une ligne après la première.
    fn lire(&mut self, ligne: Ligne<'a>) -> Result<(), Refus> {
        let texte = ligne.texte;
        if texte.trim().is_empty() {
            // Une ligne vide ne dit rien, sauf entre deux lignes du texte d'une remarque.
            if let Some(remarque) = self.courante.as_mut().filter(|remarque| !remarque.texte.is_empty()) {
                remarque.vides += 1;
            }
            return Ok(());
        }
        if texte.starts_with([' ', '\t']) {
            if let Some((_, _, lignes)) = self.bloc.as_mut() {
                lignes.push(texte.trim().to_string());
                return Ok(());
            }
            return self.indentee(ligne);
        }
        match (self.forme, texte.trim_end()) {
            (2, "rejoue") => return self.ouvrir(ligne, Bloc::Rejoue),
            (2, "non rejoue") => return self.ouvrir(ligne, Bloc::NonRejoue),
            (1, "rejoue" | "non rejoue") => {
                let quoi = "les blocs `rejoue` et `non rejoue` ne se lisent que sous `remarques 2`, et la première \
                            ligne de ce fichier est `remarques 1`";
                let correction = format!("Écris `remarques 2` en première ligne. {RAPPEL_FORME}");
                return Err(refus(ligne, quoi, correction));
            }
            _ => {}
        }
        match texte.strip_prefix("remarque").filter(|numero| numero.is_empty() || numero.starts_with([' ', '\t'])) {
            Some(numero) => self.remarque(ligne, numero.trim()),
            None => self.cle(ligne),
        }
    }

    /// Une ligne indentée, non vide : elle continue le texte de la remarque en cours, une fois sa ligne `bloquante`
    /// vue.
    fn indentee(&mut self, ligne: Ligne<'a>) -> Result<(), Refus> {
        let Some(remarque) = self.courante.as_mut() else {
            let quoi = "cette ligne est indentée, et aucune ligne `remarque <n>` ne la précède";
            return Err(refus(ligne, quoi, A_ECRIRE));
        };
        if remarque.bloquante.is_none() {
            let quoi = "le texte d'une remarque vient après ses clés, la dernière étant `bloquante`";
            return Err(refus(ligne, quoi, A_ECRIRE));
        }
        let sauts = if remarque.texte.is_empty() { 0 } else { remarque.vides + 1 };
        remarque.vides = 0;
        remarque.texte.push((sauts, ligne.texte));
        Ok(())
    }

    /// Une ligne `remarque <numero>` : la remarque en cours se ferme, et une autre s'ouvre, de numéro le suivant.
    fn remarque(&mut self, ligne: Ligne<'a>, numero: &str) -> Result<(), Refus> {
        if self.bloc.is_some() || self.rejoue.is_some() {
            return Err(refus(ligne, "une remarque vient avant les blocs `rejoue` et `non rejoue`", A_ECRIRE));
        }
        self.fermer()?;
        let attendu = self.finies.len() + 1;
        if attendu > NOMBRE_MAX {
            let quoi =
                format!("c'est la remarque {attendu}, et un fichier de remarques en compte {NOMBRE_MAX} au plus");
            let correction = "Garde les remarques qui comptent : la relecture n'en note pas davantage au fil.";
            return Err(refus(ligne, quoi, correction));
        }
        let chiffres = !numero.is_empty() && numero.bytes().all(|c| c.is_ascii_digit());
        if !chiffres || numero.parse().ok() != Some(attendu) {
            let quoi = format!("la remarque porte le numéro {}, et la suivante est la {attendu}", cite(numero));
            let correction =
                format!("Écris `remarque {attendu}` : les remarques sont numérotées 1, 2, 3… dans l'ordre.");
            return Err(refus(ligne, quoi, correction));
        }
        let vide =
            Brouillon { entete: ligne, fichier: None, ligne: None, bloquante: None, texte: Vec::new(), vides: 0 };
        self.courante = Some(vide);
        Ok(())
    }

    /// Une ligne `<clé> : <valeur>` : une clé de la remarque en cours, à sa place, une fois, avant le texte, d'une
    /// valeur non blanche de son domaine.
    fn cle(&mut self, ligne: Ligne<'a>) -> Result<(), Refus> {
        let faute = |quoi: String| refus(ligne, quoi, A_ECRIRE);
        let Some((cle, valeur)) = ligne.texte.split_once(':') else {
            return Err(faute("elle n'est ni vide, ni indentée, ni `remarque <n>`, ni `<clé> : <valeur>`".into()));
        };
        let (cle, valeur) = (cle.trim(), valeur.trim());
        if !CLES.contains(&cle) {
            return Err(faute(format!("la clé {} n'est pas une clé de remarque ({})", cite(cle), CLES.join(", "))));
        }
        let Some(remarque) = self.courante.as_mut() else {
            return Err(faute(format!("la clé {} ne suit aucune ligne `remarque <n>`", cite(cle))));
        };
        if !remarque.texte.is_empty() {
            let apres = "vient après les lignes indentées de la remarque, que ses clés précèdent";
            return Err(faute(format!("la clé {} {apres}", cite(cle))));
        }
        // `fichier` vient d'abord, `ligne` la suit si elle paraît, `bloquante` ferme les clés.
        let deja = "est déjà donnée dans cette remarque";
        let place = match cle {
            "fichier" if remarque.fichier.is_some() => Some(deja),
            "fichier" => None,
            _ if remarque.fichier.is_none() => Some("vient avant `fichier`, la première clé"),
            "ligne" if remarque.ligne.is_some() => Some(deja),
            "ligne" if remarque.bloquante.is_some() => Some("vient après `bloquante`, la dernière clé"),
            "bloquante" if remarque.bloquante.is_some() => Some(deja),
            _ => None,
        };
        if let Some(quoi) = place {
            return Err(faute(format!("la clé {} {quoi}", cite(cle))));
        }
        if valeur.is_empty() {
            let correction = format!("Écris une valeur après `{cle} :`, ou retire la ligne si la clé est facultative.");
            return Err(refus(ligne, format!("la clé {} est blanche", cite(cle)), correction));
        }
        match cle {
            "fichier" => {
                if let Some(faute_de_chemin) = hors_forme(valeur) {
                    let quoi = format!("le chemin {} {faute_de_chemin}", cite(valeur));
                    let correction = "Écris le chemin du fichier visé, relatif à la racine du dépôt, tel que git \
                                      l'écrit : des noms séparés par `/`, sans `..`.";
                    return Err(refus(ligne, quoi, correction));
                }
                remarque.fichier = Some(valeur);
            }
            "ligne" => {
                let numero = valeur.bytes().all(|c| c.is_ascii_digit()).then(|| valeur.parse::<u32>().ok());
                let Some(numero) = numero.flatten().filter(|numero| *numero > 0) else {
                    let quoi = format!("la clé « ligne » vaut {}, et sa valeur est un entier positif", cite(valeur));
                    let correction = "Écris le numéro de la ligne visée, à partir de 1, ou retire la ligne.";
                    return Err(refus(ligne, quoi, correction));
                };
                remarque.ligne = Some(numero);
            }
            _ => {
                remarque.bloquante = Some(match valeur {
                    "oui" => true,
                    "non" => false,
                    autre => {
                        let quoi = format!(
                            "la clé « bloquante » vaut {}, et ses seules valeurs sont `oui` et `non`",
                            cite(autre)
                        );
                        let correction =
                            "Écris `bloquante : oui` si la remarque vaut une correction, `bloquante : non` sinon.";
                        return Err(refus(ligne, quoi, correction));
                    }
                });
            }
        }
        Ok(())
    }

    /// Ferme la remarque en cours, si elle est entière : un `fichier`, un `bloquante`, et au moins une ligne de texte.
    fn fermer(&mut self) -> Result<(), Refus> {
        let Some(Brouillon { entete, fichier, ligne, bloquante, texte, .. }) = self.courante.take() else {
            return Ok(());
        };
        let manque = |quoi: &str| refus(entete, format!("la remarque n'a pas {quoi}"), A_ECRIRE);
        let Some(fichier) = fichier else { return Err(manque("de ligne `fichier : …`")) };
        let Some(bloquante) = bloquante else { return Err(manque("de ligne `bloquante : …`")) };
        if texte.is_empty() {
            return Err(manque("de texte : son constat et sa correction, en lignes indentées"));
        }
        self.finies.push(Remarque { fichier: fichier.into(), ligne, bloquante, texte: texte_de(&texte) });
        Ok(())
    }

    /// Ouvre le bloc `qui` à la ligne `ligne` : la remarque et le bloc en cours se ferment. `rejoue` vient d'abord,
    /// `non rejoue` ensuite, chacun une fois.
    fn ouvrir(&mut self, ligne: Ligne<'a>, qui: Bloc) -> Result<(), Refus> {
        self.fermer()?;
        self.fermer_le_bloc()?;
        let nom = qui.nom();
        let deja = match qui {
            Bloc::Rejoue => self.rejoue.is_some(),
            Bloc::NonRejoue => self.non_rejoue.is_some(),
        };
        if deja {
            return Err(refus(ligne, format!("le bloc `{nom}` est déjà donné"), A_ECRIRE));
        }
        if qui == Bloc::NonRejoue && self.rejoue.is_none() {
            return Err(refus(ligne, "le bloc `non rejoue` vient après le bloc `rejoue`", A_ECRIRE));
        }
        self.bloc = Some((ligne, qui, Vec::new()));
        Ok(())
    }

    /// Ferme le bloc en cours, s'il y en a un : il porte au moins une ligne indentée.
    fn fermer_le_bloc(&mut self) -> Result<(), Refus> {
        let Some((entete, qui, lignes)) = self.bloc.take() else { return Ok(()) };
        if lignes.is_empty() {
            let quoi = format!("le bloc `{}` n'a pas de ligne indentée", qui.nom());
            return Err(refus(entete, quoi, A_ECRIRE));
        }
        match qui {
            Bloc::Rejoue => self.rejoue = Some(lignes),
            Bloc::NonRejoue => self.non_rejoue = Some(lignes),
        }
        Ok(())
    }

    fn finir(mut self) -> Result<Fichier, Refus> {
        self.fermer()?;
        self.fermer_le_bloc()?;
        if self.forme == 2 {
            for (bloc, present) in [(Bloc::Rejoue, self.rejoue.is_some()), (Bloc::NonRejoue, self.non_rejoue.is_some())]
            {
                if !present {
                    let quoi = format!("le fichier n'a pas de bloc `{}`, que la forme 2 exige", bloc.nom());
                    return Err(refus(self.derniere, quoi, A_ECRIRE));
                }
            }
        }
        let (rejoue, non_rejoue) = (self.rejoue.unwrap_or_default(), self.non_rejoue.unwrap_or_default());
        Ok(Fichier { forme: self.forme, remarques: self.finies, rejoue, non_rejoue })
    }
}

/// Le texte du fichier : ses octets en UTF-8, le BOM de tête ôté. Une marque UTF-16 ou un octet hors de l'UTF-8 est le
/// refus 7015, qui dit le rang de l'octet (à partir de 1, BOM compris) et sa ligne.
fn en_utf8(octets: &[u8]) -> Result<&str, Refus> {
    let reecrire = "Réécris le fichier en UTF-8 : sous PowerShell, `(Get-Content <fichier>) | Set-Content -Encoding \
                    utf8 <fichier>` ; ailleurs, enregistre-le en UTF-8 depuis ton éditeur.";
    let marque = match octets {
        [0xFF, 0xFE, ..] => Some("FF FE"),
        [0xFE, 0xFF, ..] => Some("FE FF"),
        _ => None,
    };
    if let Some(marque) = marque {
        let preuve = format!("le fichier commence par les octets {marque}, la marque d'un texte en UTF-16");
        return Err(Refus::new(Code::RemarquesIllisibles, preuve, reecrire));
    }
    let (bom, utf8) = match octets.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(reste) => (3, reste),
        None => (0, octets),
    };
    str::from_utf8(utf8).map_err(|erreur| {
        let avant = erreur.valid_up_to();
        let ligne = utf8[..avant].iter().filter(|&&octet| octet == b'\n').count() + 1;
        let octet = utf8.get(avant).map_or_else(String::new, |octet| format!(" ({octet:#04X})"));
        let preuve = format!("l'octet de rang {}{octet}, ligne {ligne}, n'est pas de l'UTF-8", bom + avant + 1);
        Refus::new(Code::RemarquesIllisibles, preuve, reecrire)
    })
}

/// Ce qui écarte un chemin de `fichier` de la forme que git écrit, s'il y a quelque chose : `None` pour un chemin
/// relatif à la racine du dépôt, en noms séparés par `/` (un `/` final pour un dossier), sans `.` ni `..`.
fn hors_forme(chemin: &str) -> Option<&'static str> {
    if chemin.chars().any(char::is_control) {
        return Some("porte un caractère de contrôle");
    }
    if chemin.contains('\\') {
        return Some("s'écrit avec une barre inverse, et git sépare les noms par `/`");
    }
    let lecteur = matches!(chemin.as_bytes(), [lettre, b':', ..] if lettre.is_ascii_alphabetic());
    if chemin.starts_with('/') || lecteur {
        return Some("est absolu");
    }
    let mut noms = chemin.strip_suffix('/').unwrap_or(chemin).split('/');
    noms.any(|nom| matches!(nom, "" | "." | "..")).then_some("a un nom vide, `.` ou `..`")
}

/// Le texte d'une remarque : ses lignes sans leur indentation commune (la plus longue suite de blancs par quoi elles
/// commencent toutes), chacune après les retours à la ligne qui la séparent de la précédente.
fn texte_de(lignes: &[(usize, &str)]) -> String {
    let commune = lignes
        .iter()
        .map(|(_, ligne)| &ligne[..ligne.len() - ligne.trim_start_matches([' ', '\t']).len()])
        .reduce(|commune, indentation| {
            let egaux = commune.bytes().zip(indentation.bytes()).take_while(|(a, b)| a == b).count();
            &commune[..egaux]
        })
        .unwrap_or_default();
    let mut texte = String::new();
    for &(sauts, ligne) in lignes {
        texte.extend(iter::repeat_n('\n', sauts));
        texte.push_str(ligne.strip_prefix(commune).unwrap_or(ligne));
    }
    texte
}

/// Le refus 7015 d'une ligne du fichier : son numéro et son texte d'abord, puis ce qui ne va pas.
fn refus(ligne: Ligne<'_>, quoi: impl Display, correction: impl Into<String>) -> Refus {
    let preuve = format!("ligne {} {} : {quoi}", ligne.numero, cite(ligne.texte));
    Refus::new(Code::RemarquesIllisibles, preuve, correction)
}

/// Un bout du fichier pour une preuve : entre guillemets français, ses caractères de contrôle écrits (`\t`, `\x1b`), et
/// coupé à `CITE_MAX` caractères. Une ligne peut être immense, ou piéger un terminal.
fn cite(texte: &str) -> String {
    let mut citation = String::from("« ");
    for c in texte.chars().take(CITE_MAX) {
        if c.is_control() {
            citation.extend(c.escape_default());
        } else {
            citation.push(c);
        }
    }
    if texte.chars().nth(CITE_MAX).is_some() {
        citation.push('…');
    }
    citation.push_str(" »");
    citation
}

#[cfg(test)]
mod tests;
