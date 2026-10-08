import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')) as { version: string };

function buildCommit(): string {
  if (process.env.HONGSI_BUILD_COMMIT !== undefined) {
    const sha = process.env.HONGSI_BUILD_COMMIT.trim();
    return /^[a-f0-9]{40}$/i.test(sha) ? sha : '';
  }
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], {
      cwd: fileURLToPath(new URL('..', import.meta.url)),
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    const sha = process.env.GITHUB_SHA?.trim() ?? '';
    return /^[a-f0-9]{40}$/i.test(sha) ? sha : '';
  }
}

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(process.env.HONGSI_RELEASE_LABEL || pkg.version),
    __APP_COMMIT__: JSON.stringify(buildCommit()),
  },
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    proxy: { '/api': 'http://127.0.0.1:8787' },
  },
  build: { target: 'es2022', outDir: 'dist' },
});
