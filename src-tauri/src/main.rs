// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(unix)]
mod allocation_budget;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match startup(&args) {
        Startup::Desktop => vox_bridge_lib::run(),
        Startup::InvalidWorker => std::process::exit(2),
        Startup::Worker { allocation_limit } => {
            // Enable before stdio setup or any worker runtime/thread creation.
            #[cfg(unix)]
            if let Err(error) = allocation_budget::enable_parser_allocation_limit(allocation_limit)
            {
                let code = match error {
                    allocation_budget::EnableBudgetError::BaselineTooLarge => {
                        allocation_budget::PARSER_ALLOC_LIMIT_EXIT
                    }
                    _ => 2,
                };
                // SAFETY: a failed startup budget exits without allocating or
                // running shutdown handlers, just like allocator enforcement.
                unsafe { libc::_exit(code) };
            }
            #[cfg(not(unix))]
            let _ = allocation_limit;
            std::process::exit(vox_bridge_lib::run_document_parse_worker());
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Startup {
    Desktop,
    Worker { allocation_limit: usize },
    InvalidWorker,
}
fn startup(args: &[std::ffi::OsString]) -> Startup {
    const FLAG: &str = "--voxbridge-parse-worker";
    if !args.iter().any(|arg| {
        arg == FLAG
            || arg
                .as_encoded_bytes()
                .starts_with(b"--voxbridge-parse-worker=")
    }) {
        return Startup::Desktop;
    }
    #[cfg(unix)]
    if args.len() == 3 && args[0] == FLAG && args[1] == "--allocation-limit" {
        if let Some(decimal) = args[2].to_str() {
            if !decimal.is_empty() && decimal.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(allocation_limit) = decimal.parse::<usize>() {
                    if allocation_limit > 0 {
                        return Startup::Worker { allocation_limit };
                    }
                }
            }
        }
    }
    #[cfg(windows)]
    if args.len() == 1 && args[0] == FLAG {
        return Startup::Worker {
            allocation_limit: 0,
        };
    }
    Startup::InvalidWorker
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<std::ffi::OsString> {
        values.iter().map(Into::into).collect()
    }
    #[test]
    fn parser_worker_arguments_never_launch_gui_on_malformed_worker_invocation() {
        assert_eq!(startup(&args(&[])), Startup::Desktop);
        assert_eq!(startup(&args(&["--unrelated"])), Startup::Desktop);
        for values in [
            vec!["--voxbridge-parse-worker=1"],
            vec!["--other", "--voxbridge-parse-worker"],
            vec!["--voxbridge-parse-worker", "--allocation-limit", "0"],
            vec!["--voxbridge-parse-worker", "--allocation-limit", "+20"],
            vec![
                "--voxbridge-parse-worker",
                "--allocation-limit",
                "20",
                "extra",
            ],
        ] {
            assert_eq!(startup(&args(&values)), Startup::InvalidWorker);
        }
        #[cfg(unix)]
        {
            assert_eq!(
                startup(&args(&["--voxbridge-parse-worker"])),
                Startup::InvalidWorker
            );
            assert_eq!(
                startup(&args(&[
                    "--voxbridge-parse-worker",
                    "--allocation-limit",
                    "123456"
                ])),
                Startup::Worker {
                    allocation_limit: 123456
                }
            );
        }
        #[cfg(windows)]
        assert_eq!(
            startup(&args(&["--voxbridge-parse-worker"])),
            Startup::Worker {
                allocation_limit: 0
            }
        );
    }
}
