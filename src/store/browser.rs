//! Browser persistence helpers: the database lives in SQLite's memory VFS and the web shell
//! saves a serialized image of it. The schema and every query stay the same as on desktop.
use super::{Result, Store};
use rusqlite::{Connection, MAIN_DB};

impl Store {
    /// Restores a previously saved database image, or starts empty, then migrates as usual.
    pub fn from_image(image: Option<&[u8]>) -> Result<Self> {
        let mut connection = Connection::open_in_memory()?;
        if let Some(image) = image.filter(|image| !image.is_empty()) {
            connection.deserialize_read_exact(MAIN_DB, image, image.len(), false)?;
        }
        Self::from_connection(connection)
    }
    /// A complete, self-contained copy of the database for saving in the browser.
    pub fn image(&self) -> Result<Vec<u8>> {
        Ok(self.connection.serialize(MAIN_DB)?.to_vec())
    }
    /// Grows on every committed row change; the shell saves when it differs from the last save.
    pub fn revision(&self) -> u64 {
        self.connection.total_changes()
    }
}
