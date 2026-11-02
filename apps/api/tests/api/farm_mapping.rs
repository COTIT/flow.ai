//! docs/ruaha-rls-mapping.md stays true: one row per RUAHA assertion (280), every named test
//! exists, every N/A says why.

use std::path::Path;

const DOC: &str = "docs/ruaha-rls-mapping.md";
const RUAHA_ASSERTIONS: usize = 280;

/// (row number, last cell) of each table row.
fn rows(doc: &str) -> Vec<(usize, String)> {
    doc.lines()
        .filter_map(|l| {
            let cells: Vec<&str> = l.split('|').map(str::trim).collect();
            let n = cells.get(1)?.parse::<usize>().ok()?;
            Some((n, cells.get(4)?.to_string()))
        })
        .collect()
}

/// Every `tests/<file>.rs::<fn>` in the cell names a test fn that exists.
fn missing_tests(cell: &str) -> Vec<String> {
    cell.split('`')
        .filter(|s| s.starts_with("tests/"))
        .filter_map(|target| {
            let (file, name) = target.split_once("::")?;
            let source = std::fs::read_to_string(Path::new(file)).unwrap_or_default();
            let found = source.contains(&format!("async fn {name}("))
                || source.contains(&format!("fn {name}("));
            (!found).then(|| target.to_owned())
        })
        .collect()
}

#[test]
fn every_ruaha_assertion_has_one_row() {
    let doc = std::fs::read_to_string(DOC).unwrap();
    let rows = rows(&doc);
    assert_eq!(rows.len(), RUAHA_ASSERTIONS);
    let numbers: Vec<usize> = rows.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        numbers,
        (1..=RUAHA_ASSERTIONS).collect::<Vec<_>>(),
        "rows numbered 1..=280 in order"
    );
}

#[test]
fn every_row_names_a_real_test_or_a_reason() {
    let doc = std::fs::read_to_string(DOC).unwrap();
    for (n, cell) in rows(&doc) {
        if let Some(reason) = cell.strip_prefix("N/A because ") {
            assert!(reason.trim().len() > 10, "row {n}: N/A needs a real reason");
            continue;
        }
        assert!(
            cell.contains("`tests/"),
            "row {n}: neither a test nor N/A: {cell}"
        );
        let missing = missing_tests(&cell);
        assert!(missing.is_empty(), "row {n}: no such test {missing:?}");
    }
}

/// Phase 4c rows (energy, PUE, market, Tower, catalogue, supply attacher, bug reports): RUAHA
/// rls #8, 9, 13–21, 23; mvp_security #9, 12, 13, 17–22, 25, 33–39, 49, 50; village_create #2,
/// 3, 5, 7, 12; resource_catalogue #1–16; supply_attached_by #1–6; bug_report #1–16.
fn phase_4c_rows() -> Vec<usize> {
    let mut rows = vec![8, 9, 13, 14, 15, 16, 17, 18, 19, 20, 21, 23];
    rows.extend([48, 51, 52, 56, 57, 58, 59, 60, 61, 64, 88, 89]);
    rows.extend(72..=78);
    rows.extend([209, 210, 212, 214, 219]);
    rows.extend(243..=280);
    rows
}

/// Phase 4c rows that stay N/A on purpose: no route edits a request's content (#13, #14), no
/// name lookup exists (supply_attached_by #1), no storage policy layer (bug_report #11).
const PHASE_4C_BY_DESIGN: [usize; 4] = [255, 256, 259, 275];

#[test]
fn phase_4c_rows_are_ported() {
    let doc = std::fs::read_to_string(DOC).unwrap();
    let rows = rows(&doc);
    for n in phase_4c_rows() {
        let (_, cell) = rows.iter().find(|(r, _)| *r == n).unwrap();
        if PHASE_4C_BY_DESIGN.contains(&n) {
            assert!(
                cell.starts_with("N/A because "),
                "row {n}: by design N/A: {cell}"
            );
        } else {
            assert!(
                cell.contains("`tests/"),
                "row {n}: Phase 4c row not ported: {cell}"
            );
        }
    }
}
