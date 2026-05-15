use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
struct CliArgs {
    config: dam_config::ConfigOverrides,
    file: Option<PathBuf>,
    report: bool,
    json_report: bool,
    strict: bool,
}

fn main() {
    let cli = match parse_args(env::args().skip(1)) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };

    let config = match dam_config::load(&cli.config) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load config: {error}");
            std::process::exit(2);
        }
    };

    let input = match read_input(&cli) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("failed to read input: {error}");
            std::process::exit(1);
        }
    };

    let db_path = match vault_db_path(&config) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let vault = match dam_vault::Vault::open(db_path) {
        Ok(vault) => vault,
        Err(error) => {
            eprintln!("failed to open vault db {}: {error}", db_path.display());
            std::process::exit(1);
        }
    };

    let log_path = match log_db_path(&config) {
        Ok(log_path) => log_path,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let operation_id = dam_core::generate_operation_id();
    let plan = dam_core::build_resolve_plan(&input, &vault);
    record_log_events(log_path, &operation_id, &plan);

    if cli.strict && plan.has_unresolved() {
        print_report(&operation_id, &plan, cli.strict, cli.json_report);
        std::process::exit(1);
    }

    let output = dam_core::apply_resolve_plan(&input, &plan);
    print!("{output}");

    if cli.report {
        print_report(&operation_id, &plan, cli.strict, cli.json_report);
    } else if cli.json_report {
        print_json_report(&operation_id, &plan, cli.strict);
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<CliArgs, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    let mut cli = CliArgs::default();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--report" => cli.report = true,
            "--json-report" => cli.json_report = true,
            "--strict" => cli.strict = true,
            "--config" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--config requires a path".to_string())?;
                cli.config.config_path = Some(PathBuf::from(value));
            }
            "--db" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--db requires a path".to_string())?;
                cli.config.vault_sqlite_path = Some(PathBuf::from(value));
            }
            "--log" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--log requires a path".to_string())?;
                cli.config.log_sqlite_path = Some(PathBuf::from(value));
            }
            "--no-log" => {
                cli.config.log_enabled = Some(false);
            }
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => return Err(format!("unknown argument: {arg}")),
            _ => {
                if cli.file.is_some() {
                    return Err("only one input file is supported".to_string());
                }
                cli.file = Some(PathBuf::from(arg));
            }
        }
        i += 1;
    }

    Ok(cli)
}

fn vault_db_path(config: &dam_config::DamConfig) -> Result<&Path, String> {
    match config.vault.backend {
        dam_config::VaultBackend::Sqlite => Ok(&config.vault.sqlite_path),
        dam_config::VaultBackend::Remote => Err(
            "remote vault backend is configured but not implemented in dam-resolve yet".to_string(),
        ),
    }
}

fn log_db_path(config: &dam_config::DamConfig) -> Result<Option<&Path>, String> {
    if !config.log.enabled || config.log.backend == dam_config::LogBackend::None {
        return Ok(None);
    }

    match config.log.backend {
        dam_config::LogBackend::Sqlite => Ok(Some(&config.log.sqlite_path)),
        dam_config::LogBackend::Remote => Err(
            "remote log backend is configured but not implemented in dam-resolve yet".to_string(),
        ),
        dam_config::LogBackend::None => Ok(None),
    }
}

fn record_log_events(log_path: Option<&Path>, operation_id: &str, plan: &dam_core::ResolvePlan) {
    let Some(log_path) = log_path else {
        return;
    };

    let store = match dam_log::LogStore::open(log_path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!(
                "log_warning failed to open log db {}: {error}",
                log_path.display()
            );
            return;
        }
    };

    for event in dam_core::build_resolve_log_events(operation_id, plan) {
        if let Err(error) = dam_core::EventSink::record(&store, &event) {
            eprintln!("log_warning failed to write log event: {error}");
            return;
        }
    }
}

fn read_input(cli: &CliArgs) -> io::Result<String> {
    match &cli.file {
        Some(path) => fs::read_to_string(path),
        None => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input)?;
            Ok(input)
        }
    }
}

fn print_report(operation_id: &str, plan: &dam_core::ResolvePlan, strict: bool, json_report: bool) {
    if json_report {
        print_json_report(operation_id, plan, strict);
        return;
    }

    eprint!("{}", plain_report(operation_id, plan));
}

fn plain_report(operation_id: &str, plan: &dam_core::ResolvePlan) -> String {
    use std::fmt::Write as _;

    let mut report = String::new();
    writeln!(report, "operation_id: {operation_id}").expect("write plain report");
    writeln!(report, "references: {}", plan.references.len()).expect("write plain report");
    writeln!(report, "resolved: {}", plan.resolved_count()).expect("write plain report");
    writeln!(report, "missing: {}", plan.missing_count()).expect("write plain report");
    writeln!(report, "read_failures: {}", plan.read_failure_count()).expect("write plain report");

    for replacement in &plan.replacements {
        writeln!(
            report,
            "resolved {} {}..{} {}",
            replacement.reference.kind.tag(),
            replacement.span.start,
            replacement.span.end,
            replacement.reference.key()
        )
        .expect("write plain report");
    }
    for missing in &plan.missing {
        writeln!(
            report,
            "missing {} {}..{} {}",
            missing.reference.kind.tag(),
            missing.span.start,
            missing.span.end,
            missing.reference.key()
        )
        .expect("write plain report");
    }
    for failure in &plan.read_failures {
        writeln!(
            report,
            "read_error {} {}..{} {} {}",
            failure.reference.kind.tag(),
            failure.span.start,
            failure.span.end,
            failure.reference.key(),
            dam_api::VAULT_READ_FAILURE_REPORT_ERROR
        )
        .expect("write plain report");
    }

    report
}

fn print_json_report(operation_id: &str, plan: &dam_core::ResolvePlan, strict: bool) {
    let report = dam_api::resolve_report(operation_id, plan, strict);
    match serde_json::to_string_pretty(&report) {
        Ok(json) => eprintln!("{json}"),
        Err(error) => eprintln!("report_warning failed to serialize json report: {error}"),
    }
}

fn usage() -> &'static str {
    "Usage: dam-resolve [--config dam.toml] [--db vault.db] [--log log.db] [--no-log] [--report] [--json-report] [--strict] [FILE]"
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
