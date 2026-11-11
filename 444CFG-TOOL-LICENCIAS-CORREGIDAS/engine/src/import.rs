//! Importación y análisis de paquetes. Nada se instala aquí: solo se guarda el contenido
//! en la biblioteca (por hash) y se calcula a qué destinos iría cada archivo.
use crate::model::*;
use crate::pathsafe::sanitize_rel;
use crate::rpf;
use crate::store::*;
use crate::weapon;
use anyhow::{anyhow, bail, Context, Result};
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const MAX_TOTAL: u64 = 8 << 30;

fn base_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn ignorable(base: &str) -> bool {
    let b = base.to_lowercase();
    b.starts_with("readme")
        || b.starts_with("license")
        || b.ends_with(".txt")
        || b.ends_with(".url")
        || b.ends_with(".png")
        || b.ends_with(".jpg")
        || b.ends_with(".jpeg")
        || b == "thumbs.db"
        || b == ".ds_store"
}

/// Ok(Some) = destino; Ok(None) = archivo ignorable; Err = motivo de incompatibilidad.
fn map_entry(cat: Category, rel: &str) -> std::result::Result<Option<(Root, String)>, String> {
    let lower = rel.to_lowercase();
    let base = base_name(rel);
    let lbase = base.to_lowercase();
    let is_rpf = lbase.ends_with(".rpf");
    let bad = |msg: &str| Err(format!("{}: {}", rel, msg));
    match cat {
        Category::Citizen => {
            if lower.starts_with("citizen/") || lower.starts_with("mods/") {
                Ok(Some((Root::Fivem, rel.to_string())))
            } else if is_rpf {
                // Un RPF suelto o dentro de otra carpeta del paquete es un mod del Citizen: va a «mods».
                Ok(Some((Root::Fivem, format!("mods/{}", base))))
            } else if ignorable(base) {
                Ok(None)
            } else {
                bad("no se instala: un Citizen solo admite archivos dentro de «citizen/» o «mods/» y archivos RPF (que van a «mods/»)")
            }
        }
        Category::Mod => {
            if is_rpf {
                Ok(Some((Root::Fivem, format!("mods/{}", base))))
            } else if lower.starts_with("mods/") {
                Ok(Some((Root::Fivem, rel.to_string())))
            } else if ignorable(base) {
                Ok(None)
            } else {
                bad("un mod solo puede contener RPF o archivos dentro de «mods/»")
            }
        }
        Category::Weapon => {
            if is_rpf {
                Ok(Some((Root::Fivem, format!("mods/{}", base))))
            } else if ignorable(base) {
                Ok(None)
            } else {
                bad("una skin de arma debe ser un RPF")
            }
        }
        Category::BloodFx => {
            if lbase == "bloodfx.dat" {
                Ok(Some((Root::Fivem, "citizen/common/data/effects/bloodfx.dat".into())))
            } else if is_rpf {
                Ok(Some((Root::Fivem, format!("mods/{}", base))))
            } else if ignorable(base) {
                Ok(None)
            } else {
                bad("Blood FX solo admite bloodfx.dat y RPF")
            }
        }
        Category::Tracer => {
            if lbase.ends_with(".xml") {
                Ok(Some((Root::Fivem, format!("citizen/common/data/timecycle/{}", base))))
            } else if is_rpf {
                Ok(Some((Root::Fivem, format!("mods/{}", base))))
            } else if ignorable(base) {
                Ok(None)
            } else {
                bad("Tracers admite XML de timecycle y RPF")
            }
        }
        Category::SoundPack => {
            if lbase == "resident.rpf" {
                Ok(Some((Root::Gta, "x64/audio/sfx/RESIDENT.rpf".into())))
            } else if lbase == "weapons_player.rpf" {
                Ok(Some((Root::Gta, "x64/audio/sfx/WEAPONS_PLAYER.rpf".into())))
            } else if ignorable(base) {
                Ok(None)
            } else {
                bad("no se puede instalar directamente: solo RESIDENT.rpf y WEAPONS_PLAYER.rpf; los .awc/.rel sueltos deben ir dentro de un RPF")
            }
        }
    }
}

struct Item {
    src: usize,
    zip_idx: Option<usize>,
    root: Root,
    dest: String,
    size: u64,
}

struct Raw {
    src: usize,
    zip_idx: Option<usize>,
    path: String,
    size: u64,
}

fn is_zip(p: &Path) -> Result<bool> {
    let mut f = File::open(p).with_context(|| format!("no se puede abrir {}", p.display()))?;
    let mut m = [0u8; 4];
    let n = f.read(&mut m)?;
    if n == 4 && &m == b"Rar!" {
        bail!("{}: los RAR no se pueden leer; conviértelo a ZIP", p.display());
    }
    Ok(n >= 4 && &m[0..2] == b"PK")
}

/// Número de carpetas «envoltorio» que hay que quitar para que `citizen/` quede en la raíz del paquete.
fn citizen_prefix(paths: &[&str]) -> usize {
    let mut best: Option<usize> = None;
    for p in paths {
        let segs: Vec<&str> = p.split('/').collect();
        for (k, s) in segs.iter().enumerate() {
            if k + 1 < segs.len() && s.eq_ignore_ascii_case("citizen") {
                best = Some(best.map_or(k, |b| b.min(k)));
                break;
            }
        }
    }
    best.unwrap_or(0)
}

fn rebase(path: &str, k: usize) -> String {
    if k == 0 {
        return path.to_string();
    }
    let segs: Vec<&str> = path.split('/').collect();
    if segs.len() > k + 1 && segs[k].eq_ignore_ascii_case("citizen") {
        segs[k..].join("/")
    } else {
        path.to_string()
    }
}

/// Lector que informa del avance real (bytes leídos del paquete).
struct PReader<'a, R: Read> {
    inner: R,
    done: &'a Cell<u64>,
    total: u64,
    cb: Option<&'a dyn Fn(u64, u64)>,
}

impl<'a, R: Read> Read for PReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.done.set(self.done.get() + n as u64);
        if let Some(cb) = self.cb {
            cb(self.done.get(), self.total);
        }
        Ok(n)
    }
}

pub fn analyze(store: &Store, cat: Category, sources: &[PathBuf], name: Option<String>) -> Result<Package> {
    analyze_with(store, cat, sources, name, None)
}

pub fn analyze_with(
    store: &Store,
    cat: Category,
    sources: &[PathBuf],
    name: Option<String>,
    progress: Option<&dyn Fn(u64, u64)>,
) -> Result<Package> {
    if sources.is_empty() {
        bail!("no se ha seleccionado ningún archivo");
    }
    let lenient = matches!(cat, Category::Citizen | Category::SoundPack);
    let mut raws: Vec<Raw> = Vec::new();
    // Pasada 1: validar rutas y leer el índice, sin guardar nada.
    for (si, src) in sources.iter().enumerate() {
        if is_zip(src)? {
            let mut a = ZipArchive::new(BufReader::new(File::open(src)?))
                .with_context(|| format!("ZIP no válido: {}", src.display()))?;
            for i in 0..a.len() {
                let f = a.by_index(i).map_err(|e| {
                    anyhow!(
                        "no se puede leer la entrada {} de «{}» ({}). Si usa una compresión poco común, vuelve a comprimirlo como ZIP estándar",
                        i,
                        src.display(),
                        e
                    )
                })?;
                let raw = f.name().to_string();
                let clean = sanitize_rel(raw.trim_end_matches('/'))
                    .map_err(|e| anyhow!("ZIP rechazado, ruta peligrosa «{}»: {}", raw, e))?;
                if f.is_dir() {
                    continue;
                }
                if f.encrypted() {
                    bail!("ZIP rechazado: «{}» está cifrado", raw);
                }
                if let Some(mode) = f.unix_mode() {
                    if mode & 0o170000 == 0o120000 {
                        bail!("ZIP rechazado: «{}» es un enlace simbólico", raw);
                    }
                }
                raws.push(Raw { src: si, zip_idx: Some(i), path: clean, size: f.size() });
            }
        } else {
            let fname = src
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow!("nombre de archivo no válido"))?
                .to_string();
            let clean = sanitize_rel(&fname)?;
            let size = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
            raws.push(Raw { src: si, zip_idx: None, path: clean, size });
        }
    }
    // Estructura real del paquete: si «citizen/» está dentro de una carpeta envoltorio, se descarta el envoltorio.
    let mut strip: HashMap<usize, usize> = HashMap::new();
    if cat == Category::Citizen {
        let mut by_src: HashMap<usize, Vec<&str>> = HashMap::new();
        for r in &raws {
            if r.zip_idx.is_some() {
                by_src.entry(r.src).or_default().push(r.path.as_str());
            }
        }
        for (s, ps) in by_src {
            strip.insert(s, citizen_prefix(&ps));
        }
    }
    let mut items: Vec<Item> = Vec::new();
    let mut rejects: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut ignored = 0usize;
    for r in &raws {
        let path = rebase(&r.path, strip.get(&r.src).copied().unwrap_or(0));
        match map_entry(cat, &path) {
            Ok(Some((root, dest))) => items.push(Item { src: r.src, zip_idx: r.zip_idx, root, dest, size: r.size }),
            Ok(None) => ignored += 1,
            Err(m) => {
                if lenient {
                    skipped.push(m);
                } else {
                    rejects.push(m);
                }
            }
        }
    }
    if !rejects.is_empty() {
        let shown: Vec<String> = rejects.iter().take(12).cloned().collect();
        bail!("Paquete incompatible ({} archivos):\n{}", rejects.len(), shown.join("\n"));
    }
    if items.is_empty() {
        if skipped.is_empty() {
            bail!("el paquete no contiene archivos instalables para esta categoría");
        }
        let shown: Vec<String> = skipped.iter().take(12).cloned().collect();
        bail!("el paquete no contiene archivos instalables para esta categoría:\n{}", shown.join("\n"));
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for it in &items {
        sanitize_rel(&it.dest)?;
        if !seen.insert(fkey(it.root, &it.dest)) {
            bail!("dos archivos del paquete tienen el mismo destino: {}", it.dest);
        }
    }
    // Pasada 2: guardar contenido por hash (con avance real).
    let mut budget = MAX_TOTAL;
    let total: u64 = items.iter().map(|i| i.size).sum();
    let done = Cell::new(0u64);
    let mut zips: HashMap<usize, ZipArchive<BufReader<File>>> = HashMap::new();
    let mut files: Vec<FileSpec> = Vec::new();
    for it in &items {
        let (sha, size) = match it.zip_idx {
            Some(zi) => {
                if !zips.contains_key(&it.src) {
                    zips.insert(it.src, ZipArchive::new(BufReader::new(File::open(&sources[it.src])?))?);
                }
                let a = zips.get_mut(&it.src).unwrap();
                let mut f = a.by_index(zi)?;
                let mut pr = PReader { inner: &mut f, done: &done, total, cb: progress };
                store.put_reader(&mut pr, &mut budget)?
            }
            None => {
                let f = File::open(&sources[it.src])?;
                let mut pr = PReader { inner: f, done: &done, total, cb: progress };
                store.put_reader(&mut pr, &mut budget)?
            }
        };
        files.push(FileSpec { root: it.root, rel: it.dest.clone(), sha256: sha, size });
    }
    // Análisis de RPF.
    let mut warnings: Vec<String> = Vec::new();
    let mut mount_ids: Vec<String> = Vec::new();
    let mut contents: Vec<String> = Vec::new();
    let mut models: Vec<String> = Vec::new();
    let mut rpf_names: Vec<String> = Vec::new();
    let mut all_tree: Vec<String> = Vec::new();
    let mut has_rpf = false;
    for s in &skipped {
        warnings.push(format!("No se instalará: {}", s));
    }
    for f in &files {
        contents.push(format!("{} → {}", base_name(&f.rel), f.rel));
        if !f.rel.to_lowercase().ends_with(".rpf") {
            continue;
        }
        has_rpf = true;
        rpf_names.push(base_name(&f.rel).to_string());
        // Un RPF que viaja con un Citizen (fuera de citizen/) solo se acepta si es un RPF7 real.
        let citizen_rpf = cat == Category::Citizen && !f.rel.to_lowercase().starts_with("citizen/");
        let info = match rpf::inspect(&store.blob_path(&f.sha256)?) {
            Ok(i) => i,
            Err(e) => {
                if citizen_rpf {
                    bail!("{}: no es un RPF válido ({}); no se instalará", base_name(&f.rel), e);
                }
                warnings.push(format!("{}: no se pudo analizar el RPF ({})", base_name(&f.rel), e));
                continue;
            }
        };
        if !info.valid {
            if citizen_rpf {
                bail!("{}: no tiene cabecera RPF7 válida; no se instalará", base_name(&f.rel));
            }
            warnings.push(format!("{} no tiene cabecera RPF7 válida", base_name(&f.rel)));
        }
        if info.toc_encrypted {
            warnings.push(format!(
                "{}: índice cifrado; su contenido no es inspeccionable y se trata como archivo completo (sin vista previa de audio)",
                base_name(&f.rel)
            ));
        }
        for n in &info.notes {
            if !info.toc_encrypted {
                warnings.push(n.clone());
            }
        }
        for d in &info.device_names {
            let id = format!("device:{}", d.to_lowercase());
            if !mount_ids.contains(&id) {
                mount_ids.push(id);
            }
        }
        for t in &info.assembly_targets {
            let id = format!("dlc:{}", t);
            if !mount_ids.contains(&id) {
                mount_ids.push(id);
            }
        }
        for m in &info.weapon_models {
            if !models.contains(m) {
                models.push(m.clone());
            }
            all_tree.push(format!("{}.ydr", m));
        }
        all_tree.extend(info.tree.iter().cloned());
        for t in info.tree.iter().take(120) {
            contents.push(format!("   {}: {}", base_name(&f.rel), t));
        }
    }
    contents.truncate(400);
    models.sort();
    let mut weapon_kind: Option<String> = None;
    let mut weapon_evidence: Vec<String> = Vec::new();
    let slot = match cat {
        Category::Citizen => Some("citizen".to_string()),
        Category::BloodFx => Some("bloodfx".to_string()),
        Category::Tracer => Some("tracer".to_string()),
        Category::SoundPack => Some("soundpack".to_string()),
        Category::Mod => None,
        Category::Weapon => {
            let det = weapon::detect(&rpf_names, &all_tree);
            weapon_kind = det.kind.clone();
            weapon_evidence = det.evidence;
            match &weapon_kind {
                Some(k) => Some(format!("weapon:{}", k)),
                None => {
                    if models.is_empty() {
                        warnings.push("no se pudo identificar el modelo del arma; se sustituirá solo un paquete con el mismo nombre de archivo".into());
                        let first = files.first().map(|f| base_name(&f.rel).to_lowercase()).unwrap_or_default();
                        Some(format!("weapon:file:{}", first))
                    } else {
                        Some(format!("weapon:{}", models.join("+")))
                    }
                }
            }
        }
    };
    if cat == Category::Weapon && weapon_kind.is_none() {
        warnings.push("No se pudo decidir si es una skin de MK2 o de 9mm: elige el arma manualmente en los detalles del paquete (o se tratará como un arma distinta)".into());
    }
    if cat == Category::BloodFx && !files.iter().any(|f| f.rel.ends_with("bloodfx.dat")) {
        warnings.push("este Blood FX no incluye bloodfx.dat; solo se instalarán los RPF".into());
    }
    if cat == Category::SoundPack && files.len() == 1 {
        warnings.push("el Sound Pack solo incluye uno de los dos RPF (RESIDENT / WEAPONS_PLAYER)".into());
    }
    let needs_ack = cat == Category::Tracer && has_rpf;
    if needs_ack {
        warnings.push("RPF de Tracer: el destino (mods) debe verificarse para este paquete; se pedirá confirmación expresa".into());
    }
    if ignored > 0 {
        warnings.push(format!("{} archivos informativos (readme, imágenes) se han ignorado", ignored));
    }
    let mut lines: Vec<String> = files
        .iter()
        .map(|f| format!("{:?}|{}|{}", f.root, f.rel.to_lowercase(), f.sha256))
        .collect();
    lines.sort();
    let id = sha256_bytes(format!("{:?}\n{}", cat, lines.join("\n")).as_bytes());
    let default_name = sources[0]
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("paquete")
        .to_string();
    let pkg = Package {
        id,
        name: name.filter(|n| !n.trim().is_empty()).unwrap_or(default_name),
        category: cat,
        slot,
        files,
        warnings,
        mount_ids,
        contents,
        needs_ack_unverified: needs_ack,
        created_at: now_secs(),
        weapon_kind,
        weapon_evidence,
    };
    store.save_package(&pkg)?;
    Ok(pkg)
}
