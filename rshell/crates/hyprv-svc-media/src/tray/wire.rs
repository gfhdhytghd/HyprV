//! Decode large SNI byte arrays directly, without constructing a Value per byte.
use super::model::{self, TrayItem, TrayPixmap};
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{collections::HashMap, fmt};
use zbus::zvariant::{DeserializeValue, OwnedValue, Type};

#[derive(Default)]
pub(super) struct Properties {
    fields: HashMap<String, OwnedValue>,
    pixmaps: HashMap<String, Vec<TrayPixmap>>,
    tooltip: Option<(String, String, String)>,
}
impl Type for Properties {
    const SIGNATURE: &'static zbus::zvariant::Signature = <HashMap<String, OwnedValue>>::SIGNATURE;
}
// Borrowing the wire byte array avoids both per-byte Values and temporary copies.
struct Bytes<'a>(&'a [u8]);
impl Type for Bytes<'_> {
    const SIGNATURE: &'static zbus::zvariant::Signature = <Vec<u8>>::SIGNATURE;
}
impl<'de> Deserialize<'de> for Bytes<'de> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Bytes<'de>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("borrowed D-Bus bytes")
            }
            fn visit_borrowed_bytes<E: serde::de::Error>(
                self,
                v: &'de [u8],
            ) -> Result<Self::Value, E> {
                Ok(Bytes(v))
            }
        }
        d.deserialize_bytes(V)
    }
}
// Consume the entire wire array but retain at most the same eight valid
// records as the legacy model, regardless of the advertised array length.
struct Pixmaps<'a>(Vec<(i32, i32, Bytes<'a>)>);
impl Type for Pixmaps<'_> {
    const SIGNATURE: &'static zbus::zvariant::Signature = <Vec<(i32, i32, Bytes<'_>)>>::SIGNATURE;
}
impl<'de> Deserialize<'de> for Pixmaps<'de> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Pixmaps<'de>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("SNI pixmaps")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some((w, h, b)) = a.next_element::<(i32, i32, Bytes<'de>)>()? {
                    if out.len() < 8
                        && w > 0
                        && h > 0
                        && w <= 512
                        && h <= 512
                        && b.0.len() == w as usize * h as usize * 4
                    {
                        out.push((w, h, b));
                    }
                }
                Ok(Pixmaps(out))
            }
        }
        d.deserialize_seq(V)
    }
}
impl<'de> Deserialize<'de> for Properties {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Properties;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("SNI properties")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut out = Properties::default();
                while let Some(key) = a.next_key::<String>()? {
                    if matches!(
                        key.as_str(),
                        "IconPixmap" | "AttentionIconPixmap" | "OverlayIconPixmap"
                    ) {
                        let wire = a.next_value::<DeserializeValue<Pixmaps<'de>>>()?.0;
                        let pixmaps = wire
                            .0
                            .into_iter()
                            .map(|(w, h, b)| TrayPixmap {
                                width: w as u32,
                                height: h as u32,
                                argb: b.0.to_vec(),
                            })
                            .collect();
                        out.pixmaps.insert(key, pixmaps);
                    } else if key == "ToolTip" {
                        let (icon, _, title, description) = a
                            .next_value::<DeserializeValue<(String, Pixmaps<'de>, String, String)>>(
                            )?
                            .0;
                        out.tooltip = Some((icon, title, description));
                    } else {
                        out.fields.insert(key, a.next_value()?);
                    }
                }
                Ok(out)
            }
        }
        d.deserialize_map(V)
    }
}
impl Properties {
    pub fn decode(message: &zbus::Message) -> zbus::Result<Self> {
        
        // Preserve tolerant legacy handling of wrongly typed optional properties.
        
        message.body().deserialize().or_else(|_| {
            
            message.body().deserialize().map(|fields| Self {
                fields,
                ..Default::default()
            })
        })
    }
    pub fn apply(mut self, item: &mut TrayItem) {
        model::apply_item_props(item, &self.fields);
        if let Some((icon, title, description)) = self.tooltip {
            item.tooltip_icon_name = icon;
            item.tooltip_title = title;
            item.tooltip_description = description;
        }
        if let Some(p) = self.pixmaps.remove("IconPixmap") {
            item.icon_pixmaps = p;
        }
        if let Some(p) = self.pixmaps.remove("AttentionIconPixmap") {
            item.attention_pixmaps = p;
        }
        if let Some(p) = self.pixmaps.remove("OverlayIconPixmap") {
            item.overlay_pixmaps = p;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;
    #[test]
    #[ignore = "manual decode CPU comparison"]
    fn icon_decode_cpu_comparison() {
        let mut fields = HashMap::<String, OwnedValue>::new();
        fields.insert(
            "IconPixmap".into(),
            Value::from(vec![(128i32, 128i32, vec![7u8; 128 * 128 * 4])])
                .try_into()
                .unwrap(),
        );
        let message = zbus::Message::method_return(
            &zbus::Message::method_call("/test", "Test")
                .unwrap()
                .build(&())
                .unwrap()
                .header(),
        )
        .unwrap()
        .build(&fields)
        .unwrap();
        let start = std::time::Instant::now();
        for _ in 0..200 {
            let fields: HashMap<String, OwnedValue> = message.body().deserialize().unwrap();
            let mut item = TrayItem::default();
            model::apply_item_props(&mut item, &fields);
            std::hint::black_box(item);
        }
        let legacy = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..200 {
            let mut item = TrayItem::default();
            Properties::decode(&message).unwrap().apply(&mut item);
            std::hint::black_box(item);
        }
        println!(
            "200 x 128px decode: legacy={legacy:?}, typed={:?}",
            start.elapsed()
        );
    }
    #[test]
    fn invalid_records_do_not_consume_retained_pixmap_budget() {
        let mut pix = vec![(0i32, 2i32, vec![]); 1000];
        pix.extend((0..12).map(|v| (1, 1, vec![v as u8; 4])));
        let mut fields = HashMap::<String, OwnedValue>::new();
        fields.insert("IconPixmap".into(), Value::from(pix).try_into().unwrap());
        let message = zbus::Message::method_return(
            &zbus::Message::method_call("/test", "Test")
                .unwrap()
                .build(&())
                .unwrap()
                .header(),
        )
        .unwrap()
        .build(&fields)
        .unwrap();
        let mut actual = TrayItem::default();
        Properties::decode(&message).unwrap().apply(&mut actual);
        let mut expected = TrayItem::default();
        model::apply_item_props(&mut expected, &fields);
        assert_eq!(actual, expected);
        assert_eq!(actual.icon_pixmaps.len(), 8);
        assert_eq!(actual.icon_pixmaps[7].argb, vec![7; 4]);
    }
    #[test]
    fn typed_wire_matches_legacy_and_malformed_fallback() {
        for malformed in [false, true] {
            let mut fields = HashMap::<String, OwnedValue>::new();
            fields.insert("Id".into(), Value::from("test").try_into().unwrap());
            let pix = if malformed {
                Value::from("wrong type")
            } else {
                Value::from(vec![(2i32, 2i32, vec![7u8; 16]), (512, 513, vec![])])
            };
            fields.insert("IconPixmap".into(), pix.try_into().unwrap());
            fields.insert(
                "ToolTip".into(),
                Value::from((
                    "tip-icon",
                    vec![(128i32, 128i32, vec![1u8; 128 * 128 * 4])],
                    "title",
                    "description",
                ))
                .try_into()
                .unwrap(),
            );
            let message = zbus::Message::method_return(
                &zbus::Message::method_call("/test", "Test")
                    .unwrap()
                    .build(&())
                    .unwrap()
                    .header(),
            )
            .unwrap()
            .build(&fields)
            .unwrap();
            let mut expected = TrayItem::default();
            model::apply_item_props(&mut expected, &fields);
            let mut actual = TrayItem::default();
            Properties::decode(&message).unwrap().apply(&mut actual);
            assert_eq!(actual, expected);
        }
    }
}
