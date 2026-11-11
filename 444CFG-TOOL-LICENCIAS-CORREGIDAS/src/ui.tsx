import { createContext, useContext, useState, useCallback, ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import type { ProgressInfo } from "./types";

interface Toast { id: number; text: string; kind: "ok" | "err" }
const ToastCtx = createContext<(text: string, kind?: "ok" | "err") => void>(() => {});
export const useToast = () => useContext(ToastCtx);

export function ToastHost({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Toast[]>([]);
  const push = useCallback((text: string, kind: "ok" | "err" = "ok") => {
    const id = Date.now() + Math.random();
    setItems((x) => [...x, { id, text, kind }]);
    setTimeout(() => setItems((x) => x.filter((t) => t.id !== id)), kind === "err" ? 9000 : 4000);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        <AnimatePresence>
          {items.map((t) => (
            <motion.div key={t.id} className={`toast ${t.kind}`} initial={{ opacity: 0, x: 30 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 30 }}>
              {t.text}
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </ToastCtx.Provider>
  );
}

export function Modal({ title, onClose, children, wide }: { title: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  return (
    <motion.div className="overlay" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onClick={onClose}>
      <motion.div
        className="modal" style={wide ? { width: 760 } : undefined} role="dialog" aria-modal="true" aria-label={title}
        initial={{ opacity: 0, y: 14, scale: 0.98 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: 14 }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-h"><h3>{title}</h3><button className="wc" aria-label="Cerrar" onClick={onClose}>✕</button></div>
        <div className="modal-b">{children}</div>
      </motion.div>
    </motion.div>
  );
}

export const fmtSize = (n: number) => {
  if (n < 1024) return `${n} B`;
  const u = ["KB", "MB", "GB"];
  let v = n / 1024, i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${u[i]}`;
};

/** Barra de progreso real: porcentaje por bytes copiados (o por operaciones si no hay bytes). */
export function Progress({ p, label }: { p: ProgressInfo; label?: string }) {
  const byBytes = p.total_bytes > 0;
  const pct = byBytes ? Math.min(100, Math.round((p.done_bytes / p.total_bytes) * 100)) : p.ops_total ? Math.round(((p.ops_done ?? 0) / p.ops_total) * 100) : 0;
  return (
    <div className="prog" aria-label={`Progreso ${pct}%`}>
      <div className="prog-bar"><div style={{ width: `${pct}%` }} /></div>
      <span>
        {label ? `${label} · ` : ""}{byBytes ? `${fmtSize(p.done_bytes)} de ${fmtSize(p.total_bytes)}` : ""}
        {p.ops_total !== undefined ? `${byBytes ? " · " : ""}${p.ops_done ?? 0}/${p.ops_total} operaciones` : ""} · {pct}%
      </span>
    </div>
  );
}

export const fmtDate = (s: number) => new Date(s * 1000).toLocaleString("es-ES");
export const catLabel: Record<string, string> = {
  citizen: "Citizen", mod: "Mod", weapon: "Arma", blood_fx: "Blood FX", tracer: "Tracer", sound_pack: "Sound Pack",
  Citizen: "Citizen", Mod: "Mod", Weapon: "Arma", BloodFx: "Blood FX", Tracer: "Tracer", SoundPack: "Sound Pack"
};
