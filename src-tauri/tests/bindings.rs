use std::{fs, path::Path};

#[test]
fn generated_bindings_are_current() {
    let temporary_directory = tempfile::tempdir().expect("创建临时目录失败");
    let generated_path = temporary_directory.path().join("commands.ts");

    easytoagents_lib::export_typescript_bindings(&generated_path);

    let generated = fs::read_to_string(&generated_path).expect("读取临时绑定失败");
    let committed_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings/commands.ts");
    let committed = fs::read_to_string(committed_path).expect("读取已提交绑定失败");
    assert_eq!(
        canonicalize_bindings(&generated),
        canonicalize_bindings(&committed),
        "Rust 命令合同已变化，请运行 pnpm bindings:generate"
    );
}

/// tauri-specta 2.0.0-rc.21 stores constants in a `HashMap`, so the two
/// exported constants can be emitted in either order between processes. Keep
/// the binding freshness check strict while ignoring that nondeterministic
/// ordering.
fn canonicalize_bindings(source: &str) -> String {
    const CONSTANTS_MARKER: &str = "/** user-defined constants **/\n";
    const TYPES_MARKER: &str = "/** user-defined types **/";

    let Some(constants_start) = source.find(CONSTANTS_MARKER) else {
        return source.to_owned();
    };
    let constants_content_start = constants_start + CONSTANTS_MARKER.len();
    let Some(types_offset) = source[constants_content_start..].find(TYPES_MARKER) else {
        return source.to_owned();
    };
    let constants_content_end = constants_content_start + types_offset;
    let constants_block = &source[constants_content_start..constants_content_end];
    let Some(first_constant) = constants_block.find("export const ") else {
        return source.to_owned();
    };
    let Some(last_constant_end) = constants_block.rfind(";\n").map(|index| index + 2) else {
        return source.to_owned();
    };

    let mut constants = constants_block[first_constant..last_constant_end]
        .lines()
        .collect::<Vec<_>>();
    constants.sort_unstable();

    let mut normalized = String::with_capacity(source.len());
    normalized.push_str(&source[..constants_content_start]);
    normalized.push_str(&constants_block[..first_constant]);
    normalized.push_str(&constants.join("\n"));
    normalized.push_str(&constants_block[last_constant_end..]);
    normalized.push_str(&source[constants_content_end..]);
    normalized
}
