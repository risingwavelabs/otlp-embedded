use otlp_tempo_dump::{run_jaeger_server, MyServer, State, TraceServiceServer};

#[tokio::main]
async fn main() {
    let state = State::new();

    tokio::spawn(run_jaeger_server(state.clone()));

    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(MyServer::new(state)))
        .serve("0.0.0.0:43177".parse().unwrap())
        .await
        .unwrap();
}
