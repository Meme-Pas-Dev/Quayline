//! Systèmes non encore pris en charge : implémentations neutres, pour que
//! l'application compile. À remplacer par un module dédié (macos, windows).

pub mod hardening {
    pub fn harden_process() {}
}

pub mod keyring {
    use zeroize::Zeroizing;

    use crate::platform::SecretRef;


    pub async fn store(_label: &str, _r: &SecretRef<'_>, _secret: &[u8]) -> Result<(), String> {
        Err(crate::i18n::tr!("err.keyring.unsupported"))
    }

    pub async fn load(_label: &str, _r: &SecretRef<'_>) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        Err(crate::i18n::tr!("err.keyring.unsupported"))
    }

    pub async fn delete(_r: &SecretRef<'_>) {}
}

pub mod session {
    use crate::platform::SystemEvent;

    pub fn watch(_on_event: impl Fn(SystemEvent) + Send + Sync + 'static) {}
}
