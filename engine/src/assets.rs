//! Modelos incrustados en el binario (nativo y web los usan igual).
//! Las listas se generan en `build.rs`: `MODELS` desde `assets/models/*.obj` (naves) y
//! `PROPS` desde `assets/props/*.obj` (objetos de superficie).
include!(concat!(env!("OUT_DIR"), "/models.rs"));

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
