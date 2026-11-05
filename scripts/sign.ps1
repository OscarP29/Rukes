<#
.SYNOPSIS
  Firma ejecutables con signtool y un certificado PFX (opcional en v1.0).

.DESCRIPTION
  Lee el certificado de SIGN_PFX (PFX en base64) o de SIGN_PFX_FILE, y la contraseña de
  SIGN_PASSWORD. Sin certificado no hace nada. Ver docs/release.md.

.EXAMPLE
  $env:SIGN_PFX_FILE = 'C:\certs\rukes.pfx'; $env:SIGN_PASSWORD = '...'
  ./scripts/sign.ps1 target/release/rukes.exe dist/*.exe
#>
param([Parameter(ValueFromRemainingArguments)][string[]]$Files)

$ErrorActionPreference = 'Stop'
$pfx = $env:SIGN_PFX_FILE
if (-not $pfx -and $env:SIGN_PFX) {
    $pfx = Join-Path $env:TEMP 'rukes-sign.pfx'
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:SIGN_PFX))
}
if (-not $pfx) { Write-Warning 'Sin certificado: no se firma nada.'; return }

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw 'No se encontró signtool.exe (Windows SDK).' }

foreach ($pattern in $Files) {
    foreach ($f in Get-ChildItem $pattern) {
        & $signtool.FullName sign /f $pfx /p $env:SIGN_PASSWORD /fd SHA256 /td SHA256 `
            /tr http://timestamp.digicert.com /d Rukes $f.FullName
        if ($LASTEXITCODE -ne 0) { throw "No se pudo firmar $f" }
    }
}
if ($env:SIGN_PFX) { Remove-Item $pfx -Force }
