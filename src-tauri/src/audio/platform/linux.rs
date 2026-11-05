use super::*;

#[derive(Clone)]
pub struct LinuxAudioDriver<R> {
    runner: R,
}

impl<R> LinuxAudioDriver<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: CommandRunner> LinuxAudioDriver<R> {
    fn probe_pipewire(&self) -> AppResult<Vec<AudioEndpoint>> {
        let output = run_command(&self.runner, "pw-cli", &["ls", "Node"])?;
        if output.unavailable {
            return Ok(Vec::new());
        }
        if command_failed(&output, "pw-cli").is_some() {
            return Err(AppError::Internal(format!(
                "pw-cli probe failed: {output:?}"
            )));
        }
        Ok(parse_pipewire(&output.stdout))
    }

    fn probe_pulse(&self) -> AppResult<Vec<AudioEndpoint>> {
        let sources = run_command(&self.runner, "pactl", &["list", "short", "sources"])?;
        if sources.unavailable {
            return Ok(Vec::new());
        }
        let sinks = run_command(&self.runner, "pactl", &["list", "short", "sinks"])?;
        if sinks.unavailable {
            return Ok(Vec::new());
        }
        if let Some(status) = command_failed(&sources, "pactl list sources") {
            return Err(AppError::Internal(format!(
                "pactl sources probe failed: {status:?}"
            )));
        }
        if let Some(status) = command_failed(&sinks, "pactl list sinks") {
            return Err(AppError::Internal(format!(
                "pactl sinks probe failed: {status:?}"
            )));
        }
        let mut endpoints = parse_pulse(&sources.stdout, AudioDirection::Input);
        endpoints.extend(parse_pulse(&sinks.stdout, AudioDirection::Output));
        Ok(endpoints)
    }
}

impl<R: CommandRunner> AudioDriverManager for LinuxAudioDriver<R> {
    fn status(&self) -> AppResult<DriverStatus> {
        let endpoints = self.list_endpoints()?;
        Ok(readiness(&endpoints))
    }

    fn install(&self) -> AppResult<DriverStatus> {
        let modules = run_command(&self.runner, "pactl", &["list", "short", "modules"])?;
        if modules.unavailable {
            return Ok(DriverStatus::Failed(
                "pactl is unavailable in the current Linux audio session".into(),
            ));
        }
        if let Some(status) = command_failed(&modules, "pactl list modules") {
            return Ok(status);
        }
        let ids = module_ids(&modules.stdout);
        if has_complete_module_set(&modules.stdout) {
            return self.status();
        }
        for module_id in ids {
            let output = run_command(
                &self.runner,
                "pactl",
                &["unload-module", module_id.as_str()],
            )?;
            if let Some(status) = command_failed(&output, "pactl unload-module") {
                return Ok(status);
            }
        }
        let output = run_command(
            &self.runner,
            "pactl",
            &[
                "load-module",
                "module-null-sink",
                "sink_name=VoxBridge_M1",
                "sink_properties=device.description=VoxBridge M1 Backend",
            ],
        )?;
        if let Some(status) = command_failed(&output, "pactl load-module M1 backend") {
            return Ok(status);
        }
        let output = run_command(
            &self.runner,
            "pactl",
            &[
                "load-module",
                "module-remap-source",
                "master=VoxBridge_M1.monitor",
                "source_name=VoxBridge_Input",
                "source_properties=device.description=VoxBridge Input",
            ],
        )?;
        if let Some(status) = command_failed(&output, "pactl load-module M1 source") {
            return Ok(status);
        }
        let output = run_command(
            &self.runner,
            "pactl",
            &[
                "load-module",
                "module-null-sink",
                "sink_name=VoxBridge_Output",
                "sink_properties=device.description=VoxBridge Output",
            ],
        )?;
        if let Some(status) = command_failed(&output, "pactl load-module M2 output") {
            return Ok(status);
        }
        self.status()
    }

    fn repair(&self) -> AppResult<DriverStatus> {
        self.install()
    }

    fn uninstall(&self) -> AppResult<DriverStatus> {
        let modules = run_command(&self.runner, "pactl", &["list", "short", "modules"])?;
        if let Some(status) = command_failed(&modules, "pactl list modules") {
            return Ok(status);
        }
        for module_id in module_ids(&modules.stdout) {
            let output = run_command(
                &self.runner,
                "pactl",
                &["unload-module", module_id.as_str()],
            )?;
            if let Some(status) = command_failed(&output, "pactl unload-module") {
                return Ok(status);
            }
        }
        Ok(DriverStatus::Ready)
    }

    fn list_endpoints(&self) -> AppResult<Vec<AudioEndpoint>> {
        let mut endpoints = self.probe_pipewire()?;
        endpoints.extend(self.probe_pulse()?);
        endpoints.dedup_by(|left, right| left.id == right.id && left.direction == right.direction);
        Ok(endpoints)
    }
}

fn parse_pipewire(text: &str) -> Vec<AudioEndpoint> {
    let mut endpoints = Vec::new();
    for line in text.lines() {
        let direction = if line.contains("VoxBridge_Input") {
            Some(AudioDirection::Input)
        } else if line.contains("VoxBridge_Output") {
            Some(AudioDirection::Output)
        } else {
            None
        };
        if let Some(direction) = direction {
            let display_name = if direction == AudioDirection::Input {
                "VoxBridge Input"
            } else {
                "VoxBridge Output"
            };
            endpoints.push(AudioEndpoint {
                id: display_name.replace(' ', "_"),
                display_name: display_name.into(),
                direction,
                sample_rates: vec![16_000, 24_000, 48_000],
                channels: 1,
                ready: true,
            });
        }
    }
    endpoints
}

fn parse_pulse(text: &str, direction: AudioDirection) -> Vec<AudioEndpoint> {
    text.lines()
        .filter_map(|line| {
            let lower = line.to_ascii_lowercase();
            let display_name = if direction == AudioDirection::Input
                && (lower.contains("voxbridge_input") || lower.contains("voxbridge input"))
            {
                "VoxBridge Input"
            } else if direction == AudioDirection::Output
                && (lower.contains("voxbridge_output") || lower.contains("voxbridge output"))
            {
                "VoxBridge Output"
            } else {
                return None;
            };
            Some(AudioEndpoint {
                id: display_name.replace(' ', "_"),
                display_name: display_name.into(),
                direction: direction.clone(),
                sample_rates: vec![16_000, 24_000, 48_000],
                channels: 1,
                ready: true,
            })
        })
        .collect()
}

fn module_ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let id = fields.next()?;
            let name = fields.next()?;
            let args = fields.collect::<Vec<_>>();
            (id.bytes().all(|byte| byte.is_ascii_digit())
                && !id.is_empty()
                && is_voxbridge_module(name, &args))
            .then(|| id.to_owned())
        })
        .collect()
}

fn is_voxbridge_module(name: &str, args: &[&str]) -> bool {
    (name == "module-null-sink"
        && (args.contains(&"sink_name=VoxBridge_M1")
            || args.contains(&"sink_name=VoxBridge_Output")))
        || (name == "module-remap-source" && args.contains(&"source_name=VoxBridge_Input"))
}

fn has_complete_module_set(text: &str) -> bool {
    let mut m1_backend = 0;
    let mut m1_source = 0;
    let mut m2_sink = 0;
    let mut voxbridge_modules = 0;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(id) = fields.next() else { continue };
        let Some(name) = fields.next() else { continue };
        let args = fields.collect::<Vec<_>>();
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if is_voxbridge_module(name, &args) {
            voxbridge_modules += 1;
        }
        if name == "module-null-sink" && args.contains(&"sink_name=VoxBridge_M1") {
            m1_backend += 1;
        }
        if name == "module-remap-source"
            && args.contains(&"source_name=VoxBridge_Input")
            && args.contains(&"master=VoxBridge_M1.monitor")
        {
            m1_source += 1;
        }
        if name == "module-null-sink" && args.contains(&"sink_name=VoxBridge_Output") {
            m2_sink += 1;
        }
    }
    m1_backend == 1 && m1_source == 1 && m2_sink == 1 && voxbridge_modules == 3
}
