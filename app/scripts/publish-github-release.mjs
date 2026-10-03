import { readFile, stat } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseIdentity, sha256 } from './package-android-release.mjs';

export async function publishRelease({ repository, tag, directory, run = args => execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }) }) {
  const manifestPath = resolve(directory, 'latest.json');
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const { name, url } = releaseIdentity(repository, tag, manifest.version);
  const apk = resolve(directory, name);
  if (manifest.url !== url || manifest.size !== (await stat(apk)).size || manifest.sha256 !== await sha256(apk)) throw new Error('Release manifest does not match its APK.');
  if (!Number.isSafeInteger(manifest.versionCode) || manifest.versionCode < 1 || manifest.versionCode > 2147483647) throw new Error('Invalid versionCode.');
  const files = [apk];
  const aab = resolve(directory, `hongsi-${manifest.version}.aab`);
  try { await stat(aab); files.push(aab); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  files.push(manifestPath);
  for (const file of files) {
    const release = JSON.parse(await run(['api', `repos/${repository}/releases/tags/${tag}`]));
    if (release.tag_name !== tag || release.draft !== false || release.prerelease !== false || !release.published_at) throw new Error('Publish a stable GitHub Release manually before uploading assets.');
    const filename = file.slice(file.lastIndexOf('/') + 1);
    const existing = release.assets.find(asset => asset.name === filename);
    if (existing) {
      const digest = `sha256:${await sha256(file)}`;
      if (existing.state !== 'uploaded' || existing.size !== (await stat(file)).size || existing.digest?.toLowerCase() !== digest) throw new Error(`Existing ${filename} differs. Publish a new version; assets are never overwritten.`);
      continue;
    }
    await run(['release', 'upload', tag, file, '--repo', repository]);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [directory] = process.argv.slice(2);
  if (!directory || !process.env.RELEASE_TAG || !process.env.GITHUB_REPOSITORY) throw new Error('Directory, RELEASE_TAG and GITHUB_REPOSITORY are required.');
  await publishRelease({ repository: process.env.GITHUB_REPOSITORY, tag: process.env.RELEASE_TAG, directory });
  console.log(`Uploaded release assets: ${process.env.RELEASE_TAG}`);
}
