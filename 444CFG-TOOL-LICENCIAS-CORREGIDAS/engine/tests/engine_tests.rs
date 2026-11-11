//! Pruebas del motor sobre carpetas temporales. Nunca tocan GTA V ni FiveM reales.
use engine::*;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ---------- utilidades ----------
struct Env {
    _td: TempDir,
    fivem: PathBuf,
    gta: PathBuf,
    data: PathBuf,
    roots: Roots,
    eng: Engine,
}

fn env() -> Env {
    let td = tempfile::tempdir().unwrap();
    let fivem = td.path().join("FiveM.app");
    let gta = td.path().join("GTAV");
    let data = td.path().join("store");
    let zips = td.path().join("zips");
    fs::create_dir_all(fivem.join("mods")).unwrap();
    fs::create_dir_all(fivem.join("citizen/common/data/effects")).unwrap();
    fs::create_dir_all(gta.join("x64/audio/sfx")).unwrap();
    fs::create_dir_all(&zips).unwrap();
    let roots = Roots { fivem: fivem.clone(), gta: Some(gta.clone()), sound_mode: SoundMode::GtaSfx };
    let (eng, _) = Engine::open(&data).unwrap();
    Env { _td: td, fivem, gta, data, roots, eng }
}

fn put(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}
fn put_bytes(p: &Path, b: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, b).unwrap();
}
fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|_| "<no existe>".into())
}

fn zip_of(e: &Env, name: &str, entries: &[(&str, Vec<u8>)]) -> PathBuf {
    let path = e._td.path().join("zips").join(name);
    let f = fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(f);
    for (n, d) in entries {
        zw.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(d).unwrap();
    }
    zw.finish().unwrap();
    path
}

fn opts() -> ExecOptions {
    ExecOptions { confirmed_unknown: true, ack_unverified: true, ..Default::default() }
}

fn install(e: &mut Env, cat: Category, zip: &Path) -> Plan {
    let pkg = e.eng.import(cat, &[zip.to_path_buf()], None).unwrap();
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    assert!(plan.blockers.is_empty(), "bloqueos: {:?}", plan.blockers);
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    plan
}

fn try_plan(e: &Env, cat: Category, zip: &Path) -> Plan {
    let pkg = e.eng.import(cat, &[zip.to_path_buf()], None).unwrap();
    e.eng.plan_install(&pkg.id, &e.roots, None).unwrap()
}

fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, p: &Path, out: &mut BTreeMap<String, String>) {
        for ent in fs::read_dir(p).unwrap().flatten() {
            let path = ent.path();
            let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                out.insert(format!("{}/", rel), String::new());
                walk(base, &path, out);
            } else {
                out.insert(rel, engine::store::sha256_file(&path).unwrap());
            }
        }
    }
    let mut m = BTreeMap::new();
    walk(root, root, &mut m);
    m
}

// ---------- constructor de RPF7 mínimos (índice sin cifrar) ----------
enum N {
    F(String, Vec<u8>),
    D(String, Vec<N>),
    R(String, Vec<N>),
}
struct E {
    name: String,
    dir: Option<(u32, u32)>,
    data: Vec<u8>,
}
fn layout(entries: &mut Vec<E>, kids: &[N]) {
    let start = entries.len();
    for _ in kids {
        entries.push(E { name: String::new(), dir: None, data: vec![] });
    }
    for (i, k) in kids.iter().enumerate() {
        match k {
            N::F(n, d) => entries[start + i] = E { name: n.clone(), dir: None, data: d.clone() },
            N::R(n, kk) => entries[start + i] = E { name: n.clone(), dir: None, data: build_rpf(kk) },
            N::D(n, kk) => {
                let s = entries.len();
                entries[start + i] = E { name: n.clone(), dir: Some((s as u32, kk.len() as u32)), data: vec![] };
                layout(entries, kk);
            }
        }
    }
}
fn build_rpf(kids: &[N]) -> Vec<u8> {
    let mut entries = vec![E { name: String::new(), dir: Some((1, kids.len() as u32)), data: vec![] }];
    layout(&mut entries, kids);
    let mut names = vec![0u8];
    let mut offs = vec![0u16];
    for e in entries.iter().skip(1) {
        offs.push(names.len() as u16);
        names.extend_from_slice(e.name.as_bytes());
        names.push(0);
    }
    let n = entries.len();
    let head = 16 + n * 16 + names.len();
    let data_start = ((head + 511) / 512) * 512;
    let mut toc = vec![];
    let mut body: Vec<u8> = vec![];
    for (i, e) in entries.iter().enumerate() {
        toc.extend_from_slice(&offs[i].to_le_bytes());
        match e.dir {
            Some((s, c)) => {
                toc.extend_from_slice(&[0, 0, 0, 0xff, 0xff, 0x7f]);
                toc.extend_from_slice(&s.to_le_bytes());
                toc.extend_from_slice(&c.to_le_bytes());
            }
            None => {
                let block = ((data_start + body.len()) / 512) as u32;
                toc.extend_from_slice(&[0, 0, 0]);
                toc.extend_from_slice(&block.to_le_bytes()[0..3]);
                toc.extend_from_slice(&(e.data.len() as u32).to_le_bytes());
                toc.extend_from_slice(&0u32.to_le_bytes());
                body.extend_from_slice(&e.data);
                while body.len() % 512 != 0 {
                    body.push(0);
                }
            }
        }
    }
    let mut out = vec![];
    out.extend_from_slice(b"7FPR");
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&(names.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E45504Fu32.to_le_bytes());
    out.extend_from_slice(&toc);
    out.extend_from_slice(&names);
    out.resize(data_start, 0);
    out.extend_from_slice(&body);
    out
}
fn weapon_rpf(device: &str, dlc: &str, models: &[&str], tag: &str) -> Vec<u8> {
    let mut files: Vec<N> = models.iter().map(|m| N::F(format!("{}.ydr", m), tag.as_bytes().to_vec())).collect();
    files.push(N::F("w_at_pi_comp_1.ydr".into(), b"shared-attachment".to_vec()));
    let setup = format!("<setup><deviceName>{}</deviceName></setup>", device);
    let asm = format!(r#"<package><add source="SKin.rpf">update\x64\dlcpacks\{}\dlc.rpf</add></package>"#, dlc);
    build_rpf(&[
        N::F("assembly.xml".into(), asm.into_bytes()),
        N::D(
            "content".into(),
            vec![N::R(
                "SKin.rpf".into(),
                vec![
                    N::F("content.xml".into(), b"<c/>".to_vec()),
                    N::F("setup2.xml".into(), setup.into_bytes()),
                    N::D("x64".into(), vec![N::R("SKin_final.rpf".into(), files)]),
                ],
            )],
        ),
    ])
}

// ---------- PRUEBA 1: Citizen A → B ----------
#[test]
fn t01_citizen_a_to_b_keeps_independent_and_unknown() {
    let mut e = env();
    put(&e.fivem.join("citizen/common/data/effects/weaponfx.dat"), "ORIG");
    put(&e.fivem.join("citizen/desconocido.txt"), "MIO");
    let ind = zip_of(&e, "ind.zip", &[("mods/independiente.bin", b"IND".to_vec())]);
    install(&mut e, Category::Mod, &ind);
    let a = zip_of(&e, "a.zip", &[
        ("citizen/common/data/effects/weaponfx.dat", b"A_FX".to_vec()),
        ("citizen/solo_a.txt", b"A".to_vec()),
        ("citizen/compartido.xml", b"S".to_vec()),
        ("citizen/dif.xml", b"A".to_vec()),
    ]);
    let pkg = e.eng.import(Category::Citizen, &[a.clone()], None).unwrap();
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    assert_eq!(plan.unknown_overwrites.len(), 1, "weaponfx.dat previo exige confirmación");
    assert!(e.eng.apply(&plan, &e.roots, &ExecOptions::default()).is_err(), "sin confirmar no debe aplicar");
    assert_eq!(read(&e.fivem.join("citizen/common/data/effects/weaponfx.dat")), "ORIG");
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    assert_eq!(read(&e.fivem.join("citizen/common/data/effects/weaponfx.dat")), "A_FX");
    let b = zip_of(&e, "b.zip", &[
        ("citizen/compartido.xml", b"S".to_vec()),
        ("citizen/dif.xml", b"B".to_vec()),
        ("citizen/solo_b.txt", b"B".to_vec()),
    ]);
    let plan = install(&mut e, Category::Citizen, &b);
    assert_eq!(plan.summary.restore_originals, 1);
    assert!(!e.fivem.join("citizen/solo_a.txt").exists(), "sin restos de A");
    assert_eq!(read(&e.fivem.join("citizen/common/data/effects/weaponfx.dat")), "ORIG");
    assert_eq!(read(&e.fivem.join("citizen/dif.xml")), "B");
    assert_eq!(read(&e.fivem.join("citizen/solo_b.txt")), "B");
    assert_eq!(read(&e.fivem.join("citizen/compartido.xml")), "S");
    assert_eq!(read(&e.fivem.join("mods/independiente.bin")), "IND");
    assert_eq!(read(&e.fivem.join("citizen/desconocido.txt")), "MIO");
    assert_eq!(e.eng.state.installs.len(), 2);
}

// ---------- PRUEBAS 2, 3 y 12: armas, RPF anidado ----------
#[test]
fn t12_nested_rpf_is_analysed() {
    let e = env();
    let rpf = weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2", "w_pi_pistolmk2_mag1"], "x");
    let z = zip_of(&e, "mk2.zip", &[("MK2.rpf", rpf)]);
    let p = e.eng.import(Category::Weapon, &[z], None).unwrap();
    assert_eq!(p.slot.as_deref(), Some("weapon:mk2"));
    assert_eq!(p.weapon_kind.as_deref(), Some("mk2"));
    assert!(p.mount_ids.contains(&"device:mod_template".to_string()));
    assert!(p.mount_ids.contains(&"dlc:update/x64/dlcpacks/skin/dlc.rpf".to_string()));
    assert!(p.contents.iter().any(|c| c.contains("w_pi_pistolmk2.ydr")));
}

#[test]
fn t02_ap_and_mk2_coexist() {
    let mut e = env();
    let ap = zip_of(&e, "ap.zip", &[("AP.rpf", weapon_rpf("ap_dev", "ap", &["w_pi_appistol"], "ap"))]);
    let mk2 = zip_of(&e, "mk2.zip", &[("MK2.rpf", weapon_rpf("mk2_dev", "mk2", &["w_pi_pistolmk2"], "mk2"))]);
    install(&mut e, Category::Weapon, &ap);
    install(&mut e, Category::Weapon, &mk2);
    assert!(e.fivem.join("mods/AP.rpf").exists() && e.fivem.join("mods/MK2.rpf").exists());
    assert_eq!(e.eng.state.installs.len(), 2);
}

#[test]
fn t03_second_mk2_skin_replaces_only_the_first() {
    let mut e = env();
    let ap = zip_of(&e, "ap.zip", &[("AP.rpf", weapon_rpf("ap_dev", "ap", &["w_pi_appistol"], "ap"))]);
    let a = zip_of(&e, "mk2a.zip", &[("MK2_A.rpf", weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "A"))]);
    let b = zip_of(&e, "mk2b.zip", &[("MK2_B.rpf", weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "B"))]);
    install(&mut e, Category::Weapon, &ap);
    install(&mut e, Category::Weapon, &a);
    install(&mut e, Category::Weapon, &b);
    assert!(!e.fivem.join("mods/MK2_A.rpf").exists(), "la skin anterior se retira");
    assert!(e.fivem.join("mods/MK2_B.rpf").exists());
    assert!(e.fivem.join("mods/AP.rpf").exists(), "la AP no se toca");
}

#[test]
fn t04_mk2_and_9mm_are_separate_weapons_and_shared_mount_needs_ack() {
    let mut e = env();
    let mk2 = zip_of(&e, "mk2.zip", &[("MK2.rpf", weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "m"))]);
    let mm = zip_of(&e, "9mm.zip", &[("MK2_9mm.rpf", weapon_rpf("mod_template", "skin", &["w_pi_pistol"], "p"))]);
    install(&mut e, Category::Weapon, &mk2);
    let pkg = e.eng.import(Category::Weapon, &[mm.clone()], None).unwrap();
    assert_eq!(pkg.weapon_kind.as_deref(), Some("9mm"), "el 9mm se identifica por contenido aunque el archivo se llame MK2_9mm");
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert!(plan.needs_ack_unverified && !plan.ack_notes.is_empty(), "montaje compartido: exige confirmación expresa");
    let before = snapshot(&e.fivem);
    let no_ack = ExecOptions { confirmed_unknown: true, ack_unverified: false, ..Default::default() };
    assert!(e.eng.apply(&plan, &e.roots, &no_ack).is_err());
    assert_eq!(before, snapshot(&e.fivem), "sin confirmar no se toca nada");
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    assert!(e.fivem.join("mods/MK2.rpf").exists() && e.fivem.join("mods/MK2_9mm.rpf").exists());
    // Una skin MK2 nueva sustituye solo a la MK2; la 9mm no se toca.
    let nine_before = fs::read(e.fivem.join("mods/MK2_9mm.rpf")).unwrap();
    let mk2b = zip_of(&e, "mk2b.zip", &[("MK2_B.rpf", weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "m2"))]);
    let plan = try_plan(&e, Category::Weapon, &mk2b);
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    assert!(!e.fivem.join("mods/MK2.rpf").exists(), "la skin MK2 anterior se retira");
    assert!(e.fivem.join("mods/MK2_B.rpf").exists());
    assert_eq!(fs::read(e.fivem.join("mods/MK2_9mm.rpf")).unwrap(), nine_before, "la 9mm no se toca");
    assert_eq!(e.eng.state.installs.len(), 2);
}

// ---------- PRUEBAS 4 y 5: Blood FX y capas ----------
#[test]
fn t05_blood_fx_stacks_over_citizen_and_switch_keeps_the_rest() {
    let mut e = env();
    let c1 = zip_of(&e, "c1.zip", &[("citizen/common/data/effects/bloodfx.dat", b"C1".to_vec()), ("citizen/x.txt", b"x".to_vec())]);
    install(&mut e, Category::Citizen, &c1);
    let ba = zip_of(&e, "ba.zip", &[("bloodfx.dat", b"BA".to_vec()), ("444.rpf", b"RPF-A".to_vec())]);
    install(&mut e, Category::BloodFx, &ba);
    let dat = e.fivem.join("citizen/common/data/effects/bloodfx.dat");
    assert_eq!(read(&dat), "BA", "Blood FX se apila sobre el Citizen");
    let bb = zip_of(&e, "bb.zip", &[("bloodfx.dat", b"BB".to_vec())]);
    install(&mut e, Category::BloodFx, &bb);
    assert_eq!(read(&dat), "BB");
    assert!(!e.fivem.join("mods/444.rpf").exists(), "el RPF asociado del Blood FX anterior se retira");
    // Cambiar Citizen no pisa la capa de Blood FX
    let c2 = zip_of(&e, "c2.zip", &[("citizen/common/data/effects/bloodfx.dat", b"C2".to_vec())]);
    install(&mut e, Category::Citizen, &c2);
    assert_eq!(read(&dat), "BB");
    assert!(!e.fivem.join("citizen/x.txt").exists());
    // Retirar Blood FX deja visible el del Citizen
    let id = e.eng.state.installs.iter().find(|i| i.category == Category::BloodFx).unwrap().id.clone();
    let plan = e.eng.plan_uninstall(&id, &e.roots).unwrap();
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    assert_eq!(read(&dat), "C2");
}

// ---------- PRUEBA 5: Tracers ----------
#[test]
fn t05b_tracer_a_then_b_only_replaces_tracer() {
    let mut e = env();
    put(&e.fivem.join("citizen/common/data/timecycle/timecycle_mods_1.xml"), "ORIG1");
    let a = zip_of(&e, "ta.zip", &[("timecycle_mods_4.xml", b"YELLOW".to_vec())]);
    let b = zip_of(&e, "tb.zip", &[("timecycle_mods_4.xml", b"RED".to_vec())]);
    install(&mut e, Category::Tracer, &a);
    install(&mut e, Category::Tracer, &b);
    assert_eq!(read(&e.fivem.join("citizen/common/data/timecycle/timecycle_mods_4.xml")), "RED");
    assert_eq!(read(&e.fivem.join("citizen/common/data/timecycle/timecycle_mods_1.xml")), "ORIG1");
}

// ---------- PRUEBAS 6 y 7: Sound Packs ----------
#[test]
fn t06_t07_sound_pack_switch_and_restore_previous_state() {
    let mut e = env();
    let sfx = e.gta.join("x64/audio/sfx");
    put(&sfx.join("RESIDENT.rpf"), "RES_ORIGINAL");
    put(&sfx.join("OTRO.rpf"), "AJENO");
    let a = zip_of(&e, "sa.zip", &[("sp/RESIDENT.rpf", b"RES_A".to_vec()), ("sp/WEAPONS_PLAYER.rpf", b"WEAP_A".to_vec())]);
    let b = zip_of(&e, "sb.zip", &[("RESIDENT.rpf", b"RES_B".to_vec()), ("WEAPONS_PLAYER.rpf", b"WEAP_B".to_vec())]);
    let ind = zip_of(&e, "ind.zip", &[("mods/ind.bin", b"I".to_vec())]);
    install(&mut e, Category::Mod, &ind);
    install(&mut e, Category::SoundPack, &a);
    install(&mut e, Category::SoundPack, &b);
    assert_eq!(read(&sfx.join("RESIDENT.rpf")), "RES_B");
    assert_eq!(read(&sfx.join("WEAPONS_PLAYER.rpf")), "WEAP_B");
    assert_eq!(read(&sfx.join("OTRO.rpf")), "AJENO");
    assert_eq!(read(&e.fivem.join("mods/ind.bin")), "I");
    let id = e.eng.state.installs.iter().find(|i| i.category == Category::SoundPack).unwrap().id.clone();
    let plan = e.eng.plan_uninstall(&id, &e.roots).unwrap();
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    assert_eq!(read(&sfx.join("RESIDENT.rpf")), "RES_ORIGINAL", "se restaura el estado previo");
    assert!(!sfx.join("WEAPONS_PLAYER.rpf").exists(), "no existía antes: se retira solo ese archivo");
    assert_eq!(read(&sfx.join("OTRO.rpf")), "AJENO");
}

#[test]
fn t06b_sound_pack_in_mods_dir_never_touches_gta() {
    let mut e = env();
    e.roots.sound_mode = SoundMode::ModsDir;
    put(&e.gta.join("x64/audio/sfx/RESIDENT.rpf"), "ORIG");
    let before = snapshot(&e.gta);
    let a = zip_of(&e, "sa.zip", &[("RESIDENT.rpf", b"A".to_vec())]);
    install(&mut e, Category::SoundPack, &a);
    assert_eq!(before, snapshot(&e.gta));
    assert_eq!(read(&e.fivem.join("mods/x64/audio/sfx/RESIDENT.rpf")), "A");
}

#[test]
fn t06c_loose_awc_is_rejected_with_reason() {
    let e = env();
    let z = zip_of(&e, "s.zip", &[("sonido.awc", b"x".to_vec())]);
    let err = e.eng.import(Category::SoundPack, &[z], None).unwrap_err().to_string();
    assert!(err.contains("sonido.awc") && err.contains("no se puede instalar directamente"));
}

// ---------- PRUEBA 8: interrupción ----------
#[test]
fn t08_failure_rolls_back_everything() {
    let mut e = env();
    put(&e.fivem.join("citizen/a.txt"), "ORIG_A");
    let z = zip_of(&e, "c.zip", &[("citizen/a.txt", b"N".to_vec()), ("citizen/nueva/b.txt", b"B".to_vec()), ("citizen/c.txt", b"C".to_vec())]);
    let pkg = e.eng.import(Category::Citizen, &[z], None).unwrap();
    let before = snapshot(&e.fivem);
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    for n in 0..=plan.ops.len() {
        let o = ExecOptions { fail_after_ops: Some(n), ..opts() };
        assert!(e.eng.apply(&plan, &e.roots, &o).is_err());
        assert_eq!(before, snapshot(&e.fivem), "estado idéntico tras fallo en operación {}", n);
        assert!(e.eng.state.installs.is_empty());
        assert!(!e.data.join("journal.json").exists());
    }
}

#[test]
fn t08b_crash_is_recovered_on_next_start() {
    let mut e = env();
    put(&e.fivem.join("citizen/a.txt"), "ORIG_A");
    let z = zip_of(&e, "c.zip", &[("citizen/a.txt", b"N".to_vec()), ("citizen/nueva/b.txt", b"B".to_vec()), ("citizen/c.txt", b"C".to_vec())]);
    let pkg = e.eng.import(Category::Citizen, &[z], None).unwrap();
    let before = snapshot(&e.fivem);
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    let o = ExecOptions { fail_after_ops: Some(2), skip_rollback_on_fail: true, ..opts() };
    assert!(e.eng.apply(&plan, &e.roots, &o).is_err());
    assert_ne!(before, snapshot(&e.fivem), "la caída deja la instalación a medias");
    let (eng2, msg) = Engine::open(&e.data).unwrap();
    assert!(msg.is_some());
    assert_eq!(before, snapshot(&e.fivem));
    assert!(eng2.state.installs.is_empty());
}

// ---------- PRUEBA 9: conflictos ----------
#[test]
fn t09_two_mods_same_file_blocked_without_damage() {
    let mut e = env();
    let m1 = zip_of(&e, "m1.zip", &[("mods/compartido.bin", b"1".to_vec())]);
    let m2 = zip_of(&e, "m2.zip", &[("mods/compartido.bin", b"2".to_vec()), ("mods/otro.bin", b"o".to_vec())]);
    install(&mut e, Category::Mod, &m1);
    let before = snapshot(&e.fivem);
    let plan = try_plan(&e, Category::Mod, &m2);
    assert!(!plan.blockers.is_empty());
    assert!(e.eng.apply(&plan, &e.roots, &opts()).is_err());
    assert_eq!(before, snapshot(&e.fivem));
}

#[test]
fn t09b_identical_shared_file_survives_until_last_owner_leaves() {
    let mut e = env();
    let m1 = zip_of(&e, "m1.zip", &[("mods/compartido.bin", b"1".to_vec()), ("mods/uno.bin", b"u".to_vec())]);
    let m2 = zip_of(&e, "m2.zip", &[("mods/compartido.bin", b"1".to_vec()), ("mods/dos.bin", b"d".to_vec())]);
    install(&mut e, Category::Mod, &m1);
    install(&mut e, Category::Mod, &m2);
    let id1 = e.eng.state.installs[0].id.clone();
    let id2 = e.eng.state.installs[1].id.clone();
    let p = e.eng.plan_uninstall(&id1, &e.roots).unwrap();
    e.eng.apply(&p, &e.roots, &opts()).unwrap();
    assert!(e.fivem.join("mods/compartido.bin").exists() && !e.fivem.join("mods/uno.bin").exists());
    let p = e.eng.plan_uninstall(&id2, &e.roots).unwrap();
    e.eng.apply(&p, &e.roots, &opts()).unwrap();
    assert!(!e.fivem.join("mods/compartido.bin").exists() && !e.fivem.join("mods/dos.bin").exists());
}

// ---------- PRUEBA 10: persistencia ----------
#[test]
fn t10_state_survives_restart() {
    let mut e = env();
    let m = zip_of(&e, "m.zip", &[("mods/a.bin", b"a".to_vec())]);
    install(&mut e, Category::Mod, &m);
    let (eng2, msg) = Engine::open(&e.data).unwrap();
    assert!(msg.is_none());
    assert_eq!(eng2.state.installs.len(), 1);
    assert_eq!(eng2.packages().unwrap().len(), 1);
}

// ---------- PRUEBA 11: ZIP malicioso ----------
#[test]
fn t11_malicious_zip_is_rejected_and_nothing_is_stored() {
    let e = env();
    for evil in ["../evil.txt", "citizen/../../evil.txt", "/abs/evil.txt", "C:/evil.txt", "citizen/..\\..\\evil.txt"] {
        let z = zip_of(&e, "evil.zip", &[("citizen/ok.txt", b"ok".to_vec()), (evil, b"x".to_vec())]);
        assert!(e.eng.import(Category::Citizen, &[z], None).is_err(), "debe rechazar {}", evil);
    }
    assert!(e.eng.packages().unwrap().is_empty());
    assert!(!e._td.path().join("evil.txt").exists());
}

// ---------- Seguridad ----------
#[test]
fn s01_externally_modified_file_blocks_uninstall() {
    let mut e = env();
    let m = zip_of(&e, "m.zip", &[("mods/a.bin", b"a".to_vec()), ("mods/b.bin", b"b".to_vec())]);
    install(&mut e, Category::Mod, &m);
    put(&e.fivem.join("mods/a.bin"), "EDITADO");
    let id = e.eng.state.installs[0].id.clone();
    let plan = e.eng.plan_uninstall(&id, &e.roots).unwrap();
    assert!(!plan.blockers.is_empty());
    assert!(e.eng.apply(&plan, &e.roots, &opts()).is_err());
    assert_eq!(read(&e.fivem.join("mods/a.bin")), "EDITADO");
    assert_eq!(read(&e.fivem.join("mods/b.bin")), "b");
    let checks = e.eng.verify(&id, &e.roots).unwrap();
    assert!(checks.iter().any(|c| c.status == "modificado"));
}

#[test]
fn s02_dirs_removed_only_if_created_by_mod_and_empty() {
    let mut e = env();
    put(&e.fivem.join("mods/existente/viejo.txt"), "v");
    let m = zip_of(&e, "m.zip", &[("mods/existente/nuevo.bin", b"n".to_vec()), ("mods/propia/a.bin", b"a".to_vec())]);
    install(&mut e, Category::Mod, &m);
    put(&e.fivem.join("mods/propia/ajeno.txt"), "no es del mod");
    let id = e.eng.state.installs[0].id.clone();
    let p = e.eng.plan_uninstall(&id, &e.roots).unwrap();
    e.eng.apply(&p, &e.roots, &opts()).unwrap();
    assert!(e.fivem.join("mods/existente/viejo.txt").exists());
    assert!(e.fivem.join("mods/propia/ajeno.txt").exists(), "carpeta con archivo desconocido: se conserva");
    assert!(!e.fivem.join("mods/propia/a.bin").exists());
}

#[test]
fn s03_linked_mod_is_removed_with_its_citizen_but_independent_stays() {
    let mut e = env();
    let c = zip_of(&e, "c.zip", &[("citizen/a.txt", b"a".to_vec())]);
    install(&mut e, Category::Citizen, &c);
    let cid = e.eng.state.installs[0].id.clone();
    let linked = zip_of(&e, "l.zip", &[("mods/vinculado.bin", b"v".to_vec())]);
    let indep = zip_of(&e, "i.zip", &[("mods/indep.bin", b"i".to_vec())]);
    let pl = e.eng.import(Category::Mod, &[linked], None).unwrap();
    let plan = e.eng.plan_install(&pl.id, &e.roots, Some(&cid)).unwrap();
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    install(&mut e, Category::Mod, &indep);
    let c2 = zip_of(&e, "c2.zip", &[("citizen/b.txt", b"b".to_vec())]);
    install(&mut e, Category::Citizen, &c2);
    assert!(!e.fivem.join("mods/vinculado.bin").exists());
    assert!(e.fivem.join("mods/indep.bin").exists());
}

#[test]
fn s04_fivem_running_blocks_apply() {
    let mut e = env();
    let m = zip_of(&e, "m.zip", &[("mods/a.bin", b"a".to_vec())]);
    let pkg = e.eng.import(Category::Mod, &[m], None).unwrap();
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    let o = ExecOptions { fivem_running: true, ..opts() };
    assert!(e.eng.apply(&plan, &e.roots, &o).is_err());
    assert!(!e.fivem.join("mods/a.bin").exists());
}

#[test]
fn s05_path_sanitizer() {
    use engine::pathsafe::sanitize_rel;
    for bad in ["..", "a/../b", "/x", "C:/x", "con.txt", "a/nul", "a/b.", "a/b ", "a/<b>", ""] {
        assert!(sanitize_rel(bad).is_err(), "{}", bad);
    }
    assert_eq!(sanitize_rel("a\\b/c.txt").unwrap(), "a/b/c.txt");
}

// ---------- MK2 / 9mm: skins instaladas a mano ----------
#[test]
fn n01_manual_mk2_skin_is_detected_confirmed_backed_up_and_restored() {
    let mut e = env();
    let manual = weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "MANUAL");
    let ap = weapon_rpf("ap_dev", "ap", &["w_pi_appistol"], "AP");
    let nine = weapon_rpf("mod_template", "skin", &["w_pi_pistol"], "NINE");
    put_bytes(&e.fivem.join("mods/mk2_manual.rpf"), &manual);
    put_bytes(&e.fivem.join("mods/AP_manual.rpf"), &ap);
    put_bytes(&e.fivem.join("mods/9mm_manual.rpf"), &nine);
    let new = zip_of(&e, "mk2new.zip", &[("MK2_NEW.rpf", weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "NEW"))]);
    let pkg = e.eng.import(Category::Weapon, &[new], None).unwrap();
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert_eq!(plan.manual_conflicts.len(), 1, "solo la skin MK2 manual es la misma arma: {:?}", plan.manual_conflicts);
    assert_eq!(plan.manual_conflicts[0].rel, "mods/mk2_manual.rpf");
    assert_eq!(plan.ack_notes.len(), 1, "la 9mm manual comparte montaje: aviso, no se toca");
    let before = snapshot(&e.fivem);
    let no_confirm = ExecOptions { ack_unverified: true, ..Default::default() };
    assert!(e.eng.apply(&plan, &e.roots, &no_confirm).is_err(), "pide confirmación antes de modificar la skin manual");
    assert_eq!(before, snapshot(&e.fivem));
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    assert!(!e.fivem.join("mods/mk2_manual.rpf").exists(), "la skin manual se retira");
    assert!(e.fivem.join("mods/MK2_NEW.rpf").exists());
    assert_eq!(fs::read(e.fivem.join("mods/AP_manual.rpf")).unwrap(), ap, "otra arma intacta");
    assert_eq!(fs::read(e.fivem.join("mods/9mm_manual.rpf")).unwrap(), nine, "la 9mm intacta");
    assert_eq!(e.eng.state.installs[0].displaced.len(), 1);
    let id = e.eng.state.installs[0].id.clone();
    let un = e.eng.plan_uninstall(&id, &e.roots).unwrap();
    assert!(un.blockers.is_empty(), "{:?}", un.blockers);
    e.eng.apply(&un, &e.roots, &opts()).unwrap();
    assert_eq!(fs::read(e.fivem.join("mods/mk2_manual.rpf")).unwrap(), manual, "se devuelve la skin original");
    assert!(!e.fivem.join("mods/MK2_NEW.rpf").exists());
    assert_eq!(fs::read(e.fivem.join("mods/AP_manual.rpf")).unwrap(), ap);
}

#[test]
fn n02_replacing_a_managed_skin_keeps_the_manual_backup_for_restore() {
    let mut e = env();
    let manual = weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "MANUAL");
    put_bytes(&e.fivem.join("mods/mk2_manual.rpf"), &manual);
    let a = zip_of(&e, "a.zip", &[("MK2_A.rpf", weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "A"))]);
    let b = zip_of(&e, "b.zip", &[("MK2_B.rpf", weapon_rpf("mod_template", "skin", &["w_pi_pistolmk2"], "B"))]);
    install(&mut e, Category::Weapon, &a);
    assert!(!e.fivem.join("mods/mk2_manual.rpf").exists());
    install(&mut e, Category::Weapon, &b);
    assert!(!e.fivem.join("mods/MK2_A.rpf").exists() && e.fivem.join("mods/MK2_B.rpf").exists());
    assert_eq!(e.eng.state.installs.len(), 1);
    assert_eq!(e.eng.state.installs[0].displaced.len(), 1, "la copia de la skin manual se conserva al cambiar A→B");
    let id = e.eng.state.installs[0].id.clone();
    let un = e.eng.plan_uninstall(&id, &e.roots).unwrap();
    e.eng.apply(&un, &e.roots, &opts()).unwrap();
    assert_eq!(fs::read(e.fivem.join("mods/mk2_manual.rpf")).unwrap(), manual);
    assert!(!e.fivem.join("mods/MK2_B.rpf").exists());
}

#[test]
fn n02b_set_weapon_kind_manually_and_reject_invalid() {
    let e = env();
    let z = zip_of(&e, "x.zip", &[("Rara.rpf", weapon_rpf("dev", "d", &["w_pi_algo"], "x"))]);
    let p = e.eng.import(Category::Weapon, &[z], None).unwrap();
    assert!(p.weapon_kind.is_none() && !p.weapon_evidence.is_empty());
    let p = e.eng.set_weapon_kind(&p.id, Some("mk2")).unwrap();
    assert_eq!(p.slot.as_deref(), Some("weapon:mk2"));
    assert!(e.eng.set_weapon_kind(&p.id, Some("ak47")).is_err());
}

// ---------- Citizen con RPF ----------
#[test]
fn n03_citizen_package_with_rpf_and_wrapper_folder_installs_and_restores() {
    let mut e = env();
    let rpf = weapon_rpf("torre_dev", "torre", &["w_pi_x"], "t");
    let orig_mod = b"RPF-ORIGINAL-MIO".to_vec();
    put_bytes(&e.fivem.join("mods/torreeifel.rpf"), &orig_mod);
    let z = zip_of(&e, "cit.zip", &[
        ("torreeifel.rpf", rpf.clone()),
        ("Mi Citizen/citizen/common/data/a.xml", b"A".to_vec()),
        ("Mi Citizen/rpf/otro.rpf", rpf.clone()),
    ]);
    let pkg = e.eng.import(Category::Citizen, &[z], None).unwrap();
    let dests: Vec<String> = pkg.files.iter().map(|f| f.rel.clone()).collect();
    assert!(dests.contains(&"citizen/common/data/a.xml".to_string()), "{:?}", dests);
    assert!(dests.contains(&"mods/torreeifel.rpf".to_string()) && dests.contains(&"mods/otro.rpf".to_string()), "{:?}", dests);
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert_eq!(plan.unknown_overwrites, vec!["mods/torreeifel.rpf".to_string()], "el RPF previo exige confirmación");
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    assert_eq!(fs::read(e.fivem.join("mods/torreeifel.rpf")).unwrap(), rpf);
    assert!(e.fivem.join("mods/otro.rpf").exists() && e.fivem.join("citizen/common/data/a.xml").exists());
    let id = e.eng.state.installs[0].id.clone();
    let un = e.eng.plan_uninstall(&id, &e.roots).unwrap();
    e.eng.apply(&un, &e.roots, &opts()).unwrap();
    assert_eq!(fs::read(e.fivem.join("mods/torreeifel.rpf")).unwrap(), orig_mod, "se recupera el archivo anterior");
    assert!(!e.fivem.join("mods/otro.rpf").exists() && !e.fivem.join("citizen/common/data/a.xml").exists());
}

#[test]
fn n03b_citizen_zip_plus_separate_rpf_file_are_one_package() {
    let mut e = env();
    let rpf = weapon_rpf("torre_dev", "torre", &["w_pi_x"], "t");
    let z = zip_of(&e, "citizen.zip", &[("citizen/x.xml", b"X".to_vec())]);
    let loose = e._td.path().join("zips").join("torreeifel.rpf");
    put_bytes(&loose, &rpf);
    let pkg = e.eng.import(Category::Citizen, &[z, loose], None).unwrap();
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    assert!(plan.blockers.is_empty());
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    assert!(e.fivem.join("mods/torreeifel.rpf").exists() && e.fivem.join("citizen/x.xml").exists());
}

#[test]
fn n04_citizen_rejects_files_that_are_not_real_rpf_and_skips_extras() {
    let e = env();
    let z = zip_of(&e, "fake.zip", &[("citizen/x.txt", b"x".to_vec()), ("falso.rpf", b"no soy un rpf".to_vec())]);
    let err = e.eng.import(Category::Citizen, &[z], None).unwrap_err().to_string();
    assert!(err.contains("falso.rpf") && err.contains("RPF"), "{}", err);
    assert!(e.eng.packages().unwrap().is_empty());
    let z2 = zip_of(&e, "extra.zip", &[("citizen/a.xml", b"A".to_vec()), ("instalar.bat", b"@echo".to_vec()), ("lista.docx", b"d".to_vec())]);
    let p = e.eng.import(Category::Citizen, &[z2], None).unwrap();
    assert_eq!(p.files.len(), 1, "los .bat y similares no se instalan");
    assert!(p.warnings.iter().any(|w| w.contains("instalar.bat")), "{:?}", p.warnings);
}

// ---------- Sound Packs ----------
#[test]
fn n05_sound_pack_skips_extras_and_checks_gta_folder() {
    let mut e = env();
    let z = zip_of(&e, "sp.zip", &[
        ("sp/RESIDENT.rpf", b"RES".to_vec()),
        ("sp/WEAPONS_PLAYER.rpf", b"WEAP".to_vec()),
        ("sp/leeme.txt", b"hola".to_vec()),
        ("sp/extra.awc", b"x".to_vec()),
        ("sp/instalar.bat", b"x".to_vec()),
    ]);
    let pkg = e.eng.import(Category::SoundPack, &[z], None).unwrap();
    assert_eq!(pkg.files.len(), 2);
    assert!(pkg.warnings.iter().any(|w| w.contains("extra.awc")) && pkg.warnings.iter().any(|w| w.contains("instalar.bat")), "{:?}", pkg.warnings);
    let mut bad = e.roots.clone();
    let otro = e._td.path().join("OTRO_GTA");
    fs::create_dir_all(&otro).unwrap();
    bad.gta = Some(otro);
    let plan = e.eng.plan_install(&pkg.id, &bad, None).unwrap();
    assert!(plan.blockers.iter().any(|b| b.contains("x64")), "{:?}", plan.blockers);
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    assert!(plan.blockers.is_empty());
    e.eng.apply(&plan, &e.roots, &opts()).unwrap();
    let sfx = e.gta.join("x64/audio/sfx");
    assert_eq!(read(&sfx.join("RESIDENT.rpf")), "RES");
    assert_eq!(read(&sfx.join("WEAPONS_PLAYER.rpf")), "WEAP");
    let verify = e.eng.verify(&e.eng.state.installs[0].id.clone(), &e.roots).unwrap();
    assert!(verify.iter().all(|c| c.status == "ok"), "se comprueba que se copió de verdad");
}

#[test]
fn n06_progress_is_real_and_monotonic_for_import_and_install() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    let mut e = env();
    let z = zip_of(&e, "sp.zip", &[("RESIDENT.rpf", vec![1u8; 3_000_000]), ("WEAPONS_PLAYER.rpf", vec![2u8; 1_500_000])]);
    let last = AtomicU64::new(0);
    let tot = AtomicU64::new(0);
    let pkg = e
        .eng
        .import_with_progress(Category::SoundPack, &[z], None, &|d, t| {
            last.store(d, Ordering::Relaxed);
            tot.store(t, Ordering::Relaxed);
        })
        .unwrap();
    assert_eq!(tot.load(Ordering::Relaxed), 4_500_000);
    assert_eq!(last.load(Ordering::Relaxed), 4_500_000, "el avance de importación llega al total");
    let plan = e.eng.plan_install(&pkg.id, &e.roots, None).unwrap();
    let events: Arc<Mutex<Vec<Progress>>> = Arc::new(Mutex::new(Vec::new()));
    let ev = events.clone();
    let o = ExecOptions { on_progress: Some(Arc::new(move |p| ev.lock().unwrap().push(p))), ..opts() };
    e.eng.apply(&plan, &e.roots, &o).unwrap();
    let v = events.lock().unwrap();
    assert!(v.len() > 2);
    assert!(v.windows(2).all(|w| w[1].done_bytes >= w[0].done_bytes), "bytes nunca retroceden");
    let l = v.last().unwrap();
    assert_eq!(l.done_bytes, l.total_bytes);
    assert_eq!(l.total_bytes, 4_500_000);
    assert_eq!(l.ops_done, l.ops_total);
}

// ---------- Explorador de archivos ----------
#[test]
fn f01_areas_per_section_and_read_only_areas() {
    let e = env();
    let ids = |c: Category| fm::areas(c, &e.roots).into_iter().map(|a| (a.id, a.writable)).collect::<Vec<_>>();
    assert_eq!(ids(Category::Citizen), vec![("mods".to_string(), true), ("citizen".to_string(), false)]);
    assert_eq!(ids(Category::SoundPack), vec![("gta_sfx".to_string(), false), ("mods".to_string(), true)]);
    assert_eq!(ids(Category::BloodFx), vec![("effects".to_string(), true), ("mods".to_string(), true)]);
    assert!(fm::find_area(Category::Weapon, &e.roots, "citizen").is_err(), "esa carpeta no existe en Armas");
    let mut no_gta = e.roots.clone();
    no_gta.gta = None;
    assert_eq!(fm::areas(Category::SoundPack, &no_gta).len(), 1);
}

#[test]
fn f02_listing_marks_managed_and_preexisting_files() {
    let mut e = env();
    put(&e.fivem.join("mods/propio.rpf"), "MIO");
    put(&e.fivem.join("mods/sub/otro.awc"), "AWC");
    let m = zip_of(&e, "m.zip", &[("mods/instalado.bin", b"I".to_vec()), ("mods/carpeta444/x.bin", b"x".to_vec())]);
    install(&mut e, Category::Mod, &m);
    let area = fm::find_area(Category::Mod, &e.roots, "mods").unwrap();
    let l = fm::list(&e.eng.state, &e.roots, &area, "").unwrap();
    let get = |n: &str| l.entries.iter().find(|x| x.name == n).unwrap().clone();
    assert_eq!(get("instalado.bin").status, "managed");
    assert!(get("instalado.bin").owner.is_some() && !get("instalado.bin").can_modify);
    assert_eq!(get("propio.rpf").status, "external");
    assert!(get("propio.rpf").can_modify && get("propio.rpf").ext == "rpf" && get("propio.rpf").size == 3);
    assert_eq!(get("carpeta444").status, "managed_dir");
    assert_eq!(get("sub").status, "folder");
    assert!(l.entries[0].is_dir, "las carpetas van primero");
    let inner = fm::list(&e.eng.state, &e.roots, &area, "sub").unwrap();
    assert_eq!(inner.entries[0].sub, "sub/otro.awc");
    assert_eq!(inner.entries[0].ext, "awc");
}

#[test]
fn f03_delete_backs_up_restores_and_protects() {
    let mut e = env();
    put(&e.fivem.join("mods/propio.rpf"), "MIO");
    put(&e.fivem.join("mods/lleno/a.txt"), "a");
    fs::create_dir_all(e.fivem.join("mods/vacia")).unwrap();
    let m = zip_of(&e, "m.zip", &[("mods/gestionado.bin", b"G".to_vec())]);
    install(&mut e, Category::Mod, &m);
    let area = fm::find_area(Category::Mod, &e.roots, "mods").unwrap();
    let del = |e: &Env, s: &str| fm::delete_items(&e.eng.store, &e.eng.state, &e.roots, &area, &[s.to_string()]);
    assert!(del(&e, "gestionado.bin").is_err(), "lo instalado por 444CFG no se borra aquí");
    assert!(e.fivem.join("mods/gestionado.bin").exists());
    assert!(del(&e, "lleno").is_err(), "carpeta no vacía");
    assert!(del(&e, "../citizen").is_err() && del(&e, "..").is_err() && del(&e, "").is_err(), "no se sale del área");
    assert!(del(&e, "noexiste.rpf").is_err());
    del(&e, "vacia").unwrap();
    assert!(!e.fivem.join("mods/vacia").exists());
    del(&e, "propio.rpf").unwrap();
    assert!(!e.fivem.join("mods/propio.rpf").exists());
    let log = fm::backups(&e.eng.store);
    assert_eq!(log.len(), 1);
    let rel = fm::restore(&e.eng.store, &e.eng.state, &e.roots, &log[0].id).unwrap();
    assert_eq!(rel, "mods/propio.rpf");
    assert_eq!(read(&e.fivem.join("mods/propio.rpf")), "MIO", "se recupera igual que estaba");
    assert!(fm::backups(&e.eng.store).is_empty());
    // Zona de solo lectura
    let ro = fm::find_area(Category::Citizen, &e.roots, "citizen").unwrap();
    put(&e.fivem.join("citizen/x.txt"), "x");
    assert!(fm::delete_items(&e.eng.store, &e.eng.state, &e.roots, &ro, &["x.txt".to_string()]).is_err());
    assert!(e.fivem.join("citizen/x.txt").exists());
}

#[test]
fn f04_import_replace_and_undo() {
    let mut e = env();
    let area = fm::find_area(Category::Weapon, &e.roots, "mods").unwrap();
    let ext = e._td.path().join("ext");
    put(&ext.join("nuevo.rpf"), "NUEVO");
    put(&ext.join("virus.exe"), "MZ");
    let mut noop = |_: u64, _: u64| {};
    assert!(fm::import_files(&e.eng.store, &e.eng.state, &e.roots, &area, "", &[ext.join("virus.exe")], false, &mut noop).is_err(), "ejecutables bloqueados");
    let w = fm::import_files(&e.eng.store, &e.eng.state, &e.roots, &area, "", &[ext.join("nuevo.rpf")], false, &mut noop).unwrap();
    assert_eq!(w, vec!["nuevo.rpf".to_string()]);
    assert_eq!(read(&e.fivem.join("mods/nuevo.rpf")), "NUEVO");
    assert!(fm::import_files(&e.eng.store, &e.eng.state, &e.roots, &area, "", &[ext.join("nuevo.rpf")], false, &mut noop).is_err(), "no sobrescribe sin pedirlo");
    put(&ext.join("nuevo.rpf"), "OTRO CONTENIDO");
    fm::import_files(&e.eng.store, &e.eng.state, &e.roots, &area, "", &[ext.join("nuevo.rpf")], true, &mut noop).unwrap();
    assert_eq!(read(&e.fivem.join("mods/nuevo.rpf")), "OTRO CONTENIDO");
    let log = fm::backups(&e.eng.store);
    assert_eq!(log[0].action, "replace");
    fm::restore(&e.eng.store, &e.eng.state, &e.roots, &log[0].id).unwrap();
    assert_eq!(read(&e.fivem.join("mods/nuevo.rpf")), "NUEVO", "se deshace la sustitución");
    let add = fm::backups(&e.eng.store);
    assert_eq!(add[0].action, "add");
    fm::restore(&e.eng.store, &e.eng.state, &e.roots, &add[0].id).unwrap();
    assert!(!e.fivem.join("mods/nuevo.rpf").exists(), "se deshace lo añadido");
    // un archivo gestionado no se sustituye desde aquí
    let m = zip_of(&e, "m.zip", &[("mods/gest.bin", b"G".to_vec())]);
    install(&mut e, Category::Mod, &m);
    put(&ext.join("gest.bin"), "X");
    assert!(fm::import_files(&e.eng.store, &e.eng.state, &e.roots, &area, "", &[ext.join("gest.bin")], true, &mut noop).is_err());
    assert_eq!(read(&e.fivem.join("mods/gest.bin")), "G");
}

#[test]
fn f05_replace_file_keeps_target_name_and_protects_managed() {
    let mut e = env();
    put(&e.fivem.join("mods/viejo.rpf"), "VIEJO");
    let area = fm::find_area(Category::Mod, &e.roots, "mods").unwrap();
    let src = e._td.path().join("ext/lo_que_sea.rpf");
    put(&src, "NUEVO");
    let mut noop = |_: u64, _: u64| {};
    fm::replace_file(&e.eng.store, &e.eng.state, &e.roots, &area, "viejo.rpf", &src, &mut noop).unwrap();
    assert_eq!(read(&e.fivem.join("mods/viejo.rpf")), "NUEVO");
    assert!(!e.fivem.join("mods/lo_que_sea.rpf").exists());
    let log = fm::backups(&e.eng.store);
    fm::restore(&e.eng.store, &e.eng.state, &e.roots, &log[0].id).unwrap();
    assert_eq!(read(&e.fivem.join("mods/viejo.rpf")), "VIEJO");
    assert!(fm::replace_file(&e.eng.store, &e.eng.state, &e.roots, &area, "../x.rpf", &src, &mut noop).is_err());
    let m = zip_of(&e, "m.zip", &[("mods/gest.bin", b"G".to_vec())]);
    install(&mut e, Category::Mod, &m);
    assert!(fm::replace_file(&e.eng.store, &e.eng.state, &e.roots, &area, "gest.bin", &src, &mut noop).is_err());
}
