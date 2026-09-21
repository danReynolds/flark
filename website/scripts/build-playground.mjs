import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const site = fileURLToPath(new URL('../', import.meta.url));
const app = join(site, 'playground');
const output = join(app, 'build/web');
execFileSync(process.env.FLUTTER || 'flutter', [
  'build', 'web', '--release', '--no-web-resources-cdn',
  '--no-wasm-dry-run', '--pwa-strategy=none', '--base-href', '/',
], { cwd: app, stdio: 'inherit' });

// Relative assets let the same build run at localhost and /flark/ on Pages.
const index = join(output, 'index.html');
const html = readFileSync(index, 'utf8');
if (!html.includes('<base href="/">')) throw new Error('Unexpected Flutter base URL; refusing to publish broken asset paths.');
writeFileSync(index, html.replace('<base href="/">', '<base href="./">'));
const hash = createHash('sha256');
function hashDirectory(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const path = join(directory, entry.name);
    hash.update(entry.name);
    if (entry.isDirectory()) hashDirectory(path);
    else hash.update(readFileSync(path));
  }
}
hashDirectory(output);
const version = hash.digest('hex').slice(0, 12);
const target = join(site, 'public/playground');
rmSync(target, { recursive: true, force: true });
mkdirSync(target, { recursive: true });
cpSync(output, join(target, version), { recursive: true });
const manifest = join(site, 'src/generated/playground.json');
mkdirSync(dirname(manifest), { recursive: true });
writeFileSync(manifest, JSON.stringify({ path: `playground/${version}/index.html` }) + '\n');
console.log(`Live Flark demo: playground/${version}/`);
