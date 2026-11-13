// Regenerate resources\llama.ico from the llama brand icon.
//
// Upstream commits no .ico. The glyph is the llama-brand package's
// logo/svg/icon/icon-light.svg: the webui's own app icons come from the same
// package (tools/ui/package.json pins the commit; the old
// tools/ui/src/lib/assets/logo.svg the webui used to carry was removed in
// llama.cpp v1.0.0). This script downloads that pinned SVG once, caches it as
// resources\llama-brand-logo.svg (gitignored), and rasterizes it the way
// upstream renders its own PWA icons: the dark #111111 glyph on a white
// rounded tile, so the icon reads on both dark taskbars and light Explorer
// backgrounds. llama.ico is gitignored; llama-cpp-config's build.rs (and,
// belt-and-braces, xtask package) runs this script automatically whenever the
// file is missing. Delete llama.ico to force a regeneration; delete
// llama-brand-logo.svg to re-download the glyph (e.g. after a brand refresh).
//
// Manual usage (Node 18+):
//   cd resources
//   npm install --no-save sharp sharp-ico
//   node generate-llama-ico.mjs [output.ico]
//
// Default output: .\llama.ico. The transient node_modules\ / package*.json
// in resources\ are gitignored.
import { readFileSync, existsSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';
import ico from 'sharp-ico';

const HERE = dirname(fileURLToPath(import.meta.url));

// The commit llama.cpp v1.0.0 pins as the llama-brand dependency of tools/ui.
const BRAND_COMMIT = 'f05540643c1ca2a18d732ed6491c839686da9a3a';
const BRAND_URL = `https://raw.githubusercontent.com/ggml-org/llama-brand/${BRAND_COMMIT}/logo/svg/icon/icon-light.svg`;
const SRC = resolve(HERE, 'llama-brand-logo.svg');
const OUT = resolve(process.argv[2] ?? resolve(HERE, 'llama.ico'));

const GLYPH = '#111111';
const TILE = '#ffffff';
const CANVAS = 512;
const PAD = 0.14; // fraction of canvas left as margin per side
const RADIUS = 0.22 * CANVAS; // rounded-tile corner radius
const SIZES = [256, 128, 64, 48, 32, 24, 16];

if (!existsSync(SRC)) {
	const res = await fetch(BRAND_URL);
	if (!res.ok) {
		console.error(`download failed (${res.status}) from ${BRAND_URL}`);
		console.error('Fetch it by hand into resources/llama-brand-logo.svg, or fix the network, and re-run.');
		process.exit(1);
	}
	writeFileSync(SRC, await res.text());
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
