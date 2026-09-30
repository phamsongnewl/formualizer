use formualizer_common::RangeAddress;
use formualizer_workbook::{LiteralValue, Workbook};
use std::time::Instant;

const SHEET: &str = "BulkSeed";
const ROWS: u32 = 500;
const COLS: u32 = 30;

fn dense_literal_values(rows: u32) -> Vec<Vec<LiteralValue>> {
    (0..rows)
        .map(|_| vec![LiteralValue::Number(1.0); COLS as usize])
        .collect()
}

#[test]
fn dense_bulk_value_ingest_matches_legacy_seed_and_sum_probe() {
    let values = dense_literal_values(ROWS);

    let mut bulk = Workbook::new();
    let events_before = bulk.changelog().events().len();
    let outcome = bulk
        .ingest_bulk_values(SHEET, 1, 1, &values)
        .expect("dense bulk values should ingest");
    assert_eq!(outcome.fast_lane_cells, (ROWS * COLS) as usize);
    assert_eq!(outcome.fallback_cells, 0);
    assert_eq!(bulk.changelog().events().len(), events_before);
    assert!(bulk.has_sheet(SHEET));
    assert_eq!(bulk.sheet_dimensions(SHEET), Some((ROWS, COLS)));
    bulk.set_formula(SHEET, ROWS + 1, 1, "=SUM(A1:AD500)")
        .expect("SUM probe should be accepted");
    bulk.evaluate_all()
        .expect("bulk-seeded workbook should evaluate");

    let mut legacy = Workbook::new();
    legacy
        .set_values(SHEET, 1, 1, &values)
        .expect("legacy dense values should seed");
    legacy
        .set_formula(SHEET, ROWS + 1, 1, "=SUM(A1:AD500)")
        .expect("SUM probe should be accepted");
    legacy
        .evaluate_all()
        .expect("legacy-seeded workbook should evaluate");

    let full_range =
        RangeAddress::new(SHEET, 1, 1, ROWS + 1, COLS).expect("full seeded range should be valid");
    assert_eq!(bulk.sheet_names(), legacy.sheet_names());
    assert_eq!(bulk.sheet_dimensions(SHEET), Some((ROWS + 1, COLS)));
    assert_eq!(bulk.sheet_dimensions(SHEET), legacy.sheet_dimensions(SHEET));
    assert_eq!(bulk.read_range(&full_range), legacy.read_range(&full_range));
    assert_eq!(
        bulk.get_formula(SHEET, ROWS + 1, 1),
        legacy.get_formula(SHEET, ROWS + 1, 1)
    );
    assert_eq!(
        bulk.get_value(SHEET, ROWS + 1, 1),
        Some(LiteralValue::Number((ROWS * COLS) as f64))
    );
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    sheet_names: Vec<String>,
    dimensions: (u32, u32),
    values: Vec<Vec<LiteralValue>>,
    formulas: Vec<Vec<Option<String>>>,
}

fn snapshot(workbook: &Workbook) -> Snapshot {
    let dimensions = workbook
        .sheet_dimensions(SHEET)
        .expect("seeded sheet should have dimensions");
    let range = RangeAddress::new(SHEET, 1, 1, dimensions.0, dimensions.1)
        .expect("snapshot range should be valid");
    let formulas = (1..=dimensions.0)
        .map(|row| {
            (1..=dimensions.1)
                .map(|col| workbook.get_formula(SHEET, row, col))
                .collect()
        })
        .collect();

    Snapshot {
        sheet_names: workbook.sheet_names(),
        dimensions,
        values: workbook.read_range(&range),
        formulas,
    }
}

fn seed_existing_cells(workbook: &mut Workbook) {
    workbook
        .set_value(SHEET, 1, 1, LiteralValue::Int(7))
        .expect("existing value should be set");
    workbook
        .set_value(SHEET, 1, 2, LiteralValue::Int(8))
        .expect("formula cell should be initialized");
    workbook
        .set_formula(SHEET, 1, 2, "=A1*2")
        .expect("existing formula should be set");
    workbook
        .set_formula(SHEET, 1, 3, "=1+1")
        .expect("staged formula text should be accepted");
}

#[test]
fn mixed_existing_cells_match_legacy_bulk_write_snapshot() {
    let values = vec![
        vec![
            LiteralValue::Number(10.0),
            LiteralValue::Number(20.0),
            LiteralValue::Number(30.0),
        ],
        vec![
            LiteralValue::Number(40.0),
            LiteralValue::Number(50.0),
            LiteralValue::Number(60.0),
        ],
    ];

    let mut bulk = Workbook::new();
    seed_existing_cells(&mut bulk);
    assert_eq!(bulk.get_formula(SHEET, 1, 2), Some("=A1 * 2".to_string()));
    assert_eq!(
        bulk.engine().get_staged_formula_text(SHEET, 1, 3),
        Some("=1+1".to_string())
    );
    let events_before = bulk.changelog().events().len();
    let outcome = bulk
        .ingest_bulk_values(SHEET, 1, 1, &values)
        .expect("mixed bulk values should ingest");
    assert_eq!(outcome.fast_lane_cells, 3);
    assert_eq!(outcome.fallback_cells, 3);

    let mut fallback_legacy = Workbook::new();
    seed_existing_cells(&mut fallback_legacy);
    let fallback_events_before = fallback_legacy.changelog().events().len();
    for (col, value) in values[0].iter().take(3).enumerate() {
        fallback_legacy
            .set_value(SHEET, 1, col as u32 + 1, value.clone())
            .expect("legacy fallback write should succeed");
    }
    assert_eq!(
        &bulk.changelog().events()[events_before..],
        &fallback_legacy.changelog().events()[fallback_events_before..]
    );

    let mut legacy = Workbook::new();
    seed_existing_cells(&mut legacy);
    legacy
        .set_values(SHEET, 1, 1, &values)
        .expect("legacy mixed values should seed");

    assert_eq!(bulk.sheet_dimensions(SHEET), Some((2, 3)));
    assert_eq!(snapshot(&bulk), snapshot(&legacy));
    assert_eq!(bulk.get_formula(SHEET, 1, 2), None);
    assert_eq!(bulk.get_formula(SHEET, 1, 3), None);
}

fn staged_formula_rows(rows: u32) -> Vec<Vec<String>> {
    (1..=rows)
        .map(|row| vec![format!("=SUM(A{row}:AD{row})"); COLS as usize])
        .collect()
}

fn timed_bulk_staged_formula_seed(rows: u32) -> (Workbook, std::time::Duration) {
    let values = dense_literal_values(rows);
    let formulas = staged_formula_rows(rows);
    let mut workbook = Workbook::new();
    workbook
        .ingest_bulk_values(SHEET, 1, 1, &values)
        .expect("dense literals should ingest before formula staging");

    let started = Instant::now();
    workbook
        .set_formulas(SHEET, 1, COLS + 1, &formulas)
        .expect("staged formula batch should be accepted");
    (workbook, started.elapsed())
}

#[test]
fn staged_formula_batches_scale_and_match_per_cell_seed() {
    let (mut small_bulk, small_batch_time) = timed_bulk_staged_formula_seed(ROWS);
    small_bulk
        .evaluate_all()
        .expect("small formula batch should evaluate");
    assert_eq!(
        small_bulk.get_value(SHEET, ROWS, COLS + 1),
        Some(LiteralValue::Number(COLS as f64))
    );
    let small_snapshot = snapshot(&small_bulk);
    drop(small_bulk);

    let values = dense_literal_values(ROWS);
    let formulas = staged_formula_rows(ROWS);
    let mut legacy = Workbook::new();
    legacy
        .set_values(SHEET, 1, 1, &values)
        .expect("legacy dense literals should seed");
    let started = Instant::now();
    for (row_idx, row_formulas) in formulas.iter().enumerate() {
        for (col_idx, formula) in row_formulas.iter().enumerate() {
            legacy
                .set_formula(
                    SHEET,
                    row_idx as u32 + 1,
                    COLS + col_idx as u32 + 1,
                    formula,
                )
                .expect("legacy staged formula should be accepted");
        }
    }
    let legacy_cellwise_time = started.elapsed();
    legacy
        .evaluate_all()
        .expect("legacy formula seed should evaluate");
    assert_eq!(
        legacy.get_value(SHEET, ROWS, COLS + 1),
        Some(LiteralValue::Number(COLS as f64))
    );
    assert_eq!(small_snapshot, snapshot(&legacy));

    let large_rows = ROWS * 12;
    let (large_bulk, large_batch_time) = timed_bulk_staged_formula_seed(large_rows);
    assert_eq!(
        large_bulk.sheet_dimensions(SHEET),
        Some((large_rows, COLS * 2))
    );
    assert_eq!(
        large_bulk.get_formula(SHEET, large_rows, COLS + 1),
        Some(format!("=SUM(A{large_rows}:AD{large_rows})"))
    );

    let small_formula_cells = ROWS as usize * COLS as usize;
    let large_formula_cells = large_rows as usize * COLS as usize;
    let scaling_ratio = large_batch_time.as_secs_f64() / small_batch_time.as_secs_f64();
    eprintln!(
        "[bulk-ingest-formulas] small: {small_formula_cells} formulas in {small_batch_time:?}; \
         large: {large_formula_cells} formulas in {large_batch_time:?}; \
         staged batch ratio={scaling_ratio:.2}x; \
         legacy per-cell small={legacy_cellwise_time:?}"
    );
}
