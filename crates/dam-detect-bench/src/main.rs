use std::process;

use crate::{cli::parse_env_args, evaluator::run_benchmark, report::print_report};

mod cases;
mod cli;
mod evaluator;
mod metrics;
mod report;

fn main() {
    let format = match parse_env_args() {
        Ok(format) => format,
        Err(message) => {
            eprintln!("{message}");
            process::exit(2);
        }
    };

    match run_benchmark() {
        Ok(report) => {
            print_report(&report, format);
            if report.summary.false_negatives > 0 || report.summary.false_positives > 0 {
                process::exit(1);
            }
        }
        Err(message) => {
            eprintln!("{message}");
            process::exit(2);
        }
    }
}
