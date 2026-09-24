use super::common::arrow_eval_config;
use crate::engine::Engine;
use crate::engine::graph::editor::change_log::ChangeLog;
use crate::reference::{CellRef, Coord};
use crate::test_workbook::TestWorkbook;
use formualizer_common::LiteralValue;

#[test]
fn sparse_chunk_overlay_triggers_compaction_and_materializes_base_lanes() {
    let cfg = arrow_eval_config();
    let mut engine = Engine::new(TestWorkbook::new(), cfg);

    let sheet = "S";
    let chunk_rows: usize = 64;

    // Seed with a single dense chunk.
    {
        let mut ab = engine.begin_bulk_ingest_arrow();
        ab.add_sheet(sheet, 1, chunk_rows);
        for _ in 0..chunk_rows {
            ab.append_row(sheet, &[LiteralValue::Empty]).unwrap();
        }
        ab.finish().unwrap();
    }

    // Extend capacity to a chunk boundary so we can write multiple cells
    // into a single sparse chunk without changing chunk_starts.
    {
        let asheet = engine
            .sheet_store_mut()
            .sheet_mut(sheet)
            .expect("arrow sheet exists");
        asheet.ensure_row_capacity(chunk_rows * 6);
    }

    // Choose two rows in the same sparse chunk.
    let r1: u32 = (chunk_rows as u32) * 5 + 1;
    let r2: u32 = r1 + 1;

    engine
        .set_cell_value(sheet, r1, 1, LiteralValue::Number(1.0))
        .unwrap();
    engine
        .set_cell_value(sheet, r2, 1, LiteralValue::Number(2.0))
        .unwrap();

    let asheet = engine.sheet_store().sheet(sheet).unwrap();
    let row0 = (r1 - 1) as usize;
    let (ch_idx, in_off) = asheet.chunk_of_row(row0).unwrap();

    let col = &asheet.columns[0];
    assert!(
        col.sparse_chunks.contains_key(&ch_idx),
        "expected writes to land in a sparse chunk"
    );

    let ch = col.chunk(ch_idx).unwrap();
    assert_eq!(ch.overlay.len(), 0, "expected compaction to clear overlay");
    assert!(
        ch.numbers.is_some(),
        "expected compaction to materialize numbers"
    );
    assert!(
        ch.meta.non_null_num >= 2,
        "expected at least two non-null numbers after compaction"
    );

    // Read back values without relying on overlay.
    let av = asheet.range_view(row0, 0, row0 + 1, 0);
    assert_eq!(av.get_cell(0, 0), LiteralValue::Number(1.0));
    assert_eq!(av.get_cell(1, 0), LiteralValue::Number(2.0));

    // Ensure the computed chunk offset stays in-bounds.
    assert!(in_off < ch.type_tag.len());
}

#[test]
fn logged_bulk_values_compact_sparse_chunks_at_the_absolute_overlay_bound() {
    const ROWS: usize = 1_100;
    let mut engine = Engine::new(TestWorkbook::new(), arrow_eval_config());
    {
        let mut ingest = engine.begin_bulk_ingest_arrow();
        ingest.add_sheet("S", 1, ROWS);
        for _ in 0..ROWS {
            ingest.append_row("S", &[LiteralValue::Empty]).unwrap();
        }
        ingest.finish().unwrap();
    }

    let sheet_id = engine.graph.sheet_id("S").expect("seed sheet exists");
    let mut log = ChangeLog::new();
    log.set_enabled(true);
    let compactions_before = engine.debug_overlay_compactions();
    engine
        .edit_with_logger(&mut log, |editor| {
            for row in 0..ROWS {
                editor.set_cell_value_with_old_state(
                    CellRef::new(sheet_id, Coord::new(row as u32, 0, true, true)),
                    LiteralValue::Number(row as f64),
                    Some(LiteralValue::Empty),
                    None,
                );
            }
        })
        .expect("bulk value edit should succeed");

    assert_eq!(
        engine.debug_overlay_compactions() - compactions_before,
        1,
        "bulk edits should compact once after crossing the absolute point bound"
    );
    assert_eq!(
        engine.get_cell_value("S", 1, 1),
        Some(LiteralValue::Number(0.0))
    );
    assert_eq!(
        engine.get_cell_value("S", ROWS as u32, 1),
        Some(LiteralValue::Number((ROWS - 1) as f64))
    );
}

#[test]
fn logged_bulk_values_compact_each_touched_chunk_once_across_many_thresholds() {
    const ROWS: usize = 32 * 1024;
    const WRITES: usize = 6_000;
    let mut engine = Engine::new(TestWorkbook::new(), arrow_eval_config());
    {
        let mut ingest = engine.begin_bulk_ingest_arrow();
        ingest.add_sheet("S", 1, ROWS);
        for _ in 0..ROWS {
            ingest.append_row("S", &[LiteralValue::Empty]).unwrap();
        }
        ingest.finish().unwrap();
    }

    let sheet_id = engine.graph.sheet_id("S").expect("seed sheet exists");
    let mut log = ChangeLog::new();
    log.set_enabled(true);
    let compactions_before = engine.debug_overlay_compactions();
    engine
        .edit_with_logger(&mut log, |editor| {
            for row in 0..WRITES {
                editor.set_cell_value_with_old_state(
                    CellRef::new(sheet_id, Coord::new(row as u32, 0, true, true)),
                    LiteralValue::Number(row as f64),
                    Some(LiteralValue::Empty),
                    None,
                );
            }
        })
        .expect("bulk value edit should succeed");

    let compactions = engine.debug_overlay_compactions() - compactions_before;
    assert!(
        compactions <= 2,
        "6000 writes in one chunk should compact at most once per touched chunk (got {compactions}, old code rebuilt ~writes/1025)"
    );
    assert_eq!(
        engine.get_cell_value("S", 1, 1),
        Some(LiteralValue::Number(0.0))
    );
    assert_eq!(
        engine.get_cell_value("S", 3000, 1),
        Some(LiteralValue::Number(2999.0))
    );
    assert_eq!(
        engine.get_cell_value("S", WRITES as u32, 1),
        Some(LiteralValue::Number((WRITES - 1) as f64))
    );
}
