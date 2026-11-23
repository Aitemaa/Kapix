//! Windows resources for `kapix.exe`: the app icon and an application manifest (PerMonitorV2
//! DPI awareness, Common-Controls v6, `asInvoker`, Windows 10/11).
//!
//! * Windows host (CI, releases): one resource script via `winresource` (icon + manifest).
//! * Non-Windows host targeting Windows (`cargo xwin` on Linux): no resource compiler is
//!   available, so only the manifest is embedded, pure-Rust, via `embed-manifest`.

const ICON: &str = "../../assets/icons/icon.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed();
    }
}

#[cfg(windows)]
fn embed() {
    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="Kapix" version="1.0.0.0"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2,PerMonitor</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#;
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ICON).set_manifest(MANIFEST);
    res.compile().expect("unable to compile Windows resources");
}

#[cfg(not(windows))]
fn embed() {
    use embed_manifest::manifest::DpiAwareness;
    embed_manifest::embed_manifest(
        embed_manifest::new_manifest("Kapix").dpi_awareness(DpiAwareness::PerMonitorV2),
    )
    .expect("unable to embed manifest");
}
