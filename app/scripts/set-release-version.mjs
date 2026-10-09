import { appendFile, readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
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
const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], { cwd: root, encoding: 'utf8' }));
const members = new Set(metadata.workspace_members);
const packages = new Map(metadata.packages.filter(pkg => members.has(pkg.id)).map(pkg => [pkg.name, pkg.version]));
const updatedLock = lock.split('\n[[package]]\n').map(entry => {
  const name = /^name = "([^"]+)"$/m.exec(entry)?.[1];
  if (!packages.has(name) || /^source = /m.test(entry)) return entry;
  const packageVersion = packages.get(name);
  packages.delete(name);
  return entry.replace(/^version = "[^"]+"$/m, `version = "${packageVersion}"`);
}).join('\n[[package]]\n');
if (packages.size) throw new Error(`Workspace package missing from Cargo.lock: ${[...packages.keys()].join(', ')}`);
await writeFile(lockPath, updatedLock);
console.log(`Release: ${tag}`);
