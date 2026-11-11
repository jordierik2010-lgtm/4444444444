# Compilar 444CFG TOOL con GitHub Actions

1. **Clave pública**: pega tu clave pública en `src-tauri/public.key` (ver `README.md`). Sin ella el workflow falla a propósito.
2. Crea un repositorio privado en https://github.com/new y sube **el contenido** de esta carpeta (no el ZIP), incluida la
   carpeta oculta `.github`. **No subas `private.key` ni el HTML del administrador** (el `.gitignore` los excluye y el workflow lo comprueba).
3. Abre **Actions** > **Build 444CFG TOOL (Windows)** > **Run workflow** (o espera a la ejecución tras el push a `main`).
4. Si todo está en verde, baja a **Artifacts** y descarga `444CFG-TOOL-Instalador-Windows` o `444CFG-TOOL-Portable-Windows`
   (GitHub los entrega en ZIP: extráelo).
5. Si un paso aparece en rojo, abre ese paso, copia el error completo y envíalo: el código **nunca se ha compilado**, así que un
   primer error de compilación o alguna prueba fallida es posible y se corrige rápido.

⚠️ No pruebes primero sobre tu instalación real de GTA/FiveM: usa una copia de `FiveM.app`.
