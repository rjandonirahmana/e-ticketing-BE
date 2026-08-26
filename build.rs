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

    // ── Embed daftar migrasi ───────────────────────────────────────────────────
    // `migration/*.sql` di-embed ke binari sebagai (nama, isi), URUT NAMA FILE.
    // Di-embed, bukan dibaca dari disk saat runtime, supaya container yang hanya
    // memuat binari tetap bisa menjalankan migrasi — dan supaya berkas yang
    // dipakai persis yang ikut ter-build, bukan yang kebetulan ada di server.
    let mig_dir = Path::new("migration");
    let mut migs: Vec<_> = fs::read_dir(mig_dir)
        .expect("read migration/")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "sql").unwrap_or(false))
        .collect();
    migs.sort();

    let mut list = String::from(
        "/// (nama berkas, isi SQL) — urut nama, di-generate build.rs.\n\
         pub static MIGRATIONS: &[(&str, &str)] = &[\n",
    );
    for p in &migs {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let abs = fs::canonicalize(p).expect("canonicalize migration");
        list.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            name,
            abs.to_string_lossy()
        ));
    }
    list.push_str("];\n");

    let mig_out = Path::new(&env::var("OUT_DIR").unwrap()).join("migrations.rs");
    fs::write(&mig_out, list).expect("write migrations.rs");

    // Rebuild bila ada perubahan CSS (part) atau folder styles lain.
    println!("cargo:rerun-if-changed=styles/");
    println!("cargo:rerun-if-changed=styles/parts");
    println!("cargo:rerun-if-changed=migration");
}
