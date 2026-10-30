//! Permission catalog: frozen copy of Laravel `src/Domain/**/V1/Permissions/permissions.json`
//! as `PermissionHelper` serves it (43 groups, 222 names), plus 3 groups Laravel checked
//! but never catalogued, plus `specialoffer` and `pdftemplate` (48 / 247). Name =
//! `group.action`.
//! Regenerate when a ported module adds actions.

/// Groups in key order, actions sorted (Laravel `ksort` + `sort`).
pub const GROUPS: &[(&str, &[&str])] = &[
    (
        "association",
        &["create", "delete", "get", "list", "update"],
    ),
    (
        "cashregister",
        &["create", "delete", "list", "update", "view"],
    ),
    ("contact", &["create", "delete", "get", "list", "update"]),
    ("currency", &["create", "delete", "get", "list", "update"]),
    (
        "customstatus",
        &[
            "archive",
            "create",
            "delete",
            "get",
            "list",
            "unarchive",
            "update",
            "viewany",
        ],
    ),
    (
        "customtype",
        &["create", "delete", "list", "update", "view"],
    ),
    (
        "deal",
        &[
            "archive",
            "create",
            "delete",
            "get",
            "list",
            "unarchive",
            "update",
        ],
    ),
    (
        "dealpipeline",
        &["create", "delete", "get", "list", "update"],
    ),
    ("dealpipelinestatus", &["create", "delete", "get", "update"]),
    ("discount", &["create", "delete", "list", "update", "view"]),
    (
        "discountgroup",
        &[
            "attach", "create", "delete", "detach", "list", "update", "view",
        ],
    ),
    ("domainlog", &["list", "view"]),
    (
        "emailprofile",
        &["create", "delete", "get", "list", "update"],
    ),
    (
        "emailtemplate",
        &["create", "delete", "get", "list", "update"],
    ),
    (
        "estimate",
        &[
            "accept",
            "cancel",
            "create",
            "decline",
            "delete",
            "get",
            "list",
            "send",
            "summarize",
            "update",
        ],
    ),
    ("file", &["create", "delete", "get", "list"]),
    ("hourlyrate", &["create", "delete", "get", "list", "update"]),
    (
        "hourregistration",
        &["create", "delete", "get", "list", "update"],
    ),
    ("invite", &["create", "delete", "get", "list"]),
    (
        "invoice",
        &[
            "create",
            "credit",
            "delete",
            "list",
            "send",
            "summarize",
            "update",
            "view",
        ],
    ),
    (
        "invoicepayment",
        &["create", "delete", "list", "update", "view"],
    ),
    ("item", &["create", "delete", "get", "list", "update"]),
    ("lead", &["create", "delete", "get", "list", "update"]),
    ("manufacturing", &["manage", "release.compliance_override"]),
    ("order", &["create", "delete", "list", "update", "view"]),
    // pdftemplate: added by genesy in F5a (Laravel checked it, never catalogued).
    (
        "pdftemplate",
        &["create", "delete", "list", "update", "view"],
    ),
    ("permission", &["assign"]),
    (
        "pricecorrection",
        &["create", "delete", "list", "update", "view"],
    ),
    (
        "product",
        &[
            "archive",
            "create",
            "delete",
            "duplicate",
            "list",
            "unarchive",
            "update",
            "view",
        ],
    ),
    (
        "product_serial_number",
        &["create", "list", "update", "view"],
    ),
    (
        "productbrand",
        &["create", "delete", "get", "list", "update"],
    ),
    (
        "productcategory",
        &["create", "delete", "get", "list", "update"],
    ),
    (
        "relation",
        &["create", "delete", "get", "history", "list", "update"],
    ),
    ("reminder", &["create", "delete", "get", "list", "update"]),
    // salesgroup, shipmentmethod, vatgroup: added by genesy (Laravel checked them, never catalogued).
    ("salesgroup", &["create", "delete", "get", "list", "update"]),
    ("setting", &["get", "update"]),
    (
        "shipmentmethod",
        &["create", "delete", "get", "list", "update"],
    ),
    // specialoffer: added by genesy in F3b (Laravel checked it, never catalogued).
    (
        "specialoffer",
        &["create", "delete", "list", "update", "view"],
    ),
    (
        "stock",
        &[
            "create",
            "get_by_bar",
            "get_by_ean",
            "get_by_id",
            "get_by_reference",
            "list",
            "move",
            "mutate",
            "reserve",
            "set_stock_levels",
            "set_stock_requirements",
        ],
    ),
    ("tag", &["create", "delete", "list", "update", "view"]),
    (
        "team",
        &[
            "create",
            "delete",
            "get",
            "invite",
            "list",
            "permissions",
            "update",
        ],
    ),
    (
        "transferlist",
        &["create", "delete", "list", "update", "view"],
    ),
    ("user", &["get"]),
    ("vatgroup", &["create", "delete", "get", "list", "update"]),
    (
        "warehouse",
        &["create", "delete", "get", "list", "set_default", "update"],
    ),
    ("workflow", &["create", "delete", "get", "update"]),
    ("workorder", &["create", "delete", "get", "list", "update"]),
    (
        "workspace",
        &[
            "get",
            "global-search",
            "list",
            "send-message",
            "update",
            "view-event",
        ],
    ),
];

/// Pass every backend check; the UI uses them to hide price columns (Laravel `config/rbac.php`).
pub const FRONTEND_ONLY: [&str; 3] = [
    "product.view_price",
    "product.view_purchase_price",
    "product.view_margin",
];

/// Superuser grant stored on a team. Outside the catalog; only the owner may grant it.
pub const WORKSPACE_ADMIN: &str = "workspace_admin";

/// Computed for the workspace creator, never stored.
pub const WORKSPACE_OWNER: &str = "workspace_owner";

/// True when `name` is exactly a catalog `group.action`. Expects lowercase input.
/// Stricter than Laravel, which matched only the last dot segment.
pub fn assignable(name: &str) -> bool {
    let Some((group, action)) = name.split_once('.') else {
        return false;
    };
    GROUPS
        .binary_search_by(|(g, _)| g.cmp(&group))
        .is_ok_and(|i| GROUPS[i].1.binary_search(&action).is_ok())
}

/// Every catalog name, in catalog order.
pub fn names() -> impl Iterator<Item = String> {
    GROUPS
        .iter()
        .flat_map(|(group, actions)| actions.iter().map(move |a| format!("{group}.{a}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_laravel_size() {
        assert_eq!(GROUPS.len(), 48);
        assert_eq!(names().count(), 247);
    }

    #[test]
    fn groups_and_actions_sorted_unique_lowercase() {
        assert!(
            GROUPS.windows(2).all(|w| w[0].0 < w[1].0),
            "groups not strictly sorted"
        );
        for (group, actions) in GROUPS {
            assert!(
                actions.windows(2).all(|w| w[0] < w[1]),
                "{group} actions not strictly sorted"
            );
            assert!(
                actions.iter().all(|a| *a == a.to_lowercase()),
                "{group} has uppercase"
            );
        }
    }

    /// Batch 4 groups Laravel never catalogued (it checked them, only owner/admin passed).
    #[test]
    fn reference_data_groups_assignable() {
        for group in ["salesgroup", "shipmentmethod", "vatgroup"] {
            for action in ["create", "delete", "get", "list", "update"] {
                assert!(assignable(&format!("{group}.{action}")), "{group}.{action}");
            }
        }
        for action in ["create", "delete", "list", "update", "view"] {
            for group in ["specialoffer", "pdftemplate"] {
                assert!(assignable(&format!("{group}.{action}")), "{group}.{action}");
            }
        }
    }

    #[test]
    fn catalog_names_are_assignable() {
        assert!(names().all(|n| assignable(&n)));
        assert!(assignable("team.permissions"));
        assert!(assignable("manufacturing.release.compliance_override"));
    }

    #[test]
    fn non_catalog_names_are_not() {
        for bad in [
            "",
            ".",
            "team",
            "team.",
            ".list",
            "team.fly",
            "nope.list",
            "relation.extra.list",
            WORKSPACE_ADMIN,
            WORKSPACE_OWNER,
            "product.view_price",
        ] {
            assert!(!assignable(bad), "accepted {bad:?}");
        }
    }
}
