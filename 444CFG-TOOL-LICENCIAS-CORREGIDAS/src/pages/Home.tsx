import { useEffect, useState } from "react";
import { api } from "../api";
import type { Dashboard, Install, Settings } from "../types";
import { catLabel, fmtDate } from "../ui";

export default function Home({ settings, go }: { settings: Settings; go: (p: string) => void }) {
  const [dash, setDash] = useState<Dashboard | null>(null);
  const [installs, setInstalls] = useState<Install[]>([]);
  useEffect(() => {
    api.dashboard().then(setDash).catch(() => {});
    api.listInstalls().then(setInstalls).catch(() => {});
  }, []);
  const active = (c: string) => installs.find((i) => i.category === c)?.name ?? "Ninguno";
  const count = (c: string) => installs.filter((i) => i.category === c).length;
  const rows: [string, string][] = [
    ["Estado de FiveM", settings.fivem_path ? (dash?.fivem_running ? "En ejecución" : "Cerrado") : "Carpeta sin configurar"],
    ["Citizen activo", active("citizen")],
    ["Mods instalados", String(count("mod"))],
    ["Armas instaladas", String(count("weapon"))],
    ["Blood FX activo", active("blood_fx")],
    ["Tracer activo", active("tracer")],
    ["Sound Pack activo", active("sound_pack")]
  ];
  return (
    <div className="page">
      <div className="hero"><img src="/logo444.png" alt="Logo 444" /><div><h1>444CFG TOOL</h1><p className="sub">FiveM Configuration Manager</p></div></div>
      <div className="grid2">
        <div className="pn">
          <div className="lbl">ESTADO</div>
          {rows.map(([a, b]) => (<div className="r" key={a}><span>{a}</span><b>{b}</b></div>))}
        </div>
        <div className="pn">
          <div className="lbl">ÚLTIMAS OPERACIONES</div>
          {dash && dash.history.length ? [...dash.history].reverse().slice(0, 8).map((h, i) => (
            <div className="r" key={i}><span>{h.action} · {h.name} <small>({catLabel[h.category] ?? h.category})</small></span><b style={{ color: h.ok ? undefined : "#c9a0a0" }}>{h.ok ? "Correcto" : "Falló"} · {fmtDate(h.ts)}</b></div>
          )) : <p className="sub">Todavía no se ha realizado ninguna operación.</p>}
        </div>
      </div>
      <div className="lbl" style={{ marginTop: 22 }}>ACCESOS RÁPIDOS</div>
      <div className="quick">
        <button className="btn" onClick={() => go("citizen")}>Instalar Citizen</button>
        <button className="btn" onClick={() => go("mod")}>Importar mod</button>
        <button className="btn" onClick={() => go("weapon")}>Añadir arma</button>
        <button className="btn" onClick={() => go("sound_pack")}>Instalar Sound Pack</button>
        <button className="btn" onClick={() => go("installs")}>Restaurar archivos</button>
      </div>
    </div>
  );
}
