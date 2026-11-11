import { useCallback, useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Home as HomeIcon, Layers, Boxes, Crosshair, Droplet, Zap, Volume2, Sparkles, ListChecks, Settings as Cog, MessageCircle } from "lucide-react";
import Background from "./components/Background";
import { api, errText } from "./api";
import { ToastHost } from "./ui";
import { VERSION, DEFAULT_UI } from "./constants";
import type { Category, LicenseStatus, Settings, UiSettings } from "./types";
import Home from "./pages/Home";
import CategoryPage from "./pages/CategoryPage";
import Installs from "./pages/Installs";
import Effects from "./pages/Effects";
import SettingsPage from "./pages/SettingsPage";
import License from "./pages/License";


type Page = "home" | Category | "effects" | "installs" | "settings";
const NAV: { id: Page; label: string; icon: JSX.Element }[] = [
  { id: "home", label: "Inicio", icon: <HomeIcon size={16} /> },
  { id: "citizen", label: "Citizen", icon: <Layers size={16} /> },
  { id: "mod", label: "Mods", icon: <Boxes size={16} /> },
  { id: "weapon", label: "Armas", icon: <Crosshair size={16} /> },
  { id: "blood_fx", label: "Blood FX", icon: <Droplet size={16} /> },
  { id: "tracer", label: "Tracers", icon: <Zap size={16} /> },
  { id: "sound_pack", label: "Sound Packs", icon: <Volume2 size={16} /> },
  { id: "effects", label: "Efectos", icon: <Sparkles size={16} /> },
  { id: "installs", label: "Mis instalaciones", icon: <ListChecks size={16} /> },
  { id: "settings", label: "Ajustes", icon: <Cog size={16} /> }
];

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  const [lic, setLic] = useState<LicenseStatus | null>(null);

  const reload = useCallback(async () => {
    try {
      let s = await api.getSettings();
      if (!s.fivem_path) {
        const d = await api.detectPaths();
        if (d.fivem) {
          s = { ...s, fivem_path: d.fivem, gta_path: s.gta_path ?? d.gta };
          await api.saveSettings(s);
        }
      }
      setSettings(s);
      setLic(await api.licenseStatus());
    } catch (e) { setFatal(errText(e)); }
  }, []);
  useEffect(() => { reload(); }, [reload]);

  const ui: UiSettings = { ...DEFAULT_UI, ...(settings?.ui ?? {}) };
  const saveSettings = async (s: Settings) => { await api.saveSettings(s); setSettings(s); };
  const win = getCurrentWindow();

  const locked = !!settings && !!lic && !lic.activated;
  let body: JSX.Element;
  if (locked && lic) body = <License status={lic} onActivated={setLic} />;
  else if (!settings || !lic) body = <div className="page"><p className="sub">{fatal ?? "Cargando…"}</p></div>;
  else if (page === "home") body = <Home settings={settings} go={(p) => setPage(p as Page)} />;
  else if (page === "effects") body = <Effects ui={ui} onChange={(u) => saveSettings({ ...settings, ui: u })} />;
  else if (page === "installs") body = <Installs />;
  else if (page === "settings") body = <SettingsPage settings={settings} onSave={saveSettings} license={lic} />;
  else body = <CategoryPage key={page} category={page as Category} settings={settings} />;

  return (
    <ToastHost>
      <div className="w">
        <Background effect={ui.effect} intensity={ui.intensity} speed={ui.speed} count={ui.count} enabled={ui.animations} />
        <aside className="sb">
          <div className="brand" data-tauri-drag-region>
            <img src="/logo444.png" alt="Logo 444" />
            <div><div className="brand-n">444CFG TOOL</div><div className="brand-v">v{VERSION}</div></div>
          </div>
          <nav aria-label="Secciones">
            {NAV.map((n) => (
              <button key={n.id} disabled={locked} className={`nav${page === n.id ? " on" : ""}`} aria-current={page === n.id ? "page" : undefined} onClick={() => setPage(n.id)}>
                {n.icon}{n.label}
              </button>
            ))}
          </nav>
          <div style={{ flex: 1 }} />
          <div className="user">
            <img src="/logo444.png" alt="" />
            <div style={{ flex: 1 }}><b>CREADOR: 444</b><small>COMUNIDAD: 444CFG</small></div>
            <button className="wc" aria-label="Discord" title="DISCORD" onClick={() => openUrl("https://discord.gg/444cfg")}><MessageCircle size={16} /></button>
          </div>
        </aside>
        <main className="main">
          <div className="titlebar" data-tauri-drag-region>
            <button className="wc" aria-label="Minimizar" onClick={() => win.minimize()}>—</button>
            <button className="wc" aria-label="Cerrar" onClick={() => win.close()}>✕</button>
          </div>
          <AnimatePresence mode="wait">
            <motion.div key={page} className="pagewrap" initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -6 }} transition={{ duration: 0.18 }}>
              {body}
            </motion.div>
          </AnimatePresence>
        </main>
      </div>
    </ToastHost>
  );
}
