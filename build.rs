fn main() -> Result<(), Box<dyn std::error::Error>> {
    // For now, we're not using protobuf/gRPC
    // When we add it, uncomment this:
    // tonic_build::compile_protos("proto/blixard.proto")?;
    Ok(())
}