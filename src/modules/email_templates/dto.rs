//! Email template bodies (Laravel Store/UpdateEmailTemplateRequest) and the list query.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::{
        pdf_templates::dto::{Conditions, Patch, conditions, object},
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
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct TemplateBody {
    template_name: Scalar,
    template_domain: Scalar,
    default_template: Scalar,
    subject: Scalar,
    body: Scalar,
    conditions: Capped,
    meta: Capped,
}

/// `POST /email-templates`.
#[derive(Deserialize)]
pub struct CreateTemplateRequest(TemplateBody);

/// `POST /email-templates/{id}`: Laravel's update rules make only `body` required and
/// don't take the domain.
#[derive(Deserialize)]
pub struct UpdateTemplateRequest(TemplateBody);

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
#[derive(Serialize)]
pub struct TemplateCreated {
    pub email_template_id: Uuid,
}

/// List query (Laravel ListEmailTemplateRequest; `relation_id` and `items` were unused).
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ListQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
    pub template_domain: Option<String>,
    pub q: Option<String>,
    pub relation_type: Option<String>,
    pub language: Option<String>,
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
