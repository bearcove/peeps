use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use dialoguer::FuzzySelect;

type AnyResult<T> = Result<T, String>;

struct Args {
    examples_dir: PathBuf,
    last_file: PathBuf,
    list: bool,
    requested: Option<String>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> AnyResult<()> {
    let args = parse_args()?;
    let examples = discover_examples(&args.examples_dir)?;
    let last = read_last_example(&args.last_file);
    let ordered = ordered_examples(&examples, last.as_deref());

    if args.list {
        for example in ordered {
            println!("{example}");
        }
        return Ok(());
    }

    let selected = if let Some(requested) = args.requested {
        resolve_requested(&examples, &requested)?
    } else {
        pick_interactively(&ordered, last.as_deref())?
    };

    let _ = fs::write(&args.last_file, format!("{selected}\n"));
    println!("{selected}");
    Ok(())
}

fn parse_args() -> AnyResult<Args> {
    let mut args = env::args().skip(1);
    let mut examples_dir: Option<PathBuf> = None;
    let mut last_file: Option<PathBuf> = None;
    let mut list = false;
    let mut requested: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--examples-dir" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --examples-dir".to_owned())?;
                examples_dir = Some(PathBuf::from(value));
            }
            "--last-file" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --last-file".to_owned())?;
                last_file = Some(PathBuf::from(value));
            }
            "--list" => {
                list = true;
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => {
                return Err(format!("Unknown option '{arg}'"));
            }
            _ => {
                if requested.is_some() {
                    return Err("Too many arguments".to_owned());
                }
                requested = Some(arg);
            }
        }
    }

    let examples_dir = examples_dir.ok_or_else(|| "Missing required --examples-dir".to_owned())?;
    let last_file = last_file.ok_or_else(|| "Missing required --last-file".to_owned())?;

    Ok(Args {
        examples_dir,
        last_file,
        list,
        requested,
    })
}

fn print_help() {
    eprintln!("Usage: peeps-example-selector --examples-dir <path> --last-file <path> [--list] [example-name]");
}

fn discover_examples(examples_dir: &Path) -> AnyResult<Vec<String>> {
    let mut names = Vec::new();

    let entries = fs::read_dir(examples_dir).map_err(|e| {
        format!(
            "Failed to read examples dir '{}': {e}",
            examples_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if path.join("Cargo.toml").is_file() {
            let name = entry.file_name();
            names.push(name.to_string_lossy().to_string());
        }
    }

    names.sort();

    if names.is_empty() {
        return Err(format!("No examples found in {}", examples_dir.display()));
    }

    Ok(names)
}

fn read_last_example(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn ordered_examples(examples: &[String], last: Option<&str>) -> Vec<String> {
    let mut ordered = Vec::with_capacity(examples.len());

    if let Some(last_name) = last {
        if examples.iter().any(|e| e == last_name) {
            ordered.push(last_name.to_owned());
        }
    }

    for example in examples {
        if Some(example.as_str()) == last {
            continue;
        }
        ordered.push(example.clone());
    }

    ordered
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn resolve_requested(examples: &[String], requested: &str) -> AnyResult<String> {
    if let Some(exact) = examples.iter().find(|name| *name == requested) {
        return Ok(exact.clone());
    }

    if let Some(substring_match) = examples
        .iter()
        .find(|name| contains_case_insensitive(name, requested))
    {
        eprintln!(
            "Using closest example match '{}' for '{}'.",
            substring_match, requested
        );
        return Ok(substring_match.clone());
    }

    eprintln!("Unknown example '{requested}'. Available examples:");
    for example in examples {
        eprintln!("{example}");
    }
    Err("No matching example found".to_owned())
}

fn pick_interactively(ordered: &[String], last: Option<&str>) -> AnyResult<String> {
    if !io::stdin().is_terminal() {
        return Err(
            "No example name provided and interactive picker is unavailable (no TTY).".to_owned(),
        );
    }

    let mut labels = Vec::with_capacity(ordered.len());
    for example in ordered {
        if Some(example.as_str()) == last {
            labels.push(format!("{example} (last)"));
        } else {
            labels.push(example.clone());
        }
    }

    let selection = FuzzySelect::new()
        .with_prompt("Select an example (type to filter)")
        .items(&labels)
        .default(0)
        .interact_opt()
        .map_err(|e| format!("Interactive picker failed: {e}"))?;

    let idx = selection.ok_or_else(|| "Selection cancelled".to_owned())?;
    Ok(ordered[idx].clone())
}
