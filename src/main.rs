//! Holocron CLI — compile a YAML schema into PostgreSQL DDL.

use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use holocron::compile;

const USAGE: &str = "\
Usage: holocron [FILE]

Compiles a YAML schema into PostgreSQL DDL.

Reads from FILE if given (or '-' / no arg for stdin).
Writes DDL to stdout; errors to stderr.";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let first = args.next();
    if matches!(first.as_deref(), Some("--help") | Some("-h")) {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if args.next().is_some() {
        eprintln!("error: too many arguments\n\n{USAGE}");
        return ExitCode::FAILURE;
    }

    let filename = first
        .as_deref()
        .filter(|name| *name != "-")
        .unwrap_or("<stdin>");
    let input = match read_input(first.as_deref()) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };

    match compile(&input) {
        Ok(result) => {
            print!("{}", result.ddl);
            ExitCode::SUCCESS
        }
        Err(errors) => {
            // Render every diagnostic the failing phase collected — one ariadne
            // report per error, so the user sees the full list in one run.
            for error in &errors {
                eprint!("{}", error.render(filename, &input));
            }
            ExitCode::FAILURE
        }
    }
}

fn read_input(path: Option<&str>) -> io::Result<String> {
    match path {
        Some("-") | None => {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            Ok(buffer)
        }
        Some(file) => fs::read_to_string(file),
    }
}
