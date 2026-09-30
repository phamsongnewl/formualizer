use super::Engine;
use crate::engine::SheetIndexMode;
use crate::traits::EvaluationContext;
use formualizer_common::LiteralValue;
use formualizer_parse::ExcelError;

impl<R> Engine<R>
where
    R: EvaluationContext,
{
    /// Insert fresh literal cells through the graph's batched allocation path.
    ///
    /// This low-level operation does not mirror values into Arrow storage, run normal per-cell
    /// dirty propagation, record undo events, or advance engine revisions. Callers must establish
    /// that every target cell is fresh, maintain their authoritative value store, and publish the
    /// appropriate edit revisions. Interactive edits should use [`Engine::set_cell_value`] instead.
    ///
    /// # Errors
    ///
    /// Returns an error if function-semantic observation, formula-span demotion, or graph
    /// admission fails.
    pub fn bulk_insert_values_untracked<I>(
        &mut self,
        sheet: &str,
        cells: I,
    ) -> Result<(), ExcelError>
    where
        I: IntoIterator<Item = (u32, u32, LiteralValue)>,
    {
        let cells: Vec<_> = cells.into_iter().collect();
        if cells.is_empty() {
            return Ok(());
        }

        self.observe_function_semantic_epoch()?;
        let sheet_id = self.graph.sheet_id_mut(sheet);
        let mut coordinates = Vec::with_capacity(cells.len());
        for (row, col, _) in &cells {
            self.demote_span_containing_cell_for_write(
                sheet_id,
                row.saturating_sub(1),
                col.saturating_sub(1),
            )
            .map_err(Self::editor_error_to_excel)?;
            coordinates.push((*row, *col));
        }

        let previous_mode = self.graph.sheet_index_mode();
        self.graph.set_sheet_index_mode(SheetIndexMode::FastBatch);
        let result = self.graph.bulk_insert_values(sheet, cells);
        self.graph.set_sheet_index_mode(previous_mode);
        result?;

        for (row, col) in coordinates {
            self.record_formula_plane_changed_cell(sheet, row, col);
        }

        Ok(())
    }
}
