# Release Workflow Guidelines

## Scenario: Publish the macOS ARM64 DMG

### 1. Scope / Trigger

Use this contract when creating or changing the GitHub Actions workflow that publishes the Tauri macOS installer. Publishing is an explicit maintainer action through `workflow_dispatch`; pushes and pull requests must not create releases.

### 2. Signatures

- Workflow input: `version: string`, required, plain `x.y.z` without a `v` prefix.
- Workflow input: `notes: string`, optional; Markdown release notes prepended verbatim to the published body.
- Git tag: `v<version>`.
- Rust target: `aarch64-apple-darwin`.
- Release asset: `EasyToAgents_<version>_aarch64.dmg`.
- Validation command: `node scripts/validate-release-version.mjs <version>`.

### 3. Contracts

- The selected workflow ref determines `GITHUB_SHA`; a new tag must point to that exact commit.
- `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and the application package in `src-tauri/Cargo.lock` must all match the workflow input.
- Tauri bundle targets must contain both `app` and `dmg`; `bundle.macOS.minimumSystemVersion` must be `13.0`.
- The packaged executable must contain only the `arm64` architecture.
- The build job has `contents: read`; only the publish job has `contents: write` through `GITHUB_TOKEN`.
- The public package is unsigned and unnotarized until the product scope explicitly changes. No signing secrets are required.
- Pin third-party Actions to full commit SHAs and keep the resolved release name in a comment.
- Every published Release must carry version notes describing what changed since the previous version tag. Derive them from `git log <previous-tag>..<release-commit>`: keep user-facing changes, group them (features / UI / fixes / performance), and drop `chore`, journal, and task-archive commits. The workflow prepends the `notes` input verbatim, then appends a fixed installation section; when `notes` is empty it publishes the generic template without a changelog, which is acceptable only for reruns of an already-annotated version.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Input is not plain `x.y.z` | Fail before dependency installation |
| Any application version differs | Fail and list the mismatched file and value |
| Existing tag points to another commit | Fail without moving the tag |
| No DMG or more than one DMG is produced | Fail before creating a public Release |
| DMG version, minimum macOS version, or architecture differs | Fail before upload |
| Existing tag points to the same commit | Allow a retry and replace only the same-named asset |
| Build or upload fails | Keep the Release absent or in draft state; never report a public success |

### 5. Good / Base / Bad Cases

- Good: `0.2.0` matches all four files, the tag is absent, one ARM64 DMG passes inspection, and `notes` carries the changelog derived from `git log v0.1.0..HEAD`; create the tag, upload to a draft Release, then publish it.
- Base: `v0.2.0` already points to the current commit; rerun the workflow and replace `EasyToAgents_0.2.0_aarch64.dmg`.
- Base: `notes` is empty on a rerun; the installation-only template replaces the body, acceptable only because the notes already exist or the version is being republished.
- Bad: `v0.2.0` points to a different commit; stop and publish a corrected version instead of moving the tag.
- Bad: a first publish of a new version ships with template-only notes and no changelog.

### 6. Tests Required

- Unit-test successful four-file version validation.
- Unit-test each file mismatch independently.
- Unit-test rejection of a `v` prefix, missing DMG target, and a minimum macOS version other than `13.0`.
- Run `actionlint` for workflow syntax and expression validation.
- Before the first public release, build and mount a real DMG and assert `CFBundleShortVersionString`, `LSMinimumSystemVersion`, and `lipo -archs`.

### 7. Wrong vs Correct

Wrong: create or move the version tag before the installer is validated, publish the Release before the asset upload succeeds, or grant repository write permission to the build job.

Correct: build with read-only permissions, inspect the DMG, create or reuse only an exact-commit tag, upload to a draft Release, and publish after the upload succeeds.
