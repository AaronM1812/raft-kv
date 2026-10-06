// Runs before `cargo build` compiles src/. Turns proto/raft.proto into Rust.
// Output lands in $OUT_DIR; pulled into the crate with `tonic::include_proto!("raft")`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Point tonic at the bundled protoc binary instead of a system one.
    unsafe {
        std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    }
    tonic_prost_build::compile_protos("proto/raft.proto")?;
    Ok(())
}
