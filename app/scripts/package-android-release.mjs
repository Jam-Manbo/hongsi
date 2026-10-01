import { readFile, writeFile, mkdir, copyFile, stat } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export function releaseIdentity(repository, tag, version) {
  if (!/^[A-Za-z0-9_-][A-Za-z0-9_.-]*\/[A-Za-z0-9_-][A-Za-z0-9_.-]*$/.test(repository)) throw new Error('Release repository must be OWNER/REPO.');
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version) || ![version, `v${version}`].includes(tag)) throw new Error('A stable release tag must match the app version.');
  const name = `hongsi-${version}.apk`;
  return { name, url: `https://github.com/${repository}/releases/download/${tag}/${name}` };
}

export async function sha256(file) {
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(file)) digest.update(chunk);
  return digest.digest('hex');
}

export async function packageRelease({ apk, versionCode, output, version, tag = `v${version}`, repository, notes = '' }) {
  if (!Number.isSafeInteger(versionCode) || versionCode < 1 || versionCode > 2147483647) throw new Error('Android versionCode is invalid.');
  const { name, url } = releaseIdentity(repository, tag, version);
  const size = (await stat(apk)).size;
  if (size < 1 || size > 256 * 1024 * 1024) throw new Error('APK size is invalid.');
  if (Buffer.byteLength(notes, 'utf8') > 10000) throw new Error('Release notes are too long.');
  const manifest = { version, versionCode, url, sha256: await sha256(apk), size, notes };
  const dir = resolve(output, 'android');
  await mkdir(dir, { recursive: true });
  await copyFile(apk, resolve(dir, name));
  await writeFile(resolve(dir, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');
  return manifest;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const [apk, code, output] = process.argv.slice(2);
  if (!apk || !output) throw new Error('Usage: package-android-release.mjs APK VERSION_CODE OUTPUT_DIRECTORY');
  const { version } = JSON.parse(await readFile(resolve(root, 'app/src-tauri/tauri.conf.json'), 'utf8'));
  const repository = process.env.HONGSI_RELEASE_REPOSITORY || process.env.GITHUB_REPOSITORY || 'Jam-Manbo/hongsi';
  const tag = process.env.RELEASE_TAG || `v${version}`;
  const notes = (await readFile(resolve(root, 'RELEASE_NOTES.md'), 'utf8')).trim();
  await packageRelease({ apk, versionCode: Number(code), output, version, tag, repository, notes });
  console.log(`Prepared Android ${version} for GitHub Release ${repository}@${tag}`);
}
