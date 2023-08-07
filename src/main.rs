use otlp_tempo_dump::{MyServer, TraceServiceServer};

#[tokio::main]
async fn main() {
    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(MyServer::new()))
        .serve("0.0.0.0:4317".parse().unwrap())
        .await
        .unwrap();
}
