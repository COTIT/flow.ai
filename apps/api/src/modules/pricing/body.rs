//! Allocation-bounded parse of the calculate and document (invoice) bodies. Unknown keys and
//! nested values are skipped while parsing, lines past the cap are counted, never kept, and
//! free objects are capped (`Capped`): as `serde_json::Value`, a 2 MiB body of `[0,0,…]`
//! grows to ~32 MiB.

use std::{cell::Cell, fmt};

use serde::{
    Deserialize, Deserializer,
    de::{
        DeserializeSeed, Error, IgnoredAny, MapAccess, SeqAccess, Visitor,
        value::{MapAccessDeserializer, StringDeserializer},
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
        match number_or_map(map)? {
            NumberOrMap::Number(n) => Ok(Scalar::Number(n)),
            NumberOrMap::Map(map) => drain_map(map).map(Scalar::Container),
        }
    }
}

enum NumberOrMap<A> {
    Number(Number),
    Map(FirstKey<A>),
}

// With arbitrary_precision, serde_json delivers numbers through its private map protocol.
fn number_or_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<NumberOrMap<A>, A::Error> {
    let first = map.next_key::<String>()?;
    let number = first.as_deref() == Some("$serde_json::private::Number");
    let map = FirstKey { first, rest: map };
    if number {
        Number::deserialize(MapAccessDeserializer::new(map)).map(NumberOrMap::Number)
    } else {
        Ok(NumberOrMap::Map(map))
    }
}

struct FirstKey<A> {
    first: Option<String>,
    rest: A,
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for FirstKey<A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        match self.first.take() {
            Some(key) => seed
                .deserialize(StringDeserializer::<A::Error>::new(key))
                .map(Some),
            None => self.rest.next_key_seed(seed),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        self.rest.next_value_seed(seed)
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
/// Most bytes of keys, strings and exact numbers a `Capped` keeps.
pub const MAX_BYTES: usize = 16 * 1024;

/// A JSON value kept only while it has at most `MAX_NODES` nodes and `MAX_BYTES` of text; a
/// bigger one is skipped while parsing (never built), so a 2 MiB body can't grow into tens of
/// MiB of `Value`, nor fill every list row.
#[derive(Debug, Default, Clone, PartialEq)]
pub enum Capped {
    #[default]
    Absent,
    Value(Value),
    TooLarge,
}

impl<'de> Deserialize<'de> for Capped {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (nodes, bytes) = (Cell::new(MAX_NODES), Cell::new(MAX_BYTES));
        Ok(
            match (Budget {
                nodes: &nodes,
                bytes: &bytes,
            })
            .deserialize(d)?
            {
                Some(v) => Self::Value(v),
                None => Self::TooLarge,
            },
        )
    }
}

/// Builds a `Value` while the shared budgets last; None (rest skipped) once one runs out.
#[derive(Clone, Copy)]
struct Budget<'a> {
    nodes: &'a Cell<usize>,
    bytes: &'a Cell<usize>,
}

impl Budget<'_> {
    fn spend(&self) -> bool {
        let left = self.nodes.get();
        self.nodes.set(left.saturating_sub(1));
        left > 0
    }

    /// One node of `len` bytes of text; once over, both budgets read as spent.
    fn spend_text(&self, len: usize) -> bool {
        let ok = self.spend() && len <= self.bytes.get();
        if ok {
            self.bytes.set(self.bytes.get() - len);
        } else {
            self.nodes.set(0);
        }
        ok
    }

    fn number(&self, n: Number) -> Option<Value> {
        self.spend_text(n.as_str().len())
            .then_some(Value::Number(n))
    }
}

impl<'de> DeserializeSeed<'de> for Budget<'_> {
    type Value = Option<Value>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<Value>, D::Error> {
        if self.nodes.get() == 0 {
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
        Ok(self.number(v.into()))
    }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(self.number(v.into()))
    }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(match Number::from_f64(v) {
            Some(n) => self.number(n),
            None => self.spend().then_some(Value::Null),
        })
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(self
            .spend_text(v.len())
            .then(|| Value::String(v.to_owned())))
    }
    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        Ok(self.spend().then_some(Value::Null))
    }
    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        Ok(self.spend().then_some(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut out = self.spend().then(Vec::new);
        while let Some(v) = seq.next_element_seed(self)? {
            match (&mut out, v) {
                (Some(list), Some(v)) => list.push(v),
                _ => out = None,
            }
        }
        Ok(out.map(Value::Array))
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        let mut map = match number_or_map(map)? {
            NumberOrMap::Number(n) => return Ok(self.number(n)),
            NumberOrMap::Map(map) => map,
        };
        let mut out = self.spend().then(Map::new);
        while let Some(k) = map.next_key::<String>()? {
            let v = if self.spend_text(k.len()) {
                map.next_value_seed(self)?
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
        match number_or_map(map)? {
            NumberOrMap::Number(_) => Ok(Row::NotObject),
            NumberOrMap::Map(map) => RawItem::deserialize(MapAccessDeserializer::new(map))
                .map(|i| Row::Item(Box::new(i))),
        }
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

    /// Request bodies built from `Scalar`/`Capped`/`Items` keep "absent = default" on every
    /// field (`#[serde(default)]` per field, for utoipa); a field without it makes `{}` fail.
    #[test]
    fn lenient_request_bodies_take_an_empty_object() {
        use crate::modules::{
            email_profiles, email_templates, files, invoice_payments, invoices, messages,
            pdf_templates,
        };
        fn empty<T: serde::de::DeserializeOwned>(name: &str) {
            if let Err(e) = serde_json::from_str::<T>("{}") {
                panic!("{name}: {e}");
            }
        }
        empty::<files::dto::UploadRequest>("UploadRequest");
        empty::<invoice_payments::dto::CreatePaymentRequest>("CreatePaymentRequest");
        empty::<invoice_payments::dto::MarkCreditedRequest>("MarkCreditedRequest");
        empty::<invoices::dto::InvoiceBody>("InvoiceBody");
        empty::<invoices::dto::CreditInvoiceRequest>("CreditInvoiceRequest");
        empty::<messages::dto::SendMessageRequest>("SendMessageRequest");
        empty::<messages::dto::GraphBody>("GraphBody");
        empty::<messages::dto::Office365Request>("Office365Request");
        empty::<email_profiles::dto::ProfileBody>("email ProfileBody");
        empty::<email_templates::dto::TemplateBody>("email TemplateBody");
        empty::<pdf_templates::dto::TemplateBody>("pdf TemplateBody");
    }

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
            (r#"{"v":-0.0}"#, false),
            (r#"{"v":0e5}"#, false),
            (r#"{"v":"0"}"#, false),
            (r#"{"v":""}"#, false),
            (r#"{"v":false}"#, false),
            (r#"{"v":[]}"#, false),
            (r#"{"v":{}}"#, false),
            (r#"{"v":1}"#, true),
            (r#"{"v":1.25e-3}"#, true),
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
    fn exact_numbers_stay_scalar() {
        for number in [
            "0.0",
            "-0.0",
            "1.25e-3",
            "18446744073709551616",
            "123456789012345678901.1234567890123456789",
        ] {
            let parsed = scalar(&format!(r#"{{"v":{number}}}"#));
            let expected: Value = serde_json::from_str(number).unwrap();
            assert!(matches!(parsed, Scalar::Number(_)), "{number}");
            assert_eq!(parsed.filled(), Some(expected.clone()), "{number}");
            assert_eq!(parsed.echo(), expected, "{number}");
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
        // One node, too many bytes; a sibling after the oversized value is still skipped.
        let long = |n: usize| format!(r#"{{"v":{{"a":"{}","b":1}}}}"#, "x".repeat(n));
        // Keys "a" and "b" plus the retained number "1" consume three bytes.
        assert!(matches!(capped(&long(MAX_BYTES - 3)), Capped::Value(_)));
        assert_eq!(capped(&long(MAX_BYTES - 2)), Capped::TooLarge);
        let key = format!(r#"{{"v":{{"{}":1}}}}"#, "k".repeat(MAX_BYTES + 1));
        assert_eq!(capped(&key), Capped::TooLarge);
    }

    #[test]
    fn capped_keeps_exact_numbers_within_budget() {
        let nested = r#"{"a":[0.0,-0.0,1.25e-3,{"b":123456789012345678901.1234567890123456789}]}"#;
        assert_eq!(
            capped(&format!(r#"{{"v":{nested}}}"#)),
            Capped::Value(serde_json::from_str(nested).unwrap())
        );
        let number = |len: usize| format!(r#"{{"v":0.{}}}"#, "1".repeat(len - 2));
        assert!(matches!(
            capped(&number(MAX_BYTES)),
            Capped::Value(Value::Number(_))
        ));
        assert_eq!(capped(&number(MAX_BYTES + 1)), Capped::TooLarge);
        let exhausted = format!(r#"{{"v":[0.{},{{"later":1.25}}]}}"#, "1".repeat(MAX_BYTES));
        assert_eq!(capped(&exhausted), Capped::TooLarge);
    }

    #[test]
    fn capped_counts_cumulative_integer_bytes() {
        for number in ["18446744073709551615", "-9223372036854775808"] {
            let count = MAX_BYTES / number.len();
            let prefix = vec![number; count].join(",");
            let remaining = MAX_BYTES - count * number.len();
            let list = |tail_len| format!(r#"{{"v":[{prefix},{}]}}"#, "1".repeat(tail_len));
            assert!(
                matches!(capped(&list(remaining)), Capped::Value(_)),
                "{number}"
            );
            assert_eq!(capped(&list(remaining + 1)), Capped::TooLarge, "{number}");
            let oversized = format!(r#"{{"v":[{}]}}"#, vec![number; MAX_NODES - 1].join(","));
            assert_eq!(capped(&oversized), Capped::TooLarge, "{number}");
        }
    }

    #[test]
    fn numeric_items_and_rows_keep_their_invalid_shape() {
        for number in ["0.0", "-0.0", "1.25e-3", "18446744073709551616"] {
            assert!(
                matches!(serde_json::from_str::<Items>(number), Ok(Items::NotList)),
                "{number}"
            );
            assert!(
                matches!(serde_json::from_str::<Row>(number), Ok(Row::NotObject)),
                "{number}"
            );
        }
        let Ok(Items::List(rows)) =
            serde_json::from_str::<Items>(r#"[0.0,1.25e-3,{"price":2.50,"quantity":1.25}]"#)
        else {
            panic!("list expected");
        };
        assert!(matches!(rows[0], Row::NotObject));
        assert!(matches!(rows[1], Row::NotObject));
        assert!(matches!(&rows[2], Row::Item(i)
            if i.price.filled() == Some(serde_json::from_str("2.50").unwrap())
                && i.quantity.filled() == Some(serde_json::from_str("1.25").unwrap())));
        for malformed in ["01", "1.", "1e", "--1"] {
            assert!(
                serde_json::from_str::<Scalar>(malformed).is_err(),
                "{malformed}"
            );
            assert!(
                serde_json::from_str::<Capped>(malformed).is_err(),
                "{malformed}"
            );
            assert!(
                serde_json::from_str::<Row>(malformed).is_err(),
                "{malformed}"
            );
            assert!(
                serde_json::from_str::<Items>(malformed).is_err(),
                "{malformed}"
            );
        }
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
