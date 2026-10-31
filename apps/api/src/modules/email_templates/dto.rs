//! Email template bodies (Laravel Store/UpdateEmailTemplateRequest) and the list query.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::{
        pdf_templates::dto::{Conditions, Patch, conditions, object, variables_schema},
        pricing::body::{Capped, Scalar},
    },
};

/// Laravel's `Domains` enum: what a template may be for.
pub const DOMAINS: &[&str] = &[
    "activity",
    "approval",
    "association",
    "cash_register",
    "cash_register_daily_log",
    "collective_invoice",
    "collective_invoice_payment",
    "contact",
    "currency",
    "custom_field",
    "custom_field_section",
    "custom_field_value",
    "custom_status",
    "custom_type",
    "dashboard",
    "data_synchronization",
    "deal",
    "deal_pipeline",
    "deal_pipeline_status",
    "discount",
    "discount_group",
    "domain_log",
    "email_profile",
    "email_template",
    "employment_group",
    "estimate",
    "external_association",
    "f_t_p_server",
    "file",
    "ftp_server",
    "hour_registration",
    "hour_registration_type",
    "hourly_rate",
    "incoming_invoice",
    "integration_profile",
    "inventory_reconciliation",
    "invite",
    "invoice",
    "invoice_payment",
    "item",
    "label_template",
    "lead",
    "lead_source",
    "leave_request",
    "order",
    "pdf_template",
    "planned_work_order_date",
    "pos_register",
    "pos_vault",
    "price_correction",
    "product",
    "product_brand",
    "product_category",
    "product_return_reason",
    "product_variant",
    "project",
    "purchase_order",
    "purchase_order_item",
    "recurring_invoice",
    "relation",
    "reminder",
    "return_item",
    "sales_group",
    "setting",
    "shipment_method",
    "special_offer",
    "stock",
    "storage_location",
    "tag",
    "team",
    "ticket",
    "ticket_box",
    "ticket_label",
    "todo",
    "transfer_list",
    "transfer_list_item",
    "user",
    "vat_group",
    "warehouse",
    "warehouse_location",
    "warehouse_section",
    "work_order",
    "workflow",
    "workspace",
];

/// Body of create and update; unknown keys are skipped.
#[derive(Deserialize, Default, ToSchema)]
#[schema(
    description = "An email template. Create needs every field but `conditions` and `meta`; update needs only `body`, ignores `template_domain` and keeps what is left out."
)]
#[schema(as = EmailTemplateBody)]
pub struct TemplateBody {
    #[schema(value_type = String, max_length = 255)]
    #[serde(default)]
    template_name: Scalar,
    /// What the template is for (`invoice`, `estimate`, ...); create only.
    #[schema(value_type = String)]
    #[serde(default)]
    template_domain: Scalar,
    #[schema(value_type = bool)]
    #[serde(default)]
    default_template: Scalar,
    #[schema(value_type = String, max_length = 255)]
    #[serde(default)]
    subject: Scalar,
    #[schema(value_type = String, max_length = 25000, required = true)]
    #[serde(default)]
    body: Scalar,
    /// When this template is picked; null clears them.
    #[schema(value_type = Option<Conditions>)]
    #[serde(default)]
    conditions: Capped,
    #[schema(value_type = Option<Object>)]
    #[serde(default)]
    meta: Capped,
}

/// `POST /email-templates`.
#[derive(Deserialize, ToSchema)]
#[schema(as = CreateEmailTemplateRequest)]
pub struct CreateTemplateRequest(TemplateBody);

// `POST /email-templates/{id}`: Laravel's update rules make only `body` required and
// don't take the domain.
/// `POST /email-templates/{id}`.
#[derive(Deserialize, ToSchema)]
#[schema(as = UpdateEmailTemplateRequest)]
pub struct UpdateTemplateRequest(TemplateBody);

// Doc-only: one template as `service::render` prints it with `json!`.
/// An email template.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct EmailTemplateResource {
    id: Uuid,
    template_name: String,
    template_domain: String,
    default_template: bool,
    subject: String,
    body: String,
    #[schema(required = true)]
    conditions: Option<Conditions>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
}

// Doc-only: `variables` serves generated JSON text in this shape.
/// Template variables of one domain.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct TemplateVariables {
    #[schema(schema_with = variables_schema)]
    variables: Value,
}

/// A validated body; on update None keeps the stored value.
pub struct TemplateInput {
    /// Create only.
    pub template_domain: Option<String>,
    pub template_name: Option<String>,
    pub default_template: Option<bool>,
    pub subject: Option<String>,
    pub body: String,
    pub conditions: Patch<Conditions>,
    pub meta: Patch<Value>,
}

/// `{data:{email_template_id}}`.
#[derive(Serialize, ToSchema)]
#[schema(as = EmailTemplateCreated)]
pub struct TemplateCreated {
    pub email_template_id: Uuid,
}

// List query (Laravel ListEmailTemplateRequest; `relation_id` and `items` were unused).
/// List query.
#[derive(Deserialize, Default, IntoParams)]
#[serde(default)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// Only templates for this domain.
    pub template_domain: Option<String>,
    /// Search text.
    pub q: Option<String>,
    /// Only templates for `business` or `personal` relations.
    pub relation_type: Option<String>,
    /// Only templates for this language.
    pub language: Option<String>,
    /// Only templates for this relation custom type.
    pub relation_custom_type_id: Option<String>,
}

impl Validate for CreateTemplateRequest {
    type Valid = TemplateInput;

    fn validate(self, r: &mut Rules) -> Option<TemplateInput> {
        let b = self.0;
        let template_domain = match b.template_domain.filled() {
            None => {
                r.missing("template_domain");
                None
            }
            Some(v) => {
                let sent = v.as_str().map(str::trim).unwrap_or_default();
                let d = sent.to_lowercase();
                if DOMAINS.contains(&d.as_str()) {
                    Some(d)
                } else {
                    let msg = format!("The selected template domain: {sent} is invalid.");
                    r.add("template_domain", msg);
                    None
                }
            }
        };
        let input = fields(r, b, true);
        Some(TemplateInput {
            template_domain,
            ..input
        })
    }
}

impl Validate for UpdateTemplateRequest {
    type Valid = TemplateInput;

    fn validate(self, r: &mut Rules) -> Option<TemplateInput> {
        Some(fields(r, self.0, false))
    }
}

/// Every field but the domain; `create` makes name, default and subject required.
fn fields(r: &mut Rules, b: TemplateBody, create: bool) -> TemplateInput {
    let template_name = text(r, "template_name", &b.template_name, 255, create);
    let subject = text(r, "subject", &b.subject, 255, create);
    let body = text(r, "body", &b.body, 25_000, true).unwrap_or_default();
    let default_template = match b.default_template.filled() {
        None => {
            if create {
                r.missing("default_template");
            }
            None
        }
        Some(v) => r.boolean("default_template", &v),
    };
    TemplateInput {
        template_domain: None,
        template_name,
        default_template,
        subject,
        body,
        conditions: conditions(r, &b.conditions),
        meta: object(r, "meta", &b.meta),
    }
}

/// A trimmed string of at most `max` characters; null or blank is missing (required) or
/// not sent (kept).
fn text(r: &mut Rules, field: &str, v: &Scalar, max: usize, required: bool) -> Option<String> {
    match v.filled() {
        None => {
            if required {
                r.missing(field);
            }
            None
        }
        Some(Value::String(s)) => {
            let s = s.trim().to_owned();
            r.max_chars(field, &s, max);
            Some(s)
        }
        Some(_) => {
            r.add(field, format!("The {} must be a string.", human(field)));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The migration's CHECK lists exactly these domains.
    #[test]
    fn the_table_check_lists_every_domain() {
        let sql =
            include_str!("../../../migrations/20261022091926_email_templates_profiles.up.sql");
        let check = sql
            .split_once("template_domain IN (")
            .and_then(|(_, rest)| rest.split_once("))"))
            .map(|(list, _)| list)
            .unwrap();
        let listed: Vec<&str> = check
            .split(',')
            .map(|d| d.trim().trim_matches('\''))
            .collect();
        assert_eq!(listed, DOMAINS);
    }
}
