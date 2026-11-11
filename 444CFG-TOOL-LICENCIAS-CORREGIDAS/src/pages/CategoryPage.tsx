import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence } from "motion/react";
import { ShieldCheck, Upload, Trash2, Info } from "lucide-react";
import { api, errText } from "../api";
import type { Category, Install, Package, PathCheck, Plan, ProgressInfo, Settings } from "../types";
import FileExplorer from "../components/FileExplorer";
import { Modal, Progress, useToast, fmtDate } from "../ui";

const META: Record<Category, { title: string; sub: string; accept: string; multi: boolean }> = {
  citizen: { title: "Citizen", sub: "Cambio de Citizen con simulación previa, copia de seguridad y verificación por hash. Acepta ZIP con carpeta citizen/ y archivos RPF (van a mods/); se ve la lista de archivos antes de confirmar.", accept: "ZIP con citizen/ y/o RPF", multi: true },
  mod: { title: "Mods", sub: "Mods independientes (RPF o archivos dentro de mods/). Se conservan al cambiar de Citizen salvo que se vinculen.", accept: "ZIP o RPF", multi: true },
  weapon: { title: "Armas", sub: "Skins MK2 y 9mm (RPF). Se detecta el arma por el contenido; una skin nueva sustituye a la anterior de la misma arma (también si se instaló a mano, con confirmación y copia) y nunca toca otras armas.", accept: "ZIP o RPF", multi: true },
  blood_fx: { title: "Blood FX", sub: "bloodfx.dat y los RPF asociados se instalan y se retiran juntos.", accept: "ZIP, bloodfx.dat o RPF", multi: true },
  tracer: { title: "Tracers", sub: "Tracers en XML de timecycle o RPF. Los RPF requieren confirmación de destino.", accept: "ZIP, XML o RPF", multi: true },
  sound_pack: { title: "Sound Packs", sub: "RESIDENT.rpf y WEAPONS_PLAYER.rpf forman un único Sound Pack; solo hay uno activo. Se instala con copia de los originales y se puede retirar restaurándolos.", accept: "ZIP o RPF (varios)", multi: true }
};
const SLOT_CATS: Category[] = ["citizen", "weapon", "blood_fx", "tracer", "sound_pack"];

export default function CategoryPage({ category, settings }: { category: Category; settings: Settings }) {
  const m = META[category];
  const toast = useToast();
  const [pkgs, setPkgs] = useState<Package[]>([]);
  const [installs, setInstalls] = useState<Install[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState<ProgressInfo | null>(null);
  const [impProg, setImpProg] = useState<ProgressInfo | null>(null);
  const [fxKey, setFxKey] = useState(0);
  const [paths, setPaths] = useState<PathCheck[]>([]);
  const [details, setDetails] = useState<Package | null>(null);
  const [confirm, setConfirm] = useState<{ kind: "install" | "uninstall"; id: string; plan: Plan } | null>(null);
  const [ack, setAck] = useState(false);
  const [linked, setLinked] = useState(false);
  const planSeq = useRef(0);

  const refresh = useCallback(async () => {
    const [p, i] = await Promise.all([api.listPackages(), api.listInstalls()]);
    setPkgs(p.filter((x) => x.category === category));
    setInstalls(i);
  }, [category]);
  useEffect(() => { refresh().catch((e) => toast(errText(e), "err")); }, [refresh]);
  useEffect(() => {
    const un = listen<ProgressInfo>("progress", (e) => setProg(e.payload));
    const un2 = listen<ProgressInfo>("import-progress", (e) => setImpProg(e.payload));
    return () => { un.then((f) => f()); un2.then((f) => f()); };
  }, []);
  useEffect(() => { if (category === "sound_pack") api.checkPaths().then(setPaths).catch(() => {}); }, [category]);

  const mine = installs.filter((i) => i.category === category);
  const activeCitizen = installs.find((i) => i.category === "citizen");
  const isInstalled = (p: Package) => mine.find((i) => i.package_id === p.id);
  const slotInstall = SLOT_CATS.includes(category) ? mine : [];

  const choose = async (p: Package) => {
    setSel(p.id); setPlan(null); setAck(false);
    const seq = ++planSeq.current;
    try {
      const pl = await api.planInstall(p.id, category === "mod" && linked ? activeCitizen?.id : null);
      if (seq === planSeq.current) setPlan(pl);
    } catch (e) { toast(errText(e), "err"); }
  };

  const importFiles = async () => {
    const r = await open({ multiple: m.multi, filters: [{ name: m.accept, extensions: ["zip", "rpf", "xml", "dat"] }] });
    if (!r) return;
    const paths = Array.isArray(r) ? r : [r];
    setBusy(true); setImpProg({ done_bytes: 0, total_bytes: 0 });
    try {
      const p = await api.importPackage(category, paths);
      toast(`Paquete «${p.name}» importado a la biblioteca`);
      await refresh();
      await choose(p);
    } catch (e) { toast(errText(e), "err"); }
    setBusy(false); setImpProg(null);
  };

  const run = async (fn: () => Promise<void>, okMsg: string) => {
    setBusy(true); setProg({ done_bytes: 0, total_bytes: 0, ops_done: 0, ops_total: 0 });
    try { await fn(); toast(okMsg); }
    catch (e) { toast(errText(e), "err"); }
    setBusy(false); setProg(null); setConfirm(null); setPlan(null); setSel(null);
    await refresh().catch(() => {});
    setFxKey((k) => k + 1);
  };

  const selPkg = pkgs.find((p) => p.id === sel) ?? null;
  const askInstall = () => {
    if (!selPkg || !plan) return;
    setAck(false);
    setConfirm({ kind: "install", id: selPkg.id, plan });
  };
  const changeKind = async (p: Package, kind: string | null) => {
    try { const np = await api.setWeaponKind(p.id, kind); toast(kind ? `Arma fijada: ${kind === "mk2" ? "MK2" : "9mm"}` : "Arma sin identificar"); setDetails(np); await refresh(); if (sel === p.id) await choose(np); }
    catch (e) { toast(errText(e), "err"); }
  };
  const askUninstall = async (i: Install) => {
    try {
      const pl = await api.planUninstall(i.id);
      setConfirm({ kind: "uninstall", id: i.id, plan: pl });
    } catch (e) { toast(errText(e), "err"); }
  };
  const doConfirm = () => {
    if (!confirm) return;
    if (confirm.kind === "install") run(() => api.applyInstall(confirm.id, category === "mod" && linked ? activeCitizen?.id ?? null : null, true, ack), "Instalación completada y verificada archivo por archivo");
    else run(() => api.applyUninstall(confirm.id, true), "Retirada completada; archivos restaurados");
  };

  const S = plan?.summary;
  return (
    <div className="page">
      <h1>{m.title}</h1>
      <p className="sub">{m.sub}</p>
      <div className="center3">
        <div className="side">
          <div className="lbl">GARANTÍAS</div>
          {["Copia de seguridad previa por hash", "Simulación antes de modificar", "Verificación SHA-256 de cada archivo", "Cancelación ante cualquier duda", "Reversión automática si falla"].map((t) => (
            <div className="chk" key={t}><ShieldCheck size={15} />{t}</div>
          ))}
        </div>
        <div className="pn sim">
          <div className="lbl">SIMULACIÓN</div>
          {!selPkg || !plan ? (
            <p className="sub">Selecciona un paquete de la biblioteca o importa uno para ver exactamente qué cambiará antes de tocar ningún archivo.</p>
          ) : (
            <>
              <div className="sim-t">{slotInstall[0] ? `${slotInstall[0].name} → ${selPkg.name}` : selPkg.name}</div>
              {S && (<>
                <div className="r"><span>Anteriores que se retirarán</span><b>{S.retire_old}</b></div>
                <div className="r"><span>Nuevos que se instalarán</span><b>{S.install_new}</b></div>
                <div className="r"><span>Originales que se restaurarán</span><b>{S.restore_originals + S.reveal_layers}</b></div>
                <div className="r"><span>De otros mods que se conservan</span><b>{S.keep_others}</b></div>
                <div className="r"><span>Idénticos sin tocar</span><b>{S.unchanged}</b></div>
              </>)}
              {plan.blockers.map((b, i) => <div className="warn" key={i}>⛔ {b}</div>)}
              {plan.warnings.map((b, i) => <div className="note" key={i}>ℹ {b}</div>)}
              {category === "mod" && activeCitizen && (
                <label className="inline"><input type="checkbox" checked={linked} onChange={async (e) => { setLinked(e.target.checked); if (selPkg) { const pl = await api.planInstall(selPkg.id, e.target.checked ? activeCitizen.id : null); setPlan(pl); } }} /> Vincular al Citizen activo («{activeCitizen.name}»)</label>
              )}
              {impProg && <Progress p={impProg} label="Importando" />}
              <button className="btn primary" disabled={busy || plan.blockers.length > 0} onClick={askInstall}>Instalar {selPkg.name}</button>
            </>
          )}
        </div>
        <div className="side">
          <div className="lbl">ACTIVO</div>
          {mine.length === 0 ? <p className="sub">Nada instalado en esta categoría.</p> : mine.map((i) => (
            <div className="act" key={i.id}>
              <b>{i.name}</b><small>{fmtDate(i.installed_at)} · {i.files.length} archivos</small>
              <button className="btn sm" disabled={busy} onClick={() => askUninstall(i)}>Retirar y restaurar</button>
            </div>
          ))}
        </div>
      </div>
      {category === "sound_pack" && paths.length > 0 && (
        <div className="note" style={{ marginTop: 18 }}>
          {paths.filter((x) => x.label === "GTA V").map((x) => <div key={x.label}>{x.ok ? "✔" : "⚠"} GTA V: {x.path ?? "sin configurar"} · {x.detail}</div>)}
          Modo actual: {settings.sound_mode === "mods_dir" ? "FiveM.app\\mods\\x64\\audio\\sfx" : "GTAV\\x64\\audio\\sfx"} (se cambia en Ajustes).
        </div>
      )}
      <div className="lbl" style={{ marginTop: 26 }}>BIBLIOTECA · {m.accept.toUpperCase()}</div>
      <div className="cards">
        {pkgs.map((p) => {
          const inst = isInstalled(p);
          return (
            <div key={p.id} className={`cd${sel === p.id ? " sel" : ""}`}>
              <button className="cd-main" onClick={() => choose(p)} aria-pressed={sel === p.id}>
                <span>{p.name}<small>{p.files.length} archivos{p.weapon_kind ? ` · ${p.weapon_kind === "mk2" ? "MK2" : "9mm"}` : ""}{p.warnings.length ? ` · ${p.warnings.length} avisos` : ""}</small></span>
                <small className={inst ? "acc" : ""}>{inst ? "● Instalado" : sel === p.id ? "Seleccionado" : "En biblioteca"}</small>
              </button>
              <div className="cd-act">
                <button className="wc" title="Detalles" aria-label="Detalles" onClick={() => setDetails(p)}><Info size={14} /></button>
                <button className="wc" title="Eliminar de la biblioteca" aria-label="Eliminar de la biblioteca" disabled={!!inst} onClick={async () => { try { await api.deletePackage(p.id); if (sel === p.id) { setSel(null); setPlan(null); } await refresh(); toast("Paquete eliminado de la biblioteca"); } catch (e) { toast(errText(e), "err"); } }}><Trash2 size={14} /></button>
              </div>
            </div>
          );
        })}
        <button className="cd dashed" disabled={busy} onClick={importFiles}><Upload size={16} /> Importar</button>
      </div>
      <FileExplorer section={category} refreshKey={fxKey} />
      <AnimatePresence>
        {details && (
          <Modal title={`Detalles · ${details.name}`} onClose={() => setDetails(null)} wide>
            {details.category === "weapon" && (<>
              <div className="lbl">ARMA DETECTADA</div>
              <div className="chips">
                {([["mk2", "MK2"], ["9mm", "9mm"]] as const).map(([k, l]) => (
                  <button key={k} className={`chip${details.weapon_kind === k ? " on" : ""}`} aria-pressed={details.weapon_kind === k} onClick={() => changeKind(details, k)}>{l}</button>
                ))}
                <button className={`chip${!details.weapon_kind ? " on" : ""}`} aria-pressed={!details.weapon_kind} onClick={() => changeKind(details, null)}>Otra / sin identificar</button>
              </div>
              <ul className="list">{details.weapon_evidence.map((c, i) => <li key={i}>{c}</li>)}</ul>
            </>)}
            <div className="lbl">DESTINOS</div>
            <ul className="list">{details.contents.map((c, i) => <li key={i}>{c}</li>)}</ul>
            {details.mount_ids.length > 0 && (<><div className="lbl">IDENTIFICADORES DE MONTAJE</div><ul className="list">{details.mount_ids.map((c) => <li key={c}>{c}</li>)}</ul></>)}
            {details.warnings.map((w, i) => <div className="note" key={i}>ℹ {w}</div>)}
            <small className="sub">ID {details.id.slice(0, 16)} · {fmtDate(details.created_at)}</small>
          </Modal>
        )}
        {confirm && (
          <Modal title={confirm.plan.title} onClose={() => !busy && setConfirm(null)} wide>
            <div className="lbl">SIMULACIÓN COMPLETA</div>
            <p className="sub">{confirm.plan.removes.length ? `Se retirará: ${confirm.plan.removes.join(", ")}.` : "No se retira ninguna instalación."} Operaciones previstas: {confirm.plan.ops.length}.</p>
            {confirm.plan.blockers.map((b, i) => <div className="warn" key={i}>⛔ {b}</div>)}
            {confirm.plan.manual_conflicts.length > 0 && (<>
              <div className="warn">Ya hay una skin de esta arma instalada sin 444CFG TOOL. Se retirará (se guarda copia) y se devolverá al retirar esta skin:</div>
              <ul className="list">{confirm.plan.manual_conflicts.map((m) => <li key={m.rel}>{m.rel} · {m.reason}</li>)}</ul>
            </>)}
            {confirm.plan.unknown_overwrites.length > 0 && (<>
              <div className="warn">Se sustituirán archivos existentes que 444CFG no gestiona. Se guarda una copia de su estado previo y se restaurará al retirar el paquete:</div>
              <ul className="list">{confirm.plan.unknown_overwrites.map((u) => <li key={u}>{u}</li>)}</ul>
            </>)}
            {confirm.plan.needs_ack_unverified && (<>
              <ul className="list">{confirm.plan.ack_notes.map((n, i) => <li key={i}>{n}</li>)}</ul>
              <label className="inline"><input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} /> Entiendo lo anterior y quiero continuar</label>
            </>)}
            <div className="lbl">ARCHIVOS QUE SE VAN A MODIFICAR</div>
            <ul className="list" style={{ maxHeight: 180 }}>{confirm.plan.ops.slice(0, 200).map((o, i) => <li key={i}>{o.kind === "write" ? "Escribir" : o.kind === "delete" ? "Eliminar" : "Quitar carpeta vacía"} · {o.rel} <small>({o.why})</small></li>)}</ul>
            {prog && <Progress p={prog} label="Instalando" />}
            <div className="row-end">
              <button className="btn" disabled={busy} onClick={() => setConfirm(null)}>Cancelar</button>
              <button className="btn primary" disabled={busy || confirm.plan.blockers.length > 0 || (confirm.plan.needs_ack_unverified && !ack)} onClick={doConfirm}>Confirmar</button>
            </div>
          </Modal>
        )}
      </AnimatePresence>
    </div>
  );
}
