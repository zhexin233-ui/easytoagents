use std::path::PathBuf;

fn main() {
    let output_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings/commands.ts")
        });

    easytoagents_lib::export_typescript_bindings(&output_path);
    println!("已生成 {}", output_path.display());
}
