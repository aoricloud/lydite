//! Les aides des tests qui lancent de vraies mains dans de vrais dépôts git, et de ceux de la forge : un faux `gh` et
//! un distant nu local.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::branchements::bac::{ETAT_DE_CLAUDE, reglages_du_bac};
use crate::branchements::dossier::{connexion_presente, lire_empreinte};
use crate::coeur::erreur::Refus;
use crate::coeur::fil::{Evenement, Note, instant_lu, lot};
use crate::coeur::plan::Plan;
use crate::coeur::profil::{Declaration, Profil};
use crate::coeur::remarques::Remarque;
use crate::conduite::lancer::{Issues, Liens, Menee, base_du_depot, lancer_avec};

/// Un dossier neuf, à ce test, dans le dossier temporaire du système : le dépôt visé, le dossier de Kraken (que le
/// premier lancement crée) et la fausse main y vivent côte à côte.
pub(crate) fn racine(nom: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lydite-test-{nom}-{}", lot()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Le texte d'un profil d'essai qui ne porte pas sur le mur : sans ligne `mur`, le mur serait exigé, et la main
/// partirait sous srt. On y ajoute `mur = demande` ; un texte qui dit déjà son mur reste tel quel (une clé redite est
/// refusée).
pub(crate) fn avec_mur_demande(texte: &str) -> String {
    let dit = texte.lines().any(|ligne| ligne.split('=').next().is_some_and(|cle| cle.trim() == "mur"));
    match (dit, texte.ends_with('\n') || texte.is_empty()) {
        (true, _) => texte.to_string(),
        (false, true) => format!("{texte}mur = demande\n"),
        (false, false) => format!("{texte}\nmur = demande\n"),
    }
}

/// Un chemin neuf et unique dans le dossier temporaire du système : il n'existe pas encore.
pub(crate) fn neuf(nom: &str) -> PathBuf {
    env::temp_dir().join(format!("lydite-test-bac-{nom}-{}", lot()))
}

/// Un dépôt git neuf avec un commit.
pub(crate) fn depot_neuf(nom: &str) -> PathBuf {
    let dir = neuf(nom);
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "init"]);
    dir
}

/// Un dépôt git neuf avec un commit, dans `<racine>/depot`.
pub(crate) fn depot(racine: &Path) -> PathBuf {
    let dir = racine.join("depot");
    fs::create_dir_all(&dir).unwrap();
    let identite = ["-c", "user.name=t", "-c", "user.email=t@t"];
    for args in [&["init", "-q"][..], &[&identite[..], &["commit", "-q", "--allow-empty", "-m", "init"]].concat()] {
        assert!(Command::new("git").current_dir(&dir).args(args).status().unwrap().success());
    }
    dir
}

/// Une fausse main : rouge la première fois, vert ensuite ; garde sa dernière consigne dans entree.txt.
pub(crate) fn fausse_main(dir: &Path) -> Vec<String> {
    if cfg!(windows) {
        let script = dir.join("fausse.bat");
        fs::write(
            &script,
            "@echo off\r\nfindstr \"^\" > entree.txt\r\n\
             if exist marque (echo PREUVE: vert) else (type nul > marque & echo PREUVE: rouge)\r\n",
        )
        .unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("fausse.sh");
        fs::write(
            &script,
            "cat > entree.txt\nif [ -f marque ]; then echo 'PREUVE: vert'; else : > marque; echo 'PREUVE: rouge'; fi\n",
        )
        .unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Une fausse main de vague : elle écrit sa consigne dans `<bac>.txt` et finit sur `PREUVE: vert`. Chaque fichier porte
/// le nom de son bac : deux tâches assemblées ne s'y heurtent jamais. Elle n'agit que dans son bac.
pub(crate) fn main_de_vague(dir: &Path) -> Vec<String> {
    if cfg!(windows) {
        let script = dir.join("vague.bat");
        fs::write(
            &script,
            "@echo off\r\n\
             for %%I in (.) do set bac=%%~nxI\r\n\
             findstr \"^\" > \"%bac%.txt\"\r\n\
             echo PREUVE: vert\r\n",
        )
        .unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("vague.sh");
        fs::write(&script, "bac=$(basename \"$(pwd)\")\ncat > \"$bac.txt\"\necho 'PREUVE: vert'\n").unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Une fausse main de vague qui écrit hors de ses fichiers : `dedans.txt`, que la tâche permet par `fichiers :
/// dedans.txt`, et `dehors.txt`, qu'elle ne permet pas, puis `PREUVE: vert`. Sa marque et sa consigne vivent dans
/// `dir`, hors du bac : une marque dans le bac serait un chemin de plus hors des fichiers, et son dossier temporaire se
/// recrée vide à chaque tentative. Elle garde sa consigne dans `entree-<n>.txt` de `dir`, `n` étant 1 à sa première
/// tentative et 2 ensuite, que dit sa marque. À la relance elle ôte `dehors.txt` si elle `se_corrige`, et l'écrit
/// encore sinon : sa tâche est alors classée. Elle n'agit que dans son bac. `dir` sert à une seule tâche.
pub(crate) fn main_hors_fichiers(dir: &Path, se_corrige: bool) -> Vec<String> {
    let d = dir.to_string_lossy();
    if cfg!(windows) {
        let script = dir.join(if se_corrige { "hors-corrige.bat" } else { "hors-obstinee.bat" });
        let dehors = if se_corrige {
            "if %n%==2 (if exist dehors.txt del dehors.txt) else (echo dehors> dehors.txt)"
        } else {
            "echo dehors> dehors.txt"
        };
        fs::write(
            &script,
            format!(
                "@echo off\r\n\
                 set \"d={d}\"\r\n\
                 set n=1\r\n\
                 if exist \"%d%\\marque\" set n=2\r\n\
                 findstr \"^\" > \"%d%\\entree-%n%.txt\"\r\n\
                 echo dedans> dedans.txt\r\n\
                 {dehors}\r\n\
                 type nul > \"%d%\\marque\"\r\n\
                 echo PREUVE: vert\r\n"
            ),
        )
        .unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join(if se_corrige { "hors-corrige.sh" } else { "hors-obstinee.sh" });
        let dehors = if se_corrige {
            "if [ \"$n\" = 2 ]; then rm -f dehors.txt; else echo dehors > dehors.txt; fi"
        } else {
            "echo dehors > dehors.txt"
        };
        fs::write(
            &script,
            format!(
                "d='{d}'\n\
                 n=1\n\
                 if [ -f \"$d/marque\" ]; then n=2; fi\n\
                 cat > \"$d/entree-$n.txt\"\n\
                 echo dedans > dedans.txt\n\
                 {dehors}\n\
                 : > \"$d/marque\"\n\
                 echo 'PREUVE: vert'\n"
            ),
        )
        .unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Une fausse main qui écrit `dedans.txt` : à sa première tentative, `alpha beta` coupé en deux lignes ; à la relance,
/// `alpha beta` d'une ligne si elle `se_corrige`, coupé encore sinon. Elle garde sa consigne dans `entree-<n>.txt` de
/// `dir`, comme [`main_hors_fichiers`], et finit sur `PREUVE: vert`. `dir` sert à une seule tâche.
pub(crate) fn main_texte_coupe(dir: &Path, se_corrige: bool) -> Vec<String> {
    let d = dir.to_string_lossy();
    if cfg!(windows) {
        let script = dir.join(if se_corrige { "coupe-corrige.bat" } else { "coupe-obstinee.bat" });
        let relance = if se_corrige { "echo alpha beta> dedans.txt" } else { "(echo alpha& echo beta) > dedans.txt" };
        fs::write(
            &script,
            format!(
                "@echo off\r\n\
                 set \"d={d}\"\r\n\
                 set n=1\r\n\
                 if exist \"%d%\\marque\" set n=2\r\n\
                 findstr \"^\" > \"%d%\\entree-%n%.txt\"\r\n\
                 if %n%==2 ({relance}) else ((echo alpha& echo beta) > dedans.txt)\r\n\
                 type nul > \"%d%\\marque\"\r\n\
                 echo PREUVE: vert\r\n"
            ),
        )
        .unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join(if se_corrige { "coupe-corrige.sh" } else { "coupe-obstinee.sh" });
        let relance = if se_corrige { "echo 'alpha beta'" } else { "printf 'alpha\\nbeta\\n'" };
        fs::write(
            &script,
            format!(
                "d='{d}'\n\
                 n=1\n\
                 if [ -f \"$d/marque\" ]; then n=2; fi\n\
                 cat > \"$d/entree-$n.txt\"\n\
                 if [ \"$n\" = 2 ]; then {relance}; else printf 'alpha\\nbeta\\n'; fi > dedans.txt\n\
                 : > \"$d/marque\"\n\
                 echo 'PREUVE: vert'\n"
            ),
        )
        .unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Une fausse main de relance qui touche `.github/` : comme [`main_de_vague`], elle écrit sa consigne dans `<bac>.txt`,
/// puis `.github/ci.yml` dans son bac, ce qu'une PR de Kraken ne publie pas.
pub(crate) fn main_qui_ecrit_la_ci(dir: &Path) -> Vec<String> {
    if cfg!(windows) {
        let script = dir.join("vague-ci.bat");
        fs::write(
            &script,
            "@echo off\r\n\
             for %%I in (.) do set bac=%%~nxI\r\n\
             findstr \"^\" > \"%bac%.txt\"\r\n\
             mkdir .github\r\n\
             echo x> .github\\ci.yml\r\n\
             echo PREUVE: vert\r\n",
        )
        .unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("vague-ci.sh");
        let corps = "bac=$(basename \"$(pwd)\")\ncat > \"$bac.txt\"\nmkdir .github\necho x > .github/ci.yml\n\
                     echo 'PREUVE: vert'\n";
        fs::write(&script, corps).unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Un faux découpeur, qui est aussi la main des tâches du plan qu'il écrit : le découpeur et les mains d'une vague
/// découpée partagent leur profil. Il garde sa consigne dans son dossier temporaire (`TEMP`, ou `TMPDIR`, que Kraken
/// pose à côté du bac), hors du bac, puis :
///
/// - si sa consigne nomme `PLAN.lydite`, il est le découpeur, à sa tentative `n` (1, puis 2 une fois posée la marque
///   `decoupe-1` de `dir`) : il garde sa consigne dans `consigne-<n>.txt` de `dir` et son `PATH` dans
///   `path-decoupeur-<n>.txt`, copie `plan-<n>.lydite` de `dir`, s'il est là, en `PLAN.lydite` à la racine de son bac,
///   et écrit `autre.txt` dans son bac si `toucher-<n>` est dans `dir` ;
/// - sinon, il est la main d'une tâche : il garde son `PATH` dans `path-<bac>.txt` de `dir`, et sa consigne dans
///   `sortie/<bac>.txt` de son bac, que `fichiers : sortie/` couvre.
///
/// Il finit sur `PREUVE: vert`, qui ne compte pas pour le découpeur. Un `PATH` gardé est une ligne `PATH=<valeur>`
/// (Windows y ajoute celle de `PATHEXT`). `dir` est un dossier à lui, que le test garnit de ses plans.
pub(crate) fn faux_decoupeur(dir: &Path) -> Vec<String> {
    fs::create_dir_all(dir).unwrap();
    let d = dir.to_string_lossy();
    if cfg!(windows) {
        let script = dir.join("decoupeur.bat");
        let texte = format!(
            "@echo off\r\n\
             set \"d={d}\"\r\n\
             for %%I in (.) do set bac=%%~nxI\r\n\
             findstr \"^\" > \"%TEMP%\\consigne.txt\"\r\n\
             findstr /l /c:\"PLAN.lydite\" \"%TEMP%\\consigne.txt\" > nul\r\n\
             if errorlevel 1 goto tache\r\n\
             set n=1\r\n\
             if exist \"%d%\\decoupe-1\" set n=2\r\n\
             type nul > \"%d%\\decoupe-%n%\"\r\n\
             copy /y \"%TEMP%\\consigne.txt\" \"%d%\\consigne-%n%.txt\" > nul\r\n\
             set PATH > \"%d%\\path-decoupeur-%n%.txt\"\r\n\
             if exist \"%d%\\plan-%n%.lydite\" copy /y \"%d%\\plan-%n%.lydite\" PLAN.lydite > nul\r\n\
             if exist \"%d%\\toucher-%n%\" echo touche> autre.txt\r\n\
             echo PREUVE: vert\r\n\
             exit /b 0\r\n\
             :tache\r\n\
             set PATH > \"%d%\\path-%bac%.txt\"\r\n\
             if not exist sortie mkdir sortie\r\n\
             copy /y \"%TEMP%\\consigne.txt\" \"sortie\\%bac%.txt\" > nul\r\n\
             echo PREUVE: vert\r\n"
        );
        fs::write(&script, texte).unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("decoupeur.sh");
        let texte = format!(
            "d='{d}'\n\
             bac=$(basename \"$(pwd)\")\n\
             cat > \"$TMPDIR/consigne.txt\"\n\
             if grep -q 'PLAN\\.lydite' \"$TMPDIR/consigne.txt\"; then\n\
             n=1\n\
             if [ -f \"$d/decoupe-1\" ]; then n=2; fi\n\
             : > \"$d/decoupe-$n\"\n\
             cp \"$TMPDIR/consigne.txt\" \"$d/consigne-$n.txt\"\n\
             printf 'PATH=%s\\n' \"$PATH\" > \"$d/path-decoupeur-$n.txt\"\n\
             if [ -f \"$d/plan-$n.lydite\" ]; then cp \"$d/plan-$n.lydite\" PLAN.lydite; fi\n\
             if [ -f \"$d/toucher-$n\" ]; then echo touche > autre.txt; fi\n\
             else\n\
             printf 'PATH=%s\\n' \"$PATH\" > \"$d/path-$bac.txt\"\n\
             mkdir -p sortie\n\
             cp \"$TMPDIR/consigne.txt\" \"sortie/$bac.txt\"\n\
             fi\n\
             echo 'PREUVE: vert'\n"
        );
        fs::write(&script, texte).unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Un faux relecteur, qui est aussi la main des tâches du plan qu'il relit et celle de sa correction : le relecteur, la
/// correction et les mains d'une vague partagent leur profil. Il garde sa consigne dans son dossier temporaire (`TEMP`,
/// ou `TMPDIR`, que Kraken pose à côté du bac), hors du bac, puis :
///
/// - si sa consigne commence la correction (`Corrige au plus petit`), il est la main de la correction : il garde sa
///   consigne dans `consigne-correction.txt` de `dir`, écrit `dehors.txt` dans son bac si `hors-fichiers` est dans
///   `dir`, et sinon, sauf si `ne-rien-faire` y est, `sortie/correction.txt` ;
/// - si sa consigne nomme `REMARQUES.lydite`, il est le relecteur, à sa tentative `n` (1, puis 2 une fois posée la
///   marque `relecture-1` de `dir`) : il garde sa consigne dans `consigne-relecteur-<n>.txt` de `dir`, son `PATH` dans
///   `path-relecteur-<n>.txt`, le `DIFF.lydite` de son bac dans `diff-<n>.txt` (ou `sans-diff-<n>` s'il n'y en a
///   pas), de même son `PREUVE.lydite` dans `preuve-<n>.txt` (ou `sans-preuve-<n>`), copie `remarques-<n>.lydite` de
///   `dir`, s'il est là, en `REMARQUES.lydite` à la racine de son bac, ôte
///   l'`autre.txt` d'une tentative d'avant, en écrit un dans son bac si `toucher-<n>` est dans `dir`, et dit dans sa
///   réponse qu'il n'y a rien de bloquant si `dit-rien-de-bloquant` y est ;
/// - sinon, il est la main d'une tâche : il garde sa consigne dans `sortie/<bac>.txt` de son bac si elle dit « critique
///   », dans `autre/<bac>.txt` sinon, et y copie de même `gros.txt` de `dir`, s'il est là, en `gros-<bac>.txt` ;
///   si `ecrire-lecon` est dans `dir`, celle qui écrit sous `sortie/` écrit aussi une leçon fausse,
///   `.agents/skills/ecrite/SKILL.md`.
///
/// Il finit sur `PREUVE: vert`, qui ne compte pas pour le relecteur. `dir` est un dossier à lui, que le test garnit.
pub(crate) fn faux_relecteur(dir: &Path) -> Vec<String> {
    fs::create_dir_all(dir).unwrap();
    let d = dir.to_string_lossy();
    if cfg!(windows) {
        let script = dir.join("relecteur.bat");
        let texte = format!(
            "@echo off\r\n\
             set \"d={d}\"\r\n\
             for %%I in (.) do set bac=%%~nxI\r\n\
             findstr \"^\" > \"%TEMP%\\consigne.txt\"\r\n\
             findstr /l /c:\"Corrige au plus petit\" \"%TEMP%\\consigne.txt\" > nul\r\n\
             if not errorlevel 1 goto correction\r\n\
             findstr /l /c:\"REMARQUES.lydite\" \"%TEMP%\\consigne.txt\" > nul\r\n\
             if not errorlevel 1 goto relecteur\r\n\
             set dossier=autre\r\n\
             findstr /l /c:\"critique\" \"%TEMP%\\consigne.txt\" > nul\r\n\
             if not errorlevel 1 set dossier=sortie\r\n\
             if not exist %dossier% mkdir %dossier%\r\n\
             copy /y \"%TEMP%\\consigne.txt\" \"%dossier%\\%bac%.txt\" > nul\r\n\
             if exist \"%d%\\gros.txt\" copy /y \"%d%\\gros.txt\" \"%dossier%\\gros-%bac%.txt\" > nul\r\n\
             if \"%dossier%\"==\"sortie\" if exist \"%d%\\ecrire-lecon\" \
             (mkdir .agents\\skills\\ecrite & echo pas une lecon> .agents\\skills\\ecrite\\SKILL.md)\r\n\
             goto fin\r\n\
             :correction\r\n\
             copy /y \"%TEMP%\\consigne.txt\" \"%d%\\consigne-correction.txt\" > nul\r\n\
             if exist \"%d%\\hors-fichiers\" echo dehors> dehors.txt\r\n\
             if exist \"%d%\\ne-rien-faire\" goto fin\r\n\
             if not exist sortie mkdir sortie\r\n\
             echo corrige> sortie\\correction.txt\r\n\
             goto fin\r\n\
             :relecteur\r\n\
             set n=1\r\n\
             if exist \"%d%\\relecture-1\" set n=2\r\n\
             type nul > \"%d%\\relecture-%n%\"\r\n\
             copy /y \"%TEMP%\\consigne.txt\" \"%d%\\consigne-relecteur-%n%.txt\" > nul\r\n\
             set PATH > \"%d%\\path-relecteur-%n%.txt\"\r\n\
             if exist DIFF.lydite copy /y DIFF.lydite \"%d%\\diff-%n%.txt\" > nul\r\n\
             if not exist DIFF.lydite type nul > \"%d%\\sans-diff-%n%\"\r\n\
             if exist PREUVE.lydite copy /y PREUVE.lydite \"%d%\\preuve-%n%.txt\" > nul\r\n\
             if not exist PREUVE.lydite type nul > \"%d%\\sans-preuve-%n%\"\r\n\
             if exist \"%d%\\remarques-%n%.lydite\" copy /y \"%d%\\remarques-%n%.lydite\" REMARQUES.lydite > nul\r\n\
             if exist autre.txt del autre.txt\r\n\
             if exist \"%d%\\toucher-%n%\" echo touche> autre.txt\r\n\
             if exist \"%d%\\dit-rien-de-bloquant\" echo There is nothing blocking in my review.\r\n\
             :fin\r\n\
             echo PREUVE: vert\r\n"
        );
        fs::write(&script, texte).unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("relecteur.sh");
        let texte = format!(
            "d='{d}'\n\
             bac=$(basename \"$(pwd)\")\n\
             cat > \"$TMPDIR/consigne.txt\"\n\
             if grep -q 'Corrige au plus petit' \"$TMPDIR/consigne.txt\"; then\n\
             cp \"$TMPDIR/consigne.txt\" \"$d/consigne-correction.txt\"\n\
             if [ -f \"$d/hors-fichiers\" ]; then echo dehors > dehors.txt; fi\n\
             if [ ! -f \"$d/ne-rien-faire\" ]; then mkdir -p sortie; echo corrige > sortie/correction.txt; fi\n\
             elif grep -q 'REMARQUES\\.lydite' \"$TMPDIR/consigne.txt\"; then\n\
             n=1\n\
             if [ -f \"$d/relecture-1\" ]; then n=2; fi\n\
             : > \"$d/relecture-$n\"\n\
             cp \"$TMPDIR/consigne.txt\" \"$d/consigne-relecteur-$n.txt\"\n\
             printf 'PATH=%s\\n' \"$PATH\" > \"$d/path-relecteur-$n.txt\"\n\
             if [ -f DIFF.lydite ]; then cp DIFF.lydite \"$d/diff-$n.txt\"; else : > \"$d/sans-diff-$n\"; fi\n\
             if [ -f PREUVE.lydite ]; then cp PREUVE.lydite \"$d/preuve-$n.txt\"; else : > \"$d/sans-preuve-$n\"; fi\n\
             if [ -f \"$d/remarques-$n.lydite\" ]; then cp \"$d/remarques-$n.lydite\" REMARQUES.lydite; fi\n\
             rm -f autre.txt\n\
             if [ -f \"$d/toucher-$n\" ]; then echo touche > autre.txt; fi\n\
             if [ -f \"$d/dit-rien-de-bloquant\" ]; then echo 'There is nothing blocking in my review.'; fi\n\
             else\n\
             dossier=autre\n\
             if grep -q 'critique' \"$TMPDIR/consigne.txt\"; then dossier=sortie; fi\n\
             mkdir -p \"$dossier\"\n\
             cp \"$TMPDIR/consigne.txt\" \"$dossier/$bac.txt\"\n\
             if [ -f \"$d/gros.txt\" ]; then cp \"$d/gros.txt\" \"$dossier/gros-$bac.txt\"; fi\n\
             if [ -f \"$d/ecrire-lecon\" ] && [ \"$dossier\" = sortie ]; then mkdir -p .agents/skills/ecrite; \
             echo 'pas une lecon' > .agents/skills/ecrite/SKILL.md; fi\n\
             fi\n\
             echo 'PREUVE: vert'\n"
        );
        fs::write(&script, texte).unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// La valeur du `PATH` qu'une fausse main a gardée dans `fichier` (voir [`faux_decoupeur`]) : la ligne `PATH=<valeur>`,
/// le nom en toute casse, ses autres lignes sautées.
pub(crate) fn path_garde(fichier: &Path) -> String {
    let texte = fs::read_to_string(fichier).unwrap_or_else(|e| panic!("{} : {e}", fichier.display()));
    let ligne = texte.lines().find(|ligne| ligne.get(..5).is_some_and(|nom| nom.eq_ignore_ascii_case("PATH=")));
    ligne.map(|ligne| ligne[5..].to_string()).unwrap_or_else(|| panic!("aucun PATH dans {texte}"))
}

/// Une fausse main qui lit sa consigne et sort en erreur, sans rien écrire : sa tâche est classée après deux
/// tentatives.
pub(crate) fn main_qui_echoue(dir: &Path) -> Vec<String> {
    if cfg!(windows) {
        let script = dir.join("echoue.bat");
        fs::write(&script, "@echo off\r\nfindstr \"^\" > nul\r\nexit /b 1\r\n").unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("echoue.sh");
        fs::write(&script, "cat > /dev/null\nexit 1\n").unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let sortie = Command::new("git").current_dir(dir).args(args).output().unwrap();
    assert!(sortie.status.success(), "git {args:?} : {}", String::from_utf8_lossy(&sortie.stderr));
    String::from_utf8_lossy(&sortie.stdout).trim().to_string()
}

/// L'URL par laquelle un dépôt de test atteint son distant nu ([`distant_nu`]) : `forge.test` ne se résout jamais.
pub(crate) const URL_DU_DISTANT: &str = "https://forge.test/p/d";

/// Un distant nu (`git init --bare`), `<racine>/distant.git`, que `depot` atteint par [`URL_DU_DISTANT`] : son `origin`
/// porte cette URL, et `url.<distant nu>.insteadOf` la renvoie au dépôt nu, pour `fetch` comme pour `push`. Le `HEAD`
/// du dépôt y est poussé dans `main`. Rend le chemin du distant.
pub(crate) fn distant_nu(racine: &Path, depot: &Path) -> PathBuf {
    let nu = racine.join("distant.git");
    git(racine, &["init", "-q", "--bare", &nu.to_string_lossy()]);
    git(depot, &["remote", "add", "origin", URL_DU_DISTANT]);
    // Avec des `/` : git lit `C:/…` comme un chemin du poste sous Windows.
    let cle = format!("url.{}.insteadOf", nu.to_string_lossy().replace('\\', "/"));
    git(depot, &["config", &cle, URL_DU_DISTANT]);
    git(depot, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
    nu
}

/// L'adresse de la PR que le faux `gh` de [`gh_qui_suit`] ouvre.
pub(crate) const PR: &str = "https://forge.test/p/d/pull/12";

/// Un faux `gh` qui suit la vague sans rien savoir d'elle d'avance, écrit dans `<racine>/gh` : il note ses arguments
/// dans `appels.txt` ; `auth status` réussit ; `pr create` garde son corps (`corps.txt`) et la branche de `--head`,
/// puis rend l'adresse de la PR 12 ; `pr view <branche>` rend la PR sur la tête que la branche a dans `depot` ; `pr
/// view 12`, la PR ouverte, sans conflit, sur la tête de la branche gardée ; `run list`, un run vert ; `api`, aucun
/// statut. Rend son chemin.
pub(crate) fn gh_qui_suit(racine: &Path, depot: &Path) -> PathBuf {
    let dir = racine.join("gh");
    fs::create_dir_all(&dir).unwrap();
    let (d, depot) = (dir.to_string_lossy(), depot.to_string_lossy());
    let runs = concat!(
        r#"[{"conclusion":"success","createdAt":"","name":"ci","startedAt":"","status":"completed","workflowName":"c"#,
        r#"i"}]"#,
    );
    let statuts = r#"{"state":"pending","statuses":[],"total_count":0}"#;
    let vue = |tete: &str| format!(r#"{{"headRefOid":"{tete}","number":12,"url":"{PR}"}}"#);
    let ci = |tete: &str| format!(r#"{{"headRefOid":"{tete}","mergeable":"MERGEABLE","state":"OPEN"}}"#);
    if cfg!(windows) {
        let chemin = dir.join("gh.cmd");
        let lignes = [
            "@echo off".to_string(),
            format!("set \"d={d}\""),
            ">>\"%d%\\appels.txt\" echo %*".into(),
            "if \"%1\"==\"auth\" exit /b 0".into(),
            format!("if \"%1\"==\"run\" echo {runs}& exit /b 0"),
            format!("if \"%1\"==\"api\" echo {statuts}& exit /b 0"),
            "if \"%2\"==\"create\" goto creer".into(),
            "if \"%3\"==\"12\" goto ci".into(),
            format!("for /f %%s in ('git -C \"{depot}\" rev-parse %3') do set tete=%%s"),
            format!("echo {}", vue("%tete%")),
            "exit /b 0".into(),
            ":creer".into(),
            "findstr \"^\" > \"%d%\\corps.txt\"".into(),
            ">\"%d%\\branche.txt\" echo %6".into(),
            format!("echo {PR}"),
            "exit /b 0".into(),
            ":ci".into(),
            "set /p branche=<\"%d%\\branche.txt\"".into(),
            format!("for /f %%s in ('git -C \"{depot}\" rev-parse %branche%') do set tete=%%s"),
            format!("echo {}", ci("%tete%")),
        ];
        fs::write(&chemin, lignes.map(|ligne| ligne + "\r\n").concat()).unwrap();
        chemin
    } else {
        let chemin = dir.join("gh");
        let lignes = [
            format!("d='{d}'"),
            "echo \"$*\" >> \"$d/appels.txt\"".to_string(),
            "if [ \"$1\" = auth ]; then exit 0; fi".into(),
            format!("if [ \"$1\" = run ]; then echo '{runs}'; exit 0; fi"),
            format!("if [ \"$1\" = api ]; then echo '{statuts}'; exit 0; fi"),
            format!(
                "if [ \"$2\" = create ]; then cat > \"$d/corps.txt\"; echo \"$6\" > \"$d/branche.txt\"; \
                 echo '{PR}'; exit 0; fi"
            ),
            "if [ \"$3\" = 12 ]; then".into(),
            format!("tete=$(git -C '{depot}' rev-parse \"$(cat \"$d/branche.txt\")\")"),
            format!("echo '{}'", ci("'\"$tete\"'")),
            "else".into(),
            format!("tete=$(git -C '{depot}' rev-parse \"$3\")"),
            format!("echo '{}'", vue("'\"$tete\"'")),
            "fi".into(),
        ];
        script(&chemin, &lignes.join("\n"));
        chemin
    }
}

/// Un faux `gh`, écrit par le test dans `<racine>/gh` : un `.cmd` sous Windows, un script `sh` ailleurs. À son appel
/// `n` (1, 2…), il note ses arguments dans `appels.txt`, son entrée dans `entree-<n>.txt` et sa venue dans
/// `gh-<n>.ici`, au dossier d'où il part ; puis il rend ce que le test a posé par [`FauxGh::rendre`] : `sortie-<n>.txt`
/// sur sa sortie, `erreur-<n>.txt` sur ses erreurs, et le code de `code-<n>.txt`, 0 sans lui.
pub(crate) struct FauxGh {
    dir: PathBuf,
    /// Le programme à donner à la forge.
    pub(crate) programme: OsString,
}

impl FauxGh {
    pub(crate) fn ecrire(racine: &Path) -> FauxGh {
        let dir = racine.join("gh");
        fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy();
        let programme = if cfg!(windows) {
            let texte = format!(
                "@echo off\r\n\
                 set \"d={d}\"\r\n\
                 set n=0\r\n\
                 if exist \"%d%\\compte.txt\" set /p n=<\"%d%\\compte.txt\"\r\n\
                 set /a n+=1\r\n\
                 >\"%d%\\compte.txt\" echo %n%\r\n\
                 >>\"%d%\\appels.txt\" echo %n% %*\r\n\
                 type nul > \"gh-%n%.ici\"\r\n\
                 findstr \"^\" > \"%d%\\entree-%n%.txt\"\r\n\
                 if exist \"%d%\\sortie-%n%.txt\" type \"%d%\\sortie-%n%.txt\"\r\n\
                 if exist \"%d%\\erreur-%n%.txt\" type \"%d%\\erreur-%n%.txt\" 1>&2\r\n\
                 set code=0\r\n\
                 if exist \"%d%\\code-%n%.txt\" set /p code=<\"%d%\\code-%n%.txt\"\r\n\
                 exit %code%\r\n"
            );
            let chemin = dir.join("gh.cmd");
            fs::write(&chemin, texte).unwrap();
            chemin
        } else {
            let chemin = dir.join("gh");
            let corps = format!(
                "d='{d}'\n\
                 n=0\n\
                 if [ -f \"$d/compte.txt\" ]; then n=$(cat \"$d/compte.txt\"); fi\n\
                 n=$((n + 1))\n\
                 echo \"$n\" > \"$d/compte.txt\"\n\
                 echo \"$n $*\" >> \"$d/appels.txt\"\n\
                 : > \"gh-$n.ici\"\n\
                 cat > \"$d/entree-$n.txt\"\n\
                 if [ -f \"$d/sortie-$n.txt\" ]; then cat \"$d/sortie-$n.txt\"; fi\n\
                 if [ -f \"$d/erreur-$n.txt\" ]; then cat \"$d/erreur-$n.txt\" >&2; fi\n\
                 code=0\n\
                 if [ -f \"$d/code-$n.txt\" ]; then code=$(cat \"$d/code-$n.txt\"); fi\n\
                 exit \"$code\""
            );
            script(&chemin, &corps);
            chemin
        };
        FauxGh { dir, programme: programme.into_os_string() }
    }

    /// L'appel `n` sortira sur `code`, après avoir écrit `sortie` sur sa sortie et `erreurs` sur ses erreurs.
    pub(crate) fn rendre(&self, n: usize, code: u8, sortie: &str, erreurs: &str) {
        fs::write(self.dir.join(format!("sortie-{n}.txt")), sortie).unwrap();
        fs::write(self.dir.join(format!("erreur-{n}.txt")), erreurs).unwrap();
        fs::write(self.dir.join(format!("code-{n}.txt")), code.to_string()).unwrap();
    }

    /// Les arguments de chaque appel, dans l'ordre, joints par des blancs, sans les guillemets que Windows ajoute à
    /// certains.
    pub(crate) fn appels(&self) -> Vec<String> {
        let octets = fs::read(self.dir.join("appels.txt")).unwrap_or_default();
        let sans_numero = |ligne: &str| ligne.split_once(' ').map_or("", |(_, args)| args).replace('"', "");
        String::from_utf8_lossy(&octets).lines().map(|ligne| sans_numero(ligne).trim().to_string()).collect()
    }

    /// Ce que l'appel `n` a lu sur son entrée, en fins de ligne `\n`.
    pub(crate) fn entree(&self, n: usize) -> String {
        let octets = fs::read(self.dir.join(format!("entree-{n}.txt"))).unwrap();
        String::from_utf8_lossy(&octets).replace("\r\n", "\n")
    }
}

/// Un script `#!/bin/sh`, exécutable sous Unix : un crochet de git, ou le programme de `core.fsmonitor`.
pub(crate) fn script(chemin: &Path, corps: &str) {
    fs::write(chemin, format!("#!/bin/sh\n{corps}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut droits = fs::metadata(chemin).unwrap().permissions();
        droits.set_mode(0o755);
        fs::set_permissions(chemin, droits).unwrap();
    }
}

/// Un chemin que la coquille de git lit tel quel : avec des `/`, car elle prendrait une `\` pour un échappement.
pub(crate) fn pour_sh(chemin: &Path) -> String {
    chemin.to_string_lossy().replace('\\', "/")
}

/// Efface le dossier d'un test qui tue un arbre de processus, à sa fin, après ses assertions. Sous Windows, un
/// processus que Kraken vient de tuer peut tenir encore le bac, son dossier courant : un enfant né entre le relevé de
/// `taskkill /T` et la mort de sa racine lui échappe, et vit jusqu'au bout de sa commande (30 s au plus dans ces
/// tests). L'effacement se reprend donc, 60 s au plus. S'il ne passe pas, le test tombe en nommant le dossier : un
/// ménage qui échoue ne se tait pas. Un test rouge n'y arrive pas, et garde son dossier.
pub(crate) fn effacer(dir: &Path) {
    effacer_avant(dir, Duration::from_secs(60));
}

/// [`effacer`], qui se reprend `limite` au plus.
pub(crate) fn effacer_avant(dir: &Path, limite: Duration) {
    let debut = Instant::now();
    crate::branchements::dossier::lacher_sous(dir);
    loop {
        match fs::remove_dir_all(dir) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                assert!(debut.elapsed() < limite, "le dossier {} ne s'efface pas : {e}", dir.display());
                thread::sleep(Duration::from_millis(100));
            }
            _ => return,
        }
    }
}
/// Écrit `contenu` dans `<dir>/<chemin>`, ses dossiers créés.
pub(crate) fn ecrire(dir: &Path, chemin: &str, contenu: &str) {
    let chemin = dir.join(chemin);
    fs::create_dir_all(chemin.parent().unwrap()).unwrap();
    fs::write(chemin, contenu).unwrap();
}

/// L'état que Claude Code tient lui-même dans un bac, à la racine et sous `sous/` : un sous-agent isolé, un worktree
/// dans `.claude/worktrees` et un fichier dedans, puis un fichier à chaque autre nom de la liste à la racine, et sous
/// chacun d'eux sous `sous/`.
pub(crate) fn etat_de_claude(bac: &Path) {
    for (dossier, sous_agent) in [("", "x"), ("sous/", "y")] {
        let worktree = format!("{dossier}.claude/worktrees/{sous_agent}");
        git(bac, &["worktree", "add", "-q", "--detach", &worktree]);
        ecrire(bac, &format!("{worktree}/f"), "travail du sous-agent");
        for nom in ETAT_DE_CLAUDE.iter().filter(|nom| **nom != "worktrees") {
            let sous = if dossier.is_empty() { "" } else { "/f" };
            ecrire(bac, &format!("{dossier}.claude/{nom}{sous}"), "état de Claude Code");
        }
    }
}

/// Le délai des tests de l'arbre : assez long pour que le petit-enfant écrive sa marque même sous charge, bien plus
/// court que son sommeil.
pub(crate) const ARBRE: Duration = Duration::from_secs(10);

/// Une commande dont l'enfant, la coquille, lance un petit-enfant (PowerShell sous Windows, `sleep` ailleurs) qui écrit
/// son pid dans `marque`, au dossier courant, puis dort 30 s.
pub(crate) const LENTE: &str = if cfg!(windows) {
    r#"powershell -NoProfile -Command "$PID | Set-Content marque; Start-Sleep 30""#
} else {
    "sleep 30 & echo $! > marque; wait"
};

/// Le pid que le petit-enfant a écrit dans `<bac>/marque`, attendu `attente` au plus : sans lui, l'arbre est mort avant
/// que le petit-enfant ne parte.
pub(crate) fn marque(bac: &Path, attente: Duration) -> u32 {
    let debut = Instant::now();
    loop {
        let lu = fs::read_to_string(bac.join("marque")).unwrap_or_default();
        if let Ok(pid) = lu.trim().parse() {
            return pid;
        }
        assert!(debut.elapsed() < attente, "délai trop court : le petit-enfant n'a pas écrit sa marque ({lu:?})");
        thread::sleep(Duration::from_millis(100));
    }
}

/// Le processus de ce pid tourne encore : `tasklist` le liste sous Windows ; ailleurs, son `/proc/<pid>/stat` existe et
/// ne le dit pas zombie.
fn vivant(pid: u32) -> bool {
    if cfg!(windows) {
        let filtre = format!("PID eq {pid}");
        let liste = Command::new("tasklist").args(["/FI", &filtre, "/FO", "CSV", "/NH"]).output().unwrap();
        String::from_utf8_lossy(&liste.stdout).contains(&format!(",\"{pid}\","))
    } else {
        fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            let etat = stat.rsplit_once(')').and_then(|(_, reste)| reste.split_whitespace().next());
            !matches!(etat, Some("Z" | "X"))
        })
    }
}

/// Le processus de ce pid meurt dans les 10 s.
pub(crate) fn meurt(pid: u32) -> bool {
    let debut = Instant::now();
    while vivant(pid) {
        if debut.elapsed() > Duration::from_secs(10) {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
    true
}
/// Un faux srt, écrit dans `<dir>/srt`, que le profil reçoit par `avec_srt` : l'environnement du test n'y est pour
/// rien. Celui qui `tient` note la forme de son appel (`forme.txt`), le chemin de son fichier de réglages
/// (`fichier.txt`) et une copie de ce fichier (`reglages.json`), saute `--settings <fichier>`, puis joue `sh -c
/// <preuve>` en rendant son code ; sous Windows, où `sh` peut manquer, il joue la preuve par la coquille, et `true`,
/// l'épreuve du mur, sort en 0. L'autre rend 1, quoi qu'on lui donne. Rend son chemin.
pub(crate) fn faux_srt(dir: &Path, tient: bool) -> OsString {
    let notes = dir.join("srt");
    fs::create_dir_all(&notes).unwrap();
    let (nom, lignes): (&str, &[&str]) = match (cfg!(windows), tient) {
        (true, true) => (
            "faux-srt.cmd",
            &[
                "@echo off",
                r#">"%~dp0forme.txt" echo %~1 %~3 %~4"#,
                r#">"%~dp0fichier.txt" echo %~2"#,
                r#"copy /y "%~2" "%~dp0reglages.json" >nul"#,
                r#"if "%~5"=="true" exit /b 0"#,
                "%~5",
                "exit /b %errorlevel%",
            ],
        ),
        (true, false) => ("sans-mur.cmd", &["@exit /b 1"]),
        (false, true) => (
            "faux-srt.sh",
            &[
                "#!/bin/sh",
                r#"ici=$(dirname "$0")"#,
                r#"printf '%s %s %s' "$1" "$3" "$4" > "$ici/forme.txt""#,
                r#"printf '%s' "$2" > "$ici/fichier.txt""#,
                r#"cp "$2" "$ici/reglages.json""#,
                "shift 2",
                r#"exec "$@""#,
            ],
        ),
        (false, false) => ("sans-mur.sh", &["#!/bin/sh", "exit 1"]),
    };
    let chemin = notes.join(nom);
    let fin = if cfg!(windows) { "\r\n" } else { "\n" };
    fs::write(&chemin, lignes.iter().map(|ligne| format!("{ligne}{fin}")).collect::<String>()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&chemin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    chemin.into_os_string()
}

/// Lance le plan dans le dépôt visé : chaque tâche par une main, dans un bac neuf sous le dossier de Kraken, deux
/// tentatives au plus, la seconde avec l'erreur de la première en contexte, chacune sous un bail de durée `bail`. Rend
/// la branche de la vague (`None` pour des copies) et l'issue de chaque tâche, dans l'ordre du plan : `Ok`, sa prise,
/// assemblée pour un plan ; `Err`, la dernière erreur d'une classée, ou le refus en clair d'une refusée, le texte même
/// que le rejeu tire de sa note `vague.refus`.
///
/// Avant la vague, rien n'est noté, et la première faute est un refus, dans cet ordre : 6002 (le dépôt n'est pas un
/// dossier), 6003 (git n'y lit pas de commit, d'où partiraient les bacs), 6006 (le dossier de Kraken ne se crée pas),
/// puis, pour un plan, l'ouverture de l'assemblage, qui éprouve la base (6022, ou 6006). La vague s'ouvre par ses
/// notes, `vague.ouverte`, puis, pour un plan, `vague.assemblage` et un `vague.tache` par tâche ; si l'une rate,
/// l'assemblage se retire, et c'est le refus 6006 : rien n'en reste sans trace.
///
/// Dans la vague, tout geste se note au fil avant d'agir, dans une séance à elle, à l'origine `humain` :
///
/// - une tentative part par son bac, à la première (une tâche sans bac est refusée, 7001, et les autres continuent),
///   puis son bail (`main.bail`), son début (`main.debut`) et son lancement (`main.lancement`), d'après la déclaration
///   du profil de sa tâche (celui de la vague, aux modèles de la tâche), avec les réglages du dépôt que le bac porte,
///   suivi de son fournisseur (`main.fournisseur`) si ce profil en nomme un ; une main brute (`Profil::faux`) ne
///   déclare rien et n'en a pas. Des réglages qui ne se lisent pas arrêtent la vague (7012) comme une note ratée, et la
///   main ne part pas. Une tâche prête ne part pas tant que [`MAINS_A_LA_FOIS`] mains tournent : elle attend, et part
///   dès qu'une main finit, dans l'ordre du plan ;
/// - au message de sa main, son bail se conclut, puis son constat (`main.constat`), quand Kraken a lu quelque chose
///   d'elle, et sa fin (`main.fin`) se notent. Une main verte dont la tâche a des `fichiers` est rouge si elle a changé
///   un chemin hors d'eux (7013), son erreur disant ces chemins et la règle. Rouge à la première, la vague regarde le
///   `.claude/` du bac (7006), puis si la main a été tuée à son délai (7005), et sinon la relance, dans le même bac,
///   l'erreur en contexte ; rouge à la seconde, la tâche est classée (`main.verdict`), son bac gardé ; verte, elle est
///   prise (`main.verdict`), puis assemblée ;
/// - un résultat périmé n'est ni pris, ni figé, ni assemblé : il se note en `main.perimee`, et c'est tout ;
/// - une main qui n'a rien rendu à l'échéance de son bail expire (7004) ;
/// - une tâche dont une dépendance ne sera jamais assemblée est sautée (7011), sans bac ;
/// - une note ratée arrête la vague (7012) : plus aucune note, ni départ, ni fusion. Les mains en cours finissent, et
///   chaque issue qui n'était pas décidée porte ce refus, dont la preuve nomme le fil, puis la sortie de sa main.
///
/// La vague finit quand plus aucune de ses mains ne court, mains expirées comprises. Ses bacs restent en place, et le
/// worktree de la vague aussi, détaché sur la tête de sa branche.
///
/// Kraken mène ses vagues par `mener_sur` (`main.rs`), qui garde la tête de l'assemblage pour la relecture et la forge
/// et passe par [`lancer_avec`] : `lancer` et [`lancer_sur`] en sont les raccourcis des tests.
#[cfg(test)]
pub(crate) fn lancer(
    depot: &Path,
    dossier: &Path,
    profil: &Profil,
    plan: &Plan,
    bail: Duration,
) -> Result<(Option<String>, Issues), Refus> {
    let base = base_du_depot(depot)?;
    let Menee { branche, issues, .. } = lancer_sur(depot, dossier, profil, plan, bail, &base)?;
    Ok((branche, issues))
}
/// La variante de [`lancer`] sur une base donnée : la vague part de `base`, un sha du dépôt visé, et non de son `HEAD`.
/// Tout le reste est celui de `lancer`, à partir du refus 6006 du dossier de Kraken. Rend aussi l'id de la vague, la
/// tête de son assemblage quand elle finit, et si elle s'est arrêtée (7012). La pause d'une tâche qui repart sur une
/// coupure de l'API y est nulle : un test n'attend pas 60 s.
#[cfg(test)]
pub(crate) fn lancer_sur(
    depot: &Path,
    dossier: &Path,
    profil: &Profil,
    plan: &Plan,
    bail: Duration,
    base: &str,
) -> Result<Menee, Refus> {
    lancer_avec(depot, dossier, profil, plan, bail, base, Liens { pause: Some(Duration::ZERO), ..Liens::default() })
}

/// La première vague des essais de `lydite etat` et de la borne de `--n`.
pub(crate) const V1: &str = "1761901100000_7_0";

/// L'heure à laquelle ces essais lisent le fil, à la milliseconde comme `fil::instant` l'écrit.
pub(crate) const MAINTENANT: &str = "2026-10-31T22:00:00.000Z";

/// Une vague ouverte, comme le fil la note.
pub(crate) fn ouverture(vague: &str, n: usize, objectif: &str) -> Evenement {
    Evenement::Vague { vague: vague.into(), n, depot: "/srv/depot".into(), verifier: None, objectif: objectif.into() }
}

/// Un dossier neuf, à ce test.
pub(crate) fn dossier(nom: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lydite-fil-{nom}-{}", lot()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Les écarts, chacun par le numéro de sa ligne et celui de son code.
pub(crate) fn codes(ecarts: &[(usize, Refus)]) -> Vec<(usize, u16)> {
    ecarts.iter().map(|(ligne, refus)| (*ligne, refus.code.numero())).collect()
}

/// Les `seq` des notes, dans l'ordre du fil.
pub(crate) fn seqs(notes: &[Note]) -> Vec<u64> {
    notes.iter().map(|note| note.seq).collect()
}

/// La ligne, son instant remplacé par `<instant>` : il change à chaque note. Il s'écrit à la milliseconde.
pub(crate) fn sans_instant(ligne: &str) -> String {
    let (avant, apres) = ligne.split_once(r#""instant":""#).expect("une ligne porte son instant");
    let (instant, apres) = apres.split_once('"').expect("un instant entre guillemets");
    assert!(instant_lu(instant) && instant.len() == 24, "un instant à la milliseconde : {instant}");
    format!(r#"{avant}"instant":"<instant>"{apres}"#)
}

/// Un dossier de Kraken neuf, à ce test, avec son dossier `profils`.
pub(crate) fn kraken(nom: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lydite-test-profil-{nom}-{}", lot()));
    fs::create_dir_all(dir.join("profils")).unwrap();
    dir
}

/// Le texte d'un fichier : ces lignes, chacune finie par `\n`.
pub(crate) fn texte<S: AsRef<str>>(lignes: &[S]) -> String {
    lignes.iter().map(|ligne| format!("{}\n", ligne.as_ref())).collect()
}

/// Une remarque, comme la lecture la rend.
pub(crate) fn remarque(fichier: &str, ligne: Option<u32>, bloquante: bool, texte: &str) -> Remarque {
    Remarque { fichier: fichier.into(), ligne, bloquante, texte: texte.into() }
}

// Les formes d'appel des tests de `garder`, `borner` et `lancement` : chacune lit sur le disque ce que la conduite y
// lit (`preparer_dans` pour les deux premières, `noter_lancement` pour la dernière), puis le passe au profil pur.
impl Profil {
    /// [`Profil::garder_selon`], l'empreinte acceptée lue dans le dossier de Kraken.
    pub(crate) fn garder(&self) -> Result<(), Refus> {
        self.garder_selon(lire_empreinte)
    }

    /// [`Profil::borner_selon`], le fichier de connexion de Claude regardé sur le disque.
    pub(crate) fn borner(
        &mut self,
        dossier: &Path,
        git_commun: &Path,
        config: Option<&Path>,
        personnel: Option<&Path>,
    ) -> Result<(), Refus> {
        self.borner_selon(dossier, git_commun, config, personnel, connexion_presente)
    }
}

impl Declaration {
    /// [`Declaration::lancement_selon`], les réglages du dépôt que `bac` porte lus juste avant : un fichier de réglages
    /// présent qui ne se lit pas est une erreur qui le nomme.
    pub(crate) fn lancement(&self, vague: &str, main: usize, tentative: usize, bac: &Path) -> io::Result<Evenement> {
        Ok(self.lancement_selon(vague, main, tentative, reglages_du_bac(bac)?))
    }
}

// Les aides des tests d'une main, de sa preuve et de leurs processus, à part : leur `dossier` n'est pas celui du fil.
pub(crate) mod mains;

// Les aides des tests de `github` et de `publication`, à part : leur `PR` n'est pas celui de `gh_qui_suit`.
pub(crate) mod forge;

/// Les codes de refus (suites d'exactement quatre chiffres) que cite ce texte que lit une main, et que la table des
/// codes ne connaît pas.
pub(crate) fn codes_inconnus(texte: &str) -> Vec<String> {
    let mut inconnus = Vec::new();
    let mut chiffres = String::new();
    for c in texte.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            chiffres.push(c);
            continue;
        }
        if chiffres.len() == 4 && chiffres.parse().ok().and_then(crate::coeur::erreur::Code::de_numero).is_none() {
            inconnus.push(chiffres.clone());
        }
        chiffres.clear();
    }
    inconnus
}
