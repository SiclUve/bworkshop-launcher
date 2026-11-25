# Construit les installateurs NSIS de test (1.0.0 et 1.1.0) à partir de NsisTool.nsi.
# Prérequis : Rust (rustc) et NSIS 3.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$makensis = @(
  "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
  "$env:ProgramFiles\NSIS\makensis.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $makensis) { throw 'makensis.exe introuvable : installe NSIS 3.' }

rustc -C opt-level=s -C strip=symbols -o testtool.exe ..\inno\testtool.rs
foreach ($v in '1.0.0', '1.1.0') {
  & $makensis /V2 "/DVERSION=$v" NsisTool.nsi
  if ($LASTEXITCODE -ne 0) { throw "makensis a échoué pour la version $v" }
}
Remove-Item testtool.pdb -ErrorAction SilentlyContinue
Get-ChildItem NsisTool_Setup_*.exe | ForEach-Object {
  '{0}  {1}  {2:N0} octets' -f $_.Name, (Get-FileHash $_ -Algorithm SHA256).Hash.ToLower(), $_.Length
}
