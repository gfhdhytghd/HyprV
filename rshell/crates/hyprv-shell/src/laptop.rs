//! Host-owned adaptive bar targets, shared by assembled and component hosts.
/// One compact/standard state for every output (user request; the laptop QML kept one per
/// output). A toggle from any output flips all bars; new outputs follow the current state.
#[derive(Default)]
pub(crate) struct BarExpansion {
    expanded: bool,
    gate: hyprv_ui_bar::ipc::ToggleGate,
}
impl BarExpansion {
    pub(crate) fn expanded(&self, _output: &str) -> bool {
        self.expanded
    }
    /// Legacy Date.now debounce is global across monitors, including backward clock steps.
    pub(crate) fn toggle(&mut self, _output: &str, wall_ms: u64) -> bool {
        if !self.gate.allow(wall_ms) {
            return false;
        }
        self.expanded = !self.expanded;
        true
    }
    /// Output removal no longer resets anything: the state is shared.
    pub(crate) fn remove(&mut self, _output: &str) {}
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
    fn starts_compact_global_debounce_and_synced_outputs() {
        let mut b = BarExpansion::default();
        assert!(!b.expanded("A"));
        assert!(b.toggle("A", 1000));
        assert!(b.expanded("A") && b.expanded("B"), "every output follows one toggle");
        assert!(!b.toggle("B", 1300));
        assert!(!b.toggle("A", 900));
        assert!(b.toggle("B", 1500));
        assert!(!b.expanded("A") && !b.expanded("B"));
        b.remove("A");
        assert!(b.toggle("A", 2100));
        assert!(b.expanded("C"), "a new output follows the shared state");
    }
}
