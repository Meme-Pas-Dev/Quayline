//! Durcissement du processus (Linux), appliqué avant toute manipulation de secret.

/// - interdit les core dumps (qui contiendraient la clé du coffre) ;
/// - rend le processus non « dumpable » : un autre processus du même
///   utilisateur ne peut plus s'y attacher avec ptrace ni lire /proc/<pid>/mem.
pub fn harden_process() {
    #[cfg(target_os = "linux")]
    unsafe {
        let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if libc::setrlimit(libc::RLIMIT_CORE, &zero) != 0 {
            eprintln!("{}: cannot disable core dumps", crate::identity::SLUG);
        }
        if libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) != 0 {
            eprintln!("{}: cannot clear PR_SET_DUMPABLE", crate::identity::SLUG);
        }
    }
}
