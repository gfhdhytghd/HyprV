//! Pure StatusNotifierItem / dbusmenu parsing with bounds on untrusted input.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::zvariant::{OwnedValue, Value};

pub const DEFAULT_ITEM_PATH: &str = "/StatusNotifierItem";
const MAX_PIXMAPS: usize = 8;
const MAX_PIXMAP_SIDE: i32 = 512;
const MAX_MENU_DEPTH: usize = 8;
const MAX_MENU_ENTRIES: usize = 1024;
const MAX_ICON_DATA: usize = 256 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayPixmap {
    pub width: u32,
    pub height: u32,
    /// ARGB32, network byte order, as sent by the item.
    pub argb: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayItem {
    /// Watcher registration string `service/path` (stable key).
    pub key: String,
    pub service: String,
    pub path: String,
    /// Quickshell `id` (SNI `Id`).
    pub id: String,
    pub title: String,
    /// `Passive` | `Active` | `NeedsAttention`.
    pub status: String,
    pub category: String,
    /// Quickshell-style `icon`: themed name (attention name while NeedsAttention), with
    /// `?path=<IconThemePath>` appended when the item ships its own theme directory
    /// (consumed by `trayIconSource`, `shell.qml:2512`). Empty for pixmap-only items.
    pub icon: String,
    pub icon_name: String,
    pub icon_theme_path: String,
    pub icon_pixmaps: Vec<TrayPixmap>,
    pub attention_icon_name: String,
    pub attention_pixmaps: Vec<TrayPixmap>,
    pub overlay_icon_name: String,
    pub overlay_pixmaps: Vec<TrayPixmap>,
    pub tooltip_icon_name: String,
    /// Quickshell `tooltipTitle`
    pub tooltip_title: String,
    /// Quickshell `tooltipDescription`
    pub tooltip_description: String,
    /// Quickshell `onlyMenu` (SNI `ItemIsMenu`).
    pub only_menu: bool,
    /// dbusmenu object path, if any.
    pub menu: Option<String>,
    /// Quickshell `hasMenu`.
    pub has_menu: bool,
    /// Registration order (Quickshell `SystemTray.items` order).
    pub order: u64,
}

/// Split a watcher registration string into (service, path).
pub fn split_key(key: &str) -> (String, String) {
    match key.find('/') {
        Some(i) => (key[..i].to_owned(), key[i..].to_owned()),
        None => (key.to_owned(), DEFAULT_ITEM_PATH.to_owned()),
    }
}

/// KDE watcher normalization: a leading `/` means "path on the sender's connection".
pub fn registration_key(service_or_path: &str, sender: &str) -> String {
    if service_or_path.starts_with('/') {
        format!("{sender}{service_or_path}")
    } else if service_or_path.contains('/') {
        service_or_path.to_owned()
    } else {
        format!("{service_or_path}{DEFAULT_ITEM_PATH}")
    }
}

fn str_of(map: &HashMap<String, OwnedValue>, k: &str) -> String {
    map.get(k).and_then(|v| match &**v {
        Value::Str(s) => Some(s.to_string()),
        Value::ObjectPath(p) => Some(p.to_string()),
        _ => None,
    }).unwrap_or_default()
}

pub fn parse_pixmaps(value: &Value) -> Vec<TrayPixmap> {
    let Value::Array(items) = unwrap_variant(value) else { return Vec::new() };
    items.iter().filter_map(|item| {
        let Value::Structure(s) = unwrap_variant(item) else { return None };
        let f = s.fields();
        let (Some(Value::I32(w)), Some(Value::I32(h)), Some(Value::Array(bytes))) = (f.first(), f.get(1), f.get(2)) else { return None };
        if *w <= 0 || *h <= 0 || *w > MAX_PIXMAP_SIDE || *h > MAX_PIXMAP_SIDE { return None }
        let argb: Vec<u8> = bytes.iter().filter_map(|b| if let Value::U8(b) = b { Some(*b) } else { None }).collect();
        if argb.len() != (*w as usize) * (*h as usize) * 4 { return None }
        Some(TrayPixmap { width: *w as u32, height: *h as u32, argb })
    }).take(MAX_PIXMAPS).collect()
}

/// Apply an `org.kde.StatusNotifierItem` GetAll result.
pub fn apply_item_props(item: &mut TrayItem, map: &HashMap<String, OwnedValue>) {
    item.id = str_of(map, "Id");
    item.title = str_of(map, "Title");
    item.status = str_of(map, "Status");
    item.category = str_of(map, "Category");
    item.icon_name = str_of(map, "IconName");
    item.icon_theme_path = str_of(map, "IconThemePath");
    item.attention_icon_name = str_of(map, "AttentionIconName");
    item.overlay_icon_name = str_of(map, "OverlayIconName");
    let pix = |k: &str| map.get(k).map(|v| parse_pixmaps(v)).unwrap_or_default();
    item.icon_pixmaps = pix("IconPixmap");
    item.attention_pixmaps = pix("AttentionIconPixmap");
    item.overlay_pixmaps = pix("OverlayIconPixmap");
    item.tooltip_icon_name.clear();
    item.tooltip_title.clear();
    item.tooltip_description.clear();
    if let Some(Value::Structure(s)) = map.get("ToolTip").map(|v| &**v) {
        let f = s.fields();
        let text = |i: usize| match f.get(i) { Some(Value::Str(s)) => s.to_string(), _ => String::new() };
        item.tooltip_icon_name = text(0);
        item.tooltip_title = text(2);
        item.tooltip_description = text(3);
    }
    item.only_menu = map.get("ItemIsMenu").and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
    let menu = str_of(map, "Menu");
    item.menu = (!menu.is_empty() && menu != "/").then_some(menu);
    item.has_menu = item.menu.is_some();
    let name = if item.status == "NeedsAttention" && !item.attention_icon_name.is_empty() {
        &item.attention_icon_name
    } else {
        &item.icon_name
    };
    item.icon = if name.is_empty() {
        String::new()
    } else if !item.icon_theme_path.is_empty() && !name.starts_with('/') {
        format!("{name}?path={}", item.icon_theme_path)
    } else {
        name.clone()
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ButtonType { #[default] None, CheckBox, RadioButton }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckState { #[default] Unchecked, PartiallyChecked, Checked }

/// Quickshell `QsMenuEntry` surface used by `TrayMenuPopup` (`shell.qml:611`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayMenuEntry {
    pub id: i32,
    /// Label with mnemonics resolved (`_x` → `x`, `__` → `_`).
    pub text: String,
    pub enabled: bool,
    pub visible: bool,
    pub is_separator: bool,
    pub button_type: ButtonType,
    pub check_state: CheckState,
    pub has_children: bool,
    pub icon_name: String,
    /// PNG bytes of `icon-data`.
    pub icon_data: Vec<u8>,
    pub children: Vec<TrayMenuEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayMenu {
    pub item_key: String,
    pub revision: u32,
    pub root: TrayMenuEntry,
}

pub fn strip_mnemonic(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' {
            if chars.peek() == Some(&'_') {
                chars.next();
                out.push('_');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse a dbusmenu `(ia{sv}av)` layout node.
pub fn parse_layout(value: &Value) -> Option<TrayMenuEntry> {
    let mut budget = MAX_MENU_ENTRIES;
    parse_node(value, 0, &mut budget)
}

/// Parse the typed `GetLayout` root `(i a{sv} av)`.
pub fn parse_layout_parts(id: i32, props: &HashMap<String, OwnedValue>, children: &[OwnedValue]) -> Option<TrayMenuEntry> {
    let mut budget = MAX_MENU_ENTRIES;
    let props: HashMap<String, Value> = props.iter().filter_map(|(k, v)| Some((k.clone(), unwrap_variant(v).try_clone().ok()?))).collect();
    let children: Vec<&Value> = children.iter().map(|c| &**c).collect();
    build_entry(id, props, &children, 0, &mut budget)
}

fn unwrap_variant<'a>(v: &'a Value<'a>) -> &'a Value<'a> {
    match v { Value::Value(inner) => unwrap_variant(inner), v => v }
}

fn parse_node(value: &Value, depth: usize, budget: &mut usize) -> Option<TrayMenuEntry> {
    let Value::Structure(s) = unwrap_variant(value) else { return None };
    let f = s.fields();
    let Some(Value::I32(id)) = f.first() else { return None };
    let props: HashMap<String, Value> = match f.get(1) {
        Some(Value::Dict(d)) => d.iter().filter_map(|(k, v)| {
            let Value::Str(k) = k else { return None };
            Some((k.to_string(), unwrap_variant(v).try_clone().ok()?))
        }).collect(),
        _ => HashMap::new(),
    };
    let children: Vec<&Value> = match f.get(2) { Some(Value::Array(items)) => items.iter().collect(), _ => Vec::new() };
    build_entry(*id, props, &children, depth, budget)
}

fn build_entry(id: i32, props: HashMap<String, Value>, kids: &[&Value], depth: usize, budget: &mut usize) -> Option<TrayMenuEntry> {
    if *budget == 0 { return None }
    *budget -= 1;
    let text = |k: &str| match props.get(k) { Some(Value::Str(s)) => s.to_string(), _ => String::new() };
    let boolean = |k: &str, d: bool| match props.get(k) { Some(Value::Bool(b)) => *b, _ => d };
    let mut children = Vec::new();
    if depth < MAX_MENU_DEPTH {
        for child in kids {
            if let Some(c) = parse_node(child, depth + 1, budget) { children.push(c) }
        }
    }
    let toggle_state = match props.get("toggle-state") { Some(Value::I32(i)) => *i, _ => -1 };
    let button_type = match text("toggle-type").as_str() { "checkmark" => ButtonType::CheckBox, "radio" => ButtonType::RadioButton, _ => ButtonType::None };
    let icon_data = match props.get("icon-data") {
        Some(Value::Array(a)) if a.len() <= MAX_ICON_DATA => a.iter().filter_map(|b| if let Value::U8(b) = b { Some(*b) } else { None }).collect(),
        _ => Vec::new(),
    };
    Some(TrayMenuEntry {
        id,
        text: strip_mnemonic(&text("label")),
        enabled: boolean("enabled", true),
        visible: boolean("visible", true),
        is_separator: text("type") == "separator",
        button_type,
        check_state: match (button_type, toggle_state) {
            (ButtonType::None, _) => CheckState::Unchecked,
            (_, 1) => CheckState::Checked,
            (_, 0) => CheckState::Unchecked,
            _ => CheckState::PartiallyChecked,
        },
        has_children: text("children-display") == "submenu" || !children.is_empty(),
        icon_name: text("icon-name"),
        icon_data,
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Array, Structure, StructureBuilder};

    #[test]
    fn keys() {
        assert_eq!(split_key(":1.5/org/ayatana/NotificationItem/x"), (":1.5".into(), "/org/ayatana/NotificationItem/x".into()));
        assert_eq!(split_key("org.kde.StatusNotifierItem-1-2"), ("org.kde.StatusNotifierItem-1-2".into(), DEFAULT_ITEM_PATH.into()));
        assert_eq!(registration_key("/org/x", ":1.9"), ":1.9/org/x");
        assert_eq!(registration_key("org.kde.StatusNotifierItem-1-2", ":1.9"), "org.kde.StatusNotifierItem-1-2/StatusNotifierItem");
        assert_eq!(registration_key(":1.4/a", ":1.9"), ":1.4/a");
    }
    #[test]
    fn mnemonics() {
        assert_eq!(strip_mnemonic("_Open"), "Open");
        assert_eq!(strip_mnemonic("Save__As"), "Save_As");
        assert_eq!(strip_mnemonic("Plain"), "Plain");
    }
    fn pixmap(w: i32, h: i32, n: usize) -> Value<'static> {
        Value::Structure(StructureBuilder::new().add_field(w).add_field(h).add_field(vec![7u8; n]).build().unwrap())
    }
    #[test]
    fn pixmaps_are_validated() {
        let arr = Value::Array(Array::from(vec![pixmap(2, 2, 16), pixmap(2, 2, 15), pixmap(0, 2, 0), pixmap(4096, 1, 16384)]));
        let p = parse_pixmaps(&arr);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].width, p[0].argb.len()), (2, 16));
    }
    #[test]
    fn item_props_and_icon_string() {
        let mut m: HashMap<String, OwnedValue> = HashMap::new();
        let put = |m: &mut HashMap<String, OwnedValue>, k: &str, v: Value| { m.insert(k.into(), OwnedValue::try_from(v).unwrap()); };
        put(&mut m, "Id", Value::from("discord"));
        put(&mut m, "Status", Value::from("Active"));
        put(&mut m, "IconName", Value::from("discord-tray"));
        put(&mut m, "IconThemePath", Value::from("/opt/discord/icons"));
        put(&mut m, "AttentionIconName", Value::from("discord-alert"));
        put(&mut m, "Menu", Value::from(zbus::zvariant::ObjectPath::try_from("/MenuBar").unwrap()));
        put(&mut m, "ItemIsMenu", Value::from(true));
        let tooltip: Structure = StructureBuilder::new().add_field("").add_field(Vec::<(i32, i32, Vec<u8>)>::new()).add_field("Discord").add_field("2 unread").build().unwrap();
        put(&mut m, "ToolTip", Value::Structure(tooltip));
        let mut item = TrayItem::default();
        apply_item_props(&mut item, &m);
        assert_eq!(item.icon, "discord-tray?path=/opt/discord/icons");
        assert_eq!((item.tooltip_title.as_str(), item.tooltip_description.as_str()), ("Discord", "2 unread"));
        assert!(item.only_menu && item.has_menu);
        put(&mut m, "Status", Value::from("NeedsAttention"));
        put(&mut m, "Menu", Value::from(zbus::zvariant::ObjectPath::try_from("/").unwrap()));
        apply_item_props(&mut item, &m);
        assert_eq!(item.icon, "discord-alert?path=/opt/discord/icons");
        assert!(!item.has_menu);
    }
    fn node(id: i32, props: Vec<(&str, Value<'static>)>, children: Vec<Value<'static>>) -> Value<'static> {
        let mut dict = HashMap::<String, Value>::new();
        for (k, v) in props { dict.insert(k.into(), Value::Value(Box::new(v))); }
        let kids: Vec<Value> = children.into_iter().map(|c| Value::Value(Box::new(c))).collect();
        Value::Structure(StructureBuilder::new().add_field(id).add_field(dict).append_field(Value::Array(Array::from(kids))).build().unwrap())
    }
    #[test]
    fn layout_parsing() {
        let tree = node(0, vec![("children-display", Value::from("submenu"))], vec![
            node(1, vec![("label", Value::from("_Show")), ("toggle-type", Value::from("checkmark")), ("toggle-state", Value::from(1i32))], vec![]),
            node(2, vec![("type", Value::from("separator"))], vec![]),
            node(3, vec![("label", Value::from("Sub")), ("children-display", Value::from("submenu")), ("enabled", Value::from(false))], vec![
                node(4, vec![("label", Value::from("Leaf")), ("visible", Value::from(false))], vec![]),
            ]),
        ]);
        let root = parse_layout(&tree).unwrap();
        assert!(root.has_children);
        assert_eq!(root.children.len(), 3);
        let show = &root.children[0];
        assert_eq!((show.text.as_str(), show.button_type, show.check_state, show.enabled), ("Show", ButtonType::CheckBox, CheckState::Checked, true));
        assert!(root.children[1].is_separator);
        let sub = &root.children[2];
        assert!(!sub.enabled && sub.has_children && !sub.children[0].visible);
    }
    #[test]
    fn layout_depth_is_bounded() {
        let mut tree = node(99, vec![], vec![]);
        for i in 0..20 { tree = node(i, vec![], vec![tree]); }
        let mut depth = 0;
        let root = parse_layout(&tree).unwrap();
        let mut cur = &root;
        while let Some(c) = cur.children.first() { depth += 1; cur = c; }
        assert_eq!(depth, MAX_MENU_DEPTH);
    }
}
