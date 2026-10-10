//! Verrouillage de session et mise en veille (macOS, notifications Cocoa).
//!
//! - `NSWorkspaceWillSleepNotification` : la machine va se mettre en veille ;
//! - `com.apple.screenIsLocked` (notification distribuée) : l'écran vient d'être
//!   verrouillé (`Ctrl+Cmd+Q`, économiseur d'écran, fermeture du capot…) ;
//! - `NSWorkspaceSessionDidResignActiveNotification` : changement rapide
//!   d'utilisateur, la session passe en arrière-plan.

use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use objc2_app_kit::{NSWorkspace, NSWorkspaceSessionDidResignActiveNotification, NSWorkspaceWillSleepNotification};
use objc2_foundation::{
    ns_string, NSDistributedNotificationCenter, NSNotification, NSNotificationCenter, NSNotificationName,
    NSOperationQueue,
};

use crate::platform::SystemEvent;

type Callback = Arc<dyn Fn(SystemEvent) + Send + Sync>;

/// Surveille la session ; `on_event` est appelé (sur le fil principal) à chaque
/// verrouillage ou mise en veille.
pub fn watch(on_event: impl Fn(SystemEvent) + Send + Sync + 'static) {
    let cb: Callback = Arc::new(on_event);
    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    let distributed = NSDistributedNotificationCenter::defaultCenter();
    unsafe {
        observe(&workspace, NSWorkspaceWillSleepNotification, SystemEvent::Suspending, cb.clone());
        observe(&workspace, NSWorkspaceSessionDidResignActiveNotification, SystemEvent::SessionLocked, cb.clone());
    }
    observe(&distributed, ns_string!("com.apple.screenIsLocked"), SystemEvent::SessionLocked, cb);
}

fn observe(center: &NSNotificationCenter, name: &NSNotificationName, event: SystemEvent, cb: Callback) {
    let block = RcBlock::new(move |_: NonNull<NSNotification>| cb(event));
    // SAFETY : aucun objet filtré, file principale, rappel `Send + Sync`.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(name),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    // L'observateur reste inscrit jusqu'à la fin du processus.
    std::mem::forget(observer);
}
