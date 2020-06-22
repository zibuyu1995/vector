use super::Transform;
use crate::{
    event::discriminant::Discriminant,
    event::merge_state::LogEventMergeState,
    event::{self, Event},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use serde::{Deserialize, Serialize};
use std::collections::{hash_map, HashMap};
use string_cache::DefaultAtom as Atom;

mod line_agg;
use line_agg::LineAgg;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MultilineConfig {
    pub start_pattern: String,
    pub condition_pattern: String,
    pub mode: line_agg::Mode,
    pub stream_discriminant_fields: Vec<Atom>,
}

inventory::submit! {
    TransformDescription::new::<MultilineConfig>("multiline")
}

#[typetag::serde(name = "multiline")]
impl TransformConfig for MultilineConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(Multiline::from(self.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "multiline"
    }
}

#[derive(Debug)]
pub struct Multiline {
    line_agg: LineAgg<>,
}

impl TryFrom<&MultilineConfig> for line_agg::Config {
    type Error = crate::Error;

    fn try_from(config: &MultilineConfig) -> crate::Result<Self> {
        let MultilineConfig {
            start_pattern,
            condition_pattern,
            mode,
            timeout_ms,
        } = config;

        let start_pattern = Regex::new(start_pattern)
            .with_context(|| InvalidMultilineStartPattern { start_pattern })?;
        let condition_pattern = Regex::new(condition_pattern)
            .with_context(|| InvalidMultilineConditionPattern { condition_pattern })?;
        let mode = mode.clone();
        let timeout = Duration::from_millis(*timeout_ms);

        Ok(Self {
            start_pattern,
            condition_pattern,
            mode,
            timeout,
        })
    }
}

impl TryFrom<MultilineConfig> for Multiline {
    fn from(config: MultilineConfig) -> Self {
        Self {
            partial_event_marker_field: config.partial_event_marker_field,
            merge_fields: config.merge_fields,
            stream_discriminant_fields: config.stream_discriminant_fields,
            log_event_merge_states: HashMap::new(),
        }
    }
}

impl Transform for Multiline {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut event = event.into_log();

        // Prepare an event's discriminant.
        let discriminant = Discriminant::from_log_event(&event, &self.stream_discriminant_fields);

        // If current event has the partial marker, consider it partial.
        // Remove the partial marker from the event and stash it.
        if event.remove(&self.partial_event_marker_field).is_some() {
            // We got a perial event. Initialize a partial event merging state
            // if there's none available yet, or extend the existing one by
            // merging the incoming partial event in.
            match self.log_event_merge_states.entry(discriminant) {
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(LogEventMergeState::new(event));
                }
                hash_map::Entry::Occupied(mut entry) => {
                    entry
                        .get_mut()
                        .merge_in_next_event(event, &self.merge_fields);
                }
            }

            // Do not emit the event yet.
            return None;
        }

        // We got non-partial event. Attempt to get a partial event merge
        // state. If it's empty then we don't have a backlog of partail events
        // so we just return the event as is. Otherwise we proceed to merge in
        // the final non-partial event to the partial event merge state - and
        // then return the merged event.
        let log_event_merge_state = match self.log_event_merge_states.remove(&discriminant) {
            Some(log_event_merge_state) => log_event_merge_state,
            None => return Some(Event::Log(event)),
        };

        // Multiline in the final non-partial event and consume the merge state in
        // exchange for the merged event.
        let merged_event = log_event_merge_state.merge_in_final_event(event, &self.merge_fields);

        // Return the merged event.
        Some(Event::Log(merged_event))
    }
}
