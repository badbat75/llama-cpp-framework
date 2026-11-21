// The font files of fe/briefs/06-hearth-h1-tokens.md section 9 (H1b), pinned as
// text: the six @font-face rules of index.css, the three metric fallbacks, the
// two --font / --mono stacks, the two npm packages in fe/package.json and in
// the lockfile, and the "nothing is fetched at run time" rules. jsdom loads no
// font and does no layout, so that the faces load, that the latin-ext files
// stay unfetched on an English page and that the swap moves no line stays for
// the UX pass (the brief's section 9). The parsers are a small local copy of
// the readers of HearthTokens.test.ts and DisabledLook.test.ts, which export
// no helper.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { expect, test } from 'vitest'

const FE = join(import.meta.dirname, '..')

// Comments dropped: a comment is the one place a citation URL may sit.
const CSS = readFileSync(join(FE, 'src', 'index.css'), 'utf8').replace(
  /\/\*[\s\S]*?\*\//g,
  '',
)

// Whitespace runs collapsed to one space: a long stack or unicode-range is
// written over several lines.
const flat = (s: string): string => s.replace(/\s+/g, ' ').trim()
// Whitespace dropped: in the two lists a space is noise, not the contract.
const tight = (s: string): string => s.replace(/\s+/g, '')
// A family as a name, without the quotes the declaration carries.
const name = (s: string): string => flat(s).replace(/^['"]|['"]$/g, '')

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name_: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name_) val = d.slice(i + 1).trim()
  }
  return val
}

// The `--name: value;` pairs of a block, in order.
const customProps = (body: string): [string, string][] =>
  [...body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]*)/g)].map((m) => [
    m[1],
    m[2].trim(),
  ])

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

type Rule = { sel: string; compact: string; body: string; at: number }

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim(),
  // Whitespace dropped: the selector is written over several lines.
  compact: m[1].replace(/\s+/g, ''),
  body: m[2],
  at: m.index ?? 0,
}))

// Top-level rules only, as the two other CSS readers do: a rule nested in an
// @media block is not the rule the cascade applies on a wide screen.
const TOP = RULES.filter((r) => opened(r.at) === 0)

// The end of the block that starts at this offset, by its matched closing brace.
const rangeEnd = (open: number): number => {
  let end = open
  let depth = 0
  for (; end < CSS.length; end++) {
    if (CSS[end] === '{') depth++
    else if (CSS[end] === '}' && --depth === 0) return end
  }
  throw new Error(`the braces of the block at ${open} never close`)
}

const atRange = (needle: string): [number, number] => {
  const open = CSS.indexOf(needle)
  if (open < 0) throw new Error(`no ${needle} block in index.css`)
  return [open, rangeEnd(open)]
}

// A rule whose selector is exactly this, and one only: a throw names the miss.
const one = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length ? ` (${list.map((r) => r.compact).join(' | ')})` : ''),
    )
  return list[0]
}

const lightBody = (): string =>
  one(
    TOP.filter((r) => r.compact === ':root'),
    'the top-level :root token block',
  ).body
const darkBody = (): string => {
  const [open, end] = atRange('@media (prefers-color-scheme: dark)')
  return one(
    RULES.filter(
      (r) =>
        opened(r.at) > 0 &&
        r.at > open &&
        r.at < end &&
        r.compact === ":root:not([data-theme='light'])",
    ),
    'the :root rule inside the @media (prefers-color-scheme: dark) block',
  ).body
}

// The @font-face blocks of the stylesheet, bodies only.
const FACE_BODIES: string[] = [
  ...CSS.matchAll(/@font-face\s*\{([^{}]*)\}/g),
].map((m) => m[1])

// The one face whose src names this file.
const faceFor = (file: string): string => {
  const hits = FACE_BODIES.filter((b) => prop(b, 'src').includes(file))
  if (hits.length !== 1)
    throw new Error(
      `expected exactly one @font-face whose src names ${file}, found ${hits.length}`,
    )
  return hits[0]
}

// The one face of this family at this weight.
const faceOf = (family: string, weight: string): string => {
  const hits = FACE_BODIES.filter(
    (b) =>
      name(prop(b, 'font-family')) === family &&
      prop(b, 'font-weight') === weight,
  )
  if (hits.length !== 1)
    throw new Error(
      `expected exactly one @font-face for ${family} at ${weight}, found ${hits.length}`,
    )
  return hits[0]
}

const PKG_NEXT = '@fontsource/atkinson-hyperlegible-next'
const PKG_MONO = '@fontsource/atkinson-hyperlegible-mono'
const VERSION = '5.3.0'
const NEXT = 'Atkinson Hyperlegible Next'
const MONO = 'Atkinson Hyperlegible Mono'

// The unicode-range of each subset, copied from each package's 400.css at
// 5.3.0, so a latin-ext file is fetched only when the page shows such a letter.
const LATIN =
  'U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD'
const LATIN_EXT =
  'U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF'

// The six faces of the brief's file list. bytes: the size of that file in the
// package at 5.3.0, read from the jsDelivr listing the brief cites.
type Wanted = {
  pkg: string
  family: string
  file: string
  weight: string
  subset: string
  range: string
  bytes: number
}
const SIX: Wanted[] = [
  {
    pkg: PKG_NEXT,
    family: NEXT,
    file: 'atkinson-hyperlegible-next-latin-400-normal.woff2',
    weight: '400',
    subset: 'latin',
    range: LATIN,
    bytes: 12096,
  },
  {
    pkg: PKG_NEXT,
    family: NEXT,
    file: 'atkinson-hyperlegible-next-latin-700-normal.woff2',
    weight: '700',
    subset: 'latin',
    range: LATIN,
    bytes: 12732,
  },
  {
    pkg: PKG_NEXT,
    family: NEXT,
    file: 'atkinson-hyperlegible-next-latin-ext-400-normal.woff2',
    weight: '400',
    subset: 'latin-ext',
    range: LATIN_EXT,
    bytes: 7376,
  },
  {
    pkg: PKG_NEXT,
    family: NEXT,
    file: 'atkinson-hyperlegible-next-latin-ext-700-normal.woff2',
    weight: '700',
    subset: 'latin-ext',
    range: LATIN_EXT,
    bytes: 7556,
  },
  {
    pkg: PKG_MONO,
    family: MONO,
    file: 'atkinson-hyperlegible-mono-latin-400-normal.woff2',
    weight: '400',
    subset: 'latin',
    range: LATIN,
    bytes: 10056,
  },
  {
    pkg: PKG_MONO,
    family: MONO,
    file: 'atkinson-hyperlegible-mono-latin-ext-400-normal.woff2',
    weight: '400',
    subset: 'latin-ext',
    range: LATIN_EXT,
    bytes: 6528,
  },
]

// The three metric fallbacks of the brief's table, so the swap does not move
// the layout: the local list and the four metric overrides.
type Fallback = {
  family: string
  weight: string
  src: string
  sizeAdjust: string
  ascent: string
  descent: string
  lineGap: string
}
const FALLBACKS: Fallback[] = [
  {
    family: 'Atkinson Hyperlegible Next Fallback',
    weight: '400',
    src: "local('Arial'), local('Liberation Sans')",
    sizeAdjust: '98.8%',
    ascent: '99.6%',
    descent: '32%',
    lineGap: '0%',
  },
  {
    family: 'Atkinson Hyperlegible Next Fallback',
    weight: '700',
    src: "local('Arial Bold'), local('Arial-BoldMT'), local('Liberation Sans Bold')",
    sizeAdjust: '99.3%',
    ascent: '99.1%',
    descent: '31.8%',
    lineGap: '0%',
  },
  {
    family: 'Atkinson Hyperlegible Mono Fallback',
    weight: '400',
    src: "local('Courier New'), local('Liberation Mono')",
    sizeAdjust: '105.3%',
    ascent: '93.4%',
    descent: '30%',
    lineGap: '0%',
  },
]

// The two stacks of the brief: the family, its metric fallback, then today's
// system stack unchanged.
const FONT_STACK =
  "'Atkinson Hyperlegible Next', 'Atkinson Hyperlegible Next Fallback', system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
const MONO_STACK =
  "'Atkinson Hyperlegible Mono', 'Atkinson Hyperlegible Mono Fallback', ui-monospace, 'Cascadia Mono', Menlo, monospace"

const json = (path: string): unknown =>
  JSON.parse(readFileSync(join(FE, path), 'utf8')) as unknown

type Deps = {
  dependencies?: Record<string, string>
  devDependencies?: Record<string, string>
}
type LockEntry = {
  version?: string
  integrity?: string
  devDependencies?: Record<string, string>
}
type Lock = { packages?: Record<string, LockEntry | undefined> }

// The font binaries of a directory under fe/, recursively, by name.
const listed = (dir: string): string[] =>
  existsSync(dir)
    ? readdirSync(dir, { recursive: true })
        .map((f) => String(f))
        .filter((f) => /\.(woff2|woff|ttf|otf|eot)$/i.test(f))
    : []

// The sources of fe/src the page ships: a test file's needles are data, so the
// scan for a fetched font skips the tests that name it.
const appSources = (): [string, string][] => {
  const root = join(FE, 'src')
  const out: [string, string][] = []
  for (const e of readdirSync(root, { recursive: true, withFileTypes: true })) {
    if (!e.isFile()) continue
    const abs = join(e.parentPath, e.name)
    const rel = relative(root, abs)
    if (/\.test\.tsx?$/.test(rel)) continue
    if (!/\.(ts|tsx|css|html|js)$/.test(rel)) continue
    out.push([rel, readFileSync(abs, 'utf8')])
  }
  return out
}

test('H1b: index.css holds exactly nine @font-face rules, the six faces and the three metric fallbacks, and no other face of the packages', () => {
  expect(
    FACE_BODIES.length,
    'six faces of the two families plus three metric fallbacks: a tenth is a face the brief did not choose (no Mono 700, no italic)',
  ).toBe(9)
  const fam = (b: string): string => name(prop(b, 'font-family'))
  expect(
    FACE_BODIES.filter((b) => fam(b) === NEXT).length,
    'Next: latin 400 and 700, latin-ext 400 and 700',
  ).toBe(4)
  expect(
    FACE_BODIES.filter((b) => fam(b) === MONO).length,
    'Mono: latin 400 and latin-ext 400, mono text is never bold',
  ).toBe(2)
  expect(
    FACE_BODIES.filter((b) => fam(b).endsWith(' Fallback')).length,
    'the three metric fallbacks',
  ).toBe(3)
})

test.each(SIX.map((w) => [w.file, w] as [string, Wanted]))(
  'H1b: %s: one @font-face of the family, normal, its weight, swap, the subset range and a woff2-only src into the package',
  (_file, w) => {
    const face = faceFor(w.file)
    expect(name(prop(face, 'font-family')), 'the family the stack names').toBe(
      w.family,
    )
    expect(prop(face, 'font-weight'), 'the weight the file carries').toBe(
      w.weight,
    )
    expect(prop(face, 'font-style'), 'normal only, no italic face').toBe(
      'normal',
    )
    expect(prop(face, 'font-display'), 'the swap the fallbacks cover').toBe(
      'swap',
    )
    expect(
      tight(prop(face, 'unicode-range')),
      `${w.subset}: the package's own range for the subset, so a latin-ext file is fetched only for a letter of it`,
    ).toBe(tight(w.range))
    const src = prop(face, 'src')
    expect(
      (src.match(/url\(/g) ?? []).length,
      'one url, not the package pair with the .woff the bff would embed for no browser',
    ).toBe(1)
    expect(
      src.includes(`${w.pkg}/files/`),
      `the src reaches ${w.pkg}'s files folder: "${src}"`,
    ).toBe(true)
    expect(
      tight(src).endsWith(`/files/${w.file})format('woff2')`),
      `the file name, format('woff2') and nothing after it: "${src}"`,
    ).toBe(true)
  },
)

test('H1b: no .woff, no remote url and no @import in index.css', () => {
  expect(
    CSS.includes('.woff)'),
    'the packages ship a .woff pair, the hand-written rules drop it',
  ).toBe(false)
  expect(CSS, 'no http(s) scheme and no protocol-relative url').not.toMatch(
    /https?:\/\/|url\(\s*\/\//,
  )
  expect(
    CSS,
    'the six rules are written in index.css, the packages are not imported',
  ).not.toMatch(/@import/)
})

test.each(
  FALLBACKS.map((f) => [`${f.family} ${f.weight}`, f] as [string, Fallback]),
)(
  'H1b: the metric fallback of %s has the local list and the four overrides of the brief',
  (_k, f) => {
    const face = faceOf(f.family, f.weight)
    expect(
      tight(prop(face, 'src')),
      'a local() list, no url and no format: the face is a font of the machine',
    ).toBe(tight(f.src))
    expect(
      prop(face, 'size-adjust'),
      'the advance width of the real face, so the swap does not reflow',
    ).toBe(f.sizeAdjust)
    expect(prop(face, 'ascent-override'), 'its ascent').toBe(f.ascent)
    expect(prop(face, 'descent-override'), 'its descent').toBe(f.descent)
    expect(prop(face, 'line-gap-override'), 'and no line gap').toBe(f.lineGap)
  },
)

test('H1b: --font and --mono are the brief stacks, the metric fallback between the family and the system stack, on the top-level :root only', () => {
  const light = customProps(lightBody())
  expect(
    flat(light.find((p) => p[0] === '--font')?.[1] ?? ''),
    'the body stack: the face, its metric fallback, then today system stack',
  ).toBe(FONT_STACK)
  expect(
    flat(light.find((p) => p[0] === '--mono')?.[1] ?? ''),
    'the mono stack: the face, its metric fallback, then today mono stack',
  ).toBe(MONO_STACK)
  expect(
    customProps(darkBody())
      .map((p) => p[0])
      .filter((n) => n === '--font' || n === '--mono'),
    'a stack is not a colour: the dark block keeps them out, as H1a.2 pins',
  ).toEqual([])
})

test('H1b guard: the stacks reach the page through the tokens only: body is var(--font), the mono elements var(--mono), no rule names a family', () => {
  const used: string[] = []
  for (const r of RULES) {
    if (r.compact === ':root' || r.compact === '@font-face') continue
    const v = prop(r.body, 'font-family')
    if (v) used.push(`${r.compact}: ${v}`)
  }
  expect(
    used.sort(),
    'every family comes from a token, so the faces apply page wide and no rule dodges them',
  ).toEqual([
    // Brief 14, DESIGN.md 5.2 "GPU list": the row's id is monospace.
    '.check-list.gpu-id: var(--mono)',
    '.full-value: var(--mono)',
    '.mono,input.mono: var(--mono)',
    'body: var(--font)',
  ])
})

test('H1b guard: nothing asks for a face the six files do not ship: no italic, and no weight outside 400, 600 and 700', () => {
  const italic: string[] = []
  const off: string[] = []
  for (const r of RULES) {
    if (r.compact === '@font-face') continue
    const style = prop(r.body, 'font-style')
    if (style && style !== 'normal')
      italic.push(`${r.compact}: font-style: ${style}`)
    const w = prop(r.body, 'font-weight')
    // The faces ship 400 and 700; a 600 renders as 700 with no synthesis,
    // which is the brief's decision (section 0, "Bold stays 600").
    if (w && !['400', '600', '700'].includes(w))
      off.push(`${r.compact}: font-weight: ${w}`)
  }
  expect(
    italic,
    'no italic face ships, so an italic request would be a synthesised slant',
  ).toEqual([])
  expect(
    off,
    'a weight outside the shipped faces is a synthesis the brief ruled out',
  ).toEqual([])
})

test('H1b: fe/package.json lists both packages at exactly 5.3.0 in devDependencies, pinned with no ^ or ~, and not as a dependency', () => {
  const pkg = json('package.json') as Deps
  for (const dep of [PKG_NEXT, PKG_MONO]) {
    expect(
      pkg.devDependencies?.[dep],
      `${dep}: a devDependency of fe, exact`,
    ).toBe(VERSION)
    expect(
      pkg.dependencies?.[dep],
      `${dep}: the page fetches nothing, so the font is not a runtime dependency`,
    ).toBeUndefined()
  }
})

test('H1b: fe/package-lock.json resolves both packages at 5.3.0 with an integrity, for the root devDependencies', () => {
  const lock = json('package-lock.json') as Lock
  for (const dep of [PKG_NEXT, PKG_MONO]) {
    expect(
      lock.packages?.['']?.devDependencies?.[dep],
      `${dep}: in the lockfile root's devDependencies at ${VERSION}`,
    ).toBe(VERSION)
    const entry = lock.packages?.[`node_modules/${dep}`]
    expect(entry?.version, `${dep}: resolved`).toBe(VERSION)
    expect(
      entry?.integrity ?? '',
      `${dep}: the resolved tarball carries an integrity`,
    ).toMatch(/^sha512-/)
  }
})

test.each(SIX.map((w) => [w.file, w] as [string, Wanted]))(
  'H1b: %s is in node_modules at the byte count of 5.3.0, the file the @font-face names',
  (_file, w) => {
    const path = join(FE, 'node_modules', w.pkg, 'files', w.file)
    expect(existsSync(path), `${path}: the file of ${w.pkg}@${VERSION}`).toBe(
      true,
    )
    expect(
      statSync(path).size,
      `${w.file}: the byte count the brief read from the package at ${VERSION}`,
    ).toBe(w.bytes)
  },
)

test('H1b: no font binary is committed under fe/ outside node_modules (fe/src and fe/public listed)', () => {
  expect(
    [...listed(join(FE, 'src')), ...listed(join(FE, 'public'))],
    'the fonts are the two npm packages, Vite copies them into fe/dist at build',
  ).toEqual([])
})

test('H1b: nothing is fetched at run time: no font CDN, no remote stylesheet, no @import, no .woff and no preload in index.html, none in the sources of fe/src', () => {
  const html = readFileSync(join(FE, 'index.html'), 'utf8')
  const files: [string, string][] = [['index.html', html], ...appSources()]
  // A comment is not a request, and the six srcs end in '.woff2', so the WOFF1
  // rule reads the same comment-free text as the CSS reader above, with the
  // woff2 ending excluded: a literal '.woff' is inside '.woff2'.
  const plain = [
    'fonts.googleapis',
    'fonts.gstatic',
    'cdn.jsdelivr',
    'unpkg.com',
    '@import',
  ]
  const hits: string[] = []
  for (const [file, raw] of files) {
    const text = /\.css$/.test(file)
      ? raw.replace(/\/\*[\s\S]*?\*\//g, '')
      : raw
    for (const needle of plain)
      if (text.includes(needle)) hits.push(`${file}: ${needle}`)
    if (/\.woff(?!2)/.test(text)) hits.push(`${file}: .woff (WOFF1)`)
  }
  expect(
    hits,
    'the fonts come from the two packages and Vite, never from a network address',
  ).toEqual([])
  expect(
    html.includes('rel="preload"'),
    'no preload: the name Vite gives the file in fe/dist carries a hash the page cannot know',
  ).toBe(false)
})
