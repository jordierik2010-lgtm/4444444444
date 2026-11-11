# Arquitectura

## Almacén (fuera de las carpetas del juego)
`<datos de la app>/store` (cambiable en Ajustes si no hay instalaciones):
- `blobs/<sha256>` — contenido de paquetes importados **y** copias del estado previo de cualquier archivo que se
  sobrescriba o borre (direccionado por hash, deduplicado).
- `library/<id>.json` — paquetes importados (id = SHA-256 del contenido + categoría).
- `state.json` — instalaciones y registros por archivo. `journal.json` — diario de la operación en curso.

## Modelo de capas
Cada archivo gestionado tiene un `Record`: `original` (hash del contenido previo, si existía) y una pila de `Layer`
(instalación + hash). La última capa es la que está en disco. Prioridad por categoría:
Citizen 0 · Mod/Arma 1 · Blood FX/Tracer/Sound Pack 2. Una capa de mayor prioridad se apila sobre una menor
(p. ej. `bloodfx.dat` de un Blood FX sobre el del Citizen). Dos instalaciones de igual prioridad con distinto
contenido en el mismo archivo = **conflicto → bloqueo**. Mismo hash → archivo compartido (se conserva hasta que
se va el último propietario).

## Planificación (simulación)
`plan_install` / `plan_uninstall` trabajan sobre una copia del estado y un disco virtual, sin escribir nada:
1. Bloqueos: paquete ya instalado, contenido ausente, raíz inexistente, **identificador de montaje compartido**
   (`device:<deviceName>` / `dlc:<destino de assembly.xml>`), vínculos inexistentes.
2. Se retira el paquete del mismo *slot* (Citizen, Blood FX, Tracer, Sound Pack o modelo de arma) y los mods
   vinculados a él. Los archivos idénticos del paquete anterior se «re-etiquetan» (no se tocan en disco).
3. Por cada archivo retirado: si no es la capa superior solo se quita la capa; si lo es, el disco debe coincidir con el
   hash registrado (si no → bloqueo «modificado fuera de la aplicación»); se revela la capa inferior, se restaura el
   original o se borra el archivo si lo creó el paquete.
4. Carpetas: solo se quitan las creadas por el paquete, si están vacías (`remove_dir`, nunca recursivo).
5. Archivos existentes no gestionados que se sustituirían → lista de confirmación; se guarda copia de su estado previo.

El resultado es un `Plan` con operaciones `Write{sha, expect}`, `Delete{expect}` y `RemoveDir`, el resumen
(retirados / nuevos / restaurados / conservados) y los bloqueos.

## Ejecución transaccional
1. Precondiciones: sin bloqueos, FiveM/GTA cerrados, confirmaciones, espacio libre suficiente.
2. Diario (`journal.json`) con todas las operaciones y los hashes del estado antes/después.
3. Cada operación comprueba que el hash actual coincide con `expect`, guarda copia por hash antes de sobrescribir o
   borrar, escribe a un temporal en la misma carpeta + `rename`, y verifica el hash resultante.
4. Verificación final de los archivos de la nueva instalación; después se escribe `state.json` (atómico) y se borra el diario.
5. Cualquier fallo → reversión inversa e idempotente desde las copias por hash. Si la app se cierra a mitad,
   `recover()` al abrir revierte (o cierra si `state.json` ya era el nuevo).
6. La interfaz **vuelve a planificar** justo antes de aplicar; nunca ejecuta un plan antiguo.

## Importación
ZIP (detectado por firma; los `.rar` se rechazan con mensaje), RPF o archivos sueltos. Todas las rutas se validan
(`..`, absolutas, unidades, nombres reservados, enlaces simbólicos, ZIP cifrado) **antes** de guardar nada; límite
de 8 GiB (anti ZIP bomb). Clasificación por categoría con motivo explícito de incompatibilidad.
Los RPF7 con índice sin cifrar se inspeccionan (incluidos RPF anidados): `assembly.xml`, `setup2.xml`,
modelos `w_*.ydr`. No se usan ni se incluyen claves de GTA V; los RPF con índice cifrado se tratan como archivos completos.

## Rutas
`FiveM.app` (autodetección `%LOCALAPPDATA%\FiveM\FiveM.app`), GTA V desde `IVPath=` en `CitizenFX.ini`; ambas editables.
Citizen → `FiveM.app\citizen`; RPF → `FiveM.app\mods`; Blood FX dat → `citizen\common\data\effects`;
Tracers XML → `citizen\common\data\timecycle`; Sound Packs → `GTAV\x64\audio\sfx` o (opción) `FiveM.app\mods\x64\audio\sfx`.

## Novedades de esta revisión
- **`weapon.rs`**: detecta MK2 / 9mm por contenido (modelos) y nombre; el «hueco» de una skin de arma es `weapon:mk2` / `weapon:9mm`
  (si no se identifica, el antiguo `weapon:<modelos>`). Las instalaciones antiguas se asignan a su arma al planificar.
- **Skins manuales**: `Install.displaced` guarda los archivos ajenos retirados al instalar (copia por hash en el almacén). Al sustituir A→B
  se traspasan; al retirar se devuelven si la ruta sigue libre.
- **Montaje compartido entre MK2 y 9mm**: confirmación expresa (`Plan.ack_notes`) en lugar de bloqueo; entre otras categorías sigue bloqueando.
- **Citizen**: `citizen/` puede venir dentro de carpetas envoltorio; los RPF (validados como RPF7) van a `mods/`; lo no compatible se omite con aviso
  (también en Sound Packs). Otras categorías siguen rechazando el paquete entero.
- **Progreso**: `exec::Progress` (bytes y operaciones) por el evento `progress`; importación con `import-progress`; explorador con `fm-progress`.
- **`fm.rs`**: áreas por sección (`mods`, `citizen` y `gta_sfx` solo lectura, `effects`, `timecycle`), rutas validadas con `Roots::resolve`, lo gestionado
  protegido, copias por hash + `fm_log.json` para restaurar. Carpetas solo si están vacías; ejecutables bloqueados al importar.
- **`license.rs`** + `src-tauri/public.key`: Ed25519 sobre `444CFGTOOL|v1|<HWID>`. HWID = SHA-256 del `MachineGuid` con sal propia (32 hex).
  Los comandos de Tauri exigen licencia (`require_license`).
