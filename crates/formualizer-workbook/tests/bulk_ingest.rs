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
    bulk.ingest_bulk_values(SHEET, 1, 1, &values)
        .expect("mixed bulk values should ingest");

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
