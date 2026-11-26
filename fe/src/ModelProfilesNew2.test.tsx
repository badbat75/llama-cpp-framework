// Brief 22, slice 22C1 (fe/briefs/22-model-sources.md, as the "22P/22C check
// (2026-11-26)" amends it): the file half of the New model profile form's
// picker. This file is the Model file field: 22 P.3 (the combobox), 22 P.5a
// (which rows become options, and what each reads), 22 P.6 and P.6a (the
// folder's state line) and 22 P.4 (one folder read per showing). The form
// itself (22 C.1a, C.1, C.2, C.3) is ModelProfilesNew1.test.tsx; the checks,
// the request and its answers (22 C.4 to C.8) are ModelProfilesNew3.test.tsx.
//
// The fixture is `MODEL_FILES` in fe/src/test/bff.ts: main models, a split
// model listed once as its first shard, and one file of each kind P.5a leaves
// out. What jsdom cannot see is the `fe/ux` check's: the relative path wrapping
// anywhere at 320 px and 200 % text (22 P.5, 1.4.10).
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  MODELS_DIR,
  MODEL_FILES,
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  modelFilesBody,
  quiet,
  reply,
  startAt,
  type BffOptions,
  type ModelFileRow,
  type ModelProfile,
} from './test/bff.ts'

const FILES = '/api/node/model-files'

/** The rows P.5a keeps: the main models of the fixture, in the reply's order. */
const MAIN = MODEL_FILES.filter(
  (r) =>
    !/mmproj|imatrix|mtp-|eagle3-|dflash-|dspark-/.test(
      r.path.slice(r.path.lastIndexOf('/') + 1),
    ),
)

/** The size the brief's rule writes (section 2): bytes / 2^30 with one decimal
 *  and ` GiB`, bytes / 2^20 with no decimal and ` MiB` below 0.1 GiB. */
function size(bytes: number): string {
  const gib = bytes / 2 ** 30
  return gib < 0.1
    ? `${Math.round(bytes / 2 ** 20)} MiB`
    : `${gib.toFixed(1)} GiB`
}

/** What one option reads (22 P.5): the path relative to `dir`, `, `, the size,
 *  then `, ` and the parts for a split model. */
function optionText(row: ModelFileRow): string {
  const rel = row.path.slice(MODELS_DIR.length + 1)
  const parts =
    row.shards > 1 ? `, ${en('model-file-parts', `${row.shards}`)}` : ''
  return `${rel}, ${size(row.size)}${parts}`
}

/** `n` main models in one folder, for the 50 option cap. */
function manyFiles(n: number): ModelFileRow[] {
  return Array.from({ length: n }, (_, i) => ({
    path: `${MODELS_DIR}/m/model-${String(i).padStart(2, '0')}.gguf`,
    size: 2 ** 30,
    shards: 1,
    category: 'm',
  }))
}

beforeEach(() => {
  startAt('#/profiles/model')
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a promise chain) run. */
async function flushTasks(n = 3) {
  await Promise.all(
    Array.from({ length: n }, () => new Promise((r) => setTimeout(r, 0))),
  )
}

function modelProfile(id: string, enabled: boolean, model = ''): ModelProfile {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model }
}

const LIST: ModelProfile[] = [
  modelProfile('Qwen3-27B', true, '/m/qwen/Qwen3-27B-Q8_0.gguf'),
  modelProfile('Old-Coder', false),
]

/** The app at #/profiles/model, the table loaded. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfiles: LIST, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  return { bff, main }
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function key(el: HTMLElement, k: string) {
  fireEvent.keyDown(el, { key: k })
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The create form, with the file choice open. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(within(main).getByRole('button', { name: en('model-profiles-new') }))
  return within(main).findByRole('form', {
    name: en('model-profiles-new-title'),
  })
}

/** The Model file combobox. */
function fileBox(form: HTMLElement): HTMLInputElement {
  return within(form).getByRole('combobox', {
    name: en('model-profile-model-file'),
  }) as HTMLInputElement
}

/** The listbox the combobox points at with `aria-controls`. */
function listbox(box: HTMLInputElement): HTMLElement {
  const id = box.getAttribute('aria-controls')
  expect(id, 'the combobox names the list it controls').not.toBeNull()
  const list = document.getElementById(id!)
  expect(list, `the list #${id} is in the document`).not.toBeNull()
  return list!
}

/** The options the listbox shows (a closed list is `hidden`, and so are its
 *  options: `isHidden` reads the nearest `[hidden]`). */
function options(box: HTMLInputElement): HTMLElement[] {
  return [
    ...listbox(box).querySelectorAll<HTMLElement>('[role="option"]'),
  ].filter((o) => !isHidden(o))
}

function optionTexts(box: HTMLInputElement): string[] {
  return options(box).map((o) =>
    (o.textContent ?? '').replace(/\s+/g, ' ').trim(),
  )
}

/** The `role="status"` lines inside `root`, in the document's order. */
function statusLines(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('[role="status"]')]
}

/** The status line inside `root` whose text holds `text`. */
function statusWith(root: HTMLElement, text: string): HTMLElement {
  const line = statusLines(root).find((p) =>
    (p.textContent ?? '').includes(text),
  )
  expect(line, `a status line holding "${text}"`).not.toBeUndefined()
  return line!
}

/** The text of that line, whitespace collapsed. */
function statusText(root: HTMLElement, text: string): string {
  return (statusWith(root, text).textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The folder's reply landed: a `role="status"` line carries `text`. The list
 *  stays closed until the reader asks (22 P.3, 22 P.4, 22C1 clarification), so
 *  a test that wants the visible options waits for this and opens with Down. */
function statusShown(main: HTMLElement, text: string) {
  return waitFor(() =>
    expect(
      statusLines(main).some((p) => (p.textContent ?? '').includes(text)),
    ).toBe(true),
  )
}

/** The folder reads the bff was asked for, in order. */
function folderReads(bff: ReturnType<typeof fakeBff>) {
  return bff.to('GET', FILES)
}

/** Settles `gate` on `answer`, in an `act`, so React sees the reply land. */
async function settle(
  gate: { resolve: (r: Response) => void },
  answer: Response,
) {
  await act(async () => {
    gate.resolve(answer)
    await new Promise((r) => setTimeout(r, 0))
  })
}

// ── The stylesheet reader (the way `Reflow.test.ts` reads `index.css`) ──────
// The 22C1 review D1 is a grid placement, so its pin reads the sheet: a small
// local copy, a test file imports no other test file.

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = { sel: string; body: string; at: number; depth: number }

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: (m[1] ?? '').trim(),
  body: m[2] ?? '',
  at: m.index ?? 0,
  depth: opened(m.index ?? 0),
}))

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

// The offset of the brace that closes the block opened at `open`.
const rangeEnd = (open: number): number => {
  const start = CSS.indexOf('{', open)
  if (start < 0) throw new Error(`no block opens at ${open}`)
  let depth = 0
  for (let i = start; i < CSS.length; i++) {
    if (CSS[i] === '{') depth++
    if (CSS[i] === '}' && --depth === 0) return i
  }
  throw new Error(`the block at ${open} is never closed`)
}

// [open, end] of every block whose header is this at-rule, in file order.
const atRanges = (header: string): Array<[number, number]> => {
  const out: Array<[number, number]> = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf(header, from)
    if (at < 0) return out
    out.push([at, rangeEnd(at)])
    from = out[out.length - 1]![1] + 1
  }
}

/** The 720 px block that makes `.field` the 200px + 1fr grid (the one the
 *  19 LS1 review D1 and 21 TP1 review D1 pins sit in). */
function fieldGrid(): [number, number] {
  const blocks = atRanges('@media (min-width: 720px)').filter(([open, end]) =>
    RULES.some(
      (r) =>
        r.depth > 0 && r.at > open && r.at < end && norm(r.sel) === '.field',
    ),
  )
  expect(
    blocks.length,
    'exactly one 720 px block styles .field, the two-column grid (the 19 LS1 review D1 pins it): a placement rule belongs where the grid is',
  ).toBe(1)
  return blocks[0]!
}

/** The compounds of one selector part: split on whitespace and on `>`/`+`/`~`.
 *  A `>` is read as a descendant here, which is looser than the sheet, and the
 *  pin reads a value, not a rule's exact reach. */
const compoundsOf = (part: string): string[] =>
  part
    .replace(/\s*([>+~])\s*/g, ' ')
    .trim()
    .split(/\s+/)
    .filter(Boolean)

/** The tokens of one compound: `.class`, `#id`, `tag`. A `*` gives no token, so
 *  a rule written that way is never the pin's winner (no grid rule is). */
const tokensOf = (c: string): string[] =>
  c.match(/[.#][-a-zA-Z0-9_]+|[-a-zA-Z0-9_]+/g) ?? []

function matches(el: HTMLElement, compound: string): boolean {
  const ts = tokensOf(compound)
  if (ts.length === 0) return false
  return ts.every((t) =>
    t.startsWith('.')
      ? el.classList.contains(t.slice(1))
      : t.startsWith('#')
        ? el.id === t.slice(1)
        : el.tagName.toLowerCase() === t.toLowerCase(),
  )
}

/** The rules reaching `el`: a selector part whose last compound is the element
 *  and whose other compounds are classes of its ANCESTORS, read off the page,
 *  so the pin follows the class the coder names (the 17f2 reader). */
function rulesReaching(el: HTMLElement): Rule[] {
  return RULES.filter((r) =>
    r.sel.split(',').some((part) => {
      const cs = compoundsOf(part)
      if (cs.length === 0 || !matches(el, cs[cs.length - 1]!)) return false
      return cs.slice(0, -1).every((c) => {
        for (let p = el.parentElement; p; p = p.parentElement)
          if (matches(p, c)) return true
        return false
      })
    }),
  )
}

/** The rule the cascade applies for `grid-column` on `el`: the most specific
 *  matching part (class and id tokens, the way `ListColumns.test.tsx` scores
 *  it), the later on a tie, with the value it declares and where it sits. */
function placed(el: HTMLElement): {
  value: string
  sel: string
  at: number
  depth: number
} | null {
  const hits: Array<{
    value: string
    sel: string
    at: number
    depth: number
    spec: number
  }> = []
  for (const r of RULES) {
    const value = norm(prop(r.body, 'grid-column'))
    if (value === '') continue
    for (const part of r.sel.split(',')) {
      const cs = compoundsOf(part)
      if (cs.length === 0 || !matches(el, cs[cs.length - 1]!)) continue
      const anc = cs.slice(0, -1).every((c) => {
        for (let p = el.parentElement; p; p = p.parentElement)
          if (matches(p, c)) return true
        return false
      })
      if (!anc) continue
      hits.push({
        value,
        sel: norm(part),
        at: r.at,
        depth: r.depth,
        spec: (part.match(/[.#]/g) ?? []).length,
      })
      break
    }
  }
  hits.sort((a, b) => b.spec - a.spec || b.at - a.at)
  const win = hits[0]
  return win === undefined
    ? null
    : { value: win.value, sel: win.sel, at: win.at, depth: win.depth }
}

// ── 22 P.3: the Model file field ────────────────────────────────────────

test('22 P.3: the Model file field is labelled model-profile-model-file with its hint behind the About toggle, and its input is a native text combobox with aria-autocomplete list, aria-expanded false and no active option at first, aria-controls the listbox in the flow under it inside the form, autocomplete off and spellcheck false', async () => {
  const { main } = await loaded()
  const form = await openForm(main)
  const box = fileBox(form)
  // The field: label, hint behind About (fe/DESIGN.md 5.2).
  const field = box.closest('.field')
  expect(field, 'the input is in its field').not.toBeNull()
  expect(
    within(field as HTMLElement).getByText(en('model-profile-model-file')),
    'the field is labelled by the key',
  ).not.toBeNull()
  const hint = within(field as HTMLElement).getByText(en('model-file-hint'))
  expect(isHidden(hint), 'the hint starts closed').toBe(true)
  press(
    within(field as HTMLElement).getByRole('button', {
      name: `${en('field-about')} ${en('model-profile-model-file')}`,
    }),
  )
  expect(isHidden(hint), 'About shows the hint').toBe(false)
  // The combobox, as the tag suggestions' one (22 P.3a).
  expect(box.tagName, 'a native input').toBe('INPUT')
  expect(box.getAttribute('type') ?? 'text', 'a text input').toBe('text')
  expect(box.getAttribute('autocomplete'), 'no browser fill').toBe('off')
  expect(box.getAttribute('spellcheck'), 'no spellcheck').toBe('false')
  expect(box.getAttribute('aria-autocomplete'), 'a list').toBe('list')
  expect(box.getAttribute('aria-expanded'), 'closed at first').toBe('false')
  expect(
    box.getAttribute('aria-activedescendant'),
    '22 P.3: no option is active at first',
  ).toBeNull()
  const list = listbox(box)
  expect(list.getAttribute('role'), 'the list is a listbox').toBe('listbox')
  expect(isHidden(list), 'closed at first').toBe(true)
  expect(before(box, list), 'the list is under the input').toBe(true)
  expect(
    list.closest('form'),
    '22 P.3: the list is in the flow, inside the form, never floating',
  ).toBe(form)
})

test('22 P.3: Down opens the list and moves aria-activedescendant picking nothing; Enter picks the active option, writing its full path into the input and closing the list; the pick is the option with aria-selected true; Escape closes the list and keeps the text; a click picks the option it lands on; an edit after a pick drops the pick', async () => {
  const { main } = await loaded()
  const form = await openForm(main)
  const box = fileBox(form)
  // The rows land with the list closed (22 P.4, 22C1 clarification): the
  // count line says the reply is in, Down opens.
  await statusShown(main, en('model-files-count', `${MAIN.length}`))
  // Down opens, and moves the active option: nothing is picked.
  key(box, 'ArrowDown')
  expect(box.getAttribute('aria-expanded'), 'Down opens').toBe('true')
  expect(isHidden(listbox(box)), 'the list shows').toBe(false)
  const first = options(box)[0]!
  expect(
    box.getAttribute('aria-activedescendant'),
    'the first option is active',
  ).toBe(first.getAttribute('id'))
  expect(box.value, '22 P.3: no option is picked automatically').toBe('')
  expect(
    options(box).filter((o) => o.getAttribute('aria-selected') === 'true'),
    'nothing picked, so nothing selected',
  ).toHaveLength(0)
  key(box, 'ArrowDown')
  const second = options(box)[1]!
  expect(
    box.getAttribute('aria-activedescendant'),
    'Down again moves the active option',
  ).toBe(second.getAttribute('id'))
  expect(box.value, 'still nothing picked').toBe('')
  // Enter picks it: the full path goes into the input, the list closes.
  key(box, 'Enter')
  expect(box.value, "the pick writes the option's full path").toBe(
    MAIN[1]!.path,
  )
  expect(box.getAttribute('aria-expanded'), 'the list closed').toBe('false')
  expect(isHidden(listbox(box)), 'the list is closed').toBe(true)
  // The pick is the selected option, and stays so when the list opens again.
  key(box, 'ArrowDown')
  expect(
    options(box).filter((o) => o.getAttribute('aria-selected') === 'true'),
    '22 P.5: the picked option has aria-selected true',
  ).toEqual([second])
  // Escape closes the list and keeps the text.
  key(box, 'Escape')
  expect(box.getAttribute('aria-expanded'), 'Escape closed the list').toBe(
    'false',
  )
  expect(box.value, 'Escape keeps the text').toBe(MAIN[1]!.path)
  // A click picks the option it lands on.
  key(box, 'ArrowDown')
  const third = options(box)[2]!
  press(third)
  expect(box.value, 'a click picks').toBe(MAIN[2]!.path)
  expect(box.getAttribute('aria-expanded'), 'and closes the list').toBe('false')
  // Any edit of the text after a pick drops the pick.
  type(box, `${MAIN[2]!.path}x`)
  key(box, 'ArrowDown')
  expect(
    options(box).filter((o) => o.getAttribute('aria-selected') === 'true'),
    '22 P.3: an edit drops the pick',
  ).toHaveLength(0)
})

// ── 22 P.5a: the options ────────────────────────────────────────────────

test('22 P.5a: the options are the rows whose file name holds none of mmproj, imatrix, mtp-, eagle3-, dflash- and dspark-, in the reply order, each reading the path relative to dir, ", ", the size, and ", " and model-file-parts for a split model', async () => {
  const { main } = await loaded()
  const form = await openForm(main)
  const box = fileBox(form)
  await statusShown(main, en('model-files-count', `${MAIN.length}`))
  key(box, 'ArrowDown')
  expect(
    optionTexts(box),
    '22 P.5a: the main models of the folder, in the reply order',
  ).toEqual(MAIN.map(optionText))
  // The split model reads its parts; a single file reads none.
  const split = MODEL_FILES.find((r) => r.shards > 1)!
  expect(optionTexts(box), 'a split model reads its shard count').toContain(
    `deepseek/DeepSeek-R1-Q4_K_M.gguf, ${size(split.size)}, ${en(
      'model-file-parts',
      `${split.shards}`,
    )}`,
  )
  expect(
    optionTexts(box).filter((t) => t.includes(en('model-file-parts', '4'))),
    'only the split model says parts',
  ).toHaveLength(1)
  // The sizes, as the brief's rule writes them.
  expect(
    optionTexts(box).filter((t) => t.startsWith('qwen/Qwen3-27B-Q8_0')),
    'bytes / 2^30 with one decimal',
  ).toEqual(['qwen/Qwen3-27B-Q8_0.gguf, 16.2 GiB'])
  expect(
    optionTexts(box).filter((t) => t.startsWith('tiny/')),
    'below 0.1 GiB: bytes / 2^20 with no decimal',
  ).toEqual(['tiny/tiny-openai-f16.gguf, 50 MiB'])
})

test('22 P.5a: a projector, an imatrix, a MTP, an eagle3, a dflash and a dspark file are never options, not even when typed; the typed text filters the relative path with letter case aside, in the reply order', async () => {
  const { main } = await loaded()
  const form = await openForm(main)
  const box = fileBox(form)
  // The reply lands closed (22C1 clarification); every edit below opens the
  // list and filters (22 P.3), so typing is what reveals the options.
  await statusShown(main, en('model-files-count', `${MAIN.length}`))
  // None of the six words, in any case, is ever offered.
  for (const word of [
    'mmproj',
    'imatrix',
    'mtp-',
    'eagle3-',
    'dflash-',
    'dspark-',
    'MMProJ',
    'EAGLE3',
  ]) {
    type(box, word)
    expect(
      options(box),
      `22 P.5a: nothing named ${word} is offered`,
    ).toHaveLength(0)
  }
  // The typed text filters the relative path, letter case aside.
  type(box, 'QWEN')
  expect(optionTexts(box), 'the case does not matter').toEqual(
    MAIN.filter((r) => r.path.toLowerCase().includes('qwen')).map(optionText),
  )
  type(box, 'qwen/Qwen3-27B-Q8_0')
  expect(optionTexts(box), 'a folder and a name narrow to one').toEqual([
    optionText(MAIN.find((r) => r.path.endsWith('qwen/Qwen3-27B-Q8_0.gguf'))!),
  ])
  type(box, '')
  await waitFor(() => expect(options(box).length).toBe(MAIN.length))
  expect(
    optionTexts(box),
    'an empty text shows them all, in the reply order',
  ).toEqual(MAIN.map(optionText))
})

test('22 P.5a: at most 50 options show, the state line reads model-files-more with 50 and the matching count, and a filter that fits counts on model-files-count', async () => {
  const files = manyFiles(60)
  const { main } = await loaded({ modelFiles: files })
  const form = await openForm(main)
  const box = fileBox(form)
  await statusShown(main, en('model-files-more', '50', '60'))
  key(box, 'ArrowDown')
  expect(options(box).length, 'the list caps at 50 open options').toBe(50)
  expect(
    optionTexts(box),
    '22 P.5: at most 50, the first of the reply',
  ).toEqual(files.slice(0, 50).map(optionText))
  expect(
    statusText(main, en('model-files-more', '50', '60')),
    '22 P.6: the line counts what is not shown',
  ).toBe(en('model-files-more', '50', '60'))
  // model-00 to model-09 match "model-0": 10, so the plain count.
  type(box, 'model-0')
  expect(options(box), '10 options').toHaveLength(10)
  expect(
    statusText(main, en('model-files-count', '10')),
    'a filter that fits counts on model-files-count',
  ).toBe(en('model-files-count', '10'))
})

// ── 22 P.6, P.6a: the folder's state line ───────────────────────────────

test('22 P.6: the line reads model-files-loading while the folder is read, the text filters only once the reply is in, then model-files-count with the matching rows; a text matching nothing reads model-files-no-match', async () => {
  const gate = deferred()
  const { main } = await loaded({ modelFiles: () => gate.promise })
  const form = await openForm(main)
  const box = fileBox(form)
  expect(statusText(main, en('model-files-loading'))).toBe(
    en('model-files-loading'),
  )
  // 22 P.4: the text filters only once the reply is in.
  type(box, 'qwen')
  expect(statusText(main, en('model-files-loading')), 'still reading').toBe(
    en('model-files-loading'),
  )
  expect(options(box), 'no option before the reply').toHaveLength(0)
  await settle(gate, reply(200, modelFilesBody(MAIN)))
  await waitFor(() => expect(options(box).length).toBe(2))
  expect(
    statusText(main, en('model-files-count', '2')),
    'the two rows that match',
  ).toBe(en('model-files-count', '2'))
  type(box, 'nothing-here')
  expect(options(box), 'and no option').toHaveLength(0)
  expect(statusText(main, en('model-files-no-match'))).toBe(
    en('model-files-no-match'),
  )
})

test('22 P.6: an empty folder reads model-files-empty with dir, an unreadable one reads model-files-unreadable with the manager arg, a failed read reads model-files-failed; each carries a secondary model-files-retry button that reads the folder again with the focus staying on it', async () => {
  const cases: [string, () => Response, string][] = [
    [
      'empty',
      () => reply(200, modelFilesBody([])),
      en('model-files-empty', MODELS_DIR),
    ],
    [
      'unreadable',
      () =>
        reply(
          200,
          modelFilesBody([], {
            error: { key: 'models-dir-unreadable', args: [MODELS_DIR] },
          }),
        ),
      en('model-files-unreadable', MODELS_DIR),
    ],
    [
      'failed',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('model-files-failed'),
    ],
  ]
  for (const [what, answer, text] of cases) {
    const first = deferred()
    const second = deferred()
    let reads = 0
    fakeBff({
      modelProfiles: LIST,
      modelFiles: () => (++reads === 1 ? first.promise : second.promise),
    })
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('table', {
      name: en('model-profiles-title'),
    })
    await flushTasks()
    const form = await openForm(main)
    await settle(first, answer())
    expect(statusText(main, text), `${what}: the line says why`).toBe(text)
    const retry = within(form).getByRole('button', {
      name: en('model-files-retry'),
    })
    expect(
      retry.classList.contains('secondary'),
      `${what}: a secondary button`,
    ).toBe(true)
    // The retry reads the folder again, and keeps the focus while it is open.
    press(retry)
    expect(
      document.activeElement,
      `${what}: the focus stays on the button`,
    ).toBe(retry)
    expect(
      statusText(main, en('model-files-loading')),
      `${what}: reading`,
    ).toBe(en('model-files-loading'))
    await settle(second, answer())
    expect(
      statusText(main, text),
      `${what}: the same line again, ready to retry`,
    ).toBe(text)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('22 P.6a: when the walk is truncated, model-files-truncated follows the count in the same line', async () => {
  const { main } = await loaded({
    modelFiles: () => reply(200, modelFilesBody(MAIN, { truncated: true })),
  })
  const form = await openForm(main)
  const box = fileBox(form)
  await statusShown(main, en('model-files-count', `${MAIN.length}`))
  key(box, 'ArrowDown')
  expect(options(box).length, 'the rows are there once opened').toBe(
    MAIN.length,
  )
  const count = en('model-files-count', `${MAIN.length}`)
  const line = statusWith(main, en('model-files-truncated'))
  expect(
    (line.textContent ?? '').replace(/\s+/g, ' ').trim(),
    '22 P.6a: the count, then the truncated sentence, in one line',
  ).toBe(`${count} ${en('model-files-truncated')}`)
  expect(
    statusLines(main).filter((p) =>
      (p.textContent ?? '').includes(en('model-files-truncated')),
    ).length,
    'one line carries it',
  ).toBe(1)
})

test('22 P.6: the line holds one text at a time, and a folder read the session refuses says so with the session text and a Sign in', async () => {
  const { main } = await loaded()
  // The form opening shows the file field, which reads the folder.
  await openForm(main)
  // The count line is the reply's announcement (22 P.6): the list itself
  // stays closed until asked (22C1 clarification).
  await statusShown(main, en('model-files-count', `${MAIN.length}`))
  // One text at a time: the folder's line holds the count and nothing else of
  // the states P.6 lists.
  const folder = statusWith(main, en('model-files-count', `${MAIN.length}`))
  const others = [
    en('model-files-loading'),
    en('model-files-more', '50', '60'),
    en('model-files-no-match'),
    en('model-files-empty', MODELS_DIR),
    en('model-files-unreadable', MODELS_DIR),
    en('model-files-failed'),
    en('model-files-truncated'),
  ]
  expect(
    others.filter((t) => (folder.textContent ?? '').includes(t)),
    '22 P.6: the line holds one text',
  ).toEqual([])
  // 401: the session line, with a Sign in.
  cleanup()
  vi.unstubAllGlobals()
  fakeBff({
    modelProfiles: LIST,
    modelFiles: () => reply(401, { key: 'session-required', args: [] }),
  })
  render(<App />)
  const main2 = screen.getByRole('main')
  await within(main2).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  const form2 = await openForm(main2)
  expect(statusText(main2, en('session-ended-retry'))).toBe(
    en('session-ended-retry'),
  )
  expect(
    within(main2).getAllByRole('button', { name: en('session-sign-in') })
      .length,
    '22 P.6: a Sign in beside it',
  ).toBeGreaterThan(0)
  expect(form2).toBeTruthy()
})

// ── 22 P.4: one read per showing ────────────────────────────────────────

test('22 P.4: the folder is read once when the form opens with the file choice, not again when the picker switches away and back, not by typing, and again by the retry only', async () => {
  const bff = fakeBff({
    modelProfiles: LIST,
    modelFiles: () => reply(200, modelFilesBody(MAIN)),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await quiet(bff)
  expect(folderReads(bff), 'the list page reads no folder').toHaveLength(0)
  const form = await openForm(main)
  const box = fileBox(form)
  await statusShown(main, en('model-files-count', `${MAIN.length}`))
  expect(
    folderReads(bff),
    '22 P.4: the showing reads the folder once',
  ).toHaveLength(1)
  // The picker switches to the copy and back: the field shows again, and the
  // folder is not read a second time (it stays mounted).
  press(within(form).getByRole('radio', { name: en('model-source-copy') }))
  press(within(form).getByRole('radio', { name: en('model-source-file') }))
  await quiet(bff)
  expect(folderReads(bff), '22 P.4: switching reads nothing').toHaveLength(1)
  expect(isHidden(box), 'the field shows again').toBe(false)
  // Typing filters what is already there: no read.
  type(box, 'qwen')
  await quiet(bff)
  expect(folderReads(bff), 'typing reads nothing').toHaveLength(1)
  // The retry (which P.6 gives the empty, unreadable and failed states) is the
  // only other thing that reads the folder.
  cleanup()
  vi.unstubAllGlobals()
  const bff2 = fakeBff({
    modelProfiles: LIST,
    modelFiles: () =>
      reply(
        200,
        modelFilesBody([], {
          error: { key: 'models-dir-unreadable', args: [MODELS_DIR] },
        }),
      ),
  })
  render(<App />)
  const main2 = screen.getByRole('main')
  await within(main2).findByRole('table', { name: en('model-profiles-title') })
  await quiet(bff2)
  const form2 = await openForm(main2)
  await waitFor(() =>
    expect(folderReads(bff2).length).toBeGreaterThanOrEqual(1),
  )
  expect(
    folderReads(bff2),
    '22 P.4: the unreadable showing reads it once',
  ).toHaveLength(1)
  press(within(form2).getByRole('button', { name: en('model-files-retry') }))
  await quiet(bff2)
  expect(folderReads(bff2), '22 P.4: the retry reads it').toHaveLength(2)
})

// ── The 22C1 review (the designer, 2026-11-26): D1, D2, and the reviewer's G1 ─

test("22 P.3 (the 22C1 review D1): the listbox, the folder's line and the retry are in ONE element that is a direct child of the field, after the control box, and the 720 px field rule places that element in grid-column: 2, so from 720 px the list is under the input and never in the 200 px label column", async () => {
  // The review measured the form at 1280 px: `after` is a fragment, so its
  // three parts are grid items of the `.field` with no `grid-column` of their
  // own: the listbox takes the 200 px label column under "Model file", its
  // options wrap to three lines, and the folder's line sits beside it
  // (build/ux-review/shots/22c1-form-1280-light.png). The review's fix is the
  // 19 LS1 D1 and 21 TP1 D1 one: the markup wraps `after` in one element and
  // the 720 px block places that element in the control column.
  // The folder unreadable, so the retry is one of the three parts.
  const { main } = await loaded({
    modelFiles: () =>
      reply(
        200,
        modelFilesBody([], {
          error: { key: 'models-dir-unreadable', args: [MODELS_DIR] },
        }),
      ),
  })
  const form = await openForm(main)
  const box = fileBox(form)
  const field = box.closest('.field') as HTMLElement | null
  expect(field, 'the input is in its field').not.toBeNull()
  const control = field!.querySelector('.field-control')
  expect(control, 'the field has its control box').not.toBeNull()
  const list = listbox(box)
  const line = statusWith(main, en('model-files-unreadable', MODELS_DIR))
  const retry = within(form).getByRole('button', {
    name: en('model-files-retry'),
  })
  const wrapper = list.parentElement as HTMLElement | null
  expect(wrapper, 'the listbox is in an element').not.toBeNull()
  expect(
    wrapper,
    '22C1 review D1: the list, the folder line and the retry are in ONE element, so they stack under the input',
  ).toBe(line.parentElement)
  expect(retry.parentElement, '22C1 review D1: the retry too').toBe(wrapper)
  expect(
    wrapper,
    '22C1 review D1: the wrapper is one element of its own, not the field: a fragment leaves the three as grid items of the field',
  ).not.toBe(field)
  expect(
    wrapper!.parentElement,
    '22C1 review D1: the wrapper is a direct child of the field, so the field grid places it',
  ).toBe(field)
  expect(
    wrapper,
    '22C1 review D1: the wrapper is not the control box the input is in',
  ).not.toBe(control)
  expect(
    wrapper!.contains(box),
    '22C1 review D1: the wrapper holds none of the control',
  ).toBe(false)
  expect(
    wrapper!.contains(within(field!).getByText(en('model-profile-model-file'))),
    '22C1 review D1: the label stays in the label column',
  ).toBe(false)
  expect(
    wrapper!.contains(within(field!).getByText(en('model-file-hint'))),
    '22C1 review D1: the hint keeps its own cell',
  ).toBe(false)
  expect(
    before(control as HTMLElement, wrapper!),
    '22C1 review D1: the wrapper, with the list in it, is under the input',
  ).toBe(true)
  // The sheet half, with the element read off the page, so the class the coder
  // names stays free (index.css 1576-1581 is the block the review names).
  const [open, end] = fieldGrid()
  const rule = placed(wrapper!)
  expect(
    rule,
    `22C1 review D1: a rule of the sheet places the wrapper (it carries class "${[...wrapper!.classList].join(' ')}"): nothing names it, so at 720 px it is a grid item with no column and the list falls into the 200 px label column`,
  ).not.toBeNull()
  expect(
    rule!.value,
    '22C1 review D1: grid-column: the control column, the one beside the 200px label where the field control is',
  ).toBe('2')
  expect(
    rule!.depth > 0 && rule!.at > open && rule!.at < end,
    `22C1 review D1: the placement sits in the 720 px block that makes .field the two-column grid (the winning rule is "${rule!.sel}" at ${rule!.at}, the block is ${open}..${end}), so it applies wherever the grid does, as the LS1 and TP1 D1 rules do`,
  ).toBe(true)
  expect(
    rulesReaching(wrapper!)
      .filter((r) => norm(prop(r.body, 'grid-row')) !== '')
      .map((r) => norm(r.sel)),
    '22C1 review D1: no grid-row: the list, the line and the retry come and go with the folder state, so the cells are placed by the flow, never by hand',
  ).toEqual([])
})

test('22 P.6 (the 22C1 review D2): one matching file reads model-files-count-one, "1 file in the models folder matches.", never "1 files"; two read model-files-count and none reads model-files-no-match', async () => {
  // Section 2 of the brief gives `model-files-count` one plural text, so a
  // folder with one match reads "1 files in the models folder match." The
  // review's fix is the one key of P.6: `model-files-count-one`.
  const { main } = await loaded()
  const form = await openForm(main)
  const box = fileBox(form)
  await statusShown(main, en('model-files-count', `${MAIN.length}`))
  // One row matches: the mmproj and the imatrix of that name are no files of
  // their own (22 P.5a), so this is exactly one match.
  type(box, 'Qwen3-27B-Q8_0')
  expect(options(box), 'one option').toHaveLength(1)
  expect(
    statusText(main, 'in the models folder'),
    '22C1 review D2: one file is singular, the words the review gives',
  ).toBe('1 file in the models folder matches.')
  expect(
    statusText(main, 'in the models folder'),
    '22C1 review D2: the key of the review, not the plural one filled with 1',
  ).toBe(en('model-files-count-one'))
  expect(
    statusText(main, 'in the models folder'),
    '22C1 review D2: never "1 files"',
  ).not.toContain('1 files')
  // Two, and none, keep their own texts.
  type(box, 'qwen')
  expect(options(box), 'two options').toHaveLength(2)
  expect(
    statusText(main, 'in the models folder'),
    '22 P.6: two read the plural key',
  ).toBe(en('model-files-count', '2'))
  type(box, 'nothing-here')
  expect(options(box), 'no option').toHaveLength(0)
  expect(
    statusText(main, en('model-files-no-match')),
    '22 P.6: none reads the no-match key',
  ).toBe(en('model-files-no-match'))
})

test('22 P.3 (the reviewer, 22C1 G1): the focus leaving the box closes the list, keeping what was typed, and Down opens it again', async () => {
  // 22 P.3: the combobox closes on Escape, a pick, "or the focus leaving". The
  // reviewer's gap: the blur is not pinned, so a list left open under a box
  // the visitor tabbed away from is unpinned.
  const { main } = await loaded()
  const form = await openForm(main)
  const box = fileBox(form)
  await statusShown(main, en('model-files-count', `${MAIN.length}`))
  key(box, 'ArrowDown')
  expect(box.getAttribute('aria-expanded'), 'Down opens').toBe('true')
  expect(isHidden(listbox(box)), 'the list shows').toBe(false)
  type(box, 'qwen')
  expect(box.getAttribute('aria-expanded'), 'typing keeps it open').toBe('true')
  // The visitor tabs on: the box loses the focus.
  const name = within(form).getByRole('textbox', {
    name: en('model-profile-name'),
  })
  name.focus()
  fireEvent.blur(box)
  expect(
    box.getAttribute('aria-expanded'),
    '22C1 G1: a box the visitor left has no list open under it',
  ).toBe('false')
  expect(isHidden(listbox(box)), '22C1 G1: and the list is closed').toBe(true)
  expect(box.value, '22 P.3: the text stays').toBe('qwen')
  // Down opens it again, on the same filter.
  key(box, 'ArrowDown')
  expect(box.getAttribute('aria-expanded'), 'Down opens it again').toBe('true')
  expect(
    optionTexts(box),
    '22C1 G1: reopened on the text that is left, the same rows as before the blur',
  ).toEqual(
    MAIN.filter((r) => r.path.toLowerCase().includes('qwen')).map(optionText),
  )
})
