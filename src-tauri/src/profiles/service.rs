//! Provider/Prompt 中央意图、首次接管与 Preview/Apply 编排。

include!("service_prelude.rs");
include!("prompt.rs");
include!("service_orchestration.rs");
include!("sync.rs");
include!("provider_discovery.rs");
include!("provider.rs");
include!("helpers.rs");

#[cfg(test)]
include!("tests.rs");
