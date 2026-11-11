#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use engine::{Category, Engine, ExecOptions, Plan, Roots, SoundMode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize, Deserialize, Clone, Default)]
struct Settings {
    fivem_path: Option<String>,
    gta_path: Option<String>,
    backup_dir: Option<String>,
    sound_mode: Option<String>,
    #[serde(default)]
    ui: serde_json::Value,
}

/// Clave pública de licencias (64 hex). La clave PRIVADA no existe en este proyecto.
const PUBLIC_KEY: &str = include_str!("../public.key");

struct App {
    dir: PathBuf,
    settings: Mutex<Settings>,
    engine: Mutex<Option<Engine>>,
    licensed: AtomicBool,
}

type R<T> = Result<T, String>;
fn e2s<E: std::fmt::Display>(e: E) -> String {
    format!("{:#}", e)
}

fn log_error(app: &App, msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(app.dir.join("errors.log")) {
        let _ = writeln!(f, "[{}] {}", engine::store::now_secs(), msg.replace('\n', " | "));
    }
}

fn log_history(app: &App, action: &str, name: &str, category: &str, ok: bool) {
    let p = app.dir.join("history.json");
    let mut v: Vec<serde_json::Value> = fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    v.push(serde_json::json!({"ts": engine::store::now_secs(), "action": action, "name": name, "category": category, "ok": ok}));
    let keep = v.len().saturating_sub(50);
    let v: Vec<_> = v.into_iter().skip(keep).collect();
    if let Ok(b) = serde_json::to_vec_pretty(&v) {
        let _ = engine::store::atomic_write(&p, &b);
    }
}

fn load_settings(dir: &Path) -> Settings {
    fs::read(dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn store_dir(app: &App) -> PathBuf {
    let s = app.settings.lock().unwrap();
    match &s.backup_dir {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => app.dir.join("store"),
    }
}

fn with_engine<T>(app: &App, f: impl FnOnce(&mut Engine) -> R<T>) -> R<T> {
    let mut g = app.engine.lock().unwrap();
    if g.is_none() {
        let (eng, _) = Engine::open(&store_dir(app)).map_err(e2s)?;
        *g = Some(eng);
    }
    f(g.as_mut().unwrap())
}

fn roots_of(app: &App) -> R<Roots> {
    let s = app.settings.lock().unwrap();
    let fivem = s.fivem_path.clone().filter(|p| !p.is_empty()).ok_or("Configura la carpeta de FiveM en Ajustes")?;
    let fp = PathBuf::from(fivem);
    if !fp.is_dir() {
        return Err(format!("La carpeta de FiveM no existe: {}", fp.display()));
    }
    Ok(Roots {
        fivem: fp,
        gta: s.gta_path.clone().filter(|p| !p.is_empty()).map(PathBuf::from),
        sound_mode: if s.sound_mode.as_deref() == Some("mods_dir") { SoundMode::ModsDir } else { SoundMode::GtaSfx },
    })
}

fn fivem_running() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(o) = std::process::Command::new("tasklist").args(["/FO", "CSV", "/NH"]).creation_flags(0x0800_0000).output() {
            let t = String::from_utf8_lossy(&o.stdout).to_lowercase();
            return t.lines().any(|l| l.starts_with("\"fivem") || l.starts_with("\"gta5"));
        }
    }
    false
}

#[derive(Serialize)]
struct Detected {
    fivem: Option<String>,
    gta: Option<String>,
    note: String,
}

#[tauri::command(async)]
fn detect_paths() -> Detected {
    let mut note = String::new();
    let fivem = std::env::var("LOCALAPPDATA").ok().map(|l| PathBuf::from(l).join("FiveM").join("FiveM.app")).filter(|p| p.is_dir());
    let mut gta = None;
    match &fivem {
        None => note.push_str("No se encontró FiveM.app en %LOCALAPPDATA%\\FiveM. Selecciónala manualmente. "),
        Some(f) => {
            if let Ok(txt) = fs::read_to_string(f.join("CitizenFX.ini")) {
                for line in txt.lines() {
                    let l = line.trim();
                    if l.to_lowercase().starts_with("ivpath=") {
                        let p = PathBuf::from(l[7..].trim());
                        if p.join("GTA5.exe").is_file() || p.is_dir() {
                            gta = Some(p.to_string_lossy().to_string());
                        }
                    }
                }
                if gta.is_none() {
                    note.push_str("CitizenFX.ini no contiene una ruta IVPath válida. ");
                }
            } else {
                note.push_str("No se pudo leer CitizenFX.ini. ");
            }
        }
    }
    if gta.is_none() {
        let mut cands: Vec<PathBuf> = Vec::new();
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(pf) = std::env::var(var) {
                let pf = PathBuf::from(pf);
                cands.push(pf.join("Rockstar Games").join("Grand Theft Auto V"));
                cands.push(pf.join("Epic Games").join("GTAV"));
                cands.push(pf.join("Steam").join("steamapps").join("common").join("Grand Theft Auto V"));
            }
        }
        if let Some(found) = cands.into_iter().find(|c| c.join("GTA5.exe").is_file()) {
            gta = Some(found.to_string_lossy().to_string());
            note.push_str("GTA V encontrado en una ubicación habitual. ");
        }
    }
    Detected { fivem: fivem.map(|p| p.to_string_lossy().to_string()), gta, note }
}

#[tauri::command(async)]
fn get_settings(app: State<'_, App>) -> Settings {
    app.settings.lock().unwrap().clone()
}

#[tauri::command(async)]
fn save_settings(app: State<'_, App>, settings: Settings) -> R<()> {
    // Validar primero: si hay instalaciones activas, no cambiar la ubicación
    // del almacén ni mutar los ajustes en memoria antes de devolver error.
    let previous = app.settings.lock().unwrap().clone();
    let old_dir = previous.backup_dir.as_deref().filter(|s| !s.is_empty())
        .map(PathBuf::from).unwrap_or_else(|| app.dir.join("store"));
    let new_dir = settings.backup_dir.as_deref().filter(|s| !s.is_empty())
        .map(PathBuf::from).unwrap_or_else(|| app.dir.join("store"));
    if old_dir != new_dir {
        let has_installs = with_engine(&app, |e| Ok(!e.state.installs.is_empty()))?;
        if has_installs {
            return Err("No se puede cambiar la carpeta de copias mientras haya instalaciones gestionadas".into());
        }
    }
    let bytes = serde_json::to_vec_pretty(&settings).map_err(e2s)?;
    engine::store::atomic_write(&app.dir.join("settings.json"), &bytes).map_err(e2s)?;
    *app.settings.lock().unwrap() = settings;
    if old_dir != new_dir {
        *app.engine.lock().unwrap() = None;
    }
    Ok(())
}

fn require_license(app: &App) -> R<()> {
    // Volver a validar HWID, firma y caducidad antes de cada operación protegida.
    let status = evaluate_license(&app.dir);
    app.licensed.store(status.activated, Ordering::SeqCst);
    if status.activated {
        Ok(())
    } else {
        Err(status.error.unwrap_or_else(|| "Licencia no activada: activa 444CFG TOOL para usar esta función".to_string()))
    }
}

fn cat_of(s: &str) -> R<Category> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| format!("categoría desconocida: {}", s))
}

#[tauri::command(async)]
fn import_package(handle: AppHandle, app: State<'_, App>, category: String, paths: Vec<String>, name: Option<String>) -> R<engine::Package> {
    require_license(&app)?;
    let cat = cat_of(&category)?;
    let srcs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let h = handle.clone();
    let r = with_engine(&app, |e| {
        e.import_with_progress(cat, &srcs, name, &move |d, t| {
            let _ = h.emit("import-progress", serde_json::json!({ "done_bytes": d, "total_bytes": t }));
        })
        .map_err(e2s)
    });
    if let Err(m) = &r {
        log_error(&app, &format!("importar: {}", m));
    }
    r
}

#[tauri::command(async)]
fn list_packages(app: State<'_, App>) -> R<Vec<engine::Package>> {
    require_license(&app)?;
    with_engine(&app, |e| e.packages().map_err(e2s))
}

#[tauri::command(async)]
fn delete_package(app: State<'_, App>, id: String) -> R<()> {
    require_license(&app)?;
    with_engine(&app, |e| e.delete_package(&id).map_err(e2s))
}

#[tauri::command(async)]
fn list_installs(app: State<'_, App>) -> R<Vec<engine::Install>> {
    require_license(&app)?;
    with_engine(&app, |e| Ok(e.state.installs.clone()))
}

#[tauri::command(async)]
fn plan_install(app: State<'_, App>, package_id: String, link_to: Option<String>) -> R<Plan> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    with_engine(&app, |e| e.plan_install(&package_id, &roots, link_to.as_deref()).map_err(e2s))
}

#[tauri::command(async)]
fn plan_uninstall(app: State<'_, App>, id: String) -> R<Plan> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    with_engine(&app, |e| e.plan_uninstall(&id, &roots).map_err(e2s))
}

fn run_plan(handle: &AppHandle, app: &App, plan: Plan, roots: Roots, confirmed: bool, ack: bool, label: (&str, String, String)) -> R<()> {
    let h = handle.clone();
    let opts = ExecOptions {
        confirmed_unknown: confirmed,
        ack_unverified: ack,
        fivem_running: fivem_running(),
        on_progress: Some(Arc::new(move |p: engine::Progress| {
            let _ = h.emit("progress", p);
        })),
        ..Default::default()
    };
    let r = with_engine(app, |e| e.apply(&plan, &roots, &opts).map_err(e2s));
    log_history(app, label.0, &label.1, &label.2, r.is_ok());
    if let Err(m) = &r {
        log_error(app, &format!("{}: {}", label.0, m));
    }
    r
}

#[tauri::command(async)]
fn apply_install(handle: AppHandle, app: State<'_, App>, package_id: String, link_to: Option<String>, confirmed_unknown: bool, ack_unverified: bool) -> R<()> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    // Se vuelve a planificar justo antes de ejecutar: nunca se confía en un plan antiguo.
    let (plan, name, cat) = with_engine(&app, |e| {
        let p = e.plan_install(&package_id, &roots, link_to.as_deref()).map_err(e2s)?;
        let pkg = e.store.load_package(&package_id).map_err(e2s)?;
        Ok((p, pkg.name, format!("{:?}", pkg.category)))
    })?;
    run_plan(&handle, &app, plan, roots, confirmed_unknown, ack_unverified, ("instalar", name, cat))
}

#[tauri::command(async)]
fn apply_uninstall(handle: AppHandle, app: State<'_, App>, id: String, confirmed_unknown: bool) -> R<()> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    let (plan, name, cat) = with_engine(&app, |e| {
        let p = e.plan_uninstall(&id, &roots).map_err(e2s)?;
        let i = e.state.installs.iter().find(|i| i.id == id).ok_or("instalación no encontrada")?;
        Ok((p, i.name.clone(), format!("{:?}", i.category)))
    })?;
    run_plan(&handle, &app, plan, roots, confirmed_unknown, false, ("retirar", name, cat))
}

#[tauri::command(async)]
fn verify_install(app: State<'_, App>, id: String) -> R<Vec<engine::FileCheck>> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    with_engine(&app, |e| e.verify(&id, &roots).map_err(e2s))
}

#[tauri::command(async)]
fn open_install_location(app: State<'_, App>, id: String) -> R<()> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    let p = with_engine(&app, |e| {
        let i = e.state.installs.iter().find(|i| i.id == id).ok_or("instalación no encontrada")?;
        let f = i.files.first().ok_or("la instalación no tiene archivos")?;
        roots.resolve(f.root, &f.rel).map_err(e2s)
    })?;
    reveal(&p)
}

/// Abre el Explorador de Windows seleccionando el archivo (o la carpeta).
fn reveal(p: &Path) -> R<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer").raw_arg(format!("/select,\"{}\"", p.display())).spawn().map_err(e2s)?;
    }
    #[cfg(not(windows))]
    {
        let _ = p;
    }
    Ok(())
}

#[derive(Serialize)]
struct Dashboard {
    fivem_running: bool,
    fivem_path: Option<String>,
    history: serde_json::Value,
}

#[tauri::command(async)]
fn dashboard(app: State<'_, App>) -> Dashboard {
    let history = fs::read(app.dir.join("history.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::json!([]));
    Dashboard { fivem_running: fivem_running(), fivem_path: app.settings.lock().unwrap().fivem_path.clone(), history }
}

#[tauri::command(async)]
fn export_errors(app: State<'_, App>, dest: String) -> R<()> {
    let src = app.dir.join("errors.log");
    if !src.is_file() {
        fs::write(&dest, "Sin errores registrados.\n").map_err(e2s)
    } else {
        fs::copy(&src, &dest).map(|_| ()).map_err(e2s)
    }
}

#[tauri::command(async)]
fn set_weapon_kind(app: State<'_, App>, package_id: String, kind: Option<String>) -> R<engine::Package> {
    require_license(&app)?;
    with_engine(&app, |e| e.set_weapon_kind(&package_id, kind.as_deref()).map_err(e2s))
}

// ---------- Comprobación de rutas ----------

#[derive(Serialize)]
struct PathCheck {
    label: String,
    path: Option<String>,
    ok: bool,
    detail: String,
}

fn writable_dir(dir: &Path) -> bool {
    let probe = dir.join(format!(".444perm-{}", engine::store::uniq()));
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[tauri::command(async)]
fn check_paths(app: State<'_, App>) -> Vec<PathCheck> {
    let s = app.settings.lock().unwrap().clone();
    let mut out = Vec::new();
    match s.fivem_path.as_deref().filter(|p| !p.is_empty()) {
        None => out.push(PathCheck { label: "FiveM.app".into(), path: None, ok: false, detail: "Sin configurar".into() }),
        Some(p) => {
            let pb = PathBuf::from(p);
            if !pb.is_dir() {
                out.push(PathCheck { label: "FiveM.app".into(), path: Some(p.into()), ok: false, detail: "La carpeta no existe".into() });
            } else {
                let mods = pb.join("mods");
                let detail = if !pb.join("citizen").is_dir() {
                    "Existe, pero no contiene «citizen»: ¿es la carpeta FiveM.app correcta?".to_string()
                } else if mods.is_dir() && !writable_dir(&mods) {
                    "Sin permiso de escritura en «mods»".to_string()
                } else if !mods.is_dir() && !writable_dir(&pb) {
                    "Sin permiso de escritura".to_string()
                } else {
                    "Correcta y con permiso de escritura".to_string()
                };
                let ok = detail.starts_with("Correcta");
                out.push(PathCheck { label: "FiveM.app".into(), path: Some(p.into()), ok, detail });
            }
        }
    }
    match s.gta_path.as_deref().filter(|p| !p.is_empty()) {
        None => out.push(PathCheck { label: "GTA V".into(), path: None, ok: false, detail: "Sin configurar (necesaria para Sound Packs en modo GTA V)".into() }),
        Some(p) => {
            let pb = PathBuf::from(p);
            let sfx = pb.join("x64").join("audio").join("sfx");
            let (ok, detail) = if !pb.is_dir() {
                (false, "La carpeta no existe".to_string())
            } else if !pb.join("GTA5.exe").is_file() {
                (false, "No se encuentra GTA5.exe: ¿es la carpeta de GTA V?".to_string())
            } else if !sfx.is_dir() {
                (false, "No existe x64\\audio\\sfx".to_string())
            } else if !writable_dir(&sfx) {
                (false, "Sin permiso de escritura: abre 444CFG TOOL como administrador o usa el modo «mods» en Sound Packs".to_string())
            } else {
                (true, "Correcta y con permiso de escritura".to_string())
            };
            out.push(PathCheck { label: "GTA V".into(), path: Some(p.into()), ok, detail });
        }
    }
    out
}

// ---------- Explorador de archivos ----------

fn guard_modify(app: &App) -> R<()> {
    require_license(app)?;
    if fivem_running() {
        return Err("FiveM o GTA V están en ejecución; ciérralos antes de modificar archivos".into());
    }
    Ok(())
}

#[tauri::command(async)]
fn fm_areas(app: State<'_, App>, section: String) -> R<Vec<engine::fm::Area>> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    Ok(engine::fm::areas(cat_of(&section)?, &roots))
}

#[tauri::command(async)]
fn fm_list(app: State<'_, App>, section: String, area: String, sub: String) -> R<engine::fm::Listing> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    let a = engine::fm::find_area(cat_of(&section)?, &roots, &area).map_err(e2s)?;
    with_engine(&app, |e| engine::fm::list(&e.state, &roots, &a, &sub).map_err(e2s))
}

#[tauri::command(async)]
fn fm_import(handle: AppHandle, app: State<'_, App>, section: String, area: String, sub: String, paths: Vec<String>, replace: bool) -> R<Vec<String>> {
    guard_modify(&app)?;
    let roots = roots_of(&app)?;
    let a = engine::fm::find_area(cat_of(&section)?, &roots, &area).map_err(e2s)?;
    let srcs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let h = handle.clone();
    let r = with_engine(&app, |e| {
        let mut cb = move |d: u64, t: u64| {
            let _ = h.emit("fm-progress", serde_json::json!({ "done_bytes": d, "total_bytes": t }));
        };
        engine::fm::import_files(&e.store, &e.state, &roots, &a, &sub, &srcs, replace, &mut cb).map_err(e2s)
    });
    if let Err(m) = &r {
        log_error(&app, &format!("explorador/importar: {}", m));
    }
    r
}

#[tauri::command(async)]
fn fm_replace(handle: AppHandle, app: State<'_, App>, section: String, area: String, sub: String, source: String) -> R<()> {
    guard_modify(&app)?;
    let roots = roots_of(&app)?;
    let a = engine::fm::find_area(cat_of(&section)?, &roots, &area).map_err(e2s)?;
    let h = handle.clone();
    let r = with_engine(&app, |e| {
        let mut cb = move |d: u64, t: u64| {
            let _ = h.emit("fm-progress", serde_json::json!({ "done_bytes": d, "total_bytes": t }));
        };
        engine::fm::replace_file(&e.store, &e.state, &roots, &a, &sub, Path::new(&source), &mut cb).map_err(e2s)
    });
    if let Err(m) = &r {
        log_error(&app, &format!("explorador/sustituir: {}", m));
    }
    r
}

#[tauri::command(async)]
fn fm_delete(app: State<'_, App>, section: String, area: String, subs: Vec<String>) -> R<Vec<String>> {
    guard_modify(&app)?;
    let roots = roots_of(&app)?;
    let a = engine::fm::find_area(cat_of(&section)?, &roots, &area).map_err(e2s)?;
    let r = with_engine(&app, |e| engine::fm::delete_items(&e.store, &e.state, &roots, &a, &subs).map_err(e2s));
    if let Err(m) = &r {
        log_error(&app, &format!("explorador/eliminar: {}", m));
    }
    r
}

#[tauri::command(async)]
fn fm_backups(app: State<'_, App>) -> R<Vec<engine::fm::FmEntry>> {
    require_license(&app)?;
    with_engine(&app, |e| Ok(engine::fm::backups(&e.store)))
}

#[tauri::command(async)]
fn fm_restore(app: State<'_, App>, id: String) -> R<String> {
    guard_modify(&app)?;
    let roots = roots_of(&app)?;
    with_engine(&app, |e| engine::fm::restore(&e.store, &e.state, &roots, &id).map_err(e2s))
}

#[tauri::command(async)]
fn fm_open(app: State<'_, App>, section: String, area: String, sub: String) -> R<()> {
    require_license(&app)?;
    let roots = roots_of(&app)?;
    let a = engine::fm::find_area(cat_of(&section)?, &roots, &area).map_err(e2s)?;
    let p = engine::fm::open_target(&roots, &a, &sub).map_err(e2s)?;
    reveal(&p)
}

// ---------- Licencia ----------

#[derive(Serialize, Deserialize, Clone)]
struct SavedLicense {
    key: String,
    hwid: String,
    activated_at: u64,
}

#[derive(Serialize)]
struct LicenseStatus {
    activated: bool,
    hwid: String,
    error: Option<String>,
}

fn machine_id() -> R<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let o = std::process::Command::new("reg")
            .args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| format!("No se pudo leer el identificador del equipo: {}", e))?;
        let t = String::from_utf8_lossy(&o.stdout).to_string();
        for line in t.lines() {
            if line.contains("MachineGuid") {
                if let Some(v) = line.split_whitespace().last() {
                    if v.len() >= 8 && v != "MachineGuid" {
                        return Ok(v.to_string());
                    }
                }
            }
        }
        Err("No se pudo leer el identificador del equipo (MachineGuid)".to_string())
    }
    #[cfg(not(windows))]
    {
        Ok(std::env::var("HOSTNAME").unwrap_or_else(|_| "equipo-no-windows".to_string()))
    }
}

fn current_hwid() -> R<String> {
    Ok(engine::license::hwid_from_machine_id(&machine_id()?))
}

fn license_path(dir: &Path) -> PathBuf {
    dir.join("license.json")
}

/// Comprueba la licencia guardada: se vuelve a verificar la firma con la clave pública y el HWID actual.
fn evaluate_license(dir: &Path) -> LicenseStatus {
    let hwid = match current_hwid() {
        Ok(h) => h,
        Err(e) => return LicenseStatus { activated: false, hwid: String::new(), error: Some(e) },
    };
    let saved: Option<SavedLicense> = fs::read(license_path(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok());
    match saved {
        None => LicenseStatus { activated: false, hwid, error: None },
        Some(l) => match engine::license::verify_key(PUBLIC_KEY, &hwid, &l.key) {
            Ok(()) => LicenseStatus { activated: true, hwid, error: None },
            Err(e) => LicenseStatus { activated: false, hwid, error: Some(format!("La licencia guardada ya no es válida: {}", e)) },
        },
    }
}

#[tauri::command(async)]
fn license_status(app: State<'_, App>) -> LicenseStatus {
    let st = evaluate_license(&app.dir);
    app.licensed.store(st.activated, Ordering::SeqCst);
    if let Err(e) = engine::license::parse_public_key(PUBLIC_KEY) {
        return LicenseStatus { activated: false, hwid: st.hwid, error: Some(e.to_string()) };
    }
    st
}

#[tauri::command(async)]
fn license_activate(app: State<'_, App>, key: String) -> R<LicenseStatus> {
    let hwid = current_hwid()?;
    engine::license::verify_key(PUBLIC_KEY, &hwid, &key).map_err(|e| e.to_string())?;
    let saved = SavedLicense { key: key.trim().to_string(), hwid: hwid.clone(), activated_at: engine::store::now_secs() };
    let bytes = serde_json::to_vec_pretty(&saved).map_err(e2s)?;
    engine::store::atomic_write(&license_path(&app.dir), &bytes).map_err(e2s)?;
    app.licensed.store(true, Ordering::SeqCst);
    Ok(LicenseStatus { activated: true, hwid, error: None })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            fs::create_dir_all(&dir)?;
            let settings = load_settings(&dir);
            let licensed = evaluate_license(&dir).activated;
            app.manage(App { dir, settings: Mutex::new(settings), engine: Mutex::new(None), licensed: AtomicBool::new(licensed) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            detect_paths, get_settings, save_settings, import_package, list_packages, delete_package, list_installs,
            plan_install, plan_uninstall, apply_install, apply_uninstall, verify_install, open_install_location,
            dashboard, export_errors, set_weapon_kind, check_paths, fm_areas, fm_list, fm_import, fm_replace, fm_delete,
            fm_backups, fm_restore, fm_open, license_status, license_activate
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar 444CFG TOOL");
}
