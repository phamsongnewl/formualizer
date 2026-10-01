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
    /// Returns an error if graph admission fails.
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

        let previous_mode = self.graph.sheet_index_mode();
        let previous_assume_new = self.first_load_assume_new();
        if !previous_assume_new {
            self.set_first_load_assume_new(true);
        }
        self.graph.set_sheet_index_mode(SheetIndexMode::FastBatch);
        let result = self.graph.bulk_insert_values(sheet, cells);
        if !previous_assume_new {
            self.set_first_load_assume_new(false);
        }
        self.graph.set_sheet_index_mode(previous_mode);
        result
    }
}
