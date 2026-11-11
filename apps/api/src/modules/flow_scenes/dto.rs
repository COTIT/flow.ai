//! Your Flow vocabulary (Rust is the only authority), built-in defaults, request/response
//! bodies. Config holds display choices only: component kind, metric key, period. No values.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{PartialSchema, ToSchema, openapi::RefOr, openapi::schema::Schema};

use crate::{
    error::AppError,
    http::json::{Rules, Validate},
    openapi::string_enum,
};

/// Built-in scenes, default order.
pub const SCENES: &[&str] = &[
    "pulse", "stock", "noticed", "orders", "sales", "routines", "studio",
];
/// Prefixes a saved custom scene key starts with.
pub const CUSTOM_PREFIXES: &[&str] = &["sales_", "stock_"];
/// Component kinds Flow can render.
pub const TYPES: &[&str] = &[
    "metric_card",
    "line_chart",
    "bar_chart",
    "comparison",
    "order_list",
    "stock_list",
    "attention_panel",
    "invoice_table",
    "report_list",
];
/// Kinds that show one metric over one period.
const METRIC_TYPES: &[&str] = &["metric_card", "line_chart", "bar_chart", "comparison"];
/// Metric keys; the API computes each value, the config only names it.
pub const METRICS: &[&str] = &[
    "sales.gross",
    "sales.net",
    "orders.pending",
    "orders.draft",
    "stock.below_minimum",
    "invoices.overdue",
    "invoices.unpaid",
];
/// Periods a component may show.
pub const PERIODS: &[&str] = &[
    "today",
    "yesterday",
    "this_week",
    "last_week",
    "last_7_days",
    "this_month",
];
/// Most components in one scene.
pub const MAX_COMPONENTS: usize = 8;
/// Most bytes of serialized config.
pub const MAX_CONFIG_BYTES: usize = 8192;
/// Highest position (20 rows at most).
pub const MAX_POSITION: i64 = 19;
/// Where a new custom scene lands when no position is sent: after the built-ins.
pub const CUSTOM_POSITION: i16 = 7;

/// Component kind. `metric_card`, `line_chart`, `bar_chart`, `comparison` need `metric` and
/// `period`; list kinds take no `metric`.
pub struct FlowComponentType;
impl PartialSchema for FlowComponentType {
    fn schema() -> RefOr<Schema> {
        string_enum(
            TYPES.iter().copied(),
            "Component kind. `metric_card`, `line_chart`, `bar_chart`, `comparison` need `metric` and `period`; list kinds take no `metric`.",
        )
    }
}
impl ToSchema for FlowComponentType {}

/// Metric key; each declares its read permission.
pub struct FlowMetric;
impl PartialSchema for FlowMetric {
    fn schema() -> RefOr<Schema> {
        string_enum(
            METRICS.iter().copied(),
            "Metric key. sales.* and orders.* need `order.list`, invoices.* `invoice.list`, stock.* `stock.list`.",
        )
    }
}
impl ToSchema for FlowMetric {}

/// Period a component shows.
pub struct FlowPeriod;
impl PartialSchema for FlowPeriod {
    fn schema() -> RefOr<Schema> {
        string_enum(PERIODS.iter().copied(), "Period a component shows.")
    }
}
impl ToSchema for FlowPeriod {}

/// One component of a scene: what to show, never the shown values.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowComponent {
    /// Unique in its scene, `^[a-z0-9-]{1,32}$`.
    #[schema(example = "sales-today")]
    pub id: String,
    #[serde(rename = "type")]
    #[schema(value_type = FlowComponentType)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<FlowMetric>)]
    pub metric: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<FlowPeriod>)]
    pub period: Option<String>,
}

/// A scene's display config.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowSceneConfig {
    /// In display order, at most 8.
    pub components: Vec<FlowComponent>,
}

impl FlowComponent {
    /// Same component for the "adds a restricted one" check: id, kind and permission area;
    /// metric (inside its area) and period may change freely.
    pub fn same(&self, other: &Self) -> bool {
        self.id == other.id && self.kind == other.kind && self.permission() == other.permission()
    }

    /// Permission needed to read what this component shows; None = any member.
    pub fn permission(&self) -> Option<&'static str> {
        let area = self.metric.as_deref().map_or(self.kind.as_str(), |m| {
            m.split_once('.').map_or(m, |(a, _)| a)
        });
        match area {
            "sales" | "orders" | "order_list" => Some("order.list"),
            "invoices" | "invoice_table" => Some("invoice.list"),
            "stock" | "stock_list" => Some("stock.list"),
            _ => None,
        }
    }
}

fn c(id: &str, kind: &str, metric: Option<&str>, period: Option<&str>) -> FlowComponent {
    FlowComponent {
        id: id.into(),
        kind: kind.into(),
        metric: metric.map(Into::into),
        period: period.map(Into::into),
    }
}

/// Built-in scene's default position and config; None for custom keys.
pub fn default(scene: &str) -> Option<(i16, FlowSceneConfig)> {
    let position = SCENES.iter().position(|s| *s == scene)?;
    let card = |id, m, p| c(id, "metric_card", Some(m), Some(p));
    let components = match scene {
        "pulse" => vec![
            card("sales-today", "sales.gross", "today"),
            card("orders-pending", "orders.pending", "today"),
            c(
                "sales-trend",
                "line_chart",
                Some("sales.gross"),
                Some("last_7_days"),
            ),
            c("attention", "attention_panel", None, None),
        ],
        "stock" => vec![
            card("below-minimum", "stock.below_minimum", "today"),
            c("stock-list", "stock_list", None, None),
        ],
        "noticed" => vec![c("attention", "attention_panel", None, None)],
        "orders" => vec![
            card("orders-pending", "orders.pending", "today"),
            card("orders-draft", "orders.draft", "today"),
            c("orders", "order_list", None, None),
        ],
        "sales" => vec![
            card("sales-week", "sales.gross", "this_week"),
            card("net-week", "sales.net", "this_week"),
            c(
                "sales-daily",
                "bar_chart",
                Some("sales.gross"),
                Some("last_7_days"),
            ),
            c(
                "vs-last-week",
                "comparison",
                Some("sales.gross"),
                Some("this_week"),
            ),
        ],
        "routines" => vec![c("reports", "report_list", None, None)],
        _ => vec![],
    };
    Some((position as i16, FlowSceneConfig { components }))
}

/// Path key: a built-in, or `sales_*` / `stock_*` (1-26 of a-z, 0-9, _ after the prefix).
pub fn check_scene(scene: &str) -> Result<(), AppError> {
    let custom = CUSTOM_PREFIXES.iter().any(|p| {
        scene.strip_prefix(p).is_some_and(|rest| {
            (1..=26).contains(&rest.len())
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
    });
    if SCENES.contains(&scene) || custom {
        return Ok(());
    }
    Err(AppError::field(
        "scene",
        "The scene must be a built-in scene or start with sales_ or stock_.",
    ))
}

/// A scene as its owner sees it.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "One Your Flow scene: built-in default or the caller's saved settings. `restricted` lists component ids the caller's role can't read; Flow shows them as not available, never their values."
)]
pub struct FlowScene {
    #[schema(example = "pulse")]
    pub scene: String,
    /// 0 to 19; scenes sort by position, then key.
    pub position: i16,
    pub hidden: bool,
    /// True when the caller saved this scene; false = Rust default.
    pub saved: bool,
    /// Config format version; 1.
    pub schema_version: i16,
    pub config: FlowSceneConfig,
    pub restricted: Vec<String>,
}

/// PUT body: any of the three; omitted ones keep the saved value (else the default).
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateFlowSceneRequest {
    /// 0 to 19.
    #[schema(value_type = Option<i64>, example = 0)]
    position: Option<Value>,
    #[schema(value_type = Option<bool>)]
    hidden: Option<Value>,
    /// Required on a new custom scene. Replaces the whole component list.
    config: Option<FlowSceneConfig>,
}

/// Validated PUT input.
pub struct SceneUpdate {
    pub position: Option<i16>,
    pub hidden: Option<bool>,
    pub config: Option<FlowSceneConfig>,
}

fn check_config(r: &mut Rules, config: &FlowSceneConfig) {
    if serde_json::to_vec(config).map_or(true, |v| v.len() > MAX_CONFIG_BYTES) {
        r.add("config", "The config must not be larger than 8 KiB.");
        return;
    }
    if config.components.len() > MAX_COMPONENTS {
        r.add(
            "config.components",
            format!("The components must not have more than {MAX_COMPONENTS} items."),
        );
        return;
    }
    for (i, comp) in config.components.iter().enumerate() {
        let at = |f: &str| format!("config.components.{i}.{f}");
        let id_ok = (1..=32).contains(&comp.id.len())
            && comp
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok {
            r.add(&at("id"), "The id must be 1-32 of a-z, 0-9 and -.");
        } else if config.components[..i].iter().any(|o| o.id == comp.id) {
            r.add(&at("id"), "The id has already been taken in this scene.");
        }
        if !TYPES.contains(&comp.kind.as_str()) {
            r.add(&at("type"), "The selected type is invalid.");
            continue;
        }
        let wants_metric = METRIC_TYPES.contains(&comp.kind.as_str());
        match (&comp.metric, wants_metric) {
            (None, true) => r.add(&at("metric"), "The metric field is required for this type."),
            (Some(_), false) => r.add(
                &at("metric"),
                "The metric field does not apply to this type.",
            ),
            (Some(m), true) if !METRICS.contains(&m.as_str()) => {
                r.add(&at("metric"), "The selected metric is invalid.")
            }
            _ => {}
        }
        match &comp.period {
            None if wants_metric => {
                r.add(&at("period"), "The period field is required for this type.")
            }
            Some(p) if !PERIODS.contains(&p.as_str()) => {
                r.add(&at("period"), "The selected period is invalid.")
            }
            _ => {}
        }
    }
}

/// True when a stored config still passes today's vocabulary.
pub fn readable(config: &FlowSceneConfig) -> bool {
    let mut r = Rules::default();
    check_config(&mut r, config);
    r.into_result().is_ok()
}

impl Validate for UpdateFlowSceneRequest {
    type Valid = SceneUpdate;

    fn validate(self, r: &mut Rules) -> Option<SceneUpdate> {
        let (position, hidden) = (
            self.position.filter(|v| !v.is_null()),
            self.hidden.filter(|v| !v.is_null()),
        );
        if position.is_none() && hidden.is_none() && self.config.is_none() {
            r.add("body", "Send position, hidden or config.");
            return None;
        }
        let position = position
            .map(|v| r.integer("position", &v, 0, MAX_POSITION))
            .map(|n| n.and_then(|n| i16::try_from(n).ok()));
        let hidden = hidden.map(|v| r.boolean("hidden", &v));
        if let Some(config) = &self.config {
            check_config(r, config);
        }
        Some(SceneUpdate {
            position: position.map(|p| p.unwrap_or_default()),
            hidden: hidden.map(|h| h.unwrap_or_default()),
            config: self.config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_keys() {
        for ok in [
            "pulse",
            "studio",
            "sales_x",
            "stock_low_2",
            &format!("sales_{}", "a".repeat(26)),
        ] {
            assert!(check_scene(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "sales_",
            "orders_x",
            "Pulse",
            "sales_A",
            "stock-low",
            &format!("sales_{}", "a".repeat(27)),
        ] {
            assert!(check_scene(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn permissions_follow_metric_then_type() {
        let p = |kind, metric: Option<&str>| c("a", kind, metric, None).permission();
        assert_eq!(p("metric_card", Some("sales.net")), Some("order.list"));
        assert_eq!(p("line_chart", Some("orders.draft")), Some("order.list"));
        assert_eq!(
            p("comparison", Some("invoices.unpaid")),
            Some("invoice.list")
        );
        assert_eq!(
            p("metric_card", Some("stock.below_minimum")),
            Some("stock.list")
        );
        assert_eq!(p("order_list", None), Some("order.list"));
        assert_eq!(p("invoice_table", None), Some("invoice.list"));
        assert_eq!(p("stock_list", None), Some("stock.list"));
        assert_eq!(p("attention_panel", None), None);
        assert_eq!(p("report_list", None), None);
    }

    #[test]
    fn defaults_pass_their_own_rules() {
        for (i, scene) in SCENES.iter().enumerate() {
            let (pos, config) = default(scene).unwrap();
            assert_eq!(pos as usize, i);
            let mut r = Rules::default();
            check_config(&mut r, &config);
            assert!(r.into_result().is_ok(), "{scene}");
        }
        assert!(default("sales_x").is_none());
        assert_eq!(CUSTOM_POSITION as usize, SCENES.len());
    }
}
