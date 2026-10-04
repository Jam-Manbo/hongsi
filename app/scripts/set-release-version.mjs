import { appendFile, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { androidVersionCode, assetStem, releaseVersion, stableVersion } from './release-version.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const config = JSON.parse(await readFile(resolve(root, 'app/src-tauri/tauri.conf.json'), 'utf8'));
const prerelease = process.env.RELEASE_PRERELEASE === 'true';
let tag = process.argv[2] || `v${config.version}`;
if (process.env.GITHUB_EVENT_NAME !== 'release' && !prerelease && /^\d/.test(tag)) tag = `v${tag}`;
const label = releaseVersion(tag, prerelease);
const version = prerelease ? stableVersion(`v${config.version}`) : label;
const values = {
  HONGSI_RELEASE_TAG: tag,
  HONGSI_RELEASE_LABEL: label,
  HONGSI_RELEASE_ASSET_STEM: assetStem(tag, prerelease),
  HONGSI_ANDROID_VERSION_NAME: label,
};
if (process.env.GITHUB_RUN_NUMBER) values.HONGSI_ANDROID_VERSION_CODE = String(androidVersionCode(process.env.GITHUB_RUN_NUMBER));
if (process.env.GITHUB_ENV) {
  await appendFile(process.env.GITHUB_ENV, Object.entries(values).map(([key, value]) => `${key}=${value}\n`).join(''));
}
for (const name of ['app/package.json', 'app/src-tauri/tauri.conf.json']) {
  const path = resolve(root, name), json = JSON.parse(await readFile(path, 'utf8'));
  json.version = version;
  await writeFile(path, JSON.stringify(json, null, 2) + '\n');
}
const cargo = resolve(root, 'Cargo.toml');
const source = await readFile(cargo, 'utf8');
if (!/\[workspace\.package\]\s*\nversion = "[^"]+"/.test(source)) throw new Error('Workspace version not found.');
await writeFile(cargo, source.replace(/(\[workspace\.package\]\s*\nversion = ")[^"]+"/, `$1${version}"`));
const lockPath = resolve(root, 'Cargo.lock');
const lock = await readFile(lockPath, 'utf8');
const packages = new Set(['hongsi-app', 'hongsi-core', 'hongsi-direct', 'hongsi-server']);
const updatedLock = lock.replace(/(\[\[package\]\]\nname = "([^"]+)"\nversion = ")[^"]+"/g, (entry, prefix, name) => {
  if (!packages.delete(name)) return entry;
  return `${prefix}${version}"`;
});
if (packages.size) throw new Error('Workspace package missing from Cargo.lock.');
await writeFile(lockPath, updatedLock);
console.log(`Release: ${tag}`);
