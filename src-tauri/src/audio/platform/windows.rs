use super::*;

#[derive(Clone)]
pub struct WindowsAudioDriver<R> {
    _runner: R,
}
impl<R> WindowsAudioDriver<R> {
    pub fn new(runner: R) -> Self {
        Self { _runner: runner }
    }
}
impl<R: CommandRunner> AudioDriverManager for WindowsAudioDriver<R> {
    fn status(&self) -> AppResult<DriverStatus> {
        Ok(DriverStatus::Failed("signed endpoint package is unavailable; Windows installation is blocked by the license gate".into()))
    }
    fn install(&self) -> AppResult<DriverStatus> {
        Ok(DriverStatus::Failed("signed endpoint package is unavailable; Windows installation is blocked by the license gate".into()))
    }
    fn repair(&self) -> AppResult<DriverStatus> {
        self.install()
    }
    fn uninstall(&self) -> AppResult<DriverStatus> {
        Ok(DriverStatus::NeedsPrivileges)
    }
    fn list_endpoints(&self) -> AppResult<Vec<AudioEndpoint>> {
        Ok(Vec::new())
    }
}
