// The fe's text lives in strings/en_US.json, linked from the code by key,
// as the repo's rule "No user-visible text in the code" asks.
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
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

// ── Failure paths first ─────────────────────────────────────────────────

test('a missing key shows as itself, as in pai-core', () => {
  expect(t('no-such-key')).toBe('no-such-key')
})

test('a missing value leaves its placeholder, other braces stay', () => {
  expect(t('version-label')).toBe('Version {0}')
})

// ── What it does ────────────────────────────────────────────────────────

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

test('no component writes text of its own in JSX', () => {
  const offenders: string[] = []
  for (const { name, text } of sources()) {
    if (!name.endsWith('.tsx')) continue
    for (const m of text.matchAll(/>([^<>{}]*[A-Za-z][^<>{}]*)</g)) {
      offenders.push(`${name}: ${m[1]!.trim()}`)
    }
  }
  expect(offenders).toEqual([])
})
