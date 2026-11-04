//! Supply link request/response bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::supply_links::repo::LinkRow,
};

/// Farmer fields a shop may show publicly through a link.
pub const FIELDS: &[&str] = &["farmer_count", "harvest_window", "village"];
/// Fields that wait for the data-protection gate (Tanzania PDPC registration and transfer
/// permit): refused until the user confirms both.
const GATED: &[&str] = &["farmer_names", "farmer_photos"];
const FIELDS_MAX: usize = 10;

/// A link as seen from one side.
#[derive(Serialize, ToSchema)]
#[schema(description = "A consented supply link between a farm workspace and a shop workspace.")]
pub struct LinkResource {
    id: Uuid,
    /// `outgoing` (this workspace granted it) or `incoming`.
    direction: &'static str,
    partner: Partner,
    /// Public farmer fields the receiving side may show.
    fields: Vec<String>,
    /// `pending`, `live` or `revoked`.
    status: &'static str,
    granted_at: DateTime<Utc>,
    #[schema(required = true)]
    accepted_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    revoked_at: Option<DateTime<Utc>>,
}

/// The other workspace.
#[derive(Serialize, ToSchema)]
pub struct Partner {
    id: Uuid,
    /// The workspace slug.
    name: String,
    /// The workspace's company name, or its slug when it has none.
    display_name: String,
}

impl Partner {
    /// The workspace `id` named `name`, shown as `display_name`.
    pub fn new(id: Uuid, name: String, display_name: String) -> Self {
        Self {
            id,
            name,
            display_name,
        }
    }
}

impl LinkResource {
    /// `l` as `workspace_id` sees it.
    pub fn new(l: LinkRow, workspace_id: Uuid) -> Self {
        let status = match (l.accepted_at, l.revoked_at) {
            (_, Some(_)) => "revoked",
            (Some(_), None) => "live",
            (None, None) => "pending",
        };
        Self {
            id: l.id,
            direction: if l.from_workspace_id == workspace_id {
                "outgoing"
            } else {
                "incoming"
            },
            partner: Partner {
                id: l.partner_id,
                name: l.partner_name,
                display_name: l.partner_display_name,
            },
            fields: l.fields,
            status,
            granted_at: l.granted_at,
            accepted_at: l.accepted_at,
            revoked_at: l.revoked_at,
        }
    }
}

/// Link statuses, as `LinkResource.status` prints them.
pub const STATUSES: &[&str] = &["pending", "live", "revoked"];
/// Sides of a link, as `LinkResource.direction` prints them.
pub const DIRECTIONS: &[&str] = &["outgoing", "incoming"];

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Only links in this status: `pending`, `live` or `revoked`.
    #[param(value_type = Option<String>, example = "pending")]
    pub status: Option<String>,
    /// Only links this workspace granted (`outgoing`) or received (`incoming`).
    #[param(value_type = Option<String>, example = "incoming")]
    pub direction: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

impl ListQuery {
    /// `(status, direction)`, each None when absent or blank; 422 on another value.
    pub fn filter(&self) -> Result<(Option<String>, Option<String>), crate::error::AppError> {
        let mut r = Rules::default();
        let status = r.one_of("status", self.status.clone(), STATUSES, None);
        let direction = r.one_of("direction", self.direction.clone(), DIRECTIONS, None);
        r.into_result()?;
        Ok((status, direction))
    }
}

/// POST /supply-links.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Grant a shop workspace a supply link.")]
pub struct GrantLinkRequest {
    /// The receiving (shop) workspace.
    #[schema(value_type = Uuid)]
    to_workspace_id: Option<String>,
    /// Public farmer fields it may show: `farmer_count`, `harvest_window`, `village` (may be empty).
    #[schema(value_type = Vec<String>, example = json!(["village", "farmer_count", "harvest_window"]))]
    fields: Option<Value>,
}

/// Validated grant.
pub struct NewLink {
    pub to_workspace_id: Uuid,
    pub fields: Vec<String>,
}

impl Validate for GrantLinkRequest {
    type Valid = NewLink;

    fn validate(self, r: &mut Rules) -> Option<NewLink> {
        let to = r
            .required("to_workspace_id", self.to_workspace_id)
            .and_then(|v| {
                let id = Uuid::parse_str(v.trim()).ok();
                r.check(
                    "to_workspace_id",
                    id.is_some(),
                    "The to workspace id must be a valid UUID.",
                );
                id
            });
        let mut fields: Vec<String> = Vec::new();
        match self.fields.unwrap_or(Value::Array(Vec::new())) {
            Value::Array(list) if list.len() <= FIELDS_MAX => {
                for v in list {
                    let f = v.as_str().unwrap_or_default().to_owned();
                    if GATED.contains(&f.as_str()) {
                        r.add(
                            "fields",
                            format!("{f} can't be shared until the data-protection gate opens."),
                        );
                    } else if !FIELDS.contains(&f.as_str()) {
                        r.add("fields", format!("Unknown field {f}."));
                    }
                    fields.push(f);
                }
            }
            _ => r.add("fields", "The fields must be a list of at most 10 fields."),
        }
        fields.sort();
        fields.dedup();
        Some(NewLink {
            to_workspace_id: to.unwrap_or_default(),
            fields,
        })
    }
}
