use std::{env, process};

use crate::report::OutputFormat;

pub(crate) fn parse_env_args() -> Result<OutputFormat, String> {
    parse_args(env::args().skip(1))
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<OutputFormat, String> {
    let mut args = args.into_iter();
    let mut format = OutputFormat::Text;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--format requires text, json, or markdown".to_string())?;
                format = match value.as_str() {
                    "text" => OutputFormat::Text,
                    "json" => OutputFormat::Json,
                    "markdown" => OutputFormat::Markdown,
                    _ => {
                        return Err(format!(
                            "unsupported format {value:?}; use text, json, or markdown"
                        ));
                    }
                };
            }
            "-h" | "--help" => {
                println!("Usage: cargo run -p dam-detect-bench -- [--format text|json|markdown]");
                process::exit(0);
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }

    Ok(format)
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
