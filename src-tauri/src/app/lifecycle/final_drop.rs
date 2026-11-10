//! Final-facade cleanup owns only the existing close producers, never AppState.
use super::{CloseState, closing_error};
use crate::{
    audio::{
        join::{AudioCloseJoin, AudioCloseKey},
        runtime::AudioBackend,
    },
    error::{AppError, AppResult},
    session::SessionOrchestrator,
    summary::SummaryDeliveryOwner,
};
use futures_util::FutureExt;
use std::{
    fs::File,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(in crate::app) struct FinalAppFacade {
    owner: Option<FinalClose>,
    executor: Mutex<Option<tokio::runtime::Handle>>,
}
struct FinalClose {
    process: Arc<File>,
    audio: Arc<dyn AudioBackend>,
    orchestrator: SessionOrchestrator,
    admission: Arc<tokio::sync::Mutex<()>>,
    state: Arc<Mutex<CloseState>>,
    delivery: SummaryDeliveryOwner,
}
// A task can be cancelled before its first poll, including when its captured
// executor has stopped. Only positive source/drain joins may disarm this owner.
struct FinalCloseGuard {
    owner: Option<FinalClose>,
}
impl FinalAppFacade {
    pub(in crate::app) fn new(
        process: Arc<File>,
        audio: Arc<dyn AudioBackend>,
        orchestrator: SessionOrchestrator,
        admission: Arc<tokio::sync::Mutex<()>>,
        state: Arc<Mutex<CloseState>>,
        delivery: SummaryDeliveryOwner,
    ) -> Arc<Self> {
        Arc::new(Self {
            owner: Some(FinalClose {
                process,
                audio,
                orchestrator,
                admission,
                state,
                delivery,
            }),
            executor: Mutex::new(None),
        })
    }
    pub(in crate::app) fn record_executor(&self) {
        if let Ok(current) = tokio::runtime::Handle::try_current() {
            if let Ok(mut executor) = self.executor.lock() {
                *executor = Some(current);
            }
        }
    }
}
impl FinalClose {
    fn capture(&self) -> AppResult<(AudioCloseKey, AudioCloseJoin)> {
        let mut state = self.state.lock().map_err(|_| closing_error())?;
        if !state.closing {
            state.epoch = state.epoch.checked_add(1).ok_or_else(closing_error)?;
            state.close_succeeded = false;
        }
        state.closing = true;
        let key = self
            .delivery
            .elect_closing(state.epoch, self.audio.actual_epoch()?)?;
        self.delivery.signal_closing(&key)?;
        if let Some(audio) = &state.audio {
            if state.audio_key.as_ref() != Some(&key) {
                return Err(closing_error());
            }
            return Ok((key, audio.clone()));
        }
        state.audio_key = Some(key.clone());
        let native = self.audio.begin_close(key.clone())?;
        let session = self.orchestrator.capture_close_join(key.clone())?;
        let orchestrator = self.orchestrator.clone();
        let admission = self.admission.clone();
        let audio = AudioCloseJoin::start_owned(
            self.process.clone(),
            key.clone(),
            session,
            native,
            async move {
                let early = orchestrator.stop_active().await;
                let _admission = admission.lock().await;
                let final_stop = orchestrator.stop_active().await;
                early.and(final_stop)
            },
        )?;
        state.audio = Some(audio.clone());
        Ok((key, audio))
    }
    fn failure(&self, branch: usize, error: AppError) {
        if let Ok(mut state) = self.state.lock() {
            state.failures[branch] = Some(error.to_string());
            state.close_succeeded = false;
            if state.first_failure.is_none() {
                state.first_failure = Some(state.failures.clone());
            }
        }
    }
    fn finished(&self, audio: &AudioCloseJoin, delivery: AppResult<()>) {
        if let Some(Ok(ack)) = audio.wait().now_or_never() {
            if let Ok(mut state) = self.state.lock() {
                state.audio_ack = Some(ack);
            }
        }
        if let Err(error) = audio.outcome() {
            self.failure(0, error);
        }
        if let Err(error) = delivery {
            self.failure(6, error);
        }
        // Other final-drop owners independently retain their process scopes.
        // This narrow branch cannot publish complete-app success or readiness.
    }
    fn retain_unknown(self, branch: usize, error: AppError) {
        self.failure(branch, error);
        // A missing producer/runtime is not a join proof. Keep the original
        // process and native registries fenced instead of releasing on drop.
        std::mem::forget(self);
    }
}
impl FinalCloseGuard {
    fn owner(&self) -> &FinalClose {
        self.owner.as_ref().expect("armed final close owner")
    }
    fn joined(mut self, audio: &AudioCloseJoin, delivery: AppResult<()>) {
        self.owner().finished(audio, delivery);
        drop(self.owner.take());
    }
    fn retain_unknown(mut self, branch: usize, error: AppError) {
        if let Some(owner) = self.owner.take() {
            owner.retain_unknown(branch, error);
        }
    }
    async fn finish(self, key: AudioCloseKey, audio: AudioCloseJoin) {
        loop {
            match audio.wait().await {
                Ok(_) => break,
                Err(error) => {
                    self.owner().failure(0, error);
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
        let delivery = self
            .owner()
            .delivery
            .close_final_facade_owned(self.owner().process.clone(), key, audio.clone())
            .await;
        match delivery {
            Ok(joined) => self.joined(&audio, joined.outcome()),
            Err(error) => self.retain_unknown(6, error),
        }
    }
}
impl Drop for FinalCloseGuard {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            owner.retain_unknown(6, closing_error());
        }
    }
}
impl Drop for FinalAppFacade {
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let owner = FinalCloseGuard { owner: Some(owner) };
        let runtime = self
            .executor
            .get_mut()
            .ok()
            .and_then(Option::take)
            .or_else(|| tokio::runtime::Handle::try_current().ok());
        // Capture/start_owned and every later spawn use the existing entered
        // executor, independently of the thread that drops the last facade.
        // Entering a Handle does not keep its Runtime alive; the guard above
        // remains armed if that executor cancels the newly spawned owner.
        let _entered = runtime.as_ref().map(tokio::runtime::Handle::enter);
        let (key, audio) = match owner.owner().capture() {
            Ok(captured) => captured,
            Err(error) => {
                owner.retain_unknown(0, error);
                return;
            }
        };
        if let Some(delivery) = owner.owner().delivery.try_drain_after_audio(&key, &audio) {
            match delivery {
                Ok(joined) => owner.joined(&audio, joined.outcome()),
                Err(error) => owner.retain_unknown(6, error),
            }
            return;
        }
        let Some(runtime) = runtime.as_ref() else {
            owner.retain_unknown(0, closing_error());
            return;
        };
        let state = owner.owner().state.clone();
        let task = runtime.spawn(owner.finish(key, audio));
        if let Ok(mut state) = state.lock() {
            state.facade_task = Some(task);
        }
    }
}
