//! Explorador de archivos acotado a las carpetas configuradas de FiveM / GTA V.
//!
//! Reglas:
//! * Solo se opera dentro de las «áreas» de cada sección; cualquier ruta se valida con `Roots::resolve`
//!   (sin `..`, sin unidades, sin enlaces simbólicos) y debe quedar dentro del área.
//! * Las áreas de solo lectura (`citizen`, audio de GTA V) se pueden ver y abrir, no modificar.
//! * Los archivos que pertenecen a una instalación de 444CFG TOOL no se tocan aquí: se retiran desde su instalación.
//! * Antes de sustituir o eliminar un archivo se guarda su copia por hash y se anota en `fm_log.json`
//!   para poder restaurarlo.
//! * Las carpetas solo se eliminan si están vacías.
use crate::exec::write_from_blob;
use crate::model::*;
use crate::pathsafe::sanitize_rel;
use crate::store::*;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Extensiones que nunca se importan desde el explorador (ejecutables y scripts).
const BLOCKED_EXT: &[&str] = &[
    "exe", "dll", "bat", "cmd", "ps1", "vbs", "vbe", "js", "jse", "msi", "lnk", "scr", "com", "jar", "sh", "reg", "hta",
];

#[derive(Clone, Debug, Serialize)]
pub struct Area {
    pub id: String,
    pub label: String,
    pub root: Root,
    pub rel: String,
    pub writable: bool,
    pub exists: bool,
}

const AREAS: &[(&str, Root, &str, &str, bool)] = &[
    ("mods", Root::Fivem, "mods", "Carpeta mods", true),
    ("citizen", Root::Fivem, "citizen", "Carpeta citizen (solo lectura)", false),
    ("effects", Root::Fivem, "citizen/common/data/effects", "Efectos · bloodfx.dat", true),
    ("timecycle", Root::Fivem, "citizen/common/data/timecycle", "Timecycle · Tracers XML", true),
    ("gta_sfx", Root::Gta, "x64/audio/sfx", "GTA V · audio sfx (solo lectura)", false),
];

fn area_ids(section: Category) -> &'static [&'static str] {
    match section {
        Category::Citizen => &["mods", "citizen"],
        Category::Mod | Category::Weapon => &["mods"],
        Category::BloodFx => &["effects", "mods"],
        Category::Tracer => &["timecycle", "mods"],
        Category::SoundPack => &["gta_sfx", "mods"],
    }
}

fn build_area(id: &str, roots: &Roots) -> Option<Area> {
    let (aid, root, rel, label, writable) = AREAS.iter().find(|a| a.0 == id)?;
    let base = roots.base(*root).ok()?;
    let exists = base.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)).is_dir();
    Some(Area { id: aid.to_string(), label: label.to_string(), root: *root, rel: rel.to_string(), writable: *writable, exists })
}

/// Áreas disponibles en una sección (las de GTA V solo si la ruta está configurada).
pub fn areas(section: Category, roots: &Roots) -> Vec<Area> {
    area_ids(section).iter().filter_map(|id| build_area(id, roots)).collect()
}

pub fn find_area(section: Category, roots: &Roots, id: &str) -> Result<Area> {
    if !area_ids(section).iter().any(|x| *x == id) {
        bail!("carpeta no disponible en esta sección");
    }
    build_area(id, roots).ok_or_else(|| anyhow!("la carpeta no está configurada (revisa las rutas en Ajustes)"))
}

/// Ruta completa (relativa a la raíz) de `sub` dentro del área, validada.
fn full_rel(area: &Area, sub: &str) -> Result<String> {
    if sub.trim().is_empty() {
        return Ok(area.rel.clone());
    }
    let s = sanitize_rel(sub)?;
    Ok(format!("{}/{}", area.rel, s))
}

fn is_temp_name(n: &str) -> bool {
    n.starts_with(".444tmp-") || n.starts_with(".444perm-")
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub name: String,
    /// Ruta relativa al área (la que debe devolver la interfaz en las operaciones).
    pub sub: String,
    pub is_dir: bool,
    pub size: u64,
    pub ext: String,
    pub modified: Option<u64>,
    /// "managed" (instalado por 444CFG) · "managed_dir" · "external" (ya existía / no gestionado) · "folder" · "link"
    pub status: String,
    pub owner: Option<String>,
    pub note: Option<String>,
    pub can_modify: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Listing {
    pub area: Area,
    pub dir: String,
    pub entries: Vec<Entry>,
}

fn managed_owner(state: &State, root: Root, rel: &str) -> Option<(String, bool)> {
    let rec = state.records.get(&fkey(root, rel))?;
    let top = rec.layers.last()?;
    let name = state.installs.iter().find(|i| i.id == top.install_id).map(|i| i.name.clone()).unwrap_or_else(|| "444CFG TOOL".to_string());
    Some((name, rec.original.is_some()))
}

fn has_managed_below(state: &State, root: Root, rel: &str) -> bool {
    let prefix = format!("{}/", fkey(root, rel));
    state.records.iter().any(|(k, r)| k.starts_with(&prefix) && !r.layers.is_empty())
}

/// Lista una carpeta del área con el estado de cada entrada. No calcula hashes (no recorre archivos grandes).
pub fn list(state: &State, roots: &Roots, area: &Area, sub: &str) -> Result<Listing> {
    let dir_rel = full_rel(area, sub)?;
    let dir = roots.resolve(area.root, &dir_rel)?;
    let mut entries: Vec<Entry> = Vec::new();
    let rd = match fs::read_dir(&dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Listing { area: area.clone(), dir: sub.to_string(), entries });
        }
        Err(e) => bail!("no se puede leer la carpeta: {}", e),
    };
    let clean_sub = if sub.trim().is_empty() { String::new() } else { sanitize_rel(sub)? };
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        if is_temp_name(&name) {
            continue;
        }
        let ft = match ent.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let md = ent.metadata().ok();
        let size = if ft.is_dir() { 0 } else { md.as_ref().map(|m| m.len()).unwrap_or(0) };
        let modified = md
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let child_sub = if clean_sub.is_empty() { name.clone() } else { format!("{}/{}", clean_sub, name) };
        let child_rel = format!("{}/{}", dir_rel, name);
        let ext = if ft.is_dir() { String::new() } else { name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default() };
        let (status, owner, note) = if ft.is_symlink() {
            ("link".to_string(), None, Some("Enlace simbólico: no se modifica".to_string()))
        } else if ft.is_dir() {
            if has_managed_below(state, area.root, &child_rel) {
                ("managed_dir".to_string(), None, Some("Contiene archivos instalados por 444CFG TOOL".to_string()))
            } else {
                ("folder".to_string(), None, None)
            }
        } else {
            match managed_owner(state, area.root, &child_rel) {
                Some((n, had_original)) => (
                    "managed".to_string(),
                    Some(n),
                    if had_original { Some("Sustituyó un archivo que ya existía (hay copia para restaurarlo)".to_string()) } else { None },
                ),
                None => ("external".to_string(), None, Some("Ya existía o se añadió fuera de 444CFG TOOL".to_string())),
            }
        };
        let can_modify = area.writable && (status == "external" || status == "folder");
        entries.push(Entry { name, sub: child_sub, is_dir: ft.is_dir(), size, ext, modified, status, owner, note, can_modify });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing { area: area.clone(), dir: clean_sub, entries })
}

/// Ruta real (existente) para «abrir ubicación».
pub fn open_target(roots: &Roots, area: &Area, sub: &str) -> Result<PathBuf> {
    let rel = full_rel(area, sub)?;
    let p = roots.resolve(area.root, &rel)?;
    if !p.exists() {
        bail!("la ruta ya no existe");
    }
    Ok(p)
}

// ---------- registro de copias del explorador ----------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FmEntry {
    pub id: String,
    pub ts: u64,
    /// "delete" · "replace" · "add"
    pub action: String,
    pub root: Root,
    pub rel: String,
    pub old_sha: Option<String>,
    pub new_sha: Option<String>,
}

fn log_path(store: &Store) -> PathBuf {
    store.dir.join("fm_log.json")
}

fn load_log(store: &Store) -> Vec<FmEntry> {
    fs::read(log_path(store)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_log(store: &Store, v: &[FmEntry]) -> Result<()> {
    let keep = v.len().saturating_sub(300);
    atomic_write(&log_path(store), &serde_json::to_vec_pretty(&v[keep..])?)
}

/// Copias y cambios hechos desde el explorador, la más reciente primero.
pub fn backups(store: &Store) -> Vec<FmEntry> {
    let mut v = load_log(store);
    v.reverse();
    v
}

fn push_log(store: &Store, e: FmEntry) -> Result<()> {
    let mut v = load_log(store);
    v.push(e);
    save_log(store, &v)
}

// ---------- operaciones ----------

fn check_writable_area(area: &Area) -> Result<()> {
    if !area.writable {
        bail!("«{}» es de solo lectura: aquí no se pueden modificar archivos (usa la instalación de la sección correspondiente)", area.label);
    }
    Ok(())
}

fn ensure_unmanaged(state: &State, root: Root, rel: &str) -> Result<()> {
    if let Some((n, _)) = managed_owner(state, root, rel) {
        bail!("«{}» pertenece a la instalación «{}» de 444CFG TOOL: retírala desde su sección o desde «Mis instalaciones»", rel, n);
    }
    if has_managed_below(state, root, rel) {
        bail!("«{}» contiene archivos instalados por 444CFG TOOL: retira primero esas instalaciones", rel);
    }
    Ok(())
}

/// Copia `src` a `dest` de forma atómica y devuelve (sha del contenido anterior si lo había, sha nuevo).
fn put_into(store: &Store, src: &Path, dest: &Path, on: &mut dyn FnMut(u64)) -> Result<(Option<String>, String)> {
    let old = match hash_path(dest)? {
        Some(_) => Some(store.put_file(dest)?.0),
        None => None,
    };
    let dir = dest.parent().ok_or_else(|| anyhow!("ruta sin carpeta padre"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".444tmp-{}", uniq()));
    let sha = match copy_hashing(src, &tmp, on) {
        Ok(s) => s,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };
    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(anyhow!(e));
    }
    if hash_path(dest)?.as_deref() != Some(sha.as_str()) {
        bail!("la verificación del archivo copiado falló: {}", dest.display());
    }
    Ok((old, sha))
}

fn copy_hashing(src: &Path, dst: &Path, on: &mut dyn FnMut(u64)) -> Result<String> {
    let mut r = File::open(src)?;
    let mut w = File::create(dst)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        w.write_all(&buf[..n])?;
        on(n as u64);
    }
    w.sync_all()?;
    Ok(format!("{:x}", h.finalize()))
}

fn check_import_name(src: &Path) -> Result<String> {
    if !src.is_file() {
        bail!("«{}» no es un archivo", src.display());
    }
    let name = src.file_name().and_then(|s| s.to_str()).ok_or_else(|| anyhow!("nombre de archivo no válido"))?.to_string();
    sanitize_rel(&name)?;
    if name.contains('/') || name.contains('\\') {
        bail!("nombre de archivo no válido: {}", name);
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    if BLOCKED_EXT.contains(&ext.as_str()) {
        bail!("«{}»: los archivos .{} no se pueden importar desde el explorador", name, ext);
    }
    if is_temp_name(&name) {
        bail!("nombre reservado: {}", name);
    }
    Ok(name)
}

/// Añade archivos a una carpeta del área. Con `replace` sustituye los que ya existen (con copia previa).
/// `on(hechos, total)` informa de los bytes copiados. Devuelve las rutas escritas (relativas al área).
pub fn import_files(
    store: &Store,
    state: &State,
    roots: &Roots,
    area: &Area,
    sub: &str,
    sources: &[PathBuf],
    replace: bool,
    on: &mut dyn FnMut(u64, u64),
) -> Result<Vec<String>> {
    check_writable_area(area)?;
    if sources.is_empty() {
        bail!("no se ha seleccionado ningún archivo");
    }
    let dir_rel = full_rel(area, sub)?;
    let dir = roots.resolve(area.root, &dir_rel)?;
    if !dir.is_dir() {
        bail!("la carpeta de destino no existe");
    }
    let clean_sub = if sub.trim().is_empty() { String::new() } else { sanitize_rel(sub)? };
    // Validación completa antes de escribir nada.
    let mut plan: Vec<(PathBuf, PathBuf, String, String, bool)> = Vec::new(); // (origen, destino, rel, sub, existe)
    let mut total = 0u64;
    for src in sources {
        let name = check_import_name(src)?;
        let rel = format!("{}/{}", dir_rel, name);
        let dest = roots.resolve(area.root, &rel)?;
        ensure_unmanaged(state, area.root, &rel)?;
        let exists = match fs::symlink_metadata(&dest) {
            Ok(m) => {
                if m.is_dir() {
                    bail!("«{}» es una carpeta; no se puede sustituir por un archivo", name);
                }
                if !replace {
                    bail!("«{}» ya existe en esa carpeta: usa «Sustituir» para reemplazarlo (se guarda copia)", name);
                }
                true
            }
            Err(_) => false,
        };
        total += fs::metadata(src).map(|m| m.len()).unwrap_or(0);
        let child_sub = if clean_sub.is_empty() { name.clone() } else { format!("{}/{}", clean_sub, name) };
        plan.push((src.clone(), dest, rel, child_sub, exists));
    }
    let mut done = 0u64;
    let mut written: Vec<String> = Vec::new();
    for (src, dest, rel, child_sub, _exists) in plan {
        let mut base = done;
        let (old, new) = {
            let mut cb = |n: u64| {
                base += n;
                on(base, total);
            };
            put_into(store, &src, &dest, &mut cb)?
        };
        done = base;
        push_log(
            store,
            FmEntry {
                id: uniq(),
                ts: now_secs(),
                action: if old.is_some() { "replace".into() } else { "add".into() },
                root: area.root,
                rel,
                old_sha: old,
                new_sha: Some(new),
            },
        )?;
        written.push(child_sub);
    }
    Ok(written)
}

/// Sustituye el archivo `sub` (sin importar su nombre de origen) por el contenido de `source`.
pub fn replace_file(
    store: &Store,
    state: &State,
    roots: &Roots,
    area: &Area,
    sub: &str,
    source: &Path,
    on: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    check_writable_area(area)?;
    if sub.trim().is_empty() {
        bail!("no se ha indicado el archivo que se sustituye");
    }
    let rel = full_rel(area, sub)?;
    let dest = roots.resolve(area.root, &rel)?;
    ensure_unmanaged(state, area.root, &rel)?;
    match fs::symlink_metadata(&dest) {
        Ok(m) if m.is_file() => {}
        Ok(_) => bail!("solo se pueden sustituir archivos"),
        Err(_) => bail!("el archivo que se quiere sustituir ya no existe"),
    }
    // El origen se valida como archivo normal (el nombre del origen no importa, el del destino se conserva).
    if !source.is_file() {
        bail!("«{}» no es un archivo", source.display());
    }
    let sname = source.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let sext = sname.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    if BLOCKED_EXT.contains(&sext.as_str()) {
        bail!("los archivos .{} no se pueden importar desde el explorador", sext);
    }
    let total = fs::metadata(source).map(|m| m.len()).unwrap_or(0);
    let mut done = 0u64;
    let (old, new) = {
        let mut cb = |n: u64| {
            done += n;
            on(done, total);
        };
        put_into(store, source, &dest, &mut cb)?
    };
    push_log(store, FmEntry { id: uniq(), ts: now_secs(), action: "replace".into(), root: area.root, rel, old_sha: old, new_sha: Some(new) })?;
    Ok(())
}

/// Elimina archivos (con copia previa) y carpetas vacías. Todo o nada en la validación.
pub fn delete_items(store: &Store, state: &State, roots: &Roots, area: &Area, subs: &[String]) -> Result<Vec<String>> {
    check_writable_area(area)?;
    if subs.is_empty() {
        bail!("no hay nada seleccionado");
    }
    let mut targets: Vec<(PathBuf, String, bool)> = Vec::new(); // (ruta, rel, es_carpeta)
    for sub in subs {
        if sub.trim().is_empty() {
            bail!("no se puede eliminar la carpeta raíz del área");
        }
        let rel = full_rel(area, sub)?;
        let path = roots.resolve(area.root, &rel)?;
        let md = fs::symlink_metadata(&path).map_err(|_| anyhow!("«{}» ya no existe", sub))?;
        if md.file_type().is_symlink() {
            bail!("«{}» es un enlace simbólico: no se elimina", sub);
        }
        ensure_unmanaged(state, area.root, &rel)?;
        if md.is_dir() {
            let empty = fs::read_dir(&path)?.next().is_none();
            if !empty {
                bail!("la carpeta «{}» no está vacía: elimina primero su contenido", sub);
            }
            targets.push((path, rel, true));
        } else {
            targets.push((path, rel, false));
        }
    }
    let mut removed = Vec::new();
    for (path, rel, is_dir) in targets {
        if is_dir {
            fs::remove_dir(&path)?;
        } else {
            let (sha, _) = store.put_file(&path)?;
            fs::remove_file(&path)?;
            push_log(store, FmEntry { id: uniq(), ts: now_secs(), action: "delete".into(), root: area.root, rel: rel.clone(), old_sha: Some(sha), new_sha: None })?;
        }
        removed.push(rel);
    }
    Ok(removed)
}

/// Deshace una entrada del registro (devuelve el archivo eliminado o sustituido, o quita el añadido).
pub fn restore(store: &Store, state: &State, roots: &Roots, id: &str) -> Result<String> {
    let mut log = load_log(store);
    let idx = log.iter().position(|e| e.id == id).ok_or_else(|| anyhow!("la copia ya no figura en el registro"))?;
    let e = log[idx].clone();
    let path = roots.resolve(e.root, &e.rel)?;
    if managed_owner(state, e.root, &e.rel).is_some() {
        bail!("«{}» ahora pertenece a una instalación de 444CFG TOOL; no se restaura desde aquí", e.rel);
    }
    let cur = hash_path(&path)?;
    match e.action.as_str() {
        "delete" => {
            if cur.is_some() {
                bail!("«{}» ya existe de nuevo; no se sobrescribe", e.rel);
            }
            let sha = e.old_sha.clone().ok_or_else(|| anyhow!("registro dañado"))?;
            write_from_blob(store, &path, &sha, &mut |_| {})?;
        }
        "replace" => {
            if cur != e.new_sha {
                bail!("«{}» cambió después de sustituirse; no se restaura automáticamente", e.rel);
            }
            let sha = e.old_sha.clone().ok_or_else(|| anyhow!("registro dañado"))?;
            write_from_blob(store, &path, &sha, &mut |_| {})?;
        }
        "add" => {
            if cur.is_none() {
                bail!("«{}» ya no existe", e.rel);
            }
            if cur != e.new_sha {
                bail!("«{}» cambió después de añadirse; no se elimina", e.rel);
            }
            fs::remove_file(&path)?;
        }
        _ => bail!("acción desconocida en el registro"),
    }
    log.remove(idx);
    save_log(store, &log)?;
    Ok(e.rel)
}
