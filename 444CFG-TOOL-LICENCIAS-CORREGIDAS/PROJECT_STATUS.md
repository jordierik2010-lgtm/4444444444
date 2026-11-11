# Estado del proyecto

**Nada de este proyecto ha sido compilado ni ejecutado.** El entorno de desarrollo no tiene Rust ni acceso a Internet.
Todo lo que sigue es «escrito, sin probar» salvo lo marcado como VERIFICADO.

## VERIFICADO de verdad en este entorno
- **Criptografía de las KEYS**: el generador de administrador adaptado (`444CFG_KEYS_ADMIN_PRIVADO.html`) se ejecutó (crear claves y
  firmar un HWID) y la KEY resultante se verificó con Python `cryptography` (Ed25519). El vector de prueba de `engine/tests/vector_*.txt`
  se generó con esa misma lógica (WebCrypto) y se comprobó con Python; el test Rust `interop_vector_from_admin_generator` lo usará en Actions.
  Una KEY firmada con `444RESTOOL|v1|` no valida con `444CFGTOOL|v1|` (comprobado).
- **Sintaxis** del script del generador (`node --check`).
- **TypeScript**: comprobado con declaraciones de tipos simplificadas de las librerías (no están instaladas aquí); sin errores en los campos y
  tipos propios. La comprobación completa (`npm run typecheck`) se hará en Actions.

## Escrito, SIN compilar ni probar (se ejecuta en Actions)
- Todo el Rust nuevo o cambiado: detección MK2/9mm, sustitución y skins manuales, Citizen con RPF, Sound Packs, progreso por bytes,
  explorador de archivos, comandos Tauri, licencias y HWID.
- Todas las pruebas (`cargo test -p engine`): ver `TEST_RESULTS.md`. 
- La interfaz completa (React) y los diálogos de Windows.

## Qué cambió en esta revisión
1. **MK2 / 9mm**: el arma se detecta por contenido (modelos `w_pi_pistolmk2` / `w_pi_pistol`, con el nombre del archivo como apoyo; se
   puede fijar a mano). Cada arma es un «hueco» independiente: una skin nueva sustituye a la anterior de la MISMA arma (aunque tenga otro
   nombre) y no toca otras armas. Skins instaladas a mano en `mods/` que son la misma arma se detectan, se piden confirmar, se guardan por hash y
   se devuelven al retirar. Si MK2 y 9mm comparten identificador de montaje ya no se bloquea: se exige una confirmación expresa
   (no está verificado que funcionen a la vez).
2. **Citizen con RPF**: se aceptan RPF dentro del ZIP o sueltos (van a `mods/`), se detecta la carpeta envoltorio de `citizen/`, se
   valida que el RPF sea un RPF7 real, y los archivos no compatibles (.bat, etc.) se omiten con aviso en vez de rechazar el paquete.
   Antes de instalar siempre se muestra la lista de archivos.
3. **Sound Packs**: los extras (.awc, .txt, .bat…) ya no rechazan el paquete; comprobación de que GTA V tiene `x64\audio\sfx`;
   comprobación previa de permisos de escritura con mensaje claro (Program Files → administrador o modo mods); progreso real por bytes;
   ZIP con compresión Deflate64 admitido; detección de GTA V en rutas Rockstar/Epic/Steam; comprobación de rutas en Ajustes.
4. **Explorador de archivos** en todas las secciones (Citizen, Mods, Armas, Blood FX, Tracers, Sound Packs).
5. **Licencias** Ed25519 + HWID con pantalla de activación; los comandos del backend exigen licencia.

## NO incluido / limitaciones
- **No hay descarga por enlace**: nunca existió en el proyecto; se importan archivos locales (ZIP/RPF). Pendiente tras la primera compilación.
- **.awc / .rel sueltos** no se instalan (deben ir dentro de un RPF; el explorador sí los muestra y permite añadirlos a `mods/`).
- **Fusión de RPF** (MK2 + 9mm en uno): no implementada.
- **Carpetas de solo lectura** en el explorador: `citizen/` y el audio de GTA V (se modifican solo con instalaciones con copia).
  Las carpetas solo se eliminan vacías.
- La detección de skins instaladas a mano solo mira RPF dentro de `mods/` (profundidad 3) y solo para Armas.
- **Licencia**: el HWID usa el `MachineGuid` de Windows; si cambia (reinstalación), hace falta una KEY nueva. La activación se guarda en
  `license.json` (la seguridad es la firma: copiarlo a otro equipo no sirve); un usuario con acceso total a su PC podría alterar el
  ejecutable, como en cualquier licencia local sin servidor.
- Sound Packs en `mods\x64\audio\sfx`: opción disponible pero sin verificar en juego.
- No se elude Pure Mode ni anticheats.
- Los **efectos visuales** (`Effects.tsx`, `Background.tsx`) NO se han tocado.
