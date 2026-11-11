import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errText } from "../api";
import type { LicenseStatus, PathCheck, Settings } from "../types";
import { useToast } from "../ui";
import { VERSION } from "../constants";

export default function SettingsPage({ settings, onSave, license }: { settings: Settings; onSave: (s: Settings) => Promise<void>; license: LicenseStatus | null }) {
  const toast = useToast();
  const [s, setS] = useState<Settings>(settings);
  const [checks, setChecks] = useState<PathCheck[]>([]);
  const runChecks = () => api.checkPaths().then(setChecks).catch((e) => toast(errText(e), "err"));
  useEffect(() => { runChecks(); }, [settings]);
  const pick = async (key: "fivem_path" | "gta_path" | "backup_dir") => {
    const r = await open({ directory: true, multiple: false });
    if (typeof r === "string") setS({ ...s, [key]: r });
  };
  const apply = async (next: Settings) => { try { await onSave(next); setS(next); toast("Ajustes guardados"); await runChecks(); } catch (e) { toast(errText(e), "err"); setS(settings); } };
  const detect = async () => {
    const d = await api.detectPaths();
    const next = { ...s, fivem_path: d.fivem ?? s.fivem_path, gta_path: d.gta ?? s.gta_path };
    setS(next);
    toast(d.fivem || d.gta ? `Detectado${d.note ? " · " + d.note : ""}` : d.note || "No se detectó nada", d.fivem || d.gta ? "ok" : "err");
  };
  const exportErrors = async () => {
    const dest = await save({ defaultPath: "444cfg-errores.log" });
    if (dest) { try { await api.exportErrors(dest); toast("Registro de errores exportado"); } catch (e) { toast(errText(e), "err"); } }
  };
  const row = (label: string, key: "fivem_path" | "gta_path" | "backup_dir", ph: string) => (
    <div className="field"><label>{label}</label>
      <div className="inp"><input value={s[key] ?? ""} placeholder={ph} onChange={(e) => setS({ ...s, [key]: e.target.value || null })} /><button className="btn sm" onClick={() => pick(key)}>Examinar</button></div>
    </div>
  );
  return (
    <div className="page">
      <h1>Ajustes</h1>
      <p className="sub">Las rutas se detectan mediante CitizenFX.ini y pueden cambiarse manualmente.</p>
      <div className="grid2">
        <div className="pn">
          <div className="lbl">RUTAS</div>
          {row("Carpeta FiveM.app", "fivem_path", "%LOCALAPPDATA%\\FiveM\\FiveM.app")}
          {row("Carpeta de GTA V", "gta_path", "Se lee de CitizenFX.ini (IVPath)")}
          {row("Copias de seguridad", "backup_dir", "Predeterminada: datos de la aplicación")}
          <div className="field"><label>Sound Packs: método de instalación</label>
            <select value={s.sound_mode ?? "gta_sfx"} onChange={(e) => setS({ ...s, sound_mode: e.target.value })}>
              <option value="gta_sfx">GTAV\x64\audio\sfx (modifica la carpeta del juego, con copia de seguridad)</option>
              <option value="mods_dir">FiveM.app\mods\x64\audio\sfx (no toca la carpeta del juego; sin verificar en todos los equipos)</option>
            </select></div>
          {checks.map((c) => (<div className={c.ok ? "note" : "warn"} key={c.label}>{c.ok ? "✔" : "⚠"} {c.label}: {c.detail}</div>))}
          <div className="row-end"><button className="btn" onClick={detect}>Detectar automáticamente</button><button className="btn primary" onClick={() => apply(s)}>Guardar</button></div>
        </div>
        <div className="pn">
          <div className="lbl">ACERCA DE 444CFG</div>
          <div className="about"><img src="/logo444.png" alt="Logo 444" /><div><b>444CFG TOOL</b><small>FiveM Configuration Manager · v{VERSION}</small><small>Creador: 444 · Comunidad: 444CFG</small></div></div>
          <div className="lbl" style={{ marginTop: 6 }}>LICENCIA</div>
          <div className="r"><span>Estado</span><b>{license?.activated ? "Activada" : "No activada"}</b></div>
          <div className="r"><span>HWID</span><b style={{ fontFamily: "Consolas,monospace", fontSize: 12 }}>{license?.hwid ?? "—"}</b></div>
          <div className="row-end" style={{ justifyContent: "flex-start" }}>
            <button className="btn" onClick={() => openUrl("https://discord.gg/444cfg")}>DISCORD</button>
            <button className="btn" onClick={exportErrors}>Exportar errores</button>
          </div>
        </div>
      </div>
    </div>
  );
}
