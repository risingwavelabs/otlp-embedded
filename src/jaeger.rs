// https://github.com/open-telemetry/opentelemetry-rust-contrib/blob/a3e0d18247d972e36a386785e50ca78b2ceec234/opentelemetry-contrib/src/trace/exporter/jaeger_json.rs

use serde::Serialize;

use crate::proto::{
    common::v1::{any_value, AnyValue, KeyValue},
    resource::v1::Resource,
    trace::v1::Span,
};

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn any_value_to_serde_value(value: any_value::Value) -> serde_json::Value {
    match value {
        any_value::Value::StringValue(s) => s.into(),
        any_value::Value::BoolValue(b) => b.into(),
        any_value::Value::IntValue(i) => i.into(),
        any_value::Value::DoubleValue(d) => d.into(),
        any_value::Value::ArrayValue(a) => {
            let v = a
                .values
                .into_iter()
                .flat_map(|v| v.value.map(any_value_to_serde_value))
                .collect::<Vec<_>>();
            serde_json::Value::Array(v).to_string().into()
        }
        any_value::Value::BytesValue(b) => hex(&b).into(),
        any_value::Value::KvlistValue(_) => "unsupported".into(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JaegerKv {
    pub key: String,
    pub r#type: &'static str,
    pub value: serde_json::Value,
}

impl From<KeyValue> for JaegerKv {
    fn from(kv: KeyValue) -> Self {
        let key = kv.key;
        let Some(AnyValue { value: Some(value) }) = kv.value else {
            return Self {
                key,
                r#type: "string",
                value: serde_json::Value::Null,
            };
        };

        let r#type = match value {
            any_value::Value::StringValue(_) => "string",
            any_value::Value::BoolValue(_) => "bool",
            any_value::Value::IntValue(_) => "int64",
            any_value::Value::DoubleValue(_) => "float64",
            any_value::Value::ArrayValue(_)
            | any_value::Value::KvlistValue(_)
            | any_value::Value::BytesValue(_) => "string",
        };
        let value = any_value_to_serde_value(value);

        Self { key, r#type, value }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JaegerProcess {
    pub key: String,
    pub service_name: String,
    pub tags: Vec<JaegerKv>,
}

impl From<Resource> for JaegerProcess {
    fn from(resource: Resource) -> Self {
        let attr = resource.attributes;

        let extract_string = |key: &str| {
            attr.iter()
                .find(|a| a.key == key)
                .and_then(|kv| {
                    if let Some(AnyValue {
                        value: Some(any_value::Value::StringValue(str)),
                    }) = &kv.value
                    {
                        Some(str.to_owned())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "unknown".to_owned())
        };

        let key = extract_string("service.instance.id");
        let service_name = extract_string("service.name");
        let tags = attr.into_iter().map(JaegerKv::from).collect::<Vec<_>>();

        Self {
            key,
            service_name,
            tags,
        }
    }
}

pub(crate) fn span_to_jaeger_json(span: Span, process: String) -> serde_json::Value {
    let logs = span
        .events
        .into_iter()
        .map(|e| {
            let fields = e
                .attributes
                .into_iter()
                .map(JaegerKv::from)
                .collect::<Vec<_>>();

            let timestamp = e.time_unix_nano / 1000;

            serde_json::json!({
                "timestamp": timestamp,
                "fields": fields,
            })
        })
        .collect::<Vec<_>>();

    let tags = span
        .attributes
        .into_iter()
        .map(JaegerKv::from)
        .collect::<Vec<_>>();

    let mut references = if span.links.is_empty() {
        None
    } else {
        Some(
            span.links
                .into_iter()
                .map(|link| {
                    serde_json::json!({
                        "refType": "FOLLOWS_FROM",
                        "traceID": hex(&link.trace_id),
                        "spanID": hex(&link.span_id)
                    })
                })
                .collect::<Vec<_>>(),
        )
    };

    if !span.parent_span_id.is_empty() {
        let val = serde_json::json!({
            "refType": "CHILD_OF",
            "traceID": hex(&span.trace_id),
            "spanID": hex(&span.parent_span_id),
        });
        references.get_or_insert_with(Vec::new).push(val);
    }

    serde_json::json!({
        "traceID": hex(&span.trace_id),
        "spanID": hex(&span.span_id),
        "startTime": span.start_time_unix_nano / 1000,
        "duration": (span.end_time_unix_nano - span.start_time_unix_nano) / 1000,
        "operationName": span.name,
        "tags": tags,
        "logs": logs,
        // "flags": span.flags,
        "processID": process,
        "warnings": None::<String>,
        "references": references,
    })
}
