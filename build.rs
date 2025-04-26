fn main() {
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile_protos(&["protos/hold.proto"], &["protos"])
        .unwrap_or_else(|e| panic!("Could not build protos: {e}"));

    built::write_built_file()
        .unwrap_or_else(|e| panic!("Failed to acquire build-time information: {e}"));
}
