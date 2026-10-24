//! `gsettings set` without spawning: send a serialized dconf changeset
//! (`a{smv}`, GVariant little-endian normal form) to
//! `ca.desrt.dconf.Writer.Change` on `/ca/desrt/dconf/Writer/user`.
use crate::ServiceError;
use std::collections::HashMap;
use zvariant::{serialized::Context, OwnedValue, Value, LE};

pub fn changeset(entries: &[(&str, Value<'_>)]) -> Result<Vec<u8>, ServiceError> {
    let map: HashMap<String, Option<OwnedValue>> = entries
        .iter()
        .map(|(k, v)| {
            let owned = OwnedValue::try_from(
                v.try_clone()
                    .map_err(|e| ServiceError::new("dconf", e.to_string()))?,
            )
            .map_err(|e| ServiceError::new("dconf", e.to_string()))?;
            Ok(((*k).to_string(), Some(owned)))
        })
        .collect::<Result<_, ServiceError>>()?;
    // zvariant 5 still ships GVariant encoding (deprecated for zvariant 6).
    #[allow(deprecated)]
    let ctxt = Context::new_gvariant(LE, 0);
    let data =
        zvariant::to_bytes(ctxt, &map).map_err(|e| ServiceError::new("dconf", e.to_string()))?;
    Ok(data.to_vec())
}
pub async fn write(entries: &[(&str, Value<'_>)]) -> Result<(), ServiceError> {
    let blob = changeset(entries)?;
    let conn = zbus::Connection::session().await?;
    conn.call_method(
        Some("ca.desrt.dconf"),
        "/ca/desrt/dconf/Writer/user",
        Some("ca.desrt.dconf.Writer"),
        "Change",
        &(blob,),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gvariant_changeset_bytes() {
        // a{smv} with one entry {"/a/b": just <'x'>}, as produced by GLib.
        let blob = changeset(&[("/a/b", Value::from("x"))]).unwrap();
        // key "/a/b\0" (5) + maybe(variant 'x\0' + \0 + 's') + framing offsets.
        assert!(blob.starts_with(b"/a/b\0"));
        assert!(blob.windows(3).any(|w| w == b"x\0\0"));
        assert!(blob.contains(&b's'));
    }
}
#[cfg(test)]
mod glib_bytes {
    #[test]
    fn matches_glib_serialization() {
        // Reference bytes from GLib.Variant('a{smv}', {...}).get_data_as_bytes().
        let s = super::changeset(&[("/a/b", zvariant::Value::from("x"))]).unwrap();
        assert_eq!(s, [47, 97, 47, 98, 0, 0, 0, 0, 120, 0, 0, 115, 0, 5, 14]);
        let i = super::changeset(&[(
            "/org/gnome/desktop/interface/cursor-size",
            zvariant::Value::I32(24),
        )])
        .unwrap();
        assert_eq!(
            &i[40..],
            [0, 0, 0, 0, 0, 0, 0, 0, 24, 0, 0, 0, 0, 105, 0, 41, 56]
        );
    }
}
