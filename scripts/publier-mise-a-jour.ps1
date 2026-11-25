# Construit l'installateur signé du launcher et prépare latest.json.
#
# Avant : changer la version dans src-tauri\tauri.conf.json, src-tauri\Cargo.toml et package.json.
# Usage : .\scripts\publier-mise-a-jour.ps1 -Notes "Ce qui change, en une phrase."
#
# Ensuite, sur https://github.com/SiclUve/bworkshop-releases/releases :
#  1. une release « Launcher<version> » avec l'installateur (et son .sig) ;
#  2. dans la release fixe « launcher-latest », remplacer latest.json par le nouveau.

param([string]$Notes = '')

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$key = Join-Path $env:USERPROFILE '.tauri\bworkshop-launcher.key'
$pubFile = Join-Path $root 'src-tauri\updater-key.pub'

if (-not (Test-Path $key)) { throw "Clé privée introuvable ($key). Lance d'abord scripts\creer-cle-mise-a-jour.ps1." }
if (-not (Get-Content $pubFile -Raw -ErrorAction SilentlyContinue)) { throw 'src-tauri\updater-key.pub est vide.' }

$conf = Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$version = $conf.version
$tag = "Launcher$version"

$secure = Read-Host 'Mot de passe de la clé de signature' -AsSecureString
# Le CONTENU de la clé (forme attendue par Tauri), pas son chemin.
$keyContent = (Get-Content $key -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY = $keyContent
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
    [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure))
try {
    Push-Location $root
    # Mot de passe vérifié tout de suite (sinon l'erreur n'arrive qu'après la construction).
    $essai = Join-Path $env:TEMP 'bworkshop-essai-signature.txt'
    [IO.File]::WriteAllText($essai, 'essai')
    $ErrorActionPreference = 'Continue'   # Windows PowerShell : stderr redirigé ne doit pas tout arrêter
    # « signer sign » lit la variable comme le CONTENU de la clé : on passe le fichier avec -f.
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY
    $sortie = node (Join-Path $root 'node_modules\@tauri-apps\cli\tauri.js') signer sign -f $key $essai 2>&1 | Out-String
    $ok = $LASTEXITCODE -eq 0
    $env:TAURI_SIGNING_PRIVATE_KEY = $keyContent
    $ErrorActionPreference = 'Stop'
    Remove-Item $essai, "$essai.sig" -ErrorAction SilentlyContinue
    if (-not $ok) {
        if ($sortie -match 'Wrong password') { throw 'Mot de passe incorrect pour cette clé.' }
        throw "Signature impossible :`n$sortie"
    }
    # Fichier de configuration en plus : produit aussi la signature (.sig) de l'installateur,
    # vérifiée par Tauri avec la clé publique (vide dans tauri.conf.json : le launcher, lui,
    # la lit dans updater-key.pub).
    $extra = Join-Path $env:TEMP 'bworkshop-updater.json'
    $extraConf = @{ bundle = @{ createUpdaterArtifacts = $true }; plugins = @{ updater = @{ pubkey = (Get-Content $pubFile -Raw).Trim() } } }
    [IO.File]::WriteAllText($extra, ($extraConf | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
    npm run tauri build -- --config $extra
    if ($LASTEXITCODE -ne 0) { throw 'La construction a échoué.' }
} finally {
    Pop-Location
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
}

$nsis = Join-Path $root 'target\release\bundle\nsis'
$setup = Get-ChildItem $nsis -Filter "*_${version}_*-setup.exe" | Select-Object -First 1
if (-not $setup) { throw "Installateur introuvable dans $nsis" }
$sig = Get-Content "$($setup.FullName).sig" -Raw
$name = $setup.Name -replace ' ', '.'   # GitHub remplace les espaces par des points
$latest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = $sig.Trim()
            url       = "https://github.com/SiclUve/bworkshop-releases/releases/download/$tag/$name"
        }
    }
}
$out = Join-Path $nsis 'latest.json'
[IO.File]::WriteAllText($out, ($latest | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
$hash = (Get-FileHash $setup.FullName -Algorithm SHA256).Hash.ToLower()
$utf8 = New-Object Text.UTF8Encoding $false
# Empreinte à joindre à la release, au format de sha256sum.
[IO.File]::WriteAllText("$($setup.FullName).sha256", "$hash  $name`n", $utf8)
# Texte de la release, prêt à coller sur GitHub.
$download = "https://github.com/SiclUve/bworkshop-releases/releases/download/$tag/$name"
$body = @"
## Bworkshop Launcher $version

$Notes

**Télécharger :** [$name]($download)

**SHA-256 :** ``$hash``

Vérifier le fichier téléchargé (PowerShell) :

``````powershell
Get-FileHash "`$env:USERPROFILE\Downloads\$name" -Algorithm SHA256
``````

Windows 10 et 11 · installation sans droits administrateur · code source : https://github.com/SiclUve/bworkshop-launcher

L'installateur n'a pas de certificat de signature de code : Windows SmartScreen peut afficher « Windows a protégé votre ordinateur ». Clique sur « Informations complémentaires » puis « Exécuter quand même ».
"@
$notesFile = Join-Path $nsis "release-$tag.md"
[IO.File]::WriteAllText($notesFile, $body, $utf8)

Write-Host ''
Write-Host "Installateur : $($setup.FullName)"
Write-Host "SHA-256      : $hash"
Write-Host "latest.json  : $out"
Write-Host ''
Write-Host "Texte release : $notesFile"
Write-Host ''
Write-Host "1. Release « $tag » : coller le texte ci-dessus, joindre $name (nom exact), son .sig et son .sha256."
Write-Host '2. Release « launcher-latest » : remplacer latest.json par le nouveau.'
Write-Host "3. Sur le site, le bouton de téléchargement pointe vers : $download"
