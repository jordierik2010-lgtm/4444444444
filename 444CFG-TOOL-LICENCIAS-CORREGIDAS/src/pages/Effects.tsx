import Background from "../components/Background";
import type { Effect, UiSettings } from "../types";

const LIST: [Effect, string][] = [["particles", "Partículas"], ["snow", "Nieve"], ["rain", "Lluvia"], ["stars", "Estrellas"], ["fog", "Niebla"], ["lines", "Líneas geométricas"], ["shine", "Brillos metálicos"], ["none", "Sin efectos"]];

export default function Effects({ ui, onChange }: { ui: UiSettings; onChange: (u: UiSettings) => void }) {
  const set = (p: Partial<UiSettings>) => onChange({ ...ui, ...p });
  return (
    <div className="page">
      <h1>Efectos</h1>
      <p className="sub">Efectos visuales de la propia herramienta. No afectan al clima ni a los mods de FiveM. Los cambios se guardan automáticamente.</p>
      <div className="grid2">
        <div className="pn">
          <div className="lbl">EFECTO</div>
          <div className="chips">
            {LIST.map(([id, l]) => (<button key={id} className={`chip${ui.effect === id ? " on" : ""}`} aria-pressed={ui.effect === id} onClick={() => set({ effect: id })}>{l}</button>))}
          </div>
          <label className="inline" style={{ marginTop: 16 }}><input type="checkbox" checked={ui.animations} onChange={(e) => set({ animations: e.target.checked })} /> Animaciones activadas</label>
          <label className="slider">Intensidad <input type="range" min={0.1} max={1} step={0.05} value={ui.intensity} onChange={(e) => set({ intensity: +e.target.value })} /><b>{Math.round(ui.intensity * 100)}%</b></label>
          <label className="slider">Velocidad <input type="range" min={0.2} max={3} step={0.1} value={ui.speed} onChange={(e) => set({ speed: +e.target.value })} /><b>{ui.speed.toFixed(1)}×</b></label>
          <label className="slider">Cantidad <input type="range" min={10} max={300} step={5} value={ui.count} onChange={(e) => set({ count: +e.target.value })} /><b>{ui.count}</b></label>
        </div>
        <div className="pn">
          <div className="lbl">VISTA PREVIA</div>
          <div className="preview"><Background effect={ui.effect} intensity={ui.intensity} speed={ui.speed} count={ui.count} enabled={ui.animations} className="prev-canvas" /></div>
        </div>
      </div>
    </div>
  );
}
