//! Ouverture d'un fichier avec l'application par défaut depuis le Flatpak.
//!
//! Dans le bac à sable, `xdg-open` passe par le portail `OpenURI.OpenURI`, qui transmet
//! le fichier à l'application en lecture seule (document portal) : impossible d'y
//! enregistrer. `OpenURI.OpenFile` avec l'option `writable` l'autorise ; le
//! descripteur doit alors être ouvert en lecture et écriture.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::path::Path;

use zbus::zvariant::{Fd, OwnedObjectPath, Value};
use zbus::{proxy, Connection};

#[proxy(
    interface = "org.freedesktop.portal.OpenURI",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait OpenUri {
    fn open_file(&self, parent_window: &str, fd: Fd<'_>, options: HashMap<&str, Value<'_>>) -> zbus::Result<OwnedObjectPath>;
}

/// Demande au portail d'ouvrir `path`, modifiable par l'application choisie.
pub async fn open_file_writable(path: &Path) -> zbus::Result<()> {
    let fd: OwnedFd = std::fs::OpenOptions::new().read(true).write(true).open(path).map_err(|e| zbus::Error::Failure(e.to_string()))?.into();
    let connection = Connection::session().await?;
    let proxy = OpenUriProxy::new(&connection).await?;
    let options = HashMap::from([("writable", Value::from(true))]);
    proxy.open_file("", Fd::from(&fd), options).await?;
    Ok(())
}
