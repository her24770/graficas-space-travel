//! Modelos incrustados en el binario (nativo y web los usan igual).
//! La lista se genera en `build.rs` a partir de `assets/models/*.obj`.
include!(concat!(env!("OUT_DIR"), "/models.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_modelos_son_validos() {
        assert!(!MODELS.is_empty(), "no hay .obj en assets/models/");
        for (nombre, bytes) in MODELS {
            if let Err(e) = crate::obj::parse(bytes) {
                panic!("assets/models/{nombre}.obj es inválido: {e}");
            }
        }
    }
}
