//! Event-driven Windows media playback state for contextual mascot motion.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
};
pub fn watch() -> Arc<AtomicBool> {
    let playing = Arc::new(AtomicBool::new(false));
    let state = playing.clone();
    std::thread::spawn(move || {
        let run = || -> windows::core::Result<()> {
            unsafe {
                RoInitialize(RO_INIT_MULTITHREADED)?;
            }
            let manager = Manager::RequestAsync()?.get()?;
            let (tx, rx) = mpsc::channel::<()>();
            let changed = tx.clone();
            let _manager_token =
                manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = changed.send(());
                    Ok(())
                }))?;
            let mut watched: Option<(Session, windows::Foundation::EventRegistrationToken)> = None;
            loop {
                if let Some((session, token)) = watched.take() {
                    let _ = session.RemovePlaybackInfoChanged(token);
                }
                match manager.GetCurrentSession() {
                    Ok(session) => {
                        let active = session
                            .GetPlaybackInfo()
                            .and_then(|info| info.PlaybackStatus())
                            .map(|status| status == Status::Playing)
                            .unwrap_or(false);
                        state.store(active, Ordering::Relaxed);
                        let changed = tx.clone();
                        let token =
                            session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                                let _ = changed.send(());
                                Ok(())
                            }))?;
                        // Read once more after subscribing to close the snapshot race.
                        state.store(
                            session
                                .GetPlaybackInfo()
                                .and_then(|i| i.PlaybackStatus())
                                .map(|s| s == Status::Playing)
                                .unwrap_or(false),
                            Ordering::Relaxed,
                        );
                        watched = Some((session, token));
                    }
                    Err(_) => state.store(false, Ordering::Relaxed),
                }
                if rx.recv().is_err() {
                    break;
                }
            }
            Ok(())
        };
        if run().is_err() {
            state.store(false, Ordering::Relaxed);
        }
    });
    playing
}
