//! A small zip reader (and writer) for `.mcpb` extensions and zipped skills.
//!
//! Handles stored and deflated entries, Zip64 sizes and offsets, CRC checks, and refuses
//! anything that would write outside the destination (absolute paths, `..`, drive letters).
//! Encrypted entries and other compression methods are refused with a readable message.
//! No zip crate is used: the offline Linux vendor bundle has none, and this is enough for
//! the bundles `mcpb pack` and ordinary zip tools make.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use crate::McpError;

/// Caps on what one archive may unpack to.
#[derive(Debug, Clone, Copy)]
pub struct ZipLimits {
    pub max_bytes: u64,
    pub max_entries: usize,
}

impl Default for ZipLimits {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024 * 1024,
            max_entries: 100_000,
        }
    }
}

/// What [`extract_zip`] wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Unpacked {
    pub files: u32,
    pub bytes: u64,
}

/// One central directory entry.
#[derive(Debug, Clone)]
pub struct ZipEntry {
    /// The path inside the archive, with `/` separators.
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    pub offset: u64,
    /// Unix permission bits, when the archive was made on a Unix system.
    pub mode: Option<u32>,
}

impl ZipEntry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

fn bad(msg: impl Into<String>) -> McpError {
    McpError::Extension(msg.into())
}

fn io(e: std::io::Error) -> McpError {
    bad(format!("couldn't read the archive: {e}"))
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .unwrap_or(0)
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .unwrap_or(0)
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    b.get(at..at + 8)
        .map(|s| {
            let mut a = [0u8; 8];
            a.copy_from_slice(s);
            u64::from_le_bytes(a)
        })
        .unwrap_or(0)
}

const EOCD: u32 = 0x0605_4b50;
const EOCD64: u32 = 0x0606_4b50;
const EOCD64_LOCATOR: u32 = 0x0706_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

/// Reads the central directory.
pub fn read_entries<R: Read + Seek>(r: &mut R) -> Result<Vec<ZipEntry>, McpError> {
    let len = r.seek(SeekFrom::End(0)).map_err(io)?;
    if len < 22 {
        return Err(bad("this isn't a zip file (it's too short)"));
    }
    let tail_len = len.min(22 + 65_535 + 20);
    r.seek(SeekFrom::Start(len - tail_len)).map_err(io)?;
    let mut tail = vec![0u8; usize::try_from(tail_len).unwrap_or(0)];
    r.read_exact(&mut tail).map_err(io)?;
    let at = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|&i| u32_at(&tail, i) == EOCD)
        .ok_or_else(|| bad("this isn't a zip file (no end of central directory)"))?;
    let mut count = u64::from(u16_at(&tail, at + 10));
    let mut cd_size = u64::from(u32_at(&tail, at + 12));
    let mut cd_offset = u64::from(u32_at(&tail, at + 16));
    // Zip64: the locator sits right before the classic record.
    if at >= 20 && u32_at(&tail, at - 20) == EOCD64_LOCATOR {
        let rec = u64_at(&tail, at - 20 + 8);
        r.seek(SeekFrom::Start(rec)).map_err(io)?;
        let mut b = [0u8; 56];
        r.read_exact(&mut b).map_err(io)?;
        if u32_at(&b, 0) != EOCD64 {
            return Err(bad("the archive's Zip64 directory is damaged"));
        }
        count = u64_at(&b, 32);
        cd_size = u64_at(&b, 40);
        cd_offset = u64_at(&b, 48);
    }
    if cd_offset.saturating_add(cd_size) > len {
        return Err(bad("the archive is damaged or incomplete"));
    }
    if cd_size > 256 * 1024 * 1024 {
        return Err(bad("the archive's directory is too large"));
    }
    r.seek(SeekFrom::Start(cd_offset)).map_err(io)?;
    let mut cd = vec![0u8; usize::try_from(cd_size).unwrap_or(0)];
    r.read_exact(&mut cd).map_err(io)?;

    let mut out = Vec::new();
    let mut p = 0usize;
    while out.len() as u64 != count {
        if p + 46 > cd.len() || u32_at(&cd, p) != CENTRAL {
            return Err(bad("the archive's directory is damaged"));
        }
        let made_by = u16_at(&cd, p + 4);
        let flags = u16_at(&cd, p + 8);
        let method = u16_at(&cd, p + 10);
        let crc = u32_at(&cd, p + 16);
        let mut compressed = u64::from(u32_at(&cd, p + 20));
        let mut size = u64::from(u32_at(&cd, p + 24));
        let name_len = usize::from(u16_at(&cd, p + 28));
        let extra_len = usize::from(u16_at(&cd, p + 30));
        let comment_len = usize::from(u16_at(&cd, p + 32));
        let external = u32_at(&cd, p + 38);
        let mut offset = u64::from(u32_at(&cd, p + 42));
        let name_bytes = cd
            .get(p + 46..p + 46 + name_len)
            .ok_or_else(|| bad("the archive's directory is damaged"))?;
        let name = String::from_utf8_lossy(name_bytes).replace('\\', "/");
        let extra = cd
            .get(p + 46 + name_len..p + 46 + name_len + extra_len)
            .unwrap_or(&[]);
        // Zip64 extra field: only the values whose 32-bit fields are maxed out, in order.
        let mut e = 0usize;
        while e + 4 <= extra.len() {
            let id = u16_at(extra, e);
            let n = usize::from(u16_at(extra, e + 2));
            if id == 0x0001 {
                let mut q = e + 4;
                if size == u64::from(u32::MAX) {
                    size = u64_at(extra, q);
                    q += 8;
                }
                if compressed == u64::from(u32::MAX) {
                    compressed = u64_at(extra, q);
                    q += 8;
                }
                if offset == u64::from(u32::MAX) {
                    offset = u64_at(extra, q);
                }
            }
            e += 4 + n;
        }
        let mode = (made_by >> 8 == 3)
            .then_some(external >> 16)
            .filter(|m| *m != 0);
        out.push(ZipEntry {
            name,
            method,
            flags,
            crc,
            compressed,
            size,
            offset,
            mode,
        });
        p += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// The relative path an entry unpacks to, or `None` for entries that are skipped (macOS
/// resource forks). Unsafe paths are an error.
pub fn safe_path(name: &str) -> Result<Option<PathBuf>, McpError> {
    let name = name.replace('\\', "/");
    if name.starts_with("__MACOSX/") || name.split('/').any(|p| p == ".DS_Store") {
        return Ok(None);
    }
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty() {
        return Ok(None);
    }
    let unsafe_path = || bad(format!("the archive contains an unsafe path: {name}"));
    if name.starts_with('/') || trimmed.contains(':') {
        return Err(unsafe_path());
    }
    let mut out = PathBuf::new();
    for c in Path::new(trimmed).components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return Err(unsafe_path()),
        }
    }
    if out.as_os_str().is_empty() {
        return Ok(None);
    }
    Ok(Some(out))
}

/// Opens one entry's data for reading (decompressed).
fn entry_reader<'a, R: Read + Seek>(
    r: &'a mut R,
    e: &ZipEntry,
) -> Result<Box<dyn Read + 'a>, McpError> {
    if e.flags & 1 != 0 {
        return Err(bad(format!(
            "{} is encrypted; Oxide can't open password-protected archives",
            e.name
        )));
    }
    r.seek(SeekFrom::Start(e.offset)).map_err(io)?;
    let mut h = [0u8; 30];
    r.read_exact(&mut h).map_err(io)?;
    if u32_at(&h, 0) != LOCAL {
        return Err(bad("the archive is damaged (bad local header)"));
    }
    let skip = u64::from(u16_at(&h, 26)) + u64::from(u16_at(&h, 28));
    r.seek(SeekFrom::Current(i64::try_from(skip).unwrap_or(0)))
        .map_err(io)?;
    let data = r.take(e.compressed);
    match e.method {
        0 => Ok(Box::new(data)),
        8 => Ok(Box::new(flate2::read::DeflateDecoder::new(data))),
        m => Err(bad(format!(
            "{} uses a compression method Oxide doesn't support ({m}); re-pack it with \
             standard zip compression",
            e.name
        ))),
    }
}

/// Reads one entry fully into memory, checking its size and CRC.
pub fn read_entry<R: Read + Seek>(r: &mut R, e: &ZipEntry, max: u64) -> Result<Vec<u8>, McpError> {
    if e.size > max {
        return Err(bad(format!("{} is too large", e.name)));
    }
    let mut out = Vec::with_capacity(usize::try_from(e.size).unwrap_or(0));
    let reader = entry_reader(r, e)?;
    reader
        .take(e.size + 1)
        .read_to_end(&mut out)
        .map_err(|err| bad(format!("couldn't unpack {}: {err}", e.name)))?;
    check(e, &out)?;
    Ok(out)
}

fn check(e: &ZipEntry, data: &[u8]) -> Result<(), McpError> {
    if data.len() as u64 != e.size {
        return Err(bad(format!("{} is damaged (wrong size)", e.name)));
    }
    if crc32fast::hash(data) != e.crc {
        return Err(bad(format!("{} is damaged (checksum mismatch)", e.name)));
    }
    Ok(())
}

/// Unpacks `archive` into `dest` (created if needed).
pub fn extract_zip(archive: &Path, dest: &Path, limits: ZipLimits) -> Result<Unpacked, McpError> {
    let mut f = File::open(archive).map_err(|e| bad(format!("couldn't open the file: {e}")))?;
    let entries = read_entries(&mut f)?;
    if entries.len() > limits.max_entries {
        return Err(bad(format!(
            "the archive has too many files (more than {})",
            limits.max_entries
        )));
    }
    let total: u64 = entries.iter().map(|e| e.size).sum();
    if total > limits.max_bytes {
        return Err(bad(format!(
            "the archive unpacks to more than {} MB",
            limits.max_bytes / (1024 * 1024)
        )));
    }
    std::fs::create_dir_all(dest).map_err(|e| bad(format!("couldn't create a folder: {e}")))?;
    let mut done = Unpacked::default();
    for e in &entries {
        let Some(rel) = safe_path(&e.name)? else {
            continue;
        };
        let target = dest.join(&rel);
        if e.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|err| bad(format!("couldn't create {}: {err}", rel.display())))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| bad(format!("couldn't create {}: {err}", rel.display())))?;
        }
        let mut reader = entry_reader(&mut f, e)?;
        let mut out = File::create(&target)
            .map_err(|err| bad(format!("couldn't write {}: {err}", rel.display())))?;
        let mut hasher = crc32fast::Hasher::new();
        let mut written = 0u64;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|err| bad(format!("couldn't unpack {}: {err}", e.name)))?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if written > e.size {
                return Err(bad(format!("{} is damaged (wrong size)", e.name)));
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])
                .map_err(|err| bad(format!("couldn't write {}: {err}", rel.display())))?;
        }
        drop(reader);
        if written != e.size || hasher.finalize() != e.crc {
            return Err(bad(format!("{} is damaged (checksum mismatch)", e.name)));
        }
        #[cfg(unix)]
        if let Some(mode) = e.mode {
            use std::os::unix::fs::PermissionsExt;
            let perm = 0o644 | (mode & 0o111);
            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(perm));
        }
        done.files += 1;
        done.bytes += written;
    }
    Ok(done)
}

/// Builds a zip in memory (deflated entries; names ending in `/` are folders). Used by tests
/// and to make sample bundles.
pub fn write_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, data) in entries {
        let is_dir = name.ends_with('/');
        let (method, body): (u16, Vec<u8>) = if is_dir || data.is_empty() {
            (0, Vec::new())
        } else {
            let mut enc =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            let _ = enc.write_all(data);
            (8, enc.finish().unwrap_or_default())
        };
        let crc = crc32fast::hash(data);
        let offset = u32::try_from(out.len()).unwrap_or(u32::MAX);
        let n = u16::try_from(name.len()).unwrap_or(u16::MAX);
        let csize = u32::try_from(body.len()).unwrap_or(u32::MAX);
        let usize_ = u32::try_from(data.len()).unwrap_or(u32::MAX);
        // Local header.
        out.extend_from_slice(&LOCAL.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0x21, 0]); // time, date (1980-01-01)
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&csize.to_le_bytes());
        out.extend_from_slice(&usize_.to_le_bytes());
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&body);
        // Central entry (made on Unix, so permissions can travel).
        central.extend_from_slice(&CENTRAL.to_le_bytes());
        central.extend_from_slice(&0x0314u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0x0800u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0, 0, 0x21, 0]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&csize.to_le_bytes());
        central.extend_from_slice(&usize_.to_le_bytes());
        central.extend_from_slice(&n.to_le_bytes());
        central.extend_from_slice(&[0; 8]); // extra, comment, disk, internal attributes
        let mode: u32 = if is_dir { 0o40755 } else { 0o100644 };
        central.extend_from_slice(&(mode << 16).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = u32::try_from(out.len()).unwrap_or(u32::MAX);
    let cd_size = u32::try_from(central.len()).unwrap_or(u32::MAX);
    let count = u16::try_from(entries.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&central);
    out.extend_from_slice(&EOCD.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trip_with_folders() {
        let zip = write_zip(&[
            ("manifest.json", b"{\"name\": \"x\"}"),
            ("server/", b""),
            ("server/index.js", "console.log('hé')".repeat(50).as_bytes()),
            ("empty.txt", b""),
        ]);
        let mut c = Cursor::new(zip.clone());
        let entries = read_entries(&mut c).expect("reads");
        assert_eq!(entries.len(), 4);
        assert!(entries[1].is_dir());
        assert_eq!(entries[2].method, 8);
        let data = read_entry(&mut c, &entries[0], 1000).expect("entry");
        assert_eq!(data, b"{\"name\": \"x\"}");
        assert!(read_entry(&mut c, &entries[2], 10).is_err(), "over the cap");

        let dir = tempfile::tempdir().expect("tmp");
        let file = dir.path().join("x.zip");
        std::fs::write(&file, &zip).expect("write");
        let out = dir.path().join("out");
        let done = extract_zip(&file, &out, ZipLimits::default()).expect("extracts");
        assert_eq!(done.files, 3);
        assert_eq!(
            std::fs::read_to_string(out.join("server").join("index.js")).expect("read"),
            "console.log('hé')".repeat(50)
        );
        assert!(out.join("empty.txt").is_file());
    }

    #[test]
    fn refuses_unsafe_paths() {
        assert!(safe_path("../evil").is_err());
        assert!(safe_path("a/../../evil").is_err());
        assert!(safe_path("/etc/passwd").is_err());
        assert!(safe_path("C:/Windows/x").is_err());
        assert!(safe_path("a\\..\\..\\b").is_err());
        assert_eq!(safe_path("__MACOSX/._x").expect("ok"), None);
        assert_eq!(
            safe_path("./a/./b").expect("ok"),
            Some(PathBuf::from("a/b"))
        );

        let dir = tempfile::tempdir().expect("tmp");
        let file = dir.path().join("evil.zip");
        std::fs::write(
            &file,
            write_zip(&[("ok.txt", b"1"), ("../escape.txt", b"2")]),
        )
        .expect("write");
        let err =
            extract_zip(&file, &dir.path().join("out"), ZipLimits::default()).expect_err("unsafe");
        assert!(err.to_string().contains("unsafe path"), "{err}");
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[test]
    fn checks_limits_and_damage() {
        let dir = tempfile::tempdir().expect("tmp");
        let file = dir.path().join("big.zip");
        std::fs::write(&file, write_zip(&[("a", &[7u8; 5000]), ("b", b"x")])).expect("write");
        let small = ZipLimits {
            max_bytes: 1000,
            max_entries: 10,
        };
        assert!(extract_zip(&file, &dir.path().join("o1"), small).is_err());
        let few = ZipLimits {
            max_bytes: 1 << 20,
            max_entries: 1,
        };
        assert!(extract_zip(&file, &dir.path().join("o2"), few).is_err());

        // Flip a byte of the compressed data: the checksum (or inflate) catches it.
        let mut zip = write_zip(&[("t.txt", "hello world ".repeat(20).as_bytes())]);
        zip[40] ^= 0xff;
        std::fs::write(&file, &zip).expect("write");
        assert!(extract_zip(&file, &dir.path().join("o3"), ZipLimits::default()).is_err());

        std::fs::write(&file, b"not a zip at all, just text").expect("write");
        let err = extract_zip(&file, &dir.path().join("o4"), ZipLimits::default())
            .expect_err("not a zip");
        assert!(err.to_string().contains("isn't a zip"), "{err}");
    }

    #[test]
    fn refuses_encrypted_and_unknown_methods() {
        let mut zip = write_zip(&[("a.txt", b"secret stuff here")]);
        let mut c = Cursor::new(zip.clone());
        let e = read_entries(&mut c).expect("reads").remove(0);
        let mut enc = e.clone();
        enc.flags |= 1;
        assert!(read_entry(&mut c, &enc, 100).is_err());
        let mut other = e;
        other.method = 12; // bzip2
        let err = read_entry(&mut c, &other, 100).expect_err("method");
        assert!(err.to_string().contains("compression method"), "{err}");
        zip.truncate(10);
        assert!(read_entries(&mut Cursor::new(zip)).is_err());
    }
}
