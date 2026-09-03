# Research: adopting drifted central Skill content

Extracted for this task so implement/check context is not truncated from the large quality-guidelines files.

## Current inspect contract

`inspect_central_skill` compares the central tree hash to `skills.content_hash`. Mismatch or non-Ready stored status returns `SkillStatus::Invalid` and `CENTRAL_SKILL_CONTENT_CHANGED` (`src-tauri/src/skills/library.rs:470-476`).

List DTO uses that inspection. Global target status short-circuits to `external_owned_change` with the same diagnostic when any assigned skill is not Ready (`src-tauri/src/skills/service.rs:439-452`).

`preview_skill_content` and `validate_ready_records` refuse unless inspection is Ready. Spec matrix: central path/type/hash/status drift blocks content preview, sync, and delete (`.trellis/spec/backend/quality-guidelines.md` Skills scenario, Validation & Error Matrix).

Ordinary Skill DTOs expose description only. Full `SKILL.md` is only for the explicit content-preview RPC after the central copy hashes as Ready.

## Why target baselines do not need updating

Skill native projection is `{ targetType: "symlink", linkTarget: central_path }`. Directory `full_hash` hashes `{ target_type, link_target }` entries only, not the linked tree bytes (`src-tauri/src/sync/mod.rs:216-254`, `src-tauri/src/skills/service.rs:839-856`).

Editing files inside the central directory does not change applied symlinks. After the central record is Ready at the new hash, assigned targets return to in_sync without managed item/target baseline writes.

## Required adopt behavior

- New command reuses `VersionedSkillInput`; returns `SkillDto`.
- Read path may parse `SKILL.md` while hash is drifted; do not widen `preview_skill_content`.
- Keep `name` and `central_path`. Reject frontmatter rename.
- CAS-update `content_hash` and `frontmatter_json`; set `status='ready'`; let the existing skills row_version trigger bump.
- Refuse `applying` / `restoring` / `rollback_failed` writers.
- No native Preview/Apply, no source recopy, no MCP readopt.

## Frontend contract

Skills page owns central-library actions. Use generated `commands` only. Confirmation uses `useDialogFocus`, not `window.confirm`. Success invalidates the Skills query family and must not call `previewSkillSync` / `applySkillPreview`. List rendering still must not show Skill body.
