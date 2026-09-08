import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, test } from "vitest";

import { validateReleaseVersion } from "./validate-release-version.mjs";

const fixtureRoots = new Set();

afterEach(async () => {
  await Promise.all(
    [...fixtureRoots].map((root) => rm(root, { recursive: true, force: true })),
  );
  fixtureRoots.clear();
});

async function createFixture(overrides = {}) {
  const root = await mkdtemp(join(tmpdir(), "easytoagents-release-"));
  fixtureRoots.add(root);
  await mkdir(join(root, "src-tauri"), { recursive: true });

  const files = {
    "package.json": JSON.stringify({
      name: "easytoagents-desktop",
      version: "0.1.0",
    }),
    "src-tauri/tauri.conf.json": JSON.stringify({
      version: "0.1.0",
      bundle: {
        targets: ["app", "dmg"],
        macOS: { minimumSystemVersion: "13.0" },
      },
    }),
    "src-tauri/Cargo.toml":
      '[package]\nname = "easytoagents"\nversion = "0.1.0"\n\n[dependencies]\n',
    "src-tauri/Cargo.lock":
      'version = 4\n\n[[package]]\nname = "easytoagents"\nversion = "0.1.0"\n',
    ...overrides,
  };

  await Promise.all(
    Object.entries(files).map(([fileName, contents]) =>
      writeFile(join(root, fileName), contents, "utf8"),
    ),
  );
  return root;
}

test("四处版本一致时通过校验", async () => {
  const root = await createFixture();
  const versions = await validateReleaseVersion("0.1.0", root);

  assert.deepEqual(versions, {
    "package.json": "0.1.0",
    "src-tauri/tauri.conf.json": "0.1.0",
    "src-tauri/Cargo.toml": "0.1.0",
    "src-tauri/Cargo.lock": "0.1.0",
  });
});

const versionMismatchCases = [
  ["package.json", JSON.stringify({ version: "0.2.0" })],
  [
    "src-tauri/tauri.conf.json",
    JSON.stringify({
      version: "0.2.0",
      bundle: {
        targets: ["app", "dmg"],
        macOS: { minimumSystemVersion: "13.0" },
      },
    }),
  ],
  [
    "src-tauri/Cargo.toml",
    '[package]\nname = "easytoagents"\nversion = "0.2.0"\n',
  ],
  [
    "src-tauri/Cargo.lock",
    'version = 4\n\n[[package]]\nname = "easytoagents"\nversion = "0.2.0"\n',
  ],
];

for (const [fileName, contents] of versionMismatchCases) {
  test(`${fileName} 版本不一致时失败`, async () => {
    const root = await createFixture({ [fileName]: contents });

    await assert.rejects(
      validateReleaseVersion("0.1.0", root),
      new RegExp(`${fileName.replaceAll(".", "\\.")}.*0\\.2\\.0`),
    );
  });
}

test("拒绝带 v 前缀的版本输入", async () => {
  const root = await createFixture();
  await assert.rejects(validateReleaseVersion("v0.1.0", root), /不带 v 前缀/);
});

test("拒绝缺少 DMG 的打包配置", async () => {
  const root = await createFixture({
    "src-tauri/tauri.conf.json": JSON.stringify({
      version: "0.1.0",
      bundle: {
        targets: ["app"],
        macOS: { minimumSystemVersion: "13.0" },
      },
    }),
  });

  await assert.rejects(
    validateReleaseVersion("0.1.0", root),
    /同时启用.*app.*dmg/,
  );
});

test("拒绝错误的 macOS 最低系统版本", async () => {
  const root = await createFixture({
    "src-tauri/tauri.conf.json": JSON.stringify({
      version: "0.1.0",
      bundle: {
        targets: ["app", "dmg"],
        macOS: { minimumSystemVersion: "14.0" },
      },
    }),
  });

  await assert.rejects(validateReleaseVersion("0.1.0", root), /必须为 13\.0/);
});
