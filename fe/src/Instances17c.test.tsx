// Brief 16, slice 17c (fe/briefs/16-server-profiles-and-instances.md, "17c:
// create and remove an instance"), as the "17c check (2026-11-25)" of the same
// file amends it. The check wins where the two differ, so this file pins the
// check:
//  - 17c.1a: the opener is the list's `+` (`.ot-add`, the `plus` glyph, a
//    `span.visually-hidden` naming it `instances-new`, `aria-expanded`) after
//    the `ul.tiles`, inside the instances `section`; the form opens below it,
//    named by an `h3` `instances-new-title`, and has NO display name field:
//    the new instance shows as the auto name until the page's pencil sets one.
//  - 17c.4a: the create lands on `#/instances/<id>`; the nav and the tile show
//    the instance as `InstanceLink` does (its name), not `<id> · <profile>`;
//    the `h1` that takes the focus is the auto name, the pencil after it.
//  - 17c.6a: Remove is the text button `instance-remove` at the END of the run
//    buttons' row of the page's head, `--space-5` before it, drawn with
//    `--danger`; only when the list item's `removable` is `true`; switched off
//    unless the state is `inactive` or `failed`, with `instance-remove-stop-first`
//    as a line under the row naming its description; it follows the polled state.
//  - 17c.7a: the Confirm in place group opens on its own line under the head's
//    row; after the 204 everything after the `h1` goes, the pencil and the run
//    row included, and the focused `instance-removed` paragraph and the
//    `not-found-home` link follow it.
//  - 17c.10a: the Tab order of the Overview and of the instance page.
//  - 17c.11a (the select at 320 px) is the UX check's: fe/ux/check.js.
//
// The file ends with the "17c review (designer, 2026-11-26)" of the same file:
// its three defects (D1, D2, D3), the pool read's Sign in (R2) and the adviser
// notes A1, A2 and A4, each pinned as the review states the fix. A1 is pinned
// as the "17c re-check" amends it (its D4): the room above the create form's
// heading, scoped to that heading alone, and no bare h2/h3 margin-top rule.
//
// 17c.9 (a Start answered 409 `instance-no-model-profiles`) is already built
// (23 A1, ffa71ac) and pinned in OverviewRun.test.tsx and InstancePage.test.tsx:
// the slice keeps that test, it is not rewritten here.
//
// The ID field's hint is `instance-new-id-hint`: 17c.1 names it
// `instance-name-hint`, a key 17f2.10 took for the display name's hint, so the
// create form's hint needs a key of its own.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type Call,
  type Instance,
  type ServerProfile,
} from './test/bff.ts'
import { instanceHref } from './route.ts'
import strings from './strings/en_US.json'

// The plus of Icon.tsx: the horizontal stroke AND the vertical one (22 D2.2
// gives the minus the horizontal alone).
const PLUS = ['M5 12h14', 'M12 5v14']

// ── Fixtures ──────────────────────────────────────────────────────────────

/** The instance the list answers: running, and not removable (the id the
 *  removal refuses, as the bff answers for `default`). */
const RUNNING: Instance = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 8080,
  serverProfile: 'default',
  removable: false,
}

/** The instance 17c creates and removes: stopped, and removable. */
const STOPPED: Instance = {
  id: 'lab',
  engine: 'llama.cpp',
  state: 'inactive',
  port: 9090,
  serverProfile: 'lab',
  removable: true,
}

/** The list of both. */
const BOTH: Instance[] = [RUNNING, STOPPED]

/** One row of GET /api/profiles/server. */
function profile(
  id: string,
  port: number,
  instance: string | null = null,
): ServerProfile {
  return { id, engine: 'llama.cpp', port, tags: [], instance }
}

/** The pool the create form reads: two free profiles. */
const POOL: ServerProfile[] = [profile('default', 8080), profile('lab', 9090)]

/** The auto name of `STOPPED`: the engine and the port (17c.4a). */
const LAB_NAME = en('instance-name-auto', 'llama.cpp', '9090')

// The sign-in Dialog of the create form (17c.2): jsdom has no showModal.
let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  dialogs = stubDialog()
  startAt('#/')
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The text of every element named by `el`'s `aria-describedby`. */
function described(el: HTMLElement): string[] {
  return (
    el
      .getAttribute('aria-describedby')
      ?.split(/\s+/)
      .filter(Boolean)
      .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`) ??
    []
  )
}

/** The calls of a method the fake saw. */
function sent(bff: { calls: Call[] }, method: string): Call[] {
  return bff.calls.filter((c) => c.method === method)
}

/** The Overview, with its instances drawn. */
async function overview(
  options: BffOptions = {},
  list: Instance[] = BOTH,
): Promise<{ bff: ReturnType<typeof fakeBff>; main: HTMLElement }> {
  startAt('#/')
  const bff = fakeBff({ serverProfiles: POOL, ...options, instances: list })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('list')
  return { bff, main }
}

/** The `section` holding the instances (the tiles, or the empty message). */
function instancesSection(anchor: Element): HTMLElement {
  const section = anchor.closest('section')
  expect(section, 'the instances are inside a section').not.toBeNull()
  return section as HTMLElement
}

/** The list's `+`. */
function addButton(main: HTMLElement): HTMLElement {
  const all = [...main.querySelectorAll<HTMLElement>('.ot-add')]
  expect(all, 'the instances end with one .ot-add').toHaveLength(1)
  return all[0]!
}

/** The create form, once the `+` is pressed. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(addButton(main))
  return within(main).findByRole('form', { name: en('instances-new-title') })
}

function idBox(form: HTMLElement): HTMLInputElement {
  return within(form).getByRole('textbox', {
    name: en('instance-name'),
  }) as HTMLInputElement
}

function serverSelect(form: HTMLElement): HTMLSelectElement {
  return within(form).getByRole('combobox', {
    name: en('instance-new-server'),
  }) as HTMLSelectElement
}

function buttonIn(form: HTMLElement, key: string): HTMLElement {
  return within(form).getByRole('button', { name: en(key) })
}

/** The form's status line. */
function formStatus(form: HTMLElement): HTMLElement {
  const all = [...form.querySelectorAll<HTMLElement>('[role="status"]')]
  expect(all, 'the form has one status line').toHaveLength(1)
  return all[0]!
}

/** The text of every `role="alert"` inside `root`. */
function alertTexts(root: HTMLElement): string[] {
  return [...root.querySelectorAll('[role="alert"]')].map(
    (a) => a.textContent ?? '',
  )
}

/** Fills the form and presses Create: the ID typed, the profile chosen. */
function fill(form: HTMLElement, id: string, server: string) {
  type(idBox(form), id)
  fireEvent.change(serverSelect(form), { target: { value: server } })
  press(buttonIn(form, 'create-button'))
}

/**
 * The instance page of `STOPPED` at `#/instances/lab`, with its run buttons
 * shown. `list` is what GET /api/instances answers.
 */
async function instancePage(options: BffOptions = {}, list: Instance[] = BOTH) {
  startAt(instanceHref(STOPPED.id))
  const bff = fakeBff({ serverProfiles: POOL, ...options, instances: list })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('button', { name: en('run-stop') })
  return { bff, main }
}

function removeButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('instance-remove'),
  }) as HTMLButtonElement
}

/** Opens the confirm and returns its group. */
async function openConfirm(main: HTMLElement): Promise<HTMLElement> {
  press(removeButton(main))
  return within(main).findByRole('group', {
    name: en('instance-remove-question', STOPPED.id),
  })
}

function confirmButton(main: HTMLElement, id: string): HTMLElement {
  const el = main.querySelector<HTMLElement>(`#${id}`)
  expect(el, `the group has #${id}`).not.toBeNull()
  return el!
}

/** The run buttons' row of the page's head. */
function runRow(main: HTMLElement): HTMLElement {
  const row = main.querySelector('.run-actions')
  expect(row, 'the head has the run buttons row').not.toBeNull()
  return row as HTMLElement
}

/** Signs in as `user` in the open sign-in Dialog, and waits for it to close. */
async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), user)
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await waitFor(() => {
    if (screen.queryByRole('dialog') === null) return
    throw new Error('the dialog is still open')
  })
}

/** The server profiles pool page with its create form open: the form the 17c
 *  review's A1 pairs with the instances' one. */
async function poolCreateForm(): Promise<HTMLElement> {
  startAt('#/profiles/server')
  fakeBff({ serverProfiles: POOL, instances: [] })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table')
  press(within(main).getByRole('button', { name: en('server-profiles-new') }))
  return within(main).findByRole('form', {
    name: en('server-profiles-new-title'),
  })
}

// The stylesheet, read the way `Reflow.test.ts` reads it.
const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  compact: string
  body: string
  depth: number
  at: number
}

const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const ALL: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim(),
  compact: m[1].replace(/\s+/g, ''),
  body: m[2],
  depth: opened(m.index ?? 0),
  at: m.index ?? 0,
}))

// Top-level rules only: the order of the rules inside an @media block says
// nothing about the cascade of the top-level ones.
const TOP: Rule[] = ALL.filter((r) => r.depth === 0)

const tokens = (part: string): string[] =>
  part.match(/\.[-a-z0-9_]+|#[-a-z0-9_]+|[-a-z0-9_]+/g) ?? []

/** The offset of the brace that closes the block opened at `open`. */
function rangeEnd(open: number): number {
  const start = CSS.indexOf('{', open)
  if (start < 0) throw new Error(`no block opens at ${open}`)
  let depth = 0
  for (let i = start; i < CSS.length; i++) {
    if (CSS[i] === '{') depth++
    if (CSS[i] === '}' && --depth === 0) return i
  }
  throw new Error(`the block at ${open} is never closed`)
}

/** `[open, close]` of every block whose header is this at-rule, in file order. */
function atRanges(header: string): Array<[number, number]> {
  const out: Array<[number, number]> = []
  let from = 0
  for (;;) {
    const at = CSS.indexOf(header, from)
    if (at < 0) return out
    const end = rangeEnd(at)
    out.push([at, end])
    from = end + 1
  }
}

/** The rules of the sheet's `@media (max-width: 479.98px)` block: the narrow
 *  width the 17c review's A2 speaks of, the breakpoint the sheet already uses
 *  (index.css 648). */
const NARROW: Rule[] = (() => {
  const ranges = atRanges('@media (max-width: 479.98px)')
  return ALL.filter(
    (r) => r.depth === 1 && ranges.some(([a, b]) => r.at >= a && r.at <= b),
  )
})()

/**
 * The rules of `rules` that style `el`, each with the specificity of the
 * selector part that reaches it: a part whose last compound is the element's
 * tag or one of its classes, whose other compounds are classes of its
 * ancestors (the reader `InstanceNameEdit17f2.test.tsx` uses, so a rule scoped
 * to a parent counts). The classes are read off the page, so the pin follows
 * the names the coder chose.
 */
function styledBy(
  el: HTMLElement,
  rules: Rule[],
): Array<{ rule: Rule; spec: number }> {
  const own = new Set([
    el.tagName.toLowerCase(),
    ...[...el.classList].map((c) => `.${c}`),
  ])
  const anc = new Set<string>()
  for (let p = el.parentElement; p; p = p.parentElement)
    for (const c of p.classList) anc.add(`.${c}`)
  const out: Array<{ rule: Rule; spec: number }> = []
  for (const rule of rules) {
    let spec = -1
    for (const part of rule.compact.split(',')) {
      const ts = tokens(part)
      if (ts.length === 0) continue
      const last = ts[ts.length - 1]!
      if (!own.has(last) || !ts.slice(0, -1).every((t) => anc.has(t))) continue
      spec = Math.max(
        spec,
        ts.reduce(
          (n, t) => n + (t.startsWith('#') ? 100 : t.startsWith('.') ? 1 : 0),
          0,
        ),
      )
    }
    if (spec >= 0) out.push({ rule, spec })
  }
  return out
}

/** The value of `name` in a rule body, the last one when the block repeats it. */
function decl(body: string, name: string): string | undefined {
  let val: string | undefined
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name)
      val = d
        .slice(i + 1)
        .replace(/\s+/g, ' ')
        .trim()
  }
  return val
}

/**
 * The declarations that reach `el` from `rules`, in the order the cascade
 * applies them: by specificity, then by the place in the sheet, the later
 * winning. A property no reaching rule declares is absent from the answer.
 */
function reaches(el: HTMLElement, rules: Rule[]): Record<string, string> {
  const hits = styledBy(el, rules).sort(
    (a, b) => a.spec - b.spec || a.rule.at - b.rule.at,
  )
  const out: Record<string, string> = {}
  for (const { rule } of hits)
    for (const d of rule.body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) out[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
  return out
}

/**
 * The selector parts of a rule, whitespace normalised. `compact` joins a
 * descendant selector into one word (`form h3` to `formh3`), so a reader that
 * names a TAG ancestor reads `sel` here.
 */
function partsOf(sel: string): string[] {
  return sel
    .split(',')
    .map((p) => p.replace(/\s+/g, ' ').trim())
    .filter((p) => p.length > 0)
}

/**
 * A selector part's compounds, each reduced to its tag and its classes:
 * pseudo-classes, attribute brackets and `:not(...)` are dropped, so
 * `form > h3:first-child` reads as `form` then `h3`. A compound that is neither
 * (`*`, `:root`, `[hidden]`, `@font-face`) answers `[]`, and `scopedBy` leaves
 * such a part out: a rule the reader cannot place never counts as giving room.
 */
function compounds(part: string): string[][] {
  return part
    .split(/[\s>+~]+/)
    .filter((c) => c.length > 0)
    .map((c) => {
      const bare = c.split('(')[0]!.split(':')[0]!
      const out: string[] = []
      const tag = bare.match(/^[a-zA-Z][a-zA-Z0-9-]*/)
      if (tag) out.push(tag[0].toLowerCase())
      for (const m of bare.matchAll(/\.[a-zA-Z0-9_-]+/g))
        out.push(m[0].toLowerCase())
      return out
    })
}

/** A part's specificity as a number: ids 100, classes and pseudo-classes 10,
 *  tags 1. Enough to order the rules that reach one heading. */
function specificity(part: string): number {
  const ids = (part.match(/#[a-zA-Z0-9_-]+/g) ?? []).length
  const cls =
    (part.match(/\.[a-zA-Z0-9_-]+/g) ?? []).length +
    (part.match(/:[a-zA-Z-]+/g) ?? []).length
  const tags = (part.match(/(^|[\s>+~])[a-z][a-z0-9-]*/g) ?? []).length
  return ids * 100 + cls * 10 + tags
}

/**
 * The rules of `rules` that style `el`, each with the specificity and the text
 * of the part that reaches it: a part whose last compound is the element (its
 * tag, and every class of that compound the element carries) and whose earlier
 * compounds are tags OR classes of its ancestors, read off the DOM. `styledBy`
 * counts ancestor classes only, so it never sees the rule the 17c re-check's D4
 * names as the fix, `form > h3:first-child`: the reader here does.
 */
function scopedBy(
  el: HTMLElement,
  rules: Rule[],
): Array<{ rule: Rule; spec: number; part: string }> {
  const anc = new Set<string>()
  for (let p = el.parentElement; p; p = p.parentElement) {
    anc.add(p.tagName.toLowerCase())
    for (const c of p.classList) anc.add(`.${c.toLowerCase()}`)
  }
  const own = new Set([
    el.tagName.toLowerCase(),
    ...[...el.classList].map((c) => `.${c.toLowerCase()}`),
  ])
  const out: Array<{ rule: Rule; spec: number; part: string }> = []
  for (const rule of rules)
    for (const part of partsOf(rule.sel)) {
      const cs = compounds(part)
      if (cs.some((c) => c.length === 0)) continue
      if (!cs[cs.length - 1]!.every((t) => own.has(t))) continue
      if (!cs.slice(0, -1).every((c) => c.every((t) => anc.has(t)))) continue
      out.push({ rule, spec: specificity(part), part })
    }
  return out
}

/**
 * The margin-top the cascade gives `el` from `rules`: the last declaration that
 * reaches it, by specificity then sheet order, a `margin` shorthand standing
 * for its top side (the sheet's heading rules write the shorthand).
 */
function marginTop(el: HTMLElement, rules: Rule[]): string | undefined {
  const hits = scopedBy(el, rules).sort(
    (a, b) => a.spec - b.spec || a.rule.at - b.rule.at,
  )
  let val: string | undefined
  for (const { rule } of hits)
    for (const d of rule.body.split(';')) {
      const i = d.indexOf(':')
      if (i < 0) continue
      const name = d.slice(0, i).trim()
      const v = d
        .slice(i + 1)
        .replace(/\s+/g, ' ')
        .trim()
      if (name === 'margin-top') val = v
      if (name === 'margin') val = v.split(' ')[0]
    }
  return val
}

/** A selector part that is one bare heading tag: `h2` or `h3`, with no class,
 *  no id, no pseudo-class and no ancestor compound. */
const isBareHeading = (part: string): boolean => {
  const cs = compounds(part)
  return (
    cs.length === 1 &&
    cs[0]!.length === 1 &&
    (cs[0]![0] === 'h2' || cs[0]![0] === 'h3')
  )
}

/**
 * The top-level rules that declare a `margin-top` on a bare `h2` or `h3`. The
 * sheet's own heading rules are NOT among them (they declare the `margin`
 * shorthand), so the answer is empty once the unscoped room is gone: it is the
 * list the 17c re-check's D4 asks to hold empty.
 */
function bareHeadingRoom(): Rule[] {
  return TOP.filter(
    (r) =>
      decl(r.body, 'margin-top') !== undefined &&
      partsOf(r.sel).some(isBareHeading),
  )
}

// ── 17c.1a: the + ─────────────────────────────────────────────────────────

test('16 17c.1a: the instances end with a .ot-add after the tiles, named instances-new by a visually hidden span, carrying the plus, aria-expanded false, and no form beside them', async () => {
  const { main } = await overview()
  const tiles = await within(main).findByRole('list')
  const section = instancesSection(tiles)
  const add = addButton(main)
  expect(add.closest('section'), 'inside the instances section').toBe(section)
  expect(add.tagName, 'a button').toBe('BUTTON')
  expect(
    within(main).getByRole('button', { name: en('instances-new') }),
    'named instances-new',
  ).toBe(add)
  const spans = [...add.querySelectorAll('span.visually-hidden')].map(
    (s) => s.textContent,
  )
  expect(spans, 'its name is a visually hidden span').toEqual([
    en('instances-new'),
  ])
  expect(
    [...add.querySelectorAll('svg[aria-hidden="true"] path')].map((p) =>
      p.getAttribute('d'),
    ),
    'the plus glyph of Icon.tsx',
  ).toEqual(PLUS)
  expect(add.getAttribute('aria-expanded'), 'it opens the form').toBe('false')
  expect(isHidden(add), 'shown').toBe(false)
  expect(before(tiles, add), 'after the tiles').toBe(true)
  expect(
    main.querySelector('form'),
    'no form before the + is pressed',
  ).toBeNull()
})

test('16 17c.1a: with no instance the + follows instances-empty, inside the same section', async () => {
  startAt('#/')
  fakeBff({ serverProfiles: POOL, instances: [] })
  render(<App />)
  const main = screen.getByRole('main')
  const empty = await within(main).findByText(en('instances-empty'))
  const add = addButton(main)
  expect(add.closest('section'), 'inside the instances section').toBe(
    instancesSection(empty),
  )
  expect(before(empty, add), 'after the message').toBe(true)
  expect(main.querySelector('form'), 'no form').toBeNull()
})

test('16 17c.1a: the + opens the form on the line after it, named by its h3 instances-new-title, with the ID field and the Server profile select, their hints and the note, Create and Cancel, and no display name field', async () => {
  const { main } = await overview()
  const tiles = await within(main).findByRole('list')
  const add = addButton(main)
  const form = await openForm(main)
  expect(before(add, form), 'the form follows the +').toBe(true)
  expect(form.closest('section'), 'in the instances section').toBe(
    instancesSection(tiles),
  )
  expect(add.getAttribute('aria-expanded'), 'the + says it is open').toBe(
    'true',
  )
  const h3 = within(form).getByRole('heading', {
    level: 3,
    name: en('instances-new-title'),
  })
  expect(
    within(main).getByRole('form', { name: en('instances-new-title') }),
    'the form is named by the h3',
  ).toBe(form)
  expect(h3.tagName, 'an h3').toBe('H3')
  const id = idBox(form)
  const select = serverSelect(form)
  expect(
    form.querySelectorAll('input, select, textarea'),
    'the ID and the Server profile, and nothing else to write: the form has no display name field',
  ).toHaveLength(2)
  expect(described(id), 'the ID shows its hint').toContain(
    en('instance-new-id-hint'),
  )
  expect(described(select), 'the select shows its hint').toContain(
    en('instance-new-server-hint'),
  )
  expect(within(form).getByText(en('instance-new-note')), 'the note')
  expect(buttonIn(form, 'create-button'), 'Create')
  expect(buttonIn(form, 'create-cancel'), 'Cancel')
  expect(document.activeElement, 'the focus goes to the ID').toBe(id)
})

// ── 17c.2: the server profiles of the form ────────────────────────────────

test('16 17c.2: the select starts on instance-new-server-choose with an empty value, then one option per free profile as instance-server-option, the id alone for port 0', async () => {
  const { main } = await overview({
    serverProfiles: [...POOL, profile('zero', 0)],
  })
  const form = await openForm(main)
  const select = serverSelect(form)
  expect(
    [...select.options].map((o) =>
      (o.textContent ?? '').replace(/\s+/g, ' ').trim(),
    ),
    'the choice, then the free profiles in the reply order',
  ).toEqual([
    en('instance-new-server-choose'),
    'default (port 8080)',
    'lab (port 9090)',
    'zero',
  ])
  expect(select.selectedIndex, 'it starts on the choice').toBe(0)
  expect(select.options[0]!.value, 'the choice has no value').toBe('')
})

test('16 17c.2: opening the form sends one GET of the server profiles; while it runs the form says instance-new-loading in a role="status" in place of the select', async () => {
  const gate = deferred()
  const { bff, main } = await overview({ serverProfiles: () => gate.promise })
  const form = await openForm(main)
  expect(
    sent(bff, 'GET').filter((c) => c.path === '/api/profiles/server').length,
    'one read of the pool',
  ).toBe(1)
  expect(formStatus(form).textContent, 'the loading text').toBe(
    en('instance-new-loading'),
  )
  expect(
    within(form).queryByRole('combobox'),
    'no select while pending',
  ).toBeNull()
  gate.resolve(reply(200, [...POOL]))
  await within(form).findByRole('combobox', { name: en('instance-new-server') })
  expect(
    sent(bff, 'GET').filter((c) => c.path === '/api/profiles/server').length,
    'still one read',
  ).toBe(1)
})

test('16 17c.2: when every server profile runs an instance the form shows instance-new-no-free and the instance-new-profiles-link to the pool, and Create is switched off', async () => {
  const { main } = await overview({
    serverProfiles: POOL.map((p) => profile(p.id, p.port ?? 0, 'default')),
  })
  const form = await openForm(main)
  expect(within(form).queryByRole('combobox'), 'no select').toBeNull()
  expect(within(form).getByText(en('instance-new-no-free')), 'the message')
  expect(
    within(form)
      .getByRole('link', {
        name: en('instance-new-profiles-link'),
      })
      .getAttribute('href'),
    'to the pool',
  ).toBe('#/profiles/server')
  expect(
    buttonIn(form, 'create-button').getAttribute('aria-disabled'),
    'Create switched off',
  ).toBe('true')
})

// ── 17c.3: the client checks ──────────────────────────────────────────────

test('16 17c.3: Create with an empty ID shows field-error-required under the ID, and with the empty option under the Server profile; nothing is sent', async () => {
  const { bff, main } = await overview()
  const form = await openForm(main)
  const id = idBox(form)
  const select = serverSelect(form)
  press(buttonIn(form, 'create-button'))
  const errors = await within(form).findAllByText(en('field-error-required'))
  expect(errors, 'both fields ask').toHaveLength(2)
  expect(before(id, errors[0]!), 'the ID error is under the ID').toBe(true)
  expect(described(id), 'the ID is described by its error').toContain(
    en('field-error-required'),
  )
  expect(
    before(select, errors[1]!),
    'the select error is under the select',
  ).toBe(true)
  expect(described(select), 'the select is described by its error').toContain(
    en('field-error-required'),
  )
  expect(sent(bff, 'POST'), 'nothing sent').toHaveLength(0)
  // The ID alone is not enough.
  fireEvent.change(select, { target: { value: 'lab' } })
  press(buttonIn(form, 'create-button'))
  await within(form).findByText(en('field-error-required'))
  expect(sent(bff, 'POST'), 'still nothing sent').toHaveLength(0)
})

// ── 17c.4: the request, and 17c.4a where it lands ─────────────────────────

test('16 17c.4: Create sends POST /api/instances with the engine, the trimmed id and the server profile; while it runs the status says create-creating and both buttons are switched off', async () => {
  const gate = deferred()
  const { bff, main } = await overview({
    instanceCreate: () => gate.promise,
  })
  const form = await openForm(main)
  fill(form, '  lab  ', 'lab')
  await within(form).findByText(en('create-creating'))
  expect(formStatus(form).textContent, 'the status').toBe(en('create-creating'))
  expect(
    buttonIn(form, 'create-button').getAttribute('aria-disabled'),
    'Create switched off',
  ).toBe('true')
  expect(
    buttonIn(form, 'create-cancel').getAttribute('aria-disabled'),
    'Cancel switched off',
  ).toBe('true')
  const posts = sent(bff, 'POST').filter((c) => c.path === '/api/instances')
  expect(posts, 'one create').toHaveLength(1)
  expect(posts[0]!.body, 'the body of 17c.4').toEqual({
    engine: 'llama.cpp',
    id: 'lab',
    serverProfile: 'lab',
  })
  gate.resolve(reply(201, { id: 'lab' }))
})

test('16 17c.4a: a 201 reads the list again and lands on the new instance page, whose focused h1 is the auto name with the pencil after it, and the nav shows it by that name', async () => {
  const { bff, main } = await overview({
    instances: [RUNNING],
    instanceCreate: () => {
      bff.setInstances([RUNNING, STOPPED])
      return reply(201, { id: STOPPED.id })
    },
  })
  const form = await openForm(main)
  fill(form, 'lab', 'lab')
  await waitFor(() => {
    if (window.location.hash === instanceHref(STOPPED.id)) return
    throw new Error(`the page is at ${window.location.hash}`)
  })
  expect(
    sent(bff, 'GET').filter((c) => c.path === '/api/instances').length,
    'the list read again',
  ).toBeGreaterThan(1)
  const h1 = await screen.findByRole('heading', { level: 1, name: LAB_NAME })
  expect(document.activeElement, 'the h1 takes the focus').toBe(h1)
  expect(h1.getAttribute('tabindex'), 'so it can be focused by the page').toBe(
    '-1',
  )
  const pencil = within(main).getByRole('button', {
    name: en('instance-name-edit'),
  })
  expect(before(h1, pencil), 'the pencil is after the name').toBe(true)
  // The nav is a SIBLING of main (App.tsx 458-549), so its links are read from
  // the nav region, as the nav's own suites read them (NodesMenu1/2).
  const nav = await screen.findByRole('navigation', { name: en('nav-label') })
  expect(
    within(nav).getByRole('link', { name: LAB_NAME }).getAttribute('href'),
    'the nav shows the new instance by the name InstanceLink gives',
  ).toBe(instanceHref(STOPPED.id))
  expect(main.querySelector('form'), 'the create form is gone').toBeNull()
})

test('16 17c.4a: the Overview tile shows the new instance by the name InstanceLink gives, not id and server profile', async () => {
  startAt('#/')
  fakeBff({ serverProfiles: POOL, instances: [RUNNING, STOPPED] })
  render(<App />)
  const main = screen.getByRole('main')
  const tiles = await within(main).findByRole('list')
  const items = within(tiles).getAllByRole('listitem')
  expect(items, 'two tiles').toHaveLength(2)
  const tile = items[1]!
  const link = within(tile).getByRole('link')
  expect(link.getAttribute('href'), 'the tile links to the page').toBe(
    instanceHref(STOPPED.id),
  )
  // The visible words of the link, the reader-only id of 17f.3 left out.
  const shown = [...link.childNodes]
    .filter(
      (n) =>
        !(n instanceof HTMLElement && n.classList.contains('visually-hidden')),
    )
    .map((n) => n.textContent ?? '')
    .join('')
    .replace(/\s+/g, ' ')
    .trim()
  expect(shown, 'the tile shows the name InstanceLink gives').toBe(LAB_NAME)
  expect(
    tile.textContent,
    'the tile does not spell `<id> · <server profile>`',
  ).not.toContain('lab · lab')
})

// ── 17c.5: the create refusals ────────────────────────────────────────────

test('16 17c.5: a refusal naming field id shows under the ID, one naming field serverProfile under the select, and one with no field in the alert; the form stays open with the values typed', async () => {
  const cases: [string, () => Response, string, string][] = [
    [
      '422 instance-bad-id',
      () => reply(422, { key: 'instance-bad-id', args: ['Lab'], field: 'id' }),
      'id',
      en('api-instance-bad-id', 'Lab'),
    ],
    [
      '422 client-address-in-use',
      () =>
        reply(422, {
          key: 'client-address-in-use',
          args: ['other'],
          field: 'serverProfile',
        }),
      'serverProfile',
      en('api-client-address-in-use', 'other'),
    ],
    [
      '409 instance-exists',
      () => reply(409, { key: 'instance-exists', args: ['lab'] }),
      '',
      en('api-instance-exists', 'lab'),
    ],
  ]
  for (const [what, answer, field, text] of cases) {
    const { bff, main } = await overview({ instanceCreate: answer })
    const form = await openForm(main)
    fill(form, 'lab', 'lab')
    await within(main).findByText(text)
    const id = idBox(form)
    const select = serverSelect(form)
    if (field === 'id') {
      expect(described(id), `${what}: under the ID`).toContain(text)
      expect(alertTexts(form), `${what}: no alert`).toEqual([])
    } else if (field === 'serverProfile') {
      expect(described(select), `${what}: under the select`).toContain(text)
      expect(alertTexts(form), `${what}: no alert`).toEqual([])
    } else {
      expect(alertTexts(form), `${what}: the alert`).toEqual([text])
    }
    expect(id.value, `${what}: the id typed stays`).toBe('lab')
    expect(select.value, `${what}: the profile chosen stays`).toBe('lab')
    expect(
      sent(bff, 'POST').filter((c) => c.path === '/api/instances').length,
      `${what}: one create`,
    ).toBe(1)
    cleanup()
  }
})

test('16 17c.5: a 503 answers create-unknown in the alert and reads the instance list again', async () => {
  const { bff, main } = await overview({
    instanceCreate: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  const form = await openForm(main)
  const before = sent(bff, 'GET').filter(
    (c) => c.path === '/api/instances',
  ).length
  fill(form, 'lab', 'lab')
  await within(main).findByText(en('create-unknown'))
  expect(alertTexts(form), 'the alert').toContain(en('create-unknown'))
  await waitFor(() => {
    const now = sent(bff, 'GET').filter(
      (c) => c.path === '/api/instances',
    ).length
    if (now > before) return
    throw new Error('the list was not read again')
  })
})

// ── 17c.6a: Remove in the head ────────────────────────────────────────────

test('16 17c.6a: the run buttons row ends with the text button instance-remove, --danger and --space-5 before it, aria-expanded false', async () => {
  const { main } = await instancePage()
  const remove = removeButton(main)
  const row = runRow(main)
  expect(remove.closest('.run-actions'), 'in the run buttons row').toBe(row)
  expect(
    [...row.querySelectorAll('button')].at(-1),
    'the last of the row, after Start, Stop and Restart',
  ).toBe(remove)
  expect(
    within(main).getByRole('button', { name: en('run-restart') }),
    'Restart is before it',
  )
  expect(
    before(
      within(main).getByRole('button', { name: en('run-restart') }),
      remove,
    ),
    'Restart, then Remove',
  ).toBe(true)
  expect(remove.getAttribute('aria-expanded'), 'it opens the confirm').toBe(
    'false',
  )
  expect(
    remove.getAttribute('aria-disabled'),
    'it can act: the state is inactive',
  ).toBeNull()
  const style = reaches(remove, TOP)
  expect(style['color'], 'drawn with --danger, as the Save bar’s Delete').toBe(
    'var(--danger)',
  )
  expect(style['margin-inline-start'], 'the row puts --space-5 before it').toBe(
    'var(--space-5)',
  )
})

test('16 17c.6a: Remove is switched off while the instance runs, with instance-remove-stop-first as a line under the row naming its description, and follows the polled state', async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  startAt(instanceHref(STOPPED.id))
  const bff = fakeBff({
    serverProfiles: POOL,
    instances: [{ ...STOPPED, state: 'activating' }],
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('button', { name: en('run-stop') })
  const remove = removeButton(main)
  expect(remove.getAttribute('aria-disabled'), 'running: switched off').toBe(
    'true',
  )
  const ids = (remove.getAttribute('aria-describedby') ?? '').split(/\s+/)
  expect(ids.length, 'the button names a description').toBeGreaterThan(0)
  expect(described(remove), 'the reason is the line of 17c.6a').toContain(
    en('instance-remove-stop-first'),
  )
  const reason = ids
    .map((id) => document.getElementById(id))
    .find((el) => el !== undefined)
  expect(reason, 'the reason is in the DOM').not.toBeUndefined()
  expect(before(runRow(main), reason!), 'a line under the row').toBe(true)
  // The polled state: the list answers `inactive`, and Remove becomes active
  // with no reload.
  bff.setInstances([STOPPED])
  await act(async () => {
    await vi.advanceTimersByTimeAsync(6000)
  })
  expect(
    removeButton(main).getAttribute('aria-disabled'),
    'stopped: it can act',
  ).toBeNull()
  expect(
    described(remove),
    'the reason line is gone with the state',
  ).not.toContain(en('instance-remove-stop-first'))
  // A failed instance can be removed too.
  bff.setInstances([{ ...STOPPED, state: 'failed' }])
  await act(async () => {
    await vi.advanceTimersByTimeAsync(6000)
  })
  expect(
    removeButton(main).getAttribute('aria-disabled'),
    'failed: it can act',
  ).toBeNull()
})

test('16 17c.6a: removable false, or no removable at all, leaves no Remove on the page', async () => {
  for (const [what, instance] of [
    ['removable false', { ...STOPPED, removable: false }],
    [
      'no removable',
      { id: STOPPED.id, engine: 'llama.cpp', state: 'inactive' },
    ],
  ] as [string, Instance][]) {
    const { main } = await instancePage({}, [instance])
    expect(
      within(main).queryByRole('button', { name: en('instance-remove') }),
      `${what}: no Remove`,
    ).toBeNull()
    expect(
      main.querySelector('.run-actions')!.querySelectorAll('button').length,
      `${what}: the row keeps its three verbs`,
    ).toBe(3)
    cleanup()
  }
})

// ── 17c.7a: the confirm ───────────────────────────────────────────────────

test('16 17c.7a: Remove opens the confirm on its own line under the head row, named by instance-remove-question, with confirm-remove and confirm-cancel, the focus on Cancel, and nothing sent', async () => {
  const { bff, main } = await instancePage()
  const remove = removeButton(main)
  const group = await openConfirm(main)
  expect(remove.getAttribute('aria-expanded'), 'the + says it is open').toBe(
    'true',
  )
  expect(
    before(runRow(main), group),
    'the group is under the run buttons row',
  ).toBe(true)
  // Start at boot is a checkbox (Field.tsx 444-452, as Boot.test.tsx 121-123
  // reads it), and the field arrives with its OWN read, so it is settled with
  // an async finder.
  const boot = await within(main).findByRole('checkbox', {
    name: en('boot-toggle'),
  })
  expect(
    group.compareDocumentPosition(boot) & Node.DOCUMENT_POSITION_CONTAINED_BY,
    'the group is above Start at boot',
  ).toBe(0)
  expect(
    within(group).getByText(en('instance-remove-question', STOPPED.id)),
    'the question names the instance',
  )
  expect(confirmButton(main, 'confirm-remove').textContent, 'the verb').toBe(
    en('confirm-remove'),
  )
  expect(confirmButton(main, 'confirm-cancel').textContent, 'Cancel').toBe(
    en('confirm-cancel'),
  )
  expect(document.activeElement, 'the focus lands on Cancel').toBe(
    confirmButton(main, 'confirm-cancel'),
  )
  expect(sent(bff, 'DELETE'), 'nothing sent yet').toHaveLength(0)
  // Cancel closes it and returns the focus.
  press(confirmButton(main, 'confirm-cancel'))
  await waitFor(() => {
    if (main.querySelector('.confirm-group') === null) return
    throw new Error('the group is still open')
  })
  expect(document.activeElement, 'the focus is back on Remove').toBe(remove)
  expect(remove.getAttribute('aria-expanded'), 'closed').toBe('false')
  // Escape, from the group, does the same.
  press(remove)
  const again = await openConfirm(main)
  fireEvent.keyDown(again, { key: 'Escape' })
  await waitFor(() => {
    if (main.querySelector('.confirm-group') === null) return
    throw new Error('Escape did not close it')
  })
  expect(document.activeElement, 'the focus is back on Remove').toBe(remove)
  expect(sent(bff, 'DELETE'), 'still nothing sent').toHaveLength(0)
})

test('16 17c.7a: confirm-remove sends DELETE /api/instances/<id> saying confirm-removing, and after the 204 everything after the h1 is the focused instance-removed and the link home', async () => {
  const gate = deferred()
  const { bff, main } = await instancePage({
    instanceDelete: () => {
      bff.setInstances([RUNNING])
      return gate.promise
    },
  })
  const group = await openConfirm(main)
  press(confirmButton(main, 'confirm-remove'))
  await within(group).findByText(en('confirm-removing'))
  expect(within(group).getByRole('status').textContent, 'the status').toBe(
    en('confirm-removing'),
  )
  const deletes = sent(bff, 'DELETE').filter((c) =>
    c.path.startsWith('/api/instances/'),
  )
  expect(
    deletes.map((c) => c.path),
    'one delete of the id',
  ).toEqual(['/api/instances/lab'])
  gate.resolve(reply(204))
  const done = await within(main).findByText(en('instance-removed', STOPPED.id))
  expect(document.activeElement, 'the paragraph takes the focus').toBe(done)
  expect(done.getAttribute('tabindex'), 'so it can be focused').toBe('-1')
  expect(
    within(main)
      .getByRole('link', { name: en('not-found-home') })
      .getAttribute('href'),
    'the link home',
  ).toBe('#/')
  expect(
    within(main).queryByRole('button', { name: en('run-start') }),
    'the run row is gone',
  ).toBeNull()
  expect(
    within(main).queryByRole('button', { name: en('instance-name-edit') }),
    'the pencil is gone',
  ).toBeNull()
  expect(
    within(main).queryByText(en('instance-models-title')),
    'the sections are gone',
  ).toBeNull()
  expect(
    within(main).queryByText(en('instance-not-found', STOPPED.id)),
    'the page does not say the instance is missing',
  ).toBeNull()
  expect(
    sent(bff, 'GET').filter((c) => c.path === '/api/instances').length,
    'the list read again',
  ).toBeGreaterThan(1)
})

// ── 17c.8: the remove refusals ────────────────────────────────────────────

test('16 17c.8: a refusal answers in the group’s alert and the group stays open: instance-running, instance-protected, 404 as instance-not-found, 503', async () => {
  const cases: [string, () => Response, string][] = [
    [
      '409 instance-running',
      () => reply(409, { key: 'instance-running', args: ['lab'] }),
      en('api-instance-running', 'lab'),
    ],
    [
      '409 instance-protected',
      () => reply(409, { key: 'instance-protected', args: ['lab'] }),
      en('api-instance-protected', 'lab'),
    ],
    [
      '404 client-no-instance',
      () => reply(404, { key: 'client-no-instance', args: [] }),
      en('instance-not-found', 'lab'),
    ],
    [
      '503 client-unavailable',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
  ]
  for (const [what, answer, text] of cases) {
    const { main } = await instancePage({ instanceDelete: answer })
    const group = await openConfirm(main)
    press(confirmButton(main, 'confirm-remove'))
    await within(group).findByText(text)
    expect(alertTexts(group), `${what}: the alert is in the group`).toEqual([
      text,
    ])
    expect(
      main.querySelector('.confirm-group'),
      `${what}: the group stays open`,
    ).not.toBeNull()
    expect(
      within(main).queryByText(en('instance-removed', STOPPED.id)),
      `${what}: the page is still the page`,
    ).toBeNull()
    cleanup()
  }
})

test('16 17c.8: a 401 on the delete shows session-ended-retry and a Sign in in the group, with the focus back on confirm-remove', async () => {
  const { main } = await instancePage({
    instanceDelete: () => reply(401, { key: 'session-required', args: [] }),
  })
  const group = await openConfirm(main)
  press(confirmButton(main, 'confirm-remove'))
  await within(group).findByText(en('session-ended-retry'))
  expect(alertTexts(group), 'the alert').toEqual([en('session-ended-retry')])
  const signIn = within(group).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(
    before(within(group).getByRole('alert'), signIn),
    'the Sign in is after the alert',
  ).toBe(true)
  expect(document.activeElement, 'the focus is back on Remove').toBe(
    confirmButton(main, 'confirm-remove'),
  )
})

test('16 17c.8: a 403 session-not-administrator on the delete is the session ended: session-revoked in the group’s alert and a Sign in after it', async () => {
  const { main } = await instancePage({
    instanceDelete: () =>
      reply(403, { key: 'session-not-administrator', args: [] }),
  })
  const group = await openConfirm(main)
  press(confirmButton(main, 'confirm-remove'))
  await within(group).findByText(en('session-revoked'))
  expect(alertTexts(group), 'the alert').toEqual([en('session-revoked')])
  expect(
    within(group).getByRole('button', { name: en('session-sign-in') }),
    'a Sign in',
  )
})

// ── 17c.10a: the keyboard ─────────────────────────────────────────────────

/** The elements Tab reaches, in DOM order (no positive tabindex is used). */
function tabbable(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

/** The name Tab shows for a control: its accessible name (for a link or a
 *  button, the text of its content, the `.visually-hidden` runs included, as
 *  17f.3 and 24 A1.3 build them), or for a field its label. */
function nameOf(el: HTMLElement): string {
  if (!['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName))
    return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
  // An input has no text of its own: its name is the label that points at it.
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((id) => document.getElementById(id)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const aria = el.getAttribute('aria-label')
  if (aria) return aria
  const id = el.getAttribute('id')
  const label = id ? document.querySelector(`label[for="${id}"]`) : null
  const wrap = el.closest('label')
  return (label?.textContent ?? wrap?.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

test('16 17c.10a: on the Overview Tab goes the tiles’ links and run buttons, then the +, and while the form is open the ID, the Server profile, Create and Cancel', async () => {
  const { main } = await overview({}, [STOPPED])
  const add = addButton(main)
  const names = tabbable().map(nameOf)
  const at = names.indexOf(en('instances-new'))
  expect(at, 'the + is a stop').toBeGreaterThan(-1)
  // The names are the accessible ones: the tile's link carries the reader-only
  // id of 17f.3 (Instances.tsx 629) and each verb names the instance it acts
  // on in a visually hidden span (24 A1.3, Instances.tsx 736, pinned by
  // OverviewRun.test.tsx 456-481).
  expect(
    names.slice(at - 4, at),
    'the tile comes last, before the +: its link and its three run buttons',
  ).toEqual([
    `${LAB_NAME}${en('instance-id-said', STOPPED.id)}`,
    `${en('run-start')} ${LAB_NAME}`,
    `${en('run-stop')} ${LAB_NAME}`,
    `${en('run-restart')} ${LAB_NAME}`,
  ])
  const form = await openForm(main)
  expect(tabbable().map(nameOf).slice(at), 'the +, then the open form').toEqual(
    [
      en('instances-new'),
      en('instance-name'),
      en('instance-new-server'),
      en('create-button'),
      en('create-cancel'),
    ],
  )
  expect(tabbable().includes(add), 'the + keeps its stop while open').toBe(true)
  expect(form, 'the form is open')
})

test('16 17c.10a: on the instance page Tab goes the pencil, Start, Stop, Restart, Remove, then Start at boot and the sections, and the confirm’s two buttons when it is open', async () => {
  const { main } = await instancePage({}, [{ ...STOPPED, serverProfile: null }])
  await within(main).findByRole('checkbox', { name: en('boot-toggle') })
  await within(main).findByRole('table', { name: en('instance-models-title') })
  const names = tabbable().map(nameOf)
  const at = names.indexOf(en('instance-name-edit'))
  expect(at, 'the pencil is a stop').toBeGreaterThan(-1)
  expect(
    names.slice(at, at + 5),
    'the head: the pencil, the three verbs, Remove last',
  ).toEqual([
    en('instance-name-edit'),
    en('run-start'),
    en('run-stop'),
    en('run-restart'),
    en('instance-remove'),
  ])
  expect(names.slice(at + 5, at + 7), 'Start at boot follows the head').toEqual(
    [`${en('field-about')} ${en('boot-label')}`, en('boot-toggle')],
  )
  const group = await openConfirm(main)
  const open = tabbable().map(nameOf)
  const opened = open.indexOf(en('instance-name-edit'))
  expect(
    open.slice(opened, opened + 7),
    'the group’s two buttons are the stops right after Remove, and the boot field follows them',
  ).toEqual([
    en('instance-name-edit'),
    en('run-start'),
    en('run-stop'),
    en('run-restart'),
    en('instance-remove'),
    en('confirm-remove'),
    en('confirm-cancel'),
  ])
  expect(document.activeElement, 'the focus lands on Cancel').toBe(
    confirmButton(main, 'confirm-cancel'),
  )
  expect(group, 'the group is open')
})

// ── 17c.12: strings ───────────────────────────────────────────────────────

/** 17c.12: the keys the strings file gains, exactly 17c's of section 2.
 *  `api-instance-no-model-profiles` is not among them: 17c.9 is built, and the
 *  sheet carries that key already. `instance-new-id-hint` stands in for the
 *  brief's `instance-name-hint`, a key 17f2.10 took for the display name. */
const GAINS = [
  'instances-new',
  'instances-new-title',
  'instance-name',
  'instance-new-id-hint',
  'instance-new-server',
  'instance-new-server-hint',
  'instance-new-server-choose',
  'instance-server-option',
  'instance-new-loading',
  'instance-new-no-free',
  'instance-new-profiles-link',
  'instance-new-note',
  'instance-remove',
  'instance-remove-stop-first',
  'instance-remove-question',
  'confirm-remove',
  'confirm-removing',
  'instance-removed',
  'api-instance-bad-id',
  'api-instance-exists',
  'api-instance-protected',
  'api-instance-running',
]

test('16 17c.12: strings/en_US.json gains exactly 17c’s keys of section 2, with the English of the brief', () => {
  const table: Record<string, string> = strings
  const missing = GAINS.filter((key) => table[key] === undefined)
  expect(missing, 'keys missing from strings/en_US.json').toEqual([])
  const wrong = GAINS.filter((key) => table[key] !== en(key))
  expect(
    wrong,
    'keys whose en_US.json text is not the English of the brief',
  ).toEqual([])
  expect(
    Object.keys(table)
      .filter((key) => key.startsWith('instance-new-'))
      .sort(),
    'nothing else in the create form’s namespace',
  ).toEqual(GAINS.filter((key) => key.startsWith('instance-new-')).sort())
})

// ── 17c review (designer, 2026-11-26) ──────────────────────────────────────
//
// The defects and the adviser notes the review of the built slice names, each
// pinned as the review states the fix:
//  - D1: a switched-off Remove opens nothing (17c.6a: switched off sends
//    nothing, as 17e.5 and the Save bar's Delete do).
//  - D2: the delete's 504 is `delete-unknown` with the list read again
//    (17f.10), its 502 `api-client-service-failed` (17c.8).
//  - D3: the create's 502 `client-service-failed` is in the alert with its
//    text (17c.5).
//  - R2: the pool read's 401 offers a Sign in, and signing in reads the pool
//    again, so the select arrives (17c.2, as 17e.8 does for a section).
//  - A1: the create form's heading asks room above itself, in one rule for
//    both create forms.
//  - A2: the Remove keeps no start margin below 480 px.
//  - A4: the Server profile field's hint shows only with its select.

test('16 17c.6a: pressing the switched-off Remove of a running instance opens no confirm group, leaves the opener closed and sends nothing (17c review D1)', async () => {
  const { bff, main } = await instancePage({}, [
    { ...STOPPED, state: 'active' },
  ])
  const remove = removeButton(main)
  expect(
    remove.getAttribute('aria-disabled'),
    'running: the verb is switched off',
  ).toBe('true')
  expect(described(remove), 'with the reason of 17c.6a').toContain(
    en('instance-remove-stop-first'),
  )
  press(remove)
  expect(
    main.querySelector('.confirm-group'),
    'no confirm opens for an action that cannot run',
  ).toBeNull()
  expect(
    within(main).queryByRole('group', {
      name: en('instance-remove-question', STOPPED.id),
    }),
    'nothing asks the question',
  ).toBeNull()
  expect(remove.getAttribute('aria-expanded'), 'the opener says closed').toBe(
    'false',
  )
  expect(sent(bff, 'DELETE'), 'nothing sent').toHaveLength(0)
})

test('16 17c.2: a 401 on the pool read shows session-ended-retry and a Sign in in place of the select, with Create switched off; signing in reads the pool again and the select arrives (17c review R2)', async () => {
  // The fake's GET option takes only `init`, so the fixture counts the reads
  // itself (as ServerProfilesDelete1.test.tsx does): the first is answered 401,
  // every later one, the read of the new session included, answers the pool.
  let reads = 0
  const { bff, main } = await overview({
    serverProfiles: () =>
      ++reads === 1
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, POOL),
  })
  const form = await openForm(main)
  expect(
    within(form).getByText(en('session-ended-retry')),
    'the text of the failed read, in place of the select',
  )
  expect(within(form).queryByRole('combobox'), 'no select').toBeNull()
  expect(
    buttonIn(form, 'create-button').getAttribute('aria-disabled'),
    'Create switched off',
  ).toBe('true')
  const signIn = within(form).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(
    before(within(form).getByText(en('session-ended-retry')), signIn),
    'the Sign in follows the text',
  ).toBe(true)
  press(signIn)
  await signInInDialog('alice')
  expect(
    within(form).queryByText(en('session-ended-retry')),
    'the words of the failed read are gone',
  ).toBeNull()
  expect(
    within(form).queryByRole('button', { name: en('session-sign-in') }),
    'no Sign in left',
  ).toBeNull()
  expect(
    sent(bff, 'GET').filter((c) => c.path === '/api/profiles/server').length,
    'the pool read again for the new session',
  ).toBe(2)
  const select = (await within(form).findByRole('combobox', {
    name: en('instance-new-server'),
  })) as HTMLSelectElement
  expect(
    [...select.options].map((o) =>
      (o.textContent ?? '').replace(/\s+/g, ' ').trim(),
    ),
    'the free profiles of the read',
  ).toEqual([
    en('instance-new-server-choose'),
    'default (port 8080)',
    'lab (port 9090)',
  ])
  expect(
    buttonIn(form, 'create-button').getAttribute('aria-disabled'),
    'Create can act again',
  ).toBeNull()
})

test('16 17c.8, 17f.10: a 504 client-timeout on the delete says delete-unknown in the group’s alert, never request-failed, and reads the instance list again (17c review D2)', async () => {
  const { bff, main } = await instancePage({
    instanceDelete: () => reply(504, { key: 'client-timeout', args: [] }),
  })
  const before = sent(bff, 'GET').filter(
    (c) => c.path === '/api/instances',
  ).length
  const group = await openConfirm(main)
  press(confirmButton(main, 'confirm-remove'))
  await within(group).findByText(en('delete-unknown'))
  expect(
    alertTexts(group),
    'the outcome is unknown, so the write says so',
  ).toEqual([en('delete-unknown')])
  expect(
    main.querySelector('.confirm-group'),
    'the group stays open',
  ).not.toBeNull()
  await waitFor(() => {
    const now = sent(bff, 'GET').filter(
      (c) => c.path === '/api/instances',
    ).length
    if (now > before) return
    throw new Error('the list was not read again')
  })
})

test('16 17c.8: a 502 client-service-failed on the delete says api-client-service-failed in the group’s alert, the group staying open (17c review D2)', async () => {
  const { main } = await instancePage({
    instanceDelete: () =>
      reply(502, { key: 'client-service-failed', args: [] }),
  })
  const group = await openConfirm(main)
  press(confirmButton(main, 'confirm-remove'))
  await within(group).findByText(en('api-client-service-failed'))
  expect(alertTexts(group), 'the key’s own text').toEqual([
    en('api-client-service-failed'),
  ])
  expect(
    main.querySelector('.confirm-group'),
    'the group stays open',
  ).not.toBeNull()
  expect(
    within(main).queryByText(en('instance-removed', STOPPED.id)),
    'the page is still the page',
  ).toBeNull()
})

test('16 17c.5: a 502 client-service-failed on the create says api-client-service-failed in the alert, the form staying open with the values typed (17c review D3)', async () => {
  const { bff, main } = await overview({
    instanceCreate: () =>
      reply(502, { key: 'client-service-failed', args: [] }),
  })
  const form = await openForm(main)
  fill(form, 'lab', 'lab')
  await within(main).findByText(en('api-client-service-failed'))
  expect(alertTexts(form), 'the alert carries the key’s text').toEqual([
    en('api-client-service-failed'),
  ])
  expect(idBox(form).value, 'the id typed stays').toBe('lab')
  expect(serverSelect(form).value, 'the profile chosen stays').toBe('lab')
  expect(
    sent(bff, 'POST').filter((c) => c.path === '/api/instances').length,
    'one create',
  ).toBe(1)
})

// The 17c review's A1 asked for room above a create form's heading "in one
// rule" for both forms; the rule built for it, a bare `h2, h3 { margin-top:
// var(--space-4) }`, moved every heading of every page (the re-check's D4). So
// the pin is two: the room, scoped to the one heading that needs it, and the
// room the other headings keep. The server profiles' create form's h2 had its
// --space-6 from the sheet's `h2` rule already and needs nothing.
test('16 17c.1a: the instance create form’s h3 gets margin-top: var(--space-4) from one rule scoped to the form, and the sheet holds no bare h2/h3 margin-top rule (17c re-check D4)', async () => {
  const { main } = await overview()
  const form = await openForm(main)
  const h3 = within(form).getByRole('heading', {
    level: 3,
    name: en('instances-new-title'),
  })
  const room = scopedBy(h3, TOP).filter(
    ({ rule }) => decl(rule.body, 'margin-top') === 'var(--space-4)',
  )
  expect(
    marginTop(h3, TOP),
    'the room between the + and the heading, so the + is not read as part of it',
  ).toBe('var(--space-4)')
  expect(
    room,
    'one rule, and only one, gives this heading its room',
  ).toHaveLength(1)
  const cs = compounds(room[0]!.part)
  expect(
    // The two shapes the re-check sanctions: `form > h3:first-child` (an
    // ancestor compound) and a class on this h3 (a compound of tag + class).
    cs.length > 1 || cs[cs.length - 1]!.length > 1,
    `the rule is scoped to its place, not written on the tag. The rule read: ${room[0]!.part}`,
  ).toBe(true)
  expect(
    bareHeadingRoom().map((r) => r.sel.replace(/\s+/g, ' ')),
    'no unscoped h2/h3 rule declares a margin-top: that pair took every h2 of every page from --space-6 to --space-4 and gave every h3 a --space-4 it never had. The sheet’s heading rules declare the margin shorthand, so this is empty once the pair is gone',
  ).toEqual([])
})

test('16 17c.1a: the headings the bare rule moved keep the sheet’s own room: every h2 keeps --space-6 above it, the server profiles’ create h2 included, and the sheet’s h3 rule keeps no margin-top (17c re-check D4)', async () => {
  const tops = (root: HTMLElement) =>
    Object.fromEntries(
      [...root.querySelectorAll('h2')].map((h) => [
        h.textContent ?? '',
        marginTop(h, TOP),
      ]),
    )
  const { main } = await overview()
  expect(
    tops(main),
    'the Overview’s host heading keeps the room of the sheet’s h2 rule',
  ).toEqual({ [en('instances-title')]: 'var(--space-6)' })
  cleanup()
  const poolForm = await poolCreateForm()
  expect(
    marginTop(
      within(poolForm).getByRole('heading', {
        level: 2,
        name: en('server-profiles-new-title'),
      }),
      TOP,
    ),
    'the server profiles’ create h2 had its 40 px already: the fix takes nothing from it',
  ).toBe('var(--space-6)')
  cleanup()
  const { main: page } = await instancePage()
  await within(page).findByRole('heading', {
    level: 2,
    name: en('instance-models-title'),
  })
  expect(
    tops(page),
    'the instance page’s section headings keep --space-6 above them',
  ).toEqual({
    [en('instance-server-title')]: 'var(--space-6)',
    [en('instance-models-title')]: 'var(--space-6)',
  })
  const h3rule = TOP.filter((r) =>
    partsOf(r.sel).some(
      (p) =>
        compounds(p).length === 1 &&
        compounds(p)[0]!.length === 1 &&
        compounds(p)[0]![0] === 'h3',
    ),
  )
  expect(
    h3rule.map((r) => r.sel.replace(/\s+/g, ' ')),
    'the sheet’s h3 rule, once, at the top level',
  ).toEqual(['h3'])
  expect(
    decl(h3rule[0]!.body, 'margin-top'),
    'an h3 keeps no room above it: the model profile page’s card headings sit where the h3 rule puts them',
  ).toBeUndefined()
})

test('16 17c.6a: below 480 px the Remove of the run row keeps no start margin, so a Remove wrapped to its own line is not indented under Start (17c review A2)', async () => {
  const { main } = await instancePage()
  const remove = removeButton(main)
  expect(
    reaches(remove, TOP)['margin-inline-start'],
    'wide: the row keeps --space-5 between Restart and Remove',
  ).toBe('var(--space-5)')
  expect(
    reaches(remove, [...TOP, ...NARROW])['margin-inline-start'],
    'narrow: the margin goes, the row wraps anyway. The rule must outweigh the top-level .run-remove: a rule of the same specificity inside the block at index.css 648 sits BEFORE it in the sheet and loses',
  ).toBe('0')
})

test('16 17c.2: with no free server profile the Server profile field shows no hint for the select that is not there, and the ID keeps its own (17c review A4)', async () => {
  const { main } = await overview({
    serverProfiles: POOL.map((p) => profile(p.id, p.port ?? 0, 'default')),
  })
  const form = await openForm(main)
  expect(within(form).getByText(en('instance-new-no-free')), 'the message')
  expect(
    within(form).queryByText(en('instance-new-server-hint')),
    'no hint describing a control that is not there',
  ).toBeNull()
  expect(
    within(form).getByText(en('instance-new-id-hint')),
    'the ID keeps its hint',
  )
})
