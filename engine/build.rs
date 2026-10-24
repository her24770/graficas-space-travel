//! Descubre los .obj de `assets/models/` y los incrusta en el binario.
//! Para agregar un modelo basta con soltar el archivo en esa carpeta.
use std::{env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let dir = Path::new(&manifest).join("../assets/models");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut names: Vec<String> = fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "obj"))
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    names.sort();

    let mut out = String::from("pub const MODELS: &[(&str, &[u8])] = &[\n");
    for n in &names {
        let abs = dir.join(format!("{n}.obj"));
        out.push_str(&format!("    ({n:?}, include_bytes!({:?})),\n", abs.to_string_lossy()));
    }
    out.push_str("];\n");
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("models.rs"), out).unwrap();
}
