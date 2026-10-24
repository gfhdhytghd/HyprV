//! Secret values that are wiped when dropped, and NetworkManager settings maps
//! that hand them to D-Bus serialization by reference, so no unwiped
//! intermediate copy is made on our side.
use serde::{Serialize, Serializer};
use std::collections::BTreeMap;
use zbus::zvariant::{OwnedValue, Signature, Type, Value};
pub use zeroize::{Zeroize, Zeroizing};

/// A secret string, zeroized on drop.
pub type Secret = Zeroizing<String>;
/// Secret key name → value (e.g. `psk`, `password`, `wep-key0`).
pub type Fields = BTreeMap<String, Secret>;

/// One `a{sv}` value: plain settings data or a secret.
pub enum SValue {
    Plain(OwnedValue),
    Secret(Secret),
}
impl SValue {
    pub fn plain(v: impl Into<Value<'static>>) -> Self {
        Self::Plain(OwnedValue::try_from(v.into()).expect("value without fds"))
    }
    pub fn u32(&self) -> Option<u32> {
        match self {
            Self::Plain(v) => u32::try_from(v).ok(),
            Self::Secret(_) => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Self::Plain(v) => <&str>::try_from(v).ok(),
            Self::Secret(s) => Some(s.as_str()),
        }
    }
    pub fn bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Plain(v) => v.try_clone().ok().and_then(|v| Vec::<u8>::try_from(v).ok()),
            Self::Secret(_) => None,
        }
    }
    pub fn bool(&self) -> Option<bool> {
        match self {
            Self::Plain(v) => bool::try_from(v).ok(),
            Self::Secret(_) => None,
        }
    }
}
impl std::fmt::Debug for SValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plain(v) => write!(f, "{v:?}"),
            Self::Secret(_) => f.write_str("<secret>"),
        }
    }
}
impl Serialize for SValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Plain(v) => v.serialize(s),
            // Borrowed: the variant references the zeroizing buffer directly.
            Self::Secret(x) => Value::from(x.as_str()).serialize(s),
        }
    }
}
impl Type for SValue {
    const SIGNATURE: &'static Signature = &Signature::Variant;
}
/// One setting's `a{sv}`.
pub type SProps = BTreeMap<String, SValue>;
/// A connection `a{sa{sv}}` that may carry secrets.
pub type SSettings = BTreeMap<String, SProps>;

/// Copy plain settings (as returned by `GetSettings`) into an `SSettings`.
pub fn from_settings(settings: &crate::bus::Settings) -> SSettings {
    settings
        .iter()
        .map(|(name, props)| {
            (
                name.clone(),
                props
                    .iter()
                    .filter_map(|(k, v)| Some((k.clone(), SValue::Plain(v.try_clone().ok()?))))
                    .collect(),
            )
        })
        .collect()
}

/// A secret returned as a D-Bus `s` (e.g. Agent1.RequestPinCode), serialized
/// by reference and wiped on drop.
pub struct SecretString(pub Secret);
impl Serialize for SecretString {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0.as_str())
    }
}
impl Type for SecretString {
    const SIGNATURE: &'static Signature = &Signature::Str;
}
