//! Laravel `Infrastructure\Enums\Domains` values: what a custom type or status belongs to.
//! Frozen copy, sorted; regenerate when Laravel adds a domain.

/// Every domain name, sorted for binary search.
pub const ALL: [&str; 84] = [
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

/// Known domain, any case (Laravel EnumExist lowercases before `tryFrom`).
pub fn known(domain: &str) -> bool {
    ALL.binary_search(&domain.to_ascii_lowercase().as_str())
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_unique_lowercase() {
        assert!(ALL.windows(2).all(|w| w[0] < w[1]));
        assert!(ALL.iter().all(|d| *d == d.to_lowercase()));
    }

    #[test]
    fn any_case_known() {
        for ok in [
            "deal",
            "Deal",
            "hour_registration_type",
            "relation",
            "lead_source",
        ] {
            assert!(known(ok), "{ok}");
        }
        for bad in ["deals", "", "hour registration type"] {
            assert!(!known(bad), "{bad}");
        }
    }
}
