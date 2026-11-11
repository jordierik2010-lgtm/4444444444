import { useCallback, useEffect, useMemo, useState } from "react";
import { AnimatePresence } from "motion/react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { ChevronRight, File as FileIcon, FileAudio, Folder, FolderOpen, Lock, RefreshCw, Replace, RotateCcw, Trash2, Upload } from "lucide-react";
import { api, errText } from "../api";
import type { Category, FmArea, FmBackup, FmEntry, FmListing, ProgressInfo } from "../types";
import { Modal, Progress, fmtDate, fmtSize, useToast } from "../ui";

const STATUS_LABEL: Record<FmEntry["status"], string> = {
  managed: "444CFG", managed_dir: "444CFG", external: "Preexistente", folder: "Carpeta", link: "Enlace"
};
const ACTION_LABEL: Record<FmBackup["action"], string> = { delete: "Eliminado", replace: "Sustituido", add: "Añadido" };

/** Explorador acotado a las carpetas de FiveM / GTA V de la sección. Todas las operaciones son reales y validadas en Rust. */
export default function FileExplorer({ section, refreshKey }: { section: Category; refreshKey: number }) {
  const toast = useToast();
  const [areas, setAreas] = useState<FmArea[]>([]);
  const [area, setArea] = useState<string>("");
  const [sub, setSub] = useState<string>("");
  const [list, setList] = useState<FmListing | null>(null);
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState<ProgressInfo | null>(null);
  const [confirmDel, setConfirmDel] = useState<FmEntry[] | null>(null);
  const [backups, setBackups] = useState<FmBackup[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api.fmAreas(section).then((a) => { if (alive) { setAreas(a); setArea((cur) => (a.some((x) => x.id === cur) ? cur : a[0]?.id ?? "")); } })
      .catch((e) => { if (alive) setError(errText(e)); });
    return () => { alive = false; };
  }, [section]);

  useEffect(() => {
    const un = listen<ProgressInfo>("fm-progress", (e) => setProg(e.payload));
    return () => { un.then((f) => f()); };
  }, []);

  const load = useCallback(async () => {
    if (!area) return;
    try { const l = await api.fmList(section, area, sub); setList(l); setError(null); setSel(new Set()); }
    catch (e) { setList(null); setError(errText(e)); }
  }, [section, area, sub]);
  useEffect(() => { load(); }, [load, refreshKey]);

  const cur = areas.find((a) => a.id === area);
  const entries = list?.entries ?? [];
  const selected = useMemo(() => entries.filter((e) => sel.has(e.sub)), [entries, sel]);
  const crumbs = sub ? sub.split("/") : [];

  const guarded = async (fn: () => Promise<void>, ok: string) => {
    setBusy(true); setProg({ done_bytes: 0, total_bytes: 0 });
    try { await fn(); toast(ok); } catch (e) { toast(errText(e), "err"); }
    setBusy(false); setProg(null); await load();
  };

  const doImport = async (replace: boolean) => {
    const r = await open({ multiple: true });
    if (!r) return;
    const paths = Array.isArray(r) ? r : [r];
    await guarded(async () => { await api.fmImport(section, area, sub, paths, replace); }, replace ? "Archivos importados (con copia de los sustituidos)" : "Archivos importados");
  };
  const doReplace = async () => {
    if (selected.length !== 1 || selected[0].is_dir) return;
    const r = await open({ multiple: false });
    if (typeof r !== "string") return;
    const target = selected[0];
    await guarded(async () => { await api.fmReplace(section, area, target.sub, r); }, `«${target.name}» sustituido; copia guardada`);
  };
  const doDelete = async () => {
    const items = confirmDel ?? [];
    setConfirmDel(null);
    await guarded(async () => { await api.fmDelete(section, area, items.map((i) => i.sub)); }, "Eliminado; puedes deshacerlo desde «Copias»");
  };
  const openBackups = async () => { try { setBackups(await api.fmBackups()); } catch (e) { toast(errText(e), "err"); } };
  const restore = async (b: FmBackup) => {
    try { await api.fmRestore(b.id); toast(`Restaurado: ${b.rel}`); setBackups(await api.fmBackups()); await load(); }
    catch (e) { toast(errText(e), "err"); }
  };
  const toggle = (e: FmEntry) => setSel((s) => { const n = new Set(s); n.has(e.sub) ? n.delete(e.sub) : n.add(e.sub); return n; });
  const canDelete = selected.length > 0 && selected.every((e) => e.can_modify);
  const canReplace = selected.length === 1 && !selected[0].is_dir && selected[0].can_modify;

  return (
    <div className="pn fx">
      <div className="fx-head">
        <div className="lbl" style={{ margin: 0 }}>EXPLORADOR DE ARCHIVOS</div>
        <div className="chips">
          {areas.map((a) => (
            <button key={a.id} className={`chip${a.id === area ? " on" : ""}`} aria-pressed={a.id === area} onClick={() => { setArea(a.id); setSub(""); }}>
              {!a.writable && <Lock size={11} style={{ verticalAlign: -1, marginRight: 5 }} />}{a.label}
            </button>
          ))}
        </div>
      </div>

      <div className="fx-bar">
        <div className="fx-crumbs" aria-label="Ruta">
          <button className="crumb" onClick={() => setSub("")}>{cur?.rel ?? "…"}</button>
          {crumbs.map((c, i) => (
            <span key={i} className="crumb-w"><ChevronRight size={12} /><button className="crumb" onClick={() => setSub(crumbs.slice(0, i + 1).join("/"))}>{c}</button></span>
          ))}
        </div>
        <div className="fx-tools">
          <button className="btn sm" disabled={busy} onClick={load}><RefreshCw size={13} style={{ verticalAlign: -2 }} /> Actualizar</button>
          <button className="btn sm" disabled={busy || !cur?.writable} onClick={() => doImport(false)}><Upload size={13} style={{ verticalAlign: -2 }} /> Importar</button>
          <button className="btn sm" disabled={busy || !canReplace} onClick={doReplace}><Replace size={13} style={{ verticalAlign: -2 }} /> Sustituir</button>
          <button className="btn sm" disabled={busy || !canDelete} onClick={() => setConfirmDel(selected)}><Trash2 size={13} style={{ verticalAlign: -2 }} /> Eliminar</button>
          <button className="btn sm" disabled={busy} onClick={() => api.fmOpen(section, area, selected.length === 1 ? selected[0].sub : sub).catch((e) => toast(errText(e), "err"))}><FolderOpen size={13} style={{ verticalAlign: -2 }} /> Abrir ubicación</button>
          <button className="btn sm" disabled={busy} onClick={openBackups}><RotateCcw size={13} style={{ verticalAlign: -2 }} /> Copias</button>
        </div>
      </div>

      {cur && !cur.writable && <div className="note"><Lock size={12} style={{ verticalAlign: -1 }} /> Carpeta de solo lectura: se puede ver y abrir. Los cambios se hacen con la instalación de la sección (con copia de seguridad).</div>}
      {error && <div className="warn">⛔ {error}</div>}
      {prog && <Progress p={prog} label="Copiando" />}

      <div className="fx-table" role="table">
        <div className="fx-row fx-th" role="row"><span /><span>Nombre</span><span>Formato</span><span>Tamaño</span><span>Modificado</span><span>Estado</span></div>
        {sub !== "" && (
          <button className="fx-row fx-up" onClick={() => setSub(crumbs.slice(0, -1).join("/"))}><span /><span><Folder size={15} /> ..</span><span /><span /><span /><span /></button>
        )}
        {entries.length === 0 && <p className="sub" style={{ padding: "14px 4px" }}>{cur && !cur.exists ? "Esta carpeta todavía no existe." : "Carpeta vacía."}</p>}
        {entries.map((e) => (
          <div key={e.sub} className={`fx-row${sel.has(e.sub) ? " on" : ""}`} role="row">
            <span><input type="checkbox" aria-label={`Seleccionar ${e.name}`} checked={sel.has(e.sub)} onChange={() => toggle(e)} /></span>
            <span className="fx-name">
              {e.is_dir
                ? <button className="fx-link" onClick={() => setSub(e.sub)}><Folder size={15} /> {e.name}</button>
                : <span title={e.name}>{e.ext === "awc" || e.ext === "rel" ? <FileAudio size={15} /> : <FileIcon size={15} />} {e.name}</span>}
            </span>
            <span>{e.is_dir ? "—" : e.ext ? `.${e.ext.toUpperCase()}` : "—"}</span>
            <span>{e.is_dir ? "—" : fmtSize(e.size)}</span>
            <span>{e.modified ? fmtDate(e.modified) : "—"}</span>
            <span><i className={`pill ${e.status}`} title={e.note ?? ""}>{e.owner ? `444CFG · ${e.owner}` : STATUS_LABEL[e.status]}</i></span>
          </div>
        ))}
      </div>
      <small className="sub">Operaciones limitadas a las carpetas configuradas; lo instalado por 444CFG se retira desde su instalación. Antes de sustituir o eliminar se guarda una copia restaurable.</small>

      <AnimatePresence>
        {confirmDel && (
          <Modal title="Eliminar archivos seleccionados" onClose={() => setConfirmDel(null)}>
            <p className="sub">Se eliminarán {confirmDel.length} elemento(s) de «{cur?.rel}». De cada archivo se guarda una copia que podrás restaurar desde «Copias». Las carpetas solo se eliminan si están vacías.</p>
            <ul className="list">{confirmDel.map((i) => <li key={i.sub}>{i.sub}{i.is_dir ? "/" : ` · ${fmtSize(i.size)}`}</li>)}</ul>
            <div className="row-end">
              <button className="btn" onClick={() => setConfirmDel(null)}>Cancelar</button>
              <button className="btn primary" onClick={doDelete}>Eliminar</button>
            </div>
          </Modal>
        )}
        {backups && (
          <Modal title="Copias del explorador" onClose={() => setBackups(null)} wide>
            {backups.length === 0 ? <p className="sub">No hay copias: todavía no se ha eliminado ni sustituido nada desde el explorador.</p> : (
              <ul className="list" style={{ maxHeight: 340, listStyle: "none", paddingLeft: 12 }}>
                {backups.map((b) => (
                  <li key={b.id} className="bk">
                    <span>{ACTION_LABEL[b.action]} · {b.rel} <small>({fmtDate(b.ts)})</small></span>
                    <button className="btn sm" onClick={() => restore(b)}>{b.action === "add" ? "Deshacer" : "Restaurar"}</button>
                  </li>
                ))}
              </ul>
            )}
            <p className="sub">Restaurar solo funciona si el archivo no ha cambiado desde entonces; nunca sobrescribe nada ajeno.</p>
          </Modal>
        )}
      </AnimatePresence>
    </div>
  );
}
