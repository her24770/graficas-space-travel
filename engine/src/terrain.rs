//! Altura del terreno de cada planeta. La usan la geometría (relieve), las colisiones y los
//! objetos de superficie, y coincide con los colores del shader porque sale del mismo ruido.
use crate::shaders::{ice_height, rocky_height, Material, SEA_LEVEL};
use crate::Vec3;

/// Elevación en [0, 1] en la dirección `dir` (espacio local del planeta). 0 = nivel base (el mar).
/// Los gigantes gaseosos y el sol no tienen superficie sólida: siempre 0.
pub fn elevation(material: &Material, dir: Vec3) -> f32 {
    let d = dir.normalize();
    match *material {
        Material::Rocky { seed, .. } => ((rocky_height(d, seed) - SEA_LEVEL) / (1.0 - SEA_LEVEL)).max(0.0),
        Material::Ice { seed, .. } => ice_height(d, seed),
        _ => 0.0,
    }
}

/// ¿Es mar en esa dirección? Solo los planetas rocosos tienen mar.
pub fn is_sea(material: &Material, dir: Vec3) -> bool {
    match *material {
        Material::Rocky { seed, .. } => rocky_height(dir.normalize(), seed) < SEA_LEVEL,
        _ => false,
    }
}

/// ¿Es zona alta (montaña)? El umbral depende del tipo de planeta.
pub fn is_high(material: &Material, dir: Vec3) -> bool {
    let umbral = match material {
        Material::Ice { .. } => 0.55,
        _ => 0.3,
    };
    elevation(material, dir) > umbral
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROCOSO: Material = Material::Rocky { sea: 0, land: 0, peak: 0, seed: 4.0 };

    fn direcciones() -> impl Iterator<Item = Vec3> {
        (0..400).map(|i| {
            let (a, b) = (i as f32 * 0.37, i as f32 * 0.91);
            Vec3::new(a.cos() * b.sin(), b.cos(), a.sin() * b.sin())
        })
    }

    #[test]
    fn la_elevacion_queda_en_rango_y_es_determinista() {
        for d in direcciones() {
            let e = elevation(&ROCOSO, d);
            assert!((0.0..=1.0).contains(&e));
            assert_eq!(e, elevation(&ROCOSO, d));
        }
    }

    #[test]
    fn el_mar_es_plano_y_hay_mar_y_tierra() {
        let (mar, tierra): (Vec<Vec3>, Vec<Vec3>) = direcciones().partition(|d| is_sea(&ROCOSO, *d));
        assert!(!mar.is_empty() && !tierra.is_empty(), "un planeta rocoso debe tener mar y tierra");
        assert!(mar.iter().all(|d| elevation(&ROCOSO, *d) == 0.0));
    }

    #[test]
    fn los_gaseosos_no_tienen_relieve_ni_mar() {
        let gas = Material::Gas { a: 0, b: 0, seed: 1.0 };
        assert!(direcciones().all(|d| elevation(&gas, d) == 0.0 && !is_sea(&gas, d)));
    }

    #[test]
    fn la_direccion_no_necesita_ser_unitaria() {
        let d = Vec3::new(0.3, 0.8, -0.2);
        assert!((elevation(&ROCOSO, d) - elevation(&ROCOSO, d * 7.0)).abs() < 1e-5);
    }
}
