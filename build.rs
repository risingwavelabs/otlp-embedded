fn main() -> Result<(), Box<dyn std::error::Error>> {
    let derive_serde = "#[derive(serde::Serialize, serde::Deserialize)]";

    tonic_build::configure()
        .message_attribute(".", derive_serde)
        .enum_attribute(".", derive_serde)
        .compile(
            &["opentelemetry-proto/opentelemetry/proto/collector/trace/v1/trace_service.proto"],
            &["opentelemetry-proto/"],
        )?;

    Ok(())
}