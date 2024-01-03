use std::{collections::BTreeSet, sync::Arc};

use crate::{
    proto::trace::v1::ResourceSpans,
    trace::{SpanValue, Trace, TraceId},
};
use schnellru::{ByLength, LruMap};
use tokio::sync::RwLock;

/// In-memory state that maintains the most recent traces.
///
/// Old traces that are no longer updated or accessed will be evicted
/// when the capacity is reached.
pub struct State {
    traces: LruMap<TraceId, Trace>,
}

/// A reference to the [`State`].
pub type StateRef = Arc<RwLock<State>>;

impl State {
    /// Create a new [`State`] with the given number of recent traces to keep.
    pub fn new(recent: u32) -> StateRef {
        let this = Self {
            traces: LruMap::new(ByLength::new(recent)),
        };

        Arc::new(RwLock::new(this))
    }

    fn add_value(&mut self, value: SpanValue) {
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
            let value = SpanValue {
                span,
                resource: resource.clone(),
            };
            self.add_value(value);
        }
    }

    /// Get a trace by its ID.
    ///
    /// The trace will be promoted to the most recent.
    pub fn get_by_id(&mut self, id: &[u8]) -> Option<Trace> {
        self.traces.get(id).cloned()
    }

    /// Get an iterator over all traces that are complete.
    pub fn get_all_complete(&self) -> impl Iterator<Item = Trace> + '_ {
        self.traces.iter().filter_map(|(_, trace)| {
            if trace.is_complete() {
                Some(trace.clone())
            } else {
                None
            }
        })
    }

    /// Get a set of all services.
    pub fn get_all_services(&self) -> BTreeSet<&str> {
        self.traces
            .iter()
            .filter_map(|(_, t)| t.root_span())
            .map(|v| v.service_name())
            .collect()
    }

    /// Get a set of all operations for the given service.
    pub fn get_operations(&self, service_name: &str) -> BTreeSet<&str> {
        self.traces
            .iter()
            .filter_map(|(_, t)| t.root_span())
            .filter(|v| v.service_name() == service_name)
            .map(|v| v.operation())
            .collect()
    }
}
