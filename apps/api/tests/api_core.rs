//! API integration tests: accounts, sign-in, settings, orders, stock and warehouses.
//! The suite is split over the `tests/api_*.rs` crates so no single compile outgrows CI's memory
//! (one ~97k-line crate peaked at 15.3 GB). Feature files stay in `tests/api/`; files that call
//! each other's helpers (`crate::other::…`) share a crate. New file: declare it in one crate below;
//! `every_file_is_a_module` in `api_core.rs` fails otherwise.

#[path = "common/mod.rs"]
mod common;

#[path = "api/active_products.rs"]
mod active_products;
#[path = "api/afm_catalog.rs"]
mod afm_catalog;
#[path = "api/body_limits.rs"]
mod body_limits;
#[path = "api/bootstrap.rs"]
mod bootstrap;
#[path = "api/checkout.rs"]
mod checkout;
#[path = "api/cli_migrate.rs"]
mod cli_migrate;
#[path = "api/currencies.rs"]
mod currencies;
#[path = "api/currency_rates.rs"]
mod currency_rates;
#[path = "api/custom_statuses.rs"]
mod custom_statuses;
#[path = "api/custom_types.rs"]
mod custom_types;
#[path = "api/db.rs"]
mod db;
#[path = "api/deal_pipelines.rs"]
mod deal_pipelines;
#[path = "api/deals.rs"]
mod deals;
#[path = "api/demo_seed.rs"]
mod demo_seed;
#[path = "api/health.rs"]
mod health;
#[path = "api/healthcheck.rs"]
mod healthcheck;
#[path = "api/hourly_rates.rs"]
mod hourly_rates;
#[path = "api/invites.rs"]
mod invites;
#[path = "api/lots.rs"]
mod lots;
#[path = "api/me.rs"]
mod me;
#[path = "api/member_removal.rs"]
mod member_removal;
#[path = "api/orders.rs"]
mod orders;
#[path = "api/permissions.rs"]
mod permissions;
#[path = "api/phase1_pipes.rs"]
mod phase1_pipes;
#[path = "api/private_limits.rs"]
mod private_limits;
#[path = "api/purchase_units.rs"]
mod purchase_units;
#[path = "api/rate_limit.rs"]
mod rate_limit;
#[path = "api/relations.rs"]
mod relations;
#[path = "api/request_timeouts.rs"]
mod request_timeouts;
#[path = "api/sale_availability.rs"]
mod sale_availability;
#[path = "api/schema.rs"]
mod schema;
#[path = "api/set_user_password.rs"]
mod set_user_password;
#[path = "api/settings.rs"]
mod settings;
#[path = "api/settings_credentials.rs"]
mod settings_credentials;
#[path = "api/signup.rs"]
mod signup;
#[path = "api/stock.rs"]
mod stock;
#[path = "api/stock_seed.rs"]
mod stock_seed;
#[path = "api/storage_locations.rs"]
mod storage_locations;
#[path = "api/teams.rs"]
mod teams;
#[path = "api/users.rs"]
mod users;
#[path = "api/verify_email.rs"]
mod verify_email;
#[path = "api/warehouse_hierarchy.rs"]
mod warehouse_hierarchy;
#[path = "api/warehouse_locations.rs"]
mod warehouse_locations;
#[path = "api/warehouses.rs"]
mod warehouses;
#[path = "api/workspace_defaults.rs"]
mod workspace_defaults;
#[path = "api/workspaces.rs"]
mod workspaces;

/// Every file in `tests/api/` is a module of exactly one `tests/api_*.rs` crate; a file without
/// its `#[path]` line would silently never compile or run, one in two crates would run twice.
#[test]
fn every_file_is_a_module() {
    let tests = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");
    let roots = [
        include_str!("api_catalog.rs"),
        include_str!("api_core.rs"),
        include_str!("api_farm.rs"),
        include_str!("api_money.rs"),
    ];
    let crates = std::fs::read_dir(tests)
        .unwrap()
        .filter(|e| {
            let name = e.as_ref().unwrap().file_name().into_string().unwrap();
            name.starts_with("api_") && name.ends_with(".rs")
        })
        .count();
    assert_eq!(
        crates,
        roots.len(),
        "list every tests/api_*.rs crate in `roots`"
    );
    let mut wrong = Vec::new();
    for entry in std::fs::read_dir(format!("{tests}/api")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let line = format!("#[path = \"api/{name}\"]");
        let declared = roots
            .iter()
            .flat_map(|r| r.lines())
            .filter(|l| *l == line)
            .count();
        if declared != 1 {
            wrong.push((name, declared));
        }
    }
    assert!(
        wrong.is_empty(),
        "declare each file in exactly one tests/api_*.rs crate: {wrong:?}"
    );
}
