//! Modelos incrustados en el binario (nativo y web los usan igual).
//! Las listas se generan en `build.rs`: `MODELS` desde `assets/models/*.obj` (naves) y
//! `PROPS` desde `assets/props/*.obj` (objetos de superficie).
include!(concat!(env!("OUT_DIR"), "/models.rs"));

pub fn model_mtl(name: &str) -> Option<&'static str> {
    MODELS_MTL.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

/// Texto del .mtl de un objeto de `assets/props/`, si lo tiene.
pub fn prop_mtl(name: &str) -> Option<&'static str> {
    PROPS_MTL.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_modelos_son_validos() {
        assert!(!MODELS.is_empty(), "no hay .obj en assets/models/");
        for (carpeta, lista) in [("models", MODELS), ("props", PROPS)] {
            for (nombre, bytes) in lista {
                if let Err(e) = crate::obj::parse(bytes) {
                    panic!("assets/{carpeta}/{nombre}.obj es inválido: {e}");
                }
            }
        }
    }
}
