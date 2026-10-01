import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const version = process.argv[2]?.replace(/^v/, '');
if (!version || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) throw new Error('Version must be a stable vMAJOR.MINOR.PATCH tag.');
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
console.log(`Release version: ${version}`);
