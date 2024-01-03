#![warn(missing_docs)]

/*!
A simple in-memory implementation of the OpenTelemetry trace collector
with a Web UI for visualizing the traces that can be embedded into other
Rust applications.

# Example

```ignore
use otlp_embedded::{ui_app, State, TraceServiceImpl, TraceServiceServer};

#[tokio::main]
async fn main() {
    let state = State::new(100);
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
*/

mod jaeger;
mod proto;
mod service;
mod state;
mod trace;

pub use jaeger::ui::app as ui_app;
pub use proto::collector::trace::v1::trace_service_server::TraceServiceServer;
pub use service::TraceServiceImpl;
pub use state::{State, StateRef};
pub use trace::*;
