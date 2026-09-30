use formualizer_common::RangeAddress;
use formualizer_workbook::{LiteralValue, Workbook};

const SHEET: &str = "Sheet1";
const ROWS: u32 = 500;
const COLS: u32 = 30;

#[test]
fn dense_bulk_value_ingest_matches_legacy_seed_and_sum_probe() {
    let values: Vec<Vec<LiteralValue>> = (0..ROWS)
        .map(|_| vec![LiteralValue::Number(1.0); COLS as usize])
        .collect();

    let mut bulk = Workbook::new();
    bulk.ingest_bulk_values(SHEET, 1, 1, &values)
        .expect("dense bulk values should ingest");
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
