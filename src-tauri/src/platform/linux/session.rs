//! Verrouillage de session et mise en veille (Linux, D-Bus).
//!
//! Trois sources, car les bureaux ne se comportent pas tous pareil :
//! - logind `PrepareForSleep(true)` : la machine va se mettre en veille ;
//! - logind, propriété `LockedHint` de la session : posée par l'écran de
//!   verrouillage de KDE (kscreenlocker) comme de GNOME ;
//! - logind, signal `Lock` (`loginctl lock-session`) et `org.freedesktop.ScreenSaver`
//!   `ActiveChanged` sur le bus de session, en complément.

use std::sync::Arc;

use futures_util::StreamExt;
use zbus::zvariant::OwnedObjectPath;
use zbus::{proxy, Connection};

use crate::platform::SystemEvent;

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LoginManager {
    fn get_session(&self, session_id: &str) -> zbus::Result<OwnedObjectPath>;
    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

#[proxy(interface = "org.freedesktop.login1.Session", default_service = "org.freedesktop.login1")]
trait LoginSession {
    #[zbus(property)]
    fn locked_hint(&self) -> zbus::Result<bool>;
    #[zbus(signal)]
    fn lock(&self) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.ScreenSaver",
    default_service = "org.freedesktop.ScreenSaver",
    default_path = "/ScreenSaver"
)]
trait ScreenSaver {
    #[zbus(signal)]
    fn active_changed(&self, active: bool) -> zbus::Result<()>;
}

type Callback = Arc<dyn Fn(SystemEvent) + Send + Sync>;

/// Surveille la session ; `on_event` est appelé à chaque verrouillage ou mise en veille.
/// Les sources indisponibles (bus absent, bac à sable…) sont simplement ignorées.
pub fn watch(on_event: impl Fn(SystemEvent) + Send + Sync + 'static) {
    let cb: Callback = Arc::new(on_event);
    tauri::async_runtime::spawn(watch_logind(cb.clone()));
    tauri::async_runtime::spawn(watch_screensaver(cb));
}

async fn watch_logind(cb: Callback) {
    let Ok(system) = Connection::system().await else { return };
    let Ok(manager) = LoginManagerProxy::new(&system).await else { return };

    let sleep_cb = cb.clone();
    if let Ok(mut sleeps) = manager.receive_prepare_for_sleep().await {
        tauri::async_runtime::spawn(async move {
            while let Some(signal) = sleeps.next().await {
                if signal.args().map(|a| a.start).unwrap_or(false) {
                    sleep_cb(SystemEvent::Suspending);
                }
            }
        });
    }

    // « auto » désigne la session de l'appelant.
    let Ok(path) = manager.get_session("auto").await else { return };
    let Ok(session) = LoginSessionProxy::builder(&system).path(path).map(|b| b.build()) else { return };
    let Ok(session) = session.await else { return };

    let lock_cb = cb.clone();
    if let Ok(mut locks) = session.receive_lock().await {
        tauri::async_runtime::spawn(async move {
            while locks.next().await.is_some() {
                lock_cb(SystemEvent::SessionLocked);
            }
        });
    }

    let mut hints = session.receive_locked_hint_changed().await;
    while let Some(change) = hints.next().await {
        if change.get().await.unwrap_or(false) {
            cb(SystemEvent::SessionLocked);
        }
    }
}

async fn watch_screensaver(cb: Callback) {
    let Ok(conn) = Connection::session().await else { return };
    let Ok(saver) = ScreenSaverProxy::new(&conn).await else { return };
    let Ok(mut changes) = saver.receive_active_changed().await else { return };
    while let Some(signal) = changes.next().await {
        if signal.args().map(|a| a.active).unwrap_or(false) {
            cb(SystemEvent::SessionLocked);
        }
    }
}
