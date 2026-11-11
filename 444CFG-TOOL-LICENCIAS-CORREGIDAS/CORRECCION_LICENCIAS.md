# Corrección de activación 444CFG TOOL

Se conserva la clave pública del proyecto. El backend acepta ahora **dos formatos firmados Ed25519**:

1. Firma BASE32 en bloques (generador `444CFG_KEYS_ADMIN_PRIVADO(3).html`).
2. Token de 2 líneas: `444CFGTOOL|v1|HWID|caducidad|plan` y firma BASE64URL (generador anterior).

Los tokens con caducidad se comprueban al activarse y antes de cada operación protegida. Ninguna firma se omite ni se simula.

**Antes de compilar:** coloca los archivos de este ZIP en la raíz de tu repositorio (incluida `.github`). No añadas `private.key` ni el generador de administrador.

**Validación pendiente:** este entorno no tiene cargo/Rust, por lo que debes ejecutar las pruebas y la compilación en GitHub Actions antes de distribuir el ejecutable.

**Nota:** los tokens de dos líneas están ligados a su HWID y a la clave pública; un token firmado con una clave distinta será rechazado.
