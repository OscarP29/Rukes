//! Incrusta en `rukes.exe` el icono y la información de versión (Explorador, instalador).

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../../assets/rukes.ico");
    println!("cargo:rerun-if-changed=build.rs");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let icon = manifest.join("../../assets/rukes.ico");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut nums: Vec<u16> = version
        .split(['.', '-', '+'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect();
    nums.resize(4, 0);
    let csv = nums
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let rc = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {csv}
PRODUCTVERSION {csv}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "OscarP29"
      VALUE "FileDescription", "Rukes"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "rukes"
      VALUE "LegalCopyright", "MIT OR Apache-2.0"
      VALUE "OriginalFilename", "rukes.exe"
      VALUE "ProductName", "Rukes"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = icon.display().to_string().replace('\\', "\\\\"),
    );
    let rc_path = out.join("rukes.rc");
    if let Err(e) = std::fs::write(&rc_path, rc) {
        println!("cargo:warning=no se pudo escribir rukes.rc: {e}");
        return;
    }
    if let Err(e) = embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=sin icono ni versión en el exe: {e}");
    }
}
