use std::fs;

#[test]
fn generated_bindings_are_current() {
    let temporary_directory = tempfile::tempdir().expect("创建临时目录失败");
    let generated_path = temporary_directory.path().join("commands.ts");

    easytoagents_lib::export_typescript_bindings(&generated_path);

    let generated = fs::read_to_string(&generated_path).expect("读取临时绑定失败");
    let committed = include_str!("../../src/bindings/commands.ts");

    assert_eq!(
        generated, committed,
        "Rust 命令合同已变化，请运行 pnpm bindings:generate"
    );
}
