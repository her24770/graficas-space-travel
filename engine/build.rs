//! Descubre los .obj de `assets/models/` (naves) y `assets/props/` (objetos de superficie)
//! y los incrusta en el binario. Para agregar un modelo basta con soltar el archivo en la carpeta.
use std::{env, fs, path::Path};

fn lista(dir: &Path, constante: &str) -> String {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "obj"))
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    names.sort();

    let mut out = format!("pub const {constante}: &[(&str, &[u8])] = &[\n");
    for n in &names {
        let abs = dir.join(format!("{n}.obj"));
        out.push_str(&format!("    ({n:?}, include_bytes!({:?})),\n", abs.to_string_lossy()));
    }
    out.push_str("];\n");
    out
}

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let assets = Path::new(&manifest).join("../assets");
    let code = lista(&assets.join("models"), "MODELS") + &lista(&assets.join("props"), "PROPS");
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("models.rs"), code).unwrap();
}
