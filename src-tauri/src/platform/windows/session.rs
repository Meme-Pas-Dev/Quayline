//! Verrouillage de session et mise en veille (Windows).
//!
//! - Verrouillage (`Win+L`, écran de verrouillage) : `WM_WTSSESSION_CHANGE` /
//!   `WTS_SESSION_LOCK`, reçu par une fenêtre invisible (« message-only ») abonnée avec
//!   `WTSRegisterSessionNotification`, dans un fil dédié qui traite ses messages.
//! - Mise en veille : `PowerRegisterSuspendResumeNotification` (`PBT_APMSUSPEND`), par
//!   rappel, sans fenêtre.

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{PowerRegisterSuspendResumeNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS};
use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage,
    DEVICE_NOTIFY_CALLBACK, HWND_MESSAGE, MSG, PBT_APMSUSPEND, WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK,
};

use super::wide;
use crate::platform::SystemEvent;

static CALLBACK: OnceLock<Box<dyn Fn(SystemEvent) + Send + Sync>> = OnceLock::new();

fn notify(event: SystemEvent) {
    if let Some(cb) = CALLBACK.get() {
        cb(event);
    }
}

/// Surveille la session ; `on_event` est appelé à chaque verrouillage ou mise en veille.
/// Une source indisponible est simplement ignorée.
pub fn watch(on_event: impl Fn(SystemEvent) + Send + Sync + 'static) {
    if CALLBACK.set(Box::new(on_event)).is_err() {
        return;
    }
    watch_suspend();
    let _ = std::thread::Builder::new().name("session-watch".into()).spawn(watch_lock);
}

unsafe extern "system" fn on_power(_context: *const core::ffi::c_void, kind: u32, _setting: *const core::ffi::c_void) -> u32 {
    if kind == PBT_APMSUSPEND {
        notify(SystemEvent::Suspending);
    }
    0
}

fn watch_suspend() {
    // Paramètres lus par Windows tant que l'abonnement dure, c'est-à-dire jusqu'à la fin
    // du processus : volontairement jamais libérés.
    let params = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(on_power), Context: std::ptr::null_mut() }));
    let mut handle = std::ptr::null_mut();
    let status = unsafe {
        PowerRegisterSuspendResumeNotification(DEVICE_NOTIFY_CALLBACK, std::ptr::from_mut(params).cast(), &mut handle)
    };
    if status != 0 {
        eprintln!("{}: cannot watch for sleep ({status})", crate::identity::SLUG);
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_WTSSESSION_CHANGE && wparam as u32 == WTS_SESSION_LOCK {
        notify(SystemEvent::SessionLocked);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn watch_lock() {
    let class = wide(&format!("{}-session-watch", crate::identity::SLUG));
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance, lpszClassName: class.as_ptr(), ..Default::default() };
        if RegisterClassW(&wc) == 0 {
            return;
        }
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() || WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
            eprintln!("{}: cannot watch for session lock", crate::identity::SLUG);
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
