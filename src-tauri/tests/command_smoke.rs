use easytoagents_lib::{commands::AppInfoDto, create_command_builder};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{assert_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    Manager, WebviewWindowBuilder,
};

#[test]
fn app_info_command_is_available_through_tauri_ipc() {
    let command_builder = create_command_builder::<MockRuntime>();
    let app = mock_builder()
        .invoke_handler(command_builder.invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("创建 Tauri 测试应用失败");
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("创建 Tauri 测试窗口失败");

    assert_ipc_response(
        &webview,
        InvokeRequest {
            cmd: "get_app_info".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("解析测试地址失败"),
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        },
        Ok::<AppInfoDto, AppInfoDto>(AppInfoDto {
            name: "EasyToAgents".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }),
    );
}

/// Agents 冒烟：真实 AppState（隔离数据根）下 list_agents 可经 Tauri IPC
/// 调用并返回空的中央列表。
#[test]
fn agents_list_command_is_available_through_tauri_ipc() {
    use easytoagents_lib::agents::AgentDto;
    use easytoagents_lib::app::{AppPaths, AppState};

    let temporary = tempfile::tempdir().expect("创建隔离根失败");
    let root = std::fs::canonicalize(temporary.path()).expect("规范化隔离根失败");
    let paths = AppPaths::from_data_root(root.join("app-data")).expect("初始化数据根失败");

    let command_builder = create_command_builder::<MockRuntime>();
    let app = mock_builder()
        .invoke_handler(command_builder.invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("创建 Tauri 测试应用失败");
    app.manage(AppState::initialize(paths).expect("初始化 AppState 失败"));
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("创建 Tauri 测试窗口失败");

    assert_ipc_response(
        &webview,
        InvokeRequest {
            cmd: "list_agents".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("解析测试地址失败"),
            body: InvokeBody::default(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        },
        Ok::<Vec<AgentDto>, Vec<AgentDto>>(Vec::new()),
    );
}
