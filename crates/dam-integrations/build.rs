use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let profiles_dir = manifest_dir.join("profiles");
    println!("cargo:rerun-if-changed={}", profiles_dir.display());

    let mut paths = fs::read_dir(&profiles_dir)
        .unwrap_or_else(|error| {
            panic!(
                "failed to read integration profiles directory {}: {error}",
                profiles_dir.display()
            )
        })
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .unwrap_or_else(|error| panic!("failed to read integration profile entry: {error}"))
        })
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();

    let mut generated = String::from("const BUNDLED_PROFILE_JSONS: &[&str] = &[\n");
    for path in &paths {
        println!("cargo:rerun-if-changed={}", path.display());
        let literal = format!("{:?}", path.display().to_string());
        generated.push_str(&format!("    include_str!({literal}),\n"));
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("bundled_profiles.rs"), generated)
        .expect("failed to write bundled integration profile list");
}
