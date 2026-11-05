# Publicar una versión

## 1. Preparar

1. Sube `version` en `[workspace.package]` del `Cargo.toml` raíz (el exe, el instalador y los
   manifiestos la toman de ahí).
2. En `CHANGELOG.md` cambia `## [Unreleased]` por `## [x.y.z] - AAAA-MM-DD`.
3. Comprueba que CI pasa: `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

## 2. Empaquetar en local

Requisitos: Rust MSVC y, para el instalador, Inno Setup 6
(`winget install JRSoftware.InnoSetup`). Sin Inno Setup solo se genera el ZIP.

```powershell
./scripts/package.ps1 -BaseUrl https://github.com/<usuario>/rukes/releases/download
```

- `-SkipBuild` usa un exe ya compilado; `-TargetDir` otra carpeta de compilación (útil si
  Rukes está abierto y bloquea `target\release\rukes.exe`).
- Resultado en `dist/`: `Rukes-x.y.z-portable-x64.zip`, `Rukes-x.y.z-setup-x64.exe`,
  `SHA256SUMS.txt` y `winget/x.y.z/*.yaml`.

El exe se compila con el CRT estático (`.cargo/config.toml`), así que no necesita el
redistribuible de Visual C++.

## 3. Firmar (opcional)

Con un certificado de firma de código en PFX:

```powershell
$env:SIGN_PFX_FILE = 'C:\ruta\cert.pfx'; $env:SIGN_PASSWORD = '...'
./scripts/package.ps1      # firma el exe y el instalador antes de calcular los SHA-256
```

`scripts/sign.ps1 <archivos>` también firma archivos sueltos.

Sin certificado los archivos se publican sin firmar: SmartScreen mostrará el aviso de
«editor desconocido» hasta que el binario gane reputación. Opciones: un certificado OV/EV de
una CA, Azure Trusted Signing o SignPath (gratuito para proyectos de código abierto).

## 4. Publicar en GitHub

Al subir una etiqueta `vX.Y.Z` el workflow `.github/workflows/release.yml` compila, empaqueta,
firma si existen los secretos `SIGN_PFX` (PFX en base64) y `SIGN_PASSWORD`, crea la release
con los archivos de `dist/` y deja los manifiestos de winget (ya con la URL del repositorio)
como artefacto `winget-manifests` de la ejecución.

```bash
git tag v1.0.0
git push origin v1.0.0
```

## 5. winget

1. Descarga el artefacto `winget-manifests` de la ejecución del workflow (o genéralos en
   local con `-BaseUrl` y la URL real).
2. Valídalos: `winget validate dist\winget\x.y.z` y pruébalos:
   `winget install --manifest dist\winget\x.y.z`.
3. Haz un PR a [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) en
   `manifests/h/HenryOsorio/Rukes/x.y.z/`, o usa `wingetcreate submit dist\winget\x.y.z`.

Las plantillas están en `packaging/winget`; el identificador es `HenryOsorio.Rukes`.
