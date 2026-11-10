//! Targeted design changes: one pure apply for write, dry run and review facts.

use super::dto::*;
use crate::AppError;
use serde::Serialize;

/// Removed value on review rows.
pub const REMOVED: &str = "—";

type Rows = Vec<StorefrontDesignChangeRow>;

fn name(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
fn kind(s: &StorefrontDesignSection) -> &'static str {
    match s {
        StorefrontDesignSection::Hero { .. } => "hero",
        StorefrontDesignSection::ProductGrid { .. } => "product_grid",
    }
}
fn heading(s: &StorefrontDesignSection) -> &str {
    match s {
        StorefrontDesignSection::Hero { props, .. } => &props.heading,
        StorefrontDesignSection::ProductGrid { props, .. } => &props.heading,
    }
}
fn columns(c: &StorefrontDesignColumns) -> String {
    format!("{} / {} / {}", c.mobile, c.tablet, c.desktop)
}
fn push(rows: &mut Rows, field: &str, label: Option<&str>, before: Option<String>, after: String) {
    if before.as_deref() != Some(after.as_str()) {
        rows.push(StorefrontDesignChangeRow {
            field: field.into(),
            label: label.map(str::to_owned),
            before,
            after,
            category_id: None,
        });
    }
}
fn find(doc: &StorefrontDesignDocument, i: usize, id: &str) -> Result<usize, AppError> {
    doc.sections
        .iter()
        .position(|s| s.id() == id)
        .ok_or_else(|| {
            AppError::field(
                &format!("changes.{i}.section_id"),
                "Choose an existing section.",
            )
        })
}
fn position(i: usize, value: u8, max: usize) -> Result<usize, AppError> {
    let p = usize::from(value);
    if (1..=max).contains(&p) {
        Ok(p - 1)
    } else {
        Err(AppError::field(
            &format!("changes.{i}.position"),
            format!("Use a position from 1 to {max}."),
        ))
    }
}
fn nothing(i: usize) -> AppError {
    AppError::field(
        &format!("changes.{i}"),
        "Name at least one property to change.",
    )
}

/// Apply changes to base copy, validate result, return document plus review rows.
pub fn apply(
    base: &StorefrontDesignDocument,
    changes: &[StorefrontDesignChange],
) -> Result<(StorefrontDesignDocument, Rows), AppError> {
    let mut doc = base.clone();
    let mut rows = Rows::new();
    for (i, change) in changes.iter().enumerate() {
        step(&mut doc, &mut rows, i, change)?;
    }
    doc.check()?;
    let same = serde_json::to_value(&doc).ok() == serde_json::to_value(base).ok();
    if same || rows.is_empty() {
        return Err(AppError::field(
            "changes",
            "This change leaves the design as it is.",
        ));
    }
    Ok((doc, rows))
}

fn step(
    doc: &mut StorefrontDesignDocument,
    rows: &mut Rows,
    i: usize,
    change: &StorefrontDesignChange,
) -> Result<(), AppError> {
    use StorefrontDesignChange as C;
    use StorefrontDesignSection as S;
    match change {
        C::SetTheme { theme } => {
            push(rows, "theme", None, Some(name(&doc.theme)), name(theme));
            doc.theme = theme.clone();
        }
        C::SetHero {
            section_id,
            heading: h,
            body,
            layout,
        } => {
            if h.is_none() && body.is_none() && layout.is_none() {
                return Err(nothing(i));
            }
            let at = find(doc, i, section_id)?;
            let S::Hero { props, .. } = &mut doc.sections[at] else {
                return Err(AppError::field(
                    &format!("changes.{i}.section_id"),
                    "Choose a hero section.",
                ));
            };
            let label = props.heading.clone();
            if let Some(h) = h {
                push(
                    rows,
                    "heading",
                    Some(&label),
                    Some(props.heading.clone()),
                    h.clone(),
                );
                props.heading = h.clone();
            }
            if let Some(b) = body {
                push(rows, "body", Some(&label), props.body.clone(), b.clone());
                props.body = Some(b.clone());
            }
            if let Some(l) = layout {
                push(
                    rows,
                    "layout",
                    Some(&label),
                    Some(name(&props.layout)),
                    name(l),
                );
                props.layout = l.clone();
            }
        }
        C::ClearHeroBody { section_id } => {
            let at = find(doc, i, section_id)?;
            let S::Hero { props, .. } = &mut doc.sections[at] else {
                return Err(AppError::field(
                    &format!("changes.{i}.section_id"),
                    "Choose a hero section.",
                ));
            };
            if props.body.is_some() {
                push(
                    rows,
                    "body",
                    Some(&props.heading.clone()),
                    props.body.take(),
                    REMOVED.into(),
                );
            }
        }
        C::SetProductGrid {
            section_id,
            heading: h,
            category_id,
            limit,
            columns: c,
        } => {
            if h.is_none() && category_id.is_none() && limit.is_none() && c.is_none() {
                return Err(nothing(i));
            }
            let props = grid(doc, i, section_id)?;
            let label = props.heading.clone();
            if let Some(h) = h {
                push(
                    rows,
                    "heading",
                    Some(&label),
                    Some(props.heading.clone()),
                    h.clone(),
                );
                props.heading = h.clone();
            }
            if let Some(id) = category_id {
                push_category(rows, Some(&label), props.category_id, Some(*id));
                props.category_id = Some(*id);
            }
            if let Some(n) = limit {
                push(
                    rows,
                    "limit",
                    Some(&label),
                    Some(props.limit.to_string()),
                    n.to_string(),
                );
                props.limit = *n;
            }
            if let Some(c) = c {
                push(
                    rows,
                    "columns",
                    Some(&label),
                    Some(columns(&props.columns)),
                    columns(c),
                );
                props.columns = c.clone();
            }
        }
        C::ShowAllProducts { section_id } => {
            let props = grid(doc, i, section_id)?;
            let before = props.category_id.take();
            push_category(rows, Some(&props.heading.clone()), before, None);
        }
        C::AddSection {
            section,
            position: p,
        } => {
            let len = doc.sections.len();
            let at = match p {
                Some(p) => position(i, *p, len + 1)?,
                None => len,
            };
            added(rows, section);
            doc.sections.insert(at, section.clone());
        }
        C::RemoveSection { section_id } => {
            let at = find(doc, i, section_id)?;
            let gone = doc.sections.remove(at);
            push(
                rows,
                "section",
                Some(heading(&gone)),
                Some(kind(&gone).into()),
                REMOVED.into(),
            );
        }
        C::MoveSection {
            section_id,
            position: p,
        } => {
            let from = find(doc, i, section_id)?;
            let to = position(i, *p, doc.sections.len())?;
            let moved = doc.sections.remove(from);
            push(
                rows,
                "position",
                Some(heading(&moved)),
                Some((from + 1).to_string()),
                (to + 1).to_string(),
            );
            doc.sections.insert(to, moved);
        }
    }
    Ok(())
}

/// Rows turning `before` into `after`; restore and publish reviews. `None` = nothing yet.
pub fn diff(before: Option<&StorefrontDesignDocument>, after: &StorefrontDesignDocument) -> Rows {
    use StorefrontDesignSection as S;
    let mut rows = Rows::new();
    push(
        &mut rows,
        "theme",
        None,
        before.map(|b| name(&b.theme)),
        name(&after.theme),
    );
    let old = before.map_or(&[][..], |b| &b.sections[..]);
    let common = |list: &[S], other: &[S]| -> Vec<String> {
        list.iter()
            .filter(|s| other.iter().any(|o| o.id() == s.id() && kind(o) == kind(s)))
            .map(|s| s.id().to_owned())
            .collect()
    };
    let (was, now) = (common(old, &after.sections), common(&after.sections, old));
    for s in &after.sections {
        match old.iter().find(|o| o.id() == s.id() && kind(o) == kind(s)) {
            None => added(&mut rows, s),
            Some(o) => {
                let label = Some(heading(o));
                match (o, s) {
                    (S::Hero { props: b, .. }, S::Hero { props: a, .. }) => {
                        push(
                            &mut rows,
                            "heading",
                            label,
                            Some(b.heading.clone()),
                            a.heading.clone(),
                        );
                        if b.body != a.body {
                            rows.push(row("body", label, b.body.clone(), a.body.clone()));
                        }
                        push(
                            &mut rows,
                            "layout",
                            label,
                            Some(name(&b.layout)),
                            name(&a.layout),
                        );
                    }
                    (S::ProductGrid { props: b, .. }, S::ProductGrid { props: a, .. }) => {
                        push(
                            &mut rows,
                            "heading",
                            label,
                            Some(b.heading.clone()),
                            a.heading.clone(),
                        );
                        push_category(&mut rows, label, b.category_id, a.category_id);
                        push(
                            &mut rows,
                            "limit",
                            label,
                            Some(b.limit.to_string()),
                            a.limit.to_string(),
                        );
                        push(
                            &mut rows,
                            "columns",
                            label,
                            Some(columns(&b.columns)),
                            columns(&a.columns),
                        );
                    }
                    _ => {}
                }
                let (from, to) = (
                    was.iter().position(|id| id == s.id()),
                    now.iter().position(|id| id == s.id()),
                );
                if let (Some(f), Some(t)) = (from, to)
                    && f != t
                {
                    push(
                        &mut rows,
                        "position",
                        label,
                        Some((f + 1).to_string()),
                        (t + 1).to_string(),
                    );
                }
            }
        }
    }
    for o in old {
        if !after
            .sections
            .iter()
            .any(|s| s.id() == o.id() && kind(s) == kind(o))
        {
            push(
                &mut rows,
                "section",
                Some(heading(o)),
                Some(kind(o).into()),
                REMOVED.into(),
            );
        }
    }
    rows
}
/// Category change carrying stored id; `after` later becomes its name.
fn push_category(
    rows: &mut Rows,
    label: Option<&str>,
    before: Option<uuid::Uuid>,
    after: Option<uuid::Uuid>,
) {
    if before != after {
        let mut r = row(
            "category",
            label,
            before.map(|v| v.to_string()),
            after.map(|v| v.to_string()),
        );
        r.category_id = after;
        rows.push(r);
    }
}
/// New section: kind row, then every content field it will write.
fn added(rows: &mut Rows, s: &StorefrontDesignSection) {
    let label = Some(heading(s));
    push(rows, "section", label, None, kind(s).into());
    push(rows, "heading", label, None, heading(s).into());
    match s {
        StorefrontDesignSection::Hero { props, .. } => {
            if let Some(body) = &props.body {
                push(rows, "body", label, None, body.clone());
            }
            push(rows, "layout", label, None, name(&props.layout));
        }
        StorefrontDesignSection::ProductGrid { props, .. } => {
            push_category(rows, label, None, props.category_id);
            push(rows, "limit", label, None, props.limit.to_string());
            push(rows, "columns", label, None, columns(&props.columns));
        }
    }
}
fn row(
    field: &str,
    label: Option<&str>,
    before: Option<String>,
    after: Option<String>,
) -> StorefrontDesignChangeRow {
    StorefrontDesignChangeRow {
        field: field.into(),
        label: label.map(str::to_owned),
        before,
        after: after.unwrap_or_else(|| REMOVED.into()),
        category_id: None,
    }
}

fn grid<'a>(
    doc: &'a mut StorefrontDesignDocument,
    i: usize,
    id: &str,
) -> Result<&'a mut StorefrontProductGridProps, AppError> {
    let at = find(doc, i, id)?;
    match &mut doc.sections[at] {
        StorefrontDesignSection::ProductGrid { props, .. } => Ok(props),
        _ => Err(AppError::field(
            &format!("changes.{i}.section_id"),
            "Choose a product grid section.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn base() -> StorefrontDesignDocument {
        StorefrontDesignDocument::from_value(
            serde_json::from_str(include_str!(
                "../../../tests/fixtures/storefront-design-v1.json"
            ))
            .unwrap(),
        )
        .unwrap()
    }
    fn changes(value: Value) -> Vec<StorefrontDesignChange> {
        serde_json::from_value(value).unwrap()
    }
    fn row(
        field: &str,
        label: Option<&str>,
        before: Option<&str>,
        after: &str,
    ) -> StorefrontDesignChangeRow {
        StorefrontDesignChangeRow {
            field: field.into(),
            label: label.map(Into::into),
            before: before.map(Into::into),
            after: after.into(),
            category_id: None,
        }
    }
    fn with_id(mut row: StorefrontDesignChangeRow, id: &str) -> StorefrontDesignChangeRow {
        row.category_id = Some(id.parse().unwrap());
        row
    }
    fn fields(err: AppError) -> Vec<String> {
        match err {
            AppError::Validation(e) => e.into_keys().collect(),
            other => panic!("expected validation, got {other:?}"),
        }
    }

    #[test]
    fn property_changes_update_only_named_fields_and_report_rows() {
        let (doc, rows) = apply(
            &base(),
            &changes(json!([
                {"op":"set_theme","theme":"neutral"},
                {"op":"set_hero","section_id":"hero-main","heading":"Fresh this week","layout":"centered"},
                {"op":"set_product_grid","section_id":"featured-products","limit":8,"columns":{"mobile":2,"tablet":3,"desktop":4}}
            ])),
        )
        .unwrap();
        let v = serde_json::to_value(&doc).unwrap();
        assert_eq!(v["theme"], "neutral");
        assert_eq!(v["sections"][0]["props"]["heading"], "Fresh this week");
        assert_eq!(
            v["sections"][0]["props"]["body"],
            "Discover the products and people behind every harvest."
        );
        assert_eq!(v["sections"][0]["props"]["layout"], "centered");
        assert_eq!(
            v["sections"][1]["props"]["heading"],
            "Fresh from our growers"
        );
        assert_eq!(v["sections"][1]["props"]["limit"], 8);
        assert_eq!(
            rows,
            vec![
                row("theme", None, Some("forest"), "neutral"),
                row(
                    "heading",
                    Some("From farm to your table"),
                    Some("From farm to your table"),
                    "Fresh this week"
                ),
                row(
                    "layout",
                    Some("From farm to your table"),
                    Some("split"),
                    "centered"
                ),
                row("limit", Some("Fresh from our growers"), Some("6"), "8"),
                row(
                    "columns",
                    Some("Fresh from our growers"),
                    Some("1 / 2 / 3"),
                    "2 / 3 / 4"
                ),
            ]
        );
    }

    #[test]
    fn clearing_body_and_category_report_dash() {
        let category = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b";
        let (doc, rows) = apply(
            &base(),
            &changes(json!([
                {"op":"clear_hero_body","section_id":"hero-main"},
                {"op":"set_product_grid","section_id":"featured-products","category_id":category}
            ])),
        )
        .unwrap();
        let v = serde_json::to_value(&doc).unwrap();
        assert!(v["sections"][0]["props"].get("body").is_none());
        assert_eq!(v["sections"][1]["props"]["category_id"], category);
        assert_eq!(rows[0].field, "body");
        assert_eq!(rows[0].after, "—");
        assert_eq!(
            rows[1],
            with_id(
                row("category", Some("Fresh from our growers"), None, category),
                category
            )
        );
        let (doc, rows) = apply(
            &doc,
            &changes(json!([{"op":"show_all_products","section_id":"featured-products"}])),
        )
        .unwrap();
        assert!(
            serde_json::to_value(&doc).unwrap()["sections"][1]["props"]
                .get("category_id")
                .is_none()
        );
        assert_eq!(
            rows,
            vec![row(
                "category",
                Some("Fresh from our growers"),
                Some(category),
                "—"
            )]
        );
    }

    #[test]
    fn sections_can_be_added_moved_and_removed() {
        let (doc, rows) = apply(
            &base(),
            &changes(json!([
                {"op":"add_section","section":{"id":"honey","type":"product_grid","props":{"heading":"Honey"}}},
                {"op":"move_section","section_id":"honey","position":1},
                {"op":"remove_section","section_id":"featured-products"}
            ])),
        )
        .unwrap();
        let v = serde_json::to_value(&doc).unwrap();
        let ids: Vec<_> = v["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["honey", "hero-main"]);
        assert_eq!(v["sections"][0]["props"]["limit"], 6, "defaults filled");
        assert_eq!(
            rows,
            vec![
                row("section", Some("Honey"), None, "product_grid"),
                row("heading", Some("Honey"), None, "Honey"),
                row("limit", Some("Honey"), None, "6"),
                row("columns", Some("Honey"), None, "1 / 2 / 3"),
                row("position", Some("Honey"), Some("3"), "1"),
                row(
                    "section",
                    Some("Fresh from our growers"),
                    Some("product_grid"),
                    "—"
                ),
            ]
        );
        let (doc, _) = apply(
            &base(),
            &changes(json!([{"op":"add_section","position":2,"section":{"id":"more","type":"product_grid","props":{"heading":"More"}}}])),
        )
        .unwrap();
        assert_eq!(doc.sections[1].id(), "more");
    }

    #[test]
    fn invalid_targets_and_results_are_rejected_with_paths() {
        let cases = [
            (
                json!([{"op":"set_hero","section_id":"missing","heading":"x"}]),
                "changes.0.section_id",
            ),
            (
                json!([{"op":"set_hero","section_id":"featured-products","heading":"x"}]),
                "changes.0.section_id",
            ),
            (
                json!([{"op":"set_product_grid","section_id":"hero-main","limit":3}]),
                "changes.0.section_id",
            ),
            (
                json!([{"op":"set_hero","section_id":"hero-main"}]),
                "changes.0",
            ),
            (
                json!([{"op":"set_product_grid","section_id":"featured-products"}]),
                "changes.0",
            ),
            (
                json!([{"op":"move_section","section_id":"hero-main","position":3}]),
                "changes.0.position",
            ),
            (
                json!([{"op":"add_section","position":4,"section":{"id":"x","type":"product_grid","props":{"heading":"X"}}}]),
                "changes.0.position",
            ),
            (
                json!([{"op":"add_section","section":{"id":"hero-main","type":"product_grid","props":{"heading":"X"}}}]),
                "document.sections.2.id",
            ),
            (
                json!([{"op":"add_section","section":{"id":"hero-two","type":"hero","props":{"heading":"X"}}}]),
                "document.sections",
            ),
            (
                json!([{"op":"remove_section","section_id":"hero-main"},{"op":"remove_section","section_id":"featured-products"}]),
                "document.sections",
            ),
            (
                json!([{"op":"set_hero","section_id":"hero-main","heading":"x".repeat(161)}]),
                "document.sections.0.props.heading",
            ),
            (
                json!([{"op":"set_hero","section_id":"hero-main","heading":"  "}]),
                "document.sections.0.props.heading",
            ),
            (
                json!([{"op":"set_product_grid","section_id":"featured-products","limit":13}]),
                "document.sections.1.props.limit",
            ),
            (json!([{"op":"set_theme","theme":"forest"}]), "changes"),
            (
                json!([{"op":"set_theme","theme":"neutral"},{"op":"set_theme","theme":"forest"}]),
                "changes",
            ),
        ];
        for (input, field) in cases {
            let err = apply(&base(), &changes(input.clone()))
                .err()
                .unwrap_or_else(|| panic!("{input}"));
            assert!(
                fields(err).contains(&field.to_owned()),
                "{input} -> {field}"
            );
        }
    }

    #[test]
    fn unknown_ops_fields_and_document_writes_never_deserialize() {
        for bad in [
            json!([{"op":"replace_document","document":{}}]),
            json!([{"op":"set_hero","section_id":"hero-main","href":"javascript:alert(1)"}]),
            json!([{"op":"set_theme","theme":"neon"}]),
            json!([{"op":"add_section","section":{"id":"x","type":"html","props":{}}}]),
            json!([{"op":"set_product_grid","section_id":"featured-products","columns":{"mobile":1,"tablet":1,"desktop":1,"raw_css":"x"}}]),
        ] {
            assert!(
                serde_json::from_value::<Vec<StorefrontDesignChange>>(bad.clone()).is_err(),
                "{bad}"
            );
        }
        assert!(
            serde_json::from_value::<StorefrontDesignEdit>(
                json!({"expected_version":0,"changes":[],"document":{}})
            )
            .is_err()
        );
    }

    #[test]
    fn diff_reports_fields_sections_and_order_for_restore_and_publish() {
        let before = base();
        assert!(diff(Some(&before), &before).is_empty());
        let (after, _) = apply(
            &before,
            &changes(json!([
                {"op":"set_hero","section_id":"hero-main","heading":"New"},
                {"op":"clear_hero_body","section_id":"hero-main"},
                {"op":"add_section","position":1,"section":{"id":"honey","type":"product_grid","props":{"heading":"Honey"}}},
                {"op":"move_section","section_id":"featured-products","position":2}
            ])),
        )
        .unwrap();
        let hero = Some("From farm to your table");
        assert_eq!(
            diff(Some(&before), &after),
            vec![
                row("section", Some("Honey"), None, "product_grid"),
                row("heading", Some("Honey"), None, "Honey"),
                row("limit", Some("Honey"), None, "6"),
                row("columns", Some("Honey"), None, "1 / 2 / 3"),
                row("position", Some("Fresh from our growers"), Some("2"), "1"),
                row("heading", hero, hero, "New"),
                row(
                    "body",
                    hero,
                    Some("Discover the products and people behind every harvest."),
                    REMOVED
                ),
                row("position", hero, Some("1"), "2"),
            ]
        );
        assert_eq!(
            diff(Some(&after), &before).last(),
            Some(&row(
                "section",
                Some("Honey"),
                Some("product_grid"),
                REMOVED
            ))
        );
        assert_eq!(
            diff(None, &before),
            vec![
                row("theme", None, None, "forest"),
                row("section", hero, None, "hero"),
                row("heading", hero, None, "From farm to your table"),
                row(
                    "body",
                    hero,
                    None,
                    "Discover the products and people behind every harvest."
                ),
                row("layout", hero, None, "split"),
                row(
                    "section",
                    Some("Fresh from our growers"),
                    None,
                    "product_grid"
                ),
                row(
                    "heading",
                    Some("Fresh from our growers"),
                    None,
                    "Fresh from our growers"
                ),
                row("limit", Some("Fresh from our growers"), None, "6"),
                row("columns", Some("Fresh from our growers"), None, "1 / 2 / 3"),
            ]
        );
    }

    #[test]
    fn added_sections_show_every_field_that_will_be_written() {
        let category = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b";
        let (_, rows) = apply(
            &base(),
            &changes(json!([
                {"op":"remove_section","section_id":"hero-main"},
                {"op":"add_section","position":1,"section":{"id":"welcome","type":"hero","props":{"heading":"Hi","body":"Fresh honey every Friday","layout":"centered"}}},
                {"op":"add_section","section":{"id":"honey","type":"product_grid","props":{"heading":"Honey","category_id":category,"limit":4,"columns":{"mobile":2,"tablet":2,"desktop":4}}}}
            ])),
        )
        .unwrap();
        let hi = Some("Hi");
        let honey = Some("Honey");
        assert_eq!(
            rows[1..].to_vec(),
            vec![
                row("section", hi, None, "hero"),
                row("heading", hi, None, "Hi"),
                row("body", hi, None, "Fresh honey every Friday"),
                row("layout", hi, None, "centered"),
                row("section", honey, None, "product_grid"),
                row("heading", honey, None, "Honey"),
                with_id(row("category", honey, None, category), category),
                row("limit", honey, None, "4"),
                row("columns", honey, None, "2 / 2 / 4"),
            ]
        );
        let (after, _) = apply(
            &base(),
            &changes(json!([{"op":"set_product_grid","section_id":"featured-products","category_id":category}])),
        )
        .unwrap();
        assert_eq!(
            diff(Some(&base()), &after),
            vec![with_id(
                row("category", Some("Fresh from our growers"), None, category),
                category
            )]
        );
    }

    #[test]
    fn starter_is_a_valid_document() {
        assert!(starter().check().is_ok());
    }
}
