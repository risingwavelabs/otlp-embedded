//! Basic end-to-end example.
//!
//! This example starts:
//! - Jaeger-compatible UI on `http://localhost:10188/`
//! - OTLP gRPC trace ingestion service on `0.0.0.0:43177`
//!
//! Use this when you want to send real spans from an external client and
//! visualize them in the embedded UI.

use otlp_embedded::{Config, State, TraceServiceImpl, TraceServiceServer, ui_app};

#[tokio::main]
async fn main() {
    let state = State::new(Config {
        max_length: 100,
        max_memory_usage: 1 << 27, // 128 MiB
    });
    let state_clone = state.clone();
    let state_clone_2 = state.clone();

    tokio::spawn(async {
        axum::serve(
            tokio::net::TcpListener::bind("0.0.0.0:10188")
                .await
                .unwrap(),
            ui_app(state, "/"),
        )
        .await
        .unwrap();
    });

    println!("Open http://localhost:10188/ to view the UI.");
    println!("Send traces to http://localhost:43177/v1/trace.");

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut last_len = 0;
        let mut last_mem = 0;

        loop {
            interval.tick().await;
            let state = state_clone_2.read().await;
            let len = state.len();
            let mem = state.estimated_memory_usage();

            if len != last_len || mem != last_mem {
                println!("Len: {}", len);
                println!("Estimated memory usage: {}", mem);
            }
            last_len = len;
            last_mem = mem;
        }
    });

    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(TraceServiceImpl::new(state_clone)))
        .serve("0.0.0.0:43177".parse().unwrap())
        .await
        .unwrap();
}
