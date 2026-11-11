use anyhow::{bail, Result};
use std::path::Path;

const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Valida y normaliza una ruta relativa (separadores '/'): rechaza '..', rutas
/// absolutas, unidades, nombres reservados de Windows y caracteres no válidos.
pub fn sanitize_rel(raw: &str) -> Result<String> {
    if raw.is_empty() || raw.contains('\0') {
        bail!("ruta vacía o con caracteres nulos");
    }
    let norm = raw.replace('\\', "/");
    if norm.starts_with('/') || norm.contains(':') {
        bail!("ruta absoluta o con unidad no permitida: {}", raw);
    }
    let mut parts: Vec<&str> = Vec::new();
    for p in norm.split('/') {
        if p.is_empty() || p == "." {
            continue;
        }
        if p == ".." {
            bail!("ruta con '..' rechazada (ZIP Slip): {}", raw);
        }
        if p.ends_with(' ') || p.ends_with('.') {
            bail!("segmento no válido en Windows: {}", raw);
        }
        if p.chars().any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*') || (c as u32) < 32) {
            bail!("caracteres no válidos en la ruta: {}", raw);
        }
        let stem = p.split('.').next().unwrap_or("").to_lowercase();
        if RESERVED.contains(&stem.as_str()) {
            bail!("nombre reservado de Windows en la ruta: {}", raw);
        }
        parts.push(p);
    }
    if parts.is_empty() {
        bail!("ruta vacía");
    }
    Ok(parts.join("/"))
}

/// Rechaza cualquier componente existente que sea un enlace simbólico.
pub fn no_symlinks(base: &Path, full: &Path) -> Result<()> {
    let rel = full.strip_prefix(base)?;
    let mut cur = base.to_path_buf();
    for c in rel.components() {
        cur.push(c);
        if let Ok(m) = std::fs::symlink_metadata(&cur) {
            if m.file_type().is_symlink() {
                bail!("enlace simbólico no permitido: {}", cur.display());
            }
        }
    }
    Ok(())
}
