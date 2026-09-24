//! Isolated Workbook batch-setter timings for the 955k local-import shape.
//!
//! Fixture data and workbook construction are prepared outside each timed
//! iteration. Run with `cargo bench -p formualizer-workbook --bench seed_batch`.

use std::time::Instant;

use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use formualizer_workbook::{LiteralValue, Workbook};

const COLUMN_COUNT: usize = 100;
const SHEET_COUNT: usize = 13;
const CELL_COUNT: usize = 955_000;
const FORMULA_COUNT: usize = 41_500;
const BOUNDED_BLOCK_CELLS: usize = 20_000;
const SAMPLE_SIZE: usize = 10;

struct ValueRun {
    sheet_index: usize,
    row: u32,
    start_col: u32,
    values: Vec<LiteralValue>,
}

struct FormulaRun {
    sheet_index: usize,
    row: u32,
    start_col: u32,
    formulas: Vec<String>,
}

struct ValueBlock {
    sheet_index: usize,
    start_row: u32,
    start_col: u32,
    rows: Vec<Vec<LiteralValue>>,
}

struct FormulaBlock {
    sheet_index: usize,
    start_row: u32,
    start_col: u32,
    rows: Vec<Vec<String>>,
}

struct SeedShape {
    sheet_names: Vec<String>,
    value_runs: Vec<ValueRun>,
    formula_runs: Vec<FormulaRun>,
    value_blocks: Vec<ValueBlock>,
    formula_blocks: Vec<FormulaBlock>,
}

impl SeedShape {
    fn large_data_class() -> Self {
        let mut cell_counts: Vec<usize> = vec![54_000; SHEET_COUNT - 2];
        cell_counts.insert(0, 306_000);
        cell_counts.push(55_000);

        let sheet_names = (1..=SHEET_COUNT)
            .map(|index| format!("Sheet {index}"))
            .collect();
        let mut value_runs = Vec::new();
        let mut formula_runs = Vec::new();
        let mut formulas_remaining = FORMULA_COUNT;

        for (sheet_index, cell_count) in cell_counts.into_iter().enumerate() {
            let row_count = cell_count.div_ceil(COLUMN_COUNT);
            for row_index in 0..row_count {
                let cells_in_row = (cell_count - row_index * COLUMN_COUNT).min(COLUMN_COUNT);
                let formula_cells = if sheet_index == SHEET_COUNT - 1 {
                    formulas_remaining.min(cells_in_row)
                } else {
                    0
                };
                if formula_cells > 0 {
                    formula_runs.push(FormulaRun {
                        sheet_index,
                        row: u32::try_from(row_index + 1).expect("fixture row fits u32"),
                        start_col: 1,
                        formulas: vec!["=1+1".to_string(); formula_cells],
                    });
                    formulas_remaining -= formula_cells;
                }

                let value_cells = cells_in_row - formula_cells;
                if value_cells > 0 {
                    let first_linear = row_index * COLUMN_COUNT + formula_cells;
                    value_runs.push(ValueRun {
                        sheet_index,
                        row: u32::try_from(row_index + 1).expect("fixture row fits u32"),
                        start_col: u32::try_from(formula_cells + 1)
                            .expect("fixture column fits u32"),
                        values: (0..value_cells)
                            .map(|offset| LiteralValue::Number((first_linear + offset) as f64))
                            .collect(),
                    });
                }
            }
        }

        assert_eq!(formulas_remaining, 0);
        assert_eq!(
            value_runs.iter().map(|run| run.values.len()).sum::<usize>()
                + formula_runs
                    .iter()
                    .map(|run| run.formulas.len())
                    .sum::<usize>(),
            CELL_COUNT
        );
        let value_blocks = value_runs_to_blocks(&value_runs);
        let formula_blocks = formula_runs_to_blocks(&formula_runs);
        Self {
            sheet_names,
            value_runs,
            formula_runs,
            value_blocks,
            formula_blocks,
        }
    }
}

fn value_runs_to_blocks(runs: &[ValueRun]) -> Vec<ValueBlock> {
    (0..SHEET_COUNT)
        .filter_map(|sheet_index| {
            let mut sheet_runs = runs.iter().filter(|run| run.sheet_index == sheet_index);
            let first = sheet_runs.next()?;
            let start_row = first.row;
            let start_col = first.start_col;
            let mut rows = Vec::new();
            for (next_row, run) in (start_row..).zip(std::iter::once(first).chain(sheet_runs)) {
                assert_eq!(run.row, next_row, "fixture value rows must be contiguous");
                assert_eq!(run.start_col, start_col, "fixture value rows must align");
                rows.push(run.values.clone());
            }
            Some(ValueBlock {
                sheet_index,
                start_row,
                start_col,
                rows,
            })
        })
        .collect()
}

fn formula_runs_to_blocks(runs: &[FormulaRun]) -> Vec<FormulaBlock> {
    (0..SHEET_COUNT)
        .filter_map(|sheet_index| {
            let mut sheet_runs = runs.iter().filter(|run| run.sheet_index == sheet_index);
            let first = sheet_runs.next()?;
            let start_row = first.row;
            let start_col = first.start_col;
            let mut rows = Vec::new();
            for (next_row, run) in (start_row..).zip(std::iter::once(first).chain(sheet_runs)) {
                assert_eq!(run.row, next_row, "fixture formula rows must be contiguous");
                assert_eq!(run.start_col, start_col, "fixture formula rows must align");
                rows.push(run.formulas.clone());
            }
            Some(FormulaBlock {
                sheet_index,
                start_row,
                start_col,
                rows,
            })
        })
        .collect()
}

fn empty_interactive_workbook(sheet_names: &[String]) -> Workbook {
    let mut workbook = Workbook::new();
    workbook
        .rename_sheet("Sheet1", &sheet_names[0])
        .expect("default sheet should be renamed for the fixture");
    for sheet_name in &sheet_names[1..] {
        workbook
            .add_sheet(sheet_name)
            .expect("fixture sheet should be added");
    }
    workbook
}

fn apply_values(workbook: &mut Workbook, shape: &SeedShape) {
    for run in &shape.value_runs {
        workbook
            .set_values(
                &shape.sheet_names[run.sheet_index],
                run.row,
                run.start_col,
                std::slice::from_ref(&run.values),
            )
            .expect("large fixture value run should be accepted");
    }
}

fn apply_formulas(workbook: &mut Workbook, shape: &SeedShape) {
    for run in &shape.formula_runs {
        workbook
            .set_formulas(
                &shape.sheet_names[run.sheet_index],
                run.row,
                run.start_col,
                std::slice::from_ref(&run.formulas),
            )
            .expect("large fixture formula run should be accepted");
    }
}

fn apply_value_blocks(workbook: &mut Workbook, shape: &SeedShape) {
    for block in &shape.value_blocks {
        workbook
            .set_values(
                &shape.sheet_names[block.sheet_index],
                block.start_row,
                block.start_col,
                &block.rows,
            )
            .expect("large fixture value block should be accepted");
    }
}

fn apply_formula_blocks(workbook: &mut Workbook, shape: &SeedShape) {
    for block in &shape.formula_blocks {
        workbook
            .set_formulas(
                &shape.sheet_names[block.sheet_index],
                block.start_row,
                block.start_col,
                &block.rows,
            )
            .expect("large fixture formula block should be accepted");
    }
}

fn apply_bounded_value_blocks(workbook: &mut Workbook, shape: &SeedShape) {
    for block in &shape.value_blocks {
        let rows_per_batch = BOUNDED_BLOCK_CELLS / block.rows[0].len();
        for (batch_index, rows) in block.rows.chunks(rows_per_batch).enumerate() {
            workbook
                .set_values(
                    &shape.sheet_names[block.sheet_index],
                    block.start_row + u32::try_from(batch_index * rows_per_batch).unwrap(),
                    block.start_col,
                    rows,
                )
                .expect("bounded fixture value block should be accepted");
        }
    }
}

fn apply_bounded_formula_blocks(workbook: &mut Workbook, shape: &SeedShape) {
    for block in &shape.formula_blocks {
        let rows_per_batch = BOUNDED_BLOCK_CELLS / block.rows[0].len();
        for (batch_index, rows) in block.rows.chunks(rows_per_batch).enumerate() {
            workbook
                .set_formulas(
                    &shape.sheet_names[block.sheet_index],
                    block.start_row + u32::try_from(batch_index * rows_per_batch).unwrap(),
                    block.start_col,
                    rows,
                )
                .expect("bounded fixture formula block should be accepted");
        }
    }
}

fn benchmark_seed_setters(criterion: &mut Criterion) {
    let shape = SeedShape::large_data_class();
    let report_phase_timing = std::env::var_os("FORMUALIZER_SEED_BATCH_TIMING").is_some();
    let mut group = criterion.benchmark_group("seed_batch/workbook_new");
    group.sample_size(SAMPLE_SIZE);
    group.bench_function("set_values_913500_cells_in_row_runs", |bench| {
        bench.iter_batched(
            || empty_interactive_workbook(&shape.sheet_names),
            |mut workbook| {
                apply_values(&mut workbook, &shape);
                black_box(workbook)
            },
            BatchSize::LargeInput,
        );
    });
    group.bench_function("set_formulas_41500_cells_in_row_runs", |bench| {
        bench.iter_batched(
            || empty_interactive_workbook(&shape.sheet_names),
            |mut workbook| {
                apply_formulas(&mut workbook, &shape);
                black_box(workbook)
            },
            BatchSize::LargeInput,
        );
    });
    group.bench_function("set_values_then_formulas_955000_cells", |bench| {
        bench.iter_batched(
            || empty_interactive_workbook(&shape.sheet_names),
            |mut workbook| {
                if report_phase_timing {
                    let values_started = Instant::now();
                    apply_values(&mut workbook, &shape);
                    let values_elapsed = values_started.elapsed();
                    let formulas_started = Instant::now();
                    apply_formulas(&mut workbook, &shape);
                    eprintln!(
                        "one-pass fork setter elapsed: set_values={values_elapsed:?}, set_formulas={:?}",
                        formulas_started.elapsed()
                    );
                } else {
                    apply_values(&mut workbook, &shape);
                    apply_formulas(&mut workbook, &shape);
                }
                black_box(workbook)
            },
            BatchSize::LargeInput,
        );
    });
    group.bench_function("set_values_then_formulas_955000_cells_in_dense_blocks", |bench| {
        bench.iter_batched(
            || empty_interactive_workbook(&shape.sheet_names),
            |mut workbook| {
                if report_phase_timing {
                    let values_started = Instant::now();
                    apply_value_blocks(&mut workbook, &shape);
                    let values_elapsed = values_started.elapsed();
                    let formulas_started = Instant::now();
                    apply_formula_blocks(&mut workbook, &shape);
                    eprintln!(
                        "one-pass fork dense-block setter elapsed: set_values={values_elapsed:?}, set_formulas={:?}",
                        formulas_started.elapsed()
                    );
                } else {
                    apply_value_blocks(&mut workbook, &shape);
                    apply_formula_blocks(&mut workbook, &shape);
                }
                black_box(workbook)
            },
            BatchSize::LargeInput,
        );
    });
    group.bench_function(
        "set_values_then_formulas_955000_cells_in_20k_cell_blocks",
        |bench| {
            bench.iter_batched(
                || empty_interactive_workbook(&shape.sheet_names),
                |mut workbook| {
                    if report_phase_timing {
                        let values_started = Instant::now();
                        apply_bounded_value_blocks(&mut workbook, &shape);
                        let values_elapsed = values_started.elapsed();
                        let formulas_started = Instant::now();
                        apply_bounded_formula_blocks(&mut workbook, &shape);
                        eprintln!(
                            "one-pass fork 20k-block setter elapsed: set_values={values_elapsed:?}, set_formulas={:?}",
                            formulas_started.elapsed()
                        );
                    } else {
                        apply_bounded_value_blocks(&mut workbook, &shape);
                        apply_bounded_formula_blocks(&mut workbook, &shape);
                    }
                    black_box(workbook)
                },
                BatchSize::LargeInput,
            );
        },
    );
    group.finish();
}

criterion_group!(benches, benchmark_seed_setters);
criterion_main!(benches);
