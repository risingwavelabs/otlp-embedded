use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_descriptor_set_path =
        PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("file_descriptor_set.bin");

    tonic_build::configure()
        .file_descriptor_set_path(&file_descriptor_set_path)
        .compile(
            &["opentelemetry-proto/opentelemetry/proto/collector/trace/v1/trace_service.proto"],
            &["opentelemetry-proto/"],
        )?;

    let descriptors = std::fs::read(file_descriptor_set_path)?;
    pbjson_build::Builder::new()
        .register_descriptors(&descriptors)?
        .build(&["."])?;

    Ok(())
}
