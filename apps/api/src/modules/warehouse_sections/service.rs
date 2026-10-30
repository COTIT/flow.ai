//! Section rules: code unique per warehouse, manager must be a member, optional auto-layout
//! (Laravel SectionCreated) capped so one request can't insert millions of rows.

use std::collections::HashSet;

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        users::repo as users,
        warehouse_bins::service as bins,
        warehouse_sections::{
            dto::{CODE_TAKEN, Fields, SectionInput},
            repo::{self, Layout, ListFilter, SectionRow},
        },
        warehouses::repo as warehouses,
    },
};

/// Bins one section layout may generate; Laravel had no cap.
const MAX_LAYOUT_BINS: i64 = 10_000;

/// The path's warehouse, live and in this workspace, else 404.
pub async fn warehouse(state: &AppState, workspace_id: Uuid, id: &str) -> Result<Uuid, AppError> {
    let id = parse(id)?;
    warehouses::exists(&state.db, workspace_id, id)
        .await?
        .then_some(id)
        .ok_or(AppError::NotFound)
}

/// The path's section under a live warehouse, else 404.
pub async fn find(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: &str,
    id: &str,
) -> Result<SectionRow, AppError> {
    repo::find(&state.db, workspace_id, parse(warehouse_id)?, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// One page plus total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<SectionRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, warehouse_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, warehouse_id, f).await?;
    }
    Ok((rows, total))
}

/// Create the section and, when rows, shelves and bins per shelf are all > 0, its layout.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    input: SectionInput,
) -> Result<Uuid, AppError> {
    check_manager(state, workspace_id, &input.fields).await?;
    let layout = layout(&input.section_code, &input.fields)?;
    if let Some(l) = &layout {
        let adding = i64::try_from(l.bins.codes.len()).unwrap_or(i64::MAX);
        if !bins::quota_allows(state, workspace_id, adding).await? {
            return Err(bins::quota_error("rows"));
        }
    }
    let mut tx = state.db.begin().await?;
    let id = repo::insert(
        &mut *tx,
        workspace_id,
        warehouse_id,
        &input.section_name,
        &input.section_code,
        &input.fields,
    )
    .await
    .map_err(code_clash)?;
    if let Some(l) = layout {
        let temperature = input
            .fields
            .temperature_control
            .as_deref()
            .unwrap_or("ambient");
        repo::insert_layout(&mut tx, workspace_id, warehouse_id, id, temperature, &l).await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// Patch a live section, else 404. The layout is never regenerated (Laravel parity).
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: &str,
    input: SectionInput,
) -> Result<(), AppError> {
    let id = parse(id)?;
    check_manager(state, workspace_id, &input.fields).await?;
    // No code pre-check: an unknown id must stay a 404; the index catches a clash.
    let found = repo::update(
        &state.db,
        workspace_id,
        warehouse_id,
        id,
        &input.section_name,
        &input.section_code,
        &input.fields,
    )
    .await
    .map_err(code_clash)?;
    found.then_some(()).ok_or(AppError::NotFound)
}

/// Soft delete, else 404.
pub async fn delete(
    state: &AppState,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    id: &str,
) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, warehouse_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Laravel SectionCreated naming: rows `{code}-ROW-{A..}`, shelves `{row}-SHELF-{n}`,
/// bins `{code}-{letter}{shelf}{nn}`. None when any count is missing or 0.
fn layout(code: &str, f: &Fields) -> Result<Option<Layout>, AppError> {
    let n = |v: Option<Option<i32>>| i64::from(v.flatten().unwrap_or_default());
    let (rows, shelves, bins) = (n(f.rows), n(f.shelves_per_row), n(f.bins_per_shelf));
    if rows <= 0 || shelves <= 0 || bins <= 0 {
        return Ok(None);
    }
    if rows.saturating_mul(shelves).saturating_mul(bins) > MAX_LAYOUT_BINS {
        return Err(AppError::field(
            "rows",
            format!("The section layout must not generate more than {MAX_LAYOUT_BINS} bins."),
        ));
    }
    let mut l = Layout {
        shelves_per_row: i32::try_from(shelves).unwrap_or_default(),
        bins_per_shelf: i32::try_from(bins).unwrap_or_default(),
        ..Layout::default()
    };
    for row in 0..rows {
        let letter = row_letters(row);
        let row_code = format!("{code}-ROW-{letter}");
        l.rows
            .push(None, row_code.clone(), format!("Row {letter}"), None);
        for shelf in 1..=shelves {
            let shelf_code = format!("{row_code}-SHELF-{shelf}");
            let level = i32::try_from(shelf).unwrap_or_default();
            l.shelves.push(
                Some(&row_code),
                shelf_code.clone(),
                format!("Shelf {shelf}"),
                Some(level),
            );
            for bin in 1..=bins {
                let tag = format!("{letter}{shelf}{bin:02}");
                l.bins.push(
                    Some(&shelf_code),
                    format!("{code}-{tag}"),
                    format!("Bin {tag}"),
                    None,
                );
            }
        }
    }
    // Laravel's scheme repeats a code when shelves >= 11 and bins >= 101 ("A1101" twice).
    let mut seen = HashSet::with_capacity(l.bins.codes.len());
    if !l.bins.codes.iter().all(|c| seen.insert(c.to_lowercase())) {
        return Err(AppError::field(
            "bins_per_shelf",
            "The section layout generates duplicate bin codes.",
        ));
    }
    Ok(Some(l))
}

/// 0 -> A, 25 -> Z, 26 -> AA (spreadsheet columns). Laravel's chr(65 + i) broke past Z.
fn row_letters(index: i64) -> String {
    let mut n = index + 1;
    let mut out = Vec::new();
    while n > 0 {
        let rem = u8::try_from((n - 1) % 26).unwrap_or_default();
        out.push(char::from(b'A' + rem));
        n = (n - 1) / 26;
    }
    out.iter().rev().collect()
}

/// A sent manager must be a member of this workspace.
async fn check_manager(state: &AppState, workspace_id: Uuid, f: &Fields) -> Result<(), AppError> {
    let Some(Some(id)) = f.assigned_manager else {
        return Ok(());
    };
    if users::members_among(&state.db, workspace_id, &[id])
        .await?
        .is_empty()
    {
        return Err(AppError::field(
            "assigned_manager",
            "The selected assigned manager is invalid.",
        ));
    }
    Ok(())
}

/// The live-code unique index fired: same 422 as the pre-check.
fn code_clash(e: sqlx::Error) -> AppError {
    AppError::unique(
        e,
        "warehouse_sections_code_unique",
        "section_code",
        CODE_TAKEN,
    )
}

/// Malformed id = unknown id = 404.
pub fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_letters_like_spreadsheet_columns() {
        let got: Vec<String> = [0, 1, 25, 26, 27, 51, 52, 701, 702]
            .into_iter()
            .map(row_letters)
            .collect();
        assert_eq!(got, ["A", "B", "Z", "AA", "AB", "AZ", "BA", "ZZ", "AAA"]);
    }

    #[test]
    fn layout_rejects_duplicate_bin_codes() {
        let f = Fields {
            rows: Some(Some(1)),
            shelves_per_row: Some(Some(11)),
            bins_per_shelf: Some(Some(101)),
            ..Fields::default()
        };
        assert!(layout("S", &f).is_err());
    }

    #[test]
    fn layout_skipped_without_all_three_counts() {
        let f = Fields {
            rows: Some(Some(2)),
            shelves_per_row: Some(Some(0)),
            bins_per_shelf: Some(Some(3)),
            ..Fields::default()
        };
        assert!(layout("S", &f).unwrap().is_none());
    }
}
