use std::{env, fs, path::Path};

fn main() {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/auth.proto"], &["proto"])
        .expect("Failed to compile protos");

    // ── Generate CSS bundle ────────────────────────────────────────────────────
    // Gabungkan semua `styles/parts/*.css` (URUT NAMA FILE → prefix `NN-` menjaga
    // urutan cascade) menjadi satu `OUT_DIR/app.bundle.css`. File itu di-embed
    // via include_str! di `src/web/assets.rs` dan disajikan di `/styles/app.css`.
    // Sumber tunggal = styles/parts/ → tak perlu sync manual bundle lagi.
    let parts_dir = Path::new("styles/parts");
    let mut parts: Vec<_> = fs::read_dir(parts_dir)
        .expect("read styles/parts")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "css").unwrap_or(false))
        .collect();
    parts.sort();

    let mut bundle = String::new();
    for p in &parts {
        bundle.push_str(&fs::read_to_string(p).expect("read css part"));
    }
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("app.bundle.css");
    fs::write(&out, bundle).expect("write app.bundle.css");

    // Rebuild bila ada perubahan CSS (part) atau folder styles lain.
    println!("cargo:rerun-if-changed=styles/");
    println!("cargo:rerun-if-changed=styles/parts");
}
