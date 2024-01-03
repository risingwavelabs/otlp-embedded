# `otlp-embedded`

A simple in-memory implementation of the OpenTelemetry trace collector
with a Web UI for visualizing the traces that can be embedded into other
Rust applications.

## Example

```rust ignore
use otlp_embedded::{ui_app, State, TraceServiceImpl, TraceServiceServer};

#[tokio::main]
async fn main() {
    let state = State::new(Config {
        max_length: 100,
        max_memory_usage: 1 << 27, // 128 MiB
    });
    let state_clone = state.clone();

    tokio::spawn(async {
        axum::Server::bind(&"0.0.0.0:10188".parse().unwrap())
            .serve(ui_app(state, "/").into_make_service())
            .await
            .unwrap();
    });

    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(TraceServiceImpl::new(state_clone)))
        .serve("0.0.0.0:43177".parse().unwrap())
        .await
        .unwrap();
}
```
