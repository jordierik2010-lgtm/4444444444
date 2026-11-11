//! Identificación del arma (MK2 / 9mm) a partir del contenido y de los nombres de un RPF.
//! El contenido (modelos `w_*.ydr`) manda sobre el nombre del archivo.
use serde::Serialize;

pub const MK2: &str = "mk2";
pub const NINE_MM: &str = "9mm";

#[derive(Clone, Debug, Default, Serialize)]
pub struct Detection {
    pub kind: Option<String>,
    pub evidence: Vec<String>,
}

pub fn label(kind: &str) -> &'static str {
    match kind {
        MK2 => "MK2",
        NINE_MM => "9mm",
        _ => "desconocida",
    }
}

/// Nombre de modelo normalizado (sin `.ydr`, `_hi` ni `_magN`) si el archivo es un modelo de arma.
pub fn model_name(base: &str) -> Option<String> {
    let l = base.trim().to_lowercase();
    if !l.starts_with("w_") || l.starts_with("w_at_") {
        return None;
    }
    let s = l.strip_suffix(".ydr")?;
    let s = s.strip_suffix("_hi").unwrap_or(s);
    let s = match s.find("_mag") {
        Some(p) => &s[..p],
        None => s,
    };
    Some(s.to_string())
}

/// Las líneas de `Package.contents` tienen la forma «   archivo.rpf: ruta/interna»; se queda con la ruta.
fn entry_path(line: &str) -> &str {
    match line.rfind(": ") {
        Some(p) => &line[p + 2..],
        None => line.trim(),
    }
}

fn base_of(p: &str) -> &str {
    p.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(p)
}

/// `file_names`: nombres de los RPF del paquete. `entries`: rutas internas (árbol de los RPF) o líneas de `contents`.
pub fn detect(file_names: &[String], entries: &[String]) -> Detection {
    let mut mk2_models: Vec<String> = Vec::new();
    let mut nine_models: Vec<String> = Vec::new();
    for e in entries {
        if let Some(m) = model_name(base_of(entry_path(e))) {
            if m.contains("pistolmk2") || m.contains("pistol_mk2") {
                if !mk2_models.contains(&m) {
                    mk2_models.push(m);
                }
            } else if m == "w_pi_pistol" && !nine_models.contains(&m) {
                nine_models.push(m);
            }
        }
    }
    let mut ev: Vec<String> = Vec::new();
    let content_kind: Option<&str> = match (!mk2_models.is_empty(), !nine_models.is_empty()) {
        (true, false) => {
            ev.push(format!("Contenido: modelo {} → MK2", mk2_models[0]));
            Some(MK2)
        }
        (false, true) => {
            ev.push(format!("Contenido: modelo {} → 9mm", nine_models[0]));
            Some(NINE_MM)
        }
        (true, true) => {
            ev.push("El contenido incluye modelos de MK2 y de 9mm a la vez: no se puede decidir automáticamente".into());
            return Detection { kind: None, evidence: ev };
        }
        (false, false) => None,
    };
    let mut name_kind: Option<&str> = None;
    let mut name_hit = String::new();
    for n in file_names {
        let l = n.to_lowercase();
        if l.contains("9mm") {
            name_kind = Some(NINE_MM);
            name_hit = n.clone();
            break;
        }
        if l.contains("mk2") && name_kind.is_none() {
            name_kind = Some(MK2);
            name_hit = n.clone();
        }
    }
    if let Some(nk) = name_kind {
        ev.push(format!("Nombre de archivo «{}» → {}", name_hit, label(nk)));
    }
    let kind = match (content_kind, name_kind) {
        (Some(c), Some(n)) if c != n => {
            ev.push(format!("El nombre sugiere {} pero el contenido indica {}: se usa el contenido", label(n), label(c)));
            Some(c)
        }
        (Some(c), _) => Some(c),
        (None, n) => n,
    };
    if kind.is_none() {
        ev.push("No se encontraron modelos ni nombres de MK2 o 9mm".into());
    }
    Detection { kind: kind.map(|k| k.to_string()), evidence: ev }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn content_beats_name() {
        let d = detect(&v(&["MK2_9mm.rpf"]), &v(&["content/x.rpf/w_pi_pistolmk2.ydr"]));
        assert_eq!(d.kind.as_deref(), Some(MK2));
    }

    #[test]
    fn nine_mm_by_model_and_by_name() {
        let d = detect(&v(&["x.rpf"]), &v(&["a/w_pi_pistol.ydr", "a/w_pi_pistol_mag1.ydr"]));
        assert_eq!(d.kind.as_deref(), Some(NINE_MM));
        let d = detect(&v(&["MK2_9mm.rpf"]), &v(&["a/otra.ytd"]));
        assert_eq!(d.kind.as_deref(), Some(NINE_MM));
        let d = detect(&v(&["MK2.rpf"]), &[]);
        assert_eq!(d.kind.as_deref(), Some(MK2));
    }

    #[test]
    fn contents_lines_and_unknown() {
        let d = detect(&v(&["a.rpf"]), &v(&["   a.rpf: content/s.rpf/w_pi_pistolmk2_hi.ydr"]));
        assert_eq!(d.kind.as_deref(), Some(MK2));
        let d = detect(&v(&["AP.rpf"]), &v(&["w_pi_appistol.ydr"]));
        assert!(d.kind.is_none());
        let d = detect(&v(&["x.rpf"]), &v(&["w_pi_pistolmk2.ydr", "w_pi_pistol.ydr"]));
        assert!(d.kind.is_none() && !d.evidence.is_empty());
    }
}
