use std::collections::HashMap;

use crate::milestone::MilestoneFile;

impl MilestoneFile {
    /// Merge legacy `work_packages[].steps` into top-level `[[steps]]` after load.
    ///
    /// Canonical top-level steps take precedence over the nested migration
    /// shape. Duplicate IDs among nested steps, or between nested and
    /// top-level steps, are rejected with both source locations instead of
    /// silently dropping the nested step.
    pub fn normalize_steps_from_disk(&mut self) -> Result<(), String> {
        let top_level_locations: HashMap<String, String> = self
            .steps
            .iter()
            .enumerate()
            .map(|(idx, s)| (s.id.clone(), format!("steps[{idx}]")))
            .collect();
        let mut nested_locations: HashMap<String, String> = HashMap::new();
        let mut from_wp = Vec::new();
        for (wp_index, wp) in self.work_packages.iter_mut().enumerate() {
            for (step_index, mut step) in wp.steps.drain(..).enumerate() {
                let location = format!("work_packages[{wp_index}]({}).steps[{step_index}]", wp.id);
                if let Some(first) = nested_locations.insert(step.id.clone(), location.clone()) {
                    return Err(format!(
                        "duplicate legacy nested step id {:?}: first at {first}, again at {location}",
                        step.id
                    ));
                }
                if let Some(top_loc) = top_level_locations.get(&step.id) {
                    // Top-level would win for migration inputs, but every
                    // conflicting nested location must be reported — silent
                    // drop hid collisions from agents.
                    return Err(format!(
                        "duplicate step id {:?}: top-level at {top_loc}, nested at {location}",
                        step.id
                    ));
                }
                if step.work_package.is_empty() {
                    step.work_package = wp.id.clone();
                }
                from_wp.push(step);
            }
        }
        self.steps.extend(from_wp);
        Ok(())
    }

    /// Strip nested steps before writing canonical on-disk shape.
    pub fn prepare_for_disk(&mut self) {
        for wp in &mut self.work_packages {
            wp.steps.clear();
        }
    }

    pub fn has_implementation_plan(&self) -> bool {
        !self.steps.is_empty() || self.work_packages.iter().any(|wp| !wp.steps.is_empty())
    }
}
