//! Le fil : le journal d'événements de Kraken, en ajout seul. Tout l'état s'en déduit, tout se rejoue.
//!
//! Son format est le contrat de toute pièce qui le lit ou y écrit (l'écran en TypeScript, des composants en Python,
//! un humain avec `cat`), dans la forme du contrat v2 de passerelle. Il se lit sans bibliothèque :
//!
//! - Le fil est `<dossier de Kraken>/fil.jsonl` : une note par ligne, finie par `\n`, chacune un objet JSON plat en
//!   UTF-8 (RFC 8259). Une ligne écrite ne change plus et ne s'efface pas ; l'ordre des lignes fait foi, pas leurs
//!   instants. L'ancien fil à tabulations (version 1), qui ne portait pas `v`, reste tel quel : plus rien ne le lit
//!   ni ne l'écrit.
//!
//! Recette de lecture :
//!
//! - lire le fichier en octets, et le couper sur `\n` et sur rien d'autre (pas `splitlines()` en Python) ;
//! - ce qui suit le dernier `\n` est en cours d'écriture, et attend la lecture suivante ;
//! - une ligne vide se saute, sans être illisible ; elle compte dans les numéros de ligne ;
//! - chaque ligne se décode en UTF-8 strict, puis se lit comme un objet JSON : `v` d'abord, puis l'enveloppe, puis le
//!   corps si le type est connu ;
//! - ce JSON est plus strict que la grammaire de la RFC 8259 sur deux points, que d'autres lecteurs laissent passer :
//!   un substitut Unicode seul (U+D800 à U+DBFF sans la moitié basse U+DC00 à U+DFFF qui doit le suivre, ou une moitié
//!   basse seule), écrit en échappement JSON, et plus de 64 niveaux d'imbrication, l'objet de la ligne étant le
//!   premier. La ligne est alors illisible (1001) et sautée, quel que soit son `v` : le refus tombe avant qu'on lise
//!   `v`, si bien que même une ligne d'une autre version ne donne pas 1002, qui arrête la lecture ;
//! - sous Windows, un lecteur ouvre le fil en partage lecture, écriture et effacement (le partage par défaut de Rust).
//!   Sinon, `noter` échoue (os error 32).
//!
//! Les règles du contrat :
//!
//! - `v`, la version du fil, vaut 2 et s'écrit en premier sur chaque ligne. Le lecteur lit l'objet entier, valeurs
//!   imbriquées comprises, puis `v` avant tout le reste : `v` absent ou qui n'est pas un entier, la ligne est mal
//!   formée (1009) et sautée ; `v` autre que 2, la lecture s'arrête sur le refus 1002. Un type neuf ne change pas `v` :
//!   un lecteur qui ne le connaît pas l'ignore. Seuls un type connu ou l'enveloppe, s'ils changeaient de champs,
//!   feraient monter `v`.
//! - Le plat, pour tout écrivain, passerelle compris : une ligne v2 n'a pour valeurs que des chaînes, des entiers
//!   positifs (sans signe, sans décimale ni exposant), des booléens et `null`. Toute autre valeur (un objet, un
//!   tableau, un autre nombre) rend la ligne mal formée (1009), que son type soit connu ou non. Si `seance` et `seq`
//!   se lisent, elle compte quand même dans sa séance : aucun faux trou.
//! - Une liste s'écrit dans une chaîne, un élément par ligne (`\n`) ; la chaîne vide veut dire aucun élément.
//! - `n`, `main`, `tentative` et `taches` sont des entiers de 1 à 10 000 (`NOMBRE_MAX`). Un autre nombre, même un
//!   entier positif qui tient en u64, rend la ligne mal formée (1009) : `lydite etat` dit une ligne par main, et une
//!   ligne valide d'un autre écrivain ne doit pas le faire paniquer ni remplir la mémoire. `lydite lancer` refuse de
//!   même `--n` au-delà (1006), pour que Kraken n'écrive pas une ligne que son lecteur refuse. Le `n` et les
//!   `bloquantes` d'une relecture comptent des remarques, dont il peut n'y avoir aucune : ce sont des entiers de 0 à
//!   10 000, et `bloquantes` ne dépasse pas `n`. Le fichier du relecteur n'a donc pas plus de 10 000 remarques.
//! - `numero`, le numéro d'une PR, que la forge donne, est un entier positif sans la borne de 10 000. Le `sha` d'un
//!   commit poussé s'écrit en 40 hexadécimaux minuscules.
//! - L'enveloppe, sur chaque ligne et dans cet ordre :
//!   `{"v":2,"seance":"<lot>","seq":3,"instant":"2026-10-31T21:03:00.123Z","origine":"humain","origine_foi":"declare",
//!   "type":"main.fin",<corps>}`.
//!   - `seance` est l'id de la séance qui écrit, un lot : deux séances n'ont jamais le même. `lancer` en ouvre une par
//!     vague ; une autre pièce écrit sur une vague depuis la sienne, car la vague est un champ du corps.
//!   - `seq` va 1, 2, 3… dans sa séance, dans l'ordre du fichier, sans trou ; une note ratée ne prend pas de numéro.
//!     Le lecteur suit chaque séance : un trou ou une redite est l'écart 1010, puis il repart du numéro lu.
//!   - `instant` est l'heure d'écriture, en RFC 3339 et en UTC, écrite à la milliseconde. Le lecteur la lit avec 0 à 3
//!     décimales, une date et une heure qui existent, sans seconde intercalaire (l'heure Unix n'en écrit pas).
//!   - `origine` dit qui est à l'origine du fait, dans la grammaire de passerelle. Ce lecteur en lit deux formes :
//!     `humain`, l'origine de la séance qu'ouvre la ligne de commande ; `agent:main-<vague>-<main>`, celle de la main,
//!     que porte la ligne de sa preuve, le `main.fin` ou le `main.perimee` à `foi: declare`, qui ne porte jamais
//!     `humain`. Un nom d'agent s'écrit en minuscules ASCII, chiffres, `-`, `_` et `.`, en 63 caractères au plus, où
//!     `main-<lot>-<k>` tient toujours. Une autre forme (`noeud:<nom>`, celles de passerelle) entrera au lecteur avec
//!     son premier écrivain, sans changer `v`.
//!   - `origine_foi`, `constate` ou `declare`, dit ce que vaut ce nom. Tout processus du compte peut taper
//!     `lydite lancer` ou écrire au fil : Kraken écrit donc `declare` sur chaque ligne. Avec le nœud, le compte séparé
//!     de chaque main prouvera l'origine, et l'écrivain mettra `constate`, valeur déjà dans le domaine. Aucun lecteur
//!     n'affiche une origine déclarée comme un fait.
//!   - `type` s'écrit `<domaine>.<quoi>`. Un type inconnu est ignoré au rejeu : son enveloppe se lit et compte dans sa
//!     séance.
//!   - Sur toute ligne, de type connu ou non, un champ de l'enveloppe qui manque ou sort de son domaine (une origine
//!     hors des deux formes, une `origine_foi` ni `constate` ni `declare`) rend la ligne mal formée (1009).
//! - Le corps d'un type connu porte tous ses champs : une absence vaut `null`, jamais une chaîne vide. Clés et
//!   valeurs-jetons sont en ASCII : un mot accentué peut s'écrire en NFC ou en NFD. Un champ en trop ou manquant, une
//!   valeur hors de son domaine, une fin ou une périmée déclarée dont l'origine n'est pas celle de sa main : la ligne
//!   est mal formée (1009), et sautée.
//! - `foi`, `constate` ou `declare`, dit ce que vaut ce qu'affirme le corps : constaté, Kraken l'a vu (un code de
//!   sortie, une preuve rejouée) ; déclaré, une main, un profil ou un appelant l'a dit. `origine` et `origine_foi`
//!   disent qui l'affirme, et ce que vaut ce nom.
//!
//! Les types de Kraken, et les champs de leur corps dans l'ordre :
//!
//! - `vague.ouverte {vague, n, depot, verifier, objectif}` : la vague `vague` (un lot) s'ouvre, `n` mains sur
//!   l'objectif dans le dépôt visé ; `verifier` est la commande que Kraken joue dans chaque bac après la main, `null`
//!   s'il n'y en a pas.
//! - `main.debut {vague, main, tentative}` : la main, numérotée de 1 à n dans sa vague, commence sa tentative (1, puis
//!   2 à la relance).
//! - `main.lancement {vague, main, tentative, profil, empreinte, modele, mur, reglages, reglages_depot, foi}` : la main
//!   part pour sa tentative, avec ce que son profil déclare. `profil`, `empreinte` et `modele` sont des jetons (de
//!   l'ASCII sans blanc, non vide) : le nom du profil, l'empreinte de son fichier (SHA-256 en hexadécimal) et le modèle
//!   de cette tentative. `mur` vaut `exige`, `demande` ou `natif`, selon que le profil exige le mur de Claude Code
//!   (la preuve rejouée par srt), le demande seulement, ou l'exige et rejoue la preuve dans le mur natif de Lydite ;
//!   un fil d'avant `natif` ne porte que les deux premiers, et se relit sans changement. `reglages` est le JSON exact
//!   que Kraken passe à la main en `--settings`, en chaîne.
//!   `reglages_depot` est une liste en chaîne, un élément `<chemin> <empreinte>` par ligne : les fichiers de réglages
//!   du dépôt que le bac porte au départ, vide s'il n'y en a aucun. `foi` vaut toujours `declare` : c'est ce que le
//!   profil dit, pas ce que la main fait. Ces lignes sont le compteur des lancements, par profil et par empreinte.
//!   Elles ne sont pas la preuve d'une main : elles portent l'origine de la séance.
//! - `main.lancement {vague, main, tentative, profil, empreinte, modele, effort, mur, reglages, reglages_depot, foi}` :
//!   le même lancement quand le profil déclare un effort pour la tentative. `effort` est un de `low`, `medium`,
//!   `high`, `xhigh`, `max`. Sans effort déclaré, la clé manque, et la ligne est la précédente : un fil écrit avant ce
//!   champ se relit sans lui, à l'identique.
//! - `main.constat {vague, main, tentative, version, mur_actif, foi}` : ce que Kraken a lu de la main avant sa consigne
//!   et à son ouverture. `version` est la version de Claude Code qu'elle a dite, `null` faute d'ouverture ; `mur_actif`
//!   dit si Claude Code a répondu que son mur est actif (`true`) ou non (`false`), `null` si rien ne l'a dit. `foi`
//!   vaut toujours `constate` : Kraken a lu cette réponse, ce qui ne dit rien de la tenue du mur. Il manque quand
//!   Kraken n'a rien lu de la main, par exemple d'une main qui ne parle pas en stream-json.
//! - `main.coupure {vague, main, tentative, statut}` : la tentative a fini sur une coupure de l'API (le fournisseur
//!   surchargé, une limite de débit, une erreur 5xx), que Claude Code marque dans son résultat par `is_error` vrai et
//!   `api_error_status`. `statut` est ce statut HTTP : 408, 429, ou 500 et plus. La ligne suit le `main.fin` de la
//!   tentative coupée, rouge comme les autres. Une coupure ne compte pas comme une relance : la tentative qui la suit
//!   n'est pas une relance au reçu, et une tâche ne repart qu'une fois sur une coupure ; la seconde coupure de la même
//!   tâche la classe. Un fil sans cette ligne se lit comme avant.
//! - `main.usage {vague, main, tentative, modele, entree, sortie, cache_ecrit, cache_lu, reflexion, micro_dollars,
//!   base, foi}` : l'usage d'un modèle de la tentative, un entier par compteur de jetons. Kraken écrit une ligne par
//!   modèle du détail de l'usage de la main ou, sans détail, une seule ligne du total sous le modèle de la tentative ;
//!   une tentative à l'usage vide n'en écrit aucune. `modele` est un jeton (de l'ASCII sans blanc, non vide). `base`
//!   dit d'où vient `micro_dollars`, une dépense en micro-dollars : `forfait`, sous l'abonnement (`apiKeySource`
//!   `none` et `costBasis` `list`), l'équivalent estimé que Claude Code annonce ; `estime`, sous un profil à `api` qui
//!   a un `prix` pour ce modèle, ou à défaut pour le modèle de la tentative (entrée, sortie, cache lu et cache écrit,
//!   chacun à son prix, la réflexion étant déjà dans la sortie ; arrondi au micro-dollar supérieur) ; `inconnu`
//!   sinon, `micro_dollars` à 0 : jamais le coût de Claude Code quand sa base n'est pas `list`.
//!   Seules les lignes `estime` comptent pour le quota du jour (`quota`, voir `lancer.rs`). `foi` vaut toujours
//!   `declare` : c'est ce que la main a compté, et ce que le profil prix. Elle porte l'origine de la séance.
//! - `main.fin {vague, main, tentative, couleur, foi, texte, verif}` : la tentative finit `vert` ou `rouge`. Vert et
//!   `constate`, la vérification que Kraken a jouée a réussi ; vert et `declare`, sans vérification, la main a fini sur
//!   `PREUVE: vert` ; rouge, Kraken l'a vu, et écrit `constate` ; rouge et `declare`, la main a fini sur
//!   `IMPOSSIBLE: <raison>`, et `texte` est sa raison. `texte` est la sortie de la main (vert) ou l'erreur
//!   (rouge). `verif` dit si la vérification a tourné :
//!   - `null` : elle n'a pas tourné, faute de commande, après une main rouge, ou parce qu'elle n'a pas pu partir
//!     (journal impossible à créer, coquille qui ne part pas). Dans ce dernier cas, la fin est rouge, et son texte le
//!     dit ;
//!   - une chaîne : elle a tourné, et c'est la fin de sa sortie, vide si elle n'a rien écrit. Un journal illisible y
//!     met « journal illisible : <erreur> », jamais la chaîne vide. Une preuve tuée du dehors (signal 9 ou 15, code
//!     137 ou 143 ; jamais sous Windows) est rejouée une seule fois : la première ligne de la chaîne est alors écrite
//!     par Lydite, `proof killed (<statut du premier jeu>), replayed once`, puis vient la fin de la sortie du second
//!     jeu. La même ligne ouvre `verif` de `vague.fusion` et de `vague.refus` pour la preuve de la vague.
//! - `main.verdict {vague, main, verdict, foi}` : la main est `prise`, `classee` ou `impossible`. Prise `constate`, sa
//!   vérification a réussi ; prise `declare`, elle ne tient qu'à la ligne `PREUVE: vert` de la main ; classée, Kraken
//!   l'a vu, et écrit `constate`. Impossible, la main de la tâche a fini sur `IMPOSSIBLE: <raison>` : le verdict est
//!   toujours `declare`, rien n'a été rejoué ni relancé, et sa dernière fin est rouge et `declare`, son texte la
//!   raison. Le texte de sa dernière fin est sa sortie si elle est prise, son erreur si elle est classée.
//! - `vague.assemblage {vague, branche, base, preuve}` : la vague a un plan, et ses tâches s'assemblent dans la branche
//!   `branche` (`lydite/vague-<vague>`). `base` est le sha complet du commit d'où elle part et où `preuve`, la commande
//!   de la vague, a passé avant toute main. Une fois par vague, juste après `vague.ouverte` ; des copies n'en écrivent
//!   pas.
//! - `vague.tache {vague, main, id, depend, preuve, objectif}` : la main `main` est la tâche `id` du plan. `depend` est
//!   une liste : les ids dont elle dépend, un par ligne et aucun vide (la chaîne vide : aucune dépendance). `preuve`
//!   est la commande qu'elle joue, la sienne ou, à défaut, celle de la vague. Une par tâche, avant tout départ ; des
//!   copies n'en écrivent pas.
//! - `main.bail {vague, main, tentative, echeance}` : le bail de la tentative, juste avant son `main.debut`, copies
//!   comprises. `echeance` est l'instant où il expire, écrit et lu comme `instant`.
//! - `main.perimee {vague, main, tentative, couleur, foi, texte, verif}` : le résultat d'une tentative expirée, que
//!   rien ne prend. Les champs de `main.fin`, dans les mêmes domaines ; déclaré, il porte l'origine de sa main, comme
//!   une fin déclarée.
//! - `vague.fusion {vague, main, sha, verif, foi}` : Kraken a fusionné le bac de la main dans la branche de la vague,
//!   et la preuve de la vague y est verte. `sha` est la nouvelle tête de la branche, `verif` la fin de la sortie de
//!   cette preuve. `foi` vaut `constate`, seule valeur que le lecteur prend : Kraken seul fusionne, et une fusion
//!   déclarée s'afficherait comme un fait.
//! - `main.gel {vague, main, sha}` : le commit `sha` (40 hexadécimaux minuscules, comme celui de `forge.poussee`) est
//!   ce que Kraken a gelé du bac de la main prise, et c'est sur lui que porte son `main.verdict`. Noté juste après le
//!   gel, avant la fusion ; une main classée ou impossible n'en note pas. Une note de Kraken, sans `foi`. Type neuf,
//!   il ne change pas `v`. Au rejeu, la main garde le dernier `sha` noté.
//! - `main.empreintes {vague, main, sha, fichiers}` : les épingles `sha256` de la tâche tiennent dans le commit gelé
//!   `sha` (40 hexadécimaux minuscules, comme celui de `main.gel`). `fichiers` est une liste en chaîne, un élément
//!   `<fichier> <empreinte>` par ligne : les épingles de la tâche, lues dans ce commit et égales à celles du plan.
//!   Noté quand tous les jugements du gel sont passés, juste avant la fusion, et seulement pour une tâche qui a des
//!   épingles. Une note de Kraken, sans `foi`. Type neuf, il ne change pas `v`. Le rejeu n'en retient rien.
//! - `vague.refus {vague, main, code, regle, preuve, correction, verif}` : Kraken a refusé la tâche. Une décision de
//!   Kraken, sans `foi`. `code` est le numéro d'un code, un entier de 1000 à 9999 ; `regle`, `preuve` et `correction`
//!   sont les trois textes de son refus (`erreur.rs`). `verif` est la fin de la sortie de la preuve de la vague, `null`
//!   quand elle n'a pas tourné.
//! - `forge.poussee {vague, branche, sha}` : Kraken va pousser le commit `sha` dans la branche distante `branche`.
//!   Notée avant le push : si la note rate, rien ne part.
//! - `forge.pr {vague, branche, base, sha, numero, url, foi}` : la PR `numero`, d'adresse `url`, contre la branche
//!   `base`, porte `sha` dans `branche`. `foi` vaut `constate` quand Kraken a lu la réponse de `gh`, `declare` sinon,
//!   et seul un `forge.pr` constaté compte au rejeu. Une relance note le sien sous son id, avec la `branche` et le
//!   `numero` de la PR qu'elle relance.
//! - `forge.ci {vague, numero, sha, couleur, texte, foi}` : la CI de la PR `numero` sur `sha` est `vert` ou `rouge`.
//!   `texte` est une ligne par check, `<nom> : <issue>`, puis, si elle est rouge, la fin du journal. `foi`, comme celle
//!   de `forge.pr` : seul un `forge.ci` constaté compte au rejeu.
//! - `forge.refus {vague, code, regle, preuve, correction}` : la forge ne va pas plus loin pour la vague. Une
//!   décision de Kraken, sans `main` ni `foi`, comme `vague.refus` : `code` est le numéro d'un code, un entier de
//!   1000 à 9999, et `regle`, `preuve` et `correction` sont les trois textes de son refus.
//! - `forge.rangement {vague, retires}` : ce que Kraken va retirer des bacs et des branches de la vague, une liste dans
//!   `retires`, un chemin ou une branche par ligne. Notée avant d'agir.
//! - `vague.decoupage {vague, taches, empreinte, plan}` : le découpage `vague`, une vague de copies d'une main, le
//!   découpeur, a pris un plan de `taches` tâches. `plan` est son texte, tel que le découpeur l'a écrit, et `empreinte`
//!   son SHA-256 en 64 hexadécimaux minuscules. Une décision de Kraken, sans `foi`. Notée une fois le découpage pris,
//!   avant que son bac se retire ; le fil reste local. Un découpage observé (`--observer`) n'a que cette note : aucune
//!   vague ne joue son plan.
//! - `vague.joue {vague, decoupage}` : la vague `vague` joue le plan que le découpage `decoupage` a écrit, une autre
//!   vague qu'elle. Une fois par vague découpée, avant tout départ.
//! - `vague.relecture {vague, relue, n, bloquantes, remarques}` : la relecture `vague`, une vague d'une main, le
//!   relecteur, de la vague `relue`, une autre, a rendu `n` remarques, dont `bloquantes` bloquantes ; `remarques` est
//!   leur texte, tel que le relecteur l'a écrit. Notée une fois la relecture prise. Une relecture classée n'en note
//!   pas : son refus, 7016, se note à sa place par un `forge.refus` de la vague relue, comme le refus 7017 d'une
//!   correction qui ne s'assemble pas.
//! - `vague.verdict {vague, relue, sha, profil, empreinte, modele}` : le verdict de la relecture `vague`, noté juste
//!   après sa `vague.relecture` et avant la correction et la forge. `relue` est la vague relue, une autre, et `sha`
//!   la tête que le relecteur a relue (40 hexadécimaux minuscules). `profil`, `empreinte` et `modele` sont ceux du
//!   `main.lancement` du relecteur, des jetons ; `null` pour une main nue, qui n'en note pas. Une note de Kraken, sans
//!   `foi`. Type neuf, il ne change pas `v`. Au rejeu, la vague relue garde le dernier verdict noté sur elle.
//! - `vague.correction {vague, corrige}` : la correction `vague`, une vague d'une tâche, corrige la vague `corrige`,
//!   une autre, que la relecture a dite à corriger. Avant tout départ de la correction. Elle n'a pas de seconde
//!   relecture, et les notes `forge.*` de la vague corrigée restent à la vague corrigée.
//! - `vague.relecteur {vague, relue}` : la vague `vague`, celle du relecteur, relit la vague `relue`, une autre. Avant
//!   tout départ du relecteur. Ce n'est pas une relecture : `vague.relecture` reste notée une fois la relecture prise,
//!   et une relecture classée n'en note toujours aucune.
//! - `main.fournisseur {vague, main, tentative, fournisseur, foi}` : la main part pour sa tentative chez le
//!   fournisseur que son profil déclare : `fournisseur` est l'hôte de son `api`, un jeton (de l'ASCII sans blanc, non
//!   vide). `foi` vaut toujours `declare` : Kraken n'a pas lu ce que répond ce serveur, et une main déclarée `sonnet`
//!   sur un autre fournisseur ne passe pas pour du Claude. Une main dont le profil ne nomme aucun fournisseur n'en a
//!   pas. Elle porte l'origine de la séance, comme le lancement qu'elle suit.
//! - `main.consigne {vague, main, tentative, role, consigne, systeme, lecons, ecartees, mesure}` : ce que la main a
//!   reçu pour sa tentative, noté avant qu'elle parte, main nue comprise. `role` est `main`, `decoupeur`,
//!   `relecteur`, `correction` ou `relance` (la relance de la CI). `consigne` est le SHA-256 du texte exact envoyé à
//!   la main, heure d'arrêt comprise, et `systeme` celui du fichier de son prompt système, ou `null` : les deux
//!   textes sont gardés dans `<dossier>/consignes/<SHA-256>.txt` (un dossier qui ne peut pas les garder arrête la
//!   vague, 7012). `lecons` dit les leçons données, une par ligne, `<nom> <SHA-256 de son texte>` : le texte
//!   donné à la main, ses `\r\n` ramenés à `\n`, et non les octets du SKILL.md au commit de base s'il est en `\r\n`
//!   (la chaîne vide sans leçon), `ecartees` le nom des autres leçons du commit de base, une par ligne ; `mesure`
//!   est la chaîne vide pour l'instant. Une note de Kraken, sans `foi`. Type neuf, il ne change pas `v`.
//! - `vague.issue {vague, issue}` : la vague `vague` a son issue sur la forge, de numéro `issue`, un entier positif
//!   sans la borne de 10 000, comme le `numero` d'une PR. Le titre et le corps de l'issue n'entrent pas au fil, seul
//!   son numéro. Une note de Kraken, sans `foi`. Type neuf, il ne change pas `v`. Il s'écrit par `Fil::noter_issue`
//!   et se relit par `lire_tout`, strictement comme les autres ; `lire` en dit les lignes mal formées.
//! - `vague.sceau {vague, octets, empreinte}` : le sceau du fil. `octets` est la longueur du fil au moment du sceau,
//!   un entier positif sans la borne de 10 000, comme le `numero` d'une PR ; `empreinte` est le SHA-256 de ses `octets`
//!   premiers octets, en 64 hexadécimaux minuscules. Une note de Kraken, sans `foi`, type neuf qui ne change pas `v`.
//!   Une vague le note juste avant de construire le corps de sa PR, puis à sa fin, sur chacune de ses sorties, qu'elle
//!   publie ou non ; le reçu de la PR reprend le premier. Plusieurs séances écrivent le même fil : le sceau couvre ce
//!   qui précède ses `octets`, même si d'autres lignes se glissent avant la sienne. `lydite etat` relit chaque sceau
//!   et recalcule l'empreinte ; le premier qui ne correspond plus est nommé sur stderr (sa vague, ses octets), et la
//!   sortie est 2. Un sceau dont les `octets` dépassent le début de sa propre ligne ne correspond pas.
//!   Ce que le sceau montre : une ligne changée ou retirée avant lui. Ce qu'il ne montre pas : une ligne ajoutée après
//!   lui, ni un fil réécrit avec des sceaux refaits (le reçu de la PR, hors de la machine, garde le sien).
//! - `vague.lancement {vague, version, plan}` : un lancement vient d'ouvrir la vague `vague`. `version` est la version
//!   du Lydite qui lance, celle que `lydite --version` dit, un jeton (de l'ASCII sans blanc, non vide). `plan` est le
//!   SHA-256 des octets du fichier que `--plan` a donné à cette vague, en 64 hexadécimaux minuscules, ou `null` pour
//!   toute autre vague (des copies, un découpage, la vague d'un plan découpé, un relecteur, une correction, une relance
//!   de la CI). Noté à l'ouverture, après les autres notes d'ouverture et avant tout départ de main. Une note de
//!   Kraken, sans `foi`. Type neuf, il ne change pas `v`.
//! - `vague.fin {vague, code}` : le lancement sort, et `vague` est la dernière vague qu'il a ouverte. `code` est le
//!   code de sortie qu'il rend, un entier de 0 à 255. Noté une fois, par la séance du lancement, après le sceau de fin.
//!   Une note de Kraken, sans `foi`. Type neuf, il ne change pas `v`.
//!   Règle de lecture de ces deux lignes : un seul lancement tient le dossier à la fois, donc la dernière vague du fil
//!   est la dernière qu'a ouverte le dernier lancement ; si elle porte un `vague.lancement` et aucun `vague.fin`, ce
//!   lancement n'est pas sorti. Un fil d'avant, sans ces lignes, se lit comme avant.
//!
//! Au rejeu d'une vague à plan, `lydite etat` ne garde que ce qu'il dit, le reste vit au fil :
//!
//! - l'issue d'une tâche est la dernière de ses `vague.fusion` (assemblée) et `vague.refus` (refusée), et l'emporte sur
//!   tout verdict : un refus 7009 qui suit une fusion la défait ;
//! - un `main.perimee` ne compte jamais, ni une fin ou un verdict qui suit le refus 7004 de sa tâche : rien d'une
//!   tentative expirée n'est pris ;
//! - l'échéance d'une tentative se range à la milliseconde (`…:00Z` devient `…:00.000Z`) : deux instants rangés ainsi
//!   se comparent comme des textes.
//!
//! Il garde de même, des notes `forge.*` de chaque vague, ce qu'il dit :
//!
//! - la PR est le dernier `forge.pr` constaté, la CI le dernier `forge.ci` constaté, le refus le premier
//!   `forge.refus` : un `forge.pr` ou un `forge.ci` déclaré ne compte pas ;
//! - une vague en relance une autre quand son `forge.pr` nomme la branche d'assemblage, celle de son
//!   `vague.assemblage`, d'une autre vague : le rejeu garde ce lien, et `lydite etat` montre d'abord la vague
//!   relancée ;
//! - `forge.poussee` et `forge.rangement` disent seulement que la forge a agi : rien d'autre ne s'en rejoue.
//!
//! Et des notes de découpage, de relecture et de correction, chacune sur la vague qui la porte, ce qu'il dit : un fait
//! redit remplace le précédent, et le texte du plan et celui des remarques, comme l'empreinte du plan, vivent au fil.
//!
//! - `vague.decoupage` : le nombre de tâches du plan que ce découpage a pris ; `vague.joue` : le découpage dont cette
//!   vague joue le plan ;
//! - `vague.relecture` : la vague que cette relecture a relue, ses remarques et celles qui sont bloquantes, en
//!   nombres ; `vague.correction` : la vague que cette correction corrige ; `vague.relecteur` : la vague que ce
//!   relecteur relit ; `vague.verdict` : sur la vague relue (non sur celle du relecteur), la tête jugée et le relecteur
//!   qui l'a jugée ;
//! - `main.fournisseur` : le fournisseur déclaré de la tentative.
//!
//! Et de `vague.issue` : le numéro de l'issue de la vague, le dernier noté, que `porter_issues` met sur
//! l'état que rend `etat`.
//!
//! Ces liens vont d'une vague à une autre, et le fil les porte tels que ses écrivains les notent : ni un lecteur ni
//! `lydite etat` ne suppose qu'un lien nomme une vague du fil, ni qu'il ne boucle pas.
//!
//! Toute main, quel que soit l'appelant, est précédée de son `main.lancement` : si la note échoue, elle ne part pas.
//! Dans une tentative, l'ordre est `main.bail`, `main.debut`, `main.lancement`, `main.fournisseur` s'il y en a un,
//! `main.consigne` (toujours, main nue comprise, avant que la main parte), `main.constat` s'il y en a un,
//! `main.usage` (une ligne par modèle) s'il y a un usage, puis `main.fin`, puis `main.coupure` si la tentative finit
//! sur une coupure de l'API. Le lancement, le fournisseur, la consigne, le constat, l'usage et la coupure ne changent
//! pas `v` : un lecteur qui ne connaît pas ces lignes les ignore, et leur `seq`
//! compte quand même dans leur séance. Le résultat périmé d'une main qui a dépassé son bail note de même son
//! `main.usage` (une ligne par modèle), juste avant son `main.perimee` : cette main a dépensé, et sa dépense compte
//! pour le quota des départs qui suivent.
//!
//! L'écriture :
//!
//! - Une ligne s'écrit d'une seule écriture, le fichier ouvert en ajout : le système la pose d'un bloc en fin de
//!   fichier (`O_APPEND` sous Linux, `FILE_APPEND_DATA` sous Windows), et des fils d'exécution ou des processus qui
//!   notent en même temps ne s'entremêlent pas.
//! - Une écriture coupée (Kraken tué, disque plein, coupure de courant) laisse en fin de fichier un bout de ligne sans
//!   `\n`. Pour écrire : regarder le dernier octet du fichier ; s'il existe et n'est pas `\n`, commencer la ligne par
//!   `\n`, qui clôt le bout (une ligne illisible de plus, sautée) au lieu de s'y coller. On clôt parfois ainsi pour
//!   rien la ligne qu'un autre écrit au même instant : le fil a alors une ligne vide, qu'un lecteur saute.
//!
//! Ce qui diffère de passerelle, un écart par ligne avec sa raison :
//!
//! - un `v` par ligne au lieu d'une poignée : un fichier n'a pas de dialogue ;
//! - `seance` au lieu de `demande` : le fil ne répond à personne ;
//! - l'instant à la milliseconde, pour mesurer les durées ;
//! - les types inconnus ignorés au lieu de refusés en 1008 : d'autres pièces y ajoutent les leurs ;
//! - des valeurs plates seulement, quand passerelle imbrique (`flux.entree`, `flux.erreur`, `bascule.publiee`) :
//!   l'enveloppe se lit toujours, et sans bibliothèque ;
//! - `Refus` à plat, au lieu d'`Erreur {code, message}` : au fil, un refus se note en champs plats (`vague.refus`) :
//!   `code` (le numéro), `regle`, `preuve` et `correction` ;
//! - l'origine `humain` au lieu de `patron`, car la doctrine nomme les gens par leur place ;
//! - `origine_foi` sur chaque ligne : passerelle tient l'origine pour déclarée à la poignée, sans le dire sur ses
//!   événements ;
//! - seules les origines que Kraken écrit se lisent : les autres entrent avec leur premier écrivain.
//!
//! Le journal de passerelle se fondra dans ce fil après la bascule, quand l'humain choisira la fusion, sans changer
//! `v`. D'ici là, rien ne change chez passerelle. Le jour de la fusion, passerelle :
//!
//! - écrit ses événements dans `fil.jsonl`, en lignes v2 plates. `bascule.publiee` et `bascule.retour` aplatissent
//!   leurs sélections : `de_compte`, `de_fournisseur` et `de_modele` (à `null` quand `de` manque), puis les mêmes en
//!   `vers_…`. Ses gestes prennent un type et un `instant` ;
//! - ouvre sa propre séance (un lot) et numérote son `seq` ;
//! - écrit l'origine `humain` au lieu de `patron`, ses autres origines dans la même grammaire, et `origine_foi` ;
//! - garde ses types sous ses préfixes (`cle.`, `secret.`, `bascule.`, `session.`, `flux.`, `porte.`, `abonnement.`,
//!   `poignee.`), que les types de Kraken ne prennent jamais ;
//! - arrête `gestes.jsonl` et `comptes.jsonl`. Le `v` à 1 de ce dernier est la version de ce journal, pas celle du fil.
//!
//! Les exemples, quatre groupes d'extraits, vivent dans `exemples.jsonl`, à côté de ce fichier ; les `seq` omis y
//! laissent leur place, et leurs trous sont voulus.
//!
//! - Des copies (les `main.bail`, `main.lancement` et `main.constat` de chaque tentative sont omis ; de la seconde
//!   séance, celle d'une vague sans vérification, seule la fin déclarée de sa main est montrée ; d'une troisième, celle
//!   d'une main d'un fournisseur (`prix = deepseek-flash 0.15 0.60 0.015`), seul le `main.usage` de sa tentative est
//!   montré : 12 000 jetons d'entrée à 0,15 $ le million, 40 000 de cache lu à 0,015 $, 800 de sortie,
//!   réflexion comprise, à 0,60 $, soit 1 800 + 600 + 480 = 2 880 micro-dollars).
//! - Une vague à plan (les `main.debut`, `main.lancement`, `main.constat`, `main.fin` et `main.verdict` de chaque
//!   tentative sont omis) : la tâche `a` est fusionnée ; `b`, qui dépend d'elle, expire (sa tentative s'arrête à son
//!   `main.lancement`), et son résultat tardif est périmé.
//! - Une vague à PR relancée (les `main.bail`, `main.debut`, `main.lancement`, `main.constat`, `main.fin` et
//!   `main.verdict` de chaque tâche sont omis ; chaque vague note dans sa séance, la forge dans une séance à elle,
//!   d'un bout à l'autre) : la vague `1761901500000_11_0` pousse sa branche et ouvre la PR 12, dont la CI est rouge ;
//!   sa relance, la vague `1761901891000_11_3` d'une tâche `ci`, repart du commit poussé, pousse le sien dans la même
//!   branche et note la même PR sous son id ; la CI est verte, et la forge range.
//! - Une vague découpée, relue et corrigée (les `main.bail`, `main.debut`, `main.lancement`, `main.constat`, `main.fin`
//!   et `main.verdict` de chaque main sont omis, sauf le verdict du découpeur, et leurs `seq` leur laissent la place ;
//!   chaque vague note dans sa séance, le découpage et la relecture dans une séance à eux, d'un bout à l'autre) : le
//!   découpage `1761901700000_13_0` prend un plan de deux tâches, que la vague `1761901790000_13_3` joue ; la tâche
//!   critique `b` passe à la relecture `1761901900000_13_5`, liée par son `vague.relecteur` à la vague qu'elle relit,
//!   qui rend une remarque, bloquante, et la correction `1761902000000_13_7`, partie de la tête de la vague, la
//!   corrige.

use std::collections::BTreeMap;
use std::path::Path;
use std::process;
use std::str;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::erreur::{Code, Refus};
use crate::coeur::json::{self, Valeur};
use crate::coeur::profil::EFFORTS;
use crate::coeur::protocole::statut_de_coupure;

/// La version du fil que Kraken écrit, et la seule qu'il lit.
const VERSION: u64 = 2;

/// L'origine de la séance qu'ouvre la ligne de commande.
const HUMAIN: &str = "humain";

/// Les clés de l'enveloppe, dans l'ordre où `Fil::noter` les écrit.
const ENVELOPPE: [&str; 7] = ["v", "seance", "seq", "instant", "origine", "origine_foi", "type"];

/// La longueur maximale d'un nom d'agent, dans la grammaire de passerelle.
const NOM_MAX: usize = 63;

/// Le plus grand `n`, `main`, `tentative` ou `taches` du fil, et le plus grand `--n` : `lydite etat` écrit une ligne
/// par main, et un entier valide d'un autre écrivain ne doit pas le mettre à genoux. Bien au-dessus de toute vague
/// réelle. C'est aussi la borne des remarques d'une relecture, de 0 à elle.
pub(crate) const NOMBRE_MAX: usize = 10_000;

/// Le corps d'une note : un événement de Kraken. Son enveloppe, c'est `Fil::noter` qui la pose.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Evenement {
    /// `vague.ouverte` : `n` mains sur `objectif` dans `depot`, jugées par la commande `verifier` s'il y en a une.
    Vague { vague: String, n: usize, depot: String, verifier: Option<String>, objectif: String },
    /// `main.debut` : la main `main` (1 à n) commence sa tentative `tentative` (1, puis 2 à la relance).
    Debut { vague: String, main: usize, tentative: usize },
    /// `main.lancement` : la main part pour sa tentative avec ce que son profil `profil`, de l'empreinte `empreinte`,
    /// déclare : le `modele` de la tentative, son `effort` déclaré (`None` sans clé : Claude Code prend son défaut, et
    /// la ligne n'a pas la clé), le `mur` du profil, le JSON exact de `--settings` (`reglages`) et les
    /// fichiers de réglages du dépôt que le bac portait au départ (`reglages_depot`, une liste en chaîne, un élément
    /// `<chemin> <empreinte>` par ligne, vide s'il n'y en a aucun). Toujours déclaré : la ligne porte `foi: declare`,
    /// qui n'est pas un champ.
    Lancement {
        vague: String,
        main: usize,
        tentative: usize,
        profil: String,
        empreinte: String,
        modele: String,
        effort: Option<String>,
        mur: Mur,
        reglages: String,
        reglages_depot: String,
    },
    /// `main.constat` : ce que Kraken a lu de la main : la `version` de Claude Code, `None` faute d'ouverture, et si
    /// son mur est actif (`mur_actif`), `None` si rien ne l'a dit. Toujours constaté : la ligne porte `foi: constate`,
    /// qui n'est pas un champ.
    Constat { vague: String, main: usize, tentative: usize, version: Option<String>, mur_actif: Option<bool> },
    /// `main.coupure` : la tentative a fini sur une coupure de l'API, dont `statut` est le statut HTTP (408, 429, ou
    /// 500 et plus). Noté après son `main.fin`.
    Coupure { vague: String, main: usize, tentative: usize, statut: u16 },
    /// `main.fin` : la tentative finit. `texte` est la sortie de la main (vert) ou l'erreur (rouge), `verif` la fin de
    /// la sortie de la vérification, `None` si elle n'a pas tourné.
    Fin {
        vague: String,
        main: usize,
        tentative: usize,
        couleur: Couleur,
        foi: Foi,
        texte: String,
        verif: Option<String>,
    },
    /// `main.verdict` : la main est prise ou classée.
    Verdict { vague: String, main: usize, verdict: Verdict, foi: Foi },
    /// `vague.assemblage` : la vague a un plan, dont les tâches s'assemblent dans la branche `branche`. `base` est le
    /// sha complet d'où elle part, où la commande `preuve` de la vague a passé avant toute main.
    Assemblage { vague: String, branche: String, base: String, preuve: String },
    /// `vague.tache` : la main `main` est la tâche `id` du plan. `depend` liste les ids dont elle dépend (aucun vide),
    /// `preuve` est la commande qu'elle joue.
    Tache { vague: String, main: usize, id: String, depend: Vec<String>, preuve: String, objectif: String },
    /// `main.bail` : la tentative a jusqu'à l'instant `echeance`, écrit comme celui d'une note, pour rendre un
    /// résultat.
    Bail { vague: String, main: usize, tentative: usize, echeance: String },
    /// `main.perimee` : le résultat d'une tentative expirée, que rien ne prend. Les champs de `Fin`.
    Perimee {
        vague: String,
        main: usize,
        tentative: usize,
        couleur: Couleur,
        foi: Foi,
        texte: String,
        verif: Option<String>,
    },
    /// `vague.fusion` : le bac de la main est fusionné dans la branche de la vague, dont `sha` est la nouvelle tête, et
    /// `verif` la fin de la sortie de la preuve de la vague, verte. Sa `foi` n'est pas un champ : elle s'écrit
    /// `constate`, seule valeur que le lecteur prend.
    Fusion { vague: String, main: usize, sha: String, verif: String },
    /// `main.gel` : le commit `sha` que Kraken a gelé du bac de la main prise, celui sur lequel porte son verdict.
    Gel { vague: String, main: usize, sha: String },
    /// `main.empreintes` : les épingles `sha256` de la tâche, lues dans le commit gelé `sha`, sont celles du plan ;
    /// `fichiers` en liste un `<fichier> <empreinte>` par élément.
    Empreintes { vague: String, main: usize, sha: String, fichiers: Vec<String> },
    /// `vague.refus` : Kraken refuse la tâche. `code` est le numéro d'un code (1000 à 9999), `verif` la fin de la
    /// sortie de la preuve de la vague, `None` si elle n'a pas tourné.
    Refus {
        vague: String,
        main: usize,
        code: u16,
        regle: String,
        preuve: String,
        correction: String,
        verif: Option<String>,
    },
    /// `forge.poussee` : Kraken va pousser le commit `sha` dans la branche distante `branche`. Notée avant le push.
    Poussee { vague: String, branche: String, sha: String },
    /// `forge.pr` : la PR `numero`, d'adresse `url`, contre la branche `base`, porte `sha` dans `branche`. `foi` est
    /// `Constate` quand Kraken a lu la réponse de `gh`, seule valeur que le rejeu prend.
    Pr { vague: String, branche: String, base: String, sha: String, numero: u64, url: String, foi: Foi },
    /// `forge.ci` : la CI de la PR `numero` sur `sha` est `couleur`. `texte` : une ligne par check, `<nom> : <issue>`,
    /// puis, rouge, la fin du journal. `foi` comme celle de `Pr`.
    Ci { vague: String, numero: u64, sha: String, couleur: Couleur, texte: String, foi: Foi },
    /// `forge.refus` : la forge ne va pas plus loin pour la vague. Une décision de Kraken, sans `main` ni `foi` :
    /// `code` est le numéro d'un code (1000 à 9999), suivi des trois textes de son refus.
    RefusForge { vague: String, code: u16, regle: String, preuve: String, correction: String },
    /// `forge.rangement` : ce que Kraken va retirer, un chemin ou une branche par élément. Notée avant d'agir.
    Rangement { vague: String, retires: Vec<String> },
    /// `vague.decoupage` : le découpage `vague`, une vague de copies du découpeur, a pris un plan de `taches` tâches
    /// (1 à 10 000), dont `plan` est le texte et `empreinte` le SHA-256, en 64 hexadécimaux minuscules.
    Decoupage { vague: String, taches: usize, empreinte: String, plan: String },
    /// `vague.joue` : la vague `vague` joue le plan que le découpage `decoupage`, une autre vague, a écrit.
    Joue { vague: String, decoupage: String },
    /// `vague.relecture` : la relecture `vague` de la vague `relue`, une autre, a rendu `n` remarques (0 à 10 000),
    /// dont `bloquantes` bloquantes (0 à `n`) ; `remarques` est leur texte.
    Relecture { vague: String, relue: String, n: usize, bloquantes: usize, remarques: String },
    /// `vague.verdict` : le verdict de la relecture `vague` de la vague `relue` : `sha` est la tête relue, et `profil`,
    /// `empreinte` et `modele` ceux du `main.lancement` du relecteur, `None` pour une main nue.
    VerdictRelecture {
        vague: String,
        relue: String,
        sha: String,
        profil: Option<String>,
        empreinte: Option<String>,
        modele: Option<String>,
    },
    /// `vague.correction` : la correction `vague` corrige la vague `corrige`, une autre.
    Correction { vague: String, corrige: String },
    /// `vague.relecteur` : la vague `vague`, celle du relecteur, relit la vague `relue`, une autre.
    Relecteur { vague: String, relue: String },
    /// `main.fournisseur` : la main part pour sa tentative chez le fournisseur que son profil déclare, l'hôte de son
    /// `api`, un jeton. Toujours déclaré : la ligne porte `foi: declare`, qui n'est pas un champ.
    Fournisseur { vague: String, main: usize, tentative: usize, fournisseur: String },
    /// `main.consigne` : ce que la main a reçu pour sa tentative, noté avant qu'elle parte : son `role`, l'empreinte
    /// SHA-256 du texte exact de sa `consigne`, celle du fichier de son prompt système (`systeme`, `None` sans lui),
    /// les `lecons` données (une par ligne, `<nom> <SHA-256 de son SKILL.md>`, vide sans leçon), le nom des leçons
    /// `ecartees` (une par ligne) et la `mesure`, vide pour l'instant. Une note de Kraken, sans `foi`.
    Consigne {
        vague: String,
        main: usize,
        tentative: usize,
        role: Role,
        consigne: String,
        systeme: Option<String>,
        lecons: String,
        ecartees: String,
        mesure: String,
    },
    /// `main.usage` : l'usage d'un modèle de la tentative, en jetons, et sa dépense en micro-dollars (0 si la `base`
    /// est `Inconnu`). Toujours déclaré : la ligne porte `foi: declare`, qui n'est pas un champ.
    Usage {
        vague: String,
        main: usize,
        tentative: usize,
        modele: String,
        entree: u64,
        sortie: u64,
        cache_ecrit: u64,
        cache_lu: u64,
        reflexion: u64,
        micro_dollars: u64,
        base: Base,
    },
}

/// D'où vient la dépense d'une ligne `main.usage`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Base {
    /// Sous l'abonnement : l'équivalent estimé que Claude Code annonce.
    Forfait,
    /// Estimée par Kraken au `prix` du profil : la seule qui compte pour le quota.
    Estime,
    /// Ni l'un ni l'autre : la dépense est 0.
    Inconnu,
}

impl Base {
    const TOUTES: [Base; 3] = [Base::Forfait, Base::Estime, Base::Inconnu];

    fn mot(self) -> &'static str {
        match self {
            Base::Forfait => "forfait",
            Base::Estime => "estime",
            Base::Inconnu => "inconnu",
        }
    }
}

/// `vague.issue` : la vague `vague` a son issue sur la forge, de numéro `issue`, un entier positif. Seul son numéro
/// entre au fil, ni son titre ni son corps. Un événement plat comme ceux d'[`Evenement`], écrit par
/// `Fil::noter_issue` et relu par [`issues`], mais hors de l'énumération : d'autres pièces la parcourent toute, et
/// ce type neuf n'y change rien.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IssueVague {
    pub(crate) vague: String,
    pub(crate) issue: u64,
}

impl IssueVague {
    /// Son nom, le `type` de sa ligne.
    const GENRE: &'static str = "vague.issue";

    /// Les champs de son corps, dans l'ordre de l'en-tête.
    fn champs(&self) -> Vec<(&'static str, Valeur)> {
        vec![("vague", Valeur::Texte(self.vague.clone())), ("issue", Valeur::Entier(self.issue))]
    }
}

/// `vague.sceau` : le fil, à la longueur `octets`, avait pour SHA-256 `empreinte` (64 hexadécimaux minuscules). Un
/// événement plat comme [`IssueVague`], hors de l'énumération d'[`Evenement`] : écrit par `Fil::sceller`, relu par
/// [`lire_octets`], vérifié par [`premier_sceau_rompu`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SceauVague {
    pub(crate) vague: String,
    pub(crate) octets: u64,
    pub(crate) empreinte: String,
}

impl SceauVague {
    /// Son nom, le `type` de sa ligne.
    const GENRE: &'static str = "vague.sceau";

    /// Le sceau de la vague `vague` sur les `octets` premiers octets du fil.
    pub(crate) fn sur(vague: &str, fil: &[u8]) -> SceauVague {
        SceauVague { vague: vague.into(), octets: fil.len() as u64, empreinte: hex(&sha256(fil)) }
    }

    /// Les champs de son corps, dans l'ordre de l'en-tête.
    fn champs(&self) -> Vec<(&'static str, Valeur)> {
        vec![
            ("vague", Valeur::Texte(self.vague.clone())),
            ("octets", Valeur::Entier(self.octets)),
            ("empreinte", Valeur::Texte(self.empreinte.clone())),
        ]
    }

    /// Le sceau tient-il encore sur `fil`, dont sa ligne commence à l'octet `debut` ? Ses `octets` ne dépassent pas
    /// le début de sa ligne, et l'empreinte de ces octets est la sienne.
    fn tient(&self, debut: usize, fil: &[u8]) -> bool {
        let Ok(octets) = usize::try_from(self.octets) else { return false };
        octets <= debut && fil.get(..octets).is_some_and(|avant| hex(&sha256(avant)) == self.empreinte)
    }
}

/// `vague.lancement` : un lancement vient d'ouvrir la vague `vague`. Un événement plat comme [`IssueVague`], hors de
/// l'énumération d'[`Evenement`] : écrit par `Fil::noter_lancement`, relu par [`lire_octets_avec_lancements`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LancementVague {
    pub(crate) vague: String,
    /// La version du Lydite qui lance (`CARGO_PKG_VERSION`), un jeton.
    pub(crate) version: String,
    /// Le SHA-256 des octets du fichier de `--plan`, en 64 hexadécimaux minuscules ; `None` pour toute autre vague.
    pub(crate) plan: Option<String>,
}

impl LancementVague {
    /// Son nom, le `type` de sa ligne.
    const GENRE: &'static str = "vague.lancement";

    /// Les champs de son corps, dans l'ordre de l'en-tête.
    fn champs(&self) -> Vec<(&'static str, Valeur)> {
        vec![
            ("vague", Valeur::Texte(self.vague.clone())),
            ("version", Valeur::Texte(self.version.clone())),
            ("plan", self.plan.clone().map_or(Valeur::Nul, Valeur::Texte)),
        ]
    }
}

/// `vague.fin` : le lancement sort, et `vague` est la dernière vague qu'il a ouverte ; `code` est son code de sortie.
/// Un événement plat comme [`LancementVague`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FinVague {
    pub(crate) vague: String,
    pub(crate) code: u8,
}

impl FinVague {
    /// Son nom, le `type` de sa ligne.
    const GENRE: &'static str = "vague.fin";

    /// Les champs de son corps, dans l'ordre de l'en-tête.
    fn champs(&self) -> Vec<(&'static str, Valeur)> {
        vec![("vague", Valeur::Texte(self.vague.clone())), ("code", Valeur::Entier(u64::from(self.code)))]
    }
}

/// Une ligne `vague.lancement` ou `vague.fin` lue, dans l'ordre du fil.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LancementLu {
    Ouvert(LancementVague),
    Sorti(FinVague),
}

/// Un sceau lu, avec l'octet où sa ligne commence.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SceauLu {
    pub(crate) sceau: SceauVague,
    pub(crate) debut: usize,
}

/// Le premier des `sceaux` lus dans `fil` qui ne tient plus : ses octets dépassent le début de sa ligne, ou leur
/// empreinte a changé. Un octet changé après le dernier sceau, ou une ligne ajoutée après lui, ne se voit pas.
pub(crate) fn premier_sceau_rompu<'a>(fil: &[u8], sceaux: &'a [SceauLu]) -> Option<&'a SceauVague> {
    sceaux.iter().find(|lu| !lu.sceau.tient(lu.debut, fil)).map(|lu| &lu.sceau)
}

/// Ce qu'une ligne d'un type connu donne : une note d'[`Evenement`], l'issue d'une vague, son sceau, ou le lancement
/// qui l'ouvre ou sa fin.
enum Lue {
    Note(Box<Note>),
    Issue(IssueVague),
    Sceau(SceauVague),
    Lancement(LancementVague),
    Fin(FinVague),
}

/// Comment une tentative finit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Couleur {
    Vert,
    Rouge,
}

impl Couleur {
    const TOUTES: [Couleur; 2] = [Couleur::Vert, Couleur::Rouge];

    fn mot(self) -> &'static str {
        match self {
            Couleur::Vert => "vert",
            Couleur::Rouge => "rouge",
        }
    }
}

/// Ce que Kraken fait d'une main, à la fin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Verdict {
    Prise,
    Classee,
    /// Une main de tâche a déclaré, par `IMPOSSIBLE: <raison>`, qu'aucun code honnête ne passe son test : la tâche est
    /// classée sans relance. Toujours déclaré.
    Impossible,
}

impl Verdict {
    const TOUS: [Verdict; 3] = [Verdict::Prise, Verdict::Classee, Verdict::Impossible];

    fn mot(self) -> &'static str {
        match self {
            Verdict::Prise => "prise",
            Verdict::Classee => "classee",
            Verdict::Impossible => "impossible",
        }
    }
}

/// Le rôle d'une main dont le fil note la consigne (`main.consigne`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Main,
    Decoupeur,
    Relecteur,
    Correction,
    /// La relance de la CI.
    Relance,
}

impl Role {
    const TOUS: [Role; 5] = [Role::Main, Role::Decoupeur, Role::Relecteur, Role::Correction, Role::Relance];

    fn mot(self) -> &'static str {
        match self {
            Role::Main => "main",
            Role::Decoupeur => "decoupeur",
            Role::Relecteur => "relecteur",
            Role::Correction => "correction",
            Role::Relance => "relance",
        }
    }
}

/// Ce que vaut une affirmation : constatée, Kraken l'a vue (un code de sortie, une preuve rejouée) ; déclarée, une
/// main, un profil ou un appelant l'a dite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Foi {
    Constate,
    Declare,
}

impl Foi {
    const TOUTES: [Foi; 2] = [Foi::Constate, Foi::Declare];

    fn mot(self) -> &'static str {
        match self {
            Foi::Constate => "constate",
            Foi::Declare => "declare",
        }
    }
}

/// Ce qu'un profil demande du mur : l'exiger et rejouer la preuve par srt, seulement le demander, ou l'exiger et
/// rejouer la preuve dans le mur natif de Lydite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mur {
    Exige,
    Demande,
    Natif,
}

impl Mur {
    const TOUS: [Mur; 3] = [Mur::Exige, Mur::Demande, Mur::Natif];

    fn mot(self) -> &'static str {
        match self {
            Mur::Exige => "exige",
            Mur::Demande => "demande",
            Mur::Natif => "natif",
        }
    }
}

/// Une ligne du fil lue : son enveloppe, et son événement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Note {
    pub(crate) seance: String,
    pub(crate) seq: u64,
    pub(crate) instant: String,
    pub(crate) origine: String,
    pub(crate) origine_foi: Foi,
    pub(crate) evenement: Evenement,
}

/// Ce que `lire` rend du fil : ses notes de Kraken, dans l'ordre du fil, et ses écarts.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Lecture {
    pub(crate) notes: Vec<Note>,
    /// Chaque écart avec le numéro de sa ligne (à partir de 1, lignes vides comprises) : 1001 ou 1009 pour une ligne
    /// sautée, 1010 pour un `seq` rompu, dont la ligne est lue.
    pub(crate) ecarts: Vec<(usize, Refus)>,
}

/// La ligne d'une note de la séance `seance`, de numéro `seq`, sans son `\n` : son enveloppe dans l'ordre
/// d'[`ENVELOPPE`], avec l'instant, et l'origine `origine` de la séance, sauf sur une fin ou une périmée déclarée, qui
/// porte celle de sa main ; puis le corps de l'événement.
pub(crate) fn ligne_de_note(seance: &str, seq: u64, origine: &str, evenement: &Evenement) -> String {
    let origine = match evenement {
        Evenement::Fin { vague, main, foi: Foi::Declare, .. }
        | Evenement::Perimee { vague, main, foi: Foi::Declare, .. } => origine_de_main(vague, *main),
        _ => origine.into(),
    };
    ligne(seance, seq, origine, evenement.genre(), evenement.champs())
}

/// La ligne de l'issue d'une vague, comme [`ligne_de_note`] celle d'un événement : à l'origine de la séance.
pub(crate) fn ligne_d_issue(seance: &str, seq: u64, origine: &str, issue: &IssueVague) -> String {
    ligne(seance, seq, origine.into(), IssueVague::GENRE, issue.champs())
}

/// La ligne du sceau d'une vague, comme [`ligne_d_issue`] : à l'origine de la séance.
pub(crate) fn ligne_de_sceau(seance: &str, seq: u64, origine: &str, sceau: &SceauVague) -> String {
    ligne(seance, seq, origine.into(), SceauVague::GENRE, sceau.champs())
}

/// La ligne du `vague.lancement` d'une vague, comme [`ligne_d_issue`] : à l'origine de la séance.
pub(crate) fn ligne_de_lancement(seance: &str, seq: u64, origine: &str, lancement: &LancementVague) -> String {
    ligne(seance, seq, origine.into(), LancementVague::GENRE, lancement.champs())
}

/// La ligne du `vague.fin` d'un lancement, comme [`ligne_d_issue`] : à l'origine de la séance.
pub(crate) fn ligne_de_fin(seance: &str, seq: u64, origine: &str, fin: &FinVague) -> String {
    ligne(seance, seq, origine.into(), FinVague::GENRE, fin.champs())
}

/// Une ligne, son enveloppe puis son corps.
fn ligne(seance: &str, seq: u64, origine: String, genre: &str, corps: Vec<(&'static str, Valeur)>) -> String {
    debug_assert!(origine_lue(&origine), "une origine que le fil lit : {origine}");
    let mut champs = vec![
        ("v", Valeur::Entier(VERSION)),
        ("seance", Valeur::Texte(seance.into())),
        ("seq", Valeur::Entier(seq)),
        ("instant", Valeur::Texte(instant(maintenant()))),
        ("origine", Valeur::Texte(origine)),
        // raccourci : toute origine est déclarée ; monter avec le nœud
        ("origine_foi", Valeur::Texte(Foi::Declare.mot().into())),
        ("type", Valeur::Texte(genre.into())),
    ];
    champs.extend(corps);
    json::ecrire(&champs)
}

/// Les octets du fil `chemin`, lus selon la recette de l'en-tête : ses notes de Kraken et ses écarts, et ses issues de
/// vague, dans l'ordre du fil. Chaque écart nomme `chemin` et sa ligne. Une ligne d'une autre version arrête la
/// lecture : le refus 1002, qui nomme la ligne et les deux versions.
///
/// Le dossier lit par [`lire_octets_avec_lancements`] ou [`lire_octets_avec_lignes`], qui passent par le même passage
/// que celle-ci : elle ne sert plus qu'aux essais.
#[cfg(test)]
pub(crate) fn lire_octets(chemin: &Path, octets: &[u8]) -> Result<Lu, Refus> {
    lire_octets_avec_lignes(chemin, octets).map(|(lu, _)| lu)
}

/// Ce que [`lire_octets`] rend, et de plus les `vague.lancement` et `vague.fin` lus, dans l'ordre du fil : la dernière
/// vague du fil, si elle porte un lancement et aucune fin, n'est pas sortie.
pub(crate) fn lire_octets_avec_lancements(chemin: &Path, octets: &[u8]) -> Result<(Lu, Vec<LancementLu>), Refus> {
    passer(chemin, octets).map(|(lu, _, lancements)| (lu, lancements))
}

/// Une ligne lue du fil, d'une note, d'une issue ou d'un sceau : de quoi la nommer et la retrouver dans les octets du
/// fil, sans la copier. Une ligne sautée (1001, 1009), une ligne d'un type inconnu et ce qui suit le dernier `\n`
/// n'en ont pas ; une ligne au `seq` rompu (1010) en a une.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LigneLue {
    /// Sa séance.
    pub(crate) seance: String,
    pub(crate) seq: u64,
    /// Son genre, le `type` de la ligne.
    pub(crate) genre: String,
    pub(crate) instant: String,
    /// La foi que son corps affirme : `None` quand la ligne n'a pas de champ `foi`.
    pub(crate) foi: Option<Foi>,
    /// L'octet où la ligne commence dans les octets du fil.
    pub(crate) debut: usize,
    /// L'octet où elle finit : le `\n` qui la termine n'en est pas.
    pub(crate) fin: usize,
}

impl LigneLue {
    /// La ligne que `note` a prise dans `objet`, dont les octets vont de `debut` à `fin`. `None` si un champ de
    /// l'enveloppe ne se lit pas : `note` les a lus, et il n'y en a pas de tel.
    fn de(objet: &BTreeMap<String, Option<Valeur>>, debut: usize, fin: usize) -> Option<LigneLue> {
        let valeur = |cle: &str| objet.get(cle).and_then(Option::as_ref);
        let texte = |cle: &str| match valeur(cle) {
            Some(Valeur::Texte(texte)) => Some(texte.clone()),
            _ => None,
        };
        let Some(Valeur::Entier(seq)) = valeur("seq") else { return None };
        let foi = texte("foi").and_then(|lue| Foi::TOUTES.into_iter().find(|foi| foi.mot() == lue));
        Some(LigneLue {
            seance: texte("seance")?,
            seq: *seq,
            genre: texte("type")?,
            instant: texte("instant")?,
            foi,
            debut,
            fin,
        })
    }
}

/// Ce que [`lire_octets`] rend, et de plus une [`LigneLue`] par note, issue, sceau, lancement ou fin lu, dans l'ordre
/// du fil.
pub(crate) fn lire_octets_avec_lignes(chemin: &Path, octets: &[u8]) -> Result<(Lu, Vec<LigneLue>), Refus> {
    passer(chemin, octets).map(|(lu, lignes, _)| (lu, lignes))
}

/// Ce que rend l'unique passage sur les octets du fil : ce que rend [`lire_octets`], une [`LigneLue`] par ligne lue, et
/// les lancements et fins lus. Les lectures publiques en prennent chacune leur part.
type Passage = (Lu, Vec<LigneLue>, Vec<LancementLu>);

/// Le passage unique sur les octets du fil, que toutes les lectures publiques partagent.
fn passer(chemin: &Path, octets: &[u8]) -> Result<Passage, Refus> {
    let mut issues = Vec::new();
    let mut sceaux = Vec::new();
    let mut lancements = Vec::new();
    let mut lues = Vec::new();
    // L'octet où commence la ligne en cours.
    let mut debut = 0usize;
    let mut lecture = Lecture::default();
    // Le dernier `seq` lu de chaque séance.
    let mut seances: BTreeMap<String, u64> = BTreeMap::new();
    let mut lignes = octets.split(|&octet| octet == b'\n');
    lignes.next_back(); // ce qui suit le dernier `\n` : rien, ou une ligne en cours d'écriture
    // Une ligne vide ne dit rien (voir l'écriture) : elle compte dans les numéros, sans être note ni écart.
    for (i, ligne) in lignes.enumerate() {
        let ici = debut;
        debut += ligne.len() + 1;
        if ligne.is_empty() {
            continue;
        }
        let numero = i + 1;
        let ecart = |code, preuve: String| {
            let correction = match code {
                Code::JsonIllisible => "La ligne est sautée : son écrivain écrit un objet JSON en UTF-8 par ligne.",
                Code::SequenceRompue => {
                    "La ligne est lue : l'écrivain de la séance numérote chaque note par le seq qui suit, sans en \
                     perdre ni en redire."
                }
                _ => {
                    "La ligne est sautée : son écrivain y met l'enveloppe entière et des valeurs plates, et pour un \
                     type connu ses seuls champs, chacun dans son domaine."
                }
            };
            (numero, Refus::new(code, format!("{}, ligne {numero} : {preuve}", chemin.display()), correction))
        };
        let lu = str::from_utf8(ligne).map_err(|e| format!("UTF-8 invalide après l'octet {}", e.valid_up_to()));
        let objet = match lu.and_then(json::lire) {
            Ok(objet) => objet,
            Err(raison) => {
                lecture.ecarts.push(ecart(Code::JsonIllisible, raison));
                continue;
            }
        };
        let v = match objet.get("v") {
            Some(Some(Valeur::Entier(v))) => *v,
            _ => {
                lecture.ecarts.push(ecart(Code::EvenementMalForme, "« v » manque ou n'est pas un entier".into()));
                continue;
            }
        };
        if v != VERSION {
            return Err(Refus::new(
                Code::VersionDifferente,
                format!("{}, ligne {numero} : v vaut {v}, et ce lecteur ne lit que v {VERSION}", chemin.display()),
                format!("Lis ce fil avec un Lydite qui connaît le fil v {v} : celui-ci ne lit que le fil v {VERSION}."),
            ));
        }
        // Une ligne mal formée dont `seance` et `seq` se lisent compte quand même dans sa séance : aucun faux trou.
        if let (Some(Some(Valeur::Texte(seance))), Some(Some(Valeur::Entier(seq)))) =
            (objet.get("seance"), objet.get("seq"))
        {
            let dernier = seances.get(seance).copied();
            if dernier.map_or(Some(1), |dernier| dernier.checked_add(1)) != Some(*seq) {
                let avant =
                    dernier.map_or("en tête de sa séance, où 1 est attendu".into(), |d| format!("après le seq {d}"));
                lecture.ecarts.push(ecart(Code::SequenceRompue, format!("séance {seance:?} : seq {seq} {avant}")));
            }
            seances.insert(seance.clone(), *seq);
        }
        match note(&objet) {
            Ok(Some(lue)) => {
                lues.extend(LigneLue::de(&objet, ici, ici + ligne.len()));
                match lue {
                    Lue::Note(note) => lecture.notes.push(*note),
                    Lue::Issue(issue) => issues.push(issue),
                    Lue::Sceau(sceau) => sceaux.push(SceauLu { sceau, debut: ici }),
                    Lue::Lancement(lancement) => lancements.push(LancementLu::Ouvert(lancement)),
                    Lue::Fin(fin) => lancements.push(LancementLu::Sorti(fin)),
                }
            }
            Ok(None) => {} // un type d'une autre pièce, ignoré au rejeu
            Err(raison) => lecture.ecarts.push(ecart(Code::EvenementMalForme, raison)),
        }
    }
    Ok(((lecture, issues, sceaux), lues, lancements))
}

/// Ce que [`lire_octets`] rend : les notes et écarts, les issues de vague, et les sceaux, dans l'ordre du fil.
pub(crate) type Lu = (Lecture, Vec<IssueVague>, Vec<SceauLu>);

/// Une ligne v2 lue en objet : sa note ou son issue, `None` pour un type inconnu dont l'enveloppe se lit, ou ce qui la
/// rend mal formée.
fn note(objet: &BTreeMap<String, Option<Valeur>>) -> Result<Option<Lue>, String> {
    if let Some((cle, _)) = objet.iter().find(|(_, valeur)| valeur.is_none()) {
        return Err(format!(
            "« {cle} » n'est pas une valeur plate : une chaîne, un entier positif, un booléen ou null"
        ));
    }
    let valeur = |cle: &str| objet.get(cle).and_then(Option::as_ref);
    let texte = |cle: &str| match valeur(cle) {
        Some(Valeur::Texte(texte)) => Ok(texte.as_str()),
        _ => Err(format!("« {cle} » manque ou n'est pas une chaîne")),
    };
    let texte_ou_nul = |cle: &str| match valeur(cle) {
        Some(Valeur::Nul) => Ok(None),
        Some(Valeur::Texte(texte)) => Ok(Some(texte.clone())),
        _ => Err(format!("« {cle} » manque, ou n'est ni une chaîne ni null")),
    };
    let booleen_ou_nul = |cle: &str| match valeur(cle) {
        Some(Valeur::Nul) => Ok(None),
        Some(Valeur::Booleen(booleen)) => Ok(Some(*booleen)),
        _ => Err(format!("« {cle} » manque, ou n'est ni un booléen ni null")),
    };
    let entier = |cle: &str| match valeur(cle) {
        Some(Valeur::Entier(n)) => Ok(*n),
        _ => Err(format!("« {cle} » manque ou n'est pas un entier positif")),
    };
    let nombre = |cle: &str| {
        let lu = entier(cle)?;
        match usize::try_from(lu) {
            Ok(n) if (1..=NOMBRE_MAX).contains(&n) => Ok(n),
            _ => Err(format!("« {cle} » vaut {lu}, hors de son domaine : un entier de 1 à {NOMBRE_MAX}")),
        }
    };
    let code = |cle: &str| {
        let lu = entier(cle)?;
        match u16::try_from(lu) {
            Ok(code) if (1000..=9999).contains(&code) => Ok(code),
            _ => Err(format!("« {cle} » vaut {lu}, hors de son domaine : un entier de 1000 à 9999")),
        }
    };
    // Le numéro d'une PR, que la forge donne : un entier positif, sans la borne des mains.
    let numero_de_pr = |cle: &str| match entier(cle)? {
        0 => Err(format!("« {cle} » vaut 0, hors de son domaine : un entier positif")),
        n => Ok(n),
    };
    // Le sha d'un commit poussé : 40 hexadécimaux minuscules.
    let sha = |cle: &str| {
        let lu = texte(cle)?;
        if lu.len() == 40 && lu.bytes().all(|octet| matches!(octet, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(lu.to_string())
        } else {
            Err(format!("« {cle} » vaut {lu:?}, hors de son domaine : 40 hexadécimaux minuscules"))
        }
    };
    // Un compte de remarques, que la relecture rend : un entier de 0 à la borne, aucune remarque comprise.
    let compte = |cle: &str| {
        let lu = entier(cle)?;
        match usize::try_from(lu) {
            Ok(n) if n <= NOMBRE_MAX => Ok(n),
            _ => Err(format!("« {cle} » vaut {lu}, hors de son domaine : un entier de 0 à {NOMBRE_MAX}")),
        }
    };
    // L'empreinte d'un plan : un SHA-256, en 64 hexadécimaux minuscules.
    let sha256 = |cle: &str| {
        let lu = texte(cle)?;
        if lu.len() == 64 && lu.bytes().all(|octet| matches!(octet, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(lu.to_string())
        } else {
            Err(format!("« {cle} » vaut {lu:?}, hors de son domaine : 64 hexadécimaux minuscules"))
        }
    };
    // La vague que la ligne relie à la sienne : une autre vague qu'elle, jamais elle-même.
    let autre_vague = |cle: &str| {
        let lu = texte(cle)?;
        if lu == texte("vague")? {
            Err(format!("« {cle} » vaut {lu:?}, la vague de la ligne, hors de son domaine : une autre vague"))
        } else {
            Ok(lu.to_string())
        }
    };
    // Un instant que le fil lit : celui de l'enveloppe, l'échéance d'un bail.
    let instant_ecrit = |cle: &str| {
        let lu = texte(cle)?;
        if instant_lu(lu) {
            Ok(lu)
        } else {
            Err(format!("« {cle} » n'est pas un instant RFC 3339 en UTC, de 0 à 3 décimales : {lu:?}"))
        }
    };
    // Une liste : un élément par ligne, aucun vide ; la chaîne vide, aucun élément.
    let liste = |cle: &str| {
        let lu = texte(cle)?;
        let elements: Vec<String> =
            if lu.is_empty() { Vec::new() } else { lu.split('\n').map(str::to_string).collect() };
        if elements.iter().any(String::is_empty) {
            return Err(format!("« {cle} » est une liste d'un élément par ligne, sans élément vide : {lu:?}"));
        }
        Ok(elements)
    };
    // Un jeton du corps (le nom d'un profil, une empreinte, un modèle) : de l'ASCII sans blanc, non vide.
    let jeton_ascii = |cle: &str| {
        let lu = texte(cle)?;
        if !lu.is_empty() && lu.bytes().all(|octet| octet.is_ascii_graphic()) {
            Ok(lu.to_string())
        } else {
            Err(format!("« {cle} » vaut {lu:?}, hors de son domaine : un jeton ASCII, non vide, sans blanc"))
        }
    };
    // Un jeton du corps, ou `null`.
    let jeton_ou_nul = |cle: &str| {
        if matches!(valeur(cle), Some(Valeur::Nul)) { Ok(None) } else { jeton_ascii(cle).map(Some) }
    };
    // `foi` ne varie pas dans ces deux types : `declare` pour un lancement, `constate` pour un constat.
    let foi_fixe = |attendue: Foi| jeton(&[attendue], Foi::mot, "foi", texte("foi")?).map(drop);
    let seance = texte("seance")?;
    if seance.is_empty() {
        return Err("« seance » est vide".into());
    }
    let seq = entier("seq")?;
    let instant = instant_ecrit("instant")?;
    let origine = texte("origine")?;
    if !origine_lue(origine) {
        return Err(format!("« origine » n'est ni humain ni agent:main-<vague>-<main> : {origine:?}"));
    }
    let origine_foi = jeton(&Foi::TOUTES, Foi::mot, "origine_foi", texte("origine_foi")?)?;
    let genre = texte("type")?;
    // Tous les champs du corps se sont lus : un de plus que l'enveloppe et le corps est en trop.
    let en_trop = |corps: &[(&str, Valeur)]| {
        let connue = |cle: &str| ENVELOPPE.contains(&cle) || corps.iter().any(|(nom, _)| *nom == cle);
        match objet.keys().find(|cle| !connue(cle)) {
            Some(cle) => Err(format!("champ en trop pour {genre} : « {cle} »")),
            None => Ok(()),
        }
    };
    let evenement = match genre {
        "vague.ouverte" => Evenement::Vague {
            vague: texte("vague")?.into(),
            n: nombre("n")?,
            depot: texte("depot")?.into(),
            verifier: texte_ou_nul("verifier")?,
            objectif: texte("objectif")?.into(),
        },
        "main.debut" => {
            Evenement::Debut { vague: texte("vague")?.into(), main: nombre("main")?, tentative: nombre("tentative")? }
        }
        "main.lancement" => {
            foi_fixe(Foi::Declare)?;
            Evenement::Lancement {
                vague: texte("vague")?.into(),
                main: nombre("main")?,
                tentative: nombre("tentative")?,
                profil: jeton_ascii("profil")?,
                empreinte: jeton_ascii("empreinte")?,
                modele: jeton_ascii("modele")?,
                // Facultatif : la clé manque quand rien n'est déclaré, et un fil d'avant ce champ se relit sans lui.
                effort: match valeur("effort") {
                    None => None,
                    Some(_) => match texte("effort")? {
                        lu if EFFORTS.contains(&lu) => Some(lu.to_string()),
                        lu => {
                            let domaine = EFFORTS.join(" ou ");
                            return Err(format!("« effort » vaut {lu:?}, hors de son domaine : {domaine}"));
                        }
                    },
                },
                mur: jeton(&Mur::TOUS, Mur::mot, "mur", texte("mur")?)?,
                reglages: texte("reglages")?.into(),
                reglages_depot: texte("reglages_depot")?.into(),
            }
        }
        "main.constat" => {
            foi_fixe(Foi::Constate)?;
            Evenement::Constat {
                vague: texte("vague")?.into(),
                main: nombre("main")?,
                tentative: nombre("tentative")?,
                version: texte_ou_nul("version")?,
                mur_actif: booleen_ou_nul("mur_actif")?,
            }
        }
        "main.coupure" => {
            let lu = entier("statut")?;
            let statut = u16::try_from(lu).ok().filter(|_| statut_de_coupure(lu));
            Evenement::Coupure {
                vague: texte("vague")?.into(),
                main: nombre("main")?,
                tentative: nombre("tentative")?,
                statut: statut
                    .ok_or_else(|| format!("« statut » vaut {lu}, hors de son domaine : 408, 429, ou 500 et plus"))?,
            }
        }
        "main.fin" => Evenement::Fin {
            vague: texte("vague")?.into(),
            main: nombre("main")?,
            tentative: nombre("tentative")?,
            couleur: jeton(&Couleur::TOUTES, Couleur::mot, "couleur", texte("couleur")?)?,
            foi: jeton(&Foi::TOUTES, Foi::mot, "foi", texte("foi")?)?,
            texte: texte("texte")?.into(),
            verif: texte_ou_nul("verif")?,
        },
        "main.verdict" => {
            let verdict = jeton(&Verdict::TOUS, Verdict::mot, "verdict", texte("verdict")?)?;
            let foi = jeton(&Foi::TOUTES, Foi::mot, "foi", texte("foi")?)?;
            if verdict == Verdict::Impossible && foi != Foi::Declare {
                return Err("un verdict « impossible » est toujours déclaré : « foi » vaut declare".into());
            }
            Evenement::Verdict { vague: texte("vague")?.into(), main: nombre("main")?, verdict, foi }
        }
        "vague.assemblage" => Evenement::Assemblage {
            vague: texte("vague")?.into(),
            branche: texte("branche")?.into(),
            base: texte("base")?.into(),
            preuve: texte("preuve")?.into(),
        },
        "vague.tache" => Evenement::Tache {
            vague: texte("vague")?.into(),
            main: nombre("main")?,
            id: texte("id")?.into(),
            depend: liste("depend")?,
            preuve: texte("preuve")?.into(),
            objectif: texte("objectif")?.into(),
        },
        "main.bail" => Evenement::Bail {
            vague: texte("vague")?.into(),
            main: nombre("main")?,
            tentative: nombre("tentative")?,
            echeance: instant_ecrit("echeance")?.into(),
        },
        "main.perimee" => Evenement::Perimee {
            vague: texte("vague")?.into(),
            main: nombre("main")?,
            tentative: nombre("tentative")?,
            couleur: jeton(&Couleur::TOUTES, Couleur::mot, "couleur", texte("couleur")?)?,
            foi: jeton(&Foi::TOUTES, Foi::mot, "foi", texte("foi")?)?,
            texte: texte("texte")?.into(),
            verif: texte_ou_nul("verif")?,
        },
        "vague.fusion" => {
            let fusion = Evenement::Fusion {
                vague: texte("vague")?.into(),
                main: nombre("main")?,
                sha: texte("sha")?.into(),
                verif: texte("verif")?.into(),
            };
            // Kraken seul fusionne : une fusion déclarée s'afficherait comme un fait.
            jeton(&[Foi::Constate], Foi::mot, "foi", texte("foi")?)?;
            fusion
        }
        "main.gel" => Evenement::Gel { vague: texte("vague")?.into(), main: nombre("main")?, sha: sha("sha")? },
        "main.empreintes" => Evenement::Empreintes {
            vague: texte("vague")?.into(),
            main: nombre("main")?,
            sha: sha("sha")?,
            fichiers: liste("fichiers")?,
        },
        "vague.refus" => Evenement::Refus {
            vague: texte("vague")?.into(),
            main: nombre("main")?,
            code: code("code")?,
            regle: texte("regle")?.into(),
            preuve: texte("preuve")?.into(),
            correction: texte("correction")?.into(),
            verif: texte_ou_nul("verif")?,
        },
        "forge.poussee" => {
            Evenement::Poussee { vague: texte("vague")?.into(), branche: texte("branche")?.into(), sha: sha("sha")? }
        }
        "forge.pr" => Evenement::Pr {
            vague: texte("vague")?.into(),
            branche: texte("branche")?.into(),
            base: texte("base")?.into(),
            sha: sha("sha")?,
            numero: numero_de_pr("numero")?,
            url: texte("url")?.into(),
            foi: jeton(&Foi::TOUTES, Foi::mot, "foi", texte("foi")?)?,
        },
        "forge.ci" => Evenement::Ci {
            vague: texte("vague")?.into(),
            numero: numero_de_pr("numero")?,
            sha: sha("sha")?,
            couleur: jeton(&Couleur::TOUTES, Couleur::mot, "couleur", texte("couleur")?)?,
            texte: texte("texte")?.into(),
            foi: jeton(&Foi::TOUTES, Foi::mot, "foi", texte("foi")?)?,
        },
        "forge.refus" => Evenement::RefusForge {
            vague: texte("vague")?.into(),
            code: code("code")?,
            regle: texte("regle")?.into(),
            preuve: texte("preuve")?.into(),
            correction: texte("correction")?.into(),
        },
        "forge.rangement" => Evenement::Rangement { vague: texte("vague")?.into(), retires: liste("retires")? },
        "vague.decoupage" => Evenement::Decoupage {
            vague: texte("vague")?.into(),
            taches: nombre("taches")?,
            empreinte: sha256("empreinte")?,
            plan: texte("plan")?.into(),
        },
        "vague.joue" => Evenement::Joue { vague: texte("vague")?.into(), decoupage: autre_vague("decoupage")? },
        "vague.relecture" => {
            let vague: String = texte("vague")?.into();
            let relue = autre_vague("relue")?;
            let (n, bloquantes) = (compte("n")?, compte("bloquantes")?);
            if bloquantes > n {
                return Err(format!(
                    "« bloquantes » vaut {bloquantes}, hors de son domaine : un entier de 0 à {n}, le « n » de la ligne"
                ));
            }
            Evenement::Relecture { vague, relue, n, bloquantes, remarques: texte("remarques")?.into() }
        }
        "vague.verdict" => Evenement::VerdictRelecture {
            vague: texte("vague")?.into(),
            relue: autre_vague("relue")?,
            sha: sha("sha")?,
            profil: jeton_ou_nul("profil")?,
            empreinte: jeton_ou_nul("empreinte")?,
            modele: jeton_ou_nul("modele")?,
        },
        "vague.correction" => Evenement::Correction { vague: texte("vague")?.into(), corrige: autre_vague("corrige")? },
        "vague.relecteur" => Evenement::Relecteur { vague: texte("vague")?.into(), relue: autre_vague("relue")? },
        "main.fournisseur" => {
            foi_fixe(Foi::Declare)?;
            Evenement::Fournisseur {
                vague: texte("vague")?.into(),
                main: nombre("main")?,
                tentative: nombre("tentative")?,
                fournisseur: jeton_ascii("fournisseur")?,
            }
        }
        "main.consigne" => Evenement::Consigne {
            vague: texte("vague")?.into(),
            main: nombre("main")?,
            tentative: nombre("tentative")?,
            role: jeton(&Role::TOUS, Role::mot, "role", texte("role")?)?,
            consigne: sha256("consigne")?,
            systeme: if matches!(valeur("systeme"), Some(Valeur::Nul)) { None } else { Some(sha256("systeme")?) },
            lecons: texte("lecons")?.into(),
            ecartees: texte("ecartees")?.into(),
            mesure: texte("mesure")?.into(),
        },
        "main.usage" => {
            foi_fixe(Foi::Declare)?;
            Evenement::Usage {
                vague: texte("vague")?.into(),
                main: nombre("main")?,
                tentative: nombre("tentative")?,
                modele: jeton_ascii("modele")?,
                entree: entier("entree")?,
                sortie: entier("sortie")?,
                cache_ecrit: entier("cache_ecrit")?,
                cache_lu: entier("cache_lu")?,
                reflexion: entier("reflexion")?,
                micro_dollars: entier("micro_dollars")?,
                base: jeton(&Base::TOUTES, Base::mot, "base", texte("base")?)?,
            }
        }
        IssueVague::GENRE => {
            let issue = IssueVague { vague: texte("vague")?.into(), issue: numero_de_pr("issue")? };
            en_trop(&issue.champs())?;
            return Ok(Some(Lue::Issue(issue)));
        }
        LancementVague::GENRE => {
            let plan = if matches!(valeur("plan"), Some(Valeur::Nul)) { None } else { Some(sha256("plan")?) };
            let lancement = LancementVague { vague: texte("vague")?.into(), version: jeton_ascii("version")?, plan };
            en_trop(&lancement.champs())?;
            return Ok(Some(Lue::Lancement(lancement)));
        }
        FinVague::GENRE => {
            let lu = entier("code")?;
            let Ok(code) = u8::try_from(lu) else {
                return Err(format!("« code » vaut {lu}, hors de son domaine : un entier de 0 à 255"));
            };
            let fin = FinVague { vague: texte("vague")?.into(), code };
            en_trop(&fin.champs())?;
            return Ok(Some(Lue::Fin(fin)));
        }
        SceauVague::GENRE => {
            let empreinte = texte("empreinte")?;
            if empreinte.len() != 64 || !empreinte.bytes().all(|octet| matches!(octet, b'0'..=b'9' | b'a'..=b'f')) {
                return Err(format!(
                    "« empreinte » vaut {empreinte:?}, hors de son domaine : 64 hexadécimaux minuscules"
                ));
            }
            let sceau = SceauVague {
                vague: texte("vague")?.into(),
                octets: numero_de_pr("octets")?,
                empreinte: empreinte.into(),
            };
            en_trop(&sceau.champs())?;
            return Ok(Some(Lue::Sceau(sceau)));
        }
        _ => return Ok(None),
    };
    en_trop(&evenement.champs())?;
    if let Evenement::Fin { vague, main, foi: Foi::Declare, .. }
    | Evenement::Perimee { vague, main, foi: Foi::Declare, .. } = &evenement
        && origine != origine_de_main(vague, *main)
    {
        let quoi = if genre == "main.fin" { "une fin" } else { "une périmée" };
        return Err(format!(
            "{quoi} déclarée porte l'origine de sa main, {}, pas {origine}",
            origine_de_main(vague, *main)
        ));
    }
    let (seance, instant, origine) = (seance.into(), instant.into(), origine.into());
    Ok(Some(Lue::Note(Box::new(Note { seance, seq, instant, origine, origine_foi, evenement }))))
}

/// Le jeton de `tous` qui s'écrit `lu`, ou ce qui rend la ligne mal formée : une valeur hors du domaine de `cle`.
fn jeton<T: Copy>(tous: &[T], mot: fn(T) -> &'static str, cle: &str, lu: &str) -> Result<T, String> {
    tous.iter().copied().find(|&jeton| mot(jeton) == lu).ok_or_else(|| {
        let domaine: Vec<&str> = tous.iter().map(|&jeton| mot(jeton)).collect();
        format!("« {cle} » vaut {lu:?}, hors de son domaine : {}", domaine.join(" ou "))
    })
}

/// L'origine de la ligne de preuve d'une main.
fn origine_de_main(vague: &str, main: usize) -> String {
    format!("agent:main-{vague}-{main}")
}

/// L'origine est d'une des deux formes que ce lecteur lit : `humain`, ou `agent:main-<vague>-<main>`, dont le nom suit
/// la grammaire de passerelle (minuscules ASCII, chiffres, `-`, `_` et `.`, 63 caractères au plus).
pub(crate) fn origine_lue(origine: &str) -> bool {
    let Some(nom) = origine.strip_prefix("agent:") else { return origine == HUMAIN };
    let grammaire = nom.len() <= NOM_MAX
        && nom.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_' | b'.'));
    let forme = nom
        .strip_prefix("main-")
        .and_then(|reste| reste.rsplit_once('-'))
        .is_some_and(|(vague, main)| !vague.is_empty() && !main.is_empty() && main.bytes().all(|c| c.is_ascii_digit()));
    grammaire && forme
}

/// L'instant d'une note, depuis les millisecondes de l'époque Unix : RFC 3339 en UTC à la milliseconde
/// (`2026-10-31T21:03:00.123Z`). Le calcul des jours civils de l'horloge de passerelle, plus les millisecondes.
pub(crate) fn instant(ms: u128) -> String {
    let (secondes, ms) = (ms / 1000, ms % 1000);
    let (jours, reste) = (secondes / 86_400, secondes % 86_400);
    let (h, m, s) = (reste / 3600, reste % 3600 / 60, reste % 60);
    let (annee, mois, jour) = civil(jours);
    format!("{annee:04}-{mois:02}-{jour:02}T{h:02}:{m:02}:{s:02}.{ms:03}Z")
}

/// Jours depuis le 1970-01-01 → (année, mois, jour), calendrier grégorien proleptique, par ères de 400 ans.
fn civil(jours: u128) -> (u128, u128, u128) {
    let z = jours + 719_468;
    let ere = z / 146_097;
    let jour_ere = z % 146_097;
    let annee_ere = (jour_ere - jour_ere / 1460 + jour_ere / 36_524 - jour_ere / 146_096) / 365;
    let jour_annee = jour_ere - (365 * annee_ere + annee_ere / 4 - annee_ere / 100);
    let mois_mars = (5 * jour_annee + 2) / 153;
    let jour = jour_annee - (153 * mois_mars + 2) / 5 + 1;
    let mois = if mois_mars < 10 { mois_mars + 3 } else { mois_mars - 9 };
    let annee = annee_ere + ere * 400 + u128::from(mois <= 2);
    (annee, mois, jour)
}

/// L'instant est un instant RFC 3339 en UTC que le fil lit : `AAAA-MM-JJTHH:MM:SS`, une date et une heure qui existent,
/// de 0 à 3 décimales à la seconde, puis `Z`.
pub(crate) fn instant_lu(instant: &str) -> bool {
    let octets = instant.as_bytes();
    let nombre = |de: usize, a: usize| {
        let chiffres = octets.get(de..a)?;
        chiffres.iter().try_fold(0, |n, &c| c.is_ascii_digit().then(|| n * 10 + u32::from(c - b'0')))
    };
    let separateurs = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    let decimales = match octets.get(19..) {
        Some([b'Z']) => true,
        Some([b'.', decimales @ .., b'Z']) => {
            (1..=3).contains(&decimales.len()) && decimales.iter().all(u8::is_ascii_digit)
        }
        _ => false,
    };
    let champs = (nombre(0, 4), nombre(5, 7), nombre(8, 10), nombre(11, 13), nombre(14, 16), nombre(17, 19));
    let (Some(annee), Some(mois), Some(jour), Some(h), Some(m), Some(s)) = champs else { return false };
    let bissextile = annee.is_multiple_of(4) && (!annee.is_multiple_of(100) || annee.is_multiple_of(400));
    let jours = match mois {
        2 if bissextile => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    separateurs.into_iter().all(|(i, c)| octets.get(i) == Some(&c))
        && decimales
        && (1..=12).contains(&mois)
        && (1..=jours).contains(&jour)
        && h < 24
        && m < 60
        && s < 60
}

/// L'instant lu rangé à la milliseconde : `…:00Z` devient `…:00.000Z`, `…:00.5Z` devient `…:00.500Z`. Deux instants
/// rangés ainsi se comparent comme des textes, ce que ne font pas deux instants de 0 à 3 décimales (`Z` suit `.`).
fn a_la_milliseconde(instant: &str) -> String {
    let sans_z = instant.trim_end_matches('Z');
    let (secondes, decimales) = sans_z.split_once('.').unwrap_or((sans_z, ""));
    format!("{secondes}.{decimales:0<3}Z")
}

/// Une vague rejouée : ce que dit son ouverture, son assemblage si elle a un plan, ses mains par numéro, et ce que le
/// fil dit de sa forge.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Vague {
    pub(crate) id: String,
    pub(crate) n: usize,
    pub(crate) depot: String,
    pub(crate) verifier: Option<String>,
    pub(crate) objectif: String,
    /// `vague.assemblage` : sa branche et la commande de sa preuve, `None` pour des copies.
    pub(crate) assemblage: Option<(String, String)>,
    pub(crate) mains: BTreeMap<usize, Main>,
    /// `forge.*` : ce que le fil dit de sa forge, `None` sans note de forge qui compte (un `forge.pr` ou un `forge.ci`
    /// déclaré ne compte pas).
    pub(crate) forge: Option<Forge>,
    /// L'id de la vague qu'elle relance, `None` si elle n'en relance aucune : son `forge.pr` nomme la branche
    /// d'assemblage d'une autre vague. Le lien se tire de tout le fil, pas d'une vague seule.
    pub(crate) relance_de: Option<String>,
    /// `vague.decoupage` : le nombre de tâches du plan que ce découpage a pris, `None` sans note.
    pub(crate) plan_pris: Option<usize>,
    /// `vague.joue` : l'id du découpage dont elle joue le plan, `None` si elle n'en joue aucun.
    pub(crate) decoupage: Option<String>,
    /// `vague.relecture` : ce que cette relecture a rendu de la vague qu'elle a relue, `None` sans note.
    pub(crate) relecture: Option<Relecture>,
    /// `vague.verdict` : le dernier verdict noté de la relecture de cette vague, qu'elle porte en tant que vague
    /// relue ; `None` sans note.
    pub(crate) jugement: Option<Jugement>,
    /// `vague.correction` : l'id de la vague que cette correction corrige, `None` si elle n'en corrige aucune.
    pub(crate) corrige: Option<String>,
    /// `vague.relecteur` : l'id de la vague que ce relecteur relit, `None` si elle n'en relit aucune.
    pub(crate) relit: Option<String>,
    /// `vague.issue` : le numéro de son issue sur la forge, le dernier noté, `None` sans note. [`etat`] le laisse à
    /// `None` : [`porter_issues`] le remplit.
    pub(crate) issue: Option<u64>,
}

/// Ce qu'une relecture a rendu, tel que le rejeu le garde : la vague relue, le nombre de remarques et celui des
/// remarques bloquantes. Leur texte vit au fil.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Relecture {
    pub(crate) relue: String,
    pub(crate) n: usize,
    pub(crate) bloquantes: usize,
}

/// Ce que le verdict d'une relecture dit, tel que le rejeu le garde sur la vague relue : la vague du relecteur qui l'a
/// rendu, la tête qu'il a relue, et le profil, l'empreinte et le modèle de son lancement (`None` pour une main nue).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Jugement {
    pub(crate) relecteur: String,
    pub(crate) sha: String,
    pub(crate) profil: Option<String>,
    pub(crate) empreinte: Option<String>,
    pub(crate) modele: Option<String>,
}

/// Ce que le rejeu garde des notes `forge.*` d'une vague : ce que `lydite etat` dit, constaté seulement.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Forge {
    /// `forge.pr` : le dernier constaté, `None` sans.
    pub(crate) pr: Option<Pr>,
    /// `forge.ci` : la couleur du dernier constaté, `None` sans.
    pub(crate) ci: Option<Couleur>,
    /// `forge.refus` : le premier, dit en clair (`refus_en_clair`), `None` sans.
    pub(crate) refus: Option<String>,
}

/// La PR d'une vague : son numéro, son adresse, et la branche distante qui la porte.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pr {
    pub(crate) numero: u64,
    pub(crate) url: String,
    pub(crate) branche: String,
}

/// Une main rejouée : ses tentatives par numéro, et son verdict avec sa foi, `None` tant qu'elle n'en a pas. Une tâche
/// de plan a de plus sa tâche, les échéances de ses tentatives et son issue.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Main {
    pub(crate) tentatives: BTreeMap<usize, Tentative>,
    pub(crate) verdict: Option<(Verdict, Foi)>,
    /// `vague.tache` : l'id de sa tâche et les ids dont elle dépend, `None` pour une copie.
    pub(crate) tache: Option<(String, Vec<String>)>,
    /// `main.bail` : l'échéance de chaque tentative, rangée à la milliseconde (`…:00Z` devient `…:00.000Z`) pour que
    /// deux instants se comparent comme des textes. Elles vivent ici, pas dans `Tentative`.
    pub(crate) echeances: BTreeMap<usize, String>,
    /// La dernière de ses `vague.fusion` et `vague.refus`, `None` tant qu'il n'y en a pas. Elle l'emporte sur
    /// `verdict`.
    pub(crate) issue: Option<Issue>,
    /// `main.gel` : le commit du bac gelé, le dernier noté, sur lequel porte son verdict ; `None` sans.
    pub(crate) gel: Option<String>,
    /// `main.coupure` : le statut de l'API de chaque tentative qui a fini sur une coupure, par numéro de tentative.
    pub(crate) coupures: BTreeMap<usize, u16>,
}

/// Ce que Kraken a décidé d'une tâche de plan : l'assembler dans la branche de la vague (`vague.fusion`), ou la refuser
/// (`vague.refus`), avec les quatre champs de son refus.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Issue {
    Assemblee,
    Refusee { code: u16, regle: String, preuve: String, correction: String },
}

impl Main {
    /// Sa sortie si elle est prise, son erreur si elle est classée : le texte de sa dernière tentative.
    pub(crate) fn texte(&self) -> &str {
        self.tentatives.last_key_value().map_or("", |(_, tentative)| &tentative.texte)
    }

    /// Son issue est le refus 7004 : sa tentative a expiré, et rien de ce que sa main rend ensuite n'est pris.
    fn expiree(&self) -> bool {
        matches!(&self.issue, Some(Issue::Refusee { code, .. }) if *code == Code::MainExpiree.numero())
    }
}

/// Une tentative rejouée : sa couleur avec sa foi, `None` tant qu'elle court, son texte et la fin de sa vérification,
/// `None` si elle n'a pas tourné ; le mur que son lancement déclarait et si son constat dit le mur actif, `None` sans
/// ces lignes (un fil qui n'a pas ces notes) ou quand rien ne l'a dit ; le fournisseur que son profil déclarait,
/// `None` sans `main.fournisseur` (un fil qui n'a pas cette note, ou un profil qui n'en nomme aucun).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Tentative {
    pub(crate) couleur: Option<(Couleur, Foi)>,
    pub(crate) texte: String,
    pub(crate) verif: Option<String>,
    pub(crate) mur: Option<Mur>,
    pub(crate) mur_actif: Option<bool>,
    pub(crate) fournisseur: Option<String>,
}

/// L'état que racontent les événements, sans rien lire ni écrire : les vagues dans l'ordre où le fil les nomme pour
/// la première fois. Un événement redit remplace le précédent ; une main dont l'ouverture de vague manque reste sous
/// une vague sans objectif. L'issue d'une tâche est la dernière de ses `vague.fusion` et `vague.refus`. Rien d'une
/// tentative expirée n'est pris : un `main.perimee` ne compte jamais, ni une fin ou un verdict qui suit le refus 7004
/// de sa tâche. Des notes `forge.*`, seuls comptent le dernier `forge.pr` constaté, le dernier `forge.ci` constaté et
/// le premier `forge.refus` de chaque vague, puis le lien d'une relance à la vague qu'elle relance. Des notes de
/// découpage, de relecture et de correction, le dernier fait de chaque sorte compte : le plan pris par un découpage,
/// le découpage qu'une vague joue, ce qu'une relecture a rendu, la vague qu'une correction corrige, celle qu'un
/// relecteur relit, le fournisseur d'une tentative. Le rejeu ne garde que ce que `lydite etat` lit, le reste (le sha
/// d'une fusion, la sortie de sa preuve, les résultats périmés, ce que la forge pousse et range, le texte d'un plan
/// et celui des remarques) vit au fil.
pub(crate) fn etat(evenements: &[Evenement]) -> Vec<Vague> {
    let mut vagues: Vec<Vague> = Vec::new();
    for evenement in evenements {
        let id = evenement.vague();
        let i = vagues.iter().position(|vague| vague.id == id).unwrap_or_else(|| {
            vagues.push(Vague { id: id.into(), ..Vague::default() });
            vagues.len() - 1
        });
        // Le verdict d'une relecture se garde sur la vague relue, non sur celle du relecteur qui le note.
        if let Evenement::VerdictRelecture { vague: relecteur, relue, sha, profil, empreinte, modele } = evenement {
            let j = vagues.iter().position(|vague| vague.id == *relue).unwrap_or_else(|| {
                vagues.push(Vague { id: relue.clone(), ..Vague::default() });
                vagues.len() - 1
            });
            vagues[j].jugement = Some(Jugement {
                relecteur: relecteur.clone(),
                sha: sha.clone(),
                profil: profil.clone(),
                empreinte: empreinte.clone(),
                modele: modele.clone(),
            });
            continue;
        }
        let vague = &mut vagues[i];
        match evenement {
            Evenement::VerdictRelecture { .. } => {} // gardé plus haut, sur la vague relue
            Evenement::Vague { n, depot, verifier, objectif, .. } => {
                (vague.n, vague.depot, vague.verifier, vague.objectif) =
                    (*n, depot.clone(), verifier.clone(), objectif.clone());
            }
            Evenement::Assemblage { branche, preuve, .. } => {
                vague.assemblage = Some((branche.clone(), preuve.clone()));
            }
            Evenement::Tache { main, id: tache, depend, .. } => {
                vague.mains.entry(*main).or_default().tache = Some((tache.clone(), depend.clone()));
            }
            Evenement::Bail { main, tentative, echeance, .. } => {
                vague.mains.entry(*main).or_default().echeances.insert(*tentative, a_la_milliseconde(echeance));
            }
            Evenement::Debut { main, tentative, .. } => {
                vague.mains.entry(*main).or_default().tentatives.insert(*tentative, Tentative::default());
            }
            Evenement::Lancement { main, tentative, mur, .. } => {
                let lancee = vague.mains.entry(*main).or_default().tentatives.entry(*tentative).or_default();
                lancee.mur = Some(*mur);
            }
            Evenement::Constat { main, tentative, mur_actif, .. } => {
                let lue = vague.mains.entry(*main).or_default().tentatives.entry(*tentative).or_default();
                lue.mur_actif = *mur_actif;
            }
            Evenement::Fin { main, tentative, couleur, foi, texte, verif, .. } => {
                let main = vague.mains.entry(*main).or_default();
                if !main.expiree() {
                    let fin = main.tentatives.entry(*tentative).or_default();
                    (fin.couleur, fin.texte, fin.verif) = (Some((*couleur, *foi)), texte.clone(), verif.clone());
                }
            }
            Evenement::Perimee { .. } => {} // rien d'une tentative expirée n'est pris
            Evenement::Verdict { main, verdict, foi, .. } => {
                let main = vague.mains.entry(*main).or_default();
                if !main.expiree() {
                    main.verdict = Some((*verdict, *foi));
                }
            }
            Evenement::Fusion { main, .. } => {
                vague.mains.entry(*main).or_default().issue = Some(Issue::Assemblee);
            }
            Evenement::Gel { main, sha, .. } => {
                vague.mains.entry(*main).or_default().gel = Some(sha.clone());
            }
            Evenement::Empreintes { .. } => {} // une note de Kraken, que le reçu lit dans les notes
            Evenement::Refus { main, code, regle, preuve, correction, .. } => {
                let (regle, preuve, correction) = (regle.clone(), preuve.clone(), correction.clone());
                vague.mains.entry(*main).or_default().issue =
                    Some(Issue::Refusee { code: *code, regle, preuve, correction });
            }
            Evenement::Pr { branche, numero, url, foi: Foi::Constate, .. } => {
                let pr = Pr { numero: *numero, url: url.clone(), branche: branche.clone() };
                vague.forge.get_or_insert_default().pr = Some(pr);
            }
            Evenement::Ci { couleur, foi: Foi::Constate, .. } => {
                vague.forge.get_or_insert_default().ci = Some(*couleur);
            }
            Evenement::Pr { .. } | Evenement::Ci { .. } => {} // déclarés, ils ne comptent pas
            Evenement::RefusForge { code, regle, preuve, correction, .. } => {
                let forge = vague.forge.get_or_insert_default();
                forge.refus.get_or_insert_with(|| refus_en_clair(*code, regle, preuve, correction));
            }
            Evenement::Poussee { .. } | Evenement::Rangement { .. } => {
                vague.forge.get_or_insert_default();
            }
            Evenement::Decoupage { taches, .. } => vague.plan_pris = Some(*taches),
            Evenement::Joue { decoupage, .. } => vague.decoupage = Some(decoupage.clone()),
            Evenement::Relecture { relue, n, bloquantes, .. } => {
                vague.relecture = Some(Relecture { relue: relue.clone(), n: *n, bloquantes: *bloquantes });
            }
            Evenement::Correction { corrige, .. } => vague.corrige = Some(corrige.clone()),
            Evenement::Relecteur { relue, .. } => vague.relit = Some(relue.clone()),
            Evenement::Fournisseur { main, tentative, fournisseur, .. } => {
                let lancee = vague.mains.entry(*main).or_default().tentatives.entry(*tentative).or_default();
                lancee.fournisseur = Some(fournisseur.clone());
            }
            Evenement::Coupure { main, tentative, statut, .. } => {
                vague.mains.entry(*main).or_default().coupures.insert(*tentative, *statut);
            }
            Evenement::Consigne { .. } => {} // la consigne vit au fil, `lydite etat` n'en dit rien
            Evenement::Usage { .. } => {}    // l'usage se lit au fil (le quota du jour), `lydite etat` n'en dit rien
        }
    }
    // Une relance nomme, dans son `forge.pr`, la branche d'assemblage d'une autre vague : la vague qu'elle relance.
    let branches: Vec<(String, String)> =
        vagues.iter().filter_map(|vague| Some((vague.assemblage.as_ref()?.0.clone(), vague.id.clone()))).collect();
    for vague in &mut vagues {
        let pr = vague.forge.as_ref().and_then(|forge| forge.pr.as_ref());
        let relancee =
            pr.and_then(|pr| branches.iter().find(|(branche, id)| *branche == pr.branche && *id != vague.id));
        vague.relance_de = relancee.map(|(_, id)| id.clone());
    }
    vagues
}

/// Porte les issues lues du fil (voir `lire_tout`) sur les vagues rejouées par [`etat`] : chaque vague prend le
/// numéro de sa dernière issue notée. Une issue d'une vague que le rejeu ne connaît pas est ignorée.
pub(crate) fn porter_issues(vagues: &mut [Vague], issues: &[IssueVague]) {
    for IssueVague { vague: id, issue } in issues {
        if let Some(vague) = vagues.iter_mut().find(|vague| vague.id == *id) {
            vague.issue = Some(*issue);
        }
    }
}

/// Ce que la dernière relecture notée de la vague `id` a rendu.
pub(crate) fn relecture_de<'a>(vagues: &'a [Vague], id: &str) -> Option<&'a Relecture> {
    vagues.iter().rev().filter_map(|vague| vague.relecture.as_ref()).find(|relecture| relecture.relue == id)
}

/// La dernière correction notée de la vague `id`.
pub(crate) fn correction_de<'a>(vagues: &'a [Vague], id: &str) -> Option<&'a Vague> {
    vagues.iter().rev().find(|vague| vague.corrige.as_deref() == Some(id))
}

/// Un refus dit en clair, en trois lignes : `refusée (<code>) : <règle>`, puis sa preuve, puis sa correction, chacune
/// sur une ligne. Une preuve ou une correction sur plusieurs lignes (la sortie de git) se met bout à bout. `lancer` le
/// montre à l'humain, et `lydite etat` le tire de `vague.refus` : l'un et l'autre disent le même texte.
pub(crate) fn refus_en_clair(code: u16, regle: &str, preuve: &str, correction: &str) -> String {
    format!("refusée ({code}) : {regle}\n{}\n{}", sur_une_ligne(preuve), sur_une_ligne(correction))
}

/// Le texte sur une seule ligne : ses lignes, rognées et non vides, mises bout à bout. La même mise bout à bout que
/// celle du refus de `erreur.rs`, et tout autre caractère de contrôle y est écrit (`coeur::texte`).
fn sur_une_ligne(texte: &str) -> String {
    let bout_a_bout =
        texte.split(['\r', '\n']).map(str::trim).filter(|ligne| !ligne.is_empty()).collect::<Vec<_>>().join(" ; ");
    crate::coeur::texte::en_ligne(&bout_a_bout)
}

impl Evenement {
    /// Son type, `<domaine>.<quoi>`.
    fn genre(&self) -> &'static str {
        match self {
            Evenement::Vague { .. } => "vague.ouverte",
            Evenement::Debut { .. } => "main.debut",
            Evenement::Lancement { .. } => "main.lancement",
            Evenement::Constat { .. } => "main.constat",
            Evenement::Fin { .. } => "main.fin",
            Evenement::Verdict { .. } => "main.verdict",
            Evenement::Assemblage { .. } => "vague.assemblage",
            Evenement::Tache { .. } => "vague.tache",
            Evenement::Bail { .. } => "main.bail",
            Evenement::Perimee { .. } => "main.perimee",
            Evenement::Fusion { .. } => "vague.fusion",
            Evenement::Gel { .. } => "main.gel",
            Evenement::Empreintes { .. } => "main.empreintes",
            Evenement::Refus { .. } => "vague.refus",
            Evenement::Poussee { .. } => "forge.poussee",
            Evenement::Pr { .. } => "forge.pr",
            Evenement::Ci { .. } => "forge.ci",
            Evenement::RefusForge { .. } => "forge.refus",
            Evenement::Rangement { .. } => "forge.rangement",
            Evenement::Decoupage { .. } => "vague.decoupage",
            Evenement::Joue { .. } => "vague.joue",
            Evenement::Relecture { .. } => "vague.relecture",
            Evenement::VerdictRelecture { .. } => "vague.verdict",
            Evenement::Correction { .. } => "vague.correction",
            Evenement::Relecteur { .. } => "vague.relecteur",
            Evenement::Coupure { .. } => "main.coupure",
            Evenement::Fournisseur { .. } => "main.fournisseur",
            Evenement::Consigne { .. } => "main.consigne",
            Evenement::Usage { .. } => "main.usage",
        }
    }

    /// La vague dont il parle : chaque événement porte la sienne.
    pub(crate) fn vague(&self) -> &str {
        let (Evenement::Vague { vague, .. }
        | Evenement::Assemblage { vague, .. }
        | Evenement::Tache { vague, .. }
        | Evenement::Bail { vague, .. }
        | Evenement::Debut { vague, .. }
        | Evenement::Lancement { vague, .. }
        | Evenement::Constat { vague, .. }
        | Evenement::Fin { vague, .. }
        | Evenement::Perimee { vague, .. }
        | Evenement::Verdict { vague, .. }
        | Evenement::Fusion { vague, .. }
        | Evenement::Gel { vague, .. }
        | Evenement::Empreintes { vague, .. }
        | Evenement::Refus { vague, .. }
        | Evenement::Poussee { vague, .. }
        | Evenement::Pr { vague, .. }
        | Evenement::Ci { vague, .. }
        | Evenement::RefusForge { vague, .. }
        | Evenement::Rangement { vague, .. }
        | Evenement::Decoupage { vague, .. }
        | Evenement::Joue { vague, .. }
        | Evenement::Relecture { vague, .. }
        | Evenement::VerdictRelecture { vague, .. }
        | Evenement::Correction { vague, .. }
        | Evenement::Relecteur { vague, .. }
        | Evenement::Coupure { vague, .. }
        | Evenement::Fournisseur { vague, .. }
        | Evenement::Consigne { vague, .. }
        | Evenement::Usage { vague, .. }) = self;
        vague
    }

    /// Les champs de son corps, dans l'ordre de l'en-tête.
    fn champs(&self) -> Vec<(&'static str, Valeur)> {
        let texte = |texte: &str| Valeur::Texte(texte.into());
        let ou_nul = |texte: &Option<String>| texte.as_deref().map_or(Valeur::Nul, |texte| Valeur::Texte(texte.into()));
        let entier = |n: usize| Valeur::Entier(n as u64);
        match self {
            Evenement::Vague { vague, n, depot, verifier, objectif } => vec![
                ("vague", texte(vague)),
                ("n", entier(*n)),
                ("depot", texte(depot)),
                ("verifier", ou_nul(verifier)),
                ("objectif", texte(objectif)),
            ],
            Evenement::Debut { vague, main, tentative } => {
                vec![("vague", texte(vague)), ("main", entier(*main)), ("tentative", entier(*tentative))]
            }
            Evenement::Lancement {
                vague,
                main,
                tentative,
                profil,
                empreinte,
                modele,
                effort,
                mur,
                reglages,
                reglages_depot,
            } => {
                let mut corps = vec![
                    ("vague", texte(vague)),
                    ("main", entier(*main)),
                    ("tentative", entier(*tentative)),
                    ("profil", texte(profil)),
                    ("empreinte", texte(empreinte)),
                    ("modele", texte(modele)),
                ];
                // Sans effort déclaré, la clé manque : la ligne est celle d'avant ce champ.
                corps.extend(effort.as_deref().map(|effort| ("effort", texte(effort))));
                corps.extend([
                    ("mur", texte(mur.mot())),
                    ("reglages", texte(reglages)),
                    ("reglages_depot", texte(reglages_depot)),
                    ("foi", texte(Foi::Declare.mot())),
                ]);
                corps
            }
            Evenement::Constat { vague, main, tentative, version, mur_actif } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("tentative", entier(*tentative)),
                ("version", ou_nul(version)),
                ("mur_actif", mur_actif.map_or(Valeur::Nul, Valeur::Booleen)),
                ("foi", texte(Foi::Constate.mot())),
            ],
            Evenement::Fin { vague, main, tentative, couleur, foi, texte: sortie, verif }
            | Evenement::Perimee { vague, main, tentative, couleur, foi, texte: sortie, verif } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("tentative", entier(*tentative)),
                ("couleur", texte(couleur.mot())),
                ("foi", texte(foi.mot())),
                ("texte", texte(sortie)),
                ("verif", ou_nul(verif)),
            ],
            Evenement::Verdict { vague, main, verdict, foi } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("verdict", texte(verdict.mot())),
                ("foi", texte(foi.mot())),
            ],
            Evenement::Assemblage { vague, branche, base, preuve } => vec![
                ("vague", texte(vague)),
                ("branche", texte(branche)),
                ("base", texte(base)),
                ("preuve", texte(preuve)),
            ],
            Evenement::Tache { vague, main, id, depend, preuve, objectif } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("id", texte(id)),
                ("depend", texte(&depend.join("\n"))),
                ("preuve", texte(preuve)),
                ("objectif", texte(objectif)),
            ],
            Evenement::Bail { vague, main, tentative, echeance } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("tentative", entier(*tentative)),
                ("echeance", texte(echeance)),
            ],
            Evenement::Fusion { vague, main, sha, verif } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("sha", texte(sha)),
                ("verif", texte(verif)),
                ("foi", texte(Foi::Constate.mot())),
            ],
            Evenement::Gel { vague, main, sha } => {
                vec![("vague", texte(vague)), ("main", entier(*main)), ("sha", texte(sha))]
            }
            Evenement::Empreintes { vague, main, sha, fichiers } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("sha", texte(sha)),
                ("fichiers", texte(&fichiers.join("\n"))),
            ],
            Evenement::Refus { vague, main, code, regle, preuve, correction, verif } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("code", Valeur::Entier(u64::from(*code))),
                ("regle", texte(regle)),
                ("preuve", texte(preuve)),
                ("correction", texte(correction)),
                ("verif", ou_nul(verif)),
            ],
            Evenement::Poussee { vague, branche, sha } => {
                vec![("vague", texte(vague)), ("branche", texte(branche)), ("sha", texte(sha))]
            }
            Evenement::Pr { vague, branche, base, sha, numero, url, foi } => vec![
                ("vague", texte(vague)),
                ("branche", texte(branche)),
                ("base", texte(base)),
                ("sha", texte(sha)),
                ("numero", Valeur::Entier(*numero)),
                ("url", texte(url)),
                ("foi", texte(foi.mot())),
            ],
            Evenement::Ci { vague, numero, sha, couleur, texte: sortie, foi } => vec![
                ("vague", texte(vague)),
                ("numero", Valeur::Entier(*numero)),
                ("sha", texte(sha)),
                ("couleur", texte(couleur.mot())),
                ("texte", texte(sortie)),
                ("foi", texte(foi.mot())),
            ],
            Evenement::RefusForge { vague, code, regle, preuve, correction } => vec![
                ("vague", texte(vague)),
                ("code", Valeur::Entier(u64::from(*code))),
                ("regle", texte(regle)),
                ("preuve", texte(preuve)),
                ("correction", texte(correction)),
            ],
            Evenement::Rangement { vague, retires } => {
                vec![("vague", texte(vague)), ("retires", texte(&retires.join("\n")))]
            }
            Evenement::Decoupage { vague, taches, empreinte, plan } => vec![
                ("vague", texte(vague)),
                ("taches", entier(*taches)),
                ("empreinte", texte(empreinte)),
                ("plan", texte(plan)),
            ],
            Evenement::Joue { vague, decoupage } => vec![("vague", texte(vague)), ("decoupage", texte(decoupage))],
            Evenement::Relecture { vague, relue, n, bloquantes, remarques } => vec![
                ("vague", texte(vague)),
                ("relue", texte(relue)),
                ("n", entier(*n)),
                ("bloquantes", entier(*bloquantes)),
                ("remarques", texte(remarques)),
            ],
            Evenement::VerdictRelecture { vague, relue, sha, profil, empreinte, modele } => vec![
                ("vague", texte(vague)),
                ("relue", texte(relue)),
                ("sha", texte(sha)),
                ("profil", ou_nul(profil)),
                ("empreinte", ou_nul(empreinte)),
                ("modele", ou_nul(modele)),
            ],
            Evenement::Correction { vague, corrige } => vec![("vague", texte(vague)), ("corrige", texte(corrige))],
            Evenement::Relecteur { vague, relue } => vec![("vague", texte(vague)), ("relue", texte(relue))],
            Evenement::Coupure { vague, main, tentative, statut } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("tentative", entier(*tentative)),
                ("statut", Valeur::Entier(u64::from(*statut))),
            ],
            Evenement::Fournisseur { vague, main, tentative, fournisseur } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("tentative", entier(*tentative)),
                ("fournisseur", texte(fournisseur)),
                ("foi", texte(Foi::Declare.mot())),
            ],
            Evenement::Consigne { vague, main, tentative, role, consigne, systeme, lecons, ecartees, mesure } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("tentative", entier(*tentative)),
                ("role", texte(role.mot())),
                ("consigne", texte(consigne)),
                ("systeme", ou_nul(systeme)),
                ("lecons", texte(lecons)),
                ("ecartees", texte(ecartees)),
                ("mesure", texte(mesure)),
            ],
            Evenement::Usage {
                vague,
                main,
                tentative,
                modele,
                entree: jetons_entree,
                sortie,
                cache_ecrit,
                cache_lu,
                reflexion,
                micro_dollars,
                base,
            } => vec![
                ("vague", texte(vague)),
                ("main", entier(*main)),
                ("tentative", entier(*tentative)),
                ("modele", texte(modele)),
                ("entree", Valeur::Entier(*jetons_entree)),
                ("sortie", Valeur::Entier(*sortie)),
                ("cache_ecrit", Valeur::Entier(*cache_ecrit)),
                ("cache_lu", Valeur::Entier(*cache_lu)),
                ("reflexion", Valeur::Entier(*reflexion)),
                ("micro_dollars", Valeur::Entier(*micro_dollars)),
                ("base", texte(base.mot())),
                ("foi", texte(Foi::Declare.mot())),
            ],
        }
    }
}

pub(crate) fn maintenant() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

/// Un identifiant de lot que personne d'autre ne tirera : `<ms>_<processus>_<tirage>`, sans tiret, pour que
/// `main-<lot>-<n>` se coupe sur les tirets. Les bacs de tous les lancements et de tous les dépôts partagent le
/// dossier de Kraken : la milliseconde seule ne départage ni deux processus, ni deux tirages du même.
// raccourci : suppose un dossier de Kraken par machine (deux machines de même pid et de même ms se heurteraient) ;
// monter si un dossier se partage entre machines.
pub(crate) fn lot() -> String {
    static TIRAGES: AtomicU32 = AtomicU32::new(0);
    let ms = maintenant();
    format!("{ms}_{}_{}", process::id(), TIRAGES.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests;
