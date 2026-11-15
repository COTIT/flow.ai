//! `tax_line`: one line's tax per component of a verified tax profile, computed in SQL.

use crate::common::{app, signup};
use sqlx::PgPool;
use uuid::Uuid;

/// A workspace with one product category and one customer.
struct World {
    pool: PgPool,
    ws: Uuid,
    category: Uuid,
    relation: Uuid,
}

async fn world(pool: PgPool, email: &str, slug: &str) -> World {
    let (_, ws) = signup(&app(pool.clone()), email, slug).await;
    let ws: Uuid = ws.parse().unwrap();
    let category = sqlx::query_scalar(
        "INSERT INTO product_categories (workspace_id, name) VALUES ($1, 'Seeds') RETURNING id",
    )
    .bind(ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let ty: Uuid = sqlx::query_scalar(
        "SELECT id FROM custom_types WHERE workspace_id = $1 AND domain = 'relation' LIMIT 1",
    )
    .bind(ws)
    .fetch_one(&pool)
    .await
    .unwrap();
    let relation = sqlx::query_scalar(
        "INSERT INTO relations (workspace_id, relation_type_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(ws)
    .bind(ty)
    .fetch_one(&pool)
    .await
    .unwrap();
    World {
        pool,
        ws,
        category,
        relation,
    }
}

/// Profile shape for a test; `components` = (code, rate, recoverable).
struct Spec<'a> {
    components: &'a [(&'a str, &'a str, bool)],
    from: &'a str,
    to: Option<&'a str>,
    types: &'a [&'a str],
    categories: Vec<Uuid>,
    verified: bool,
}

impl Default for Spec<'_> {
    fn default() -> Self {
        Self {
            components: &[("VAT", "18", false)],
            from: "2027-01-01",
            to: None,
            types: &["sale", "purchase"],
            categories: vec![],
            verified: true,
        }
    }
}

impl World {
    async fn profile(&self, s: Spec<'_>) -> Uuid {
        let id: Uuid = sqlx::query_scalar(
            "WITH n AS (SELECT uuidv7() AS id)
             INSERT INTO tax_profiles (id, workspace_id, lineage_id, country, transaction_types,
                                       product_category_ids, effective_from, effective_to,
                                       tax_point, created_by)
             SELECT n.id, $1, n.id, 'TZ', $2, $3, $4::date, $5::date, 'invoice',
                    (SELECT created_by_user_id FROM workspaces WHERE id = $1)
             FROM n RETURNING id",
        )
        .bind(self.ws)
        .bind(s.types)
        .bind(&s.categories)
        .bind(s.from)
        .bind(s.to)
        .fetch_one(&self.pool)
        .await
        .unwrap();
        for (i, (code, rate, recoverable)) in s.components.iter().enumerate() {
            sqlx::query(
                "INSERT INTO tax_rules (workspace_id, profile_id, line_no, code, name, rate,
                                        recoverable)
                 VALUES ($1, $2, $3, $4, $4, $5::numeric, $6)",
            )
            .bind(self.ws)
            .bind(id)
            .bind(i16::try_from(i + 1).unwrap())
            .bind(code)
            .bind(rate)
            .bind(recoverable)
            .execute(&self.pool)
            .await
            .unwrap();
        }
        if s.verified {
            self.verify(id).await;
        }
        id
    }

    async fn verify(&self, id: Uuid) {
        sqlx::query(
            "UPDATE tax_profiles SET state = 'verified', evidence_ref = 'TRA notice 2027/1',
                    verified_by = created_by, verified_at = now(),
                    verify_fingerprint = sha256('x'::bytea)
             WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn exempt(&self, profile: Uuid, relation: Option<Uuid>, category: Option<Uuid>) {
        sqlx::query(
            "INSERT INTO tax_exemptions (workspace_id, profile_id, relation_id, product_category_id,
                                         reason, evidence_ref)
             VALUES ($1, $2, $3, $4, 'Certificate', 'EX-1')",
        )
        .bind(self.ws)
        .bind(profile)
        .bind(relation)
        .bind(category)
        .execute(&self.pool)
        .await
        .unwrap();
    }
}

/// One component row: (code, base, tax, recoverable, exempt).
type Row = (String, i64, i64, bool, bool);

/// Call options; the defaults are a 2027-06-01 sale with no category or relation.
struct Call<'a> {
    on: &'a str,
    txn: &'a str,
    category: Option<Uuid>,
    relation: Option<Uuid>,
    ws: Option<Uuid>,
}

impl Default for Call<'_> {
    fn default() -> Self {
        Self {
            on: "2027-06-01",
            txn: "sale",
            category: None,
            relation: None,
            ws: None,
        }
    }
}

async fn line(
    w: &World,
    profile: Uuid,
    amount: i64,
    includes_tax: bool,
    c: Call<'_>,
) -> Result<Vec<Row>, sqlx::Error> {
    sqlx::query_as(
        "SELECT code, base, tax, recoverable, exempt
         FROM tax_line($1, $2, $3::date, $4, $5, $6, $7, $8)",
    )
    .bind(c.ws.unwrap_or(w.ws))
    .bind(profile)
    .bind(c.on)
    .bind(c.txn)
    .bind(amount)
    .bind(includes_tax)
    .bind(c.category)
    .bind(c.relation)
    .fetch_all(&w.pool)
    .await
}

async fn ok(w: &World, profile: Uuid, amount: i64, includes_tax: bool) -> Vec<(i64, i64)> {
    line(w, profile, amount, includes_tax, Call::default())
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.1, r.2))
        .collect()
}

/// The refusal message (raised with SQLSTATE 22023).
async fn refused(w: &World, profile: Uuid, c: Call<'_>) -> String {
    let err = line(w, profile, 1000, false, c).await.unwrap_err();
    let db = err.as_database_error().unwrap();
    assert_eq!(db.code().as_deref(), Some("22023"), "{db}");
    db.message().to_owned()
}

#[sqlx::test]
async fn exclusive_amount_adds_tax_on_top(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w.profile(Spec::default()).await;
    assert_eq!(ok(&w, p, 1000, false).await, [(1000, 180)]);
}

#[sqlx::test]
async fn ties_round_away_from_zero_both_signs(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            components: &[("VAT", "21", false)],
            ..Spec::default()
        })
        .await;
    // 250 × 21 % = 52.5
    assert_eq!(ok(&w, p, 250, false).await, [(250, 53)]);
    assert_eq!(ok(&w, p, -250, false).await, [(-250, -53)]);
}

#[sqlx::test]
async fn inclusive_amount_splits_into_base_and_tax(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p18 = w.profile(Spec::default()).await;
    assert_eq!(ok(&w, p18, 1180, true).await, [(1000, 180)]);
    let p21 = w
        .profile(Spec {
            components: &[("VAT", "21", false)],
            from: "2026-01-01",
            ..Spec::default()
        })
        .await;
    // 100 / 1.21 = 82.64 → base 83, tax 17; 1 / 1.21 = 0.83 → base 1, tax 0.
    assert_eq!(ok(&w, p21, 100, true).await, [(83, 17)]);
    assert_eq!(ok(&w, p21, 1, true).await, [(1, 0)]);
    assert_eq!(ok(&w, p21, -100, true).await, [(-83, -17)]);
}

#[sqlx::test]
async fn inclusive_components_take_the_exact_remainder(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            components: &[("STATE", "10", false), ("CITY", "5", false), ("ZERO", "0", false)],
            ..Spec::default()
        })
        .await;
    // 1000 / 1.15 = 869.57 → base 870, tax 130: STATE 87.0 → 87; CITY takes 130 − 87 = 43
    // (alone it would round 43.5 → 44); the 0 % component never takes the remainder.
    let got = ok(&w, p, 1000, true).await;
    assert_eq!(got, [(870, 87), (870, 43), (870, 0)]);
    let tax: i64 = got.iter().map(|r| r.1).sum();
    assert_eq!(got[0].0 + tax, 1000);
}

#[sqlx::test]
async fn exclusive_components_round_independently(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            components: &[("STATE", "10", false), ("CITY", "5", false)],
            ..Spec::default()
        })
        .await;
    // 33.3 → 33, 16.65 → 17.
    assert_eq!(ok(&w, p, 333, false).await, [(333, 33), (333, 17)]);
}

#[sqlx::test]
async fn eight_decimal_rates_stay_exact(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            components: &[("VAT", "12.34567891", false)],
            ..Spec::default()
        })
        .await;
    // 10^8 × 12.34567891 % = 12345678.91 → 12345679.
    assert_eq!(
        ok(&w, p, 100_000_000, false).await,
        [(100_000_000, 12_345_679)]
    );
    // Largest amount: tax never exceeds the amount, so it fits bigint.
    let p100 = w
        .profile(Spec {
            components: &[("ALL", "100", false)],
            from: "2026-01-01",
            ..Spec::default()
        })
        .await;
    assert_eq!(ok(&w, p100, i64::MAX, false).await, [(i64::MAX, i64::MAX)]);
}

#[sqlx::test]
async fn recoverability_passes_through(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            components: &[("VAT", "18", true), ("LEVY", "2", false)],
            ..Spec::default()
        })
        .await;
    let rows = line(
        &w,
        p,
        1000,
        false,
        Call {
            txn: "purchase",
            ..Call::default()
        },
    )
    .await
    .unwrap();
    let flags: Vec<_> = rows.iter().map(|r| (r.0.as_str(), r.3)).collect();
    assert_eq!(flags, [("VAT", true), ("LEVY", false)]);
}

#[sqlx::test]
async fn exemptions_zero_every_component(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            components: &[("VAT", "18", false), ("LEVY", "2", false)],
            verified: false,
            ..Spec::default()
        })
        .await;
    w.exempt(p, Some(w.relation), None).await;
    w.exempt(p, None, Some(w.category)).await;
    w.verify(p).await;
    for c in [
        Call {
            relation: Some(w.relation),
            ..Call::default()
        },
        Call {
            category: Some(w.category),
            ..Call::default()
        },
    ] {
        let rows = line(&w, p, 1180, true, c).await.unwrap();
        let got: Vec<_> = rows.iter().map(|r| (r.1, r.2, r.4)).collect();
        assert_eq!(got, [(1180, 0, true), (1180, 0, true)]);
    }
    // No match: taxed as normal.
    let rows = line(&w, p, 1000, false, Call::default()).await.unwrap();
    let got: Vec<_> = rows.iter().map(|r| (r.2, r.4)).collect();
    assert_eq!(got, [(180, false), (20, false)]);
}

#[sqlx::test]
async fn effective_dates_include_start_and_exclude_end(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            to: Some("2028-01-01"),
            ..Spec::default()
        })
        .await;
    let on = |on| Call {
        on,
        ..Call::default()
    };
    assert!(line(&w, p, 1000, false, on("2027-01-01")).await.is_ok());
    assert!(line(&w, p, 1000, false, on("2027-12-31")).await.is_ok());
    assert_eq!(
        refused(&w, p, on("2028-01-01")).await,
        "tax_profile_not_effective"
    );
    assert_eq!(
        refused(&w, p, on("2026-12-31")).await,
        "tax_profile_not_effective"
    );
}

#[sqlx::test]
async fn draft_profiles_are_refused(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            verified: false,
            ..Spec::default()
        })
        .await;
    assert_eq!(
        refused(&w, p, Call::default()).await,
        "tax_profile_not_verified"
    );
}

#[sqlx::test]
async fn transaction_type_and_category_scope_are_enforced(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            types: &["sale"],
            categories: vec![w.category],
            ..Spec::default()
        })
        .await;
    let in_scope = Call {
        category: Some(w.category),
        ..Call::default()
    };
    assert!(line(&w, p, 1000, false, in_scope).await.is_ok());
    assert_eq!(
        refused(
            &w,
            p,
            Call {
                txn: "purchase",
                category: Some(w.category),
                ..Call::default()
            }
        )
        .await,
        "tax_profile_wrong_transaction"
    );
    assert_eq!(
        refused(&w, p, Call::default()).await,
        "tax_profile_wrong_category"
    );
    assert_eq!(
        refused(
            &w,
            p,
            Call {
                category: Some(Uuid::now_v7()),
                ..Call::default()
            }
        )
        .await,
        "tax_profile_wrong_category"
    );
}

#[sqlx::test]
async fn another_workspaces_profile_is_invisible(pool: PgPool) {
    let w = world(pool.clone(), "ada@example.com", "acme").await;
    let other = world(pool, "bob@example.com", "bobco").await;
    let p = w.profile(Spec::default()).await;
    assert_eq!(
        refused(
            &w,
            p,
            Call {
                ws: Some(other.ws),
                ..Call::default()
            }
        )
        .await,
        "tax_profile_not_verified"
    );
}

#[sqlx::test]
async fn verified_content_is_immutable(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w.profile(Spec::default()).await;
    for sql in [
        "UPDATE tax_profiles SET country = 'KE' WHERE id = $1",
        "UPDATE tax_rules SET rate = 20 WHERE profile_id = $1",
        "DELETE FROM tax_rules WHERE profile_id = $1",
        "INSERT INTO tax_exemptions (workspace_id, profile_id, product_category_id, reason,
                                     evidence_ref)
         SELECT workspace_id, id, NULL, 'x', 'y' FROM tax_profiles WHERE id = $1",
    ] {
        let err = sqlx::query(sql)
            .bind(p)
            .execute(&w.pool)
            .await
            .unwrap_err();
        let code = err.as_database_error().and_then(|d| d.code());
        assert!(
            matches!(code.as_deref(), Some("23514" | "55000")),
            "{sql}: {err}"
        );
    }
    // Only the one-time supersede link may be set.
    let next = w
        .profile(Spec {
            from: "2028-01-01",
            verified: false,
            ..Spec::default()
        })
        .await;
    sqlx::query("UPDATE tax_profiles SET superseded_by_id = $2 WHERE id = $1")
        .bind(p)
        .bind(next)
        .execute(&w.pool)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE tax_profiles SET superseded_by_id = NULL WHERE id = $1")
            .bind(p)
            .execute(&w.pool)
            .await
            .is_err()
    );
}

#[sqlx::test]
async fn fixed_and_compound_components_cannot_be_stored(pool: PgPool) {
    let w = world(pool, "ada@example.com", "acme").await;
    let p = w
        .profile(Spec {
            verified: false,
            ..Spec::default()
        })
        .await;
    for kind in ["fixed", "compound"] {
        let err = sqlx::query(
            "INSERT INTO tax_rules (workspace_id, profile_id, line_no, code, name, kind, rate,
                                    recoverable)
             VALUES ($1, $2, 9, 'X', 'X', $3, 1, false)",
        )
        .bind(w.ws)
        .bind(p)
        .bind(kind)
        .execute(&w.pool)
        .await
        .unwrap_err();
        let code = err.as_database_error().and_then(|d| d.code());
        assert_eq!(code.as_deref(), Some("23514"), "{kind}");
    }
}
