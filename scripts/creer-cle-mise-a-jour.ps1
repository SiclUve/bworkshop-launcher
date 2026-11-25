# Crée la clé qui signe les mises à jour du launcher. À faire UNE SEULE FOIS.
#
# - La clé privée va dans %USERPROFILE%\.tauri\bworkshop-launcher.key, HORS du dépôt.
#   Garde-la précieusement (et une copie de secours) : sans elle, plus aucune mise à jour
#   ne pourra être publiée pour les launchers déjà installés.
# - Le mot de passe t'est demandé par l'outil de Tauri : choisis-le toi-même et note-le.
# - La clé publique est copiée dans src-tauri\updater-key.pub (elle, peut être publiée).

# -Remplacer : écrase la clé existante. Seulement tant qu'AUCUN installeur signé avec
# l'ancienne n'a été distribué (sinon ces launchers ne recevraient plus de mise à jour).
param([switch]$Remplacer)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$key = Join-Path $env:USERPROFILE '.tauri\bworkshop-launcher.key'
$pub = Join-Path $root 'src-tauri\updater-key.pub'

if ((Test-Path $key) -and -not $Remplacer) {
    Write-Host "Une clé existe déjà : $key"
    Write-Host 'Elle n''est pas remplacée (les launchers installés ne reconnaîtraient plus les mises à jour).'
    exit 1
}
New-Item -ItemType Directory -Force (Split-Path $key) | Out-Null

# Le mot de passe est demandé ici (des étoiles s'affichent) : la saisie de l'outil
# de Tauri ne lit pas le clavier dans certains terminaux.
function Read-Secret([string]$prompt) {
    $s = Read-Host $prompt -AsSecureString
    [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($s))
}
$pass = Read-Secret 'Mot de passe de la clé (à noter !)'
if ($pass.Length -lt 8) { throw 'Mot de passe trop court (8 caractères minimum).' }
# Windows PowerShell transmet mal un guillemet dans un argument : la clé aurait un autre mot de passe.
if ($pass.Contains('"')) { throw 'Pas de guillemet (") dans le mot de passe.' }
if ((Read-Secret 'Retape-le pour confirmer') -ne $pass) { throw 'Les deux mots de passe ne correspondent pas. Relance le script.' }

Push-Location $root
try {
    $force = @(); if ($Remplacer) { $force = @('--force') }
    node (Join-Path $root 'node_modules\@tauri-apps\cli\tauri.js') signer generate -w $key -p $pass --ci @force
    if ($LASTEXITCODE -ne 0) { throw 'La création de la clé a échoué.' }
} finally {
    Pop-Location
    $pass = $null
}
if (-not (Test-Path "$key.pub")) { throw 'La clé n''a pas été créée.' }

$content = (Get-Content "$key.pub" -Raw).Trim()
[IO.File]::WriteAllText($pub, $content, (New-Object Text.UTF8Encoding $false))
Write-Host ''
Write-Host "Clé privée : $key  (ne jamais la mettre dans le dépôt)"
Write-Host "Clé publique copiée dans : $pub"
Write-Host 'Reconstruis le launcher : la mise à jour automatique est maintenant active.'
