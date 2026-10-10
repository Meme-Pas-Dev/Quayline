use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Chaîne sensible (mot de passe, clé privée, phrase de passe).
///
/// - effacée de la mémoire à la destruction ;
/// - jamais affichée par `Debug` (donc jamais dans les logs) ;
/// - accessible uniquement via [`Secret::expose`], ce qui rend chaque usage explicite.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_leaks() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(format!("{:?}", Some(&s)), "Some(Secret(***))");
    }
}
