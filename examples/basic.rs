use otlp_embedded::{ui_app, Config, State, TraceServiceImpl, TraceServiceServer};

#[tokio::main]
async fn main() {
    let state = State::new(Config {
        max_length: 100,
        max_memory_usage: 1 << 27, // 128 MiB
    });
    let state_clone = state.clone();
    let state_clone_2 = state.clone();

    tokio::spawn(async {
        axum::Server::bind(&"0.0.0.0:10188".parse().unwrap())
            .serve(ui_app(state, "/").into_make_service())
            .await
            .unwrap();
    });

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            let state = state_clone_2.read().await;
            println!("Len: {}", state.len());
            println!("Estimated memory usage: {}", state.estimated_memory_usage());
        }
    });

    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(TraceServiceImpl::new(state_clone)))
        .serve("0.0.0.0:43177".parse().unwrap())
        .await
        .unwrap();
}
