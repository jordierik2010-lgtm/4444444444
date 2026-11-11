import { useEffect, useRef } from "react";
import type { Effect } from "../types";

interface Props { effect: Effect; intensity: number; speed: number; count: number; enabled: boolean; className?: string }
interface P { x: number; y: number; vx: number; vy: number; r: number; a: number }

export default function Background({ effect, intensity, speed, count, enabled, className }: Props) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cv = ref.current;
    if (!cv) return;
    const ctx = cv.getContext("2d")!;
    let raf = 0, w = 0, h = 0, t = 0;
    const mouse = { x: -9999, y: -9999 };
    const resize = () => {
      const r = cv.getBoundingClientRect();
      const d = Math.min(window.devicePixelRatio || 1, 2);
      w = r.width; h = r.height; cv.width = w * d; cv.height = h * d;
      ctx.setTransform(d, 0, 0, d, 0, 0);
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(cv);
    const move = (e: MouseEvent) => { const r = cv.getBoundingClientRect(); mouse.x = e.clientX - r.left; mouse.y = e.clientY - r.top; };
    const leave = () => { mouse.x = mouse.y = -9999; };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseleave", leave);
    const sp = Math.max(0.1, speed);
    const n = Math.max(5, Math.min(count, 400));
    const mk = (): P => ({ x: Math.random() * w, y: Math.random() * h, vx: (Math.random() - 0.5) * 0.4 * sp, vy: (Math.random() - 0.5) * 0.4 * sp, r: 1 + Math.random() * 1.6, a: 0.3 + Math.random() * 0.7 });
    const ps: P[] = Array.from({ length: n }, mk);
    const flakes: P[] = Array.from({ length: n }, () => ({ ...mk(), vx: (Math.random() - 0.5) * 0.5 * sp, vy: (0.4 + Math.random() * 1.2) * sp }));
    const drops: P[] = Array.from({ length: n }, () => ({ ...mk(), vx: -0.6 * sp, vy: (6 + Math.random() * 6) * sp }));
    const stars: P[] = Array.from({ length: n * 2 }, () => ({ ...mk(), vx: 0, vy: 0, a: Math.random() * Math.PI * 2 }));
    const fog = Array.from({ length: 7 }, () => ({ x: Math.random() * w, y: Math.random() * h, r: 160 + Math.random() * 220, vx: (Math.random() - 0.5) * 0.15 * sp }));
    const tick = () => {
      t += 0.016 * sp;
      ctx.clearRect(0, 0, w, h);
      ctx.globalAlpha = Math.max(0, Math.min(1, intensity));
      if (effect === "particles") {
        for (const p of ps) {
          p.x += p.vx; p.y += p.vy;
          if (p.x < 0 || p.x > w) p.vx *= -1;
          if (p.y < 0 || p.y > h) p.vy *= -1;
          const dx = p.x - mouse.x, dy = p.y - mouse.y, d = Math.hypot(dx, dy);
          if (d < 120 && d > 0) { p.x += (dx / d) * 1.2; p.y += (dy / d) * 1.2; }
        }
        for (let i = 0; i < ps.length; i++) {
          for (let j = i + 1; j < ps.length; j++) {
            const d = Math.hypot(ps[i].x - ps[j].x, ps[i].y - ps[j].y);
            if (d < 140) { ctx.strokeStyle = `rgba(210,212,220,${0.18 * (1 - d / 140)})`; ctx.lineWidth = 0.7; ctx.beginPath(); ctx.moveTo(ps[i].x, ps[i].y); ctx.lineTo(ps[j].x, ps[j].y); ctx.stroke(); }
          }
          const m = Math.hypot(ps[i].x - mouse.x, ps[i].y - mouse.y);
          if (m < 170) { ctx.strokeStyle = `rgba(124,134,176,${0.35 * (1 - m / 170)})`; ctx.beginPath(); ctx.moveTo(ps[i].x, ps[i].y); ctx.lineTo(mouse.x, mouse.y); ctx.stroke(); }
          ctx.fillStyle = `rgba(160,162,170,${ps[i].a})`; ctx.beginPath(); ctx.arc(ps[i].x, ps[i].y, ps[i].r, 0, 7); ctx.fill();
        }
      } else if (effect === "snow") {
        ctx.fillStyle = "#f1f1f3";
        for (const f of flakes) { f.x += f.vx + Math.sin(t + f.y * 0.01) * 0.3; f.y += f.vy; if (f.y > h) { f.y = -5; f.x = Math.random() * w; } ctx.globalAlpha = intensity * f.a; ctx.beginPath(); ctx.arc(f.x, f.y, f.r, 0, 7); ctx.fill(); }
      } else if (effect === "rain") {
        ctx.strokeStyle = "rgba(190,195,210,0.7)"; ctx.lineWidth = 1;
        for (const d of drops) { d.x += d.vx; d.y += d.vy; if (d.y > h) { d.y = -20; d.x = Math.random() * w; } ctx.globalAlpha = intensity * d.a; ctx.beginPath(); ctx.moveTo(d.x, d.y); ctx.lineTo(d.x + d.vx * 2, d.y - d.vy * 1.6); ctx.stroke(); }
      } else if (effect === "stars") {
        for (const s of stars) { s.a += 0.02 * sp; ctx.globalAlpha = intensity * (0.35 + 0.65 * Math.abs(Math.sin(s.a))); ctx.fillStyle = "#e8e9ee"; ctx.beginPath(); ctx.arc(s.x, s.y, s.r * 0.7, 0, 7); ctx.fill(); }
      } else if (effect === "fog") {
        for (const f of fog) { f.x += f.vx; if (f.x < -f.r) f.x = w + f.r; if (f.x > w + f.r) f.x = -f.r; const g = ctx.createRadialGradient(f.x, f.y, 0, f.x, f.y, f.r); g.addColorStop(0, "rgba(150,154,168,0.22)"); g.addColorStop(1, "rgba(150,154,168,0)"); ctx.globalAlpha = intensity; ctx.fillStyle = g; ctx.fillRect(f.x - f.r, f.y - f.r, f.r * 2, f.r * 2); }
      } else if (effect === "lines") {
        ctx.strokeStyle = "rgba(200,203,214,0.28)"; ctx.lineWidth = 0.8;
        const cx = w / 2, cy = h / 2, k = Math.max(4, Math.round(n / 8));
        for (let i = 0; i < k; i++) { const a = t * 0.15 + (i * Math.PI * 2) / k, r = Math.max(w, h) * (0.15 + 0.5 * ((i % 5) / 5)); ctx.beginPath(); for (let s = 0; s < 4; s++) { const aa = a + (s * Math.PI) / 2; const x = cx + Math.cos(aa) * r, y = cy + Math.sin(aa) * r; s ? ctx.lineTo(x, y) : ctx.moveTo(x, y); } ctx.closePath(); ctx.stroke(); }
      } else if (effect === "shine") {
        const x = ((t * 80) % (w + 400)) - 200;
        const g = ctx.createLinearGradient(x - 120, 0, x + 120, h);
        g.addColorStop(0, "rgba(255,255,255,0)"); g.addColorStop(0.5, "rgba(214,218,232,0.16)"); g.addColorStop(1, "rgba(255,255,255,0)");
        ctx.globalAlpha = intensity; ctx.fillStyle = g; ctx.fillRect(0, 0, w, h);
      }
      ctx.globalAlpha = 1;
      raf = requestAnimationFrame(tick);
    };
    if (enabled && effect !== "none") raf = requestAnimationFrame(tick);
    else ctx.clearRect(0, 0, w, h);
    return () => { cancelAnimationFrame(raf); ro.disconnect(); window.removeEventListener("mousemove", move); window.removeEventListener("mouseleave", leave); };
  }, [effect, intensity, speed, count, enabled]);
  return <canvas ref={ref} className={className ?? "bg-canvas"} aria-hidden="true" />;
}
