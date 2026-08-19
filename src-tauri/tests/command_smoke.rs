use easytoagents_lib::{commands::AppInfoDto, create_command_builder};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{assert_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindowBuilder,
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
