//! `lydite memory` et `lydite memory recall <address>` : l'index du fil, et le rappel d'une de ses lignes, en lecture
//! seule. Ni l'une ni l'autre n'écrit le fil, ne crée un fichier ou un dossier, ne prend le verrou du dossier (comme
//! `lydite status`) ni ne lance une main.
//!
//! Un sceau rompu se juge avant de rien rendre, comme `lydite status` le juge : le premier qui ne tient plus se dit sur
//! la sortie d'erreur par le même texte (`etat::dire_le_sceau_rompu`), rien ne s'écrit sur la sortie, et le code est 2.
//! Le sceau montre une ligne changée ou retirée avant lui ; il ne montre pas une ligne ajoutée après lui.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use super::etat::dire_le_sceau_rompu;
use crate::branchements::dossier::{self, lire_avec_lignes};
use crate::coeur::erreur::{Code, Refus, nommee};
use crate::coeur::fil::Lecture;
use crate::coeur::memoire;

/// `lydite memory` dans le dossier de Lydite, sur la sortie et la sortie d'erreur du processus.
pub(crate) fn memoire_cmd() -> Result<ExitCode, Refus> {
    memoire_ecrivant(&dossier::dossier()?, &mut io::stdout().lock(), &mut io::stderr())
}

/// `lydite memory recall <address>` dans le dossier de Lydite, sur la sortie et la sortie d'erreur du processus.
pub(crate) fn rappel_cmd(texte: &str) -> Result<ExitCode, Refus> {
    rappel_ecrivant(&dossier::dossier()?, texte, &mut io::stdout().lock(), &mut io::stderr())
}

/// L'index du fil de `dossier` (`memoire::index`), écrit d'un seul bloc sur `sortie`, une ligne par ligne lue. Les
/// écarts du lecteur (une ligne sautée, un `seq` rompu) vont sur `erreur`, comme pour `lydite status`. Un fil absent
/// rend un index vide et le code 0 ; un fil d'une autre version est son refus ; un sceau rompu est dit sur `erreur`,
/// avec le code 2 et rien sur `sortie`.
pub(crate) fn memoire_ecrivant(
    dossier: &Path,
    sortie: &mut dyn Write,
    erreur: &mut dyn Write,
) -> Result<ExitCode, Refus> {
    let ((Lecture { ecarts, .. }, _, _, rompu), lignes, octets) = lire_avec_lignes(dossier)?;
    for (_, refus) in &ecarts {
        let _ = writeln!(erreur, "{refus}");
    }
    if let Some(sceau) = rompu {
        dire_le_sceau_rompu(erreur, &sceau);
        return Ok(ExitCode::from(2));
    }
    ecrire_d_un_bloc(sortie, memoire::index(&octets, &lignes).as_bytes())
}

/// La ligne que `texte` nomme dans le fil de `dossier`, écrite sur `sortie` telle quelle, suivie d'un `\n`. Dans cet
/// ordre : un texte sans `/` qui n'est pas une empreinte est le refus 6035, avant toute lecture du fil ; puis le fil se
/// lit (une autre version : son refus) ; puis les sceaux se jugent, et un sceau rompu est dit sur `erreur`, avec le
/// code 2 et rien sur `sortie` ; puis la ligne se cherche, et son absence est le refus 6034. Les écarts du lecteur ne
/// se disent pas ici : `lydite memory` les dit.
pub(crate) fn rappel_ecrivant(
    dossier: &Path,
    texte: &str,
    sortie: &mut dyn Write,
    erreur: &mut dyn Write,
) -> Result<ExitCode, Refus> {
    // Sans ligne à chercher, `rappeler` ne refuse qu'en 6035 un texte qui n'est ni une adresse ni une empreinte, et en
    // 6034 tout autre : seul le premier se juge avant la lecture du fil.
    if let Err(refus) = memoire::rappeler(&[], &[], texte)
        && refus.code == Code::MemoireEmpreinteFausse
    {
        return Err(refus);
    }
    let ((_, _, _, rompu), lignes, octets) = lire_avec_lignes(dossier)?;
    if let Some(sceau) = rompu {
        dire_le_sceau_rompu(erreur, &sceau);
        return Ok(ExitCode::from(2));
    }
    let mut ligne = memoire::rappeler(&octets, &lignes, texte)?.to_vec();
    ligne.push(b'\n');
    ecrire_d_un_bloc(sortie, &ligne)
}

/// Écrit `octets` sur `sortie` d'un seul bloc, comme `skill_cmd` écrit la skill, jamais par `println!`. Un tube fermé
/// par son lecteur (`lydite memory | head`) n'est ni une panique ni un refus : la commande finit en silence, en 0.
/// Toute autre écriture ratée est le refus 6006.
fn ecrire_d_un_bloc(sortie: &mut dyn Write, octets: &[u8]) -> Result<ExitCode, Refus> {
    match sortie.write_all(octets).and_then(|()| sortie.flush()) {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(ExitCode::SUCCESS),
        Err(e) => Err(nommee(e, "writing the output").into()),
    }
}

#[cfg(test)]
mod tests;
