//! Lector mínimo de RPF7 con índice sin cifrar (OPEN). No descifra nada ni usa claves de GTA V.
use anyhow::{bail, Result};
use flate2::read::DeflateDecoder;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Default, Debug, Clone)]
pub struct RpfInfo {
    pub valid: bool,
    pub toc_encrypted: bool,
    pub tree: Vec<String>,
    pub assembly_targets: Vec<String>,
    pub device_names: Vec<String>,
    pub weapon_models: Vec<String>,
    pub notes: Vec<String>,
}

struct Reader<'a> {
    f: &'a File,
    base: u64,
}

impl<'a> Reader<'a> {
    fn read_at(&self, off: u64, len: usize) -> Result<Vec<u8>> {
        let mut f: &File = self.f;
        f.seek(SeekFrom::Start(self.base + off))?;
        let mut v = vec![0u8; len];
        f.read_exact(&mut v)?;
        Ok(v)
    }
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub fn inspect(path: &Path) -> Result<RpfInfo> {
    let f = File::open(path)?;
    let len = f.metadata()?.len();
    let mut info = RpfInfo::default();
    let r = Reader { f: &f, base: 0 };
    walk(&r, len, "", 0, &mut info)?;
    info.tree.truncate(400);
    Ok(info)
}

fn walk(r: &Reader, avail: u64, prefix: &str, depth: u32, info: &mut RpfInfo) -> Result<()> {
    if depth > 4 {
        info.notes.push("anidamiento de RPF demasiado profundo".into());
        return Ok(());
    }
    if avail < 16 {
        return Ok(());
    }
    let h = r.read_at(0, 16)?;
    if &h[0..4] != b"7FPR" {
        if depth > 0 {
            info.notes.push(format!("{}: no es un RPF7", prefix));
        }
        return Ok(());
    }
    if depth == 0 {
        info.valid = true;
    }
    let ec = u32le(&h[4..8]) as usize;
    let nl = u32le(&h[8..12]) as usize;
    let enc = u32le(&h[12..16]);
    if enc != 0x4E45_504F && enc != 0 {
        if depth == 0 {
            info.toc_encrypted = true;
        }
        info.notes.push(format!("{}: índice cifrado (0x{:08X})", if prefix.is_empty() { "RPF" } else { prefix }, enc));
        return Ok(());
    }
    if ec == 0 || ec > 200_000 || nl > 16_000_000 {
        bail!("cabecera RPF no válida");
    }
    if 16 + (ec as u64) * 16 + (nl as u64) > avail {
        bail!("RPF truncado");
    }
    let toc = r.read_at(16, ec * 16)?;
    let names = r.read_at(16 + (ec as u64) * 16, nl)?;
    let mut guard = 0u32;
    walk_entry(r, &toc, &names, 0, prefix, depth, info, &mut guard)
}

fn name_at(names: &[u8], off: usize) -> String {
    if off >= names.len() {
        return String::new();
    }
    let end = names[off..].iter().position(|&b| b == 0).map(|p| off + p).unwrap_or(names.len());
    String::from_utf8_lossy(&names[off..end]).to_string()
}

#[allow(clippy::too_many_arguments)]
fn walk_entry(
    r: &Reader,
    toc: &[u8],
    names: &[u8],
    idx: usize,
    dir_path: &str,
    depth: u32,
    info: &mut RpfInfo,
    guard: &mut u32,
) -> Result<()> {
    *guard += 1;
    if *guard > 200_000 {
        bail!("demasiadas entradas en el RPF");
    }
    let ec = toc.len() / 16;
    if idx >= ec {
        return Ok(());
    }
    let e = &toc[idx * 16..idx * 16 + 16];
    let name = name_at(names, u16::from_le_bytes([e[0], e[1]]) as usize);
    let size = (e[2] as u64) | ((e[3] as u64) << 8) | ((e[4] as u64) << 16);
    let off = (e[5] as u64) | ((e[6] as u64) << 8) | ((e[7] as u64) << 16);
    if off == 0x7f_ffff {
        let start = u32le(&e[8..12]) as usize;
        let count = u32le(&e[12..16]) as usize;
        let me = if name.is_empty() {
            dir_path.to_string()
        } else if dir_path.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", dir_path, name)
        };
        for c in start..start.saturating_add(count) {
            if c >= ec {
                break;
            }
            walk_entry(r, toc, names, c, &me, depth, info, guard)?;
        }
        return Ok(());
    }
    let full = if dir_path.is_empty() { name.clone() } else { format!("{}/{}", dir_path, name) };
    let unc = u32le(&e[8..12]) as u64;
    let lname = name.to_lowercase();
    info.tree.push(full.clone());
    if let Some(m) = model_of(&lname) {
        if !info.weapon_models.contains(&m) {
            info.weapon_models.push(m);
        }
    }
    let data_off = (off & 0x7f_ffff) * 512;
    if lname.ends_with(".rpf") && size == 0 {
        let nested = Reader { f: r.f, base: r.base + data_off };
        walk(&nested, unc, &full, depth + 1, info)?;
    } else if matches!(lname.as_str(), "assembly.xml" | "content.xml" | "setup2.xml") {
        let len = if size == 0 { unc } else { size };
        if len == 0 || len > 2_000_000 {
            return Ok(());
        }
        let raw = r.read_at(data_off, len as usize)?;
        let data = if size != 0 {
            let mut out = Vec::new();
            let _ = DeflateDecoder::new(&raw[..]).take(4_000_000).read_to_end(&mut out);
            out
        } else {
            raw
        };
        let text = String::from_utf8_lossy(&data).to_string();
        info.device_names.extend(between_all(&text, "<deviceName>", "</deviceName>"));
        info.assembly_targets.extend(add_targets(&text));
    }
    Ok(())
}

fn model_of(lname: &str) -> Option<String> {
    if !lname.starts_with("w_") || lname.starts_with("w_at_") {
        return None;
    }
    let s = lname.strip_suffix(".ydr")?;
    let s = s.strip_suffix("_hi").unwrap_or(s);
    let s = match s.find("_mag") {
        Some(p) => &s[..p],
        None => s,
    };
    Some(s.to_string())
}

fn between_all(text: &str, open: &str, close: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find(open) {
        let after = &rest[a + open.len()..];
        if let Some(b) = after.find(close) {
            out.push(after[..b].trim().to_string());
            rest = &after[b + close.len()..];
        } else {
            break;
        }
    }
    out
}

fn add_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find("<add ") {
        let after = &rest[a..];
        if let (Some(gt), Some(end)) = (after.find('>'), after.find("</add>")) {
            if gt < end {
                out.push(after[gt + 1..end].trim().replace('\\', "/").to_lowercase());
            }
            rest = &after[end + 6..];
        } else {
            break;
        }
    }
    out
}
