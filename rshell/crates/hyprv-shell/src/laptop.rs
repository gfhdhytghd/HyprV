//! Host-owned adaptive bar targets, shared by assembled and component hosts.
use std::collections::HashSet;
#[derive(Default)]
pub(crate) struct BarExpansion {
    expanded: HashSet<String>,
    gate: hyprv_ui_bar::ipc::ToggleGate,
}
impl BarExpansion {
    pub(crate) fn expanded(&self, output: &str) -> bool {
        self.expanded.contains(output)
    }
    /// Legacy Date.now debounce is global across monitors, including backward clock steps.
    pub(crate) fn toggle(&mut self, output: &str, wall_ms: u64) -> bool {
        if !self.gate.allow(wall_ms) {
            return false;
        }
        if !self.expanded.remove(output) {
            self.expanded.insert(output.to_owned());
        }
        true
    }
    pub(crate) fn remove(&mut self, output: &str) {
        self.expanded.remove(output);
    }
}
pub(crate) fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub(crate) fn active() -> bool {
    hyprv_core::profile::profile() == hyprv_core::profile::Profile::Laptop
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starts_compact_global_debounce_and_hotplug_reset() {
        let mut b = BarExpansion::default();
        assert!(!b.expanded("A"));
        assert!(b.toggle("A", 1000));
        assert!(b.expanded("A"));
        assert!(!b.expanded("B"));
        assert!(!b.toggle("B", 1300));
        assert!(!b.toggle("A", 900));
        assert!(b.toggle("B", 1500));
        assert!(b.expanded("B"));
        b.remove("A");
        assert!(!b.expanded("A"));
        assert!(b.expanded("B"));
    }
}
