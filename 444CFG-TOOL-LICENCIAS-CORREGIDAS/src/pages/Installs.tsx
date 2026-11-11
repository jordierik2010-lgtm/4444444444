import { useCallback, useEffect, useState } from "react";
import { AnimatePresence } from "motion/react";
import { api, errText } from "../api";
import type { FileCheck, Install, Plan, ProgressInfo } from "../types";
import { Modal, Progress, catLabel, fmtDate, useToast } from "../ui";
import { listen } from "@tauri-apps/api/event";

export default function Installs() {
  const toast = useToast();
  const [items, setItems] = useState<Install[]>([]);
  const [checks, setChecks] = useState<{ inst: Install; res: FileCheck[] } | null>(null);
  const [plan, setPlan] = useState<{ inst: Install; plan: Plan } | null>(null);
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState<ProgressInfo | null>(null);
  const load = useCallback(() => api.listInstalls().then(setItems).catch((e) => toast(errText(e), "err")), []);
  useEffect(() => { load(); }, [load]);
  useEffect(() => { const u = listen<ProgressInfo>("progress", (e) => setProg(e.payload)); return () => { u.then((f) => f()); }; }, []);

  const verify = async (i: Install) => {
    try { const res = await api.verifyInstall(i.id); setChecks({ inst: i, res }); }
    catch (e) { toast(errText(e), "err"); }
  };
  const status = (i: Install) => (i.linked_to ? "Vinculado" : "Instalado");
  return (
    <div className="page">
      <h1>Mis instalaciones</h1>
      <p className="sub">Todo lo que gestiona 444CFG, con los archivos que modificó cada instalación.</p>
      {items.length === 0 ? <div className="pn"><p className="sub">No hay instalaciones gestionadas.</p></div> : (
        <div className="pn table">
          <div className="tr th"><span>Nombre</span><span>Categoría</span><span>Fecha</span><span>Estado</span><span>Archivos</span><span /></div>
          {items.map((i) => (
            <div className="tr" key={i.id}>
              <span><b>{i.name}</b></span><span>{catLabel[i.category]}</span><span>{fmtDate(i.installed_at)}</span><span>{status(i)}</span><span>{i.files.length}</span>
              <span className="acts">
                <button className="btn sm" onClick={() => verify(i)}>Comprobar archivos</button>
                <button className="btn sm" onClick={() => api.openInstallLocation(i.id).catch((e) => toast(errText(e), "err"))}>Abrir ubicación</button>
                <button className="btn sm" onClick={async () => { try { setPlan({ inst: i, plan: await api.planUninstall(i.id) }); } catch (e) { toast(errText(e), "err"); } }}>Eliminar y restaurar</button>
              </span>
            </div>
          ))}
        </div>
      )}
      <AnimatePresence>
        {checks && (
          <Modal title={`Archivos · ${checks.inst.name}`} onClose={() => setChecks(null)} wide>
            <p className="sub">{checks.res.filter((c) => c.status === "ok" || c.status.startsWith("cubierto")).length} de {checks.res.length} correctos. {checks.res.some((c) => c.status === "modificado" || c.status === "falta") ? "Hay archivos modificados o ausentes: no se eliminarán automáticamente." : ""}</p>
            <ul className="list" style={{ maxHeight: 340 }}>{checks.res.map((c, i) => <li key={i}>{c.status.toUpperCase()} · {c.path}</li>)}</ul>
          </Modal>
        )}
        {plan && (
          <Modal title={plan.plan.title} onClose={() => !busy && setPlan(null)} wide>
            <div className="r"><span>Anteriores que se retirarán</span><b>{plan.plan.summary.retire_old}</b></div>
            <div className="r"><span>Originales que se restaurarán</span><b>{plan.plan.summary.restore_originals + plan.plan.summary.reveal_layers}</b></div>
            <div className="r"><span>De otros mods que se conservan</span><b>{plan.plan.summary.keep_others}</b></div>
            {plan.plan.removes.length > 1 && <div className="note">También se retirarán los elementos vinculados: {plan.plan.removes.join(", ")}.</div>}
            {plan.plan.blockers.map((b, i) => <div className="warn" key={i}>⛔ {b}</div>)}
            {prog && <Progress p={prog} label="Retirando" />}
            <div className="row-end">
              <button className="btn" disabled={busy} onClick={() => setPlan(null)}>Cancelar</button>
              <button className="btn primary" disabled={busy || plan.plan.blockers.length > 0} onClick={async () => {
                setBusy(true); setProg({ done_bytes: 0, total_bytes: 0, ops_done: 0, ops_total: 0 });
                try { await api.applyUninstall(plan.inst.id, true); toast("Retirada completada; archivos restaurados"); }
                catch (e) { toast(errText(e), "err"); }
                setBusy(false); setProg(null); setPlan(null); load();
              }}>Confirmar</button>
            </div>
          </Modal>
        )}
      </AnimatePresence>
    </div>
  );
}
