fn main() {
    println!("cargo:rerun-if-env-changed=CLASSROOM_BUILD_LABEL");
    println!("cargo:rerun-if-changed=src");
    let build = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let label: String = std::env::var("CLASSROOM_BUILD_LABEL")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(80)
        .collect();
    let id = if label.is_empty() {
        build.to_string()
    } else {
        format!("{build}-{label}")
    };
    println!("cargo:rustc-env=CLASSROOM_BUILD_ID={id}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_manifest();
    }
}

fn windows_manifest() {
    // Explicit normal-user execution prevents Windows installer heuristics
    // from requesting elevation merely because a copied helper has a new name.
    // This does not grant privileges or change the user's UAC policy.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTUAC:level='asInvoker' uiAccess='false'");
        #[cfg(windows)]
        {
            let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
            let resources = output.join("Classroom-version.rc");
            std::fs::write(&resources, version_resource()).expect("write version resource");
            embed_resource::compile(&resources, embed_resource::NONE)
                .manifest_optional()
                .expect("embed version resource");
        }
        return;
    }
    use std::{path::PathBuf, process::Command};
    println!("cargo:rerun-if-env-changed=CLASSROOM_WINDRES");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR missing"));
    let manifest = output.join("Classroom.manifest");
    std::fs::write(
        &manifest,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges>
<requestedExecutionLevel level="asInvoker" uiAccess="false"/>
</requestedPrivileges></security></trustInfo></assembly>"#,
    )
    .expect("write manifest");
    let resources = output.join("Classroom.rc");
    std::fs::write(
        &resources,
        format!(
            "1 24 \"{}\"\n{}",
            manifest.display().to_string().replace('\\', "/"),
            version_resource()
        ),
    )
    .expect("write resources");
    let object = output.join("Classroom-resources.o");
    let tool = std::env::var("CLASSROOM_WINDRES").unwrap_or_else(|_| {
        if cfg!(windows) {
            "windres".into()
        } else {
            "x86_64-w64-mingw32-windres".into()
        }
    });
    let status = Command::new(&tool)
        .arg("-i")
        .arg(&resources)
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(&object)
        .status()
        .expect("Windows GNU build needs windres (override CLASSROOM_WINDRES)");
    assert!(
        status.success(),
        "windres failed to embed asInvoker manifest"
    );
    println!("cargo:rustc-link-arg={}", object.display());
}

/// Version information shown in file properties and read by security tools:
/// an executable with no publisher/description metadata scores worse with
/// reputation heuristics. The name is the product's, not a claimed publisher.
fn version_resource() -> String {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts = version
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|p| p.parse::<u16>().ok());
    let numbers = [(); 4].map(|()| parts.next().unwrap_or(0));
    let comma = format!(
        "{},{},{},{}",
        numbers[0], numbers[1], numbers[2], numbers[3]
    );
    format!(
        r#"1 VERSIONINFO
FILEVERSION {comma}
PRODUCTVERSION {comma}
FILEOS 0x4
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Classroom"
      VALUE "FileDescription", "Classroom - LAN classroom screen sharing and management"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "Classroom"
      VALUE "ProductName", "Classroom"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
    )
}
