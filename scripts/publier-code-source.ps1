# Envoie le code source du launcher sur GitHub (https://github.com/SiclUve/bworkshop-launcher).
#
# Avant : créer le dépôt VIDE sur https://github.com/new
#   - propriétaire SiclUve, nom « bworkshop-launcher », Public ;
#   - ne PAS cocher README, .gitignore ni licence (ils existent déjà ici).
# Usage : .\scripts\publier-code-source.ps1            (vérifie seulement)
#         .\scripts\publier-code-source.ps1 -Envoyer   (vérifie puis envoie)

param([switch]$Envoyer)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$remote = 'https://github.com/SiclUve/bworkshop-launcher.git'
Set-Location $root

function Stop-Publication([string]$raison) {
    Write-Host "STOP : $raison" -ForegroundColor Red
    exit 1
}

# 1. Rien en attente : on publie exactement ce qui est commité.
if (git status --porcelain) { Stop-Publication 'il reste des changements non commités (git status).' }

# 2. Aucun secret suivi par git.
$suivis = git ls-files
$secrets = $suivis | Where-Object { $_ -match '\.(key|pem|pfx|p12)$' -or $_ -match '(^|/)\.env' }
if ($secrets) { Stop-Publication "fichiers sensibles suivis : $($secrets -join ', ')" }
$cle = Join-Path $env:USERPROFILE '.tauri\bworkshop-launcher.key'
if (Test-Path $cle) {
    $prive = (Get-Content $cle -Raw).Trim()
    if ($prive -and (git grep -l -F -e $prive)) { Stop-Publication 'la clé privée de signature apparaît dans un fichier suivi.' }
}

# 3. Pas de chemin personnel ni d'adresse e-mail dans le code.
$moi = [regex]::Escape("Users\$env:USERNAME")
$perso = git grep -n -i -E "$moi|@gmail\.|@outlook\.|@hotmail\." -- . ':!package-lock.json'
if ($perso) { Stop-Publication "données personnelles trouvées :`n$perso" }

# 4. Binaires suivis : signalés (un .exe dans un dépôt public fait tiquer les antivirus).
$exe = $suivis | Where-Object { $_ -match '\.(exe|dll|msi)$' }
if ($exe) {
    Write-Host 'Attention, binaires suivis (installeurs de test) :' -ForegroundColor Yellow
    $exe | ForEach-Object { Write-Host "  $_" }
}

Write-Host ''
Write-Host "Branche  : $(git branch --show-current)"
Write-Host "Commits  : $(git rev-list --count HEAD)"
Write-Host "Fichiers : $($suivis.Count)"
Write-Host "Auteurs  :"; git log --format='  %an <%ae>' | Sort-Object -Unique
Write-Host ''

if (-not $Envoyer) {
    Write-Host 'Vérifications OK. Relance avec -Envoyer pour publier.' -ForegroundColor Green
    exit 0
}

if (git remote | Select-String -SimpleMatch 'origin') { git remote set-url origin $remote }
else { git remote add origin $remote }
git push -u origin main
if ($LASTEXITCODE -ne 0) { Stop-Publication "l'envoi a échoué (le dépôt existe-t-il, vide, sur GitHub ?)." }
Write-Host "Publié : https://github.com/SiclUve/bworkshop-launcher" -ForegroundColor Green
