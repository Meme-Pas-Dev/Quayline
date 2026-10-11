//! Durcissement du processus (Windows), appliqué avant toute manipulation de secret.

use windows_sys::Win32::System::Diagnostics::Debug::{SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX};
use windows_sys::Win32::System::ErrorReporting::WerAddExcludedApplication;

use super::wide;

/// - pas de rapport d'erreurs Windows (WER) pour l'application : un plantage ne produit
///   pas de vidage mémoire, qui contiendrait la clé du coffre ;
/// - pas de fenêtre « a cessé de fonctionner », qui retiendrait le processus en mémoire.
///
/// Un autre processus du même utilisateur peut toujours lire la mémoire (le propriétaire
/// peut modifier les droits du processus) : comme sous Linux, la protection vise les
/// vidages et les outils ordinaires, pas un logiciel malveillant déjà installé.
pub fn harden_process() {
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    }
    let Some(exe) = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
    else {
        return;
    };
    // Exclusion propre à l'utilisateur (ballusers = 0) : aucun droit administrateur requis.
    if unsafe { WerAddExcludedApplication(wide(&exe).as_ptr(), 0) } < 0 {
        eprintln!("{}: cannot exclude the application from Windows Error Reporting", crate::identity::SLUG);
    }
}
