use crate::model::*;
use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn uniq() -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", std::process::id(), t, n)
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn sha256_bytes(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    format!("{:x}", h.finalize())
}

pub fn sha256_file(p: &Path) -> Result<String> {
    let mut f = File::open(p).with_context(|| format!("no se puede abrir {}", p.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Hash de un archivo; `None` si no existe; error si es una carpeta.
pub fn hash_path(p: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(p) {
        Ok(m) => {
            if m.is_dir() {
                bail!("se esperaba un archivo pero es una carpeta: {}", p.display());
            }
            Ok(Some(sha256_file(p)?))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| anyhow!("ruta sin carpeta padre"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".444tmp-{}", uniq()));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        anyhow!(e)
    })
}

pub fn valid_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Almacén de la aplicación: blobs por hash (paquetes y copias de seguridad),
/// biblioteca, estado y diario. Siempre fuera de las carpetas del juego.
#[derive(Clone)]
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Store> {
        for s in ["blobs", "tmp", "library"] {
            fs::create_dir_all(dir.join(s))?;
        }
        Ok(Store { dir: dir.to_path_buf() })
    }

    pub fn blob_path(&self, sha: &str) -> Result<PathBuf> {
        if !valid_sha(sha) {
            bail!("hash no válido");
        }
        Ok(self.dir.join("blobs").join(sha))
    }

    pub fn has_blob(&self, sha: &str) -> bool {
        self.blob_path(sha).map(|p| p.is_file()).unwrap_or(false)
    }

    /// Copia un flujo al almacén calculando su SHA-256. `budget` limita los bytes totales.
    pub fn put_reader<R: Read>(&self, r: &mut R, budget: &mut u64) -> Result<(String, u64)> {
        let tmp = self.dir.join("tmp").join(uniq());
        let res = (|| -> Result<(String, u64)> {
            let mut f = File::create(&tmp)?;
            let mut h = Sha256::new();
            let mut buf = vec![0u8; 1 << 20];
            let mut total = 0u64;
            loop {
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                if (n as u64) > *budget {
                    bail!("el contenido supera el límite de tamaño permitido (posible ZIP bomb)");
                }
                *budget -= n as u64;
                total += n as u64;
                h.update(&buf[..n]);
                f.write_all(&buf[..n])?;
            }
            f.sync_all()?;
            Ok((format!("{:x}", h.finalize()), total))
        })();
        match res {
            Ok((sha, total)) => {
                let dest = self.blob_path(&sha)?;
                if dest.exists() {
                    let _ = fs::remove_file(&tmp);
                } else {
                    fs::rename(&tmp, &dest)?;
                }
                Ok((sha, total))
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    pub fn put_file(&self, p: &Path) -> Result<(String, u64)> {
        let mut f = File::open(p)?;
        let mut budget = u64::MAX;
        self.put_reader(&mut f, &mut budget)
    }

    pub fn state_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    pub fn journal_path(&self) -> PathBuf {
        self.dir.join("journal.json")
    }

    pub fn state_bytes(&self) -> Result<Vec<u8>> {
        match fs::read(self.state_path()) {
            Ok(b) => Ok(b),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn load_state(&self) -> Result<State> {
        let b = self.state_bytes()?;
        if b.is_empty() {
            return Ok(State::default());
        }
        serde_json::from_slice(&b).context("el archivo de estado está dañado")
    }

    pub fn save_package(&self, p: &Package) -> Result<()> {
        atomic_write(
            &self.dir.join("library").join(format!("{}.json", p.id)),
            &serde_json::to_vec_pretty(p)?,
        )
    }

    pub fn load_package(&self, id: &str) -> Result<Package> {
        if !valid_sha(id) {
            bail!("identificador de paquete no válido");
        }
        let b = fs::read(self.dir.join("library").join(format!("{}.json", id)))
            .context("paquete no encontrado en la biblioteca")?;
        Ok(serde_json::from_slice(&b)?)
    }

    pub fn list_packages(&self) -> Result<Vec<Package>> {
        let mut v = Vec::new();
        for e in fs::read_dir(self.dir.join("library"))? {
            let e = e?;
            if e.path().extension().and_then(|x| x.to_str()) == Some("json") {
                if let Ok(b) = fs::read(e.path()) {
                    if let Ok(p) = serde_json::from_slice::<Package>(&b) {
                        v.push(p);
                    }
                }
            }
        }
        v.sort_by_key(|p| p.created_at);
        Ok(v)
    }

    pub fn delete_package(&self, id: &str) -> Result<()> {
        if !valid_sha(id) {
            bail!("identificador de paquete no válido");
        }
        fs::remove_file(self.dir.join("library").join(format!("{}.json", id)))?;
        Ok(())
    }
}
