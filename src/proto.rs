#![allow(non_snake_case)]
#![allow(clippy::all)]

mod opentelemetry {
    pub mod proto {
        pub mod common {
            pub mod v1 {
                tonic::include_proto!("opentelemetry.proto.common.v1");
                tonic::include_proto!("opentelemetry.proto.common.v1.serde");
            }
        }
        pub mod resource {
            pub mod v1 {
                tonic::include_proto!("opentelemetry.proto.resource.v1");
                tonic::include_proto!("opentelemetry.proto.resource.v1.serde");
            }
        }
        pub mod trace {
            pub mod v1 {
                tonic::include_proto!("opentelemetry.proto.trace.v1");
                tonic::include_proto!("opentelemetry.proto.trace.v1.serde");
            }
        }
        pub mod collector {
            pub mod trace {
                pub mod v1 {
                    tonic::include_proto!("opentelemetry.proto.collector.trace.v1");
                    tonic::include_proto!("opentelemetry.proto.collector.trace.v1.serde");
                }
            }
        }
    }
}

pub use self::opentelemetry::proto::*;
