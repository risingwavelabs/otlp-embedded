mod jaeger;
mod proto;

use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use itertools::Itertools;
use proto::{
    collector::trace::v1::{trace_service_server::TraceService, *},
    resource::v1::Resource,
    trace::v1::{ResourceSpans, Span},
};
use schnellru::{ByLength, LruMap};
use serde_json::json;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

type TraceId = Vec<u8>;
type SpanId = Vec<u8>;

#[derive(Debug, Clone)]
pub struct Value {
    span: Span,
    resource: Arc<Resource>,
}

#[derive(Debug, Clone)]
pub enum ValueNode {
    Placeholder,
    Value(Value),
}

#[derive(Default, Debug, Clone)]
pub struct Trace {
    spans: HashMap<SpanId, ValueNode>,
}

pub use jaeger::server::run as run_jaeger_server;
pub use proto::collector::trace::v1::trace_service_server::TraceServiceServer;

use crate::jaeger::model::{span_to_jaeger_json, JaegerProcess};

impl Trace {
    fn add_value(&mut self, mut value: Value) {
        let span_id = &value.span.span_id;
        let parent_id = &value.span.parent_span_id;

        if span_id.is_empty() {
            return;
        }

        // If there's a parent and not recorded yet, add a placeholder.
        if !parent_id.is_empty() {
            self.spans
                .entry(parent_id.clone())
                .or_insert(ValueNode::Placeholder);
        }

        // Add a `message` attribute from the event name. Otherwise, it won't be displayed in Tempo.
        for event in &mut value.span.events {
            const MESSAGE: &str = "message";

            event.attributes.push(proto::common::v1::KeyValue {
                key: MESSAGE.to_string(),
                value: Some(proto::common::v1::AnyValue {
                    value: Some(proto::common::v1::any_value::Value::StringValue(
                        event.name.clone(),
                    )),
                }),
            });
        }

        match self.spans.entry(span_id.clone()) {
            Entry::Occupied(o) => {
                let o = o.into_mut();
                match o {
                    ValueNode::Placeholder => *o = ValueNode::Value(value),
                    ValueNode::Value(o) => {
                        // Update the span with the new value.
                        o.span.attributes.extend(value.span.attributes);
                        o.span.events.extend(value.span.events);
                        o.span.start_time_unix_nano =
                            (o.span.start_time_unix_nano).min(value.span.start_time_unix_nano);
                        o.span.end_time_unix_nano =
                            (o.span.end_time_unix_nano).max(value.span.end_time_unix_nano);
                    }
                }
            }
            Entry::Vacant(v) => {
                v.insert(ValueNode::Value(value));
            }
        }
    }

    fn iter_valid(&self) -> impl Iterator<Item = &Value> {
        self.spans.values().filter_map(|node| match node {
            ValueNode::Placeholder => None,
            ValueNode::Value(value) => Some(value),
        })
    }

    pub fn is_complete(&self) -> bool {
        // Since all new non-root values recorded will add a placeholder for the parent.
        // If there's no placeholder, it means the trace is complete.
        self.spans
            .values()
            .all(|v| matches!(v, ValueNode::Value(_)))
    }

    pub fn hex_id(&self) -> String {
        let bytes = &self.iter_valid().next().unwrap().span.trace_id;
        hex::encode(bytes)
    }

    pub fn to_tempo(&self) -> serde_json::Value {
        let entries = self
            .iter_valid()
            .map(|v| {
                json!({
                    "resource": &*v.resource,
                    "instrumentationLibrarySpans": [{
                        "spans": [v.span]
                    }]
                })
            })
            .collect_vec();

        json!({
            "batches": entries
        })
    }

    pub fn to_jaeger(&self) -> serde_json::Value {
        let mut processes = HashMap::new();

        let entries = self
            .iter_valid()
            .map(|v| {
                let process = JaegerProcess::from((*v.resource).clone());
                let key = process.key.clone();
                processes.insert(key.clone(), process);

                span_to_jaeger_json(v.span.clone(), key)
            })
            .collect_vec();

        if entries.is_empty() {
            return json!({});
        }

        let trace_id = &entries[0]["traceID"];

        json!({
            "data": [{
                "traceID": trace_id,
                "spans": entries,
                "processes": processes,
            }]
        })
    }
}

pub struct State {
    traces: LruMap<TraceId, Trace>,
}

pub type StateRef = Arc<RwLock<State>>;

impl State {
    pub fn new() -> StateRef {
        let this = Self {
            traces: LruMap::new(ByLength::new(100)),
        };

        Arc::new(RwLock::new(this))
    }

    fn add_value(&mut self, value: Value) {
        self.traces
            .get_or_insert(value.span.trace_id.clone(), Default::default)
            .unwrap()
            .add_value(value);
    }

    fn apply(&mut self, resource_spans: ResourceSpans) {
        let ResourceSpans {
            resource,
            scope_spans,
            schema_url: _,
        } = resource_spans;

        let resource = Arc::new(resource.unwrap_or_default());

        for span in scope_spans.into_iter().flat_map(|s| s.spans) {
            let value = Value {
                span,
                resource: resource.clone(),
            };
            self.add_value(value);
        }
    }

    pub fn get_by_id(&self, id: &[u8]) -> Option<Trace> {
        self.traces.peek(id).cloned()
    }
}

pub struct MyServer {
    state: Arc<RwLock<State>>,
}

impl MyServer {
    pub fn new(state: Arc<RwLock<State>>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl TraceService for MyServer {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> std::result::Result<Response<ExportTraceServiceResponse>, Status> {
        let request = request.into_inner();

        let mut state = self.state.write().await;
        for resource_spans in request.resource_spans {
            state.apply(resource_spans);
        }

        if let Some(completed) = state.traces.iter().find(|(_, trace)| trace.is_complete()) {
            println!("{}", completed.1.hex_id());
        }

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}
