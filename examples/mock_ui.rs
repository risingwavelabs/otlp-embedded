//! Mock-data-only UI example.
//!
//! This example seeds several traces directly into in-memory state and starts
//! the Jaeger-compatible UI on `http://localhost:10188/`.
//!
//! No OTLP gRPC service is started, so this is useful for quick UI validation
//! without an external trace producer.

use std::time::{SystemTime, UNIX_EPOCH};

use otlp_embedded::proto::{
    collector::trace::v1::ExportTraceServiceRequest,
    common::v1::{InstrumentationScope, KeyValue, any_value},
    resource::v1::Resource,
    trace::v1::{ResourceSpans, ScopeSpans, Span, Status, span, status},
};
use otlp_embedded::{Config, State, StateRef, TraceService, TraceServiceImpl, ui_app};
use tonic::Request;

#[tokio::main]
async fn main() {
    let state = State::new(Config {
        max_length: 100,
        max_memory_usage: 1 << 27, // 128 MiB
    });
    seed_mock_traces(state.clone()).await;

    {
        let state_guard = state.read().await;
        let services: Vec<_> = state_guard.get_all_services().into_iter().collect();
        println!("Seeded {} traces.", state_guard.len());
        println!("Services in mock data: {:?}", services);
    }

    println!("Open http://localhost:10188/ to view the UI.");
    println!("This example only serves mock data and does not start OTLP gRPC.");

    axum::serve(
        tokio::net::TcpListener::bind("0.0.0.0:10188")
            .await
            .unwrap(),
        ui_app(state, "/"),
    )
    .await
    .unwrap();
}

async fn seed_mock_traces(state: StateRef) {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let ago = |ms: u64| now_nanos.saturating_sub(ms * 1_000_000);
    let event = |name: &str, ms_ago: u64| span::Event {
        time_unix_nano: ago(ms_ago),
        name: name.to_string(),
        attributes: vec![],
        dropped_attributes_count: 0,
    };

    let frontend_rs = make_resource_spans(
        "frontend",
        "frontend-01",
        vec![
            make_span(
                "11111111111111111111111111111111",
                "a1a1a1a1a1a1a1a1",
                None,
                "GET /checkout",
                span::SpanKind::Server as i32,
                ago(28_000),
                ago(27_200),
                vec![
                    kv_str("http.method", "GET"),
                    kv_str("http.target", "/checkout"),
                    kv_int("http.status_code", 200),
                ],
                vec![event("request.received", 27_980)],
                None,
            ),
            make_span(
                "55555555555555555555555555555555",
                "e5e5e5e5e5e5e5e5",
                None,
                "GET /cart",
                span::SpanKind::Server as i32,
                ago(6_000),
                ago(5_500),
                vec![
                    kv_str("http.method", "GET"),
                    kv_str("http.target", "/cart"),
                    kv_int("http.status_code", 200),
                ],
                vec![event("cache.hit", 5_800)],
                None,
            ),
        ],
    );

    let checkout_rs = make_resource_spans(
        "checkout",
        "checkout-01",
        vec![
            make_span(
                "11111111111111111111111111111111",
                "b2b2b2b2b2b2b2b2",
                Some("a1a1a1a1a1a1a1a1"),
                "POST /checkout",
                span::SpanKind::Server as i32,
                ago(27_850),
                ago(27_260),
                vec![
                    kv_str("http.method", "POST"),
                    kv_str("http.target", "/checkout"),
                    kv_int("http.status_code", 200),
                ],
                vec![event("payment.requested", 27_600)],
                None,
            ),
            make_span(
                "22222222222222222222222222222222",
                "b1b1b1b1b1b1b1b1",
                None,
                "GET /health",
                span::SpanKind::Server as i32,
                ago(20_000),
                ago(19_900),
                vec![
                    kv_str("http.method", "GET"),
                    kv_str("http.target", "/health"),
                    kv_int("http.status_code", 200),
                ],
                vec![],
                None,
            ),
            make_span(
                "66666666666666666666666666666666",
                "f6f6f6f6f6f6f6f6",
                None,
                "POST /checkout",
                span::SpanKind::Server as i32,
                ago(3_000),
                ago(2_200),
                vec![
                    kv_str("http.method", "POST"),
                    kv_str("http.target", "/checkout"),
                    kv_int("http.status_code", 200),
                ],
                vec![event("validation.done", 2_600)],
                None,
            ),
        ],
    );

    let payment_rs = make_resource_spans(
        "payment",
        "payment-01",
        vec![
            make_span(
                "11111111111111111111111111111111",
                "c3c3c3c3c3c3c3c3",
                Some("b2b2b2b2b2b2b2b2"),
                "POST /charge",
                span::SpanKind::Server as i32,
                ago(27_700),
                ago(27_400),
                vec![
                    kv_str("http.method", "POST"),
                    kv_str("http.target", "/charge"),
                    kv_int("http.status_code", 200),
                ],
                vec![event("gateway.authorized", 27_520)],
                None,
            ),
            make_span(
                "33333333333333333333333333333333",
                "c1c1c1c1c1c1c1c1",
                None,
                "POST /charge",
                span::SpanKind::Server as i32,
                ago(14_000),
                ago(13_600),
                vec![
                    kv_str("http.method", "POST"),
                    kv_str("http.target", "/charge"),
                    kv_int("http.status_code", 500),
                ],
                vec![event("gateway.timeout", 13_800)],
                Some(Status {
                    message: "upstream timeout".to_string(),
                    code: status::StatusCode::Error as i32,
                }),
            ),
        ],
    );

    let inventory_rs = make_resource_spans(
        "inventory",
        "inventory-01",
        vec![
            make_span(
                "11111111111111111111111111111111",
                "d4d4d4d4d4d4d4d4",
                Some("b2b2b2b2b2b2b2b2"),
                "POST /reserve",
                span::SpanKind::Server as i32,
                ago(27_690),
                ago(27_500),
                vec![
                    kv_str("http.method", "POST"),
                    kv_str("http.target", "/reserve"),
                    kv_int("http.status_code", 200),
                ],
                vec![event("redis.write", 27_560)],
                None,
            ),
            make_span(
                "44444444444444444444444444444444",
                "d1d1d1d1d1d1d1d1",
                None,
                "GET /stock/{sku}",
                span::SpanKind::Server as i32,
                ago(10_000),
                ago(9_500),
                vec![
                    kv_str("http.method", "GET"),
                    kv_str("http.target", "/stock/sku-123"),
                    kv_int("http.status_code", 200),
                ],
                vec![event("db.read", 9_700)],
                None,
            ),
        ],
    );

    let req = ExportTraceServiceRequest {
        resource_spans: vec![frontend_rs, checkout_rs, payment_rs, inventory_rs],
    };

    let service = TraceServiceImpl::new(state);
    TraceService::export(&service, Request::new(req))
        .await
        .expect("failed to seed mock traces");
}

fn make_resource_spans(service_name: &str, instance_id: &str, spans: Vec<Span>) -> ResourceSpans {
    ResourceSpans {
        resource: Some(Resource {
            attributes: vec![
                kv_str("service.name", service_name),
                kv_str("service.instance.id", instance_id),
                kv_str("deployment.environment", "dev"),
            ],
            dropped_attributes_count: 0,
        }),
        scope_spans: vec![ScopeSpans {
            scope: Some(InstrumentationScope {
                name: "examples/mock_ui".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                attributes: vec![],
                dropped_attributes_count: 0,
            }),
            spans,
            schema_url: String::new(),
        }],
        schema_url: String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn make_span(
    trace_id_hex: &str,
    span_id_hex: &str,
    parent_span_id_hex: Option<&str>,
    name: &str,
    kind: i32,
    start_time_unix_nano: u64,
    end_time_unix_nano: u64,
    attributes: Vec<KeyValue>,
    events: Vec<span::Event>,
    status: Option<Status>,
) -> Span {
    Span {
        trace_id: hex::decode(trace_id_hex).expect("invalid trace id hex"),
        span_id: hex::decode(span_id_hex).expect("invalid span id hex"),
        trace_state: String::new(),
        parent_span_id: parent_span_id_hex
            .map(|id| hex::decode(id).expect("invalid parent span id hex"))
            .unwrap_or_default(),
        name: name.to_string(),
        kind,
        start_time_unix_nano,
        end_time_unix_nano,
        attributes,
        dropped_attributes_count: 0,
        events,
        dropped_events_count: 0,
        links: vec![],
        dropped_links_count: 0,
        status,
    }
}

fn kv_str(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(any_value_string(value)),
    }
}

fn kv_int(key: &str, value: i64) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(otlp_embedded::proto::common::v1::AnyValue {
            value: Some(any_value::Value::IntValue(value)),
        }),
    }
}

fn any_value_string(value: &str) -> otlp_embedded::proto::common::v1::AnyValue {
    otlp_embedded::proto::common::v1::AnyValue {
        value: Some(any_value::Value::StringValue(value.to_string())),
    }
}
