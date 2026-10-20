//! Warehouse hierarchy: /warehouses/{w}/sections/{s}/rows/{r}/shelves/{sh}/bins/{b}, section
//! auto-layout, and GET /warehouses/{w}/bins.

mod common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

/// `'static` base path for `Api::with_base`.
fn base(path: String) -> &'static str {
    Box::leak(path.into_boxed_str())
}

struct World {
    api: Api,
    wh: String,
}

impl World {
    async fn new(pool: PgPool) -> Self {
        let api = Api::new(pool, "/warehouses").await;
        let wh = api.create(json!({ "name": "Main" }), "warehouse_id").await;
        Self { api, wh }
    }

    fn sections(&self) -> Api {
        self.api
            .with_base(base(format!("/warehouses/{}/sections", self.wh)))
    }

    fn rows(&self, s: &str) -> Api {
        self.api
            .with_base(base(format!("/warehouses/{}/sections/{s}/rows", self.wh)))
    }

    fn shelves(&self, s: &str, r: &str) -> Api {
        self.api.with_base(base(format!(
            "/warehouses/{}/sections/{s}/rows/{r}/shelves",
            self.wh
        )))
    }

    fn bins(&self, s: &str, r: &str, sh: &str) -> Api {
        self.api.with_base(base(format!(
            "/warehouses/{}/sections/{s}/rows/{r}/shelves/{sh}/bins",
            self.wh
        )))
    }

    async fn section(&self, code: &str) -> String {
        self.sections()
            .create(
                json!({ "section_name": code, "section_code": code }),
                "section_id",
            )
            .await
    }

    async fn row(&self, s: &str, code: &str) -> String {
        self.rows(s)
            .create(
                json!({ "row_code": code, "row_name": code, "length_meters": 10,
                        "width_meters": "1.5", "height_meters": 6 }),
                "row_id",
            )
            .await
    }

    async fn shelf(&self, s: &str, r: &str, code: &str) -> String {
        self.shelves(s, r)
            .create(json!({ "shelf_code": code, "shelf_level": 1 }), "shelf_id")
            .await
    }

    async fn bin(&self, s: &str, r: &str, sh: &str, code: &str) -> String {
        self.bins(s, r, sh)
            .create(json!({ "bin_code": code }), "bin_id")
            .await
    }
}

fn data(res: &Value) -> &Vec<Value> {
    res["data"].as_array().unwrap()
}

fn field(res: &Value, key: &str) -> Vec<String> {
    data(res)
        .iter()
        .map(|v| v[key].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn section_layout_generates_rows_shelves_and_bins(pool: PgPool) {
    let w = World::new(pool).await;

    let s = w
        .sections()
        .create(
            json!({ "section_name": "Hall", "section_code": "S1", "rows": 2, "shelves_per_row": 2,
                    "bins_per_shelf": 2, "temperature_control": "cold" }),
            "section_id",
        )
        .await;

    let rows = w.rows(&s).list("").await;
    assert_eq!(field(&rows, "row_code"), vec!["S1-ROW-A", "S1-ROW-B"]);
    assert_eq!(field(&rows, "row_name"), vec!["Row A", "Row B"]);
    let row_a = &data(&rows)[0];
    assert_eq!(row_a["length_meters"], 10.0);
    assert_eq!(row_a["width_meters"], 1.2);
    assert_eq!(row_a["shelf_configuration"]["number_of_shelves"], 2);
    assert_eq!(row_a["additional_settings"]["temperature_zone"], "cold");
    let r = row_a["id"].as_str().unwrap();

    let shelves = w.shelves(&s, r).list("").await;
    assert_eq!(
        field(&shelves, "shelf_code"),
        vec!["S1-ROW-A-SHELF-1", "S1-ROW-A-SHELF-2"]
    );
    assert_eq!(field(&shelves, "shelf_name"), vec!["Shelf 1", "Shelf 2"]);
    assert_eq!(data(&shelves)[1]["shelf_level"], 2);
    let sh = data(&shelves)[0]["id"].as_str().unwrap();

    let bins = w.bins(&s, r, sh).list("").await;
    assert_eq!(field(&bins, "bin_code"), vec!["S1-A101", "S1-A102"]);
    assert_eq!(field(&bins, "bin_name"), vec!["Bin A101", "Bin A102"]);

    let all = w
        .api
        .with_base(base(format!("/warehouses/{}/bins", w.wh)))
        .list("")
        .await;
    assert_eq!(data(&all).len(), 8);
    assert_eq!(
        field(&all, "bin_code"),
        vec![
            "S1-A101", "S1-A102", "S1-A201", "S1-A202", "S1-B101", "S1-B102", "S1-B201", "S1-B202"
        ]
    );
    let first = &data(&all)[0];
    assert_eq!(first["warehouse"], json!({ "id": w.wh, "name": "Main" }));
    assert_eq!(first["section"]["section_code"], "S1");
    assert_eq!(first["row"]["row_code"], "S1-ROW-A");
    assert_eq!(first["shelf"]["shelf_code"], "S1-ROW-A-SHELF-1");
}

#[sqlx::test]
async fn section_resource_layout_and_defaults(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w
        .sections()
        .create(
            json!({ "section_name": "Hall", "section_code": "S1", "rows": 8, "shelves_per_row": 6,
                    "bins_per_shelf": 4 }),
            "section_id",
        )
        .await;

    let sec = w.sections().get(&s).await;

    assert_eq!(
        (sec["id"].clone(), sec["section_id"].clone()),
        (json!(s), json!(s))
    );
    assert_eq!(sec["warehouse_id"], w.wh.as_str());
    assert_eq!(
        sec["layout"],
        json!({ "rows": 8, "shelves_per_row": 6, "bins_per_shelf": 4, "total_rows": 8,
                "total_shelves": 48, "total_bins": 192, "total_capacity": 192,
                "location_format": "S1-GENERAL" })
    );
    assert_eq!(sec["total_bins"], Value::Null);
    assert_eq!(sec["temperature_control"], "ambient");
    assert_eq!(sec["priority_level"], "standard");
    assert_eq!(sec["restricted_access"], false);
    assert_eq!(sec["assigned_manager"], Value::Null);
    assert_eq!(
        sec["relationships"],
        json!({ "warehouse": { "id": w.wh, "name": "Main" }, "assigned_manager": null })
    );

    let empty = w
        .sections()
        .create(
            json!({ "section_name": "E", "section_code": "E", "zone": "Z1" }),
            "section_id",
        )
        .await;
    let e = w.sections().get(&empty).await;
    assert_eq!(e["layout"]["total_capacity"], 0);
    assert_eq!(e["layout"]["total_bins"], 0);
    assert_eq!(e["layout"]["location_format"], "E-Z1");
    assert_eq!(
        data(&w.rows(&empty).list("").await).len(),
        0,
        "no layout, no rows"
    );
}

#[sqlx::test]
async fn row_letters_continue_past_z_and_layout_is_capped(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w
        .sections()
        .create(
            json!({ "section_name": "Long", "section_code": "L", "rows": 28, "shelves_per_row": 1,
                    "bins_per_shelf": 1 }),
            "section_id",
        )
        .await;
    let codes = field(&w.rows(&s).list("?page_size=100").await, "row_code");
    assert_eq!(&codes[24..], ["L-ROW-Y", "L-ROW-Z", "L-ROW-AA", "L-ROW-AB"]);

    let (status, res) = w
        .sections()
        .call(
            Method::POST,
            "",
            Some(
                json!({ "section_name": "Huge", "section_code": "H", "rows": 101,
                        "shelves_per_row": 10, "bins_per_shelf": 10 }),
            ),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["rows"],
        json!(["The section layout must not generate more than 10000 bins."])
    );
}

#[sqlx::test]
async fn codes_unique_per_parent_ignoring_case(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w.section("S1").await;

    let (status, res) = w
        .sections()
        .call(
            Method::POST,
            "",
            Some(json!({ "section_name": "x", "section_code": "s1" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["section_code"],
        json!(["The section code must be unique within this warehouse."])
    );
    let other_wh = w
        .api
        .create(json!({ "name": "Other" }), "warehouse_id")
        .await;
    w.api
        .with_base(base(format!("/warehouses/{other_wh}/sections")))
        .create(
            json!({ "section_name": "x", "section_code": "S1" }),
            "section_id",
        )
        .await;

    let r = w.row(&s, "R1").await;
    let (status, res) = w
        .rows(&s)
        .call(
            Method::POST,
            "",
            Some(
                json!({ "row_code": "r1", "row_name": "x", "length_meters": 1,
                        "width_meters": 1, "height_meters": 1 }),
            ),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["row_code"],
        json!(["The row code has already been taken."])
    );

    let sh = w.shelf(&s, &r, "SH1").await;
    let (status, res) = w
        .shelves(&s, &r)
        .call(
            Method::POST,
            "",
            Some(json!({ "shelf_code": "sh1", "shelf_level": 2 })),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["shelf_code"],
        json!(["The shelf code has already been taken."])
    );

    w.bin(&s, &r, &sh, "B1").await;
    let sh2 = w.shelf(&s, &r, "SH2").await;
    let (status, res) = w
        .bins(&s, &r, &sh2)
        .call(
            Method::POST,
            "",
            Some(json!({ "bin_code": "b1" })),
            &w.api.owner,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "bin codes unique per row"
    );
    assert_eq!(
        res["errors"]["bin_code"],
        json!(["The bin code has already been taken."])
    );

    // A deleted section frees its code.
    w.sections().delete(&s).await;
    w.section("S1").await;
}

#[sqlx::test]
async fn validation_wording(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w.section("S1").await;
    let r = w.row(&s, "R1").await;
    let sh = w.shelf(&s, &r, "SH1").await;
    let cases: Vec<(Api, Value, &str, Value)> = vec![
        (
            w.sections(),
            json!({ "section_code": "x" }),
            "section_name",
            json!(["The section name field is required."]),
        ),
        (
            w.sections(),
            json!({ "section_name": "x", "section_code": "c".repeat(51) }),
            "section_code",
            json!(["The section code must not be greater than 50 characters."]),
        ),
        (
            w.sections(),
            json!({ "section_name": "x", "section_code": "y", "priority_level": "urgent" }),
            "priority_level",
            json!(["Priority level must be one of: low, standard, high, critical."]),
        ),
        (
            w.sections(),
            json!({ "section_name": "x", "section_code": "y", "temperature_control": "hot" }),
            "temperature_control",
            json!(["Temperature control must be one of: ambient, cold, freezer, heated."]),
        ),
        (
            w.sections(),
            json!({ "section_name": "x", "section_code": "y", "rows": -1 }),
            "rows",
            json!(["The rows must be at least 0."]),
        ),
        (
            w.sections(),
            json!({ "section_name": "x", "section_code": "y", "restricted_access": "x" }),
            "restricted_access",
            json!(["The restricted access field must be true or false."]),
        ),
        (
            w.rows(&s),
            json!({ "row_code": "x", "row_name": "x", "width_meters": 1, "height_meters": 1 }),
            "length_meters",
            json!(["The length meters field is required."]),
        ),
        (
            w.rows(&s),
            json!({ "row_code": "x", "row_name": "x", "length_meters": -1,
                              "width_meters": 1, "height_meters": 1 }),
            "length_meters",
            json!(["The length meters must be at least 0."]),
        ),
        (
            w.rows(&s),
            json!({ "row_code": "x", "row_name": "x", "length_meters": 1, "width_meters": 1,
                              "height_meters": 1, "temperature_zone": "hot" }),
            "temperature_zone",
            json!(["The selected temperature zone is invalid."]),
        ),
        (
            w.shelves(&s, &r),
            json!({ "shelf_code": "x", "shelf_level": 0 }),
            "shelf_level",
            json!(["The shelf level must be at least 1."]),
        ),
        (
            w.shelves(&s, &r),
            json!({ "shelf_code": "x", "shelf_level": 1, "side": "top" }),
            "side",
            json!(["The selected side is invalid."]),
        ),
        (
            w.bins(&s, &r, &sh),
            json!({ "bin_code": "x", "bin_type": "huge" }),
            "bin_type",
            json!(["The selected bin type is invalid."]),
        ),
        (
            w.bins(&s, &r, &sh),
            json!({ "bin_code": "x", "access_type": "drone" }),
            "access_type",
            json!(["The selected access type is invalid."]),
        ),
        (
            w.bins(&s, &r, &sh),
            json!({ "bin_code": "x", "volume_m3": "big" }),
            "volume_m3",
            json!(["The volume m3 must be a number."]),
        ),
    ];
    for (api, body, key, expected) in cases {
        let (status, res) = api
            .call(Method::POST, "", Some(body.clone()), &w.api.owner)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert_eq!(res["errors"][key], expected, "{body}: {res}");
    }
}

#[sqlx::test]
async fn row_shelf_bin_resources(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w
        .sections()
        .create(
            json!({ "section_name": "Hall", "section_code": "S1", "bins_per_shelf": 4 }),
            "section_id",
        )
        .await;
    let (status, res) = w
        .rows(&s)
        .call(
            Method::POST,
            "",
            Some(
                json!({ "row_code": "R1", "row_name": "Row 1", "length_meters": 10,
                        "width_meters": "1.5", "height_meters": 6, "number_of_shelves": 3,
                        "position_x": 2.5, "notes": "n", "generate_qr_codes_for_shelves": true }),
            ),
            &w.api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let r = res["data"]["row_id"].as_str().unwrap().to_owned();
    assert_eq!(res["data"]["warehouse_section_row_id"], r.as_str());

    let row = w.rows(&s).get(&r).await;
    assert_eq!(row["workspace_id"], w.api.ws.as_str());
    assert_eq!(
        (row["warehouse_id"].clone(), row["section_id"].clone()),
        (json!(w.wh), json!(s))
    );
    assert_eq!(row["physical_dimensions"]["width_meters"], 1.5);
    assert_eq!(row["bins_per_shelf"], 4);
    assert_eq!(row["total_bins"], 12);
    assert_eq!(row["utilization"], 0.0);
    assert_eq!(
        row["location_access"]["starting_position"],
        json!({ "x": 2.5, "y": null })
    );
    assert_eq!(row["additional_settings"]["priority_level"], "standard");
    assert_eq!(
        row["additional_settings"]["generate_qr_codes_for_shelves"],
        true
    );
    assert_eq!(
        row["relationships"],
        json!({ "warehouse": { "id": w.wh, "name": "Main" }, "assigned_manager": null })
    );

    let sh = w.shelf(&s, &r, "SH1").await;
    let shelf = w.shelves(&s, &r).get(&sh).await;
    assert_eq!(shelf["shelf_id"], sh.as_str());
    assert_eq!(
        (
            shelf["warehouse_id"].clone(),
            shelf["section_id"].clone(),
            shelf["row_id"].clone()
        ),
        (json!(w.wh), json!(s), json!(r))
    );
    assert_eq!(shelf["shelf_name"], Value::Null);

    let b = w.bin(&s, &r, &sh, "B1").await;
    let bin = w.bins(&s, &r, &sh).get(&b).await;
    assert_eq!(
        (bin["bin_id"].clone(), bin["uuid"].clone()),
        (json!(b), json!(b))
    );
    assert_eq!(bin["shelf_id"], sh.as_str());
    assert_eq!(bin["bin_type"], "standard");
    assert_eq!(bin["access_type"], "manual_access");
    assert_eq!(
        bin["access_configuration"]["special_features"]["rfid_enabled"],
        false
    );
    assert_eq!(bin["stocks"], json!([]));
    let listed = w.bins(&s, &r, &sh).list("").await;
    assert!(
        data(&listed)[0].get("stocks").is_none(),
        "stocks only on GET one"
    );
}

#[sqlx::test]
async fn updates_are_patches(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w
        .sections()
        .create(
            json!({ "section_name": "Hall", "section_code": "S1", "zone": "Z",
                    "temperature_control": "cold", "priority_level": "high" }),
            "section_id",
        )
        .await;

    let (status, res) = w
        .sections()
        .update(
            &s,
            json!({ "section_name": "Hall 2", "section_code": "S1" }),
        )
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let sec = w.sections().get(&s).await;
    assert_eq!(sec["section_name"], "Hall 2");
    assert_eq!(sec["temperature_control"], "cold", "absent key kept");
    assert_eq!(sec["zone"], "Z");

    let r = w.row(&s, "R1").await;
    let (status, _) = w
        .rows(&s)
        .update(&r, json!({ "row_name": "Renamed" }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let row = w.rows(&s).get(&r).await;
    assert_eq!(
        (row["row_name"].clone(), row["row_code"].clone()),
        (json!("Renamed"), json!("R1"))
    );

    let sh = w.shelf(&s, &r, "SH1").await;
    let (status, _) = w
        .shelves(&s, &r)
        .update(&sh, json!({ "notes": "top" }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(w.shelves(&s, &r).get(&sh).await["notes"], "top");

    let b = w.bin(&s, &r, &sh, "B1").await;
    let (status, _) = w
        .bins(&s, &r, &sh)
        .update(&b, json!({ "bin_name": "First" }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(w.bins(&s, &r, &sh).get(&b).await["bin_name"], "First");

    // Renaming onto a sibling's code is the same 422 as create.
    w.row(&s, "R2").await;
    let (status, _) = w.rows(&s).update(&r, json!({ "row_code": "r2" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn section_list_sorts_by_name_and_filters(pool: PgPool) {
    let w = World::new(pool).await;
    for (name, code, cat) in [
        ("Charlie", "C", "storage"),
        ("Alpha", "A", "picking"),
        ("Bravo", "B", "storage"),
    ] {
        w.sections()
            .create(
                json!({ "section_name": name, "section_code": code, "category": cat }),
                "section_id",
            )
            .await;
    }

    let all = w.sections().list("").await;
    assert_eq!(
        field(&all, "section_name"),
        vec!["Alpha", "Bravo", "Charlie"]
    );
    assert_eq!(all["meta"]["per_page"], 15);
    assert_eq!(
        w.sections().list("?per_page=0").await["meta"]["per_page"],
        15
    );
    assert_eq!(
        field(
            &w.sections().list("?category=storage").await,
            "section_code"
        ),
        vec!["B", "C"]
    );
    assert_eq!(
        field(&w.sections().list("?search=rav").await, "section_code"),
        vec!["B"]
    );
    assert_eq!(
        w.sections().list("?per_page=2").await["meta"]["last_page"],
        2
    );
}

#[sqlx::test]
async fn deleted_ancestor_hides_descendants(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w.section("S1").await;
    let r = w.row(&s, "R1").await;
    let sh = w.shelf(&s, &r, "SH1").await;
    let b = w.bin(&s, &r, &sh, "B1").await;

    w.rows(&s).delete(&r).await;

    for (api, id) in [(w.shelves(&s, &r), &sh), (w.bins(&s, &r, &sh), &b)] {
        for (method, path) in [
            (Method::GET, String::new()),
            (Method::GET, format!("/{id}")),
        ] {
            let (status, _) = api.call(method, &path, None, &w.api.owner).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
    }
    // A bin reached through a live row whose shelf list doesn't hold it is a 404 too.
    let r2 = w.row(&s, "R2").await;
    let sh2 = w.shelf(&s, &r2, "SH2").await;
    let b2 = w.bin(&s, &r2, &sh2, "B2").await;
    let (status, _) = w
        .bins(&s, &r2, &sh2)
        .call(Method::GET, &format!("/{b2}"), None, &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "control: right path works");
    let sh3 = w.shelf(&s, &r2, "SH3").await;
    let (status, _) = w
        .bins(&s, &r2, &sh3)
        .call(Method::GET, &format!("/{b2}"), None, &w.api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "bin under another shelf");
}

#[sqlx::test]
async fn access_rules_every_level(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w.section("S1").await;
    let r = w.row(&s, "R1").await;
    let sh = w.shelf(&s, &r, "SH1").await;
    let b = w.bin(&s, &r, &sh, "B1").await;
    let perms = ["warehouse.get", "warehouse.update"];

    let gone_s = w.section("GONE").await;
    w.sections().delete(&gone_s).await;
    w.sections().assert_member_forbidden(&s).await;
    w.sections()
        .assert_bad_ids_404(&gone_s, json!({ "section_name": "x", "section_code": "x" }))
        .await;

    // One member holding just the two warehouse names can work every lower level too.
    let carl = member_with(
        &w.api.app,
        &w.api.pool,
        &w.api.ws,
        "carl@example.com",
        &perms,
    )
    .await;
    let row_body = json!({ "row_code": "G", "row_name": "G", "length_meters": 1,
                           "width_meters": 1, "height_meters": 1 });
    for (api, body, key) in [
        (
            w.sections(),
            json!({ "section_name": "G", "section_code": "G" }),
            "section_id",
        ),
        (w.rows(&s), row_body, "row_id"),
        (
            w.shelves(&s, &r),
            json!({ "shelf_code": "G", "shelf_level": 1 }),
            "shelf_id",
        ),
        (w.bins(&s, &r, &sh), json!({ "bin_code": "G" }), "bin_id"),
    ] {
        let (status, res) = api.call(Method::POST, "", Some(body.clone()), &carl).await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        let id = res["data"][key].as_str().unwrap().to_owned();
        for (method, path, body) in [
            (Method::GET, String::new(), None),
            (Method::GET, format!("/{id}"), None),
            (Method::POST, format!("/{id}"), Some(body)),
            (Method::DELETE, format!("/{id}"), None),
        ] {
            let (status, res) = api.call(method.clone(), &path, body, &carl).await;
            assert_eq!(status, StatusCode::OK, "{method} {path}: {res}");
        }
    }
    let (status, _) = w
        .api
        .with_base(base(format!("/warehouses/{}/bins", w.wh)))
        .call(Method::GET, "", None, &carl)
        .await;
    assert_eq!(status, StatusCode::OK);

    let (eve, eve_ws) = signup(&w.api.app, "eve@example.com", "evil").await;
    let wh = &w.wh;
    for path in [
        format!("/warehouses/{wh}/sections/{s}"),
        format!("/warehouses/{wh}/sections/{s}/rows/{r}"),
        format!("/warehouses/{wh}/sections/{s}/rows/{r}/shelves/{sh}"),
        format!("/warehouses/{wh}/sections/{s}/rows/{r}/shelves/{sh}/bins/{b}"),
        format!("/warehouses/{wh}/bins"),
    ] {
        let uri = format!("/v1/workspace/{eve_ws}{path}");
        let (status, _) = send(&w.api.app, Method::GET, &uri, None, Some(&eve)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    assert_eq!(w.bins(&s, &r, &sh).get(&b).await["bin_code"], "B1");
}

#[sqlx::test]
async fn huge_section_counts_never_overflow(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w.section("S1").await;
    let max = i32::MAX;

    let (status, res) = w
        .sections()
        .update(
            &s,
            json!({ "section_name": "S1", "section_code": "S1", "rows": max,
                    "shelves_per_row": max, "bins_per_shelf": max }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let sec = w.sections().get(&s).await;
    assert_eq!(sec["layout"]["total_capacity"], i64::MAX);
    assert_eq!(
        sec["layout"]["total_shelves"],
        i64::from(max) * i64::from(max)
    );
}

#[sqlx::test]
async fn update_of_unknown_id_is_404_even_with_a_taken_code(pool: PgPool) {
    let w = World::new(pool).await;
    let s = w.section("S1").await;
    let r = w.row(&s, "R1").await;
    let sh = w.shelf(&s, &r, "SH1").await;
    w.bin(&s, &r, &sh, "B1").await;
    let unknown = "01900000-0000-7000-8000-000000000001";

    for (api, body) in [
        (
            w.sections(),
            json!({ "section_name": "x", "section_code": "S1" }),
        ),
        (w.rows(&s), json!({ "row_code": "R1" })),
        (w.shelves(&s, &r), json!({ "shelf_code": "SH1" })),
        (w.bins(&s, &r, &sh), json!({ "bin_code": "B1" })),
    ] {
        let (status, res) = api.update(unknown, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{res}");
    }
}

#[sqlx::test]
async fn warehouse_bin_list_flags_truncation(pool: PgPool) {
    let w = World::new(pool.clone()).await;
    let s = w.section("S1").await;
    let r = w.row(&s, "R1").await;
    let sh = w.shelf(&s, &r, "SH1").await;
    // One past the cap, straight in SQL.
    sqlx::query(
        "INSERT INTO warehouse_section_row_bins (workspace_id, warehouse_id, section_id, row_id,
             shelf_id, bin_code)
         SELECT $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'B' || g
         FROM generate_series(1, 10001) g",
    )
    .bind(&w.api.ws)
    .bind(&w.wh)
    .bind(&s)
    .bind(&r)
    .bind(&sh)
    .execute(&pool)
    .await
    .unwrap();
    let list = w.api.with_base(base(format!("/warehouses/{}/bins", w.wh)));

    let res = list.list("").await;
    assert_eq!(data(&res).len(), 10_000);
    assert_eq!(res["meta"], json!({ "truncated": true }));

    sqlx::query("UPDATE warehouse_section_row_bins SET deleted_at = now() WHERE bin_code = 'B1'")
        .execute(&pool)
        .await
        .unwrap();
    let res = list.list("").await;
    assert_eq!(data(&res).len(), 10_000);
    assert!(res.get("meta").is_none(), "exactly at the cap is complete");
}
