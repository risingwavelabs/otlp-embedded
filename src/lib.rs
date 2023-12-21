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

pub struct Value {
    span: Span,
    resource: Arc<Resource>,
}

pub enum ValueNode {
    Placeholder,
    Value(Value),
}

#[derive(Default)]
pub struct Trace {
    spans: HashMap<SpanId, ValueNode>,
}

pub use proto::collector::trace::v1::trace_service_server::TraceServiceServer;

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

    pub fn is_complete(&self) -> bool {
        // Since all new non-root values recorded will add a placeholder for the parent.
        // If there's no placeholder, it means the trace is complete.
        self.spans
            .values()
            .all(|v| matches!(v, ValueNode::Value(_)))
    }

    pub fn to_tempo(&self) -> serde_json::Value {
        let entries = self
            .spans
            .values()
            .filter_map(|node| match node {
                ValueNode::Placeholder => None,
                ValueNode::Value(value) => Some(value),
            })
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
}

pub struct State {
    traces: LruMap<TraceId, Trace>,
}

impl State {
    fn new() -> Self {
        Self {
            traces: LruMap::new(ByLength::new(100)),
        }
    }

    fn add_value(&mut self, value: Value) {
        if self.traces.is_empty() {
            println!("got first value!!!");
        }

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
}

pub struct MyServer {
    state: RwLock<State>,
}

impl MyServer {
    pub fn new() -> Self {
        Self {
            state: State::new().into(),
        }
    }
}

#[tonic::async_trait]
impl TraceService for MyServer {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> std::result::Result<Response<ExportTraceServiceResponse>, Status> {
        let request = request.into_inner();

        // eprintln!("Got a request {:#?}", request);

        let mut state = self.state.write().await;
        for resource_spans in request.resource_spans {
            state.apply(resource_spans);
        }

        if let Some(completed) = state.traces.iter().find(|(_, trace)| trace.is_complete()) {
            println!("{}", completed.1.to_tempo());
        }

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}
