import { invoke } from "@tauri-apps/api/core";
import type { Category, Dashboard, Detected, FileCheck, FmArea, FmBackup, FmListing, Install, LicenseStatus, Package, PathCheck, Plan, Settings } from "./types";

export const api = {
  detectPaths: () => invoke<Detected>("detect_paths"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  importPackage: (category: Category, paths: string[], name?: string) =>
    invoke<Package>("import_package", { category, paths, name: name ?? null }),
  listPackages: () => invoke<Package[]>("list_packages"),
  deletePackage: (id: string) => invoke<void>("delete_package", { id }),
  listInstalls: () => invoke<Install[]>("list_installs"),
  planInstall: (packageId: string, linkTo?: string | null) => invoke<Plan>("plan_install", { packageId, linkTo: linkTo ?? null }),
  planUninstall: (id: string) => invoke<Plan>("plan_uninstall", { id }),
  applyInstall: (packageId: string, linkTo: string | null, confirmedUnknown: boolean, ackUnverified: boolean) =>
    invoke<void>("apply_install", { packageId, linkTo, confirmedUnknown, ackUnverified }),
  applyUninstall: (id: string, confirmedUnknown: boolean) => invoke<void>("apply_uninstall", { id, confirmedUnknown }),
  verifyInstall: (id: string) => invoke<FileCheck[]>("verify_install", { id }),
  openInstallLocation: (id: string) => invoke<void>("open_install_location", { id }),
  dashboard: () => invoke<Dashboard>("dashboard"),
  exportErrors: (dest: string) => invoke<void>("export_errors", { dest }),
  setWeaponKind: (packageId: string, kind: string | null) => invoke<Package>("set_weapon_kind", { packageId, kind }),
  checkPaths: () => invoke<PathCheck[]>("check_paths"),
  licenseStatus: () => invoke<LicenseStatus>("license_status"),
  licenseActivate: (key: string) => invoke<LicenseStatus>("license_activate", { key }),
  fmAreas: (section: Category) => invoke<FmArea[]>("fm_areas", { section }),
  fmList: (section: Category, area: string, sub: string) => invoke<FmListing>("fm_list", { section, area, sub }),
  fmImport: (section: Category, area: string, sub: string, paths: string[], replace: boolean) =>
    invoke<string[]>("fm_import", { section, area, sub, paths, replace }),
  fmReplace: (section: Category, area: string, sub: string, source: string) => invoke<void>("fm_replace", { section, area, sub, source }),
  fmDelete: (section: Category, area: string, subs: string[]) => invoke<string[]>("fm_delete", { section, area, subs }),
  fmBackups: () => invoke<FmBackup[]>("fm_backups"),
  fmRestore: (id: string) => invoke<string>("fm_restore", { id }),
  fmOpen: (section: Category, area: string, sub: string) => invoke<void>("fm_open", { section, area, sub })
};

export const errText = (e: unknown): string => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
