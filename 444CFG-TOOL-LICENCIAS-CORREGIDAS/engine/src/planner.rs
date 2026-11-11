//! Planificador: calcula, sin tocar el disco, qué archivos se retiran, restauran e instalan.
//! Trabaja sobre una copia del estado y un disco virtual; el resultado es una lista de
//! operaciones con precondiciones (hash esperado) que el ejecutor valida una a una.
use crate::model::*;
use crate::rpf;
use crate::store::*;
use crate::weapon;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpKind {
    Write { sha: String, expect: Option<String> },
    Delete { expect: String },
    RemoveDir,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Op {
    pub root: Root,
    pub rel: String,
    #[serde(flatten)]
    pub op: OpKind,
    pub why: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Summary {
    /// Archivos del paquete anterior que dejan de estar instalados
    pub retire_old: usize,
    /// Archivos nuevos que se instalan
    pub install_new: usize,
    /// Archivos idénticos que no se tocan
    pub unchanged: usize,
    /// Originales (estado previo) que se restauran
    pub restore_originals: usize,
    /// Archivos de otra capa que vuelven a quedar visibles
    pub reveal_layers: usize,
    /// Archivos de otras instalaciones que se conservan
    pub keep_others: usize,
}

/// Skin instalada a mano (sin manifiesto de 444CFG) que coincide con la que se va a instalar.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManualConflict {
    pub rel: String,
    pub kind: Option<String>,
    pub reason: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub kind: String,
    pub title: String,
    pub package_id: Option<String>,
    pub removes: Vec<String>,
    pub ops: Vec<Op>,
    pub summary: Summary,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub unknown_overwrites: Vec<String>,
    pub needs_ack_unverified: bool,
    /// Motivos concretos por los que hace falta la confirmación expresa (casilla) del usuario.
    pub ack_notes: Vec<String>,
    /// Skins instaladas a mano que se retirarán (con copia) si el usuario confirma.
    pub manual_conflicts: Vec<ManualConflict>,
    pub weapon_kind: Option<String>,
    #[serde(skip)]
    pub state_after: State,
    #[serde(skip)]
    pub new_install: Option<Install>,
}

struct Vdisk<'a> {
    roots: &'a Roots,
    cache: HashMap<String, Option<String>>,
    overlay: HashMap<String, Option<String>>,
    removed_dirs: HashSet<String>,
    made_dirs: HashSet<String>,
}

impl<'a> Vdisk<'a> {
    fn new(roots: &'a Roots) -> Self {
        Vdisk { roots, cache: HashMap::new(), overlay: HashMap::new(), removed_dirs: HashSet::new(), made_dirs: HashSet::new() }
    }
    fn hash(&mut self, root: Root, rel: &str) -> Result<Option<String>> {
        let k = fkey(root, rel);
        if let Some(v) = self.overlay.get(&k) {
            return Ok(v.clone());
        }
        if let Some(v) = self.cache.get(&k) {
            return Ok(v.clone());
        }
        let p = self.roots.resolve(root, rel)?;
        let h = hash_path(&p)?;
        self.cache.insert(k, h.clone());
        Ok(h)
    }
    fn set(&mut self, root: Root, rel: &str, v: Option<String>) {
        self.overlay.insert(fkey(root, rel), v);
    }
    fn dir_exists(&self, root: Root, rel: &str) -> bool {
        let k = fkey(root, rel);
        if self.removed_dirs.contains(&k) {
            return false;
        }
        if self.made_dirs.contains(&k) {
            return true;
        }
        self.roots.resolve(root, rel).map(|p| p.is_dir()).unwrap_or(false)
    }
    fn dir_empty(&self, root: Root, rel: &str) -> bool {
        let prefix = format!("{}/", fkey(root, rel));
        for (k, v) in &self.overlay {
            if k.starts_with(&prefix) && v.is_some() {
                return false;
            }
        }
        let p = match self.roots.resolve(root, rel) {
            Ok(p) => p,
            Err(_) => return false,
        };
        let rd = match std::fs::read_dir(&p) {
            Ok(r) => r,
            Err(_) => return true,
        };
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            let ck = fkey(root, &format!("{}/{}", rel, name));
            let is_dir = ent.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                if !self.removed_dirs.contains(&ck) {
                    return false;
                }
            } else if !matches!(self.overlay.get(&ck), Some(None)) {
                return false;
            }
        }
        true
    }
}

struct Ctx<'a> {
    store: &'a Store,
    s: State,
    d: Vdisk<'a>,
    ops: Vec<Op>,
    blockers: Vec<String>,
    unknown: Vec<String>,
    restored: HashSet<String>,
    new_dirs: Vec<DirRef>,
    removed_names: Vec<String>,
    summary: Summary,
    warnings: Vec<String>,
    ack_notes: Vec<String>,
    manual: Vec<ManualConflict>,
    displaced_new: Vec<FileSpec>,
}

fn effective(roots: &Roots, f: &FileSpec) -> FileSpec {
    if f.root == Root::Gta && f.rel.starts_with("x64/audio/sfx/") && roots.sound_mode == SoundMode::ModsDir {
        return FileSpec { root: Root::Fivem, rel: format!("mods/{}", f.rel), sha256: f.sha256.clone(), size: f.size };
    }
    f.clone()
}

fn push_op(ctx: &mut Ctx, f_root: Root, rel: &str, op: OpKind, why: &str) {
    ctx.ops.push(Op { root: f_root, rel: rel.to_string(), op, why: why.to_string() });
}

fn remove_install(ctx: &mut Ctx, id: &str, new_id: Option<&str>, retained: &HashSet<String>) -> Result<()> {
    let inst = match ctx.s.installs.iter().find(|i| i.id == id) {
        Some(i) => i.clone(),
        None => {
            ctx.blockers.push(format!("instalación {} no encontrada", id));
            return Ok(());
        }
    };
    ctx.removed_names.push(inst.name.clone());
    for f in &inst.files {
        let key = f.key();
        if retained.contains(&key) {
            if let (Some(nid), Some(rec)) = (new_id, ctx.s.records.get_mut(&key)) {
                for l in rec.layers.iter_mut() {
                    if l.install_id == id {
                        l.install_id = nid.to_string();
                    }
                }
            }
            ctx.summary.unchanged += 1;
            continue;
        }
        let rec = match ctx.s.records.get(&key) {
            Some(r) => r.clone(),
            None => {
                ctx.blockers.push(format!("registro inconsistente para «{}»", f.rel));
                continue;
            }
        };
        let idx = match rec.layers.iter().position(|l| l.install_id == id) {
            Some(i) => i,
            None => {
                ctx.blockers.push(format!("«{}» no figura como propiedad de «{}»", f.rel, inst.name));
                continue;
            }
        };
        let layer_sha = rec.layers[idx].sha256.clone();
        ctx.summary.retire_old += 1;
        let is_top = idx + 1 == rec.layers.len();
        let mut r = rec.clone();
        r.layers.remove(idx);
        if !is_top {
            ctx.s.records.insert(key, r);
            continue;
        }
        let cur = ctx.d.hash(f.root, &f.rel)?;
        if let Some(c) = &cur {
            if *c != layer_sha {
                ctx.blockers.push(format!("«{}» fue modificado fuera de la aplicación; no se elimina nada", f.rel));
                continue;
            }
        }
        if let Some(top) = r.layers.last().map(|l| l.sha256.clone()) {
            if !ctx.store.has_blob(&top) {
                ctx.blockers.push(format!("falta la copia de la capa inferior de «{}»", f.rel));
                continue;
            }
            if cur.as_deref() != Some(top.as_str()) {
                push_op(ctx, f.root, &f.rel, OpKind::Write { sha: top.clone(), expect: cur.clone() }, "volver a la capa inferior");
                ctx.d.set(f.root, &f.rel, Some(top));
            }
            ctx.summary.reveal_layers += 1;
            ctx.s.records.insert(key, r);
        } else if let Some(o) = r.original.clone() {
            if !ctx.store.has_blob(&o) {
                ctx.blockers.push(format!("falta la copia original de «{}»", f.rel));
                continue;
            }
            if cur.as_deref() != Some(o.as_str()) {
                push_op(ctx, f.root, &f.rel, OpKind::Write { sha: o.clone(), expect: cur.clone() }, "restaurar original");
                ctx.d.set(f.root, &f.rel, Some(o));
            }
            ctx.restored.insert(key.clone());
            ctx.summary.restore_originals += 1;
            ctx.s.records.remove(&key);
        } else {
            if let Some(c) = cur {
                push_op(ctx, f.root, &f.rel, OpKind::Delete { expect: c }, "archivo propio del paquete anterior");
                ctx.d.set(f.root, &f.rel, None);
            }
            ctx.s.records.remove(&key);
        }
    }
    // Archivos ajenos (skins instaladas a mano) que esta instalación retiró al instalarse.
    if !inst.displaced.is_empty() {
        match new_id {
            Some(nid) => {
                // Sustitución dentro del mismo hueco: se traspasan a la instalación nueva.
                if let Some(ni) = ctx.s.installs.iter_mut().find(|i| i.id == nid) {
                    for d in &inst.displaced {
                        if !ni.displaced.iter().any(|x| x.key() == d.key()) {
                            ni.displaced.push(d.clone());
                        }
                    }
                }
            }
            None => {
                for d in &inst.displaced {
                    let cur = ctx.d.hash(d.root, &d.rel)?;
                    match cur {
                        None => {
                            if ctx.store.has_blob(&d.sha256) {
                                push_op(ctx, d.root, &d.rel, OpKind::Write { sha: d.sha256.clone(), expect: None }, "devolver el archivo que se retiró al instalar");
                                ctx.d.set(d.root, &d.rel, Some(d.sha256.clone()));
                                ctx.summary.restore_originals += 1;
                            } else {
                                ctx.blockers.push(format!("falta la copia de «{}», que se retiró al instalar", d.rel));
                            }
                        }
                        Some(c) if c == d.sha256 => {}
                        Some(_) => ctx.warnings.push(format!("«{}» no se devuelve: ya existe otro archivo en esa ruta", d.rel)),
                    }
                }
            }
        }
    }
    // Carpetas creadas por esta instalación: solo si quedan vacías.
    let mut dirs = inst.created_dirs.clone();
    dirs.sort_by(|a, b| b.rel.len().cmp(&a.rel.len()));
    for dr in dirs {
        if !ctx.d.dir_exists(dr.root, &dr.rel) {
            continue;
        }
        if ctx.d.dir_empty(dr.root, &dr.rel) {
            push_op(ctx, dr.root, &dr.rel, OpKind::RemoveDir, "carpeta creada por el paquete, vacía");
            ctx.d.removed_dirs.insert(fkey(dr.root, &dr.rel));
        } else {
            let prefix = format!("{}/", dr.rel.to_lowercase());
            let owner = ctx
                .s
                .installs
                .iter_mut()
                .find(|i| i.id != id && i.files.iter().any(|f| f.root == dr.root && f.rel.to_lowercase().starts_with(&prefix)));
            if let Some(o) = owner {
                if !o.created_dirs.contains(&dr) {
                    o.created_dirs.push(dr.clone());
                }
            }
        }
    }
    ctx.s.installs.retain(|i| i.id != id);
    Ok(())
}

fn track_dirs(ctx: &mut Ctx, root: Root, rel: &str) {
    let parts: Vec<&str> = rel.split('/').collect();
    for n in 1..parts.len() {
        let d = parts[..n].join("/");
        if !ctx.d.dir_exists(root, &d) {
            let k = fkey(root, &d);
            ctx.d.made_dirs.insert(k.clone());
            ctx.d.removed_dirs.remove(&k);
            ctx.new_dirs.push(DirRef { root, rel: d });
        }
    }
}

fn install_files(ctx: &mut Ctx, cat: Category, new_id: &str, files: &[FileSpec], retained: &HashSet<String>) -> Result<()> {
    let prio = cat.priority();
    for f in files {
        let key = f.key();
        if retained.contains(&key) {
            continue;
        }
        let rec_opt = ctx.s.records.get(&key).cloned();
        match rec_opt {
            Some(mut rec) if !rec.layers.is_empty() => {
                let mut conflict: Option<String> = None;
                for l in &rec.layers {
                    if let Some(i) = ctx.s.installs.iter().find(|i| i.id == l.install_id) {
                        if i.category.priority() == prio && l.sha256 != f.sha256 {
                            conflict = Some(i.name.clone());
                        }
                    }
                }
                if let Some(n) = conflict {
                    ctx.blockers.push(format!("conflicto: «{}» ya pertenece a «{}» con contenido distinto", f.rel, n));
                    continue;
                }
                let top = rec.layers.last().unwrap().sha256.clone();
                let cur = ctx.d.hash(f.root, &f.rel)?;
                if cur.as_deref() != Some(top.as_str()) {
                    ctx.blockers.push(format!("«{}» fue modificado o falta fuera de la aplicación", f.rel));
                    continue;
                }
                let mut pos = rec.layers.len();
                while pos > 0 {
                    let p = ctx
                        .s
                        .installs
                        .iter()
                        .find(|i| i.id == rec.layers[pos - 1].install_id)
                        .map(|i| i.category.priority())
                        .unwrap_or(0);
                    if p > prio {
                        pos -= 1;
                    } else {
                        break;
                    }
                }
                rec.layers.insert(pos, Layer { install_id: new_id.to_string(), sha256: f.sha256.clone() });
                let is_top = pos + 1 == rec.layers.len();
                if is_top && top != f.sha256 {
                    push_op(ctx, f.root, &f.rel, OpKind::Write { sha: f.sha256.clone(), expect: Some(top) }, "capa superior");
                    ctx.d.set(f.root, &f.rel, Some(f.sha256.clone()));
                    ctx.summary.install_new += 1;
                } else if is_top {
                    ctx.summary.unchanged += 1;
                } else {
                    ctx.summary.install_new += 1;
                }
                ctx.s.records.insert(key, rec);
            }
            _ => {
                let cur = ctx.d.hash(f.root, &f.rel)?;
                let mut original = None;
                match cur {
                    None => {
                        track_dirs(ctx, f.root, &f.rel);
                        push_op(ctx, f.root, &f.rel, OpKind::Write { sha: f.sha256.clone(), expect: None }, "archivo nuevo");
                        ctx.d.set(f.root, &f.rel, Some(f.sha256.clone()));
                        ctx.summary.install_new += 1;
                    }
                    Some(c) if c == f.sha256 => {
                        original = Some(c);
                        ctx.summary.unchanged += 1;
                    }
                    Some(c) => {
                        if !ctx.restored.contains(&key) {
                            ctx.unknown.push(f.rel.clone());
                        }
                        push_op(ctx, f.root, &f.rel, OpKind::Write { sha: f.sha256.clone(), expect: Some(c.clone()) }, "sustituye un archivo previo (se guarda copia)");
                        ctx.d.set(f.root, &f.rel, Some(f.sha256.clone()));
                        original = Some(c);
                        ctx.summary.install_new += 1;
                    }
                }
                ctx.s.records.insert(
                    key,
                    Record { root: f.root, rel: f.rel.clone(), original, layers: vec![Layer { install_id: new_id.to_string(), sha256: f.sha256.clone() }] },
                );
            }
        }
    }
    Ok(())
}

fn merge_ops(ops: Vec<Op>) -> Vec<Op> {
    let mut out: Vec<Option<Op>> = Vec::new();
    let mut last: HashMap<String, usize> = HashMap::new();
    for op in ops {
        if matches!(op.op, OpKind::RemoveDir) {
            out.push(Some(op));
            continue;
        }
        let k = fkey(op.root, &op.rel);
        if let Some(&pi) = last.get(&k) {
            if let Some(p) = out[pi].clone() {
                let merged = match (&p.op, &op.op) {
                    (OpKind::Delete { expect }, OpKind::Write { sha, .. }) => Some(OpKind::Write { sha: sha.clone(), expect: Some(expect.clone()) }),
                    (OpKind::Write { expect: e, .. }, OpKind::Write { sha, .. }) => Some(OpKind::Write { sha: sha.clone(), expect: e.clone() }),
                    _ => None,
                };
                if let Some(m) = merged {
                    out[pi] = None;
                    out.push(Some(Op { root: op.root, rel: op.rel.clone(), op: m, why: op.why.clone() }));
                    last.insert(k, out.len() - 1);
                    continue;
                }
            }
        }
        out.push(Some(op));
        last.insert(k, out.len() - 1);
    }
    out.into_iter().flatten().collect()
}

fn dependents(state: &State, ids: &mut Vec<String>) {
    let mut changed = true;
    while changed {
        changed = false;
        for i in &state.installs {
            if let Some(l) = &i.linked_to {
                if ids.contains(l) && !ids.contains(&i.id) {
                    ids.push(i.id.clone());
                    changed = true;
                }
            }
        }
    }
}

/// Arma de una instalación: la guardada o, para instalaciones antiguas, la deducida del paquete de la biblioteca.
fn install_weapon_kind(store: &Store, inst: &Install) -> Option<String> {
    if inst.weapon_kind.is_some() {
        return inst.weapon_kind.clone();
    }
    if inst.category != Category::Weapon {
        return None;
    }
    match store.load_package(&inst.package_id) {
        Ok(p) => p.weapon_kind.clone().or_else(|| {
            let names: Vec<String> = p.files.iter().map(|f| f.rel.rsplit('/').next().unwrap_or("").to_string()).collect();
            weapon::detect(&names, &p.contents).kind
        }),
        Err(_) => {
            let names: Vec<String> = inst.files.iter().map(|f| f.rel.rsplit('/').next().unwrap_or("").to_string()).collect();
            weapon::detect(&names, &[]).kind
        }
    }
}

struct Found {
    rel: String,
    kind: Option<String>,
    same_weapon: bool,
    shared_mount: Option<String>,
    size: u64,
    sha: String,
}

fn collect_rpf(dir: &Path, rel: &str, depth: u32, out: &mut Vec<(String, PathBuf)>) {
    if depth > 3 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        let ft = match ent.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            continue;
        }
        let r = format!("{}/{}", rel, name);
        if ft.is_dir() {
            collect_rpf(&ent.path(), &r, depth + 1, out);
        } else if name.to_lowercase().ends_with(".rpf") && !name.starts_with(".444tmp-") {
            out.push((r, ent.path()));
        }
    }
}

/// Busca en `mods/` RPF de armas que NO gestiona 444CFG y que son la misma arma (o comparten montaje).
fn scan_manual_weapons(state: &State, roots: &Roots, pkg: &Package, files: &[FileSpec]) -> Vec<Found> {
    let mut found = Vec::new();
    let base = match roots.base(Root::Fivem) {
        Ok(b) => b.join("mods"),
        Err(_) => return found,
    };
    if !base.is_dir() {
        return found;
    }
    let mut cands: Vec<(String, PathBuf)> = Vec::new();
    collect_rpf(&base, "mods", 0, &mut cands);
    for (rel, path) in cands {
        let managed = state.records.get(&fkey(Root::Fivem, &rel)).map(|r| !r.layers.is_empty()).unwrap_or(false);
        if managed {
            continue;
        }
        if files.iter().any(|f| f.root == Root::Fivem && f.rel.eq_ignore_ascii_case(&rel)) {
            continue; // mismo destino: lo gestiona la sustitución normal con copia
        }
        let info = match rpf::inspect(&path) {
            Ok(i) if i.valid => i,
            _ => continue,
        };
        let fname = rel.rsplit('/').next().unwrap_or("").to_string();
        let mut entries: Vec<String> = info.weapon_models.iter().map(|m| format!("{}.ydr", m)).collect();
        entries.extend(info.tree.iter().cloned());
        let det = weapon::detect(&[fname], &entries);
        let same_weapon = match &pkg.weapon_kind {
            Some(k) => det.kind.as_deref() == Some(k.as_str()),
            None => {
                let mut ms = info.weapon_models.clone();
                ms.sort();
                !ms.is_empty() && pkg.slot.as_deref() == Some(format!("weapon:{}", ms.join("+")).as_str())
            }
        };
        let mut ids: Vec<String> = info.device_names.iter().map(|d| format!("device:{}", d.to_lowercase())).collect();
        ids.extend(info.assembly_targets.iter().map(|t| format!("dlc:{}", t)));
        let shared_mount = ids.into_iter().find(|i| pkg.mount_ids.contains(i));
        if !same_weapon && shared_mount.is_none() {
            continue;
        }
        let sha = match hash_path(&path) {
            Ok(Some(h)) => h,
            _ => continue,
        };
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        found.push(Found { rel, kind: det.kind, same_weapon, shared_mount, size, sha });
    }
    found
}

pub fn plan_install(store: &Store, state: &State, pkg: &Package, roots: &Roots, link_to: Option<&str>) -> Result<Plan> {
    let mut ctx = Ctx {
        store,
        s: state.clone(),
        d: Vdisk::new(roots),
        ops: vec![],
        blockers: vec![],
        unknown: vec![],
        restored: HashSet::new(),
        new_dirs: vec![],
        removed_names: vec![],
        summary: Summary::default(),
        warnings: vec![],
        ack_notes: vec![],
        manual: vec![],
        displaced_new: vec![],
    };
    let new_id: String = sha256_bytes(format!("{}|{}", pkg.id, uniq()).as_bytes())[..16].to_string();
    let files: Vec<FileSpec> = pkg.files.iter().map(|f| effective(roots, f)).collect();
    for f in &files {
        if !store.has_blob(&f.sha256) {
            ctx.blockers.push(format!("falta en la biblioteca el contenido de «{}»", f.rel));
        }
    }
    if state.installs.iter().any(|i| i.package_id == pkg.id) {
        ctx.blockers.push("este paquete ya está instalado".into());
    }
    for r in [Root::Fivem, Root::Gta] {
        if files.iter().any(|f| f.root == r) {
            match roots.base(r) {
                Ok(b) => {
                    if !b.is_dir() {
                        ctx.blockers.push(format!("la carpeta {} no existe", b.display()));
                    }
                }
                Err(e) => ctx.blockers.push(e.to_string()),
            }
        }
    }
    if files.iter().any(|f| f.root == Root::Gta && f.rel.starts_with("x64/audio/sfx/")) {
        if let Ok(b) = roots.base(Root::Gta) {
            if b.is_dir() && !b.join("x64").join("audio").join("sfx").is_dir() {
                ctx.blockers.push(format!(
                    "la carpeta de GTA V seleccionada ({}) no contiene x64\\audio\\sfx: revisa la ruta en Ajustes",
                    b.display()
                ));
            }
        }
    }
    let mut remove_ids: Vec<String> = vec![];
    if let Some(slot) = &pkg.slot {
        for i in &state.installs {
            let same = if pkg.category == Category::Weapon && pkg.weapon_kind.is_some() {
                i.category == Category::Weapon && install_weapon_kind(store, i) == pkg.weapon_kind
            } else {
                i.category == pkg.category && i.slot.as_ref() == Some(slot)
            };
            if same {
                remove_ids.push(i.id.clone());
            }
        }
    }
    let slot_ids: Vec<String> = remove_ids.clone();
    dependents(state, &mut remove_ids);
    if let Some(l) = link_to {
        if !state.installs.iter().any(|i| i.id == l) || remove_ids.iter().any(|r| r == l) {
            ctx.blockers.push("el elemento al que se quiere vincular no existe o se está retirando".into());
        }
    }
    for i in &state.installs {
        if remove_ids.contains(&i.id) {
            continue;
        }
        if let Some(m) = i.mount_ids.iter().find(|m| pkg.mount_ids.contains(m)) {
            let other_kind = install_weapon_kind(store, i);
            let both_weapons = pkg.category == Category::Weapon && i.category == Category::Weapon;
            if both_weapons && pkg.weapon_kind.is_some() && other_kind.is_some() && other_kind != pkg.weapon_kind {
                // MK2 y 9mm son armas distintas y se gestionan por separado; que compartan montaje no se puede verificar.
                let note = format!(
                    "«{}» ({}) y «{}» ({}) comparten el identificador de montaje «{}»: FiveM puede cargar solo una de las dos y no está verificado que funcionen a la vez",
                    pkg.name,
                    weapon::label(pkg.weapon_kind.as_deref().unwrap_or("")),
                    i.name,
                    weapon::label(other_kind.as_deref().unwrap_or("")),
                    m
                );
                ctx.ack_notes.push(note.clone());
                ctx.warnings.push(note);
            } else {
                ctx.blockers.push(format!(
                    "«{}» comparte el identificador de montaje «{}» con «{}»: instalación simultánea no verificada, operación bloqueada",
                    pkg.name, m, i.name
                ));
            }
        }
    }
    ctx.s.installs.push(Install {
        id: new_id.clone(),
        package_id: pkg.id.clone(),
        name: pkg.name.clone(),
        category: pkg.category,
        slot: pkg.slot.clone(),
        installed_at: now_secs(),
        files: files.clone(),
        created_dirs: vec![],
        mount_ids: pkg.mount_ids.clone(),
        linked_to: link_to.map(|s| s.to_string()),
        weapon_kind: pkg.weapon_kind.clone(),
        displaced: vec![],
    });
    let new_map: HashMap<String, String> = files.iter().map(|f| (f.key(), f.sha256.clone())).collect();
    let mut retained: HashSet<String> = HashSet::new();
    for rid in &slot_ids {
        if let Some(old) = state.installs.iter().find(|i| i.id == *rid) {
            for f in &old.files {
                if new_map.get(&f.key()) == Some(&f.sha256) {
                    retained.insert(f.key());
                }
            }
        }
    }
    for rid in &remove_ids {
        let ret = if slot_ids.contains(rid) { retained.clone() } else { HashSet::new() };
        remove_install(&mut ctx, rid, Some(new_id.as_str()), &ret)?;
    }
    install_files(&mut ctx, pkg.category, &new_id, &files, &retained)?;
    if pkg.category == Category::Weapon {
        for fnd in scan_manual_weapons(state, roots, pkg, &files) {
            if fnd.same_weapon {
                let reason = match &fnd.kind {
                    Some(k) => format!("misma arma ({}) instalada sin 444CFG TOOL", weapon::label(k)),
                    None => "mismo modelo de arma instalado sin 444CFG TOOL".to_string(),
                };
                if ctx.d.hash(Root::Fivem, &fnd.rel)?.as_deref() != Some(fnd.sha.as_str()) {
                    ctx.blockers.push(format!("«{}» cambió mientras se preparaba la operación", fnd.rel));
                    continue;
                }
                push_op(&mut ctx, Root::Fivem, &fnd.rel, OpKind::Delete { expect: fnd.sha.clone() }, "skin anterior instalada a mano (se guarda copia)");
                ctx.d.set(Root::Fivem, &fnd.rel, None);
                ctx.displaced_new.push(FileSpec { root: Root::Fivem, rel: fnd.rel.clone(), sha256: fnd.sha.clone(), size: fnd.size });
                ctx.manual.push(ManualConflict { rel: fnd.rel.clone(), kind: fnd.kind.clone(), reason, size: fnd.size });
            } else if let Some(m) = &fnd.shared_mount {
                let note = format!(
                    "«{}» (instalado a mano) comparte el identificador de montaje «{}» con esta skin; no se toca, pero puede entrar en conflicto",
                    fnd.rel, m
                );
                ctx.ack_notes.push(note.clone());
                ctx.warnings.push(note);
            }
        }
    }
    if pkg.needs_ack_unverified {
        ctx.ack_notes.push("El destino de los RPF de este Tracer (mods) no está verificado para este paquete".into());
    }
    let nd = ctx.new_dirs.clone();
    let dn = ctx.displaced_new.clone();
    if let Some(i) = ctx.s.installs.iter_mut().find(|i| i.id == new_id) {
        i.created_dirs = nd;
        for d in dn {
            if !i.displaced.iter().any(|x| x.key() == d.key()) {
                i.displaced.push(d);
            }
        }
    }
    ctx.summary.keep_others = ctx.s.installs.iter().filter(|i| i.id != new_id).map(|i| i.files.len()).sum();
    let new_install = ctx.s.installs.iter().find(|i| i.id == new_id).cloned();
    Ok(Plan {
        kind: "install".into(),
        title: format!("Instalar «{}»", pkg.name),
        package_id: Some(pkg.id.clone()),
        removes: ctx.removed_names.clone(),
        ops: merge_ops(ctx.ops),
        summary: ctx.summary,
        blockers: ctx.blockers,
        warnings: pkg.warnings.iter().cloned().chain(ctx.warnings.iter().cloned()).collect(),
        unknown_overwrites: ctx.unknown,
        needs_ack_unverified: !ctx.ack_notes.is_empty(),
        ack_notes: ctx.ack_notes,
        manual_conflicts: ctx.manual,
        weapon_kind: pkg.weapon_kind.clone(),
        state_after: ctx.s,
        new_install,
    })
}

pub fn plan_uninstall(store: &Store, state: &State, id: &str, roots: &Roots) -> Result<Plan> {
    let mut ctx = Ctx {
        store,
        s: state.clone(),
        d: Vdisk::new(roots),
        ops: vec![],
        blockers: vec![],
        unknown: vec![],
        restored: HashSet::new(),
        new_dirs: vec![],
        removed_names: vec![],
        summary: Summary::default(),
        warnings: vec![],
        ack_notes: vec![],
        manual: vec![],
        displaced_new: vec![],
    };
    let mut ids = vec![id.to_string()];
    let name = state.installs.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or_default();
    dependents(state, &mut ids);
    let empty = HashSet::new();
    for rid in &ids {
        remove_install(&mut ctx, rid, None, &empty)?;
    }
    ctx.summary.keep_others = ctx.s.installs.iter().map(|i| i.files.len()).sum();
    Ok(Plan {
        kind: "uninstall".into(),
        title: format!("Retirar «{}»", name),
        package_id: None,
        removes: ctx.removed_names.clone(),
        ops: merge_ops(ctx.ops),
        summary: ctx.summary,
        blockers: ctx.blockers,
        warnings: ctx.warnings,
        unknown_overwrites: ctx.unknown,
        needs_ack_unverified: false,
        ack_notes: vec![],
        manual_conflicts: vec![],
        weapon_kind: None,
        state_after: ctx.s,
        new_install: None,
    })
}
