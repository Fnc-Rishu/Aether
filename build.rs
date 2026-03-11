fn main() {
    prost_build::compile_protos(
        &["proto/checkin.proto", "proto/mcs.proto"],
        &["proto"],
    )
    .expect("Failed to compile protobuf definitions");
}