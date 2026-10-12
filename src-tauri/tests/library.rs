use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::{Local, NaiveDate, TimeZone};
use exif::experimental::Writer;
use exif::{Field, In, Tag, Value};
use frutyfoto_lib::import::{self, hash_file, ImportOptions, ImportProgress};
use frutyfoto_lib::library::Library;
use frutyfoto_lib::queries::{self, Filter, Sort};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

// ---- fixtures ----

/// A deterministic test picture; `variant` changes the structure, not just colors.
fn picture(variant: u32, w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let v = match variant {
            0 => {
                let d = ((fx - 0.3).powi(2) + (fy - 0.35).powi(2)).sqrt();
                if d < 0.2 { 20.0 } else { 60.0 + 180.0 * fx }
            }
            1 => if ((fx * 6.0) as u32) % 2 == 0 { 30.0 } else { 220.0 },
            _ => 255.0 * (1.0 - fy) * if fx > 0.5 { 1.0 } else { 0.4 },
        } as u8;
        Rgb([v, v.wrapping_add(variant as u8 * 40), 255 - v])
    })
}

fn jpeg_bytes(img: &RgbImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(img).unwrap();
    out
}

/// Inserts an EXIF APP1 segment (DateTimeOriginal + Model) right after the JPEG SOI marker.
fn with_exif(jpeg: Vec<u8>, taken: &str, model: &str) -> Vec<u8> {
    let dto = Field { tag: Tag::DateTimeOriginal, ifd_num: In::PRIMARY, value: Value::Ascii(vec![taken.into()]) };
    let cam = Field { tag: Tag::Model, ifd_num: In::PRIMARY, value: Value::Ascii(vec![model.into()]) };
    let mut writer = Writer::new();
    writer.push_field(&dto);
    writer.push_field(&cam);
    let mut tiff = Cursor::new(Vec::new());
    writer.write(&mut tiff, false).unwrap();
    let tiff = tiff.into_inner();

    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&tiff);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn import_all(lib: &Library, src: &Path) -> ImportProgress {
    import::run(lib, src, &ImportOptions::default(), &AtomicBool::new(false), |_| {}).unwrap()
}

struct Env {
    _tmp: tempfile::TempDir,
    src: PathBuf,
    lib: Library,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    let lib = Library::open(&tmp.path().join("lib"), true).unwrap();
    Env { _tmp: tmp, src, lib }
}

/// Empties the trash without touching the real system Trash.
fn empty_trash(lib: &Library) -> frutyfoto_lib::error::Result<usize> {
    queries::empty_trash_with(lib, |p| Ok(std::fs::remove_file(p)?))
}

// ---- tests ----

#[test]
fn import_organises_by_date_and_skips_duplicates() {
    let Env { _tmp, src, lib } = env();

    let a = with_exif(jpeg_bytes(&picture(0, 320, 240), 90), "2021:07:15 10:20:30", "TestCam X1");
    write(&src.join("a.jpg"), &a);
    write(&src.join("copies/a.jpg"), &a); // identical bytes -> duplicate
    let other = with_exif(jpeg_bytes(&picture(1, 320, 240), 90), "2021:07:15 18:00:00", "TestCam X1");
    write(&src.join("other/a.jpg"), &other); // same name, same day -> a-1.jpg
    write(&src.join("notes.txt"), b"not a photo");

    // A PNG without EXIF falls back to the file's mtime.
    let png = src.join("scans/b.png");
    std::fs::create_dir_all(png.parent().unwrap()).unwrap();
    DynamicImage::ImageRgb8(picture(2, 200, 300)).save_with_format(&png, ImageFormat::Png).unwrap();
    let mtime = Local.from_local_datetime(&NaiveDate::from_ymd_opt(2019, 3, 4).unwrap().and_hms_opt(12, 0, 0).unwrap()).unwrap();
    std::fs::File::options().write(true).open(&png).unwrap().set_modified(mtime.into()).unwrap();

    let source_hash = hash_file(&src.join("a.jpg")).unwrap();

    let p = import_all(&lib, &src);
    assert_eq!((p.total, p.added, p.skipped, p.failed), (4, 3, 1, 0), "errors: {:?}", p.errors);

    let day = lib.root.join("originals/2021/07/15");
    assert!(day.join("a.jpg").is_file());
    assert!(day.join("a-1.jpg").is_file());
    assert!(lib.root.join("originals/2019/03/04/b.png").is_file());

    // Sources are untouched.
    assert_eq!(hash_file(&src.join("a.jpg")).unwrap(), source_hash);
    assert!(src.join("copies/a.jpg").is_file());

    // Metadata and thumbnails.
    let photos = queries::list_photos(&lib, &Filter::default(), Sort::TakenDesc, 0, 100).unwrap();
    assert_eq!(photos.len(), 3);
    let first = &photos[0];
    assert_eq!(first.taken_at, "2021-07-15T18:00:00");
    assert_eq!(first.camera_model.as_deref(), Some("TestCam X1"));
    assert!(Path::new(&first.thumb).is_file());
    let png_row = photos.iter().find(|p| p.orig_name == "b.png").unwrap();
    assert_eq!((png_row.width, png_row.height), (Some(200), Some(300)));

    let buckets = queries::date_buckets(&lib, &Filter::default(), Sort::TakenDesc).unwrap();
    let days: Vec<_> = buckets.iter().map(|b| (b.day.as_str(), b.count)).collect();
    assert_eq!(days, vec![("2021-07-15", 2), ("2019-03-04", 1)]);

    // Re-importing the same folder adds nothing.
    let again = import_all(&lib, &src);
    assert_eq!((again.added, again.skipped, again.failed), (0, 4, 0));
}

#[test]
fn refuses_to_import_the_library_itself() {
    let Env { _tmp, lib, .. } = env();
    let err = import::run(&lib, &lib.root.join("originals"), &ImportOptions::default(), &AtomicBool::new(false), |_| {});
    assert!(err.is_err());
}

#[test]
fn near_duplicates_are_grouped() {
    let Env { _tmp, src, lib } = env();
    let base = picture(0, 800, 600);
    write(&src.join("orig.jpg"), &jpeg_bytes(&base, 92));
    let small = DynamicImage::ImageRgb8(base).resize_exact(400, 300, image::imageops::FilterType::Triangle);
    write(&src.join("small.jpg"), &jpeg_bytes(&small.to_rgb8(), 60));
    write(&src.join("different.jpg"), &jpeg_bytes(&picture(1, 800, 600), 92));
    write(&src.join("different2.jpg"), &jpeg_bytes(&picture(2, 800, 600), 92));
    assert_eq!(import_all(&lib, &src).added, 4);

    let groups = queries::find_duplicates(&lib, 6).unwrap();
    assert_eq!(groups.len(), 1, "groups: {groups:?}");
    let mut names: Vec<_> = groups[0].iter().map(|p| p.orig_name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["orig.jpg", "small.jpg"]);
    // Largest image first.
    assert_eq!(groups[0][0].orig_name, "orig.jpg");
}

#[test]
fn tags_albums_ratings_and_filters() {
    let Env { _tmp, src, lib } = env();
    for (i, day) in ["2020:01:01", "2020:06:15", "2021:12:31"].iter().enumerate() {
        let bytes = with_exif(jpeg_bytes(&picture(i as u32, 64, 64), 90), &format!("{day} 08:00:00"), "Cam");
        write(&src.join(format!("p{i}.jpg")), &bytes);
    }
    import_all(&lib, &src);
    let all = queries::list_photos(&lib, &Filter::default(), Sort::TakenAsc, 0, 10).unwrap();
    let ids: Vec<i64> = all.iter().map(|p| p.id).collect();

    queries::set_rating(&lib, &ids[1..], 4).unwrap();
    queries::set_favorite(&lib, &[ids[0]], true).unwrap();
    queries::add_tags(&lib, &ids[..2], &["beach".into(), " family ".into()]).unwrap();
    queries::add_tags(&lib, &[ids[2]], &["Beach".into()]).unwrap(); // case-insensitive reuse

    let tags = queries::list_tags(&lib).unwrap();
    let names: Vec<_> = tags.iter().map(|t| (t.name.as_str(), t.count)).collect();
    assert_eq!(names, vec![("beach", 3), ("family", 2)]);
    let beach = tags[0].id;
    let family = tags[1].id;

    let find = |f: Filter| -> Vec<i64> {
        queries::list_photos(&lib, &f, Sort::TakenAsc, 0, 10).unwrap().iter().map(|p| p.id).collect()
    };
    assert_eq!(find(Filter { tag_ids: vec![beach, family], ..Default::default() }), ids[..2]);
    assert_eq!(find(Filter { min_rating: Some(4), ..Default::default() }), ids[1..]);
    assert_eq!(find(Filter { favorite: Some(true), ..Default::default() }), ids[..1]);
    assert_eq!(
        find(Filter { date_from: Some("2020-06-15".into()), date_to: Some("2020-06-15".into()), ..Default::default() }),
        ids[1..2]
    );
    assert_eq!(find(Filter { text: Some("p2".into()), ..Default::default() }), ids[2..]);
    assert_eq!(find(Filter { camera: Some("Cam".into()), ..Default::default() }).len(), 3);

    let album = queries::create_album(&lib, "Trip").unwrap();
    queries::add_to_album(&lib, album, &[ids[2], ids[0]]).unwrap();
    queries::add_to_album(&lib, album, &[ids[0]]).unwrap(); // no double insert
    assert_eq!(find(Filter { album_id: Some(album), ..Default::default() }), vec![ids[0], ids[2]]);
    let albums = queries::list_albums(&lib).unwrap();
    assert_eq!((albums[0].count, albums[0].cover.is_some()), (2, true));

    let detail = queries::get_photo(&lib, ids[0]).unwrap();
    assert_eq!(detail.tags.len(), 2);
    assert_eq!(detail.albums.len(), 1);

    queries::remove_tag(&lib, &ids, family).unwrap();
    assert_eq!(queries::list_tags(&lib).unwrap().len(), 1, "unused tag is dropped");
    queries::remove_from_album(&lib, album, &[ids[2]]).unwrap();
    assert_eq!(find(Filter { album_id: Some(album), ..Default::default() }), ids[..1]);
    assert!(queries::set_rating(&lib, &ids, 6).is_err());
}

#[test]
fn trash_restore_and_empty() {
    let Env { _tmp, src, lib } = env();
    write(&src.join("x.jpg"), &jpeg_bytes(&picture(0, 64, 64), 90));
    write(&src.join("y.jpg"), &jpeg_bytes(&picture(1, 64, 64), 90));
    import_all(&lib, &src);
    let photos = queries::list_photos(&lib, &Filter::default(), Sort::TakenDesc, 0, 10).unwrap();
    let x = photos.iter().find(|p| p.orig_name == "x.jpg").unwrap().clone();

    queries::trash_photos(&lib, &[x.id]).unwrap();
    assert!(!Path::new(&x.path).exists());
    let trashed = queries::list_photos(&lib, &Filter { trashed: true, ..Default::default() }, Sort::TakenDesc, 0, 10).unwrap();
    assert_eq!(trashed.len(), 1);
    assert!(Path::new(&trashed[0].path).is_file());
    assert_eq!(queries::counts(&lib).unwrap().trash, 1);

    queries::restore_photos(&lib, &[x.id]).unwrap();
    assert!(Path::new(&x.path).is_file());

    queries::trash_photos(&lib, &[x.id]).unwrap();
    assert_eq!(empty_trash(&lib).unwrap(), 1);
    assert!(!Path::new(&x.thumb).exists());
    let c = queries::counts(&lib).unwrap();
    assert_eq!((c.all, c.trash), (1, 0));

    // A deleted photo isn't imported again by default (see deleted_photos_are_not_imported_again).
    assert_eq!(import_all(&lib, &src).skipped_deleted, 1);
}

#[test]
fn reopening_requires_existing_library() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(Library::open(tmp.path(), false).is_err());
    Library::open(tmp.path(), true).unwrap();
    Library::open(tmp.path(), false).unwrap();
}

#[test]
fn opens_library_with_legacy_meta_dir() {
    let tmp = tempfile::tempdir().unwrap();
    Library::open(tmp.path(), true).unwrap();
    std::fs::rename(tmp.path().join(".frutyfoto"), tmp.path().join(".fotolake")).unwrap();
    Library::open(tmp.path(), false).unwrap();
    assert!(tmp.path().join(".frutyfoto/library.db").exists());
    assert!(!tmp.path().join(".fotolake").exists());
}

#[test]
fn imports_heic_with_exif_thumbnail_and_preview() {
    let Env { _tmp, src, lib } = env();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.heic");
    std::fs::copy(&fixture, src.join("IMG_0001.HEIC")).unwrap();
    // Same picture as a JPEG must not affect the HEIC row.
    write(&src.join("other.jpg"), &jpeg_bytes(&picture(1, 64, 64), 90));

    let p = import_all(&lib, &src);
    assert_eq!((p.added, p.failed), (2, 0), "errors: {:?}", p.errors);

    let photos = queries::list_photos(&lib, &Filter::default(), Sort::TakenDesc, 0, 10).unwrap();
    let heic = photos.iter().find(|p| p.orig_name == "IMG_0001.HEIC").unwrap();
    assert_eq!(heic.taken_at, "2022-05-06T07:08:09", "EXIF date read from HEIC");
    assert_eq!(heic.camera_model.as_deref(), Some("HeicCam 9"));
    assert_eq!((heic.width, heic.height), (Some(320), Some(240)));
    assert_eq!(heic.mime.as_deref(), Some("image/heic"));
    assert!(lib.root.join("originals/2022/05/06/IMG_0001.HEIC").is_file());

    // Viewer gets a decodable JPEG preview; thumbnail exists too.
    assert!(heic.display.ends_with(".jpg") && heic.display != heic.path);
    let preview = image::open(&heic.display).unwrap();
    assert_eq!((preview.width(), preview.height()), (320, 240));
    assert!(Path::new(&heic.thumb).is_file());

    // Non-HEIC photos are displayed from the original file.
    let jpg = photos.iter().find(|p| p.orig_name == "other.jpg").unwrap();
    assert_eq!(jpg.display, jpg.path);

    // Emptying trash removes the preview as well.
    queries::trash_photos(&lib, &[heic.id]).unwrap();
    empty_trash(&lib).unwrap();
    assert!(!Path::new(&heic.display).exists());
}

#[test]
fn tiff_gets_preview_and_backfill_restores_missing_ones() {
    let Env { _tmp, src, lib } = env();
    let tif = src.join("scan.tif");
    DynamicImage::ImageRgb8(picture(2, 300, 200)).save_with_format(&tif, ImageFormat::Tiff).unwrap();
    write(&src.join("plain.png"), &{
        let mut v = Vec::new();
        DynamicImage::ImageRgb8(picture(0, 64, 64)).write_to(&mut Cursor::new(&mut v), ImageFormat::Png).unwrap();
        v
    });
    assert_eq!(import_all(&lib, &src).added, 2);

    let photos = queries::list_photos(&lib, &Filter::default(), Sort::TakenDesc, 0, 10).unwrap();
    let t = photos.iter().find(|p| p.orig_name == "scan.tif").unwrap();
    assert_eq!(t.mime.as_deref(), Some("image/tiff"));
    assert!(t.display.ends_with(".jpg") && t.display != t.path);
    let preview = image::open(&t.display).unwrap();
    assert_eq!((preview.width(), preview.height()), (300, 200));

    // A library imported before TIFF previews existed has none; backfill makes them.
    std::fs::remove_file(&t.display).unwrap();
    assert_eq!(import::backfill_previews(&lib).unwrap(), 1);
    assert!(Path::new(&t.display).is_file());
    assert_eq!(import::backfill_previews(&lib).unwrap(), 0, "nothing left to do");
}

#[test]
fn import_history_records_each_run() {
    let Env { _tmp, src, lib } = env();
    let (first, second) = (src.join("first"), src.join("second"));
    write(&first.join("a.jpg"), &jpeg_bytes(&picture(0, 64, 64), 90));
    write(&first.join("b.jpg"), &jpeg_bytes(&picture(1, 64, 64), 90));
    write(&first.join("broken.jpg"), b"not really a jpeg");
    write(&second.join("c.jpg"), &jpeg_bytes(&picture(2, 64, 64), 90));
    write(&second.join("a-again.jpg"), &jpeg_bytes(&picture(0, 64, 64), 90)); // dup of a.jpg

    import_all(&lib, &first);
    import_all(&lib, &second);

    let history = queries::list_imports(&lib).unwrap();
    assert_eq!(history.len(), 2);
    let (newest, oldest) = (&history[0], &history[1]);
    assert_eq!(newest.source_dir, second.to_string_lossy());
    assert_eq!((newest.added, newest.skipped_dupes, newest.failed, newest.photo_count), (1, 1, 0, 1));
    assert_eq!((oldest.added, oldest.skipped_dupes, oldest.failed, oldest.photo_count), (2, 0, 1, 2));
    assert!(oldest.finished_at.is_some() && !oldest.cancelled);
    assert!(oldest.cover.is_some());

    // Failed files are kept with the import.
    let errors = queries::import_errors(&lib, oldest.id).unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].path.ends_with("broken.jpg"));

    // Photos can be filtered by the import that added them.
    let names = |id| -> Vec<String> {
        let f = Filter { import_id: Some(id), ..Default::default() };
        let mut v: Vec<_> = queries::list_photos(&lib, &f, Sort::TakenAsc, 0, 10).unwrap().into_iter().map(|p| p.orig_name).collect();
        v.sort();
        v
    };
    assert_eq!(names(oldest.id), vec!["a.jpg", "b.jpg"]);
    assert_eq!(names(newest.id), vec!["c.jpg"]);

    // Trashed photos no longer count toward an import.
    let c = queries::list_photos(&lib, &Filter { import_id: Some(newest.id), ..Default::default() }, Sort::TakenAsc, 0, 1).unwrap();
    queries::trash_photos(&lib, &[c[0].id]).unwrap();
    assert_eq!(queries::list_imports(&lib).unwrap()[0].photo_count, 0);
}

#[test]
fn migration_links_existing_photos_to_their_import() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".frutyfoto")).unwrap();
    {
        // A library created before import history existed (schema v1).
        let conn = rusqlite::Connection::open(tmp.path().join(".frutyfoto/library.db")).unwrap();
        conn.execute_batch(frutyfoto_lib::db::MIGRATIONS[0]).unwrap();
        conn.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO imports (id, source_dir, started_at, finished_at, added)
               VALUES (1, '/a', '2026-01-01T10:00:00', '2026-01-01T10:05:00', 1),
                      (2, '/b', '2026-02-01T09:00:00', '2026-02-01T09:00:30', 1);
             INSERT INTO photos (hash, rel_path, orig_name, taken_at, imported_at, file_size)
               VALUES ('h1', 'originals/x.jpg', 'x.jpg', '2020-01-01T00:00:00', '2026-01-01T10:02:00', 1),
                      ('h2', 'originals/y.jpg', 'y.jpg', '2020-01-01T00:00:00', '2026-02-01T09:00:30', 1);",
        )
        .unwrap();
    }
    let lib = Library::open(tmp.path(), false).unwrap();
    let import_of = |hash: &str| -> Option<i64> {
        lib.conn().query_row("SELECT import_id FROM photos WHERE hash = ?1", [hash], |r| r.get(0)).unwrap()
    };
    assert_eq!(import_of("h1"), Some(1));
    assert_eq!(import_of("h2"), Some(2));
}

// ---- albums, marks, sources ----

fn rel_of(lib: &Library, id: i64) -> String {
    queries::get_photo(lib, id).unwrap().photo.rel_path
}

/// Imports three photos taken on 2021-07-15 and returns their ids (oldest first).
fn three_photos(lib: &Library, src: &Path) -> Vec<i64> {
    for i in 0..3 {
        let bytes = with_exif(jpeg_bytes(&picture(i, 64, 64), 90), &format!("2021:07:15 0{i}:00:00"), "Cam");
        write(&src.join(format!("p{i}.jpg")), &bytes);
    }
    import_all(lib, src);
    queries::list_photos(lib, &Filter::default(), Sort::TakenAsc, 0, 10).unwrap().iter().map(|p| p.id).collect()
}

#[test]
fn albums_never_move_files_and_share_photos() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    let trip = queries::create_album(&lib, "Trip: Kyoto/Osaka").unwrap();
    let best = queries::create_album(&lib, "Best").unwrap();

    // One photo in two albums; files stay in their date folder throughout.
    queries::add_to_album(&lib, trip, &ids[..2]).unwrap();
    queries::add_to_album(&lib, best, &ids[..1]).unwrap();
    assert_eq!(queries::get_photo(&lib, ids[0]).unwrap().albums.len(), 2);
    queries::rename_album(&lib, trip, "Japan").unwrap();
    queries::remove_from_album(&lib, trip, &[ids[0]]).unwrap();
    queries::delete_album(&lib, best).unwrap();
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(rel_of(&lib, *id), format!("originals/2021/07/15/p{i}.jpg"));
    }
    assert!(!lib.root.join("albums").exists());
    assert_eq!(queries::get_photo(&lib, ids[1]).unwrap().albums.len(), 1);
}

#[test]
fn photos_in_old_album_folders_move_to_date_folders() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    // As the album-folder layout left them: one live photo and one trashed one under albums/.
    for (id, base, name) in [(ids[0], lib.root.clone(), "p0.jpg"), (ids[1], lib.trash_dir(), "p1.jpg")] {
        let to = base.join("albums/Old").join(name);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        if base == lib.trash_dir() {
            queries::trash_photos(&lib, &[id]).unwrap();
        }
        let from = queries::get_photo(&lib, id).unwrap().photo.path;
        std::fs::rename(from, &to).unwrap();
        lib.conn().execute("UPDATE photos SET rel_path = ?1 WHERE id = ?2", rusqlite::params![format!("albums/Old/{name}"), id]).unwrap();
    }
    // A same-named file already in the date folder gets a unique name instead of being overwritten.
    write(&lib.root.join("originals/2021/07/15/p0.jpg"), b"other");

    assert_eq!(frutyfoto_lib::storage::move_out_of_album_folders(&lib).unwrap(), 2);
    assert_eq!(rel_of(&lib, ids[0]), "originals/2021/07/15/p0-1.jpg");
    assert_eq!(rel_of(&lib, ids[1]), "originals/2021/07/15/p1.jpg");
    assert!(lib.trash_dir().join("originals/2021/07/15/p1.jpg").is_file());
    assert!(!lib.root.join("albums").exists() && !lib.trash_dir().join("albums").exists());
    assert_eq!(frutyfoto_lib::storage::move_out_of_album_folders(&lib).unwrap(), 0);
}

#[test]
fn marks_are_set_filtered_and_run() {
    use frutyfoto_lib::marks::{self, ActionKind};
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    let find = |f: Filter| -> Vec<i64> {
        queries::list_photos(&lib, &f, Sort::TakenAsc, 0, 10).unwrap().iter().map(|p| p.id).collect()
    };

    marks::set_mark(&lib, &[ids[0]], Some("x")).unwrap();
    marks::set_mark(&lib, &ids[1..], Some("1")).unwrap();
    assert!(marks::set_mark(&lib, &ids, Some("9")).is_err());
    assert_eq!(find(Filter { mark: Some("1".into()), ..Default::default() }), ids[1..]);
    assert_eq!(find(Filter { mark: Some("any".into()), ..Default::default() }).len(), 3);
    assert_eq!(queries::counts(&lib).unwrap().marked, 3);

    // Unconfigured custom marks can't run.
    assert!(marks::run(&lib, "1", |_, _| {}).is_err());

    // x -> library trash.
    let r = marks::run(&lib, "x", |_, _| {}).unwrap();
    assert_eq!(r.done, 1);
    assert_eq!(queries::counts(&lib).unwrap().trash, 1);

    // 1 -> copy to another disk, keeping the date folders; marks are cleared after.
    let out = _tmp.path().join("usb");
    std::fs::create_dir_all(&out).unwrap();
    marks::set_action(&lib, 1, "Backup", Some(ActionKind::Copy), &out.to_string_lossy()).unwrap();
    let slots = marks::list_slots(&lib).unwrap();
    assert_eq!((slots[1].label.as_str(), slots[1].count), ("Backup", 2));
    let r = marks::run(&lib, "1", |_, _| {}).unwrap();
    assert_eq!((r.done, r.skipped, r.errors.len()), (2, 0, 0));
    assert!(out.join("2021/07/15/p1.jpg").is_file() && out.join("2021/07/15/p2.jpg").is_file());
    assert_eq!(queries::counts(&lib).unwrap().marked, 0);
    // Running again with identical copies present skips them.
    marks::set_mark(&lib, &ids[1..2], Some("1")).unwrap();
    let r = marks::run(&lib, "1", |_, _| {}).unwrap();
    assert_eq!((r.done, r.skipped), (0, 1));
    assert!(!out.join("2021/07/15/p1-1.jpg").exists());

    // 2 -> album, 3 -> tag.
    let album = queries::create_album(&lib, "Picks").unwrap();
    marks::set_action(&lib, 2, "", Some(ActionKind::Album), &album.to_string()).unwrap();
    marks::set_action(&lib, 3, "", Some(ActionKind::Tag), "keeper").unwrap();
    marks::set_mark(&lib, &ids[1..2], Some("2")).unwrap();
    marks::set_mark(&lib, &ids[2..], Some("3")).unwrap();
    marks::run(&lib, "2", |_, _| {}).unwrap();
    marks::run(&lib, "3", |_, _| {}).unwrap();
    assert_eq!(queries::get_photo(&lib, ids[1]).unwrap().albums[0].name, "Picks");
    assert_eq!(queries::get_photo(&lib, ids[2]).unwrap().tags[0].name, "keeper");

    // Removing an action setup.
    marks::set_action(&lib, 3, "", None, "").unwrap();
    assert!(marks::list_slots(&lib).unwrap()[3].kind.is_none());
    assert!(marks::set_action(&lib, 1, "", Some(ActionKind::Trash), "").is_err());
}

#[test]
fn sources_and_device_kind() {
    use frutyfoto_lib::db::device_kind;
    assert_eq!(device_kind(Some("Apple"), Some("iPhone 15 Pro")), "phone");
    assert_eq!(device_kind(Some("samsung"), Some("SM-S918B")), "phone");
    assert_eq!(device_kind(Some("SONY"), Some("ILCE-7M4")), "camera");
    assert_eq!(device_kind(Some("Sony"), Some("Xperia 1 V")), "phone");
    assert_eq!(device_kind(Some("FUJIFILM"), Some("X-T5")), "camera");
    assert_eq!(device_kind(None, None), "unknown");

    let Env { _tmp, src, lib } = env();
    write(&src.join("cam/a.jpg"), &with_exif(jpeg_bytes(&picture(0, 64, 64), 90), "2021:01:01 00:00:00", "ILCE-7M4"));
    write(&src.join("cam/b.jpg"), &with_exif(jpeg_bytes(&picture(1, 64, 64), 90), "2021:01:01 00:00:00", "iPhone 15"));
    import_all(&lib, &src.join("cam"));
    // Photos saved from a chat app have no EXIF; label the whole import.
    write(&src.join("line/c.jpg"), &jpeg_bytes(&picture(2, 64, 64), 90));
    import::run(&lib, &src.join("line"), &ImportOptions { source: Some(" LINE "), ..Default::default() }, &AtomicBool::new(false), |_| {}).unwrap();

    let names = |origin: &str| -> Vec<String> {
        let f = Filter { origin: Some(origin.into()), ..Default::default() };
        queries::list_photos(&lib, &f, Sort::TakenAsc, 0, 10).unwrap().into_iter().map(|p| p.orig_name).collect()
    };
    assert_eq!(names("camera"), vec!["a.jpg"]);
    assert_eq!(names("phone"), vec!["b.jpg"]);
    assert_eq!(names("LINE"), vec!["c.jpg"]);
    assert_eq!(frutyfoto_lib::marks::list_sources(&lib).unwrap(), vec!["LINE"]);

    let a = queries::list_photos(&lib, &Filter { origin: Some("camera".into()), ..Default::default() }, Sort::TakenAsc, 0, 1)
        .unwrap()[0]
        .clone();
    assert_eq!((a.device, a.source.as_deref()), ("camera", None));
    frutyfoto_lib::marks::set_source(&lib, &[a.id], "Facebook").unwrap();
    assert_eq!(names("Facebook"), vec!["a.jpg"]);
    frutyfoto_lib::marks::set_source(&lib, &[a.id], "  ").unwrap();
    assert_eq!(names("camera"), vec!["a.jpg"]);
}

#[test]
fn lens_can_be_set_by_hand() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    // Stand-in for a lens the camera did record.
    lib.conn().execute("UPDATE photos SET lens = 'M.12-40mm' WHERE id = ?1", [ids[2]]).unwrap();
    let lens = |id: i64| {
        let p = queries::get_photo(&lib, id).unwrap().photo;
        (p.lens, p.lens_set)
    };
    assert_eq!(lens(ids[0]), (None, false));
    assert!(queries::get_photo(&lib, ids[2]).unwrap().photo.exif_lens);

    assert_eq!(queries::set_lens(&lib, &ids, "  7Artisans 25mm F1.8 ").unwrap(), 2);
    assert_eq!(lens(ids[0]), (Some("7Artisans 25mm F1.8".into()), true));
    assert_eq!(lens(ids[2]), (Some("M.12-40mm".into()), false), "a lens the camera recorded is kept");
    let names = || queries::list_lenses(&lib).unwrap().into_iter().map(|l| (l.name, l.photos)).collect::<Vec<_>>();
    assert_eq!(names(), vec![("7Artisans 25mm F1.8".into(), 2)]);

    // Clearing goes back to what EXIF says.
    queries::set_lens(&lib, &ids, "").unwrap();
    assert_eq!(lens(ids[0]), (None, false));
    assert_eq!(lens(ids[2]), (Some("M.12-40mm".into()), false));
    // The lens stays on the saved list, and EXIF lenses never join it.
    assert_eq!(names(), vec![("7Artisans 25mm F1.8".into(), 0)]);

    // A lens can be saved before any photo uses it, and renaming moves its photos along.
    queries::add_lens(&lib, " Helios 44-2 ").unwrap();
    queries::set_lens(&lib, &ids[..1], "helios 44-2").unwrap();
    assert_eq!(names(), vec![("7Artisans 25mm F1.8".into(), 0), ("Helios 44-2".into(), 1)], "same name, any case");
    queries::rename_lens(&lib, "Helios 44-2", "Helios 44-2 58mm f/2").unwrap();
    assert_eq!(lens(ids[0]), (Some("Helios 44-2 58mm f/2".into()), true));

    // Removing from the list leaves the photos alone.
    queries::remove_lens(&lib, "Helios 44-2 58mm f/2").unwrap();
    queries::remove_lens(&lib, "7Artisans 25mm F1.8").unwrap();
    assert!(names().is_empty());
    assert_eq!(lens(ids[0]).0.as_deref(), Some("Helios 44-2 58mm f/2"));
}

#[test]
fn notes_can_be_written_and_searched() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    queries::set_note(&lib, &ids[..1], "note", "  Grandma's birthday ").unwrap();
    queries::set_note(&lib, &ids[..2], "settings", "tripod, bulb 4s, ND8").unwrap();
    assert!(queries::set_note(&lib, &ids, "lens", "x").is_err());

    let p = queries::get_photo(&lib, ids[0]).unwrap().photo;
    assert_eq!(p.note.as_deref(), Some("Grandma's birthday"));
    assert_eq!(p.settings_note.as_deref(), Some("tripod, bulb 4s, ND8"));
    assert_eq!(queries::get_photo(&lib, ids[2]).unwrap().photo.note, None);

    let found = |text: &str| {
        let f = Filter { text: Some(text.into()), ..Default::default() };
        queries::list_ids(&lib, &f, Sort::TakenAsc, 0, 100).unwrap().len()
    };
    assert_eq!(found("birthday"), 1);
    assert_eq!(found("bulb"), 2);
    queries::set_note(&lib, &ids[2..], "private", "first try with the new filter").unwrap();
    assert_eq!(queries::get_photo(&lib, ids[2]).unwrap().photo.private_note.as_deref(), Some("first try with the new filter"));
    assert_eq!(found("new filter"), 1);
    queries::set_note(&lib, &ids[2..], "settings", "handheld").unwrap();
    assert_eq!(queries::list_settings_notes(&lib).unwrap(), vec!["tripod, bulb 4s, ND8", "handheld"]);

    queries::set_note(&lib, &ids, "note", "").unwrap();
    assert_eq!(queries::get_photo(&lib, ids[0]).unwrap().photo.note, None);
    assert_eq!(found("birthday"), 0);
}

#[test]
fn albums_group_into_topics() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    let kyoto = queries::create_album(&lib, "2019 Kyoto").unwrap();
    let bali = queries::create_album(&lib, "2020 Bali").unwrap();
    queries::add_to_album(&lib, kyoto, &ids).unwrap();

    let travel = queries::create_topic(&lib, " Travel ").unwrap();
    assert_eq!(queries::create_topic(&lib, "travel").unwrap(), travel, "same name, any case");
    let family = queries::create_topic(&lib, "Family").unwrap();

    queries::set_album_topic(&lib, &[kyoto, bali], Some(travel)).unwrap();
    let topics = queries::list_topics(&lib).unwrap();
    let summary = |ts: &[queries::Topic]| ts.iter().map(|t| (t.name.clone(), t.albums)).collect::<Vec<_>>();
    assert_eq!(summary(&topics), vec![("Family".into(), 0), ("Travel".into(), 2)]);
    assert!(topics[1].cover.is_some(), "cover from Kyoto, the album with photos");
    let topic_of = |id: i64| queries::list_albums(&lib).unwrap().into_iter().find(|a| a.id == id).unwrap().topic_id;
    assert_eq!(topic_of(kyoto), Some(travel));

    queries::set_album_topic(&lib, &[bali], Some(family)).unwrap();
    assert_eq!(topic_of(bali), Some(family));
    queries::set_album_topic(&lib, &[bali], None).unwrap();
    assert_eq!(topic_of(bali), None);

    queries::rename_topic(&lib, travel, "Trips").unwrap();
    assert_eq!(queries::list_topics(&lib).unwrap()[1].name, "Trips");

    // Deleting a topic keeps its albums and their photos.
    queries::delete_topic(&lib, travel).unwrap();
    assert_eq!(topic_of(kyoto), None);
    assert_eq!(queries::list_albums(&lib).unwrap().len(), 2);
    assert_eq!(queries::list_albums(&lib).unwrap().iter().find(|a| a.id == kyoto).unwrap().count, 3);
}

#[test]
fn albums_have_sections() {
    let Env { _tmp, src, lib } = env();
    three_photos(&lib, &src);
    // A fourth photo, a day later.
    let bytes = with_exif(jpeg_bytes(&picture(3, 64, 64), 90), "2021:07:16 09:00:00", "Cam");
    write(&src.join("later/p3.jpg"), &bytes);
    import_all(&lib, &src.join("later"));
    let ids: Vec<i64> = queries::list_photos(&lib, &Filter::default(), Sort::TakenAsc, 0, 10).unwrap().iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 4);

    let japan = queries::create_album(&lib, "2019 Japan").unwrap();
    let other = queries::create_album(&lib, "Best of").unwrap();
    queries::add_to_album(&lib, japan, &ids).unwrap();
    queries::add_to_album(&lib, other, &ids).unwrap();

    let kyoto = queries::create_section(&lib, japan, " Kyoto ").unwrap();
    assert_eq!(queries::create_section(&lib, japan, "kyoto").unwrap(), kyoto, "same name, any case");
    let nara = queries::create_section(&lib, japan, "Nara").unwrap();
    // A section of another album can't take this album's photos.
    let elsewhere = queries::create_section(&lib, other, "Kyoto").unwrap();
    assert_ne!(elsewhere, kyoto);
    assert!(queries::set_photo_section(&lib, japan, &ids[..1], Some(elsewhere)).is_err());
    queries::delete_section(&lib, elsewhere).unwrap();

    queries::set_photo_section(&lib, japan, &ids[..2], Some(nara)).unwrap();
    queries::set_photo_section(&lib, japan, &ids[2..3], Some(kyoto)).unwrap();
    let sections = |album: i64| -> Vec<(String, i64)> {
        queries::list_sections(&lib, album).unwrap().into_iter().map(|s| (s.name, s.count)).collect()
    };
    assert_eq!(sections(japan), vec![("Kyoto".into(), 1), ("Nara".into(), 2)]);

    // By section: Kyoto, then Nara (by date), then the photo in no section.
    let f = Filter { album_id: Some(japan), ..Default::default() };
    let order = |f: &Filter| -> Vec<i64> { queries::list_photos(&lib, f, Sort::Section, 0, 10).unwrap().iter().map(|p| p.id).collect() };
    assert_eq!(order(&f), vec![ids[2], ids[0], ids[1], ids[3]]);
    assert_eq!(queries::list_ids(&lib, &f, Sort::Section, 0, 10).unwrap(), order(&f));
    let buckets = |f: &Filter| -> Vec<(Option<i64>, String, i64)> {
        queries::date_buckets(&lib, f, Sort::Section).unwrap().into_iter().map(|b| (b.section, b.label, b.count)).collect()
    };
    assert_eq!(buckets(&f), vec![(Some(kyoto), "Kyoto".into(), 1), (Some(nara), "Nara".into(), 2), (Some(0), String::new(), 1)]);
    // The same photos in another album have no section there; outside an album it sorts by date.
    assert_eq!(order(&Filter { album_id: Some(other), ..Default::default() }), ids);
    assert_eq!(buckets(&Filter { album_id: Some(other), ..Default::default() }), vec![(None, String::new(), 4)]);
    assert_eq!(order(&Filter::default()), ids);
    let detail = queries::get_photo(&lib, ids[2]).unwrap();
    assert_eq!((detail.albums.len(), detail.sections.get(&japan).map(String::as_str), detail.sections.get(&other)), (2, Some("Kyoto"), None));

    queries::move_section(&lib, nara, -1).unwrap();
    assert_eq!(sections(japan), vec![("Nara".into(), 2), ("Kyoto".into(), 1)]);
    assert_eq!(order(&f), vec![ids[0], ids[1], ids[2], ids[3]]);
    queries::move_section(&lib, nara, -1).unwrap(); // already first: no change
    assert_eq!(sections(japan)[0].0, "Nara");
    queries::rename_section(&lib, nara, "Nara Park").unwrap();

    // Split by day only takes photos in no section.
    assert_eq!(queries::split_by_day(&lib, japan).unwrap(), 1);
    assert_eq!(sections(japan), vec![("Nara Park".into(), 2), ("Kyoto".into(), 1), ("2021-07-16".into(), 1)]);
    assert_eq!(queries::split_by_day(&lib, japan).unwrap(), 0);

    // Deleting a section keeps its photos in the album, in no section.
    queries::delete_section(&lib, nara).unwrap();
    assert_eq!(buckets(&f).last().unwrap(), &(Some(0), String::new(), 2));
    assert_eq!(queries::list_albums(&lib).unwrap().iter().find(|a| a.id == japan).unwrap().count, 4);
    queries::set_photo_section(&lib, japan, &ids[2..3], None).unwrap();
    assert_eq!(sections(japan), vec![("Kyoto".into(), 0), ("2021-07-16".into(), 1)]);

    // Deleting the album removes its sections.
    queries::delete_album(&lib, japan).unwrap();
    assert!(sections(japan).is_empty());
}

#[test]
fn emptying_trash_hands_files_to_the_system_trash() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    let album = queries::create_album(&lib, "A").unwrap();
    queries::add_to_album(&lib, album, &ids[..1]).unwrap();
    queries::trash_photos(&lib, &ids[..2]).unwrap();

    let discarded = std::sync::Mutex::new(Vec::new());
    let n = queries::empty_trash_with(&lib, |p| {
        discarded.lock().unwrap().push(p.file_name().unwrap().to_string_lossy().into_owned());
        Ok(std::fs::remove_file(p)?)
    })
    .unwrap();
    assert_eq!(n, 2);
    let mut d = discarded.into_inner().unwrap();
    d.sort();
    assert_eq!(d, vec!["p0.jpg", "p1.jpg"]);
    // Empty folders left in the trash are cleaned up.
    assert_eq!(std::fs::read_dir(lib.trash_dir().join("originals")).unwrap().count(), 0);
}

#[test]
fn deleted_photos_are_not_imported_again() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    queries::trash_photos(&lib, &ids[..2]).unwrap();
    // Still in the trash: counts as already in the library.
    assert_eq!(import_all(&lib, &src).skipped, 3);
    empty_trash(&lib).unwrap();

    // Same card again: the emptied photos stay out.
    let p = import_all(&lib, &src);
    assert_eq!((p.added, p.skipped, p.skipped_deleted), (0, 1, 2));
    assert_eq!(queries::list_imports(&lib).unwrap()[0].skipped_deleted, 2);

    // Unless asked for; once back, they're ordinary library photos again.
    let opts = ImportOptions { include_deleted: true, ..Default::default() };
    let p = import::run(&lib, &src, &opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((p.added, p.skipped, p.skipped_deleted), (2, 1, 0));
    let p = import_all(&lib, &src);
    assert_eq!((p.added, p.skipped, p.skipped_deleted), (0, 3, 0));
}

#[test]
fn reimport_skips_known_files_without_reading_them() {
    use std::os::unix::fs::PermissionsExt;
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    queries::trash_photos(&lib, &ids[..1]).unwrap();
    empty_trash(&lib).unwrap();
    // Unreadable now, but unchanged in path, size and time: still recognised.
    for i in 0..3 {
        std::fs::set_permissions(src.join(format!("p{i}.jpg")), std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    let p = import_all(&lib, &src);
    assert_eq!((p.added, p.skipped, p.skipped_deleted, p.failed), (0, 2, 1, 0));

    // A changed file is read again.
    std::fs::set_permissions(src.join("p1.jpg"), std::fs::Permissions::from_mode(0o644)).unwrap();
    write(&src.join("p1.jpg"), &jpeg_bytes(&picture(2, 80, 80), 90));
    let p = import_all(&lib, &src);
    assert_eq!((p.added, p.skipped, p.skipped_deleted, p.failed), (1, 1, 1, 0));

    // Bringing deleted photos back has to read them (the file is unreadable here, so it fails).
    let opts = ImportOptions { include_deleted: true, ..Default::default() };
    let p = import::run(&lib, &src, &opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((p.skipped, p.failed), (2, 1));
}

#[test]
fn skips_hidden_nas_and_trash_entries() {
    let Env { _tmp, src, lib } = env();
    write(&src.join("keep.jpg"), &jpeg_bytes(&picture(0, 64, 64), 90));
    write(&src.join("@eaDir/keep.jpg/SYNOPHOTO_THUMB_XL.jpg"), &jpeg_bytes(&picture(1, 64, 64), 90));
    write(&src.join("._keep.jpg"), b"AppleDouble junk");
    write(&src.join(".thumbnails/t.jpg"), &jpeg_bytes(&picture(2, 64, 64), 90));
    write(&src.join("$RECYCLE.BIN/old.jpg"), &jpeg_bytes(&picture(2, 32, 32), 90));

    let p = import_all(&lib, &src);
    assert_eq!((p.total, p.added, p.failed), (1, 1, 0));
}

#[test]
fn refuses_the_library_through_a_symlink() {
    let Env { _tmp, src, lib } = env();
    let link = src.join("lib-link");
    std::os::unix::fs::symlink(&lib.root, &link).unwrap();
    assert!(import::run(&lib, &link, &ImportOptions::default(), &AtomicBool::new(false), |_| {}).is_err());
    assert!(import::run(&lib, &link.join("originals/.."), &ImportOptions::default(), &AtomicBool::new(false), |_| {}).is_err());
}

#[test]
fn unreadable_folders_are_reported() {
    use std::os::unix::fs::PermissionsExt;
    let Env { _tmp, src, lib } = env();
    write(&src.join("a.jpg"), &jpeg_bytes(&picture(0, 64, 64), 90));
    let locked = src.join("locked");
    write(&locked.join("b.jpg"), &jpeg_bytes(&picture(1, 64, 64), 90));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&locked).is_ok() {
        return; // running as root: permissions aren't enforced
    }
    let p = import_all(&lib, &src);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!((p.added, p.failed), (1, 1));
    assert!(p.errors[0].path.ends_with("locked"));
}

#[test]
fn progress_reports_scanning_and_caps_errors() {
    let Env { _tmp, src, lib } = env();
    let n = import::MAX_SENT_ERRORS + 10;
    for i in 0..n {
        write(&src.join(format!("broken-{i}.jpg")), format!("not a jpeg {i}").as_bytes());
    }
    let events = std::sync::Mutex::new(Vec::new());
    let p = import::run(&lib, &src, &ImportOptions::default(), &AtomicBool::new(false), |p| events.lock().unwrap().push(p.clone())).unwrap();
    let events = events.into_inner().unwrap();

    assert!(events[0].scanning && !events.last().unwrap().scanning);
    assert!(events.iter().all(|e| e.errors.len() <= import::MAX_SENT_ERRORS));
    let last = events.last().unwrap();
    assert!(last.finished && last.failed == n);
    // The full list is still returned and saved.
    assert_eq!(p.errors.len(), n);
    assert_eq!(queries::import_errors(&lib, p.import_id).unwrap().len(), n);
}

// ---- large libraries ----

#[test]
fn organized_folders_become_albums() {
    let Env { _tmp, src, lib } = env();
    let photo = |i: u32, day: u32| with_exif(jpeg_bytes(&picture(i, 64, 64), 90), &format!("2021:07:{day:02} 10:00:00"), "Cam");
    // Already in the library from an earlier (camera) import.
    write(&src.join("card/old.jpg"), &photo(0, 1));
    import_all(&lib, &src.join("card"));
    // An existing album whose name differs only in case is reused.
    let kyoto = queries::create_album(&lib, "2019 kyoto").unwrap();

    let tree = src.join("my-fotos");
    write(&tree.join("2019 Kyoto/a.jpg"), &photo(1, 3));
    write(&tree.join("2019 Kyoto/b.jpg"), &photo(2, 2));
    write(&tree.join("2019 Kyoto/old copy.jpg"), &photo(0, 1));
    write(&tree.join("2020 Bali/beach/c.jpg"), &photo(3, 4));
    write(&tree.join("loose.jpg"), &photo(4, 5));
    let run = |albums: bool| import::run(&lib, &tree, &ImportOptions { folder_albums: albums, ..Default::default() }, &AtomicBool::new(false), |_| {}).unwrap();

    let p = run(true);
    assert_eq!((p.added, p.skipped, p.albums), (4, 1, 3));
    let summary = || -> Vec<(String, i64)> {
        let mut a: Vec<_> = queries::list_albums(&lib).unwrap().into_iter().map(|a| (a.name, a.count)).collect();
        a.sort();
        a
    };
    assert_eq!(summary(), vec![("2019 kyoto".into(), 3), ("beach".into(), 1), ("my-fotos".into(), 1)]);
    // Album order follows the date taken; the existing photo was linked, not copied again.
    let in_kyoto: Vec<String> = queries::list_photos(&lib, &Filter { album_id: Some(kyoto), ..Default::default() }, Sort::TakenAsc, 0, 10)
        .unwrap()
        .into_iter()
        .map(|p| p.orig_name)
        .collect();
    assert_eq!(in_kyoto, vec!["old.jpg", "b.jpg", "a.jpg"]);
    assert_eq!(queries::counts(&lib).unwrap().all, 5);
    assert!(queries::list_imports(&lib).unwrap()[0].folder_albums);

    // Importing again adds nothing and makes no new albums.
    let p = run(true);
    assert_eq!((p.added, p.skipped, p.albums), (0, 5, 3));
    assert_eq!(summary(), vec![("2019 kyoto".into(), 3), ("beach".into(), 1), ("my-fotos".into(), 1)]);

    // A plain import never touches albums.
    write(&tree.join("2021 Taipei/d.jpg"), &photo(5, 6));
    let p = run(false);
    assert_eq!((p.added, p.albums), (1, 0));
    assert_eq!(summary().len(), 3);

    // Picking an event folder itself, after it was imported as a plain import:
    // "Import again" as organized folders links the existing photos into its album.
    let event = src.join("261010 Fireworks");
    write(&event.join("f1.jpg"), &photo(6, 7));
    write(&event.join("f2.jpg"), &photo(1, 8));
    import_all(&lib, &event);
    let p = import::run(&lib, &event, &ImportOptions { folder_albums: true, ..Default::default() }, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((p.added, p.skipped, p.albums), (0, 2, 1));
    assert!(summary().contains(&("261010 Fireworks".into(), 2)));
}

#[test]
fn subfolders_can_become_sections() {
    let Env { _tmp, src, lib } = env();
    let photo = |i: u32, day: u32| with_exif(jpeg_bytes(&picture(i, 64, 64), 90), &format!("2021:07:{day:02} 10:00:00"), "Cam");
    let tree = src.join("my-fotos");
    write(&tree.join("Japan/b.jpg"), &photo(0, 1));
    write(&tree.join("Japan/Kyoto/a.jpg"), &photo(1, 2));
    write(&tree.join("Japan/Kyoto/Day 2/c.jpg"), &photo(2, 3));
    write(&tree.join("Japan/Nara/d.jpg"), &photo(3, 4));
    write(&tree.join("loose.jpg"), &photo(4, 5));
    let opts = ImportOptions { folder_albums: true, folder_sections: true, ..Default::default() };
    let p = import::run(&lib, &tree, &opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!((p.added, p.albums), (5, 2));
    assert!(queries::list_imports(&lib).unwrap()[0].folder_sections);

    let albums: Vec<(String, i64)> = queries::list_albums(&lib).unwrap().into_iter().map(|a| (a.name, a.count)).collect();
    assert_eq!(albums, vec![("Japan".into(), 4), ("my-fotos".into(), 1)]);
    let japan = queries::list_albums(&lib).unwrap()[0].id;
    let sections = || -> Vec<(String, i64)> {
        queries::list_sections(&lib, japan).unwrap().into_iter().map(|s| (s.name, s.count)).collect()
    };
    assert_eq!(sections(), vec![("Kyoto".into(), 1), ("Kyoto / Day 2".into(), 1), ("Nara".into(), 1)]);
    let f = Filter { album_id: Some(japan), ..Default::default() };
    let names: Vec<String> = queries::list_photos(&lib, &f, Sort::Section, 0, 10).unwrap().into_iter().map(|p| p.orig_name).collect();
    assert_eq!(names, vec!["a.jpg", "c.jpg", "d.jpg", "b.jpg"]);

    // Importing again keeps sections the user changed.
    let nara = queries::list_sections(&lib, japan).unwrap()[2].id;
    let a = queries::list_photos(&lib, &f, Sort::Section, 0, 1).unwrap()[0].id;
    queries::set_photo_section(&lib, japan, &[a], Some(nara)).unwrap();
    import::run(&lib, &tree, &opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(sections(), vec![("Kyoto".into(), 0), ("Kyoto / Day 2".into(), 1), ("Nara".into(), 2)]);
}

/// Inserts `n` photo rows directly (no files), spread over many days, newest id = latest photo.
fn seed_rows(lib: &Library, n: usize) {
    let conn = lib.conn();
    conn.execute_batch("BEGIN").unwrap();
    {
        let mut stmt = conn
            .prepare(
                "INSERT INTO photos (hash, rel_path, orig_name, taken_at, imported_at, file_size)
                 VALUES (?1, ?2, ?3, ?4, '2026-01-01T00:00:00', 1)",
            )
            .unwrap();
        let start = NaiveDate::from_ymd_opt(2015, 1, 1).unwrap().and_hms_opt(8, 0, 0).unwrap();
        for i in 0..n {
            // ~7 photos per day, 7 minutes apart.
            let taken = start + chrono::Duration::minutes(i as i64 * 7 + (i as i64 / 7) * 1391);
            let name = format!("IMG_{i:06}.jpg");
            stmt.execute(rusqlite::params![
                format!("hash{i}"),
                format!("originals/{}/{name}", taken.format("%Y/%m/%d")),
                name,
                taken.format("%Y-%m-%dT%H:%M:%S").to_string(),
            ])
            .unwrap();
        }
    }
    conn.execute_batch("COMMIT").unwrap();
}

#[test]
fn ids_date_span_and_toggles() {
    let Env { _tmp, lib, .. } = env();
    seed_rows(&lib, 20);
    let all = Filter::default();

    // Ids come back in the same order (and window) as full photo rows.
    for (offset, limit) in [(0, 100), (5, 7)] {
        let photos = queries::list_photos(&lib, &all, Sort::TakenDesc, offset, limit).unwrap();
        let ids = queries::list_ids(&lib, &all, Sort::TakenDesc, offset, limit).unwrap();
        assert_eq!(ids, photos.iter().map(|p| p.id).collect::<Vec<_>>());
    }

    let photos = queries::list_photos(&lib, &all, Sort::TakenAsc, 0, 100).unwrap();
    let ids: Vec<i64> = photos.iter().map(|p| p.id).collect();
    let (first, last) = (&photos[0].taken_at[..10], &photos[19].taken_at[..10]);
    assert_eq!(queries::date_span(&lib, &ids).unwrap(), Some((first.to_string(), last.to_string())));
    assert_eq!(queries::date_span(&lib, &[]).unwrap(), None);

    // Mixed selection -> all favorite; all favorite -> none.
    queries::set_favorite(&lib, &ids[..3], true).unwrap();
    assert!(queries::toggle_favorite(&lib, &ids[..6]).unwrap());
    assert_eq!(queries::counts(&lib).unwrap().favorites, 6);
    assert!(!queries::toggle_favorite(&lib, &ids[..6]).unwrap());
    assert_eq!(queries::counts(&lib).unwrap().favorites, 0);

    // Marks: mixed (including another mark) -> set; all carrying it -> cleared.
    let mark_of = |id: i64| queries::get_photo(&lib, id).unwrap().photo.mark;
    frutyfoto_lib::marks::set_mark(&lib, &ids[..2], Some("1")).unwrap();
    frutyfoto_lib::marks::set_mark(&lib, &ids[2..3], Some("x")).unwrap();
    frutyfoto_lib::marks::toggle_mark(&lib, &ids[..4], "1").unwrap();
    assert!(ids[..4].iter().all(|&id| mark_of(id).as_deref() == Some("1")));
    frutyfoto_lib::marks::toggle_mark(&lib, &ids[..4], "1").unwrap();
    assert!(ids[..4].iter().all(|&id| mark_of(id).is_none()));
    assert!(frutyfoto_lib::marks::toggle_mark(&lib, &ids[..1], "z").is_err());
}

/// Timing check for a big library: `cargo test --release -- --ignored --nocapture large_library`.
#[test]
#[ignore]
fn large_library_queries_stay_fast() {
    const N: usize = 100_000;
    let Env { _tmp, lib, .. } = env();
    seed_rows(&lib, N);
    let all = Filter::default();
    let time = |label: &str, f: &dyn Fn() -> usize| {
        let t = std::time::Instant::now();
        let n = f();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("{label:<32} {ms:>8.1} ms  ({n} rows)");
        ms
    };
    let mut worst: f64 = 0.0;
    worst = worst.max(time("date_buckets", &|| queries::date_buckets(&lib, &all, Sort::TakenDesc).unwrap().len()));
    for offset in [0, N as i64 / 2, N as i64 - 200] {
        worst = worst.max(time(&format!("list_photos @ {offset}"), &|| {
            queries::list_photos(&lib, &all, Sort::TakenDesc, offset, 200).unwrap().len()
        }));
    }
    worst = worst.max(time("list_ids (all)", &|| queries::list_ids(&lib, &all, Sort::TakenDesc, 0, N as i64).unwrap().len()));
    worst = worst.max(time("counts", &|| queries::counts(&lib).unwrap().all as usize));
    assert!(worst < 100.0, "slowest query took {worst:.1} ms");
}
