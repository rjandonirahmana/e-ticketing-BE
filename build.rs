fn main() {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/auth.proto"], &["proto"])
        .expect("Failed to compile protos");

    // Cargo mendeteksi perubahan CSS → trigger rebuild shell() yang include_str!
    println!("cargo:rerun-if-changed=styles/");
}
