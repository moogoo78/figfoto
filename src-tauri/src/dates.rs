//! When a photo was taken, for photos whose EXIF has no date. Dates are guessed from the file
//! name (IMG_20190503_101500.jpg), then from a folder name (2019-05-03 Kyoto/), and only then
//! taken from the file's modified time. Each photo records where its date came from in
//! `photos.date_source`, so guessed dates can be found and set by hand.

use std::path::Path;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime};
use rayon::prelude::*;
use rusqlite::params;

use crate::error::{AppError, Result};
use crate::import::fmt_dt;
use crate::library::Library;
use crate::{exif, storage};

/// Where a photo's date came from.
pub const EXIF: &str = "exif";
pub const FILE_NAME: &str = "filename";
pub const FOLDER: &str = "folder";
pub const FILE_TIME: &str = "file";
pub const MANUAL: &str = "manual";

/// A date found in a name, with the time when the name has one.
pub fn from_name(name: &str) -> Option<(NaiveDate, Option<NaiveTime>)> {
    let b = name.as_bytes();
    (0..b.len()).filter(|&i| b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit())).find_map(|i| date_at(b, i))
}

/// `n` digits at `*i`, moving past them.
fn digits(b: &[u8], i: &mut usize, n: usize) -> Option<u32> {
    let s = b.get(*i..*i + n)?;
    if !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    *i += n;
    Some(s.iter().fold(0, |v, d| v * 10 + (d - b'0') as u32))
}

/// One of `seps` at `*i`, moving past it; `None` when there is none.
fn sep(b: &[u8], i: &mut usize, seps: &[u8]) -> Option<u8> {
    let c = *b.get(*i)?;
    if seps.contains(&c) {
        *i += 1;
        Some(c)
    } else {
        None
    }
}

/// YYYY-MM-DD (or YYYYMMDD, with `-`, `_`, `.` or space) starting at `i`, then maybe a time
/// (HHMMSS, HH-MM-SS, HH.MM.SS, HH:MM:SS) after a separator or `T`.
fn date_at(b: &[u8], start: usize) -> Option<(NaiveDate, Option<NaiveTime>)> {
    let mut i = start;
    let year = digits(b, &mut i, 4)?;
    let s = sep(b, &mut i, b"-_. ");
    let month = digits(b, &mut i, 2)?;
    if s.is_some() && sep(b, &mut i, &[s?]).is_none() {
        return None;
    }
    let day = digits(b, &mut i, 2)?;
    let latest = Local::now().year() as u32 + 1;
    if !(1990..=latest).contains(&year) {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(year as i32, month, day)?;
    let after_date = i;
    let time = (|| {
        sep(b, &mut i, b"-_. T");
        let h = digits(b, &mut i, 2)?;
        let s = sep(b, &mut i, b"-_.: ");
        let m = digits(b, &mut i, 2)?;
        if s.is_some() && sep(b, &mut i, &[s?]).is_none() {
            return None;
        }
        let sec = digits(b, &mut i, 2)?;
        NaiveTime::from_hms_opt(h, m, sec)
    })();
    // A date run straight into more digits (an id, a longer number) is no date, unless a time
    // follows it.
    if time.is_none() && b.get(after_date).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    Some((date, time))
}

/// The date for a photo without one in EXIF: from its file name, else the nearest folder name
/// above it that has one, else its modified time. A date without a time takes the modified
/// time's time of day, which keeps photos of that day in their order.
pub fn guess(name: &str, source: Option<&Path>, modified: NaiveDateTime) -> (NaiveDateTime, &'static str) {
    let with_time = |(date, time): (NaiveDate, Option<NaiveTime>)| date.and_time(time.unwrap_or(modified.time()));
    if let Some(found) = from_name(name) {
        return (with_time(found), FILE_NAME);
    }
    let folders = source.into_iter().flat_map(|p| p.ancestors().skip(1));
    if let Some(found) = folders.filter_map(|d| d.file_name()).find_map(|n| from_name(&n.to_string_lossy())) {
        return (with_time(found), FOLDER);
    }
    (modified, FILE_TIME)
}

/// Sets when photos were taken (by hand) and moves their files to the new date folders. With
/// only a date, each photo keeps its time of day. With a time too, the earliest photo gets it
/// and the others move by as much, which keeps their order. Returns how many photos changed.
pub fn set_taken(lib: &Library, ids: &[i64], date: NaiveDate, time: Option<NaiveTime>) -> Result<usize> {
    let mut photos: Vec<(i64, NaiveDateTime)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, taken_at FROM photos WHERE id IN ({}) ORDER BY taken_at, id",
            ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
        ))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|(id, t)| NaiveDateTime::parse_from_str(&t, "%Y-%m-%dT%H:%M:%S").ok().map(|t| (id, t)))
            .collect()
    };
    let shift = match (time, photos.first()) {
        (Some(time), Some((_, first))) => Some(date.and_time(time) - *first),
        _ => None,
    };
    for (_, taken) in &mut photos {
        *taken = match shift {
            Some(by) => *taken + by,
            None => date.and_time(taken.time()),
        };
    }
    let mut errors = Vec::new();
    for (id, taken) in &photos {
        lib.conn().execute(
            "UPDATE photos SET original_taken_at = COALESCE(original_taken_at, taken_at), taken_at = ?1, date_source = ?2
             WHERE id = ?3",
            params![fmt_dt(*taken), MANUAL, id],
        )?;
        if let Err(e) = storage::move_to_date_folder(lib, *id) {
            errors.push(e.to_string());
        }
    }
    match errors.len() {
        0 => Ok(photos.len()),
        n => Err(AppError::msg(format!("dates set, but {n} file(s) could not be moved: {}", errors.join("; ")))),
    }
}

/// Once per photo added before dates recorded their source: reads its EXIF again, and for one
/// without a date, guesses it from the file and folder names it was added from, moving the file
/// if the date changes. Returns how many dates changed.
pub fn backfill_sources(lib: &Library) -> Result<usize> {
    let rows: Vec<(i64, String, String, Option<String>, String, bool)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT id, rel_path, orig_name, source_path, taken_at, trashed_at IS NOT NULL FROM photos WHERE date_source IS NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    // Reading EXIF is the slow part; do it in parallel, then write one by one.
    let found: Vec<(i64, NaiveDateTime, &'static str, bool)> = rows
        .par_iter()
        .filter_map(|(id, rel, name, source, taken, trashed)| {
            let stored = NaiveDateTime::parse_from_str(taken, "%Y-%m-%dT%H:%M:%S").ok()?;
            if exif::read(&lib.photo_path(rel, *trashed)).taken_at.is_some() {
                return Some((*id, stored, EXIF, false));
            }
            let (date, how) = guess(name, source.as_deref().map(Path::new), stored);
            Some((*id, date, how, date != stored))
        })
        .collect();
    let mut changed = 0;
    for (id, date, how, moved) in found {
        lib.conn().execute(
            "UPDATE photos SET original_taken_at = CASE WHEN taken_at = ?1 THEN original_taken_at
                    ELSE COALESCE(original_taken_at, taken_at) END,
                taken_at = ?1, date_source = ?2
             WHERE id = ?3",
            params![fmt_dt(date), how, id],
        )?;
        if moved {
            changed += 1;
            if let Err(e) = storage::move_to_date_folder(lib, id) {
                eprintln!("moving photo {id} to its date folder failed: {e}");
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn t(h: u32, m: u32, s: u32) -> Option<NaiveTime> {
        NaiveTime::from_hms_opt(h, m, s)
    }

    #[test]
    fn reads_dates_from_names() {
        assert_eq!(from_name("IMG_20190503_101500.jpg"), Some((d(2019, 5, 3), t(10, 15, 0))));
        assert_eq!(from_name("PXL_20190503_101500123.jpg"), Some((d(2019, 5, 3), t(10, 15, 0))));
        assert_eq!(from_name("IMG-20190503-WA0001.jpg"), Some((d(2019, 5, 3), None)));
        assert_eq!(from_name("Screenshot_2019-05-03-10-15-00.png"), Some((d(2019, 5, 3), t(10, 15, 0))));
        assert_eq!(from_name("2019-05-03 10.15.00.jpg"), Some((d(2019, 5, 3), t(10, 15, 0))));
        assert_eq!(from_name("2019-05-03 Kyoto"), Some((d(2019, 5, 3), None)));
        assert_eq!(from_name("20190503"), Some((d(2019, 5, 3), None)));
        assert_eq!(from_name("Kyoto 2019.05.03"), Some((d(2019, 5, 3), None)));
    }

    #[test]
    fn ignores_numbers_that_are_not_dates() {
        assert_eq!(from_name("DSC01234.JPG"), None);
        assert_eq!(from_name("IMG_2680.jpeg"), None);
        assert_eq!(from_name("1556878500000.jpg"), None);
        assert_eq!(from_name("20191345.jpg"), None, "no 13th month");
        assert_eq!(from_name("2019-05_03.jpg"), None, "mixed separators");
        assert_eq!(from_name("123456789012.jpg"), None);
        assert_eq!(from_name("2019"), None);
    }

    #[test]
    fn guesses_from_the_name_then_the_folder_then_the_file_time() {
        let modified = d(2023, 1, 9).and_time(t(8, 30, 0).unwrap());
        assert_eq!(guess("IMG_20190503_101500.jpg", None, modified), (d(2019, 5, 3).and_hms_opt(10, 15, 0).unwrap(), FILE_NAME));
        let src = Path::new("/media/fotos/2019-05-03 Kyoto/day/a.jpg");
        assert_eq!(guess("a.jpg", Some(src), modified), (d(2019, 5, 3).and_hms_opt(8, 30, 0).unwrap(), FOLDER));
        assert_eq!(guess("a.jpg", Some(Path::new("/media/fotos/a.jpg")), modified), (modified, FILE_TIME));
    }
}
