use std::path::Path;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let svg_path = "resources/icons/awara.svg";

    // --- Generate window-icon data (all platforms) ---
    generate_icon_rs(svg_path, &out_dir);

    // --- Windows: embed pre-generated .ico into the .exe ---
    #[cfg(target_os = "windows")]
    embed_windows_icon(&out_dir);

    // --- macOS: stage pre-generated .icns and Info.plist ---
    #[cfg(target_os = "macos")]
    stage_macos_resources(&out_dir);
}

/// Convert premultiplied RGBA to straight alpha.
/// Window protocols (x11's _NET_WM_ICON, Wayland, etc.) expect straight alpha.
fn premul_to_straight(pixels: &[u8]) -> Vec<u8> {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|c| {
            let a = c[3] as f32 / 255.0;
            if a == 0.0 {
                [c[0], c[1], c[2], c[3]]
            } else {
                [
                    (c[0] as f32 / a).round().min(255.0) as u8,
                    (c[1] as f32 / a).round().min(255.0) as u8,
                    (c[2] as f32 / a).round().min(255.0) as u8,
                    c[3],
                ]
            }
        })
        .collect()
}

fn generate_icon_rs(svg_path: &str, out_dir: &str) {
    let svg_data =
        std::fs::read(svg_path).expect("awara.svg not found at resources/icons/awara.svg");

    let tree = usvg::Tree::from_data(&svg_data, &usvg::Options::default())
        .expect("Failed to parse awara.svg");

    // Render at 256x256 (max icon size, downscaled by the OS as needed)
    let render_size = 256u32;
    let mut pixmap =
        tiny_skia::Pixmap::new(render_size, render_size).expect("Failed to create pixmap");

    // Scale to fill the render target
    let svg_max = tree.size().width().max(tree.size().height());
    let scale = render_size as f32 / svg_max;
    let transform = tiny_skia::Transform::from_scale(scale, scale);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let pixels = premul_to_straight(pixmap.data()); // straight RGBA

    // Write an expression producing egui::IconData (used via include! in mod.rs)
    let icon_rs = format!(
        r#"egui::IconData {{
    rgba: {pixels:?}.to_vec(),
    width: {width},
    height: {height},
}}
"#,
        width = render_size,
        height = render_size,
        pixels = pixels,
    );

    std::fs::write(Path::new(out_dir).join("awara_icon.rs"), &icon_rs)
        .expect("Failed to write awara_icon.rs");
}

#[cfg(target_os = "windows")]
fn embed_windows_icon(out_dir: &str) {
    // Use the pre-generated .ico from the repo
    let ico_src = Path::new("resources/windows/awara.ico");
    println!("cargo:rerun-if-changed={}", ico_src.display());
    let ico_dst = Path::new(out_dir).join("awara.ico");
    std::fs::copy(ico_src, &ico_dst).expect("Failed to copy awara.ico to OUT_DIR");

    // Write .rc resource file alongside the .ico: exe icon + VERSIONINFO
    // (shown in Explorer → Properties → Details). The version comes from
    // Cargo.toml at build-script compile time, so the exe can never drift
    // from the manifest.
    let rc_path = Path::new(out_dir).join("awara.rc");
    std::fs::write(&rc_path, windows_rc_source()).expect("Failed to write awara.rc");

    // Compile & embed the .res into the .exe
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .expect("Failed to embed Windows icon resource");
}

/// The .rc source: exe icon + VERSIONINFO.
#[cfg(target_os = "windows")]
fn windows_rc_source() -> String {
    let (major, minor, patch, build) = windows_version_parts(env!("CARGO_PKG_VERSION"));

    format!(
        r#"MAINICON ICON "awara.ico"

1 VERSIONINFO
 FILEVERSION {major},{minor},{patch},{build}
 PRODUCTVERSION {major},{minor},{patch},{build}
 FILEFLAGSMASK 0x3fL
 FILEFLAGS 0x0L
 FILEOS 0x40004L
 FILETYPE 0x1L
 FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Matthew Deik"
            VALUE "FileDescription", "Batch file renamer with GUI, TUI, and CLI"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "awara"
            VALUE "LegalCopyright", "Copyright (C) Matthew Deik"
            VALUE "OriginalFilename", "awara.exe"
            VALUE "ProductName", "Awara"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#,
        version = env!("CARGO_PKG_VERSION"),
    )
}

/// Split "0.2.1[-suffix][+meta]" into up to four u16 segments (missing ones
/// default to 0), matching VERSIONINFO's fixed 4-field layout.
#[cfg(target_os = "windows")]
fn windows_version_parts(version: &str) -> (u16, u16, u16, u16) {
    let core = version
        .split('+')
        .next()
        .unwrap_or(version)
        .split('-')
        .next()
        .unwrap_or(version);
    let mut segments = core.split('.');
    let major = segments.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = segments.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = segments.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let build = segments.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch, build)
}

#[cfg(target_os = "macos")]
fn stage_macos_resources(out_dir: &str) {
    // Copy pre-generated .icns
    let icns_src = Path::new("resources/macos/awara.icns");
    let icns_dst = Path::new(out_dir).join("awara.icns");
    std::fs::copy(icns_src, &icns_dst).expect("Failed to copy awara.icns to OUT_DIR");

    // Copy Info.plist template alongside for easy bundling
    let plist_src = "resources/macos/Info.plist";
    if std::fs::metadata(plist_src).is_ok() {
        let plist_dst = Path::new(out_dir).join("awara-Info.plist");
        std::fs::copy(plist_src, &plist_dst).expect("Failed to copy Info.plist");
    }
}
