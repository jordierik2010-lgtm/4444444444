# Resultados de pruebas

**Pruebas Rust ejecutadas: 0.** No había Rust en el entorno. Están escritas para ejecutarse en GitHub Actions (`cargo test -p engine`).
Hasta ver ese resultado en verde, **no debe considerarse que el motor funciona**.

## Ejecutado de verdad
| Qué | Resultado |
|---|---|
| Generador de KEYS (crear claves + firmar HWID) con DOM simulado en Node | OK |
| KEY del generador verificada con Python `cryptography` (Ed25519, mensaje `444CFGTOOL|v1|HWID`) | OK |
| Firma con otro identificador (`444RESTOOL|v1|`) rechazada por la verificación | OK |
| Sintaxis JS del generador | OK |
| TypeScript con tipos simplificados de librerías | sin errores propios |

## Escritas, sin ejecutar (`engine/tests/engine_tests.rs` y módulos)
| Test | Qué comprueba |
|---|---|
| `t01`–`t12`, `s01`–`s05` | las 21 anteriores (Citizen A→B, capas, Blood FX, Tracers, Sound Packs, fallo/recuperación, conflictos, ZIP malicioso, RPF anidado, seguridad) |
| `t04` (cambiado) | MK2 y 9mm separadas; montaje compartido exige confirmación; una skin MK2 nueva sustituye solo la MK2 |
| `n01` | skin MK2 manual: detectada, pide confirmación, copia por hash, devuelta al retirar; AP y 9mm intactas |
| `n02`, `n02b` | A→B conserva la copia de la skin manual; fijar arma a mano y rechazar valores no válidos |
| `n03`, `n03b`, `n04` | Citizen con RPF (envoltorio, RPF sueltos, RPF previo restaurado), RPF falso rechazado, extras omitidos con aviso |
| `n05`, `n06` | Sound Pack con extras, carpeta GTA inválida, comprobación posterior de lo copiado, progreso por bytes monótono (importación e instalación) |
| `f01`–`f05` | explorador: áreas por sección, solo lectura, estados gestionado/preexistente, eliminar con copia y restaurar, protección de lo gestionado, escape de ruta rechazado, importar/sustituir/deshacer, ejecutables bloqueados |
| `weapon::tests` | detección por contenido y por nombre |
| `license::tests` | firma válida/otro HWID/otro identificador/manipulada/formato/clave pública de relleno; interoperabilidad con el generador |

Sin pruebas automáticas: interfaz React, comandos Tauri, HWID de Windows, diálogos, instalador NSIS, permisos reales de Program Files.
