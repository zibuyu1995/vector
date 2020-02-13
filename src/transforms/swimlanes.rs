use super::Transform;
use crate::{
    conditions::{Condition, ConditionConfig},
    event::Event,
    runtime::TaskExecutor,
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

//------------------------------------------------------------------------------

pub struct Swimlanes {
    lanes: IndexMap<String, Box<dyn Condition>>,
}

impl Swimlanes {
    pub fn new(lanes: IndexMap<String, Box<dyn Condition>>) -> Self {
        Self { lanes }
    }
}

impl Transform for Swimlanes {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut output = Vec::new();
        let mut named_outputs = Vec::new();
        self.transform_into(&mut output, &mut named_outputs, event);
        output.pop()
    }

    fn transform_into(&mut self, output: &mut Vec<Event>, named_outputs: &mut Vec<(String, Vec<Event>)>, event: Event) {
    }
}

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SwimlanesConfig {
    lanes: IndexMap<String, Box<dyn ConditionConfig>>,
}

inventory::submit! {
    TransformDescription::new_without_default::<SwimlanesConfig>("swimlanes")
}

#[typetag::serde(name = "swimlanes")]
impl TransformConfig for SwimlanesConfig {
    fn build(&self, _exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        let mut lanes: IndexMap<String, Box<dyn Condition>> = IndexMap::new();

        for (k, cond) in &self.lanes {
            lanes.insert(k.clone(), cond.build()?);
        }

        if lanes.is_empty() {
            Err("swimlanes must have at least one lane".into())
        } else {
            Ok(Box::new(Swimlanes{lanes}))
        }
    }

    fn named_outputs(&self) -> Option<Vec<String>> {
        Some(self.lanes.keys().map(|s| s.clone()).collect())
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "swimlanes"
    }
}

//------------------------------------------------------------------------------
