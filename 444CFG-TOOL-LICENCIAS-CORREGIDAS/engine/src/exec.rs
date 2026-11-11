//! Ejecutor transaccional: diario previo, comprobación de hash antes de cada operación,
//! copia de seguridad por hash antes de sobrescribir o borrar, verificación posterior
//! y reversión completa si algo falla (también tras una caída, vía `recover`).
use crate::model::*;
use crate::planner::{Op, OpKind, Plan};
use crate::store::*;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Avance real de una operación: bytes copiados y operaciones terminadas.
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub ops_done: usize,
    pub ops_total: usize,
    pub current: String,
}

#[derive(Clone, Default)]
pub struct ExecOptions {
    pub confirmed_unknown: bool,
    pub fivem_running: bool,
    pub ack_unverified: bool,
    /// Solo pruebas: falla antes de ejecutar la operación n.
    pub fail_after_ops: Option<usize>,
    /// Solo pruebas: simula una caída sin revertir (el diario queda para `recover`).
    pub skip_rollback_on_fail: bool,
    pub on_progress: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
}

#[derive(Serialize, Deserialize)]
struct Journal {
    id: String,
    before_sha: String,
    after_sha: String,
    roots: Roots,
    ops: Vec<Op>,
    candidate_dirs: Vec<DirRef>,
}

fn existing_ancestor(p: &Path) -> PathBuf {
    let mut c = p.to_path_buf();
    while !c.exists() {
        if !c.pop() {
            break;
        }
    }
    c
}

fn check_space(store: &Store, roots: &Roots, ops: &[Op]) -> Result<()> {
    let mut per_root: BTreeMap<Root, u64> = BTreeMap::new();
    let mut total = 0u64;
    for op in ops {
        if let OpKind::Write { sha, .. } = &op.op {
            let sz = fs::metadata(store.blob_path(sha)?).map(|m| m.len()).unwrap_or(0);
            *per_root.entry(op.root).or_insert(0) += sz;
            total += sz;
        }
    }
    let margin = 64u64 << 20;
    for (r, need) in per_root {
        let base = existing_ancestor(roots.base(r)?);
        let free = fs2::available_space(&base)?;
        if free < need + margin {
            bail!("espacio insuficiente en {}: se necesitan {} MB", base.display(), (need + margin) >> 20);
        }
    }
    let free = fs2::available_space(existing_ancestor(&store.dir))?;
    if free < total + margin {
        bail!("espacio insuficiente para las copias de seguridad en {}", store.dir.display());
    }
    Ok(())
}

pub(crate) fn copy_chunked(src: &Path, dst: &Path, on: &mut dyn FnMut(u64)) -> Result<()> {
    let mut r = File::open(src)?;
    let mut w = File::create(dst)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n])?;
        on(n as u64);
    }
    w.sync_all()?;
    Ok(())
}

pub(crate) fn write_from_blob(store: &Store, dest: &Path, sha: &str, on: &mut dyn FnMut(u64)) -> Result<()> {
    let blob = store.blob_path(sha)?;
    if !blob.is_file() {
        bail!("falta la copia almacenada {}", sha);
    }
    let dir = dest.parent().ok_or_else(|| anyhow!("ruta sin carpeta padre"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".444tmp-{}", uniq()));
    if let Err(e) = copy_chunked(&blob, &tmp, on) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Comprueba antes de empezar que se puede escribir en cada carpeta de destino (p. ej. «Program Files»).
fn check_writable(roots: &Roots, ops: &[Op]) -> Result<()> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    for op in ops {
        if matches!(op.op, OpKind::RemoveDir) {
            continue;
        }
        let p = roots.resolve(op.root, &op.rel)?;
        let dir = existing_ancestor(p.parent().unwrap_or(&p));
        if !seen.insert(dir.clone()) {
            continue;
        }
        let probe = dir.join(format!(".444perm-{}", uniq()));
        match File::create(&probe) {
            Ok(_) => {
                let _ = fs::remove_file(&probe);
            }
            Err(e) => bail!(
                "Sin permiso de escritura en «{}» ({}). Si GTA V está en «Program Files», abre 444CFG TOOL con «Ejecutar como administrador» o cambia el método de Sound Packs en Ajustes.",
                dir.display(),
                e
            ),
        }
    }
    Ok(())
}

fn exec_op(store: &Store, roots: &Roots, op: &Op, on: &mut dyn FnMut(u64)) -> Result<()> {
    let path = roots.resolve(op.root, &op.rel)?;
    match &op.op {
        OpKind::Write { sha, expect } => {
            let cur = hash_path(&path)?;
            if &cur != expect {
                bail!("«{}» cambió antes de escribirse; operación cancelada", op.rel);
            }
            if let Some(h) = expect {
                if !store.has_blob(h) {
                    let (s, _) = store.put_file(&path)?;
                    if &s != h {
                        bail!("copia de seguridad inconsistente para «{}»", op.rel);
                    }
                }
            }
            write_from_blob(store, &path, sha, on)?;
            let after = hash_path(&path)?;
            if after.as_deref() != Some(sha.as_str()) {
                bail!("la verificación falló tras escribir «{}»", op.rel);
            }
        }
        OpKind::Delete { expect } => {
            if let Some(c) = hash_path(&path)? {
                if &c != expect {
                    bail!("«{}» cambió antes de borrarse; operación cancelada", op.rel);
                }
                if !store.has_blob(expect) {
                    let (s, _) = store.put_file(&path)?;
                    if &s != expect {
                        bail!("copia de seguridad inconsistente para «{}»", op.rel);
                    }
                }
                fs::remove_file(&path)?;
            }
        }
        OpKind::RemoveDir => match fs::remove_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => bail!("no se pudo quitar la carpeta «{}»: {}", op.rel, e),
        },
    }
    Ok(())
}

fn undo(store: &Store, j: &Journal) -> Result<()> {
    let roots = &j.roots;
    for op in j.ops.iter().rev() {
        let path = roots.resolve(op.root, &op.rel)?;
        match &op.op {
            OpKind::Write { sha, expect } => {
                let cur = hash_path(&path)?;
                match expect {
                    Some(h) => {
                        if cur.as_deref() != Some(h.as_str()) {
                            if cur.is_some() && cur.as_deref() != Some(sha.as_str()) {
                                bail!("«{}» cambió por una causa ajena; no se restaura automáticamente", op.rel);
                            }
                            write_from_blob(store, &path, h, &mut |_| {})?;
                        }
                    }
                    None => {
                        if cur.as_deref() == Some(sha.as_str()) {
                            fs::remove_file(&path)?;
                        } else if cur.is_some() {
                            bail!("«{}» cambió por una causa ajena; no se elimina", op.rel);
                        }
                    }
                }
            }
            OpKind::Delete { expect } => {
                let cur = hash_path(&path)?;
                if cur.is_none() {
                    write_from_blob(store, &path, expect, &mut |_| {})?;
                } else if cur.as_deref() != Some(expect.as_str()) {
                    bail!("«{}» cambió por una causa ajena; no se restaura", op.rel);
                }
            }
            OpKind::RemoveDir => {
                fs::create_dir_all(&path)?;
            }
        }
    }
    let mut dirs = j.candidate_dirs.clone();
    dirs.sort_by(|a, b| b.rel.len().cmp(&a.rel.len()));
    for d in dirs {
        if let Ok(p) = roots.resolve(d.root, &d.rel) {
            let _ = fs::remove_dir(p); // remove_dir nunca borra carpetas con contenido
        }
    }
    Ok(())
}

fn verify_post(roots: &Roots, plan: &Plan) -> Result<()> {
    if let Some(ni) = &plan.new_install {
        for f in &ni.files {
            if let Some(rec) = plan.state_after.records.get(&f.key()) {
                if let Some(top) = rec.layers.last() {
                    if top.install_id == ni.id {
                        let h = hash_path(&roots.resolve(f.root, &f.rel)?)?;
                        if h.as_deref() != Some(top.sha256.as_str()) {
                            bail!("verificación final fallida en «{}»", f.rel);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn apply(store: &Store, roots: &Roots, plan: &Plan, opts: &ExecOptions) -> Result<State> {
    if !plan.blockers.is_empty() {
        bail!("operación bloqueada: {}", plan.blockers.join(" | "));
    }
    if opts.fivem_running {
        bail!("FiveM o GTA V están en ejecución; ciérralos antes de continuar");
    }
    if plan.needs_ack_unverified && !opts.ack_unverified {
        bail!("se requiere confirmación expresa: {}", plan.ack_notes.join(" | "));
    }
    if (!plan.unknown_overwrites.is_empty() || !plan.manual_conflicts.is_empty()) && !opts.confirmed_unknown {
        let mut names: Vec<String> = plan.unknown_overwrites.clone();
        names.extend(plan.manual_conflicts.iter().map(|m| m.rel.clone()));
        bail!(
            "se requiere confirmación para reemplazar archivos no gestionados (se guardará copia): {}",
            names.join(", ")
        );
    }
    if store.journal_path().exists() {
        bail!("hay una operación anterior pendiente de recuperar");
    }
    check_space(store, roots, &plan.ops)?;
    check_writable(roots, &plan.ops)?;
    let state_bytes = serde_json::to_vec_pretty(&plan.state_after)?;
    let journal = Journal {
        id: uniq(),
        before_sha: sha256_bytes(&store.state_bytes()?),
        after_sha: sha256_bytes(&state_bytes),
        roots: roots.clone(),
        ops: plan.ops.clone(),
        candidate_dirs: plan.new_install.as_ref().map(|i| i.created_dirs.clone()).unwrap_or_default(),
    };
    atomic_write(&store.journal_path(), &serde_json::to_vec_pretty(&journal)?)?;
    let total = plan.ops.len();
    let total_bytes: u64 = plan
        .ops
        .iter()
        .map(|op| match &op.op {
            OpKind::Write { sha, .. } => store.blob_path(sha).ok().and_then(|p| fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum();
    let run = (|| -> Result<()> {
        let mut base_bytes = 0u64;
        for (i, op) in plan.ops.iter().enumerate() {
            if opts.fail_after_ops == Some(i) {
                bail!("fallo simulado antes de la operación {}", i);
            }
            let mut cur_bytes = base_bytes;
            {
                let mut cb = |n: u64| {
                    cur_bytes += n;
                    if let Some(p) = &opts.on_progress {
                        p(Progress { done_bytes: cur_bytes, total_bytes, ops_done: i, ops_total: total, current: op.rel.clone() });
                    }
                };
                exec_op(store, roots, op, &mut cb)?;
            }
            base_bytes = cur_bytes;
            if let Some(p) = &opts.on_progress {
                p(Progress { done_bytes: base_bytes, total_bytes, ops_done: i + 1, ops_total: total, current: op.rel.clone() });
            }
        }
        if opts.fail_after_ops == Some(total) {
            bail!("fallo simulado tras las operaciones");
        }
        verify_post(roots, plan)?;
        atomic_write(&store.state_path(), &state_bytes)?;
        Ok(())
    })();
    match run {
        Ok(()) => {
            let _ = fs::remove_file(store.journal_path());
            Ok(plan.state_after.clone())
        }
        Err(e) => {
            if opts.skip_rollback_on_fail {
                return Err(e.context("(simulación de caída: sin reversión)"));
            }
            match undo(store, &journal) {
                Ok(()) => {
                    let _ = fs::remove_file(store.journal_path());
                    Err(e.context("la operación falló y se restauró el estado anterior"))
                }
                Err(u) => Err(anyhow!(
                    "la operación falló ({}) y la recuperación también ({}); se conserva el diario para recuperar al reiniciar",
                    e,
                    u
                )),
            }
        }
    }
}

/// Recupera una operación interrumpida. Devuelve un mensaje si había algo pendiente.
pub fn recover(store: &Store) -> Result<Option<String>> {
    let jp = store.journal_path();
    let bytes = match fs::read(&jp) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let j: Journal = serde_json::from_slice(&bytes)?;
    if sha256_bytes(&store.state_bytes()?) == j.after_sha {
        fs::remove_file(&jp)?;
        return Ok(Some("Operación anterior completada; diario cerrado".into()));
    }
    undo(store, &j)?;
    fs::remove_file(&jp)?;
    Ok(Some("Operación interrumpida revertida; los archivos recuperaron su estado anterior".into()))
}
