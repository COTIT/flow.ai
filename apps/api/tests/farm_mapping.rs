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
