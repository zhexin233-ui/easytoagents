#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const RELEASE_VERSION_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function readTomlPackage(source, fileName) {
  const packageHeader = source.match(/^\[package\]\s*$/m);
  if (!packageHeader) {
    throw new Error(`${fileName} 缺少 [package] 配置`);
  }
  const packageBodyStart = packageHeader.index + packageHeader[0].length;
  const remainingSource = source.slice(packageBodyStart);
  const nextSectionOffset = remainingSource.search(/^\[/m);
  const packageSection =
    nextSectionOffset === -1
      ? remainingSource
      : remainingSource.slice(0, nextSectionOffset);

  const name = packageSection.match(/^name\s*=\s*"([^"]+)"\s*$/m)?.[1];
  const version = packageSection.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!name || !version) {
    throw new Error(`${fileName} 缺少 package name 或 version`);
  }

  return { name, version };
}

function readLockfilePackageVersion(source, packageName) {
  const packages = source.split(/^\[\[package\]\]\s*$/m).slice(1);
  for (const packageBlock of packages) {
    const name = packageBlock.match(/^name\s*=\s*"([^"]+)"\s*$/m)?.[1];
    if (name !== packageName) continue;

    const version = packageBlock.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
    if (!version) {
      throw new Error(`src-tauri/Cargo.lock 中 ${packageName} 缺少 version`);
    }
    return version;
  }

  throw new Error(`src-tauri/Cargo.lock 中找不到应用包 ${packageName}`);
}

export async function validateReleaseVersion(
  expectedVersion,
  repositoryRoot = process.cwd(),
) {
  if (!RELEASE_VERSION_PATTERN.test(expectedVersion)) {
    throw new Error(
      `发布版本必须使用不带 v 前缀的 x.y.z 格式，收到：${expectedVersion}`,
    );
  }

  const paths = {
    packageJson: resolve(repositoryRoot, "package.json"),
    tauriConfig: resolve(repositoryRoot, "src-tauri/tauri.conf.json"),
    cargoManifest: resolve(repositoryRoot, "src-tauri/Cargo.toml"),
    cargoLock: resolve(repositoryRoot, "src-tauri/Cargo.lock"),
  };
  const [
    packageJsonSource,
    tauriConfigSource,
    cargoManifestSource,
    cargoLockSource,
  ] = await Promise.all([
    readFile(paths.packageJson, "utf8"),
    readFile(paths.tauriConfig, "utf8"),
    readFile(paths.cargoManifest, "utf8"),
    readFile(paths.cargoLock, "utf8"),
  ]);

  const packageJson = JSON.parse(packageJsonSource);
  const tauriConfig = JSON.parse(tauriConfigSource);
  const cargoPackage = readTomlPackage(
    cargoManifestSource,
    "src-tauri/Cargo.toml",
  );
  const versions = new Map([
    ["package.json", packageJson.version],
    ["src-tauri/tauri.conf.json", tauriConfig.version],
    ["src-tauri/Cargo.toml", cargoPackage.version],
    [
      "src-tauri/Cargo.lock",
      readLockfilePackageVersion(cargoLockSource, cargoPackage.name),
    ],
  ]);

  const mismatches = [...versions].filter(
    ([, version]) => version !== expectedVersion,
  );
  if (mismatches.length > 0) {
    const details = mismatches
      .map(([fileName, version]) => `${fileName}=${String(version)}`)
      .join(", ");
    throw new Error(`发布版本 ${expectedVersion} 与应用版本不一致：${details}`);
  }

  const bundleTargets = tauriConfig.bundle?.targets;
  if (
    !Array.isArray(bundleTargets) ||
    !bundleTargets.includes("app") ||
    !bundleTargets.includes("dmg")
  ) {
    throw new Error(
      'src-tauri/tauri.conf.json 必须同时启用 "app" 和 "dmg" 打包目标',
    );
  }
  if (tauriConfig.bundle?.macOS?.minimumSystemVersion !== "13.0") {
    throw new Error(
      "src-tauri/tauri.conf.json 的 macOS 最低系统版本必须为 13.0",
    );
  }

  return Object.fromEntries(versions);
}

async function main() {
  const expectedVersion = process.argv[2];
  if (!expectedVersion) {
    throw new Error("用法：node scripts/validate-release-version.mjs <x.y.z>");
  }

  const versions = await validateReleaseVersion(expectedVersion);
  console.log(`版本校验通过：${expectedVersion}`);
  for (const [fileName, version] of Object.entries(versions)) {
    console.log(`- ${fileName}: ${version}`);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
