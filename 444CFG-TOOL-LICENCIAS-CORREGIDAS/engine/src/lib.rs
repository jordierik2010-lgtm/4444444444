//! Motor de instalaciones de 444CFG TOOL: manifiestos, capas, backups por hash,
//! simulación previa y restauración transaccional. No depende de Tauri.
pub mod manager;
pub mod exec;
pub mod fm;
pub mod import;
pub mod license;
pub mod model;
pub mod pathsafe;
pub mod planner;
pub mod rpf;
pub mod store;
pub mod weapon;

pub use manager::{Engine, FileCheck};
pub use exec::{ExecOptions, Progress};
pub use model::*;
pub use planner::{ManualConflict, Op, OpKind, Plan, Summary};
