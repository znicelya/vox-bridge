use super::*;

#[derive(Clone)]
pub struct MacOsAudioDriver<R> {
    _runner: R,
}
impl<R> MacOsAudioDriver<R> {
    pub fn new(runner: R) -> Self {
        Self { _runner: runner }
    }
}
impl<R: CommandRunner> AudioDriverManager for MacOsAudioDriver<R> {
    fn status(&self) -> AppResult<DriverStatus> {
        Ok(DriverStatus::NeedsApproval("macOS system audio extension package is unavailable; a licensed bundle and user approval are required".into()))
    }
    fn install(&self) -> AppResult<DriverStatus> {
        Ok(DriverStatus::NeedsApproval("macOS system audio extension package is unavailable; a licensed bundle and user approval are required".into()))
    }
    fn repair(&self) -> AppResult<DriverStatus> {
        self.install()
    }
    fn uninstall(&self) -> AppResult<DriverStatus> {
        Ok(DriverStatus::NeedsApproval(
            "macOS extension removal requires user approval".into(),
        ))
    }
    fn list_endpoints(&self) -> AppResult<Vec<AudioEndpoint>> {
        Ok(Vec::new())
    }
}
