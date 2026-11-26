# Sécurité

## Télécharger le launcher

La seule source officielle est la page de téléchargement de [bworkshop.app](https://bworkshop.app),
qui renvoie vers les releases de [SiclUve/bworkshop-releases](https://github.com/SiclUve/bworkshop-releases/releases).
Un installateur obtenu ailleurs n'est pas le nôtre.

L'installateur n'est pas signé avec un certificat de signature de code : Windows SmartScreen
peut afficher un avertissement. Avant de passer outre, vérifie le fichier.

### 1. Empreinte SHA-256

Elle est affichée sur la page de téléchargement, dans les notes de la release et dans le fichier
`.sha256` publié à côté de l'installateur :

```powershell
Get-FileHash "$env:USERPROFILE\Downloads\Bworkshop.Launcher_<version>_x64-setup.exe" -Algorithm SHA256
```

Les deux empreintes doivent être identiques, caractère pour caractère.

### 2. Provenance (facultatif)

Chaque installateur est construit par GitHub Actions à partir du code de
[SiclUve/bworkshop-launcher](https://github.com/SiclUve/bworkshop-launcher), jamais sur un PC,
avec une attestation de provenance. Avec l'outil `gh` de GitHub :

```powershell
gh attestation verify "Bworkshop.Launcher_<version>_x64-setup.exe" --repo SiclUve/bworkshop-launcher
```

La release indique aussi le commit exact, marqué par le tag `v<version>`.

## Mises à jour automatiques

Elles sont signées avec une clé dédiée (minisign). Seule la clé publique est dans le launcher
(`src-tauri/updater-key.pub`) : une mise à jour dont la signature ne correspond pas est refusée,
et une version plus ancienne que celle installée n'est jamais proposée.

## Ce que le launcher s'interdit

Il ne touche jamais au jeu (pas de lecture mémoire, d'injection, de clic simulé ni de capture
d'écran), n'envoie aucune donnée (aucune télémétrie) et ne télécharge qu'en HTTPS, depuis une
liste fermée de domaines, des fichiers dont l'empreinte SHA-256 est publiée. Le détail est dans le
[README](README.md#ce-que-le-launcher-ne-fait-pas).

## Signaler une faille

Merci de ne pas l'exposer publiquement avant correction. Ouvre un signalement privé depuis
l'onglet **Security** du dépôt
([« Report a vulnerability »](https://github.com/SiclUve/bworkshop-launcher/security/advisories/new)),
avec la version concernée et les étapes pour reproduire.
