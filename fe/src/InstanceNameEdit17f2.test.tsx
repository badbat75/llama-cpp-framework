// Brief 16, slice 17f2 (fe/briefs/16-server-profiles-and-instances.md): the
// instance's DISPLAY NAME, set from a pencil right after the page heading,
// replacing 17f's Rename button. The whole app renders at
// #/instances/default against a fake bff (mocked fetch); "16 17f2.n" is the
// brief's criterion n.
//
// This file is 17f's `InstanceRename17f.test.tsx`, which the brief supersedes
// (17f2 "Superseded: 17f.5, 17f.6's and 17f.9's references to Rename and its
// place"): the form's save, refusal, mid-write and Enter pins are kept here
// under their 17f2 numbers, with the keys, the texts and the place 17f2 gives
// them. The name in the nav, the tiles and the headings is
// `InstanceName17f.test.tsx`; the 504 rule of the other writes is
// `WriteOutcome17f.test.tsx`.
//
// 17f2.1's look and 17f2.9's reflow are pinned from the sheet, read the way
// `Reflow.test.ts` reads it (a small local copy: a test file imports no other
// test file). What jsdom cannot see is the designer's and the `fe/ux` check's:
// the pencil drawn as a 40 px square beside the heading, the title row
// wrapping with a 64-character name at 320 px and 200 % text, the text
// selected in the box, and the form's width against the Server form's field.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  deferred,
  en,
  fakeBff,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

/** The instance the page shows: labelled, so the name is the label (17f.1). */
const ITEM = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 9931,
  tags: [],
  instanceTags: [],
  boot: true,
  label: 'Coding assistant',
}

const PUT = '/api/instances/default/label'

/** 17f2.10 takes `instance-rename` out of the strings file, so `en()` throws
 *  on it: the absence is pinned with the word the button used to show. */
const OLD_RENAME = 'Rename'

/** The `pencil` of the icon set (17f2.1: "added to `Icon.tsx` from the same
 *  icon set as the others"). The glyph's own paths are the coder's, so only
 *  the name in the union is pinned. */
const ICONS = readFileSync(join(import.meta.dirname, 'Icon.tsx'), 'utf8')

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/instances/default')
  dialogs = stubDialog()
})

afterEach(async () => {
  // A link click queues its hashchange as a task: let it land on this test's
  // page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a dialog closing) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** A keyed refusal of `status`, with the reply's `field` when it names one. */
function refused(
  status: number,
  key: string,
  args: string[] = [],
  field?: string,
): () => Response {
  return () => reply(status, { key, args, ...(field ? { field } : {}) })
}

/** The app at the instance page, loaded. `name` is the h1 the fixture's item
 *  gives (17f.1): a case that changes the label awaits the name that change
 *  gives, not the fixture's. */
async function openPage(options: BffOptions = {}, name = 'Coding assistant') {
  const bff = fakeBff({
    instances: [ITEM],
    boot: { read: () => reply(200, { enabled: true }) },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name })
  return { bff, main }
}

/** The pencil, by its accessible name (17f2.1). */
function pencil(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('instance-name-edit') })
}

/** Opens the form by pressing the pencil and returns it. The pencil toggles,
 *  so a caller never presses it first. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  fireEvent.click(pencil(main))
  return within(main).findByRole('form', { name: en('instance-name-label') })
}

/** The Display name box of the open form. */
function nameBox(main: HTMLElement): HTMLInputElement {
  return within(main).getByRole('textbox', {
    name: en('instance-name-label'),
  })
}

/** Types `text` in a box. */
function typeIn(box: HTMLInputElement, text: string): void {
  fireEvent.change(box, { target: { value: text } })
}

/** The Save of the open form (17f2.3: `instance-name-save`, "Save"). */
const saveButton = (main: HTMLElement) =>
  within(main).getByRole('button', { name: en('instance-name-save') })

/** The Cancel of the open form (17f2.3: `instance-label-cancel` stays). */
const cancelButton = (main: HTMLElement) =>
  within(main).getByRole('button', { name: en('instance-label-cancel') })

/** The writes the bff saw on the label route. */
const labelWrites = (bff: ReturnType<typeof fakeBff>) => bff.to('PUT', PUT)

/** The body of the n-th write (from 1). */
const writtenLabel = (bff: ReturnType<typeof fakeBff>, n = 1) =>
  (labelWrites(bff)[n - 1]!.body as { label: string }).label

/** The id line under the heading (17f2.2). */
const idLine = (main: HTMLElement) => main.querySelector('p.instance-id')

/** The title row: the box holding the heading and the pencil (17f2.1). */
function titleRow(main: HTMLElement): HTMLElement {
  const row = pencil(main).parentElement
  expect(row, 'the pencil is in a row').not.toBeNull()
  expect(
    row!.contains(within(main).getByRole('heading', { level: 1 })),
    'the row holds the heading and the pencil',
  ).toBe(true)
  return row!
}

/** The text a sighted user reads inside `el`: what is not an aria-hidden icon
 *  and not a `visually-hidden` span (17f2.1: the icon and the hidden label). */
function visibleText(el: HTMLElement): string {
  return [...el.childNodes]
    .filter(
      (n) =>
        !(n instanceof Element && n.getAttribute('aria-hidden') === 'true') &&
        !(n instanceof Element && n.classList.contains('visually-hidden')),
    )
    .map((n) => n.textContent ?? '')
    .join('')
    .replace(/\s+/g, ' ')
    .trim()
}

/** The focusable elements of `root`, in document order, as Tab names them. */
function tabbable(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      el.closest('dialog:not([open])') === null,
  )
}

/** Signs in through the dialog of the app's Sign in button. */
async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  fireEvent.change(
    within(d).getByRole('textbox', { name: en('signin-username') }),
    { target: { value: user } },
  )
  fireEvent.change(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
}

// ── The stylesheet reader (the way `Reflow.test.ts` reads `index.css`) ─────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  compact: string
  body: string
  depth: number
}

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const SHEET: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim(),
  compact: m[1].replace(/\s+/g, ''),
  body: m[2],
  depth: opened(m.index ?? 0),
}))

// Top-level rules only: the layout applies at every width, not in a media block.
const TOP = SHEET.filter((r) => r.depth === 0)

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

const tokens = (part: string): string[] =>
  part.match(/\.[-a-z0-9_]+|#[-a-z0-9_]+|[-a-z0-9_]+/g) ?? []

/** The top-level rules that style this element: a selector part ending in the
 *  element's tag or one of its classes, whose other parts are classes of its
 *  ancestors. The classes are read off the page, so the pin follows the names
 *  the coder chose, and a rule scoped to a parent counts. */
function stylesFor(el: HTMLElement): Rule[] {
  const own = new Set([
    el.tagName.toLowerCase(),
    ...[...el.classList].map((c) => `.${c}`),
  ])
  const anc = new Set<string>()
  for (let p = el.parentElement; p; p = p.parentElement)
    for (const c of p.classList) anc.add(`.${c}`)
  return TOP.filter((r) =>
    r.compact.split(',').some((part) => {
      const ts = tokens(part)
      if (ts.length === 0) return false
      const last = ts[ts.length - 1]!
      return own.has(last) && ts.slice(0, -1).every((t) => anc.has(t))
    }),
  )
}

/** The top-level rules that style the `tag` inside a box carrying `classes`:
 *  a selector part whose last compound is the tag (or the heading's id) and
 *  whose other compounds are classes of that box. The box's own rules are left
 *  out, so a declaration read here is one that reaches the tag. */
function rulesInside(tag: string, classes: string[]): Rule[] {
  const own = new Set(classes.map((c) => `.${c}`))
  return TOP.filter((r) =>
    r.compact.split(',').some((part) => {
      const ts = tokens(part)
      if (ts.length === 0) return false
      const last = ts[ts.length - 1]!
      if (last !== tag && last !== '#page-heading') return false
      return ts.slice(0, -1).every((t) => own.has(t))
    }),
  )
}

/** The declarations of `rules`, later ones winning, as the cascade applies
 *  them in the sheet's order. */
function merged(rules: Rule[]): Record<string, string> {
  const out: Record<string, string> = {}
  for (const rule of rules)
    for (const d of rule.body.split(';')) {
      const i = d.indexOf(':')
      if (i >= 0) out[d.slice(0, i).trim()] = d.slice(i + 1).trim()
    }
  return out
}

/** A selector, as a needle names it: runs of whitespace collapsed to one space
 *  and the space around a combinator dropped, so `.instance-name-form > *`
 *  matches `…>*` too, while a descendant keeps its space (`.a .b` is not
 *  `.a.b`, the pitfall `Reflow.test.ts` names for `compact`). */
const selKey = (s: string): string =>
  norm(s)
    .replace(/\s*([>+~])\s*/g, '$1')
    .replace(/\s+/g, ' ')

/** The top-level rules whose selector group carries `sel` as one of its parts
 *  (the way the 22 D5 to D8 pins read the sheet: the rule that styles the
 *  thing, by its selector, not the cascade). */
function rulesFor(sel: string): Rule[] {
  const want = selKey(sel)
  return TOP.filter((r) => r.sel.split(',').some((p) => selKey(p) === want))
}

/** The declarations of those rules, each as written with its whitespace
 *  collapsed, in the sheet's order. */
const declsOf = (sel: string): string[] =>
  rulesFor(sel).flatMap((r) => r.body.split(';').map(norm).filter(Boolean))

// ── 17f2.1: the pencil ────────────────────────────────────────────────────

test('16 17f2.1: the title row is the h1 and, right after it, the pencil: an aria-hidden icon and a span.visually-hidden with instance-name-edit, no visible text, aria-expanded false and aria-controls the form, outside the h1, and 17f’s Rename button is gone from the header', async () => {
  const { main } = await openPage()
  const h1 = within(main).getByRole('heading', { level: 1 })
  expect(h1.getAttribute('id'), 'the heading keeps its id').toBe('page-heading')
  expect(
    h1.getAttribute('tabindex'),
    'the page still focuses the heading on arrival',
  ).toBe('-1')
  expect(h1.textContent, 'the heading is the name alone').toBe(
    'Coding assistant',
  )
  const button = pencil(main)
  expect(
    h1.contains(button),
    'the button is outside the h1, so the heading’s accessible name stays the name alone',
  ).toBe(false)
  expect(
    h1.nextElementSibling,
    'the pencil is right after the heading, in the same row',
  ).toBe(button)
  expect(
    button.parentElement,
    'the heading and the pencil are in one title row',
  ).toBe(h1.parentElement)
  expect(button.getAttribute('aria-expanded'), 'closed at first').toBe('false')
  expect(
    button.getAttribute('aria-label'),
    'the name comes from the hidden span, not an aria-label',
  ).toBeNull()
  const icon = button.querySelector('svg[aria-hidden="true"]')
  expect(icon, 'the button carries an aria-hidden icon').not.toBeNull()
  expect(
    [...icon!.querySelectorAll('path')].length,
    'the icon is drawn by a path of the icon set',
  ).toBeGreaterThan(0)
  const spans = [...button.querySelectorAll('span')].filter((s) =>
    s.classList.contains('visually-hidden'),
  )
  expect(spans.length, 'exactly one visually hidden span').toBe(1)
  expect(
    spans[0]!.textContent?.replace(/\s+/g, ' ').trim(),
    'the hidden span holds instance-name-edit',
  ).toBe(en('instance-name-edit'))
  expect(visibleText(button), 'the button shows no text').toBe('')
  const form = await openForm(main)
  expect(button.getAttribute('aria-expanded'), 'open').toBe('true')
  expect(
    button.getAttribute('aria-controls'),
    'aria-controls names the form',
  ).toBe(form.id)
  expect(
    within(main).queryAllByRole('button', { name: OLD_RENAME }),
    '17f’s Rename button is removed from the header (17f2.1)',
  ).toHaveLength(0)
  expect(
    ICONS,
    "Icon.tsx gains the pencil of the same icon set (17f2.1: 'added to Icon.tsx from the same icon set as the others')",
  ).toMatch(/\|\s*'pencil'/)
})

test('16 17f2.1 (the sheet): the pencil is drawn by the icon-button rule the list’s + uses: a 40 px square with the --control outline, never shrinking, sharing that rule, and --space-2 from the heading’s text', async () => {
  // "Share the class or the rule, not a copy" (17b2.2, fe/DESIGN.md "Icon
  // buttons of a list"): the declarations that reach the pencil are the list
  // +’s look, and the rule that gives them styles `.ot-add` too. The 40 by 40
  // target is WCAG 2.5.8.
  const { main } = await openPage()
  const button = pencil(main)
  const at = merged(stylesFor(button))
  expect(norm(at['width'] ?? ''), 'the pencil is 40 px wide').toBe('40px')
  expect(norm(at['min-height'] ?? ''), 'and at least 40 px high').toBe('40px')
  expect(
    norm(at['border'] ?? ''),
    'with the --control outline of the list’s icon button',
  ).toBe('var(--border-w) solid var(--control)')
  expect(
    norm(at['flex'] ?? '') === 'none' || norm(at['min-width'] ?? '') === '40px',
    'it never shrinks below 40 px (17f2.9): flex: none, or a 40 px min-width',
  ).toBe(true)
  // The one rule, not a copy of it: the rule that declares the 40 px styles
  // `.ot-add` as well, or the pencil carries `.ot-add` itself.
  const classes = [...button.classList]
  const wide = stylesFor(button).filter(
    (r) => norm(prop(r.body, 'width')) === '40px',
  )
  expect(wide.length, 'one rule gives the pencil its 40 px square').toBe(1)
  const group = wide[0]!.sel.split(',').map((p) => p.replace(/\s+/g, ''))
  expect(
    group.includes('.ot-add') || classes.includes('ot-add'),
    'the pencil shares the rule of the list’s + (the brief: share the class or the rule, not a copy): the rule styles .ot-add too, or the pencil carries the class',
  ).toBe(true)
  // The room from the heading's text: the coder uses a gap on the row or a
  // margin on the pencil, so the pin names the value, not the property.
  const room = [...stylesFor(button), ...stylesFor(titleRow(main))].filter(
    (r) =>
      ['gap', 'margin-left', 'margin-inline', 'margin-inline-start'].some(
        (name) => norm(prop(r.body, name)) === 'var(--space-2)',
      ),
  )
  expect(
    room.length,
    'exactly one rule gives the pencil its --space-2 from the heading’s text',
  ).toBe(1)
})

// ── 17f2.2: the id line says it stays ─────────────────────────────────────

test('16 17f2.2: the id line under the heading reads instance-id-fixed with the id, and shows too on the page of an instance named by its id, while the tile keeps instance-id-shown', async () => {
  const { main } = await openPage()
  const id = idLine(main)
  expect(id, 'the id line is there').not.toBeNull()
  expect(
    id!.textContent,
    'the instance page reads instance-id-fixed: the id does not change',
  ).toBe(en('instance-id-fixed', 'default'))
  cleanup()
  // The name IS the id (no label, port 0): the line shows on this page too, so
  // the reader always sees which word is the key (17f2.2). The fixture has no
  // `default`, so the address moves to the page this fixture serves.
  startAt('#/instances/embed')
  const second = await openPage(
    {
      instances: [
        { ...ITEM, id: 'embed', engine: 'strata', port: 0, label: null },
      ],
    },
    'embed',
  )
  await within(second.main).findByText(en('instance-id-fixed', 'embed'))
  expect(
    idLine(second.main)?.textContent,
    'the page of an instance named by its id says it too',
  ).toBe(en('instance-id-fixed', 'embed'))
  cleanup()
  // The tiles keep the short key (17f2.2: "the tiles and Used by keep
  // instance-id-shown").
  startAt('#/')
  fakeBff({ instances: [ITEM] })
  render(<App />)
  await screen.findByRole('heading', { level: 2, name: en('instances-title') })
  await screen.findByText(en('instance-id-shown', 'default'))
  expect(
    document.querySelector('.tile .instance-id')?.textContent,
    'the tile keeps instance-id-shown',
  ).toBe(en('instance-id-shown', 'default'))
})

// ── 17f2.3: the form ──────────────────────────────────────────────────────

test('16 17f2.3: pressing the pencil sets aria-expanded true and opens, right under the id line and before the engine, port and state lines, a form named instance-name-label: the Display name label above a box (autocomplete off, spellcheck false, no placeholder) holding the current label with its text selected and the focus in it, the hint, the error place, then Save primary and Cancel in a row, then the status line and the alert place', async () => {
  const { main } = await openPage()
  const button = pencil(main)
  const form = await openForm(main)
  // The form's layout (the --space-3 under the title row, the column, the
  // label above the box, the --space-2 between the buttons) is pinned from the
  // sheet in Reflow.test.ts, "16 17f2.3 (17f review D3)".
  const id = idLine(main)
  expect(id, 'the id line is there to sit under').not.toBeNull()
  expect(before(id!, form), 'the form is under the id line').toBe(true)
  const header = main.querySelector('.instance-header')
  expect(
    header,
    'the engine, port and state lines are in the header',
  ).not.toBeNull()
  expect(
    before(form, header!),
    'the form is before them (17f2.3: right under the id line)',
  ).toBe(true)
  expect(before(button, form), 'the form is after the pencil').toBe(true)
  const box = within(form).getByRole<HTMLInputElement>('textbox', {
    name: en('instance-name-label'),
  })
  expect(box.getAttribute('type'), 'a text input').toBe('text')
  expect(box.getAttribute('autocomplete'), 'no autofill of a name').toBe('off')
  expect(box.getAttribute('spellcheck'), 'no spellcheck of a name').toBe(
    'false',
  )
  expect(
    box.getAttribute('placeholder'),
    'no placeholder: the hint names the automatic name (1.4.3)',
  ).toBeNull()
  expect(box.value, 'the box holds the current label').toBe('Coding assistant')
  expect(
    [box.selectionStart, box.selectionEnd],
    'its text is selected, so typing replaces it',
  ).toEqual([0, 'Coding assistant'.length])
  expect(document.activeElement, 'the focus is in the box').toBe(box)
  const label = within(form).getByText(en('instance-name-label'), {
    selector: 'label',
  })
  expect(label.getAttribute('for'), 'the label is the box’s').toBe(box.id)
  expect(before(label, box), 'the label is above the box').toBe(true)
  const hint = form.querySelector('[id$="-hint"], .hint, .field-hint')
  expect(hint, 'the hint is there').not.toBeNull()
  const save = within(form).getByRole('button', {
    name: en('instance-name-save'),
  })
  const cancel = within(form).getByRole('button', {
    name: en('instance-label-cancel'),
  })
  expect(save.className, 'Save is primary').toContain('primary')
  expect(before(box, save), 'Save follows the box and the hint').toBe(true)
  expect(before(save, cancel), 'Cancel follows Save').toBe(true)
  const status = within(form).getByRole('status')
  expect(before(cancel, status), 'the status line follows the buttons').toBe(
    true,
  )
  expect(
    form.querySelector('[role="alert"]'),
    'the alert place is in the form',
  ).toBeNull()
})

// ── 17f2.4: the hint ──────────────────────────────────────────────────────

test('16 17f2.4: the hint is always shown, naming the automatic name and the id, with no About toggle, and the box’s aria-describedby names the hint, then the error when a write is refused', async () => {
  const { main } = await openPage({
    instanceLabel: refused(422, 'instance-bad-label', ['a;b'], 'label'),
  })
  const form = await openForm(main)
  const box = nameBox(main)
  const hint = form.querySelector('.field-hint, [id$="-hint"]')
  expect(hint, 'the hint is in the form').not.toBeNull()
  expect(
    hint!.textContent?.replace(/\s+/g, ' ').trim(),
    'it says what changes and what does not, naming the automatic name and the id',
  ).toBe(en('instance-name-hint', 'llama.cpp :9931', 'default'))
  expect(
    hint!.getAttribute('hidden'),
    'the hint is shown, not hidden behind a toggle',
  ).toBeNull()
  expect(
    form.querySelectorAll('button.about').length,
    'no About toggle: the hint is always shown (17f2.4)',
  ).toBe(0)
  const described = () =>
    (box.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(Boolean)
  expect(described()[0], 'the box is described by its hint').toBe(hint!.id)
  // 17f2.6: an unchanged text sends nothing, so the refusal needs a changed
  // name in the box before the press.
  typeIn(box, 'Coding assistant 2')
  fireEvent.click(saveButton(main))
  const error = await within(main).findByText(en('api-instance-bad-label'))
  expect(described(), 'the hint first, then the error (3.3.2)').toEqual([
    hint!.id,
    error.id,
  ])
})

// ── 17f2.5: the empty state ───────────────────────────────────────────────

test('16 17f2.5: with no label the heading reads the automatic name, the id line reads instance-id-fixed, the form opens with an empty box whose hint names that automatic name, and saving the empty box sends nothing and closes the form with the focus on the pencil', async () => {
  const { bff, main } = await openPage(
    { instances: [{ ...ITEM, label: null }] },
    // 17f.1: no label, port above 0: the name is the engine and the port.
    'llama.cpp :9931',
  )
  expect(idLine(main)?.textContent, 'the id line reads instance-id-fixed').toBe(
    en('instance-id-fixed', 'default'),
  )
  const form = await openForm(main)
  const box = nameBox(main)
  expect(box.value, 'no label: an empty box').toBe('')
  expect(
    form
      .querySelector('.field-hint, [id$="-hint"]')!
      .textContent!.replace(/\s+/g, ' ')
      .trim(),
    'the hint names the automatic name the empty box falls back to',
  ).toBe(en('instance-name-hint', 'llama.cpp :9931', 'default'))
  fireEvent.click(saveButton(main))
  await flushTasks()
  expect(labelWrites(bff), 'nothing to write').toHaveLength(0)
  expect(
    within(main).queryByRole('form', { name: en('instance-name-label') }),
    'the form closed',
  ).toBeNull()
  expect(document.activeElement, 'the focus is on the pencil').toBe(
    pencil(main),
  )
})

// ── 17f2.6: save and close ────────────────────────────────────────────────

test('16 17f2.6: Save sends PUT /api/instances/default/label with the text trimmed, and Enter in the box sends the same', async () => {
  for (const how of ['button', 'enter'] as const) {
    const { bff, main } = await openPage()
    await openForm(main)
    const box = nameBox(main)
    typeIn(box, '  Edge box  ')
    if (how === 'button') {
      fireEvent.click(saveButton(main))
    } else {
      fireEvent.keyDown(box, { key: 'Enter' })
    }
    await flushTasks()
    const writes = labelWrites(bff)
    expect(writes, `${how}: one write`).toHaveLength(1)
    expect(writes[0]!.body, `${how}: the body is the label, trimmed`).toEqual({
      label: 'Edge box',
    })
    // The loop renders a fresh app per case: RTL cleans only between tests.
    cleanup()
  }
})

test('16 17f2.6: an empty box sends the empty string, which is the clear', async () => {
  const { bff, main } = await openPage()
  await openForm(main)
  typeIn(nameBox(main), '')
  fireEvent.click(saveButton(main))
  await flushTasks()
  expect(writtenLabel(bff), 'the clear is the empty string, not a null').toBe(
    '',
  )
})

test('16 17f2.6: a text that trims to the current display name sends nothing and closes the form with the focus on the pencil', async () => {
  const { bff, main } = await openPage()
  await openForm(main)
  typeIn(nameBox(main), '  Coding assistant  ')
  fireEvent.click(saveButton(main))
  await flushTasks()
  expect(labelWrites(bff), 'nothing to write').toHaveLength(0)
  expect(
    within(main).queryByRole('form', { name: en('instance-name-label') }),
    'the form closed',
  ).toBeNull()
  expect(document.activeElement, 'the focus is on the pencil').toBe(
    pencil(main),
  )
})

test('16 17f2.6: Escape, Cancel and the pencil pressed again close the form, drop the typing, and put the focus on the pencil', async () => {
  for (const close of ['cancel', 'escape', 'pencil'] as const) {
    const { main } = await openPage()
    await openForm(main)
    typeIn(nameBox(main), 'a name nobody saved')
    if (close === 'cancel') {
      fireEvent.click(cancelButton(main))
    } else if (close === 'escape') {
      fireEvent.keyDown(nameBox(main), { key: 'Escape' })
    } else {
      fireEvent.click(pencil(main))
    }
    await flushTasks()
    expect(
      within(main).queryByRole('form', { name: en('instance-name-label') }),
      `${close}: the form is closed`,
    ).toBeNull()
    expect(
      pencil(main).getAttribute('aria-expanded'),
      `${close}: the pencil says closed`,
    ).toBe('false')
    expect(document.activeElement, `${close}: the focus is on the pencil`).toBe(
      pencil(main),
    )
    const again = await openForm(main)
    expect(nameBox(main).value, `${close}: the typing is dropped`).toBe(
      'Coding assistant',
    )
    fireEvent.click(
      within(again).getByRole('button', {
        name: en('instance-label-cancel'),
      }),
    )
    await flushTasks()
    // The loop renders a fresh app per case: RTL cleans only between tests.
    cleanup()
  }
})

test('16 17f2.6: while the write runs the status reads instance-name-saving and both buttons are aria-disabled, and a second press sends nothing', async () => {
  const gate = deferred()
  const { bff, main } = await openPage({
    instanceLabel: () => gate.promise,
  })
  await openForm(main)
  typeIn(nameBox(main), 'Edge box')
  const save = saveButton(main)
  const cancel = cancelButton(main)
  fireEvent.click(save)
  const status = await within(main).findByText(en('instance-name-saving'))
  expect(status.getAttribute('role'), 'the status line says it').toBe('status')
  expect(
    save.getAttribute('aria-disabled'),
    'Save is aria-disabled, not disabled: the focus stays',
  ).toBe('true')
  expect(cancel.getAttribute('aria-disabled'), 'and Cancel too').toBe('true')
  fireEvent.click(save)
  fireEvent.click(save)
  await flushTasks()
  expect(labelWrites(bff), 'a second press sends nothing').toHaveLength(1)
  gate.resolve(reply(204))
  await flushTasks()
})

test('16 17f2.6: a 204 closes the form, puts the focus on the pencil, reads the list again, and the status right after the title row reads instance-name-saved with the text sent, kept after the form closes', async () => {
  const { bff, main } = await openPage()
  await openForm(main)
  typeIn(nameBox(main), 'Edge box')
  fireEvent.click(saveButton(main))
  const status = await within(main).findByText(
    en('instance-name-saved', 'Edge box'),
  )
  expect(
    within(main).queryByRole('form', { name: en('instance-name-label') }),
    'the form closed',
  ).toBeNull()
  expect(document.activeElement, 'the focus is on the pencil').toBe(
    pencil(main),
  )
  expect(status.getAttribute('role'), 'the line is a status').toBe('status')
  expect(
    before(titleRow(main), status),
    'the status is right after the title row, and kept after the close',
  ).toBe(true)
  expect(
    bff.to('GET', '/api/instances').length,
    'the list is read again, so the h1, the title, the nav and the tiles follow',
  ).toBeGreaterThan(1)
})

test('16 17f2.6: a 204 to the clear reads instance-name-cleared with the name the instance is back to, and a 204 on an unlabelled instance reads instance-name-saved with the text sent', async () => {
  const { main } = await openPage()
  await openForm(main)
  typeIn(nameBox(main), '   ')
  fireEvent.click(saveButton(main))
  await within(main).findByText(
    en('instance-name-cleared', en('instance-name-auto', 'llama.cpp', '9931')),
  )
  cleanup()
  const second = await openPage(
    { instances: [{ ...ITEM, label: null }] },
    'llama.cpp :9931',
  )
  await openForm(second.main)
  typeIn(nameBox(second.main), 'Edge box')
  fireEvent.click(saveButton(second.main))
  await within(second.main).findByText(en('instance-name-saved', 'Edge box'))
})

test('16 17f2.6 (17f review D2): Enter on a name that did not change is preventDefault-ed: the form closes and stays closed, and nothing is sent', async () => {
  // The Enter branch of the box’s onKeyDown closes the form and moves the focus
  // to the pencil without stopping the key: the same Enter then presses the
  // pencil, which is focused, and the form opens again (Chromium and WebKit),
  // and the implicit submit calls save a second time.
  const { bff, main } = await openPage()
  await openForm(main)
  const box = nameBox(main)
  typeIn(box, '  Coding assistant  ')
  expect(
    fireEvent.keyDown(box, { key: 'Enter' }),
    'the Enter branch calls preventDefault: the browser does nothing more with the key',
  ).toBe(false)
  await flushTasks()
  expect(labelWrites(bff), 'nothing to write').toHaveLength(0)
  expect(
    within(main).queryByRole('form', { name: en('instance-name-label') }),
    'the form is closed, and stays closed',
  ).toBeNull()
  expect(document.activeElement, 'the focus is on the pencil').toBe(
    pencil(main),
  )
})

test('16 17f2.6 (17b review R1): a write on its way is not cancelled: Escape, Cancel and the pencil pressed mid-write leave the form open with the typing, send no second write, and the 504 that lands answers in the open form', async () => {
  // 17f2.6 makes both buttons aria-disabled while the write runs, and 17f2.7
  // says the form stays open with what was typed after every failure: a form
  // closed mid-write answers the 409, the 422 or the 504 to nothing, and the
  // form opened again holds the text and sends a second write.
  for (const how of ['escape', 'cancel', 'pencil'] as const) {
    const gate = deferred()
    const { bff, main } = await openPage({
      instanceLabel: () => gate.promise,
    })
    await openForm(main)
    typeIn(nameBox(main), 'Edge box')
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('instance-name-saving'))
    if (how === 'escape') {
      fireEvent.keyDown(nameBox(main), { key: 'Escape' })
    } else if (how === 'cancel') {
      fireEvent.click(cancelButton(main))
    } else {
      fireEvent.click(pencil(main))
    }
    await flushTasks()
    expect(
      within(main).queryByRole('form', { name: en('instance-name-label') }),
      `${how}: the form is open, the write is on its way`,
    ).not.toBeNull()
    expect(nameBox(main).value, `${how}: the typing is kept`).toBe('Edge box')
    expect(labelWrites(bff), `${how}: nothing new is sent`).toHaveLength(1)
    gate.resolve(reply(504, { key: 'client-timeout' }))
    const alert = await within(main).findByText(en('write-timeout'))
    expect(
      alert.getAttribute('role'),
      `${how}: the refusal answers in the open form (17f2.7)`,
    ).toBe('alert')
    expect(
      within(main).queryByRole('form', { name: en('instance-name-label') }),
      `${how}: the form is open with the failure`,
    ).not.toBeNull()
    expect(nameBox(main).value, `${how}: the typing is still there`).toBe(
      'Edge box',
    )
    expect(labelWrites(bff), `${how}: still one write`).toHaveLength(1)
    cleanup()
  }
})

test('16 17f2.6: a 401 reads session-ended-retry with a Sign in after the alert; signing in puts the focus on Save with the typing kept, and sends nothing', async () => {
  const { bff, main } = await openPage({
    instanceLabel: refused(401, 'session-required'),
  })
  await openForm(main)
  typeIn(nameBox(main), 'Edge box')
  fireEvent.click(saveButton(main))
  const alert = await within(main).findByText(en('session-ended-retry'))
  const signIn = within(main).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(before(alert, signIn), 'Sign in is after the alert').toBe(true)
  fireEvent.click(signIn)
  await flushTasks()
  await signInInDialog('alice')
  expect(document.activeElement, 'the focus is on Save').toBe(saveButton(main))
  expect(nameBox(main).value, 'the typing is kept').toBe('Edge box')
  expect(labelWrites(bff), 'the refused write is not resent').toHaveLength(1)
})

// ── 17f2.7: the refusals ──────────────────────────────────────────────────

/** The field refusals: the text under the box, the box marked, the focus back
 *  in it, the typing kept (17f2.7, as 17f.7). */
for (const [what, answer, text] of [
  [
    '422 instance-bad-label',
    refused(422, 'instance-bad-label', ['a;b'], 'label'),
    en('api-instance-bad-label'),
  ],
  [
    '422 comment-markers',
    refused(422, 'comment-markers', ['a;b'], 'label'),
    en('api-comment-markers', 'a;b'),
  ],
  [
    '409 instance-label-taken',
    refused(409, 'instance-label-taken', ['Edge box', 'coder']),
    en('api-instance-label-taken', 'Edge box', 'coder'),
  ],
] as Array<[string, () => Response, string]>) {
  test(`16 17f2.7: ${what} shows its text under the Display name field, marks the field aria-invalid and described by the error, focuses the field, and keeps the typing with the form open`, async () => {
    const { main } = await openPage({ instanceLabel: answer })
    await openForm(main)
    const box = nameBox(main)
    typeIn(box, 'Edge box')
    fireEvent.click(saveButton(main))
    const error = await within(main).findByText(text)
    expect(box.getAttribute('aria-invalid'), 'the field is marked').toBe('true')
    expect(
      box.getAttribute('aria-describedby'),
      'the field is described by the error',
    ).toContain(error.id)
    expect(document.activeElement, 'the focus is in the field').toBe(box)
    expect(
      within(main).queryByRole('form', { name: en('instance-name-label') }),
      'the form stays open',
    ).not.toBeNull()
    expect(nameBox(main).value, 'with what was typed').toBe('Edge box')
  })
}

/** The alert refusals: one line in the form's alert, the form open with what
 *  was typed (17f2.7, as 17f.7). */
for (const [what, answer, text] of [
  [
    '422 with an unknown key',
    refused(422, 'x-y', [], 'label'),
    en('api-other', 'x-y'),
  ],
  [
    '400 instance-malformed',
    refused(400, 'instance-malformed'),
    en('request-failed'),
  ],
  [
    '404 client-no-instance',
    refused(404, 'client-no-instance'),
    en('instance-not-found', 'default'),
  ],
  [
    '503 client-unavailable',
    refused(503, 'client-unavailable'),
    en('api-client-unavailable'),
  ],
  ['413', refused(413, 'client-too-large'), en('request-failed')],
] as Array<[string, () => Response, string]>) {
  test(`16 17f2.7: ${what} shows its text in the form’s alert, keeps the form open with what was typed, and marks no field`, async () => {
    const { main } = await openPage({ instanceLabel: answer })
    await openForm(main)
    typeIn(nameBox(main), 'Edge box')
    fireEvent.click(saveButton(main))
    const alert = await within(main).findByText(text)
    expect(alert.getAttribute('role'), 'the line is an alert').toBe('alert')
    expect(
      within(main).queryByRole('form', { name: en('instance-name-label') }),
      'the form stays open',
    ).not.toBeNull()
    expect(nameBox(main).value, 'with what was typed').toBe('Edge box')
    expect(
      nameBox(main).getAttribute('aria-invalid'),
      'the field is not marked: the alert says it',
    ).toBeNull()
  })
}

test('16 17f2.7: a 504 client-timeout is not confirmed: write-timeout in the alert, the form open with what was typed, and the list read again so the name shown is the manager’s', async () => {
  const { bff, main } = await openPage({
    instanceLabel: refused(504, 'client-timeout'),
  })
  await openForm(main)
  typeIn(nameBox(main), 'Edge box')
  fireEvent.click(saveButton(main))
  const alert = await within(main).findByText(en('write-timeout'))
  expect(alert.getAttribute('role'), 'the line is an alert').toBe('alert')
  expect(
    alert.textContent,
    'a 504 never reads as a request that did not go through (17f.10)',
  ).not.toBe(en('request-failed'))
  expect(nameBox(main).value, 'the typing is kept').toBe('Edge box')
  expect(
    bff.to('GET', '/api/instances').length,
    'the list is read again',
  ).toBeGreaterThan(1)
})

test('16 17f2.7: a 403 session-not-administrator shows session-revoked in the form’s alert with a Sign in after it, and keeps the typing with the form open', async () => {
  // 17f.7 maps the 403 by writeFailure, as every other write does (15 C1.9):
  // the form's alert says it, the Sign in follows the alert, the typing stays.
  const { main } = await openPage({
    instanceLabel: refused(403, 'session-not-administrator'),
  })
  await openForm(main)
  typeIn(nameBox(main), 'Edge box')
  fireEvent.click(saveButton(main))
  const alert = await within(main).findByText(en('session-revoked'))
  expect(alert.getAttribute('role'), 'the line is an alert').toBe('alert')
  const signIn = within(main).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(before(alert, signIn), 'Sign in is after the alert').toBe(true)
  expect(
    within(main).queryByRole('form', { name: en('instance-name-label') }),
    'the form stays open',
  ).not.toBeNull()
  expect(nameBox(main).value, 'with what was typed').toBe('Edge box')
})

// ── 17f2.8: the keyboard ──────────────────────────────────────────────────

test('16 17f2.8: the tab order in main starts with the pencil, before Start, and with the form open the box, Save and Cancel follow the pencil', async () => {
  const { main } = await openPage()
  // The boot checkbox renders once its read lands: settle it before the
  // snapshot, and name it by the element (an input carries no aria-label and
  // no textContent, so the name map below needs the element, not the text).
  const bootBox = await within(main).findByRole('checkbox', {
    name: en('boot-toggle'),
  })
  const named = new Map<Element, string>([
    [bootBox, en('boot-label')],
    [pencil(main), en('instance-name-edit')],
  ])
  const nameOf = (el: HTMLElement): string =>
    named.get(el) ??
    (el.getAttribute('aria-label') ?? el.textContent ?? '').trim()
  const closed = tabbable(main).map(nameOf)
  expect(
    closed[0],
    'the pencil is the first stop of main, before Start (17f2.8)',
  ).toBe(en('instance-name-edit'))
  const order = [
    en('instance-name-edit'),
    en('run-start'),
    en('run-stop'),
    en('run-restart'),
    en('boot-label'),
    `${en('field-about')} ${en('instance-tags-own')}`,
  ]
  const at = order.map((name) => closed.indexOf(name))
  expect(
    at.every((i) => i >= 0),
    `every control is tabbable: ${closed}`,
  ).toBe(true)
  expect(at, 'in that order').toEqual([...at].sort((a, b) => a - b))
  await openForm(main)
  named.set(nameBox(main), en('instance-name-label'))
  const open = tabbable(main).map(nameOf)
  expect(
    open.slice(0, 4),
    'open, the pencil, the box, Save and Cancel are the first four stops',
  ).toEqual([
    en('instance-name-edit'),
    en('instance-name-label'),
    en('instance-name-save'),
    en('instance-label-cancel'),
  ])
  expect(
    open.indexOf(`${en('field-about')} ${en('instance-tags-own')}`),
    'the tags field comes after the run controls',
  ).toBeGreaterThan(open.indexOf(en('run-restart')))
})

test('16 17f2.8: the focus never drops: closing the form with its buttons aria-disabled keeps it on a control of the page', async () => {
  const gate = deferred()
  const { main } = await openPage({ instanceLabel: () => gate.promise })
  await openForm(main)
  typeIn(nameBox(main), 'Edge box')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('instance-name-saving'))
  fireEvent.keyDown(nameBox(main), { key: 'Escape' })
  await flushTasks()
  expect(
    document.activeElement,
    'the focus is on a real element, not the body',
  ).not.toBe(document.body)
  gate.resolve(reply(204))
  await flushTasks()
})

// ── 17f2.9: long text ─────────────────────────────────────────────────────

test('16 17f2.9 (the sheet): the title row wraps, the heading in it shrinks and breaks, and the pencil keeps its 40 px: a 64-character display name with no break point beside the pencil, and the 40-character id in instance-id-fixed, cause no sideways scroll (1.4.10)', async () => {
  const long = 'A'.repeat(64)
  const { main } = await openPage(
    { instances: [{ ...ITEM, label: long }] },
    long,
  )
  const row = titleRow(main)
  const at = merged(stylesFor(row))
  expect(norm(at['display'] ?? ''), 'the title row is a flex row').toBe('flex')
  expect(
    norm(at['flex-wrap'] ?? ''),
    'and wraps, so the pencil drops to the next line when the name fills it',
  ).toBe('wrap')
  const heading = merged(rulesInside('h1', [...row.classList]))
  expect(
    norm(heading['min-width'] ?? ''),
    'the heading is a flex item that can go below its content',
  ).toBe('0')
  expect(
    TOP.some(
      (r) =>
        r.compact === 'h1' &&
        norm(prop(r.body, 'overflow-wrap')) === 'anywhere',
    ),
    'the h1 breaks a name with no break point (the global h1 rule of 2d.7, kept by 17f2.9)',
  ).toBe(true)
  const pencilAt = merged(stylesFor(pencil(main)))
  expect(
    norm(pencilAt['flex'] ?? '') === 'none' ||
      norm(pencilAt['min-width'] ?? '') === '40px',
    'the pencil never shrinks below 40 px',
  ).toBe(true)
})

// ── 17f2 review (designer, 2026-11-25): D1, D2, D3 ────────────────────────

test('16 17f2 D1: the form is one column (minmax(0, 1fr))', () => {
  // The designer measured it in Chromium, Firefox and WebKit at 1280 and
  // 320 px: the form's hint is a `.field-hint`, which the 720 px field rule
  // places in column 2 (index.css, the block's `.field-control, .full-value,
  // .field-hint, .field-error`), so the form gains an implicit second column
  // and at 1280 px the label sits BESIDE the box (17f2.3 says above it) and
  // Save and Cancel drop under the label. One column of its own, capped at the
  // Server form's field width, is the measured fix.
  const cols = declsOf('.instance-name-form').filter((d) =>
    d.startsWith('grid-template-columns:'),
  )
  expect(
    cols.map((d) => d.replace(/\s+/g, '')),
    '17f2 review D1: one declaration sets the form’s columns, and it is the measured single column (the space inside minmax is the coder’s)',
  ).toEqual(['grid-template-columns:minmax(0,1fr)'])
})

test('16 17f2 D1: every child of the form sits in column 1', () => {
  // The column alone is not enough: the 720 px rule still places the
  // `.field-hint` (and anything else of the field shape) in column 2, so the
  // measured fix names every child.
  const rules = rulesFor('.instance-name-form > *')
  expect(
    rules.length,
    '17f2 review D1: one rule places every child of the form',
  ).toBe(1)
  expect(
    declsOf('.instance-name-form > *').filter((d) =>
      d.startsWith('grid-column:'),
    ),
    '17f2 review D1: every child in the form’s one column, never in a second one',
  ).toEqual(['grid-column: 1'])
})

test('16 17f2 D1b: the hint and error beat the 720 px field rule (grid-column: 1)', () => {
  // The designer's re-check (17f2 re-check, 2026-11-25): the rule above is
  // (0,1,0) and sits BEFORE the 720 px `.field-hint { grid-column: 2 }` (0,1,0),
  // so source order gives the later rule the win: the hint goes back to column 2
  // and column 1 collapses (columns "0px 636px"), leaving the box 18 px wide at
  // 720 px and wider. The measured fix names both children with a selector of
  // (0,2,0), which beats it whatever the source order.
  const rules = TOP.filter((r) => {
    const parts = r.sel.split(',').map((p) => selKey(p))
    return (
      parts.includes(selKey('.instance-name-form > .field-hint')) &&
      parts.includes(selKey('.instance-name-form > .field-error'))
    )
  })
  expect(
    rules.length,
    '17f2 re-check D1: one top-level rule names the hint AND the error of the form (the specificity, not the source order, is what beats the 720 px rule)',
  ).toBe(1)
  expect(
    rules
      .flatMap((r) => r.body.split(';').map(norm))
      .filter((d) => d.startsWith('grid-column:')),
    '17f2 re-check D1: that rule puts them in the form’s one column',
  ).toEqual(['grid-column: 1'])
})

test('16 17f2 D2: the hint breaks a long id (overflow-wrap: anywhere)', () => {
  // 17f2.9 (WCAG 1.4.10): with the form open, the hint holds the 40-character
  // id with no break point. Its width sets the grid column and the box
  // follows it: 342 to 346 px at 320 px (the page scrolls 38 to 42 px) and
  // 680 px at 200 % text (376 to 379 px). The break on the hint is what
  // lowers the column, the way 17f.8 gives it to the id line.
  const rules = rulesFor('.instance-name-form .field-hint')
  expect(
    rules.length,
    '17f2 review D2: one rule scopes the break to the hint of this form',
  ).toBe(1)
  expect(
    declsOf('.instance-name-form .field-hint').filter((d) =>
      d.startsWith('overflow-wrap:'),
    ),
    '17f2 review D2: the hint breaks the id where it has no break point',
  ).toEqual(['overflow-wrap: anywhere'])
})

test('16 17f2 D3: the form keeps room below it (margin-bottom: var(--space-3), max-width: 40rem)', () => {
  // Principle 9 (fe/DESIGN.md): the form ends flush against the engine, port
  // and state lines (0 px measured), so it keeps --space-3 under it as it has
  // above. The cap is 17f2.3's "up to the Server form's field width", which
  // the measured fix keeps at 40rem (640 px at 1280 px).
  const decls = declsOf('.instance-name-form')
  expect(
    decls.filter((d) => d.startsWith('margin-bottom:')),
    '17f2 review D3: the form is 12 px above the lines that follow it',
  ).toEqual(['margin-bottom: var(--space-3)'])
  expect(
    decls.filter((d) => d.startsWith('max-width:')),
    '17f2 review D3 (17f2.3): the form is no wider than the Server form’s field',
  ).toEqual(['max-width: 40rem'])
})

// ── 17f2.10: one word per thing, everywhere ───────────────────────────────

/** 17f2.10: the keys the strings file gains, with the English of the brief. */
const GAINS = [
  'instance-name-edit',
  'instance-id-fixed',
  'instance-name-label',
  'instance-name-hint',
  'instance-name-save',
  'instance-name-saving',
  'instance-name-saved',
  'instance-name-cleared',
]

/** 17f2.10: the keys the strings file loses with 17f’s Rename form. */
const LOST = [
  'instance-rename',
  'instance-label',
  'instance-label-hint',
  'instance-label-save',
  'instance-label-saving',
  'instance-label-saved',
  'instance-label-cleared',
]

test('16 17f2.10: en_US.json gains the 17f2 keys with the English of the brief, keeps instance-label-cancel, loses the seven of 17f’s Rename form, and says display name for the two refusals', () => {
  const table: Record<string, string> = strings
  const missing = GAINS.filter((key) => table[key] === undefined)
  expect(missing, 'keys missing from strings/en_US.json').toEqual([])
  const wrong = GAINS.filter((key) => table[key] !== en(key))
  expect(
    wrong,
    'keys whose en_US.json text is not the English of the brief',
  ).toEqual([])
  expect(
    table['instance-label-cancel'],
    'instance-label-cancel stays (17f2.10)',
  ).toBe(en('instance-label-cancel'))
  expect(
    Object.keys(table).filter((key) => LOST.includes(key)),
    'the keys 17f2.10 takes out of the strings file',
  ).toEqual([])
  // 17f2.10's first bullet, `instance-name` reading "ID", is 17c's create form
  // field: the form is not built and the key is not in the strings file, so
  // there is nothing here to read. 17c adds the key with the "ID" text.
  expect(table['api-instance-bad-label'], '17f2.7').toBe(
    en('api-instance-bad-label'),
  )
  // The file keeps the `{0}`/`{1}` placeholders (17f2.7 needs them): the
  // template is pinned raw on both sides, the fill where the text renders
  // (17f2.7's refusal test).
  expect(table['api-instance-label-taken'], '17f2.7').toBe(
    en('api-instance-label-taken'),
  )
})
