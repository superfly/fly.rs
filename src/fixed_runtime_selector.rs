use crate::runtime::Runtime;
use crate::{RuntimeSelector, SelectorError};

pub struct FixedRuntimeSelector {
    runtime: Box<Runtime>,
}

impl FixedRuntimeSelector {
    pub fn new(runtime: Box<Runtime>) -> Self {
        FixedRuntimeSelector { runtime }
    }
}

impl RuntimeSelector for FixedRuntimeSelector {
    fn get_by_hostname(&self, _: &str) -> Result<Option<&mut Runtime>, SelectorError> {
        Ok(Some(self.runtime.ptr.to_runtime()))
    }
}

impl Drop for FixedRuntimeSelector {
    fn drop(&mut self) {
        debug!("Dropping FixedRuntime, will dispose of runtime.");
        self.runtime.dispose();
    }
}
