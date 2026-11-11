export type Category = "citizen" | "mod" | "weapon" | "blood_fx" | "tracer" | "sound_pack";
export interface FileSpec { root: "fivem" | "gta"; rel: string; sha256: string; size: number }
export interface Package {
  id: string; name: string; category: Category; slot: string | null; files: FileSpec[];
  warnings: string[]; mount_ids: string[]; contents: string[]; needs_ack_unverified: boolean; created_at: number;
  weapon_kind: "mk2" | "9mm" | null; weapon_evidence: string[];
}
export interface Install {
  id: string; package_id: string; name: string; category: Category; slot: string | null;
  installed_at: number; files: FileSpec[]; created_dirs: unknown[]; mount_ids: string[]; linked_to: string | null;
  weapon_kind?: string | null; displaced?: FileSpec[];
}
export interface ManualConflict { rel: string; kind: string | null; reason: string; size: number }
export interface Summary {
  retire_old: number; install_new: number; unchanged: number; restore_originals: number; reveal_layers: number; keep_others: number;
}
export interface Plan {
  kind: "install" | "uninstall"; title: string; package_id: string | null; removes: string[];
  ops: { root: string; rel: string; kind: string; why: string }[];
  summary: Summary; blockers: string[]; warnings: string[]; unknown_overwrites: string[]; needs_ack_unverified: boolean;
  ack_notes: string[]; manual_conflicts: ManualConflict[]; weapon_kind: string | null;
}
export interface ProgressInfo { done_bytes: number; total_bytes: number; ops_done?: number; ops_total?: number; current?: string }
export interface PathCheck { label: string; path: string | null; ok: boolean; detail: string }
export interface LicenseStatus { activated: boolean; hwid: string; error: string | null }
export interface FmArea { id: string; label: string; root: string; rel: string; writable: boolean; exists: boolean }
export interface FmEntry {
  name: string; sub: string; is_dir: boolean; size: number; ext: string; modified: number | null;
  status: "managed" | "managed_dir" | "external" | "folder" | "link"; owner: string | null; note: string | null; can_modify: boolean;
}
export interface FmListing { area: FmArea; dir: string; entries: FmEntry[] }
export interface FmBackup { id: string; ts: number; action: "delete" | "replace" | "add"; root: string; rel: string; old_sha: string | null; new_sha: string | null }
export interface FileCheck { root: string; path: string; status: string }
export interface UiSettings {
  animations: boolean; effect: Effect; intensity: number; speed: number; count: number;
}
export type Effect = "particles" | "snow" | "rain" | "stars" | "fog" | "lines" | "shine" | "none";
export interface Settings {
  fivem_path: string | null; gta_path: string | null; backup_dir: string | null; sound_mode: string | null;
  ui: Partial<UiSettings>;
}
export interface Detected { fivem: string | null; gta: string | null; note: string }
export interface Dashboard { fivem_running: boolean; fivem_path: string | null; history: { ts: number; action: string; name: string; category: string; ok: boolean }[] }
