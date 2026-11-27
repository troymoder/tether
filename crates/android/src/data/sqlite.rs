//! Queries run on an in-memory copy of the database, so the same code runs
//! in the page and in tetherd.

use anyhow::{Result, ensure};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, MAIN_DB};

const ROWS: usize = 1000;

pub(super) fn ask(file: Vec<u8>, sql: &str) -> Result<Vec<Vec<String>>> {
    let mut connection = Connection::open_in_memory()?;

    // An empty file is a valid empty database.
    if !file.is_empty() {
        connection.deserialize_read_exact(MAIN_DB, &file[..], file.len(), true)?;
    }

    let mut statement = connection.prepare(sql)?;

    ensure!(statement.readonly(), "read-only: statement would write");

    let columns = statement.column_count();
    let mut rows = vec![statement.column_names().map(str::to_owned).collect()];
    let mut found = statement.query([])?;

    while let Some(found) = found.next()? {
        if rows.len() > ROWS {
            break;
        }

        rows.push(
            (0..columns)
                .map(|column| match found.get_ref(column) {
                    Ok(value) => cell(value),
                    Err(_) => String::new(),
                })
                .collect(),
        );
    }

    Ok(rows)
}

fn cell(value: ValueRef) -> String {
    match value {
        ValueRef::Null => "NULL".to_owned(),
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => number.to_string(),
        ValueRef::Text(text) => String::from_utf8_lossy(text).into_owned(),
        ValueRef::Blob(bytes) => {
            let start: String = bytes
                .iter()
                .take(24)
                .map(|byte| format!("{byte:02x}"))
                .collect();

            format!("x'{start}' ({} bytes)", bytes.len())
        }
    }
}

/// Applies the committed pages of a write-ahead log to a database, as a
/// checkpoint would, and marks it as using a rollback journal, because an
/// in-memory copy cannot have a write-ahead log.
pub(super) fn with_wal(mut file: Vec<u8>, wal: &[u8]) -> Vec<u8> {
    if let Some(committed) = committed(wal) {
        file.resize(committed.pages * committed.size, 0);

        for frame in committed.frames {
            let at = (word(frame, 0) as usize).saturating_sub(1) * committed.size;

            if let Some(page) = file.get_mut(at..at + committed.size) {
                page.copy_from_slice(&frame[24..]);
            }
        }
    }

    // The file format read and write versions at bytes 18 and 19. 1 means a
    // rollback journal, 2 means WAL.
    if let Some(versions) = file.get_mut(18..20) {
        versions.copy_from_slice(&[1, 1]);
    }

    file
}

struct Committed<'a> {
    size: usize,
    /// The number of pages in the database after the commit.
    pages: usize,
    frames: Vec<&'a [u8]>,
}

/// Reads a write-ahead log as https://www.sqlite.org/fileformat.html lays
/// it out: a 32-byte header, then frames of a 24-byte header and a page.
/// None when it is not a log, or has no commit.
fn committed(wal: &[u8]) -> Option<Committed<'_>> {
    if wal.len() < 32 || !matches!(word(wal, 0), 0x377f_0682 | 0x377f_0683) {
        return None;
    }

    // The magic number's low bit gives the byte order of the checksum words.
    let big = word(wal, 0) & 1 == 1;
    let size = word(wal, 8) as usize;
    let salts = &wal[16..24];
    let mut sums = checksum((0, 0), &wal[..24], big);

    if size < 512 || sums != (word(wal, 24), word(wal, 28)) {
        return None;
    }

    let mut frames = Vec::new();
    let mut committed = None;

    // Frames are valid while their salts match and their checksum, which
    // continues from the previous frame's, holds. Only frames up to the last
    // commit count.
    for frame in wal[32..].chunks_exact(24 + size) {
        sums = checksum(checksum(sums, &frame[..8], big), &frame[24..], big);

        if &frame[8..16] != salts || sums != (word(frame, 16), word(frame, 20)) {
            break;
        }

        frames.push(frame);

        if word(frame, 4) != 0 {
            committed = Some((frames.len(), word(frame, 4) as usize));
        }
    }

    let (count, pages) = committed?;

    frames.truncate(count);

    Some(Committed {
        size,
        pages,
        frames,
    })
}

fn checksum(mut sums: (u32, u32), bytes: &[u8], big: bool) -> (u32, u32) {
    for pair in bytes.as_chunks::<8>().0 {
        let read = |at: usize| {
            let four = [pair[at], pair[at + 1], pair[at + 2], pair[at + 3]];

            if big {
                u32::from_be_bytes(four)
            } else {
                u32::from_le_bytes(four)
            }
        };

        sums.0 = sums.0.wrapping_add(read(0)).wrapping_add(sums.1);
        sums.1 = sums.1.wrapping_add(read(4)).wrapping_add(sums.0);
    }

    sums
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_a_database_with_its_log() {
        let folder = std::env::temp_dir().join(format!("android-test-{}", std::process::id()));

        std::fs::create_dir_all(&folder).unwrap();

        let file = folder.join("asked.db");
        let _ = std::fs::remove_file(&file);

        // Kept open as an app keeps it, so the last rows stay in the log.
        let connection = Connection::open(&file).unwrap();

        connection
            .execute_batch(
                "CREATE TABLE notes (id INTEGER, body TEXT, raw BLOB, score REAL);
                 INSERT INTO notes VALUES (1, 'a\tb', x'00ff', 1.5);
                 PRAGMA journal_mode = WAL;
                 PRAGMA wal_autocheckpoint = 0;
                 INSERT INTO notes VALUES (2, NULL, NULL, NULL);
                 CREATE TABLE later (x);",
            )
            .unwrap();

        let copy = std::fs::read(&file).unwrap();
        let wal = std::fs::read(folder.join("asked.db-wal")).unwrap();
        let ask = |sql: &str| ask(with_wal(copy.clone(), &wal), sql);

        assert_eq!(
            ask("SELECT * FROM notes ORDER BY id").unwrap(),
            [
                ["id", "body", "raw", "score"],
                ["1", "a\tb", "x'00ff' (2 bytes)", "1.5"],
                ["2", "NULL", "NULL", "NULL"],
            ]
        );

        assert_eq!(ask("SELECT count(*) FROM later").unwrap()[1], ["0"]);
        assert_eq!(
            super::ask(with_wal(copy.clone(), &[]), "SELECT count(*) FROM notes").unwrap()[1],
            ["1"]
        );

        assert!(ask("DELETE FROM notes").is_err());
        assert!(ask("SELECT * FROM nothing").is_err());
        assert_eq!(super::ask(Vec::new(), "SELECT 1").unwrap()[1], ["1"]);

        drop(connection);

        let _ = std::fs::remove_dir_all(&folder);
    }
}
