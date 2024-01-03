#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod jaeger;
mod limiter;
/// The generated protobuf and gRPC code for OpenTelemetry trace service.
pub mod proto;
mod service;
mod state;
mod trace;

pub use jaeger::ui::app as ui_app;
pub use proto::collector::trace::v1::trace_service_server::{TraceService, TraceServiceServer};
pub use service::TraceServiceImpl;
pub use state::{Config, State, StateRef};
pub use trace::*;
