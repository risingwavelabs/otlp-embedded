fn main() -> Result<(), Box<dyn std::error::Error>> {
    let derive = "#[derive(serde::Serialize, serde::Deserialize, get_size2::GetSize)]";

    tonic_prost_build::configure()
        .build_transport(false)
        .build_client(false)
        .message_attribute(".", derive)
        .enum_attribute(".", derive)
        .compile_protos(
            &["proto/opentelemetry/proto/collector/trace/v1/trace_service.proto"],
            &["proto/"],
        )?;

    Ok(())
}
