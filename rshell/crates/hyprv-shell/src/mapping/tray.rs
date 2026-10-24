//! Quickshell's ObjectModel order, before shell.qml:503-521's stable priority sort.
//! qml.cpp:28-31 and core/model.hpp:89-99 in Quickshell's status_notifier service.
use hyprv_svc_media::tray::{TrayItem, TrayState};

// Preserve the upstream OR predicate; it is not a lexicographic category sort.
pub const QUIRK_QS_TRAY_CATEGORY_OR_ID: bool = true;
fn category(item: &TrayItem) -> u8 {
    match item.category.as_str() {
        "Hardware" => 0,
        "SystemServices" => 1,
        "Communications" => 3,
        _ => 2, // ApplicationStatus is Quickshell's fallback.
    }
}
pub(crate) fn ordered(mut state: TrayState) -> TrayState {
    let mut items: Vec<(TrayItem, String)> = Vec::with_capacity(state.items.len());
    for item in state.items.drain(..) {
        let key = item.id.to_lowercase();
        let at = items.iter().position(|(old, old_key)| {
            !(QUIRK_QS_TRAY_CATEGORY_OR_ID && category(&item) < category(old)
                || key >= *old_key)
        }).unwrap_or(items.len());
        items.insert(at, (item, key));
    }
    state.items = items.into_iter().map(|(item, _)| item).collect();
    state
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qobject_order_precedes_stable_qml_priority_and_overflow_slice() {
        let mut state = TrayState::default();
        for (id, category) in [("telegram-5", ""), ("chrome_status_icon_1-6", ""),
            ("Telegram-11", ""), ("chrome_status_icon_1-0", "")] {
            state.items.push(TrayItem { id:id.into(), key:id.into(), category:category.into(), ..Default::default() });
        }
        let state = ordered(state);
        let ids: Vec<_> = state.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["chrome_status_icon_1-0", "chrome_status_icon_1-6", "Telegram-11", "telegram-5"]);
    }
    #[test]
    fn category_or_predicate_and_equal_ids_keep_qt_insertion_semantics() {
        let items = [("z", "Hardware"), ("a", "ApplicationStatus"),
            ("A", "ApplicationStatus"), ("b", "SystemServices")]
            .into_iter().enumerate().map(|(n,(id, category))| TrayItem {
                id:id.into(), category:category.into(), key:n.to_string(), ..Default::default()
            }).collect();
        let state=ordered(TrayState { items, ..Default::default() });
        let keys:Vec<_>=state.items.iter().map(|i|i.key.as_str()).collect();
        assert_eq!(keys, ["1", "2", "3", "0"]);
    }
}
