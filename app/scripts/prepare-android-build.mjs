import { access, mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const android = resolve(dirname(fileURLToPath(import.meta.url)), '../src-tauri/gen/android');
const created = [];

function required(variable, label) {
  const value = process.env[variable];
  if (!value) throw new Error(`Set ${label}.`);
  return value;
}

function decode(value, label) {
  const encoded = value.replace(/\s/g, '');
  if (!encoded || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded)) {
    throw new Error(`${label} must contain valid Base64.`);
  }
  return Buffer.from(encoded, 'base64');
}

function escaped(value) {
  let result = '';
  for (let i = 0; i < value.length; i++) {
    const code = value.charCodeAt(i);
    result += code <= 32 || code > 126 || '\\=:#!'.includes(value[i])
      ? `\\u${code.toString(16).padStart(4, '0')}` : value[i];
  }
  return result;
}

try {
  await access(resolve(android, 'gradlew')).catch(() => {
    throw new Error('Commit the customized gen/android project.');
  });
  const firebase = decode(required('FIREBASE_BASE64', 'GOOGLE_SERVICES_JSON_BASE64'), 'GOOGLE_SERVICES_JSON_BASE64');
  try {
    const config = JSON.parse(firebase.toString('utf8'));
    if (!config || typeof config !== 'object' || Array.isArray(config)) throw new Error();
  } catch {
    throw new Error('GOOGLE_SERVICES_JSON_BASE64 must decode to a Firebase JSON configuration.');
  }
  const signed = Boolean(process.env.KEY_BASE64);
  if (process.env.RELEASE_EVENT === 'release' && !signed) {
    throw new Error('Stable releases require the permanent Android signing key. Debug APKs are never published as updates.');
  }
  const files = [[resolve(android, 'app/google-services.json'), firebase]];
  if (signed) {
    const keyAlias = required('KEY_ALIAS', 'ANDROID_KEY_ALIAS');
    const keyPassword = required('KEY_PASSWORD', 'ANDROID_KEY_PASSWORD');
    const key = decode(process.env.KEY_BASE64, 'ANDROID_KEY_BASE64');
    const storeFile = resolve(required('RUNNER_TEMP', 'RUNNER_TEMP'), 'hongsi-release.jks');
    const values = { storeFile, keyAlias, keyPassword, storePassword: process.env.STORE_PASSWORD || keyPassword };
    const properties = Object.entries(values).map(([name, value]) => `${name}=${escaped(value)}\n`).join('');
    files.push([storeFile, key], [resolve(android, 'keystore.properties'), properties]);
  }
  for (const [path, content] of files) {
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, content, { flag: 'wx', mode: 0o600 });
    created.push(path);
  }
  console.log(signed ? 'Firebase and Android signing files are ready.' : 'Firebase configuration is ready for a debug build.');
} catch (error) {
  for (const path of created) await rm(path, { force: true });
  console.error(error instanceof Error ? error.message : 'Android build preparation failed.');
  process.exitCode = 1;
}
