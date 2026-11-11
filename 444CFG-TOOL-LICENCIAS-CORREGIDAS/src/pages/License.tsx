import { useEffect, useState } from "react";
import { KeyRound, Copy, ShieldCheck } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errText } from "../api";
import type { LicenseStatus } from "../types";
import { useToast } from "../ui";

/** Pantalla de activación: muestra el HWID del equipo y valida la KEY localmente con la clave pública. */
export default function License({ status, onActivated }: { status: LicenseStatus; onActivated: (s: LicenseStatus) => void }) {
  const toast = useToast();
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(status.error);
  useEffect(() => { setErr(status.error); }, [status.error]);

  const copy = async () => {
    try { await navigator.clipboard.writeText(status.hwid); toast("HWID copiado"); }
    catch { toast("No se pudo copiar: selecciona el HWID y pulsa Ctrl+C", "err"); }
  };
  const activate = async () => {
    setErr(null);
    if (!key.trim()) { setErr("Pega tu KEY para activar."); return; }
    setBusy(true);
    try { const s = await api.licenseActivate(key); toast("444CFG TOOL activado"); onActivated(s); }
    catch (e) { setErr(errText(e)); }
    setBusy(false);
  };
  return (
    <div className="lic-wrap">
      <div className="pn lic">
        <div className="hero"><img src="/logo444.png" alt="Logo 444" /><div><h1>Activar 444CFG TOOL</h1><p className="sub">Licencia ligada a este equipo</p></div></div>
        <div className="lbl" style={{ marginTop: 18 }}>PASO 1 · TU HWID</div>
        <div className="inp">
          <input type="text" readOnly value={status.hwid || "No disponible"} aria-label="HWID" onFocus={(e) => e.currentTarget.select()} />
          <button className="btn sm" onClick={copy} disabled={!status.hwid}><Copy size={13} style={{ verticalAlign: -2 }} /> Copiar</button>
        </div>
        <p className="sub" style={{ marginTop: 8 }}>Envía este HWID al creador (444) para recibir tu KEY.</p>
        <div className="lbl" style={{ marginTop: 18 }}>PASO 2 · TU KEY</div>
        <textarea className="lic-key" value={key} onChange={(e) => setKey(e.target.value)} placeholder="XXXXXXXX-XXXXXXXX-XXXXXXXX-…" spellCheck={false} aria-label="KEY" />
        {err && <div className="warn" role="alert">⛔ {err}</div>}
        <button className="btn primary" disabled={busy} onClick={activate}><KeyRound size={14} style={{ verticalAlign: -2 }} /> {busy ? "Comprobando…" : "Activar"}</button>
        <div className="chk" style={{ marginTop: 14 }}><ShieldCheck size={15} />La KEY se verifica en tu equipo con una firma digital; no se envía nada a Internet.</div>
        <div className="row-end" style={{ justifyContent: "flex-start" }}>
          <button className="btn sm" onClick={() => openUrl("https://discord.gg/444cfg")}>DISCORD</button>
        </div>
      </div>
    </div>
  );
}
