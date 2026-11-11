# 444CFG TOOL — FiveM Configuration Manager

Creador: **444** · Comunidad: **444CFG** · Discord: https://discord.gg/444cfg

Aplicación de escritorio para Windows 10/11 (Tauri 2 + Rust + React + TypeScript) que instala, cambia y retira
Citizens, mods, armas (MK2 / 9mm), Blood FX, Tracers y Sound Packs de FiveM **archivo por archivo**, con manifiestos,
capas, copias de seguridad por SHA-256, simulación previa y restauración transaccional. Incluye un **explorador de
archivos** acotado a las carpetas de FiveM / GTA V en cada sección y un **sistema de licencias** (Ed25519 + HWID).

> **Estado honesto:** el código está escrito pero **no se ha compilado ni ejecutado** (el entorno donde se creó no tiene
> Rust). Lee `PROJECT_STATUS.md` y `TEST_RESULTS.md` antes de usarlo.

## ANTES DE COMPILAR: clave pública de licencias (obligatorio)

1. Abre en tu ordenador `444CFG_KEYS_ADMIN_PRIVADO.html` (se entrega **aparte**, fuera de este proyecto) con Chrome o Edge.
2. Pulsa **Crear las dos claves** y guarda `private.key` y `public.key`. **`private.key` jamás se sube a GitHub.**
3. Copia el contenido de `public.key` (64 caracteres) dentro de `src-tauri/public.key`, sustituyendo el texto de relleno.
4. Sube el proyecto. El workflow se detiene con un mensaje claro si `public.key` no es válida o si detecta una clave privada.

Para dar una licencia: el cliente abre 444CFG TOOL, copia su **HWID** de la pantalla de activación y tú lo firmas en el
generador (paso 2 del HTML) con tu `private.key`; le envías la **KEY**. La verificación se hace en su equipo con la clave
pública; no hay servidor. Las KEYS de 444 RES TOOL **no** valen (identificador distinto: `444CFGTOOL|v1|`).

## Compilar sin instalar Rust (GitHub Actions)

Ver `GITHUB_INSTRUCCIONES.md`. El flujo comprueba la clave pública, TypeScript, ejecuta `cargo test -p engine`,
compila y genera el instalador y el `.exe` portable.

## Estructura

- `engine/` — motor (Rust, sin Tauri): importación, planificador, ejecutor, detección MK2/9mm (`weapon.rs`),
  explorador acotado (`fm.rs`), verificación de licencias (`license.rs`). Pruebas en `engine/tests/engine_tests.rs`.
- `src-tauri/` — comandos Tauri, rutas (`CitizenFX.ini`), ajustes, historial, HWID y activación. `public.key`: clave pública.
- `src/` — interfaz React. `components/FileExplorer.tsx` es el panel de archivos; `pages/License.tsx` la activación.
- `.github/workflows/build-windows.yml` — compilación en Windows.

## Seguridad

Nunca se borran carpetas completas. Solo se retiran archivos del manifiesto cuyo hash coincide con el instalado.
El explorador no puede salir de las carpetas configuradas, no toca lo instalado por la herramienta y guarda copia
restaurable antes de sustituir o eliminar. La clave privada de licencias no existe en este proyecto.
