//! Allocation-bounded parse of the calculate body. Unknown keys and nested values are skipped
//! while parsing, and lines past the cap are counted, never kept: as `serde_json::Value`, a
//! 2 MiB body of `[0,0,…]` grows to ~32 MiB.

use std::{cell::Cell, fmt};

use serde::{
    Deserialize, Deserializer,
    de::{
        DeserializeSeed, Error, IgnoredAny, MapAccess, SeqAccess, Visitor,
        value::MapAccessDeserializer,
    },
};
use serde_json::{Map, Number, Value};

/// Most lines one request may carry. Laravel had no cap; a document never comes close.
pub const MAX_ITEMS: usize = 1000;

/// One JSON value with arrays and objects reduced to whether they had entries.
#[derive(Debug, Default, Clone, PartialEq)]
pub enum Scalar {
    /// Key not sent.
    #[default]
    Absent,
    Null,
    Bool(bool),
    Number(Number),
    Str(String),
    /// Array or object, dropped while parsing; true when it had entries.
    Container(bool),
}

impl Scalar {
    /// Value for `Rules`: absent, null and blank text are missing; a container becomes `[]`,
    /// which fails every scalar rule.
    pub fn filled(&self) -> Option<Value> {
        match self {
            Self::Absent | Self::Null => None,
            Self::Str(s) if s.trim().is_empty() => None,
            Self::Str(s) => Some(Value::String(s.clone())),
            Self::Bool(b) => Some(Value::Bool(*b)),
            Self::Number(n) => Some(Value::Number(n.clone())),
            Self::Container(_) => Some(Value::Array(Vec::new())),
        }
    }

    /// Echo form: scalars as sent, absent and containers as null.
    pub fn echo(self) -> Value {
        match self {
            Self::Bool(b) => Value::Bool(b),
            Self::Number(n) => Value::Number(n),
            Self::Str(s) => Value::String(s),
            Self::Absent | Self::Null | Self::Container(_) => Value::Null,
        }
    }

    /// PHP truthiness after `json_decode($body, true)` (`{}` and `[]` are empty arrays).
    pub fn truthy(&self) -> bool {
        match self {
            Self::Absent | Self::Null => false,
            Self::Bool(b) => *b,
            Self::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
            Self::Str(s) => !s.is_empty() && s != "0",
            Self::Container(any) => *any,
        }
    }
}

impl<'de> Deserialize<'de> for Scalar {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(ScalarVisitor)
    }
}

struct ScalarVisitor;

impl<'de> Visitor<'de> for ScalarVisitor {
    type Value = Scalar;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_bool<E: Error>(self, v: bool) -> Result<Scalar, E> {
        Ok(Scalar::Bool(v))
    }
    fn visit_i64<E: Error>(self, v: i64) -> Result<Scalar, E> {
        Ok(Scalar::Number(v.into()))
    }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Scalar, E> {
        Ok(Scalar::Number(v.into()))
    }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Scalar, E> {
        // JSON numbers are finite, so from_f64 always succeeds.
        Ok(Number::from_f64(v).map_or(Scalar::Null, Scalar::Number))
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Scalar, E> {
        Ok(Scalar::Str(v.to_owned()))
    }
    fn visit_string<E: Error>(self, v: String) -> Result<Scalar, E> {
        Ok(Scalar::Str(v))
    }
    fn visit_unit<E: Error>(self) -> Result<Scalar, E> {
        Ok(Scalar::Null)
    }
    fn visit_none<E: Error>(self) -> Result<Scalar, E> {
        Ok(Scalar::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Scalar, A::Error> {
        drain_seq(seq).map(Scalar::Container)
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Scalar, A::Error> {
        drain_map(map).map(Scalar::Container)
    }
}

/// Skip a sequence without keeping it; true when it had entries.
fn drain_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> Result<bool, A::Error> {
    let mut any = false;
    while seq.next_element::<IgnoredAny>()?.is_some() {
        any = true;
    }
    Ok(any)
}

/// Skip a map without keeping it; true when it had entries.
fn drain_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<bool, A::Error> {
    let mut any = false;
    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {
        any = true;
    }
    Ok(any)
}

/// One item row (calculate requests and stored document lines); other keys are skipped.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawItem {
    pub id: Scalar,
    pub title: Scalar,
    pub description: Scalar,
    pub quantity: Scalar,
    pub price: Scalar,
    pub vat: Scalar,
    #[serde(rename = "productDiscount")]
    pub product_discount: Scalar,
    pub purchase_price: Scalar,
    pub product_id: Scalar,
    pub multi_set_item_id: Scalar,
    pub client_default_discount: Scalar,
    pub position: Scalar,
    pub deleted: Scalar,
    pub parent_deleted: Scalar,
    pub product_variant_id: Scalar,
    pub mutate_from_warehouse_id: Scalar,
    pub multi_set_product_id: Scalar,
}

/// Most nodes (values, keys included) a `Capped` keeps.
pub const MAX_NODES: usize = 1000;

/// A JSON value kept only while it has at most `MAX_NODES` nodes; a bigger one is skipped
/// while parsing (never built), so a 2 MiB body can't grow into tens of MiB of `Value`.
#[derive(Debug, Default, Clone, PartialEq)]
pub enum Capped {
    #[default]
    Absent,
    Value(Value),
    TooLarge,
}

impl<'de> Deserialize<'de> for Capped {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let budget = Cell::new(MAX_NODES);
        Ok(match Budget(&budget).deserialize(d)? {
            Some(v) => Self::Value(v),
            None => Self::TooLarge,
        })
    }
}

/// Builds a `Value` while the shared budget lasts; None (rest skipped) once it runs out.
struct Budget<'a>(&'a Cell<usize>);

impl Budget<'_> {
    fn spend(&self) -> bool {
        let left = self.0.get();
        self.0.set(left.saturating_sub(1));
        left > 0
    }
}

impl<'de> DeserializeSeed<'de> for Budget<'_> {
    type Value = Option<Value>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<Value>, D::Error> {
        if self.0.get() == 0 {
            IgnoredAny::deserialize(d)?;
            return Ok(None);
        }
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Budget<'_> {
    type Value = Option<Value>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_bool<E: Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(self.spend().then_some(Value::Bool(v)))
    }
    fn visit_i64<E: Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(self.spend().then(|| Value::from(v)))
    }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(self.spend().then(|| Value::from(v)))
    }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(self
            .spend()
            .then(|| Number::from_f64(v).map_or(Value::Null, Value::Number)))
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(self.spend().then(|| Value::String(v.to_owned())))
    }
    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        Ok(self.spend().then_some(Value::Null))
    }
    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        Ok(self.spend().then_some(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut out = self.spend().then(Vec::new);
        while let Some(v) = seq.next_element_seed(Budget(self.0))? {
            match (&mut out, v) {
                (Some(list), Some(v)) => list.push(v),
                _ => out = None,
            }
        }
        Ok(out.map(Value::Array))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut out = self.spend().then(Map::new);
        while let Some(k) = map.next_key::<String>()? {
            let v = if self.spend() {
                map.next_value_seed(Budget(self.0))?
            } else {
                map.next_value::<IgnoredAny>()?;
                None
            };
            match (&mut out, v) {
                (Some(obj), Some(v)) => {
                    obj.insert(k, v);
                }
                _ => out = None,
            }
        }
        Ok(out.map(Value::Object))
    }
}

/// One entry of `items`.
#[derive(Debug)]
pub enum Row {
    Item(Box<RawItem>),
    NotObject,
}

impl<'de> Deserialize<'de> for Row {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(RowVisitor)
    }
}

struct RowVisitor;

impl<'de> Visitor<'de> for RowVisitor {
    type Value = Row;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an item")
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Row, A::Error> {
        RawItem::deserialize(MapAccessDeserializer::new(map)).map(|i| Row::Item(Box::new(i)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Row, A::Error> {
        drain_seq(seq).map(|_| Row::NotObject)
    }
    fn visit_bool<E: Error>(self, _: bool) -> Result<Row, E> {
        Ok(Row::NotObject)
    }
    fn visit_i64<E: Error>(self, _: i64) -> Result<Row, E> {
        Ok(Row::NotObject)
    }
    fn visit_u64<E: Error>(self, _: u64) -> Result<Row, E> {
        Ok(Row::NotObject)
    }
    fn visit_f64<E: Error>(self, _: f64) -> Result<Row, E> {
        Ok(Row::NotObject)
    }
    fn visit_str<E: Error>(self, _: &str) -> Result<Row, E> {
        Ok(Row::NotObject)
    }
    fn visit_unit<E: Error>(self) -> Result<Row, E> {
        Ok(Row::NotObject)
    }
    fn visit_none<E: Error>(self) -> Result<Row, E> {
        Ok(Row::NotObject)
    }
}

/// The `items` value.
#[derive(Debug, Default)]
pub enum Items {
    /// Absent, null or blank text: "required".
    #[default]
    Missing,
    NotList,
    /// More than `MAX_ITEMS` entries (the extra ones were skipped, not kept).
    TooMany,
    List(Vec<Row>),
}

impl<'de> Deserialize<'de> for Items {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(ItemsVisitor)
    }
}

struct ItemsVisitor;

impl<'de> Visitor<'de> for ItemsVisitor {
    type Value = Items;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a list of items")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Items, A::Error> {
        let mut rows = Vec::new();
        while rows.len() < MAX_ITEMS {
            match seq.next_element::<Row>()? {
                Some(row) => rows.push(row),
                None => return Ok(Items::List(rows)),
            }
        }
        Ok(if drain_seq(seq)? {
            Items::TooMany
        } else {
            Items::List(rows)
        })
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Items, A::Error> {
        drain_map(map).map(|_| Items::NotList)
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Items, E> {
        Ok(if v.trim().is_empty() {
            Items::Missing
        } else {
            Items::NotList
        })
    }
    fn visit_unit<E: Error>(self) -> Result<Items, E> {
        Ok(Items::Missing)
    }
    fn visit_none<E: Error>(self) -> Result<Items, E> {
        Ok(Items::Missing)
    }
    fn visit_bool<E: Error>(self, _: bool) -> Result<Items, E> {
        Ok(Items::NotList)
    }
    fn visit_i64<E: Error>(self, _: i64) -> Result<Items, E> {
        Ok(Items::NotList)
    }
    fn visit_u64<E: Error>(self, _: u64) -> Result<Items, E> {
        Ok(Items::NotList)
    }
    fn visit_f64<E: Error>(self, _: f64) -> Result<Items, E> {
        Ok(Items::NotList)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Probe {
        #[serde(default)]
        v: Scalar,
    }

    fn scalar(json: &str) -> Scalar {
        serde_json::from_str::<Probe>(json).unwrap().v
    }

    #[test]
    fn php_truthiness() {
        for (json, truthy) in [
            ("{}", false),
            (r#"{"v":null}"#, false),
            (r#"{"v":0}"#, false),
            (r#"{"v":0.0}"#, false),
            (r#"{"v":"0"}"#, false),
            (r#"{"v":""}"#, false),
            (r#"{"v":false}"#, false),
            (r#"{"v":[]}"#, false),
            (r#"{"v":{}}"#, false),
            (r#"{"v":1}"#, true),
            (r#"{"v":"00"}"#, true),
            (r#"{"v":" "}"#, true),
            (r#"{"v":true}"#, true),
            (r#"{"v":[0]}"#, true),
            (r#"{"v":{"a":1}}"#, true),
        ] {
            assert_eq!(scalar(json).truthy(), truthy, "{json}");
        }
    }

    #[test]
    fn containers_are_dropped() {
        assert_eq!(
            scalar(r#"{"v":[[1,[2]],{"a":[3]}]}"#),
            Scalar::Container(true)
        );
        assert_eq!(scalar(r#"{"v":"x"}"#).echo(), Value::String("x".into()));
        assert_eq!(scalar(r#"{"v":{"a":1}}"#).echo(), Value::Null);
    }

    #[test]
    fn items_cap_skips_the_rest() {
        let n = MAX_ITEMS + 5;
        let body = format!("[{}]", vec!["{}"; n].join(","));
        assert!(matches!(
            serde_json::from_str::<Items>(&body),
            Ok(Items::TooMany)
        ));
        let body = format!("[{}]", vec!["{}"; MAX_ITEMS].join(","));
        assert!(
            matches!(serde_json::from_str::<Items>(&body), Ok(Items::List(r)) if r.len() == MAX_ITEMS)
        );
    }

    #[derive(Deserialize)]
    struct CappedProbe {
        #[serde(default)]
        v: Capped,
    }

    fn capped(json: &str) -> Capped {
        serde_json::from_str::<CappedProbe>(json).unwrap().v
    }

    #[test]
    fn capped_keeps_small_values_and_skips_big_ones() {
        assert_eq!(capped("{}"), Capped::Absent);
        assert_eq!(capped(r#"{"v":null}"#), Capped::Value(Value::Null));
        let small = r#"{"v":{"a":[1,2,{"b":"c"}],"d":true}}"#;
        assert_eq!(
            capped(small),
            Capped::Value(serde_json::json!({"a":[1,2,{"b":"c"}],"d":true}))
        );
        let list = |n: usize| format!(r#"{{"v":[{}]}}"#, vec!["0"; n].join(","));
        assert!(matches!(capped(&list(MAX_NODES - 1)), Capped::Value(_)));
        assert_eq!(capped(&list(MAX_NODES)), Capped::TooLarge);
        let wide = format!(
            r#"{{"v":{{{}}}}}"#,
            (0..MAX_NODES)
                .map(|i| format!(r#""k{i}":0"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(capped(&wide), Capped::TooLarge);
        let deep = format!(r#"{{"v":{}1{}}}"#, "[".repeat(100), "]".repeat(100));
        assert!(matches!(capped(&deep), Capped::Value(_)));
    }

    #[test]
    fn non_lists_and_rows() {
        assert!(matches!(
            serde_json::from_str::<Items>(r#""  ""#),
            Ok(Items::Missing)
        ));
        assert!(matches!(
            serde_json::from_str::<Items>("null"),
            Ok(Items::Missing)
        ));
        assert!(matches!(
            serde_json::from_str::<Items>("5"),
            Ok(Items::NotList)
        ));
        assert!(matches!(
            serde_json::from_str::<Items>(r#"{"a":1}"#),
            Ok(Items::NotList)
        ));
        let Ok(Items::List(rows)) =
            serde_json::from_str::<Items>(r#"[5,[1],{"price":"2","x":[9]}]"#)
        else {
            panic!("list expected");
        };
        assert!(matches!(rows[0], Row::NotObject));
        assert!(matches!(rows[1], Row::NotObject));
        assert!(matches!(&rows[2], Row::Item(i) if i.price == Scalar::Str("2".into())));
    }
}
