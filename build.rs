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

    // ── SELURUH CSS LAMA DIBUNGKUS `@layer legacy` ───────────────────────────
    // Tanpa pembungkus ini, CSS lama TAK BERLAYER sedangkan utility Tailwind
    // hidup di `@layer utilities` — dan dalam cascade CSS, deklarasi TAK
    // BERLAYER selalu mengalahkan deklarasi BERLAYER, berapa pun spesifisitasnya.
    // Bukan "spesifisitas lebih tinggi menang"; layer diperiksa LEBIH DULU.
    //
    // Akibatnya satu aturan di `01-base.css:8` melumpuhkan hampir seluruh
    // Tailwind di aplikasi ini:
    //
    //     *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    //
    // `padding: 0` di situ mengalahkan SETIAP `p-4`, `px-5`, `py-3.5`, `pb-36`;
    // `margin: 0` mengalahkan setiap `mx-5`, `m-5`, `mt-2`, `my-4`. Halaman yang
    // ditulis sepenuhnya dengan utility (mis. `pages/cart.rs`) karena itu tampil
    // rapat tanpa jarak sama sekali — kartu menempel ke tepi kolom, ringkasan
    // tanpa inset — padahal kelasnya SEMUA ada di /pkg/e-ticketing.css. Yang
    // hilang bukan CSS-nya, melainkan pertarungan cascade-nya.
    //
    // Urutannya dinyatakan di `style/tailwind.css`:
    //     @layer theme, base, components, legacy, utilities;
    // `legacy` sebelum `utilities` → utility menang atas CSS lama, yaitu
    // konvensi Tailwind yang normal dan prasyarat untuk migrasi per halaman.
    //
    // Dua hal yang SENGAJA tidak berubah karenanya:
    //   • 104 deklarasi `!important` di CSS lama tetap menang — untuk deklarasi
    //     penting, urutan layer BERLAKU TERBALIK, jadi `legacy` justru di atas
    //     `utilities`. Perilaku yang sudah ada tetap seperti semula.
    //   • Token di `:root` (00-tokens.css) tetap sah di dalam layer; custom
    //     property diwarisi dan di-`var()` seperti biasa.
    let mut bundle = String::from("@layer legacy {\n");
    for p in &parts {
        bundle.push_str(&fs::read_to_string(p).expect("read css part"));
        // Pemisah baris: sebagian berkas tak diakhiri newline, dan tanpa ini
        // aturan terakhir sebuah berkas menyatu dengan selector pertama berkas
        // berikutnya menjadi satu selector yang tak pernah cocok.
        bundle.push('\n');
    }
    bundle.push_str("\n}\n");
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
