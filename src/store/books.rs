use super::{Result, Store, StoreError};
use crate::books::model::Book;
use rusqlite::{OptionalExtension, params};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingProgress {
    pub page_index: u32,
    pub node_id: Option<u32>,
    pub updated_at: i64,
}
impl Store {
    pub fn book_ids(&self) -> Result<Vec<i64>> {
        Ok(self
            .connection
            .prepare("SELECT id FROM books ORDER BY imported_at DESC, id DESC")?
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn delete_book(&self, id: i64) -> Result<()> {
        self.connection
            .execute("DELETE FROM books WHERE id=?1", [id])?;
        Ok(())
    }
    /// Duplicate file hashes return the existing book without replacing progress or node IDs.
    pub fn save_book(&self, book: &Book, hash: &str, imported_at: i64) -> Result<i64> {
        let json = serde_json::to_vec(book)?;
        let blob = zstd::stream::encode_all(json.as_slice(), 3)?;
        self.connection.execute("INSERT OR IGNORE INTO books(title,author,format,file_hash,imported_at,model) VALUES(?1,?2,?3,?4,?5,?6)", params![book.document.title, book.document.author, book.document.format, hash, imported_at, blob])?;
        Ok(self
            .connection
            .query_row("SELECT id FROM books WHERE file_hash=?1", [hash], |r| {
                r.get(0)
            })?)
    }
    pub fn book(&self, id: i64) -> Result<Option<Book>> {
        let blob: Option<Vec<u8>> = self
            .connection
            .query_row("SELECT model FROM books WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        blob.map(|blob| {
            use std::io::Read;
            let mut json = Vec::new();
            zstd::stream::read::Decoder::new(blob.as_slice())?
                .take(640 * 1024 * 1024 + 1)
                .read_to_end(&mut json)?;
            if json.len() > 640 * 1024 * 1024 {
                return Err(StoreError::InvalidData(
                    "Book model exceeds size limit".into(),
                ));
            }
            let book: Book = serde_json::from_slice(&json)?;
            if book.version != 1 {
                return Err(StoreError::InvalidData(
                    "Unsupported book model version".into(),
                ));
            }
            Ok(book)
        })
        .transpose()
    }
    /// Cached online covers by book: image bytes (empty after a lookup found nothing) and
    /// when the lookup ran.
    pub fn book_covers(&self) -> Result<Vec<(i64, Vec<u8>, i64)>> {
        Ok(self
            .connection
            .prepare("SELECT book_id,image,checked_at FROM book_covers")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn save_book_cover(&self, id: i64, image: &[u8], checked_at: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO book_covers(book_id,image,checked_at) SELECT ?1,?2,?3 WHERE EXISTS(SELECT 1 FROM books WHERE id=?1) ON CONFLICT(book_id) DO UPDATE SET image=excluded.image,checked_at=excluded.checked_at",
            params![id, image, checked_at],
        )?;
        Ok(())
    }
    pub fn save_reading_progress(&self, id: i64, progress: &ReadingProgress) -> Result<()> {
        let book = self.book(id)?.ok_or(StoreError::NotFound)?;
        if u64::from(progress.page_index) >= book.document.pages.len() as u64
            || progress.node_id.is_some_and(|n| {
                book.nodes
                    .get(n as usize)
                    .is_none_or(|n| n.page as u64 != u64::from(progress.page_index))
            })
        {
            return Err(StoreError::InvalidData(
                "Reading location is outside this book page".into(),
            ));
        }
        self.connection.execute("INSERT INTO book_progress(book_id,page_index,node_id,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(book_id) DO UPDATE SET page_index=excluded.page_index,node_id=excluded.node_id,updated_at=excluded.updated_at WHERE excluded.updated_at>=book_progress.updated_at", params![id, progress.page_index, progress.node_id, progress.updated_at])?;
        Ok(())
    }
    pub fn reading_progress(&self, id: i64) -> Result<Option<ReadingProgress>> {
        Ok(self
            .connection
            .query_row(
                "SELECT page_index,node_id,updated_at FROM book_progress WHERE book_id=?1",
                [id],
                |r| {
                    Ok(ReadingProgress {
                        page_index: r.get(0)?,
                        node_id: r.get(1)?,
                        updated_at: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_and_roundtrip() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        super::super::schema::migrate_until(&mut connection, 5).unwrap();
        connection
            .execute("INSERT INTO settings VALUES('sentinel','kept')", [])
            .unwrap();
        let store = Store::from_connection(connection).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::super::schema::VERSION
        );
        assert_eq!(store.setting("sentinel").unwrap().as_deref(), Some("kept"));
        let book = crate::books::resolve::resolve(crate::books::document::Document {
            cover: None,
            isbn: None,
            title: "Test".into(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![crate::books::document::Page::default()],
            assets: vec![],
        });
        let id = store.save_book(&book, "hash", 1).unwrap();
        assert_eq!(store.book(id).unwrap().unwrap().document.title, "Test");
        let progress = ReadingProgress {
            page_index: 0,
            node_id: None,
            updated_at: 2,
        };
        store.save_reading_progress(id, &progress).unwrap();
        assert_eq!(store.save_book(&book, "hash", 3).unwrap(), id);
        assert_eq!(store.reading_progress(id).unwrap(), Some(progress));
        assert!(
            store
                .save_reading_progress(
                    id,
                    &ReadingProgress {
                        page_index: 1,
                        node_id: None,
                        updated_at: 4
                    }
                )
                .is_err()
        );
    }
}

#[cfg(test)]
mod collection_tests {
    use super::*;
    #[test]
    fn duplicate_preserves_progress_and_delete_removes_location() {
        let store = Store::memory().unwrap();
        let book = crate::books::resolve::resolve(crate::books::document::Document {
            cover: None,
            isbn: None,
            title: "Collection".into(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![crate::books::document::Page::default()],
            assets: vec![],
        });
        let id = store.save_book(&book, "collection", 1).unwrap();
        let progress = ReadingProgress {
            page_index: 0,
            node_id: None,
            updated_at: 3,
        };
        store.save_reading_progress(id, &progress).unwrap();
        assert_eq!(store.save_book(&book, "collection", 4).unwrap(), id);
        assert_eq!(store.book_ids().unwrap(), vec![id]);
        assert_eq!(store.reading_progress(id).unwrap(), Some(progress));
        store.delete_book(id).unwrap();
        assert_eq!(store.book_ids().unwrap(), Vec::<i64>::new());
        assert!(store.reading_progress(id).unwrap().is_none());
    }
}

#[cfg(test)]
mod cover_tests {
    use super::*;
    #[test]
    fn online_covers_upsert_and_go_with_their_book() {
        let store = Store::memory().unwrap();
        let book = crate::books::resolve::resolve(crate::books::document::Document {
            cover: None,
            isbn: None,
            title: "Covers".into(),
            author: String::new(),
            format: "epub".into(),
            pages: vec![crate::books::document::Page::default()],
            assets: vec![],
        });
        let id = store.save_book(&book, "covers", 1).unwrap();
        store.save_book_cover(id, &[], 10).unwrap();
        store.save_book_cover(id, b"jpeg", 20).unwrap();
        store.save_book_cover(id + 1, b"orphan", 20).unwrap();
        assert_eq!(
            store.book_covers().unwrap(),
            vec![(id, b"jpeg".to_vec(), 20)]
        );
        store.delete_book(id).unwrap();
        assert_eq!(store.book_covers().unwrap(), vec![]);
    }
}
