use crate::pathsafe::sanitize_rel;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Raíz sobre la que se resuelve una ruta relativa.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Root {
    /// Carpeta FiveM.app
    Fivem,
    /// Carpeta de GTA V
    Gta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Citizen,
    Mod,
    Weapon,
    BloodFx,
    Tracer,
    SoundPack,
}

impl Category {
    /// Prioridad de capa: una capa de mayor prioridad se apila sobre una menor.
    /// Dos instalaciones distintas con la misma prioridad y distinto contenido en
    /// el mismo archivo son un conflicto y la operación se bloquea.
    pub fn priority(self) -> u8 {
        match self {
            Category::Citizen => 0,
            Category::Mod | Category::Weapon => 1,
            Category::BloodFx | Category::Tracer | Category::SoundPack => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundMode {
    /// GTAV\x64\audio\sfx (modifica la carpeta del juego; se guarda copia)
    GtaSfx,
    /// FiveM.app\mods\x64\audio\sfx (no toca la carpeta del juego; sin verificar en todos los entornos)
    ModsDir,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Roots {
    pub fivem: PathBuf,
    pub gta: Option<PathBuf>,
    pub sound_mode: SoundMode,
}

impl Roots {
    pub fn base(&self, root: Root) -> Result<&Path> {
        match root {
            Root::Fivem => Ok(self.fivem.as_path()),
            Root::Gta => self
                .gta
                .as_deref()
                .ok_or_else(|| anyhow!("la ruta de GTA V no está configurada")),
        }
    }

    /// Resuelve una ruta relativa de forma segura dentro de la raíz.
    pub fn resolve(&self, root: Root, rel: &str) -> Result<PathBuf> {
        let rel = sanitize_rel(rel)?;
        let base = self.base(root)?;
        let mut p = base.to_path_buf();
        for part in rel.split('/') {
            p.push(part);
        }
        crate::pathsafe::no_symlinks(base, &p)?;
        Ok(p)
    }
}

pub fn fkey(root: Root, rel: &str) -> String {
    format!(
        "{}:{}",
        match root {
            Root::Fivem => "f",
            Root::Gta => "g",
        },
        rel.to_lowercase()
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileSpec {
    pub root: Root,
    pub rel: String,
    pub sha256: String,
    pub size: u64,
}

impl FileSpec {
    pub fn key(&self) -> String {
        fkey(self.root, &self.rel)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DirRef {
    pub root: Root,
    pub rel: String,
}

/// Paquete importado a la biblioteca (contenido guardado por hash).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub slot: Option<String>,
    pub files: Vec<FileSpec>,
    pub warnings: Vec<String>,
    pub mount_ids: Vec<String>,
    pub contents: Vec<String>,
    pub needs_ack_unverified: bool,
    pub created_at: u64,
    /// Arma detectada en una skin de arma: "mk2" o "9mm" (None si no se pudo identificar).
    #[serde(default)]
    pub weapon_kind: Option<String>,
    /// Pruebas que llevaron a esa detección (contenido y nombre de archivo).
    #[serde(default)]
    pub weapon_evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Install {
    pub id: String,
    pub package_id: String,
    pub name: String,
    pub category: Category,
    pub slot: Option<String>,
    pub installed_at: u64,
    pub files: Vec<FileSpec>,
    pub created_dirs: Vec<DirRef>,
    pub mount_ids: Vec<String>,
    pub linked_to: Option<String>,
    #[serde(default)]
    pub weapon_kind: Option<String>,
    /// Archivos ajenos (skins instaladas a mano) que esta instalación retiró.
    /// Se guarda su copia por hash y se devuelven al retirar la instalación.
    #[serde(default)]
    pub displaced: Vec<FileSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub install_id: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub root: Root,
    pub rel: String,
    /// Hash del contenido que había antes de que 444CFG tocara el archivo (si existía).
    pub original: Option<String>,
    /// Pila de capas: la última es la que está en disco.
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub installs: Vec<Install>,
    pub records: BTreeMap<String, Record>,
}
