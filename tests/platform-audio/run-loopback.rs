//! Debug-only audio diagnostics. Never starts a device without explicit physical arguments.

#[cfg(debug_assertions)]
mod physical;

#[cfg(debug_assertions)]
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Synthetic(u32),
    ListDevices,
    Physical(physical::Options),
    Help,
}

#[cfg(debug_assertions)]
fn parse_args(args: &[String]) -> Result<Action, String> {
    if args.is_empty() || args == ["--help"] {
        return Ok(Action::Help);
    }
    if args == ["--list-devices"] {
        return Ok(Action::ListDevices);
    }
    let Some(mode) = args.first() else {
        return Ok(Action::Help);
    };
    let mut options = std::collections::BTreeMap::new();
    for pair in args[1..].chunks(2) {
        if pair.len() != 2 || options.insert(pair[0].as_str(), pair[1].as_str()).is_some() {
            return Err("options must be unique flag/value pairs".into());
        }
    }
    let number = |value: &str| {
        value
            .parse::<u32>()
            .map_err(|_| format!("invalid integer: {value}"))
    };
    match mode.as_str() {
        "--synthetic" => {
            let frames = number(options.remove("--frames").unwrap_or("50"))?;
            if !options.is_empty() || !(1..=250).contains(&frames) {
                return Err("synthetic mode accepts --frames 1..250 only".into());
            }
            Ok(Action::Synthetic(frames))
        }
        "--physical" => {
            let input = options
                .remove("--input")
                .ok_or("--input is required")?
                .to_owned();
            let output = options
                .remove("--output")
                .ok_or("--output is required")?
                .to_owned();
            let seconds = number(options.remove("--seconds").ok_or("--seconds is required")?)?;
            let timeout_seconds = number(
                options
                    .remove("--timeout-seconds")
                    .ok_or("--timeout-seconds is required")?,
            )?;
            if !options.is_empty()
                || input.trim().is_empty()
                || output.trim().is_empty()
                || !(1..=5).contains(&seconds)
                || timeout_seconds < seconds.saturating_add(2)
                || timeout_seconds > 15
            {
                return Err("physical mode requires input/output names, seconds 1..5, and timeout-seconds between seconds+2 and 15".into());
            }
            Ok(Action::Physical(physical::Options {
                input,
                output,
                seconds,
                timeout_seconds,
            }))
        }
        _ => Err(format!("unknown mode: {mode}")),
    }
}

#[cfg(debug_assertions)]
fn main() {
    if let Err(error) = run() {
        eprintln!("loopback failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(debug_assertions)]
fn run() -> Result<(), String> {
    use vox_bridge_lib::audio::loopback::run_synthetic;
    let action = parse_args(&std::env::args().skip(1).collect::<Vec<_>>())?;
    let output = match action {
        Action::Help => {
            println!("Debug audio diagnostics (no provider calls):\n  --synthetic [--frames 1..250]\n  --list-devices\n  --physical --input EXACT_NAME --output EXACT_NAME --seconds 1..5 --timeout-seconds N\nPhysical mode emits a -20 dBFS 1 kHz tone. N must be between seconds+2 and 15.\nSynthetic counts are not hardware evidence; physical counts do not prove acoustic content or translation.");
            return Ok(());
        }
        Action::Synthetic(frames) => {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error| error.to_string())?;
            serde_json::to_value(runtime.block_on(run_synthetic(frames)).map_err(|error| error.to_string())?)
        }
        Action::ListDevices => serde_json::to_value(physical::list_devices()?),
        Action::Physical(options) => serde_json::to_value(physical::run(options)?),
    }.map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[cfg(not(debug_assertions))]
fn main() {
    eprintln!("run-loopback is only available in debug builds");
    std::process::exit(2);
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Action, String> {
        parse_args(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn no_arguments_cannot_open_audio_devices() {
        assert_eq!(parse(&[]).unwrap(), Action::Help);
        assert_eq!(parse(&["--synthetic"]).unwrap(), Action::Synthetic(50));
        assert_eq!(
            parse(&["--synthetic", "--frames", "1"]).unwrap(),
            Action::Synthetic(1)
        );
        assert_eq!(parse(&["--list-devices"]).unwrap(), Action::ListDevices);
    }

    #[test]
    fn physical_mode_requires_exact_devices_duration_and_timeout() {
        let args = [
            "--physical",
            "--input",
            "capture",
            "--output",
            "render",
            "--seconds",
            "1",
            "--timeout-seconds",
            "3",
        ];
        assert!(matches!(parse(&args), Ok(Action::Physical(_))));
        for omit in [1, 3, 5, 7] {
            let mut incomplete = args.to_vec();
            incomplete.drain(omit..omit + 2);
            assert!(parse(&incomplete).is_err());
        }
        for bad in ["0", "6", "4294967295", "invalid"] {
            let mut invalid = args.to_vec();
            invalid[6] = bad;
            assert!(parse(&invalid).is_err());
        }
        for bad in ["0", "2", "16"] {
            let mut invalid = args.to_vec();
            invalid[8] = bad;
            assert!(parse(&invalid).is_err());
        }
    }

    #[test]
    fn rejects_ambiguous_and_invalid_cli_options() {
        for args in [
            vec!["--synthetic", "--physical"],
            vec!["--synthetic", "--frames", "0"],
            vec!["--synthetic", "--frames", "251"],
            vec!["--synthetic", "--frames", "1", "--frames", "2"],
            vec!["--unknown"],
        ] {
            assert!(parse(&args).is_err());
        }
    }
}
