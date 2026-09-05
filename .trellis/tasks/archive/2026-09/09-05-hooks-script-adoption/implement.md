# 执行计划：Hook 脚本中央接管

- [x] B1 迁移 0015 + db/hooks record/insert 带 script 字段 + schema_version 15
- [x] B2 AppPaths::central_hooks + private_directories + 测试更新
- [x] B3 import.rs：分词器、接管判定、discover 候选字段、confirm 复制与命令重写、去重
- [x] B4 service.rs：create_hook 走脚本通道（先写文件后插库、失败回滚）、delete_hook 清理
- [x] C1 bindings:generate
- [x] D1 导入对话框 + Hooks 页面 DTO 适配
- [x] E1 单测（分词/接管规则/生命周期）+ 迁移升级测试 + e2e 扩展 + pnpm check
- [x] E2 spec 沉淀、提交、归档、journal
