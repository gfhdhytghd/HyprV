//! Order-preserving JSON value for editing user config files in place
//! (Python `dict` semantics: replaced keys keep their position, new keys append).
//! Serialization reuses serde_json's escaping/number formatting with the
//! file's own indent, so an unchanged document round-trips byte-identically.
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    ser::{SerializeMap, SerializeSeq},
    Deserialize, Deserializer, Serialize, Serializer,
};

#[derive(Clone, Debug, PartialEq)]
pub enum OValue {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<OValue>),
    Object(Vec<(String, OValue)>),
}
impl OValue {
    pub fn obj() -> Self {
        Self::Object(Vec::new())
    }
    pub fn str(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }
    pub fn get(&self, key: &str) -> Option<&OValue> {
        match self {
            Self::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&Vec<OValue>> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }
    /// `d[key] = value`: keeps the position of an existing key.
    pub fn insert(&mut self, key: &str, value: OValue) {
        if let Self::Object(m) = self {
            match m.iter_mut().find(|(k, _)| k == key) {
                Some((_, v)) => *v = value,
                None => m.push((key.to_string(), value)),
            }
        }
    }
    pub fn remove(&mut self, key: &str) {
        if let Self::Object(m) = self {
            m.retain(|(k, _)| k != key);
        }
    }
    /// Python `str(value)`-ish text used for the managed-marker check.
    pub fn text(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Null => String::new(),
            other => serde_json::to_string(other).unwrap_or_default(),
        }
    }
}

impl Serialize for OValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => s.serialize_unit(),
            Self::Bool(b) => s.serialize_bool(*b),
            Self::Number(n) => n.serialize(s),
            Self::String(v) => s.serialize_str(v),
            Self::Array(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            Self::Object(m) => {
                let mut map = s.serialize_map(Some(m.len()))?;
                for (k, v) in m {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}
impl<'de> Deserialize<'de> for OValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = OValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> Result<OValue, E> {
                Ok(OValue::Null)
            }
            fn visit_none<E>(self) -> Result<OValue, E> {
                Ok(OValue::Null)
            }
            fn visit_bool<E>(self, b: bool) -> Result<OValue, E> {
                Ok(OValue::Bool(b))
            }
            fn visit_i64<E>(self, n: i64) -> Result<OValue, E> {
                Ok(OValue::Number(n.into()))
            }
            fn visit_u64<E>(self, n: u64) -> Result<OValue, E> {
                Ok(OValue::Number(n.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, n: f64) -> Result<OValue, E> {
                serde_json::Number::from_f64(n)
                    .map(OValue::Number)
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E>(self, s: &str) -> Result<OValue, E> {
                Ok(OValue::String(s.to_string()))
            }
            fn visit_string<E>(self, s: String) -> Result<OValue, E> {
                Ok(OValue::String(s))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<OValue, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element()? {
                    out.push(v);
                }
                Ok(OValue::Array(out))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<OValue, A::Error> {
                let mut out: Vec<(String, OValue)> = Vec::new();
                while let Some((k, v)) = a.next_entry::<String, OValue>()? {
                    // Duplicate keys: last wins, first position kept (like json.loads).
                    match out.iter_mut().find(|(ok, _)| *ok == k) {
                        Some((_, ov)) => *ov = v,
                        None => out.push((k, v)),
                    }
                }
                Ok(OValue::Object(out))
            }
        }
        d.deserialize_any(V)
    }
}

/// Indentation of the first indented line (default two spaces, like json.dumps).
pub fn detect_indent(text: &str) -> String {
    text.lines()
        .skip(1)
        .find_map(|l| {
            let ws: String = l.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            (!ws.is_empty()).then_some(ws)
        })
        .unwrap_or_else(|| "  ".into())
}
/// Pretty-print with `indent`, matching json.dumps(indent=N, ensure_ascii=False).
pub fn to_pretty(value: &OValue, indent: &str, trailing_newline: bool) -> String {
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    value.serialize(&mut ser).expect("in-memory JSON");
    let mut s = String::from_utf8(buf).expect("utf8 JSON");
    if trailing_newline {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_keeps_order_and_bytes() {
        let text = "{\n  \"z\": 1,\n  \"a\": [\n    {\n      \"type\": \"command\",\n      \"timeout\": 5\n    }\n  ],\n  \"m\": {},\n  \"e\": [],\n  \"f\": 1.5,\n  \"u\": \"ü\\\"\"\n}\n";
        let v: OValue = serde_json::from_str(text).unwrap();
        assert_eq!(to_pretty(&v, &detect_indent(text), true), text);
        let mut v = v;
        v.insert("a", OValue::Null);
        v.insert("new", OValue::Bool(true));
        let keys: Vec<_> = match &v {
            OValue::Object(m) => m.iter().map(|(k, _)| k.as_str()).collect(),
            _ => vec![],
        };
        assert_eq!(keys, ["z", "a", "m", "e", "f", "u", "new"]);
    }
    #[test]
    fn real_agent_configs_round_trip_byte_identically() {
        // Read-only: never writes the user's files.
        let home = std::env::var("HOME").unwrap_or_default();
        for p in [".claude/settings.json", ".codex/hooks.json"] {
            let Ok(text) = std::fs::read_to_string(std::path::Path::new(&home).join(p)) else {
                continue;
            };
            let v: OValue = serde_json::from_str(&text).unwrap();
            assert_eq!(
                to_pretty(&v, &detect_indent(&text), text.ends_with('\n')),
                text,
                "{p}"
            );
        }
    }
}
