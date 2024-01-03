use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
    time::{Duration, SystemTime},
};

use crate::proto::{
    common::v1::{any_value, AnyValue, KeyValue},
    resource::v1::Resource,
    trace::v1::Span,
};
use itertools::Itertools;
use serde_json::json;

use crate::jaeger::model::{span_to_jaeger_json, JaegerProcess};

pub(crate) type TraceId = Vec<u8>;
pub(crate) type SpanId = Vec<u8>;

#[derive(Debug, Clone)]
pub(crate) struct SpanValue {
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

impl SpanValue {
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
pub(crate) enum SpanNode {
    Placeholder,
    Value(SpanValue),
}

/// A trace that consists of multiple spans in a tree structure.
#[derive(Debug, Clone)]
pub struct Trace {
    pub(crate) spans: HashMap<SpanId, SpanNode>,
    pub(crate) end_time: SystemTime,
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
    pub(crate) fn add_value(&mut self, mut value: SpanValue) {
        let span_id = &value.span.span_id;
        let parent_id = &value.span.parent_span_id;

        if span_id.is_empty() {
            return;
        }

        // If there's a parent and not recorded yet, add a placeholder.
        if !parent_id.is_empty() {
            self.spans
                .entry(parent_id.clone())
                .or_insert(SpanNode::Placeholder);
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
                    SpanNode::Placeholder => *o = SpanNode::Value(value),
                    SpanNode::Value(o) => {
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
                v.insert(SpanNode::Value(value));
            }
        }
    }

    fn iter_valid(&self) -> impl Iterator<Item = &SpanValue> {
        self.spans.values().filter_map(|node| match node {
            SpanNode::Placeholder => None,
            SpanNode::Value(value) => Some(value),
        })
    }

    /// Check if the trace is complete.
    pub fn is_complete(&self) -> bool {
        // Since all new non-root values recorded will add a placeholder for the parent.
        // If there's no placeholder, it means the trace is complete.
        self.spans.values().all(|v| matches!(v, SpanNode::Value(_)))
    }

    /// Get the trace ID.
    pub fn id(&self) -> &[u8] {
        &self.iter_valid().next().unwrap().span.trace_id
    }

    /// Get the trace ID as a hex string.
    pub fn hex_id(&self) -> String {
        hex::encode(self.id())
    }

    /// Convert the trace into a JSON value that can be directly imported into Grafana Tempo
    /// as a batch.
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

    /// Convert the trace into a JSON value that can be directly imported into Jaeger
    /// as a batch.
    pub fn to_jaeger_batch(&self) -> serde_json::Value {
        json!({
            "data": [
                self.to_jaeger()
            ]
        })
    }

    pub(crate) fn to_jaeger(&self) -> serde_json::Value {
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
    pub(crate) fn root_span(&self) -> Option<&SpanValue> {
        self.iter_valid().find(|v| v.span.parent_span_id.is_empty())
    }

    /// Get the service name of the root span in this trace.
    ///
    /// Returns `None` if the trace is not complete and the root span is not received.
    pub fn service_name(&self) -> Option<&str> {
        self.root_span().map(|v| v.service_name())
    }

    /// Get the service instance ID of the root span in this trace.
    ///
    /// Returns `None` if the trace is not complete and the root span is not received.
    pub fn service_instance_id(&self) -> Option<&str> {
        self.root_span().map(|v| v.service_instance_id())
    }

    /// Get the operation (span name) of the root span in this trace.
    ///
    /// Returns `None` if the trace is not complete and the root span is not received.
    pub fn operation(&self) -> Option<&str> {
        self.root_span().map(|v| v.operation())
    }
}
