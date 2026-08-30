# 错误提示展示 details.reason 具体原因

## Goal

后端 `AppError::conflict/invalid_input` 的 `message` 是按错误码分类的通用文案
（如「检测到配置冲突」），具体原因（如「该资源仍有项目分配，不能直接创建重复的全局分配」）
放在 `details.reason`。前端 `profileErrorText` 只渲染了通用 message，用户看到的
"CONFLICT：检测到配置冲突" 无法判断真实原因。让错误提示优先展示 `details.reason`。

## Requirements

- `profileErrorText`（全局唯一错误渲染出口，所有 feature 页面共用）：
  存在字符串型 `details.reason` 时展示 `code：reason`；否则回退 `code：message`。
- 保持既有行为：NOT_FOUND 的 resource 专用文案优先；非字符串 reason（如被脱敏的对象）
  回退 message；普通 Error 与未知值的兜底不变。
- 不改后端序列化合同（details 允许列表已含 reason，脱敏逻辑不动）。

## Acceptance Criteria

- [x] 真实生产形态（message=通用文案 + details.reason=具体原因）下，界面展示具体原因。
- [x] 新增 `src/lib/profile-api.test.ts` 覆盖 reason 优先、回退、非字符串回退、NOT_FOUND 专用文案、普通 Error 兜底。
- [x] `pnpm format:check` / `lint` / `typecheck` / `test`（139 项）全部通过；既有断言不受影响。
