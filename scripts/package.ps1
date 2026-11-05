<#
.SYNOPSIS
  Compila Rukes en release y genera en dist/ el ZIP portable, el instalador (si hay Inno Setup)
  y los manifiestos de winget con los SHA-256 (RNF-032). Si hay certificado (SIGN_PFX o
  SIGN_PFX_FILE, ver scripts/sign.ps1) firma el exe y el instalador antes de calcular los hashes.

.EXAMPLE
  ./scripts/package.ps1
  ./scripts/package.ps1 -SkipBuild
#>
param(
    [switch]$SkipBuild,
    # Carpeta de compilación (cargo --target-dir).
    [string]$TargetDir = 'target',
    # URL base de descarga de la release (para el manifiesto de winget). Cambia OWNER por tu usuario.
    [string]$BaseUrl = "https://github.com/OWNER/rukes/releases/download"
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
Write-Host "Rukes $version"

if (-not $SkipBuild) {
    cargo build --release -p rukes-app --target-dir $TargetDir
    if ($LASTEXITCODE -ne 0) { throw "cargo build falló" }
}
$exe = Join-Path (Resolve-Path $TargetDir) 'release/rukes.exe'
if (-not (Test-Path $exe)) { throw "No existe $exe" }
& (Join-Path $PSScriptRoot 'sign.ps1') $exe

$dist = Join-Path $root 'dist'
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory $dist | Out-Null

# --- ZIP portable: con portable.txt todo se guarda en la carpeta data junto al exe (RF-204).
$stage = Join-Path $dist "Rukes-$version-portable"
New-Item -ItemType Directory $stage | Out-Null
Copy-Item $exe $stage
Copy-Item README.md, CHANGELOG.md, LICENSE-MIT, LICENSE-APACHE $stage
Copy-Item docs/usuario.md (Join-Path $stage 'GUIA.md')
Set-Content -Path (Join-Path $stage 'portable.txt') -Encoding utf8 -Value @(
    'Este archivo activa el modo portable: la configuración, la caché y los logs se guardan',
    'en la carpeta "data" junto a rukes.exe. Bórralo para usar %APPDATA%\Rukes.'
)
$zip = Join-Path $dist "Rukes-$version-portable-x64.zip"
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
Remove-Item -Recurse -Force $stage
Write-Host "ZIP: $zip"

# --- Instalador (Inno Setup 6).
$iscc = @(
    (Get-Command iscc.exe -ErrorAction SilentlyContinue)?.Source,
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
$setup = $null
if ($iscc) {
    & $iscc "/DAppVersion=$version" "/DExePath=$exe" "/O$dist" packaging/rukes.iss | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup falló" }
    $setup = Join-Path $dist "Rukes-$version-setup-x64.exe"
    & (Join-Path $PSScriptRoot 'sign.ps1') $setup
    Write-Host "Instalador: $setup"
} else {
    Write-Warning "Inno Setup 6 no está instalado: se omite el instalador (winget install JRSoftware.InnoSetup)."
}

# --- SHA-256 y manifiestos de winget.
$sums = Get-ChildItem $dist -File | ForEach-Object {
    "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
}
$sums | Set-Content (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii
$sums | Write-Host

if ($setup) {
    $hash = (Get-FileHash $setup -Algorithm SHA256).Hash
    $out = Join-Path $dist "winget/$version"
    New-Item -ItemType Directory $out -Force | Out-Null
    foreach ($f in Get-ChildItem packaging/winget -Filter *.yaml) {
        (Get-Content $f.FullName -Raw).
            Replace('{{VERSION}}', $version).
            Replace('{{SHA256}}', $hash).
            Replace('{{URL}}', "$BaseUrl/v$version/Rukes-$version-setup-x64.exe") |
            Set-Content (Join-Path $out $f.Name) -Encoding utf8
    }
    Write-Host "Manifiestos de winget: $out"
}
