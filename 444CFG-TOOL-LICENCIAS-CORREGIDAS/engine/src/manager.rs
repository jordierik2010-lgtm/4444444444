use crate::exec::{self, ExecOptions};
use crate::import;
use crate::model::*;
use crate::planner::{self, Plan};
use crate::store::*;
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct FileCheck {
    pub root: Root,
    pub path: String,
    pub status: String,
}

pub struct Engine {
    pub store: Store,
    pub state: State,
}

impl Engine {
    /// Abre el almacén y recupera cualquier operación interrumpida.
    pub fn open(dir: &Path) -> Result<(Engine, Option<String>)> {
        let store = Store::open(dir)?;
        let rec = exec::recover(&store)?;
        let state = store.load_state()?;
        Ok((Engine { store, state }, rec))
    }

    pub fn import(&self, cat: Category, sources: &[PathBuf], name: Option<String>) -> Result<Package> {
        import::analyze(&self.store, cat, sources, name)
    }

    /// Importa mostrando el avance real (bytes guardados, bytes totales).
    pub fn import_with_progress(
        &self,
        cat: Category,
        sources: &[PathBuf],
        name: Option<String>,
        progress: &dyn Fn(u64, u64),
    ) -> Result<Package> {
        import::analyze_with(&self.store, cat, sources, name, Some(progress))
    }

    /// Fija manualmente el arma de una skin (cuando la detección automática no es concluyente o se equivocó).
    pub fn set_weapon_kind(&self, pkg_id: &str, kind: Option<&str>) -> Result<Package> {
        if self.state.installs.iter().any(|i| i.package_id == pkg_id) {
            bail!("el paquete está instalado; retíralo antes de cambiar el arma");
        }
        let mut p = self.store.load_package(pkg_id)?;
        if p.category != Category::Weapon {
            bail!("solo las skins de armas tienen tipo de arma");
        }
        match kind {
            Some(k) if k == crate::weapon::MK2 || k == crate::weapon::NINE_MM => {
                p.weapon_kind = Some(k.to_string());
                p.slot = Some(format!("weapon:{}", k));
                p.weapon_evidence.push(format!("Arma fijada manualmente: {}", crate::weapon::label(k)));
            }
            Some(_) => bail!("arma no válida: solo «mk2» o «9mm»"),
            None => {
                p.weapon_kind = None;
                let first = p.files.first().map(|f| f.rel.rsplit('/').next().unwrap_or("").to_lowercase()).unwrap_or_default();
                p.slot = Some(format!("weapon:file:{}", first));
                p.weapon_evidence.push("Arma sin identificar (fijado manualmente)".into());
            }
        }
        self.store.save_package(&p)?;
        Ok(p)
    }

    pub fn packages(&self) -> Result<Vec<Package>> {
        self.store.list_packages()
    }

    pub fn plan_install(&self, pkg_id: &str, roots: &Roots, link_to: Option<&str>) -> Result<Plan> {
        let pkg = self.store.load_package(pkg_id)?;
        planner::plan_install(&self.store, &self.state, &pkg, roots, link_to)
    }

    pub fn plan_uninstall(&self, id: &str, roots: &Roots) -> Result<Plan> {
        if !self.state.installs.iter().any(|i| i.id == id) {
            bail!("instalación no encontrada");
        }
        planner::plan_uninstall(&self.store, &self.state, id, roots)
    }

    pub fn apply(&mut self, plan: &Plan, roots: &Roots, opts: &ExecOptions) -> Result<()> {
        let st = exec::apply(&self.store, roots, plan, opts)?;
        self.state = st;
        Ok(())
    }

    pub fn delete_package(&self, id: &str) -> Result<()> {
        if self.state.installs.iter().any(|i| i.package_id == id) {
            bail!("el paquete está instalado; retíralo antes de eliminarlo de la biblioteca");
        }
        self.store.delete_package(id)
    }

    pub fn verify(&self, id: &str, roots: &Roots) -> Result<Vec<FileCheck>> {
        let inst = self.state.installs.iter().find(|i| i.id == id).ok_or_else(|| anyhow!("instalación no encontrada"))?;
        let mut out = Vec::new();
        for f in &inst.files {
            let status = match self.state.records.get(&f.key()) {
                None => "sin registro".to_string(),
                Some(rec) => match rec.layers.last() {
                    Some(t) if t.install_id == id => match hash_path(&roots.resolve(f.root, &f.rel)?)? {
                        None => "falta".to_string(),
                        Some(h) if h == t.sha256 => "ok".to_string(),
                        Some(_) => "modificado".to_string(),
                    },
                    Some(_) => "cubierto por otra capa".to_string(),
                    None => "sin capas".to_string(),
                },
            };
            out.push(FileCheck { root: f.root, path: f.rel.clone(), status });
        }
        Ok(out)
    }
}
