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

/// A zip written entry by entry to `W` (an export file), with Zip64 records once offsets or
/// the entry count outgrow the classic format. Each entry is compressed in memory, so one
/// entry must stay under 4 GiB; callers skip larger files.
pub struct ZipWriter<W: Write> {
    out: W,
    offset: u64,
    central: Vec<u8>,
    count: u64,
    dos_time: [u8; 4],
    /// Offsets and counts at or above this use Zip64 (lowered in tests).
    limit: u64,
}

/// Largest entry [`ZipWriter`] takes.
pub const MAX_ZIP_ENTRY: u64 = u32::MAX as u64 - 1;

impl<W: Write> ZipWriter<W> {
    /// Entries are dated `unix_secs` (UTC).
    pub fn new(out: W, unix_secs: i64) -> Self {
        Self {
            out,
            offset: 0,
            central: Vec::new(),
            count: 0,
            dos_time: dos_time(unix_secs),
            limit: u64::from(u32::MAX),
        }
    }

    #[cfg(test)]
    fn with_limit(mut self, limit: u64) -> Self {
        self.limit = limit;
        self
    }

    /// Adds a file named `name` (`/` separators) holding `data`; deflated unless that
    /// doesn't make it smaller.
    pub fn add(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        let size = data.len() as u64;
        if size > MAX_ZIP_ENTRY {
            return Err(std::io::Error::other(format!(
                "{name} is too large for the zip"
            )));
        }
        let deflated = if data.is_empty() {
            None
        } else {
            let mut enc =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(data)?;
            Some(enc.finish()?).filter(|d| d.len() < data.len())
        };
        let (method, body): (u16, &[u8]) = match &deflated {
            Some(d) => (8, d),
            None => (0, data),
        };
        let crc = crc32fast::hash(data);
        let n = u16::try_from(name.len())
            .map_err(|_| std::io::Error::other(format!("{name} has too long a name")))?;
        let csize = u32::try_from(body.len()).unwrap_or(u32::MAX);
        let usize_ = u32::try_from(size).unwrap_or(u32::MAX);
        let big_offset = self.offset >= self.limit;
        let version: u16 = if big_offset { 45 } else { 20 };

        let mut local = Vec::with_capacity(30 + name.len());
        local.extend_from_slice(&LOCAL.to_le_bytes());
        local.extend_from_slice(&version.to_le_bytes());
        local.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        local.extend_from_slice(&method.to_le_bytes());
        local.extend_from_slice(&self.dos_time);
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&csize.to_le_bytes());
        local.extend_from_slice(&usize_.to_le_bytes());
        local.extend_from_slice(&n.to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name.as_bytes());
        self.out.write_all(&local)?;
        self.out.write_all(body)?;

        let c = &mut self.central;
        c.extend_from_slice(&CENTRAL.to_le_bytes());
        c.extend_from_slice(&(0x0300u16 | version).to_le_bytes()); // made on Unix
        c.extend_from_slice(&version.to_le_bytes());
        c.extend_from_slice(&0x0800u16.to_le_bytes());
        c.extend_from_slice(&method.to_le_bytes());
        c.extend_from_slice(&self.dos_time);
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&csize.to_le_bytes());
        c.extend_from_slice(&usize_.to_le_bytes());
        c.extend_from_slice(&n.to_le_bytes());
        c.extend_from_slice(&(if big_offset { 12u16 } else { 0 }).to_le_bytes());
        c.extend_from_slice(&[0; 6]); // comment, disk, internal attributes
        c.extend_from_slice(&((0o100_644u32) << 16).to_le_bytes());
        let off32 = if big_offset {
            u32::MAX
        } else {
            u32::try_from(self.offset).unwrap_or(u32::MAX)
        };
        c.extend_from_slice(&off32.to_le_bytes());
        c.extend_from_slice(name.as_bytes());
        if big_offset {
            c.extend_from_slice(&1u16.to_le_bytes());
            c.extend_from_slice(&8u16.to_le_bytes());
            c.extend_from_slice(&self.offset.to_le_bytes());
        }
        self.offset += (local.len() + body.len()) as u64;
        self.count += 1;
        Ok(())
    }

    /// Writes the central directory and returns the writer.
    pub fn finish(mut self) -> std::io::Result<W> {
        let cd_offset = self.offset;
        let cd_size = self.central.len() as u64;
        self.out.write_all(&self.central)?;
        let zip64 = cd_offset >= self.limit
            || cd_offset + cd_size >= self.limit
            || self.count >= u64::from(u16::MAX).min(self.limit);
        if zip64 {
            let rec = cd_offset + cd_size;
            let mut b = Vec::with_capacity(56 + 20);
            b.extend_from_slice(&EOCD64.to_le_bytes());
            b.extend_from_slice(&44u64.to_le_bytes()); // size of the rest of the record
            b.extend_from_slice(&0x032Du16.to_le_bytes()); // made by Unix, 4.5
            b.extend_from_slice(&45u16.to_le_bytes());
            b.extend_from_slice(&[0; 8]); // this disk, the directory's disk
            b.extend_from_slice(&self.count.to_le_bytes());
            b.extend_from_slice(&self.count.to_le_bytes());
            b.extend_from_slice(&cd_size.to_le_bytes());
            b.extend_from_slice(&cd_offset.to_le_bytes());
            b.extend_from_slice(&EOCD64_LOCATOR.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&rec.to_le_bytes());
            b.extend_from_slice(&1u32.to_le_bytes());
            self.out.write_all(&b)?;
        }
        let count16 = if zip64 {
            u16::MAX
        } else {
            u16::try_from(self.count).unwrap_or(u16::MAX)
        };
        let size32 = if zip64 {
            u32::MAX
        } else {
            u32::try_from(cd_size).unwrap_or(u32::MAX)
        };
        let off32 = if zip64 {
            u32::MAX
        } else {
            u32::try_from(cd_offset).unwrap_or(u32::MAX)
        };
        let mut e = Vec::with_capacity(22);
        e.extend_from_slice(&EOCD.to_le_bytes());
        e.extend_from_slice(&[0; 4]);
        e.extend_from_slice(&count16.to_le_bytes());
        e.extend_from_slice(&count16.to_le_bytes());
        e.extend_from_slice(&size32.to_le_bytes());
        e.extend_from_slice(&off32.to_le_bytes());
        e.extend_from_slice(&0u16.to_le_bytes());
        self.out.write_all(&e)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

/// MS-DOS time and date (little-endian, time first) for a Unix time, clamped to 1980–2107.
fn dos_time(unix_secs: i64) -> [u8; 4] {
    let days = unix_secs.div_euclid(86_400);
    let secs = unix_secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (year, month, day, secs) = if year < 1980 {
        (1980, 1, 1, 0)
    } else if year > 2107 {
        (2107, 12, 31, 86_399)
    } else {
        (year, month, day, secs)
    };
    let time =
        u16::try_from(((secs / 3600) << 11) | (((secs % 3600) / 60) << 5) | ((secs % 60) / 2))
            .unwrap_or(0);
    let date = u16::try_from(((year - 1980) << 9) | (month << 5) | day).unwrap_or(0x21);
    let [t0, t1] = time.to_le_bytes();
    let [d0, d1] = date.to_le_bytes();
    [t0, t1, d0, d1]
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

    #[test]
    fn zip_writer_round_trips_through_the_reader() {
        let big: Vec<u8> = (0..50_000u32).flat_map(|i| (i % 7).to_le_bytes()).collect();
        let random: Vec<u8> = (0..2_000u32)
            .map(|i| u8::try_from(i.wrapping_mul(2_654_435_761) >> 24).unwrap_or(0))
            .collect();
        let mut w = ZipWriter::new(Vec::new(), 1_794_600_000);
        w.add("database/messages.json", &big).expect("adds");
        w.add("random.bin", &random).expect("adds");
        w.add("empty.txt", b"").expect("adds");
        w.add("attachments/ä ö.txt", "hej".as_bytes())
            .expect("adds");
        let bytes = w.finish().expect("finishes");
        let mut cur = std::io::Cursor::new(bytes);
        let entries = read_entries(&mut cur).expect("reads");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "database/messages.json",
                "random.bin",
                "empty.txt",
                "attachments/ä ö.txt"
            ]
        );
        assert_eq!(entries[0].method, 8, "compressible data is deflated");
        assert_eq!(
            read_entry(&mut cur, &entries[0], u64::MAX).expect("entry"),
            big
        );
        assert_eq!(
            read_entry(&mut cur, &entries[1], u64::MAX).expect("entry"),
            random
        );
        assert!(
            read_entry(&mut cur, &entries[2], u64::MAX)
                .expect("entry")
                .is_empty()
        );
        assert_eq!(
            read_entry(&mut cur, &entries[3], u64::MAX).expect("entry"),
            b"hej"
        );
    }

    #[test]
    fn zip_writer_switches_to_zip64_past_the_limit() {
        let mut w = ZipWriter::new(Vec::new(), 0).with_limit(100);
        for i in 0..5 {
            w.add(
                &format!("file-{i}.txt"),
                format!("content number {i} ").repeat(10).as_bytes(),
            )
            .expect("adds");
        }
        let bytes = w.finish().expect("finishes");
        assert!(
            bytes.windows(4).any(|b| b == EOCD64.to_le_bytes()),
            "a Zip64 end record is written"
        );
        let mut cur = std::io::Cursor::new(bytes);
        let entries = read_entries(&mut cur).expect("reads");
        assert_eq!(entries.len(), 5);
        assert!(entries[4].offset > 100);
        for (i, e) in entries.iter().enumerate() {
            let data = read_entry(&mut cur, e, u64::MAX).expect("entry");
            assert_eq!(data, format!("content number {i} ").repeat(10).as_bytes());
        }
    }

    #[test]
    fn dos_times_are_civil_dates() {
        // 2026-11-13 21:52:30 UTC
        let t = dos_time(1_794_606_750);
        let time = u16::from_le_bytes([t[0], t[1]]);
        let date = u16::from_le_bytes([t[2], t[3]]);
        assert_eq!((date >> 9) + 1980, 2026);
        assert_eq!((date >> 5) & 0xF, 11);
        assert_eq!(date & 0x1F, 13);
        assert_eq!(time >> 11, 21);
        assert_eq!((time >> 5) & 0x3F, 52);
        assert_eq!((time & 0x1F) * 2, 30);
        assert_eq!(dos_time(0), [0, 0, 0x21, 0], "before 1980 is 1980-01-01");
    }
}
