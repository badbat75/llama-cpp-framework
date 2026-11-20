// The fe's text lives in strings/en_US.json, linked from the code by key,
// as the repo's rule "No user-visible text in the code" asks.
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import ts from 'typescript'
import { expect, test } from 'vitest'
import strings from './strings/en_US.json'
import { t } from './strings.ts'

const SRC = join(import.meta.dirname, '.')

/** Every source file of the page, without the tests. */
function sources(): { name: string; text: string }[] {
  return readdirSync(SRC, { recursive: true, encoding: 'utf8' })
    .filter((f) => /\.tsx?$/.test(f) && !/\.test\.tsx?$/.test(f))
    .map((name) => ({ name, text: readFileSync(join(SRC, name), 'utf8') }))
}

/**
 * The literal text written between JSX tags in `source` (a .tsx file), trimmed:
 * the JsxText nodes holding a letter. Parsed with the TypeScript compiler, so
 * generics (`useRef<A>(null)`), comparisons (`a > 1`) and arrows (`=>`) in plain
 * code are never taken for text, and `{expressions}` are not text.
 */
function jsxText(name: string, source: string): string[] {
  const file = ts.createSourceFile(
    name,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX,
  )
  const found: string[] = []
  const visit = (node: ts.Node) => {
    if (ts.isJsxText(node) && /[A-Za-z]/.test(node.text))
      found.push(node.text.trim())
    ts.forEachChild(node, visit)
  }
  visit(file)
  return found
}

// ── Failure paths first ─────────────────────────────────────────────────

test('a missing key shows as itself, as in pai-core', () => {
  expect(t('no-such-key')).toBe('no-such-key')
})

test('a missing value leaves its placeholder, other braces stay', () => {
  expect(t('version-label')).toBe('Version {0}')
})

// ── What it does ────────────────────────────────────────────────────────

// Brief 11 (fe/briefs/11-resource-refusals.md): the two resource refusals of a
// model profile, each in its own key. The expected text is the brief's, between
// the outer double quotes, backticks included, so the strings file carries it
// exactly (the value the code links by key, never text in the code).

test('11 M1.1: en_US.json holds api-model-profile-over-memory with exactly the text of brief 11 section 2', () => {
  const table: Record<string, string> = strings
  expect(table['api-model-profile-over-memory'], 'the text of the key').toBe(
    '`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB. To make it fit, lower its Context size, or pick a smaller K-cache type or V-cache type (cards Resource / context and KV cache).',
  )
})

test('11 M1.10: en_US.json holds api-model-profile-model-missing with exactly the text of brief 11 section 7', () => {
  const table: Record<string, string> = strings
  expect(table['api-model-profile-model-missing'], 'the text of the key').toBe(
    '`{0}` names a model file that is not there: `{2}` is missing for instance `{1}`. Put the file at that path on the server machine, or change the Model file in pai-config there.',
  )
})

test('placeholders are filled by position', () => {
  expect(t('version-label', '2.99.0')).toBe('Version 2.99.0')
})

test('every key the code uses has a value, and every value is used', () => {
  const used = new Set<string>()
  for (const { text } of sources()) {
    for (const m of text.matchAll(/\bt\(\s*'([^']+)'/g)) used.add(m[1]!)
  }
  const defined = new Set(Object.keys(strings))
  expect(
    [...used].filter((k) => !defined.has(k)),
    'keys with no value',
  ).toEqual([])
  expect(
    [...defined].filter((k) => !used.has(k)),
    'values no code uses',
  ).toEqual([])
})

test('the JSX text check catches literal text and nothing else', () => {
  // Literal text between tags is caught.
  expect(jsxText('a.tsx', 'const a = <p>Hello</p>')).toEqual(['Hello'])
  expect(
    jsxText('b.tsx', 'const b = <button type="button">Save</button>'),
  ).toEqual(['Save'])
  expect(jsxText('c.tsx', "const c = <p>{t('k')} more words</p>")).toEqual([
    'more words',
  ])
  // Plain TypeScript between two tags is not text.
  const code = [
    'function C(x: Set<string>, items: { ok: boolean }[]) {',
    '  const a = useRef<HTMLElement>(null)',
    '  const b = useRef<HTMLDivElement>(null)',
    '  const many = x.size > 1',
    '  if (many) return (<table ref={a}></table>)',
    '  const kept = items.filter((i) => i.ok)',
    '  return <p ref={b}>{kept.length}</p>',
    '}',
  ].join('\n')
  expect(jsxText('d.tsx', code)).toEqual([])
  // Expressions, whitespace and punctuation are not text.
  expect(
    jsxText('e.tsx', "const e = <p>\n  {t('k')} : {'CUDA'}\n</p>"),
  ).toEqual([])
})

test('no component writes text of its own in JSX', () => {
  const offenders: string[] = []
  for (const { name, text } of sources()) {
    if (!name.endsWith('.tsx')) continue
    for (const literal of jsxText(name, text)) {
      offenders.push(`${name}: ${literal}`)
    }
  }
  expect(offenders).toEqual([])
})
