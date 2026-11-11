//! Licencias de 444CFG TOOL: firma Ed25519 sobre `444CFGTOOL|v1|<HWID>`.
//!
//! * La **clave privada** solo existe en el generador del administrador; nunca entra en la aplicación.
//! * La aplicación incorpora únicamente la **clave pública** (64 caracteres hexadecimales).
//! * KEY = firma de 64 bytes en BASE32 (RFC 4648, sin relleno) en bloques de 8 separados por «-».
//! * HWID = 32 caracteres hexadecimales (mayúsculas) derivados del identificador de la máquina.
//!
//! El identificador `444CFGTOOL|v1|` es propio de esta aplicación: una KEY de otra herramienta no valida aquí.
use crate::store::sha256_bytes;
use ed25519_dalek::{Signature, VerifyingKey};
use std::fmt;

pub const LICENSE_ID: &str = "444CFGTOOL|v1|";
const HWID_SALT: &str = "444CFGTOOL|hwid|v1|";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseError {
    /// La aplicación se compiló sin una clave pública válida.
    PublicKeyMissing,
    /// El HWID no tiene 32 caracteres hexadecimales.
    BadHwid,
    /// La KEY no tiene el formato esperado.
    BadFormat(String),
    /// La firma no corresponde a este equipo (o a esta herramienta).
    Invalid,
    /// La firma es válida pero la licencia ha caducado.
    Expired,
}

impl fmt::Display for LicenseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LicenseError::PublicKeyMissing => write!(
                f,
                "Esta versión de 444CFG TOOL no tiene la clave pública de licencias configurada. Contacta con el administrador."
            ),
            LicenseError::BadHwid => write!(f, "No se pudo calcular un HWID válido en este equipo."),
            LicenseError::BadFormat(m) => write!(f, "La KEY no tiene un formato válido: {}", m),
            LicenseError::Expired => write!(f, "La licencia ha caducado; solicita una nueva KEY."),
            LicenseError::Invalid => write!(
                f,
                "KEY incorrecta: no corresponde a este equipo (HWID) o no es una licencia de 444CFG TOOL."
            ),
        }
    }
}

impl std::error::Error for LicenseError {}

/// HWID de 32 caracteres hexadecimales en mayúsculas a partir del identificador de la máquina.
pub fn hwid_from_machine_id(machine_id: &str) -> String {
    let h = sha256_bytes(format!("{}{}", HWID_SALT, machine_id.trim().to_lowercase()).as_bytes());
    h[..32].to_uppercase()
}

pub fn normalize_hwid(s: &str) -> Result<String, LicenseError> {
    let t = s.trim().to_uppercase();
    if t.len() == 32 && t.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)) {
        Ok(t)
    } else {
        Err(LicenseError::BadHwid)
    }
}

pub fn decode_base32(s: &str) -> Result<Vec<u8>, LicenseError> {
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    let mut out: Vec<u8> = Vec::with_capacity(s.len() * 5 / 8 + 1);
    for c in s.chars() {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            '2'..='7' => c as u32 - '2' as u32 + 26,
            _ => return Err(LicenseError::BadFormat(format!("carácter no válido «{}»", c))),
        };
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
            buf &= (1u32 << bits) - 1;
        }
    }
    Ok(out)
}

pub fn encode_base32(b: &[u8]) -> String {
    const ABC: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = String::new();
    for &n in b {
        buf = (buf << 8) | n as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ABC[((buf >> bits) & 31) as usize] as char);
        }
        buf &= (1u32 << bits) - 1;
    }
    if bits > 0 {
        out.push(ABC[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Formatea una firma como KEY (bloques de 8 separados por guiones). Lo usan las pruebas.
pub fn format_key(sig: &[u8]) -> String {
    let s = encode_base32(sig);
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && i % 8 == 0 {
            out.push('-');
        }
        out.push(c);
    }
    out
}

pub fn parse_public_key(hex: &str) -> Result<VerifyingKey, LicenseError> {
    let t = hex.trim();
    if t.len() != 64 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(LicenseError::PublicKeyMissing);
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&t[i * 2..i * 2 + 2], 16).map_err(|_| LicenseError::PublicKeyMissing)?;
    }
    VerifyingKey::from_bytes(&bytes).map_err(|_| LicenseError::PublicKeyMissing)
}

/// Decodifica una firma Ed25519 en BASE64URL sin relleno (formato de tokens con duración).
fn decode_base64url_signature(s: &str) -> Result<[u8; 64], LicenseError> {
    let clean = s.trim().trim_end_matches('=');
    if clean.len() != 86 || !clean.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
        return Err(LicenseError::BadFormat("firma BASE64URL inválida".into()));
    }
    let mut out = Vec::with_capacity(64);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for b in clean.bytes() {
        let v = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'-' => 62,
            b'_' => 63,
            _ => unreachable!(),
        };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
            buf &= (1u32 << bits) - 1;
        }
    }
    if out.len() != 64 || bits != 4 || buf != 0 {
        return Err(LicenseError::BadFormat("firma BASE64URL incompleta o no canónica".into()));
    }
    let mut arr = [0u8; 64];
    arr.copy_from_slice(&out);
    Ok(arr)
}

/// Comprueba una KEY del generador antiguo de duración (dos líneas).
/// Se verifica la firma sobre el payload COMPLETO: HWID, caducidad y plan no se pueden modificar.
fn verify_token_with_duration(vk: &VerifyingKey, hwid: &str, token: &str) -> Result<(), LicenseError> {
    let mut lines = token.trim().lines().map(str::trim);
    let payload = lines.next().unwrap_or("");
    let signature_b64url = lines.next().ok_or_else(|| LicenseError::BadFormat("el token necesita dos líneas".into()))?;
    if lines.next().is_some() {
        return Err(LicenseError::BadFormat("el token debe tener exactamente dos líneas".into()));
    }
    let parts: Vec<&str> = payload.split('|').collect();
    if parts.len() != 5 || parts[0] != "444CFGTOOL" || parts[1] != "v1" {
        return Err(LicenseError::BadFormat("identificador o estructura de token incorrectos".into()));
    }
    let signed_hwid = normalize_hwid(parts[2])?;
    if signed_hwid != hwid || parts[2] != hwid {
        return Err(LicenseError::Invalid);
    }
    let expiry: u64 = parts[3].parse().map_err(|_| LicenseError::BadFormat("caducidad incorrecta".into()))?;
    if !matches!(parts[4], "1d" | "7d" | "30d" | "90d" | "permanent") {
        return Err(LicenseError::BadFormat("plan de licencia desconocido".into()));
    }
    if (parts[4] == "permanent") != (expiry == 0) {
        return Err(LicenseError::BadFormat("plan y caducidad no coinciden".into()));
    }
    let sig = Signature::from_bytes(&decode_base64url_signature(signature_b64url)?);
    vk.verify_strict(payload.as_bytes(), &sig).map_err(|_| LicenseError::Invalid)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| LicenseError::BadFormat("reloj del sistema incorrecto".into()))?
        .as_secs();
    if expiry != 0 && now >= expiry {
        return Err(LicenseError::Expired);
    }
    Ok(())
}

/// Verifica dos formatos válidos del administrador 444CFG TOOL:
/// - firma BASE32 separada por guiones sobre `444CFGTOOL|v1|HWID`
/// - token de dos líneas `444CFGTOOL|v1|HWID|expiry|plan` + firma BASE64URL
pub fn verify_key(public_hex: &str, hwid: &str, key: &str) -> Result<(), LicenseError> {
    let vk = parse_public_key(public_hex)?;
    let hwid = normalize_hwid(hwid)?;
    if key.trim().starts_with("444CFGTOOL|") {
        return verify_token_with_duration(&vk, &hwid, key);
    }
    let compact: String = key.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>().to_uppercase();
    if compact.is_empty() {
        return Err(LicenseError::BadFormat("está vacía".into()));
    }
    let sig_bytes = decode_base32(&compact)?;
    if sig_bytes.len() != 64 {
        return Err(LicenseError::BadFormat(format!(
            "debe contener 64 bytes de firma (tiene {}); revisa que esté completa",
            sig_bytes.len()
        )));
    }
    let mut arr = [0u8; 64];
    arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&arr);
    let msg = format!("{}{}", LICENSE_ID, hwid);
    vk.verify_strict(msg.as_bytes(), &sig).map_err(|_| LicenseError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    // Pareja de claves SOLO para pruebas (semilla fija pública, sin ningún valor real).
    fn test_pair() -> (SigningKey, String) {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk = sk.verifying_key().to_bytes();
        let hex: String = pk.iter().map(|b| format!("{:02x}", b)).collect();
        (sk, hex)
    }

    fn base64url_test(bytes: &[u8]) -> String {
        const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut result = String::new();
        for chunk in bytes.chunks(3) {
            let a = chunk[0];
            let b = *chunk.get(1).unwrap_or(&0);
            let c = *chunk.get(2).unwrap_or(&0);
            result.push(ABC[(a >> 2) as usize] as char);
            result.push(ABC[(((a & 3) << 4) | (b >> 4)) as usize] as char);
            if chunk.len() >= 2 { result.push(ABC[(((b & 15) << 2) | (c >> 6)) as usize] as char); }
            if chunk.len() == 3 { result.push(ABC[(c & 63) as usize] as char); }
        }
        result
    }

    #[test]
    fn timed_and_permanent_tokens_are_checked() {
        let (sk, public) = test_pair();
        let hwid = "0123456789ABCDEF0123456789ABCDEF";
        let make = |expiry: u64, plan: &str| {
            let payload = format!("444CFGTOOL|v1|{}|{}|{}", hwid, expiry, plan);
            format!("{}\n{}", payload, base64url_test(&sk.sign(payload.as_bytes()).to_bytes()))
        };
        assert!(verify_key(&public, hwid, &make(0, "permanent")).is_ok());
        assert!(verify_key(&public, hwid, &make(u64::MAX, "30d")).is_ok());
        assert_eq!(verify_key(&public, hwid, &make(1, "1d")), Err(LicenseError::Expired));
        assert_eq!(verify_key(&public, "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", &make(0, "permanent")), Err(LicenseError::Invalid));
        assert_eq!(verify_key(&public, hwid, &make(0, "permanent").replace("|0|", "|1|")), Err(LicenseError::BadFormat("plan y caducidad no coinciden".into())));
        assert_eq!(verify_key(&public, hwid, &make(0, "permanent").replace("permanent", "30d")), Err(LicenseError::BadFormat("plan y caducidad no coinciden".into())));
        assert!(matches!(verify_key(&public, hwid, &make(1, "1d").replace("|1|", "|2|")), Err(LicenseError::Invalid)));
    }

    #[test]
    fn base32_roundtrip() {
        let data: Vec<u8> = (0..64u8).collect();
        assert_eq!(decode_base32(&encode_base32(&data)).unwrap(), data);
    }

    #[test]
    fn valid_key_verifies_and_other_hwid_fails() {
        let (sk, pub_hex) = test_pair();
        let hwid = "0123456789ABCDEF0123456789ABCDEF";
        let sig = sk.sign(format!("{}{}", LICENSE_ID, hwid).as_bytes());
        let key = format_key(&sig.to_bytes());
        assert_eq!(key.split('-').next().unwrap().len(), 8);
        assert!(verify_key(&pub_hex, hwid, &key).is_ok());
        assert!(verify_key(&pub_hex, &hwid.to_lowercase(), &key.to_lowercase()).is_ok(), "mayúsculas/minúsculas indiferentes");
        assert!(verify_key(&pub_hex, hwid, &key.replace('-', " ")).is_ok(), "separadores tolerados");
        assert_eq!(verify_key(&pub_hex, "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", &key), Err(LicenseError::Invalid));
    }

    #[test]
    fn other_tool_identifier_and_tampering_are_rejected() {
        let (sk, pub_hex) = test_pair();
        let hwid = "0123456789ABCDEF0123456789ABCDEF";
        let old = sk.sign(format!("444RESTOOL|v1|{}", hwid).as_bytes());
        assert_eq!(verify_key(&pub_hex, hwid, &format_key(&old.to_bytes())), Err(LicenseError::Invalid));
        let good = format_key(&sk.sign(format!("{}{}", LICENSE_ID, hwid).as_bytes()).to_bytes());
        let mut bad: Vec<char> = good.chars().collect();
        bad[3] = if bad[3] == 'A' { 'B' } else { 'A' };
        let bad: String = bad.into_iter().collect();
        assert_eq!(verify_key(&pub_hex, hwid, &bad), Err(LicenseError::Invalid));
        assert!(matches!(verify_key(&pub_hex, hwid, "ABC-DEF"), Err(LicenseError::BadFormat(_))));
        assert!(matches!(verify_key(&pub_hex, hwid, "!!!!"), Err(LicenseError::BadFormat(_))));
        assert!(matches!(verify_key(&pub_hex, hwid, ""), Err(LicenseError::BadFormat(_))));
    }

    #[test]
    fn placeholder_public_key_and_bad_hwid() {
        assert_eq!(verify_key("REEMPLAZAR_CON_TU_CLAVE_PUBLICA", "0123456789ABCDEF0123456789ABCDEF", "AAAA"), Err(LicenseError::PublicKeyMissing));
        let (_, pub_hex) = test_pair();
        assert_eq!(verify_key(&pub_hex, "XYZ", "AAAA"), Err(LicenseError::BadHwid));
    }

    #[test]
    fn hwid_is_stable_32_hex() {
        let h = hwid_from_machine_id("ABC-123");
        assert_eq!(h.len(), 32);
        assert_eq!(h, hwid_from_machine_id(" abc-123 "));
        assert!(normalize_hwid(&h).is_ok());
    }

    /// Vector generado con el generador de administrador (WebCrypto, Node) y verificado con Python `cryptography`.
    #[test]
    fn interop_vector_from_admin_generator() {
        const PUB: &str = include_str!("../tests/vector_public.txt");
        const HWID: &str = include_str!("../tests/vector_hwid.txt");
        const KEY: &str = include_str!("../tests/vector_key.txt");
        assert!(verify_key(PUB.trim(), HWID.trim(), KEY.trim()).is_ok());
    }
}
