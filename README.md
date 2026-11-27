# Bworkshop Launcher

Gestionnaire des outils Dofus publiés sur [bworkshop.app](https://bworkshop.app) :
il les liste, les installe, les met à jour et les lance, et il ouvre leurs
versions web en petits widgets au-dessus du jeu.

> Fonctionnel de bout en bout : catalogue, installation, mises à jour, widgets, barre
> d'outils, dispositions, mise à jour du launcher. La version en cours est celle de
> `package.json` ; l'historique est dans les [releases](https://github.com/SiclUve/bworkshop-releases/releases).

## Installer

Télécharger `Bworkshop Launcher_<version>_x64-setup.exe` sur bworkshop.app, puis le lancer.

- Installation **pour l'utilisateur seul**, sans droits administrateur
  (`%LOCALAPPDATA%\Bworkshop Launcher`), raccourcis dans le menu Démarrer (dossier « Bworkshop ») et sur le Bureau.
- Le moteur web WebView2, déjà présent sur Windows 10 et 11 à jour, est installé au besoin.
- Taille : ≈ 3 Mo.
- Désinstallation : Paramètres Windows → Applications. Elle retire aussi le lancement au
  démarrage de Windows. Les outils installés par le launcher restent installés, et tes
  réglages restent dans `%APPDATA%\BworkshopLauncher` (à supprimer à la main si tu veux).

### Avertissement « Windows a protégé votre ordinateur »

L'installateur n'est pas signé avec un certificat de signature de code (payant, ≈ 200 à
400 € par an). Windows SmartScreen affiche donc un avertissement les premières fois :
« Informations complémentaires » puis « Exécuter quand même ». L'avertissement disparaît
de lui-même quand assez de personnes ont installé le même fichier.

Pour vérifier que le fichier est bien celui publié, compare son empreinte SHA-256 avec
celle indiquée sur la page de téléchargement (et dans le fichier `.sha256` de la release) :

```powershell
Get-FileHash "$env:USERPROFILE\Downloads\Bworkshop.Launcher_<version>_x64-setup.exe" -Algorithm SHA256
```

Chaque installateur est construit par GitHub Actions à partir du code publié, avec une
attestation de provenance vérifiable (voir [SECURITY.md](SECURITY.md)).

Les **mises à jour automatiques**, elles, sont toujours signées (voir plus bas) : le
launcher refuse toute mise à jour dont la signature ne correspond pas.

### Mac

Télécharger `Bworkshop.Launcher_<version>_universal.dmg` (Apple Silicon et Intel, macOS 11 ou
plus récent), l'ouvrir et glisser l'application dans « Applications ».

- L'application n'est pas signée par un compte développeur Apple (signature « ad hoc »). Au
  premier lancement, macOS refuse de l'ouvrir : fermer le message, puis Réglages Système →
  Confidentialité et sécurité → en bas, « Ouvrir quand même » (mot de passe du Mac demandé).
  Sur macOS 14 ou plus ancien, clic droit sur l'application → Ouvrir → Ouvrir suffit aussi.
  Ensuite, elle s'ouvre normalement. Même chose pour chaque outil installé.
- Les outils sont lancés sous leur propre nom : macOS leur demande à eux les autorisations
  (enregistrement de l'écran, surveillance de l'entrée), pas au launcher.
- Réglages, journal et téléchargements : `~/Library/Application Support/BworkshopLauncher/`.
- Les outils sont des images disque. Catalogue officiel : `downloadMacArm64` (puce Apple) et
  `downloadMacX64` (Intel), sinon `downloadMac` (universelle). Communauté : la paire
  `downloadUrlMacArm64` + `sha256MacArm64` (ou `…MacX64`), sinon `downloadUrlMac` + `sha256Mac`.
  Leur application est copiée dans
  `~/Applications` (sans mot de passe administrateur). Désinstaller la met à la corbeille.
  Un outil sans version Mac n'a pas de bouton « Installer » (sa version web reste disponible).
- Lancement à l'ouverture de session : `~/Library/LaunchAgents/app.bworkshop.launcher.plist`.
- Raccourcis : Win = ⌘, Alt = ⌥ ; écoutés par macOS (`RegisterEventHotKey`), sans autorisation
  « Surveillance de l'entrée ».
- Suivre Dofus : lecture de la liste des fenêtres 6 fois par seconde quand l'option est activée
  (rien sinon). Dofus est reconnu au nom de son application ; son titre seulement si
  « Enregistrement de l'écran » a été accordé (jamais demandé par le launcher).
- Pas encore sur Mac : l'aimantation pendant qu'on déplace une fenêtre.

## Ce que le launcher ne fait pas

Le launcher **ne touche jamais au jeu** :

- il ne lit pas la mémoire de Dofus, n'y injecte rien et ne lit pas son trafic réseau ;
- il n'ouvre jamais de poignée (handle) sur le processus de Dofus : pas
  d'`OpenProcess`, pas de lecture du nom de l'exécutable, pas d'énumération de
  ses modules ;
- il n'envoie aucun clic ni aucune touche, ni au jeu ni à une autre fenêtre ;
- il ne fait aucune capture d'écran ;
- il ne s'injecte dans aucun jeu pour s'afficher par-dessus.

Pour savoir où est Dofus, il demande seulement à Windows, comme Discord, OBS ou PowerToys :

- quelle fenêtre est au premier plan (`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`) ;
- le titre de cette fenêtre (`GetWindowTextW`), pour reconnaître Dofus ;
- sa position et sa taille (`DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`),
  et être prévenu quand elle bouge (`EVENT_OBJECT_LOCATIONCHANGE`) ;
- le numéro de processus de cette fenêtre (`GetWindowThreadProcessId`), qui
  renvoie seulement un nombre, **sans ouvrir de poignée**, pour limiter les
  événements de déplacement à cette seule fenêtre ;
- l'état de la touche Maj (`GetAsyncKeyState`) pendant qu'on déplace **un
  widget du launcher**, pour désactiver l'aimantation.

Les raccourcis clavier sont seulement écoutés (`RegisterHotKey`), jamais simulés.

## Suivre Dofus (option, désactivée par défaut)

Bouton « Réglages » du launcher, partie « En jeu », case « Suivre la fenêtre de Dofus ».
Désactivée, le launcher ne demande rien à Windows sur les autres fenêtres. Activée :

- les versions web et la barre **posées sur le jeu ou collées à ses bords** le suivent
  quand tu le déplaces (pas quand tu le réduis, l'agrandis ou le redimensionnes) ;
- l'aimantation (voir ci-dessous) colle aussi aux bords de Dofus ;
- les dispositions retiennent la place des fenêtres **par rapport au jeu** ;
- « Cacher quand le jeu n'est pas devant » : versions web et barre se cachent quand une
  autre fenêtre passe au premier plan, et reviennent avec le jeu (ou le launcher). Les
  logiciels lancés depuis le launcher ne comptent pas comme « une autre fenêtre ».

Dofus est reconnu au titre de sa fenêtre (réglage « Titre de la fenêtre du jeu »,
« Dofus » par défaut). Avec « Dofus », le titre de Dofus 3 en jeu, qui ne contient pas
« Dofus » (« Personnage - Classe - 3.7.14.18 - Release »), est aussi reconnu. Un autre
texte, par exemple le nom d'un personnage, ne suit que les fenêtres qui le contiennent. Les logiciels
lancés par le launcher ne sont jamais pris pour le jeu, même si leur titre contient « Dofus ».
Seuls les appels listés ci-dessus sont utilisés. Les écoutes sont « hors contexte »
(`WINEVENT_OUTOFCONTEXT`) : rien n'est chargé dans le jeu.

## Aimantation

Réglage « Aimanter les fenêtres » (activé par défaut). En déplaçant une version web ou la
barre, elle se colle à moins de 12 px des bords de l'écran, des autres fenêtres du
launcher et, avec « Suivre Dofus », des bords du jeu. Maj enfoncée pendant le
déplacement : pas d'aimantation.

## Dispositions

Une disposition retient quelles versions web sont ouvertes, leur place, leur taille, leur
transparence, si elles sont repliées, et la place de la barre d'outils.

- Enregistrer : bouton « Dispositions » du launcher, ou menu de la barre d'outils →
  « Dispositions » → « Enregistrer l'état actuel… ». 20 dispositions au plus.
- Rappeler : Alt+1 à Alt+4 pour les quatre premières, ou depuis la fenêtre « Dispositions »,
  le menu de la barre d'outils ou le menu de l'icône près de l'horloge.
- Rappeler ferme les versions web qui n'en font pas partie et ouvre celles qui manquent.
  La barre d'outils n'est jamais fermée par une disposition.
- Avec « Suivre Dofus », chaque fenêtre est retenue par rapport au bord du jeu le plus
  proche : la disposition reste juste si le jeu a bougé ou changé de taille. Sinon, la
  place est retenue à l'écran (par écran, en tenant compte du zoom Windows).
- Réglage « Rouvrir la dernière disposition au démarrage ».
- Fichier : `%APPDATA%\BworkshopLauncher\layouts.json`.

## Raccourcis

| Par défaut | Action |
|---|---|
| Alt+B | Afficher / cacher la barre d'outils (sans prendre le focus au jeu) |
| Alt+H | Tout cacher / tout montrer |
| Alt+L | Verrouiller / déverrouiller les versions web (les clics passent au jeu) |
| Alt+1 à Alt+4 | Rappeler la disposition 1 à 4 |

Ils se changent dans le launcher, bouton « Raccourcis ». Une combinaison déjà prise par
un autre programme est signalée, sans être forcée. Une combinaison prise par le launcher
ne parvient plus aux autres programmes, Dofus compris : en cas de gêne, change-la ou désactive-la.
Refusés : une touche seule (sauf F1 à F24), Alt+F4, Alt+Tab, Alt+Espace, Win + lettre.

**Une seule exception, qui ne concerne jamais Dofus :** dans la barre d'outils, un
clic sur la tuile d'un logiciel **que le launcher a lui-même lancé** lui demande de
se fermer, comme un clic sur sa croix (`WM_CLOSE`, envoyé par `PostMessageW`).

- Seul le logiciel démarré par le launcher est visé. Il est démarré suspendu, placé dans
  un groupe de processus Windows (« job », `CreateJobObjectW` puis
  `AssignProcessToJobObject` avec la poignée rendue au lancement), puis repris
  (`ResumeThread` sur son propre fil). Les processus que ce logiciel ouvre lui-même y
  entrent automatiquement, dès sa première instruction, et aucun autre. Un outil qui démarre
  par un petit lanceur intermédiaire reste suivi tant qu'un processus de son groupe tourne.
  Quitter le launcher ne ferme pas les logiciels du groupe.
- Garde-fou : si un outil démarrait lui-même l'Ankama Launcher (et donc Dofus), ces
  processus entreraient dans son groupe. Tout processus du groupe, autre que celui lancé,
  qui a une fenêtre au titre de Dofus ou d'Ankama est donc écarté (titre lu avec
  `GetWindowTextW`, sans poignée) : il ne reçoit jamais `WM_CLOSE`, et l'arrêt forcé ne vise
  alors que le processus lancé, plus le groupe.
- Seules ses fenêtres principales (visibles, sans propriétaire) reçoivent le message,
  trouvées avec `EnumWindows` et `GetWindowThreadProcessId`.
- Le titre des fenêtres n'est pas utilisé pour choisir la cible (un outil peut
  s'appeler « Dofus Color Picker ») : seule l'appartenance au groupe compte.
- Aucun clic, aucune touche : le logiciel peut proposer d'enregistrer ou refuser de se fermer.
- S'il tourne encore 3 secondes plus tard (certains se cachent près de l'horloge au
  lieu de quitter), sa tuile passe en rouge pendant une minute. Après, un clic redemande
  une fermeture normale. Un nouveau clic pendant cette minute l'arrête de force
  (`TerminateJobObject` sur son groupe), **seulement pour ce que le launcher a lui-même
  démarré** : aucune poignée n'est ouverte sur un autre processus. Ce qui n'était pas enregistré dans ce logiciel est alors perdu.

Les widgets s'affichent par-dessus Dofus. Si un autre jeu utilisait un vrai
plein écran exclusif, une fenêtre normale ne pourrait pas passer par-dessus, et
le launcher ne s'injecte jamais dans un jeu pour contourner ça.

## Mesures

Mesures du 25/11/2026 (version 0.1.0, installateur ≈ 3 Mo depuis 0.1.4), compilée en
`release`, Windows 11, sur la machine de développement. Mémoire privée du launcher + des processus du moteur web WebView2 qui
lui appartiennent (`Get-Process`, processus `msedgewebview2.exe` dont la ligne de commande
contient `app.bworkshop.launcher`).

| Mesure | Résultat | Objectif |
|---|---|---|
| Taille de l'installateur | 2,7 Mo (3,1 Mo en 0.1.4) | < 10 Mo |
| Démarrage jusqu'à la première fenêtre | 30 à 80 ms | — |
| Démarrage jusqu'au moteur web prêt | 230 à 400 ms | — |
| Mémoire, près de l'horloge, rien d'ouvert | ≈ 4 Mo | < 40 Mo |
| Mémoire, barre d'outils ouverte | ≈ 100 à 140 Mo | — |
| Mémoire, fenêtre du launcher + barre | ≈ 140 Mo | — |
| Processeur au repos (barre ouverte, 20 s) | 0,8 % d'un cœur | — |

- Les versions web partagent le même moteur web : chacune ajoute surtout le poids de sa
  propre page, qui dépend de l'outil (non mesuré outil par outil).
- La fenêtre du launcher n'existe que lorsqu'elle est affichée : fermée, sa page est libérée.
- Les pages sont rendues sans carte graphique (`--disable-gpu`), ≈ 60 Mo de moins.
- Le moteur web coûte ≈ 60 Mo dès qu'une fenêtre l'utilise (barre ou version web).
- Non mesuré : le délai d'Alt+B en jeu (à faire pendant les tests manuels, à l'œil :
  la barre doit apparaître sans délai visible).

## Mise à jour du launcher

Le launcher vérifie en silence, 20 s après le démarrage puis toutes les 6 heures, s'il
existe une nouvelle version. Rien ne surgit en jeu : la mise à jour est seulement
**proposée** dans la fenêtre du launcher, et ne s'installe que sur un clic (le launcher se
ferme, s'installe puis redémarre ; les outils ne sont pas touchés).

- Chaque version est **signée** avec une clé dédiée. Seule la clé publique est dans le
  launcher (`src-tauri/updater-key.pub`) ; une signature invalide fait refuser la mise à jour.
- La version signée doit être celle annoncée : impossible de faire réinstaller une
  ancienne version en trafiquant le fichier d'annonce.
- L'annonce est `latest.json`, dans la release fixe `launcher-latest` de
  https://github.com/SiclUve/bworkshop-releases (domaine autorisé).
- Tant que `src-tauri/updater-key.pub` est vide, la mise à jour automatique est désactivée.

**Mettre en place (une seule fois)** :

```powershell
.\scripts\creer-cle-mise-a-jour.ps1
```

La clé privée va dans `%USERPROFILE%\.tauri\bworkshop-launcher.key`, **hors du dépôt**.
Choisis et note son mot de passe, et garde une copie de secours de la clé : sans elle,
plus aucune mise à jour ne pourra être publiée pour les launchers déjà installés.

La clé privée et son mot de passe sont aussi enregistrés comme secrets du dépôt
(`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), avec un jeton limité
au dépôt des releases (`RELEASES_TOKEN`).

**Publier une version** (tout se construit sur GitHub, rien sur un PC) :

1. Changer la version, identique partout : `Cargo.toml` (racine, `[workspace.package]`),
   `src-tauri/tauri.conf.json`, `package.json`, puis `npm install` et `cargo check` pour
   mettre à jour `package-lock.json` et `Cargo.lock`. Commiter et pousser.
2. Actions → « Construire une version » : notes en une phrase. La case « Essai » construit
   tout sans rien publier. Le workflow vérifie la cohérence des versions, lance les tests,
   construit avec `--locked`, signe, atteste la provenance, crée la release
   `Launcher<version>` **en brouillon** et pose le tag `v<version>`.
3. Relire le brouillon (empreinte, signature), puis le publier.
4. Actions → « Annoncer la mise à jour » avec la version : `latest.json` est copié dans la
   release `launcher-latest`, et les launchers installés la proposent.
5. Mettre à jour le lien et le SHA-256 sur la page de téléchargement du site.

## Vie privée

Aucune télémétrie, aucune donnée envoyée. Le journal reste sur ta machine
(`%LOCALAPPDATA%\BworkshopLauncher\logs`, rotation à 5 Mo). Le chemin de ton
profil Windows y est remplacé par `%USERPROFILE%`, et les adresses e-mail par `<e-mail>`.

## Construire

Outils nécessaires, tous gratuits :

- Rust (toolchain `stable-x86_64-pc-windows-msvc`) ;
- Visual Studio Build Tools 2022, charge de travail « Développement Desktop en C++ » ;
- Node.js 22.18 ou plus récent (`npm test` lance directement les fichiers `.ts`) ;
- Inno Setup 6 (seulement pour les tests d'intégration de l'installation).

```powershell
npm install
npm run tauri dev      # développement
npm run tauri build    # installateur dans target\release\bundle\nsis\
cargo test -p bworkshop-core
```

## Ajouter un outil à la table id → AppId

Le launcher reconnaît un outil installé grâce à la clé
`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{AppId}_is1` écrite par Inno Setup.

1. Récupère l'`AppId` dans le script `.iss` de l'outil (ligne `AppId={{…}`), ou installe
   l'outil puis cherche sa clé dans le registre :
   ```powershell
   Get-ChildItem HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall | Where-Object PSChildName -like '*_is1' | ForEach-Object { '{0}  {1}' -f $_.PSChildName, (Get-ItemProperty $_.PSPath).DisplayName }
   ```
2. Ajoute une ligne dans `crates/core/src/installer/known_apps.rs`, avec l'identifiant
   du catalogue (`id` ou `slug`) et l'AppId **avec ses accolades** :
   ```rust
   KnownApp { id: "mon-outil", app_id: "{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}", main_exe: None },
   ```
3. `main_exe` sert seulement si `DisplayIcon` ne pointe pas vers l'exécutable principal
   (chemin relatif au dossier d'installation, par exemple `Some("MonOutil.exe")`).

Sans entrée dans la table, le launcher cherche l'outil par son nom affiché
(« Nom » ou « Nom version X ») parmi les clés Inno.

## Icônes des outils

Le launcher affiche l'icône donnée par le champ `iconUrl` de chaque outil, dans
`catalog.json` (outils officiels) et dans `api/catalog.json` (plugins communautaires) :

```json
{ "id": "assistant-forge", "iconUrl": "/assets/icons/assistant-forge.webp", … }
```

- Une image carrée par outil (256 × 256 conseillé), en PNG, WebP, JPEG ou SVG, 512 Ko au plus.
  Pas de planche de plusieurs icônes. Un SVG n'est affiché que comme image : son script éventuel ne s'exécute pas.
- Adresse relative au site (`/assets/…`) ou en HTTPS sur un domaine autorisé
  (bworkshop.app, github.com/SiclUve/, github.com/rubiconte/). Sinon l'icône est ignorée.
- L'icône est gardée en cache : changer d'image demande de changer d'adresse
  (par exemple `forge.webp?v=2`).
- Sans icône, la carte affiche l'initiale de l'outil.

## Tests

```powershell
cargo test --workspace
npm test
```

Tests d'intégration réels (installent, mettent à jour, lancent puis désinstallent
de petits installateurs Inno Setup et NSIS de test dans `%LOCALAPPDATA%\BworkshopLauncherTest`) :

```powershell
cargo test -p bworkshop-core --test installer_integration -- --ignored --test-threads=1
```

Les installateurs de test se reconstruisent avec `test-assets\inno\build.ps1`
(Inno Setup 6) et `test-assets\nsis\build.ps1` (NSIS 3), Rust étant nécessaire aux deux.

## Installateurs pris en charge

| Type | Reconnu par | Installation | Désinstallation |
|---|---|---|---|
| Inno Setup | signature `rDlPtS` + « Inno Setup Setup Data » | `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /CLOSEAPPLICATIONS /CURRENTUSER` | `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART` |
| NSIS | signature `NullsoftInst` | `/S` | `/S`, seulement si le désinstalleur est lui-même un exécutable NSIS |
| Autre | — | aucune : le fichier est téléchargé, vérifié, puis son dossier est ouvert | — |

Le type est toujours reconnu par la signature du fichier, jamais par son nom.
Un désinstalleur d'un autre type ne reçoit jamais d'arguments « silencieux » :
le launcher indique alors de passer par les paramètres de Windows.

## Fichiers

| Contenu | Emplacement |
|---|---|
| Réglages (`settings.json`), dispositions (`layouts.json`), places des versions web — JSON versionné, écriture atomique | `%APPDATA%\BworkshopLauncher\` |
| Journal, téléchargements, cache | `%LOCALAPPDATA%\BworkshopLauncher\` |

Un fichier de réglages illisible est renommé en `settings.json.corrupt-<horodatage>`
et remplacé par les valeurs par défaut (de même pour `layouts.json`).

## Licence

MIT. Polices Gaegu, Lacquer et Nunito : SIL Open Font License 1.1.
