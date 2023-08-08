mod proto;

use std::{collections::HashMap, sync::Arc};

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
    tree: indextree::Arena<ValueNode>,
    span_to_id: HashMap<SpanId, indextree::NodeId>,
}

pub use proto::collector::trace::v1::trace_service_server::TraceServiceServer;

impl Trace {
    fn add_value(&mut self, value: Value) {
        if value.span.span_id.is_empty() {
            return;
        }

        if value.span.name.starts_with("Epoch") {
            eprintln!("{:#?}", value.span);
        }

        let parent_id = {
            let parent_span_id = value.span.parent_span_id.clone();

            if parent_span_id.is_empty() {
                None
            } else {
                match self.span_to_id.entry(parent_span_id) {
                    std::collections::hash_map::Entry::Occupied(o) => *o.get(),
                    std::collections::hash_map::Entry::Vacant(v) => {
                        *v.insert(self.tree.new_node(ValueNode::Placeholder))
                    }
                }
                .into()
            }
        };

        if let Some(&id) = self.span_to_id.get(&value.span.span_id) {
            let node = &mut self.tree[id];
            match node.get_mut() {
                n @ ValueNode::Placeholder => *n = ValueNode::Value(value),
                ValueNode::Value(original) => {
                    original.span.events.extend(value.span.events);
                    original.span.start_time_unix_nano = original
                        .span
                        .start_time_unix_nano
                        .min(value.span.start_time_unix_nano);
                    original.span.end_time_unix_nano = original
                        .span
                        .end_time_unix_nano
                        .max(value.span.end_time_unix_nano);
                }
            }
        } else {
            let span_id = value.span.span_id.clone();
            let child_id = if let Some(parent_id) = parent_id {
                parent_id.append_value(ValueNode::Value(value), &mut self.tree)
            } else {
                self.tree.new_node(ValueNode::Value(value))
            };
            let old = self.span_to_id.insert(span_id, child_id);
            assert!(old.is_none());
        }
    }

    pub fn is_complete(&self) -> bool {
        !self.tree.is_empty()
            && self
                .tree
                .iter()
                .all(|n| matches!(n.get(), ValueNode::Value(_)))
    }

    pub fn to_tempo(&self) -> serde_json::Value {
        let entries = self
            .tree
            .iter()
            .map(|node| node.get())
            .filter_map(|value| match value {
                ValueNode::Placeholder => None,
                ValueNode::Value(value) => Some(value),
            })
            .map(|v| {
                json!({
                    "resource": (*v.resource).clone(),
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

        // if let Some(completed) = state
        //     .traces
        //     .iter()
        //     .find(|(_, trace)| trace.is_complete() && trace.tree.iter().count() > 5)
        // {
        //     println!("{}", completed.1.to_tempo());
        // }

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}
