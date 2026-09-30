//! Batched literal-value ingestion for `Workbook`.

use super::Workbook;
use crate::error::IoError;
use formualizer_common::LiteralValue;

/// Counts cells routed through the fast and fallback paths of bulk ingestion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BulkIngestOutcome {
    /// Fresh cells inserted through the batched fast lane.
    pub fast_lane_cells: usize,
    /// Existing cells written through the regular single-cell setter.
    pub fallback_cells: usize,
}

impl Workbook {
    /// Ingest literal values in a rectangle, batching writes to fresh cells.
    ///
    /// Cells with an existing graph value, formula, staged formula, or non-empty Arrow value use
    /// [`Workbook::set_value`] so their regular edit and undo behavior is preserved. Fresh cells
    /// are inserted together and do not create undo events. The target sheet is created if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the target sheet cannot be created, the engine rejects a fast-lane
    /// batch, or a fallback single-cell write fails.
    pub fn ingest_bulk_values(
        &mut self,
        sheet: &str,
        start_row: u32,
        start_col: u32,
        rows: &[Vec<LiteralValue>],
    ) -> Result<BulkIngestOutcome, IoError> {
        let sheet_existed = self.has_sheet(sheet);
        let cell_count = rows
            .iter()
            .fold(0usize, |total, row| total.saturating_add(row.len()));

        if cell_count == 0 {
            if !sheet_existed {
                self.add_sheet(sheet)
                    .map_err(|error| IoError::from_backend("workbook", error))?;
            }
            return Ok(BulkIngestOutcome::default());
        }

        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        if let Some(last_row_idx) = rows.iter().rposition(|row| !row.is_empty())
            && width > 0
        {
            let end_row = start_row.saturating_add(last_row_idx as u32);
            let end_col = start_col.saturating_add((width - 1) as u32);
            self.ensure_arrow_sheet_capacity(sheet, end_row as usize, end_col as usize);
        }

        let mut fast_cells = Vec::with_capacity(cell_count);
        let mut fallback_cells = Vec::new();
        for (row_idx, values) in rows.iter().enumerate() {
            let row = start_row + row_idx as u32;
            for (col_idx, _) in values.iter().enumerate() {
                let col = start_col + col_idx as u32;
                let graph_empty = self.engine.get_cell(sheet, row, col).is_none();
                let staged_empty = self
                    .engine
                    .get_staged_formula_text(sheet, row, col)
                    .is_none();
                let arrow_empty =
                    self.engine
                        .sheet_store()
                        .sheet(sheet)
                        .is_none_or(|arrow_sheet| {
                            matches!(
                                arrow_sheet.get_cell_value(
                                    row.saturating_sub(1) as usize,
                                    col.saturating_sub(1) as usize,
                                ),
                                LiteralValue::Empty
                            )
                        });

                let cell = (row, col, row_idx, col_idx);
                if graph_empty && staged_empty && arrow_empty {
                    fast_cells.push(cell);
                } else {
                    fallback_cells.push(cell);
                }
            }
        }

        let fast_lane_cells = fast_cells.len();
        let fallback_count = fallback_cells.len();
        self.engine.begin_deferred_dirty();
        let mut fast_inserted = false;
        let mut fast_attempted = false;
        let result = (|| {
            if fast_lane_cells > 0 {
                fast_attempted = true;
                self.engine
                    .bulk_insert_values_untracked(
                        sheet,
                        fast_cells.iter().map(|(row, col, row_idx, col_idx)| {
                            (*row, *col, rows[*row_idx][*col_idx].clone())
                        }),
                    )
                    .map_err(IoError::Engine)?;
                fast_inserted = true;

                for (row, col, row_idx, col_idx) in &fast_cells {
                    self.mirror_value_to_overlay(sheet, *row, *col, &rows[*row_idx][*col_idx]);
                }
            }

            for (row, col, row_idx, col_idx) in &fallback_cells {
                self.set_value(sheet, *row, *col, rows[*row_idx][*col_idx].clone())?;
            }

            Ok(BulkIngestOutcome {
                fast_lane_cells,
                fallback_cells: fallback_count,
            })
        })();
        self.engine.end_deferred_dirty();

        if fast_attempted && !sheet_existed && self.has_sheet(sheet) {
            self.engine.mark_topology_edited();
        }
        if fast_inserted {
            self.engine.mark_data_edited();
        }

        result
    }
}
