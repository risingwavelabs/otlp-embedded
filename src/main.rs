use otlp_embedded::{jaeger_ui_app, State, TraceServiceImpl, TraceServiceServer};

#[tokio::main]
async fn main() {
    let state = State::new();
    let state_clone = state.clone();

    tokio::spawn(async {
        axum::Server::bind(&"0.0.0.0:10188".parse().unwrap())
            .serve(jaeger_ui_app(state, "/").into_make_service())
            .await
            .unwrap();
    });

    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(TraceServiceImpl::new(state_clone)))
        .serve("0.0.0.0:43177".parse().unwrap())
        .await
        .unwrap();
}
