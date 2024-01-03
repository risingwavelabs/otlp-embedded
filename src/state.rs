use std::{collections::BTreeSet, sync::Arc};

use crate::{
    limiter::MyLimiter,
    proto::trace::v1::ResourceSpans,
    trace::{SpanValue, Trace, TraceId},
};
use schnellru::LruMap;
use tokio::sync::RwLock;

/// Configuration for the [`State`].
///
/// Either the maximum number of traces or the maximum memory usage
/// is reached, the oldest traces will be evicted.
pub struct Config {
    /// The maximum number of traces to keep.
    pub max_length: u32,

    /// The maximum memory usage of the traces in bytes.
    ///
    /// The memory usage is estimated and the actual value may be higher.
    pub max_memory_usage: usize,
}

/// In-memory state that maintains the most recent traces.
///
/// Old traces that are no longer updated or accessed will be evicted
/// when the capacity is reached.
pub struct State {
    traces: LruMap<TraceId, Trace, MyLimiter>,
}

/// A reference to the [`State`].
pub type StateRef = Arc<RwLock<State>>;

impl State {
    /// Create a new [`State`] with the given configuration.
    pub fn new(
        Config {
            max_length,
            max_memory_usage,
        }: Config,
    ) -> StateRef {
        let this = Self {
            traces: LruMap::new(MyLimiter::new(max_memory_usage, max_length)),
        };

        Arc::new(RwLock::new(this))
    }

    fn add_value(&mut self, value: SpanValue) {
        // Use a pair of `remove` and `insert` to maintain the memory usage correctly.
        let mut trace = self.traces.remove(&value.span.trace_id).unwrap_or_default();
        let id = value.span.trace_id.clone();
        trace.add_value(value);

        self.traces.insert(id, trace);
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

    /// Get the number of traces in the state.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.traces.len()
    }

    /// Get the estimated memory usage of the state in bytes.
    pub fn estimated_memory_usage(&self) -> usize {
        self.traces.limiter().estimated_memory_usage()
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
