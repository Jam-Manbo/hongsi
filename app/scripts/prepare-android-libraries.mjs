import { createHash } from 'node:crypto';
import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const android = resolve(dirname(fileURLToPath(import.meta.url)), '../src-tauri/gen/android');
const libraries = new Set(['tauri-android', 'tauri-plugin-notifications']);

async function readOptional(path) {
  try { return await readFile(path); }
  catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
}

function replaceRequired(source, before, after, count = 1) {
  if (source.split(before).length - 1 !== count) {
    throw new Error(`Android dependency changed; review the AGP compatibility patch for: ${before}`);
  }
  return source.replaceAll(before, after);
}

async function writeChanged(path, content) {
  const bytes = Buffer.isBuffer(content) ? content : Buffer.from(content);
  const previous = await readOptional(path);
  if (previous?.equals(bytes)) return;
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, bytes);
}

async function copyRuntime(source, destination, hash, prefix = '') {
  await mkdir(destination, { recursive: true });
  const entries = (await readdir(source, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name));
  const names = new Set(entries.map(entry => entry.name));
  for (const name of await readdir(destination)) {
    if (!names.has(name)) await rm(join(destination, name), { recursive: true, force: true });
  }
  for (const entry of entries) {
    const relative = `${prefix}${entry.name}`;
    if (entry.isDirectory()) {
      await copyRuntime(join(source, entry.name), join(destination, entry.name), hash, `${relative}/`);
    } else if (entry.isFile()) {
      let content = await readFile(join(source, entry.name));
      if (relative === 'java/app/tauri/plugin/PluginHandle.kt') {
        let text = content.toString('utf8');
        text = replaceRequired(text, 'var pluginCursor: Class<*> = instance.javaClass', 'var pluginCursor: Class<*>? = instance.javaClass');
        text = replaceRequired(text, 'while (pluginCursor.name != Any::class.java.name)', 'while (pluginCursor != null && pluginCursor != Any::class.java)');
        content = Buffer.from(text);
      }
      hash.update(relative).update(content);
      await writeChanged(join(destination, entry.name), content);
    } else {
      throw new Error(`Unexpected Android dependency entry: ${relative}`);
    }
  }
}

async function prepare(name, source) {
  if (!libraries.has(name)) throw new Error(`Unsupported Android dependency: ${name}`);
  const destination = join(android, '.tauri', 'libraries', name);
  const hash = createHash('sha256');
  const original = await readFile(join(source, 'build.gradle.kts'), 'utf8');
  let gradle = replaceRequired(original, '    id("org.jetbrains.kotlin.android")\n', '');
  gradle = replaceRequired(gradle, 'JavaVersion.VERSION_1_8', 'JavaVersion.VERSION_21', 2);
  gradle = replaceRequired(gradle, 'JvmTarget.JVM_1_8', 'JvmTarget.JVM_21');
  if (!(await readOptional(join(source, 'consumer-rules.pro')))) {
    gradle = replaceRequired(gradle, '        consumerProguardFiles("consumer-rules.pro")\n', '');
  }
  hash.update(gradle);
  await writeChanged(join(destination, 'build.gradle.kts'), gradle);
  await copyRuntime(join(source, 'src', 'main'), join(destination, 'src', 'main'), hash);
  for (const file of ['build.properties', 'consumer-rules.pro', 'proguard-rules.pro']) {
    const content = await readOptional(join(source, file));
    if (content) {
      hash.update(file).update(content);
      await writeChanged(join(destination, file), content);
    } else {
      await rm(join(destination, file), { force: true });
    }
  }
  return { name, directory: destination, fingerprint: hash.digest('hex') };
}

try {
  const args = process.argv.slice(2);
  if (args.length !== 4 || new Set([args[0], args[2]]).size !== 2) {
    throw new Error('Pass the Tauri and notifications Android project names and source directories.');
  }
  const prepared = [];
  for (let i = 0; i < args.length; i += 2) prepared.push(await prepare(args[i], resolve(args[i + 1])));
  console.log(JSON.stringify(prepared));
} catch (error) {
  console.error(error instanceof Error ? error.message : 'Android dependency preparation failed.');
  process.exitCode = 1;
}
