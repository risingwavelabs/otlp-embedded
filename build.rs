fn main() -> Result<(), Box<dyn std::error::Error>> {
    let derive_serde = "#[derive(serde::Serialize, serde::Deserialize, datasize::DataSize)]";

    tonic_build::configure()
        .message_attribute(".", derive_serde)
        .enum_attribute(".", derive_serde)
        .compile(
            &["proto/opentelemetry/proto/collector/trace/v1/trace_service.proto"],
            &["proto/"],
        )?;

    Ok(())
}
