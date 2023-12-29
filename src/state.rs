use std::{
    collections::{hash_map::Entry, BTreeSet, HashMap},
    sync::Arc,
    time::{Duration, SystemTime},
};

use crate::proto::{
    common::v1::{any_value, AnyValue, KeyValue},
    resource::v1::Resource,
    trace::v1::{ResourceSpans, Span},
};
use itertools::Itertools;
use schnellru::{ByLength, LruMap};
use serde_json::json;
use tokio::sync::RwLock;

use crate::jaeger::model::{span_to_jaeger_json, JaegerProcess};

type TraceId = Vec<u8>;
type SpanId = Vec<u8>;

#[derive(Debug, Clone)]
pub struct Value {
    pub span: Span,
    pub resource: Arc<Resource>,
}

fn extract_string<'a>(attr: &'a [KeyValue], key: &'static str) -> &'a str {
    attr.iter()
        .find(|a| a.key == key)
        .and_then(|kv| {
            if let Some(AnyValue {
                value: Some(any_value::Value::StringValue(str)),
            }) = &kv.value
            {
                Some(str.as_str())
            } else {
                None
            }
        })
        .unwrap_or("unknown")
}

impl Value {
    pub fn service_name(&self) -> &str {
        extract_string(&self.resource.attributes, "service.name")
    }

    pub fn service_instance_id(&self) -> &str {
        extract_string(&self.resource.attributes, "service.instance.id")
    }

    pub fn operation(&self) -> &str {
        self.span.name.as_str()
    }
}

#[derive(Debug, Clone)]
pub enum ValueNode {
    Placeholder,
    Value(Value),
}

#[derive(Debug, Clone)]
pub struct Trace {
    pub spans: HashMap<SpanId, ValueNode>,
    pub end_time: SystemTime,
}

impl Default for Trace {
    fn default() -> Self {
        Self {
            spans: Default::default(),
            end_time: SystemTime::UNIX_EPOCH,
        }
    }
}

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

            event.attributes.push(KeyValue {
                key: MESSAGE.to_string(),
                value: Some(AnyValue {
                    value: Some(any_value::Value::StringValue(event.name.clone())),
                }),
            });
        }

        self.end_time = (self.end_time)
            .max(SystemTime::UNIX_EPOCH + Duration::from_nanos(value.span.end_time_unix_nano as _));

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

    pub fn to_tempo_batch(&self) -> serde_json::Value {
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

    pub fn to_jaeger_batch(&self) -> serde_json::Value {
        json!({
            "data": [
                self.to_jaeger()
            ]
        })
    }

    pub fn to_jaeger(&self) -> serde_json::Value {
        let mut processes = HashMap::new();

        let entries = self
            .iter_valid()
            .map(|v| {
                let process = JaegerProcess::from(v);
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
            "traceID": trace_id,
            "spans": entries,
            "processes": processes,
        })
    }
}

impl Trace {
    fn root_span(&self) -> Option<&Value> {
        self.iter_valid().find(|v| v.span.parent_span_id.is_empty())
    }

    pub fn service_name(&self) -> Option<&str> {
        self.root_span().map(|v| v.service_name())
    }

    pub fn service_instance_id(&self) -> Option<&str> {
        self.root_span().map(|v| v.service_instance_id())
    }

    pub fn operation(&self) -> Option<&str> {
        self.root_span().map(|v| v.operation())
    }
}

pub struct State {
    traces: LruMap<TraceId, Trace>,
}

pub type StateRef = Arc<RwLock<State>>;

impl State {
    pub fn new(recent: u32) -> StateRef {
        let this = Self {
            traces: LruMap::new(ByLength::new(recent)),
        };

        Arc::new(RwLock::new(this))
    }

    fn add_value(&mut self, value: Value) {
        self.traces
            .get_or_insert(value.span.trace_id.clone(), Default::default)
            .unwrap()
            .add_value(value);
    }

    pub(crate) fn apply(&mut self, resource_spans: ResourceSpans) {
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

    pub fn get_all_complete(&self) -> impl Iterator<Item = Trace> + '_ {
        self.traces.iter().filter_map(|(_, trace)| {
            if trace.is_complete() {
                Some(trace.clone())
            } else {
                None
            }
        })
    }

    pub fn get_all_services(&self) -> BTreeSet<&str> {
        self.traces
            .iter()
            .filter_map(|(_, t)| t.root_span())
            .map(|v| v.service_name())
            .collect()
    }

    pub fn get_operations(&self, service_name: &str) -> BTreeSet<&str> {
        self.traces
            .iter()
            .filter_map(|(_, t)| t.root_span())
            .filter(|v| v.service_name() == service_name)
            .map(|v| v.operation())
            .collect()
    }
}
