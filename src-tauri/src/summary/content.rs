//! Strict Native content and the separately bounded Legacy compatibility reader.
use crate::intelligence::types::IntelligenceErrorCode as Code;
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use std::{
    fmt,
    io::{self, Write},
    marker::PhantomData,
};

pub(crate) const CONTENT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SummaryContent {
    #[serde(deserialize_with = "summary_string")]
    pub summary: String,
    #[serde(deserialize_with = "strings")]
    pub participants: Vec<String>,
    #[serde(deserialize_with = "strings")]
    pub companies: Vec<String>,
    #[serde(deserialize_with = "strings")]
    pub requirements: Vec<String>,
    #[serde(deserialize_with = "actions")]
    pub action_items: Vec<ActionItem>,
    #[serde(deserialize_with = "strings")]
    pub risks: Vec<String>,
    #[serde(deserialize_with = "terms")]
    pub terms: Vec<Term>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionItem {
    #[serde(deserialize_with = "short_string")]
    pub text: String,
    #[serde(default, deserialize_with = "optional_short")]
    pub owner: Option<String>,
    #[serde(default, deserialize_with = "optional_short")]
    pub due: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    #[serde(deserialize_with = "short_string")]
    pub term: String,
    #[serde(deserialize_with = "short_string")]
    pub meaning: String,
}
impl SummaryContent {
    pub(crate) fn parse(raw: &str) -> Result<Self, Code> {
        if raw.len() > CONTENT_BYTES {
            return Err(Code::ResponseTooLarge);
        }
        serde_json::from_str(raw).map_err(|_| Code::InvalidResponse)
    }
}
fn string<'de, D: de::Deserializer<'de>, const N: usize>(d: D) -> Result<String, D::Error> {
    struct Bounded<const N: usize>;
    impl<'de, const N: usize> Visitor<'de> for Bounded<N> {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded text")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            if v.chars().take(N + 1).count() > N {
                return Err(E::custom("summary string limit"));
            }
            Ok(v.to_owned())
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<String, E> {
            if v.chars().take(N + 1).count() > N {
                return Err(E::custom("summary string limit"));
            }
            Ok(v)
        }
    }
    d.deserialize_str(Bounded::<N>)
}
fn summary_string<'de, D: de::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    string::<D, 20000>(d)
}
fn short_string<'de, D: de::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    string::<D, 2000>(d)
}
struct Short(String);
impl<'de> Deserialize<'de> for Short {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        short_string(d).map(Self)
    }
}
fn optional_short<'de, D: de::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::<Short>::deserialize(d).map(|v| v.map(|v| v.0))
}
struct BoundedArray<T>(Vec<T>);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for BoundedArray<T> {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Array<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Array<T> {
            type Value = BoundedArray<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("array with at most 100 complete items")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                loop {
                    if out.len() == 100 {
                        if a.next_element::<de::IgnoredAny>()?.is_some() {
                            return Err(de::Error::custom("summary array limit"));
                        }
                        break;
                    }
                    match a.next_element()? {
                        Some(v) => out.push(v),
                        None => break,
                    }
                }
                Ok(BoundedArray(out))
            }
        }
        d.deserialize_seq(Array(PhantomData))
    }
}
fn strings<'de, D: de::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    BoundedArray::<Short>::deserialize(d).map(|v| v.0.into_iter().map(|v| v.0).collect())
}
fn actions<'de, D: de::Deserializer<'de>>(d: D) -> Result<Vec<ActionItem>, D::Error> {
    BoundedArray::<ActionItem>::deserialize(d).map(|v| v.0)
}
fn terms<'de, D: de::Deserializer<'de>>(d: D) -> Result<Vec<Term>, D::Error> {
    BoundedArray::<Term>::deserialize(d).map(|v| v.0)
}

pub(crate) struct JsonCount {
    bytes: usize,
    limit: usize,
}
impl JsonCount {
    pub(crate) fn new(limit: usize) -> Self {
        Self { bytes: 0, limit }
    }
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
}
impl Write for JsonCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| io::Error::other("JSON limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn json_size(value: &impl Serialize, limit: usize) -> Result<usize, Code> {
    let mut writer = JsonCount::new(limit);
    serde_json::to_writer(&mut writer, value).map_err(|_| Code::ResponseTooLarge)?;
    Ok(writer.bytes())
}
pub(crate) fn valid_hash(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Default)]
struct LegacyBudget {
    nodes: usize,
    decoded: usize,
}
impl LegacyBudget {
    fn node<E: de::Error>(&mut self, depth: usize) -> Result<(), E> {
        if depth > 64 || self.nodes >= 100000 {
            return Err(E::custom("legacy JSON limit"));
        }
        self.nodes += 1;
        Ok(())
    }
    fn text<E: de::Error>(&mut self, text: &str) -> Result<(), E> {
        self.decoded = self
            .decoded
            .checked_add(text.len())
            .filter(|n| *n <= CONTENT_BYTES)
            .ok_or_else(|| E::custom("legacy JSON limit"))?;
        Ok(())
    }
}
struct LegacySeed<'a> {
    budget: &'a mut LegacyBudget,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for LegacySeed<'_> {
    type Value = serde_json::Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.budget.node(self.depth)?;
        d.deserialize_any(LegacyVisitor {
            budget: self.budget,
            depth: self.depth,
        })
    }
}
struct LegacyKey<'a>(&'a mut LegacyBudget);
impl<'de> DeserializeSeed<'de> for LegacyKey<'_> {
    type Value = String;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<String, D::Error> {
        struct Key<'a>(&'a mut LegacyBudget);
        impl<'de> Visitor<'de> for Key<'_> {
            type Value = String;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded legacy key")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
                self.0.node(1)?;
                self.0.text(v)?;
                Ok(v.to_owned())
            }
        }
        d.deserialize_str(Key(self.0))
    }
}
struct LegacyVisitor<'a> {
    budget: &'a mut LegacyBudget,
    depth: usize,
}
impl<'de> Visitor<'de> for LegacyVisitor<'_> {
    type Value = serde_json::Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded legacy JSON")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(v.into())
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(v)
            .map(serde_json::Value::Number)
            .ok_or_else(|| E::custom("invalid number"))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.budget.text(v)?;
        Ok(v.into())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = a.next_element_seed(LegacySeed {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            out.push(v);
        }
        Ok(serde_json::Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut out = serde_json::Map::new();
        while let Some(key) = a.next_key_seed(LegacyKey(self.budget))? {
            let normalized: String = key
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            if reserved_raw(&normalized) {
                return Err(de::Error::custom("reserved summary field"));
            }
            let value = a.next_value_seed(LegacySeed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            if matches!(
                normalized.as_str(),
                "inputhash" | "responsehash" | "rawresponsehash"
            ) && !value.as_str().is_some_and(valid_hash)
            {
                return Err(de::Error::custom("invalid summary hash"));
            }
            if !sensitive(&normalized) {
                out.insert(key, value);
            }
        }
        Ok(serde_json::Value::Object(out))
    }
}
fn reserved_raw(key: &str) -> bool {
    matches!(
        key,
        "rawtext"
            | "rawresponse"
            | "rawresponses"
            | "rawresponsebody"
            | "rawresponsecontent"
            | "responsebody"
            | "jobpayload"
            | "jobpayloadhash"
    )
}
fn sensitive(key: &str) -> bool {
    key.contains("secret")
        || key.contains("token")
        || key.contains("password")
        || key.contains("passphrase")
        || key.contains("passcode")
        || matches!(key, "passwd" | "pwd" | "key" | "auth")
        || [
            "apikey",
            "accesskey",
            "clientkey",
            "privatekey",
            "signingkey",
            "encryptionkey",
            "credential",
            "cookie",
            "sessionid",
            "authorization",
            "bearer",
        ]
        .iter()
        .any(|part| key.contains(part))
        || key.starts_with("audio")
}
pub(crate) fn legacy_json(raw: &str) -> Result<serde_json::Value, Code> {
    if raw.len() > CONTENT_BYTES {
        return Err(Code::ResponseTooLarge);
    }
    let mut parser = serde_json::Deserializer::from_str(raw);
    let mut budget = LegacyBudget::default();
    let value = LegacySeed {
        budget: &mut budget,
        depth: 1,
    }
    .deserialize(&mut parser)
    .map_err(|_| Code::InvalidResponse)?;
    parser.end().map_err(|_| Code::InvalidResponse)?;
    Ok(value)
}
