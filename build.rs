fn main() {
    // tonic_build::configure()
    //     .out_dir("/Users/LFordyc1/Workspace/Rust/tonic_broadcast/proto")
    //     .compile(&["proto/service.proto"], &["proto"])
    //     .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));

    tonic_build::compile_protos("proto/v1/service.proto")
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
