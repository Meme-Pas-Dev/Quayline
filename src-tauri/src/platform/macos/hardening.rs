//! Durcissement du processus (macOS), appliqué avant toute manipulation de secret.

/// - interdit les core dumps (qui contiendraient la clé du coffre) ;
/// - en version publiée seulement, refuse tout débogueur (`PT_DENY_ATTACH`) : un
///   autre processus ne peut plus s'y attacher pour lire la mémoire. Désactivé en
///   développement, où il empêcherait d'utiliser lldb.
pub fn harden_process() {
    unsafe {
        let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if libc::setrlimit(libc::RLIMIT_CORE, &zero) != 0 {
            eprintln!("{}: cannot disable core dumps", crate::identity::SLUG);
        }
        #[cfg(not(debug_assertions))]
        if libc::ptrace(libc::PT_DENY_ATTACH, 0, std::ptr::null_mut(), 0) != 0 {
            eprintln!("{}: cannot enable PT_DENY_ATTACH", crate::identity::SLUG);
        }
    }
}
