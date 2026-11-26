# Construit les installateurs de test (1.0.0 et 1.1.0) à partir de TestTool.iss.
# Prérequis : Rust (rustc) et Inno Setup 6.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$iscc = @(
  "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
  "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'ISCC.exe introuvable : installe Inno Setup 6.' }

rustc -C opt-level=s -C strip=symbols -o testtool.exe testtool.rs
foreach ($v in '1.0.0', '1.1.0') {
  & $iscc /Q "/DAppVersion=$v" TestTool.iss
  if ($LASTEXITCODE -ne 0) { throw "ISCC a échoué pour la version $v" }
}
Remove-Item testtool.pdb -ErrorAction SilentlyContinue
Get-ChildItem TestTool_Setup_*.exe | ForEach-Object {
  '{0}  {1}  {2:N0} octets' -f $_.Name, (Get-FileHash $_ -Algorithm SHA256).Hash.ToLower(), $_.Length
}
