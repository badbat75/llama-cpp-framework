// Regenerate resources\llama.ico from llama.cpp's webui logo SVG.
//
// Upstream commits no .ico — the webui's favicon.ico is produced by its npm
// build from tools/ui/src/lib/assets/logo.svg, a `currentColor` SVG. This
// script mirrors how upstream renders its own PWA/apple-touch app icons:
// dark #111111 glyph on a white rounded tile, so the icon reads on both dark
// taskbars and light Explorer backgrounds. llama.ico is gitignored;
// llama-cpp-config's build.rs (and, belt-and-braces, 03-package.ps1) runs
// this script automatically whenever the file is missing — delete llama.ico
// to force a regeneration after an upstream logo change.
//
// Manual usage (Node 18+; the clone must exist — 02-build.ps1 creates it):
//   cd resources
//   npm install --no-save sharp sharp-ico
//   node generate-llama-ico.mjs [path-to-llama.cpp-clone] [output.ico]
//
// Defaults: clone = ..\build\llama.cpp, output = .\llama.ico. The transient
// node_modules\ / package*.json in resources\ are gitignored.
import { readFileSync, existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';
import ico from 'sharp-ico';

const HERE = dirname(fileURLToPath(import.meta.url));
const CLONE = resolve(process.argv[2] ?? resolve(HERE, '..', 'build', 'llama.cpp'));
const SRC = resolve(CLONE, 'tools', 'ui', 'src', 'lib', 'assets', 'logo.svg');
const OUT = resolve(process.argv[3] ?? resolve(HERE, 'llama.ico'));

const GLYPH = '#111111';
const TILE = '#ffffff';
const CANVAS = 512;
const PAD = 0.14; // fraction of canvas left as margin per side
const RADIUS = 0.22 * CANVAS; // rounded-tile corner radius
const SIZES = [256, 128, 64, 48, 32, 24, 16];

if (!existsSync(SRC)) {
	console.error(`Logo SVG not found: ${SRC}`);
	console.error('Clone llama.cpp first (02-build.ps1 does), or pass the clone path as argument.');
	process.exit(1);
}

const svg = readFileSync(SRC, 'utf8').replaceAll('currentColor', GLYPH);

const glyphSize = Math.round(CANVAS * (1 - 2 * PAD));
const glyphPng = await sharp(Buffer.from(svg), { density: 300 })
	.resize(glyphSize, glyphSize, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
	.png()
	.toBuffer();

const tile = Buffer.from(
	`<svg width="${CANVAS}" height="${CANVAS}" xmlns="http://www.w3.org/2000/svg">
	   <rect x="0" y="0" width="${CANVAS}" height="${CANVAS}" rx="${RADIUS}" fill="${TILE}"/>
	 </svg>`
);

const masterPng = await sharp(tile)
	.composite([{ input: glyphPng, gravity: 'center' }])
	.png()
	.toBuffer();

await ico.sharpsToIco(
	SIZES.map((s) => sharp(masterPng).resize(s, s)),
	OUT,
	{ sizes: 'default', resizeOptions: {} }
);

const frames = ico.decode(readFileSync(OUT)).map((f) => `${f.width}x${f.height}`);
console.log(`wrote ${OUT} (${frames.join(', ')})`);
