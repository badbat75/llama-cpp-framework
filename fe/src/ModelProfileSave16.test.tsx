// The end of sub-slice 2h.10 of fe/briefs/02h-model-profile-save.md (194, 195, 196,
// 197, 198, 198a): the read-only Notice `model-profile-partly-editable` is gone from
// the page and from the strings file while the controls 194 lists as greyed
// out stay disabled; the Assets card's intro paragraph
// `model-profile-assets-read-only` right after its h3; the section jump `<nav>` named
// `model-profile-jump-label` between the h2 and the Engine field, a `ul` of
// buttons named by the card titles, moving focus to a card's h3
// (`tabindex="-1"`, not a tab stop) and scrolling it into view with no hash
// change and no discard dialog while dirty; the tab order of the page's start;
// the strings of 198; and 198a: a 422 on a field whose control and default box
// are both disabled reads `save-refused-at` with the check text, the field's
// label and its card's title, while every other refusal keeps `save-refused`
// (143, 144). The cards, their fields and the reading of a value are 2e's, in
// ModelProfileCards*.test.tsx; the helpers are those of ModelProfileSave14 and
// ModelProfileSave15.test.tsx, which this file repeats (a test file imports no other
// test file).
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { t } from './strings.ts'
import {
  before,
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'
import { cardHeadings } from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const ENGINE = en('model-profile-engine')
/** 196: the cards in page order; the jump buttons take their titles. */
const TITLES = cardHeadings()
const ASSETS = TITLES[0]!
const KV = en('model-profile-kv-cache')
const SPEC = TITLES[TITLES.length - 1]!
/** 198 as H10.5 rewords it: the label the nav, the label p and the toggle share. */
const JUMP_LABEL = 'On this page'
const ASSETS_INTRO =
  'The files, Project URL and drafters of a model profile cannot be changed here yet. Change them in pai-config on the server machine.'
const REFUSED_AT = 'Not saved: {0}. The field is {1}, on the card {2}.'
/** 198's change (193): the draft group's line names the Assets row as the page
 * labels it, "Draft model". */
const DRAFT_OFF =
  'Used only with a draft: a Draft model or a Drafter, and this model profile has neither (Assets).'
/** 194: the Notice's text, out of the page and out of the strings file. */
const NOTICE =
  'Some settings cannot be changed here yet: their controls are greyed out. Change those in pai-config on the server machine.'
/** 198a: the brief's example, a file the manager refuses whole. */
const MOE_SPLIT = en('api-moe-cache-with-tensor-split')

/** A read that locks every control 194 lists as greyed out: no `parallel`
 * (Unified KV, 155), no `mmproj` (the four Vision controls, 176), no
 * `model-draft` (the draft pair, 177, and the draft group, 190), `spec-type`
 * `none` (every n-gram group, 191), `fit` off (the two Fit dependents, 186),
 * and the two MoE rows, never editable in 2h (178). `moe-cache-mib` and
 * `split-mode` are 198a's example. */
const BASE: Record<string, string> = {
  'ctx-size': '172032',
  'moe-cache-mib': '512',
  'split-mode': 'tensor',
  fit: 'off',
  'spec-type': 'none',
  embeddings: 'true',
  pooling: 'mean',
}

let dialogs: ReturnType<typeof stubDialog>
/** jsdom has no scrollIntoView: record the elements it is called on. */
const scrolled: Element[] = []

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
  scrolled.length = 0
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this)
  }
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers (as ModelProfileSave14 and ModelProfileSave15.test.tsx) ────────────────

/** Waits for the page read with revision `r1` and the values `values`. */
async function form(
  values: Record<string, string> = BASE,
  options: BffOptions = {},
) {
  const bff = fakeBff({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...values } }),
      ),
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: ASSETS })
  return { bff, main }
}

/** A card's h3, by its title. */
function headingOf(main: HTMLElement, title: string): HTMLElement {
  return within(main).getByRole('heading', { level: 3, name: title })
}

/** A card's `section`, from its h3 (Card.tsx). */
function cardOf(main: HTMLElement, title: string): HTMLElement {
  const h3 = headingOf(main, title)
  const card = h3.closest('section')
  expect(card, `${title}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The jump nav, found by its name (196). */
function navOf(main: HTMLElement): HTMLElement {
  return within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
}

/** The jump buttons, in the list's order. H10.1 puts the disclosure in the nav
 * too, so the ten entries are read from the list the toggle controls. */
function jumpButtons(main: HTMLElement): HTMLElement[] {
  const list = navOf(main).querySelector('ul')
  expect(list, '196, H10.1: the nav holds the ul of entries').not.toBeNull()
  return within(list as HTMLElement).getAllByRole('button')
}

/** The Engine select, by its label. */
function engineSelect(main: HTMLElement): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: ENGINE,
  }) as HTMLSelectElement
}

/** A field's control by the key of its row: `model-profile-<key>-control`. */
function control(key: string): HTMLInputElement | HTMLSelectElement {
  const el = document.getElementById(`model-profile-${key}-control`)
  expect(el, `the control of ${key}`).not.toBeNull()
  return el as HTMLInputElement | HTMLSelectElement
}

/** A number field's default box. */
function boxOf(key: string): HTMLInputElement {
  const el = document.getElementById(`model-profile-${key}-box`)
  expect(el, `the default box of ${key}`).not.toBeNull()
  return el as HTMLInputElement
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function pick(main: HTMLElement, label: string, value: string) {
  const el = within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
  fireEvent.change(el, { target: { value } })
}

/** A card checkbox, named by its own toggle text (87), e.g. the embedded
 * chat template. Its checked state is in the draft, not the box state. */
function checkBoxOf(main: HTMLElement, toggle: string): HTMLInputElement {
  return within(main).getByRole('checkbox', {
    name: toggle,
  }) as HTMLInputElement
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** Clicks Save changes and waits for the PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** The save bar's status line (the only role="status" after the h2). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line once the page is loaded').toHaveLength(1)
  return all[0]!
}

/** The bar's alert reads `text` (exact): its own role="alert", after the
 * status line and before the buttons (01's 30a). */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
  const line = status(main)
  expect(line.contains(alert), `${what}: not inside the status line`).toBe(
    false,
  )
  expect(before(line, alert!), `${what}: after the status line`).toBe(true)
  expect(
    before(alert!, discardButton(main)),
    `${what}: before the buttons`,
  ).toBe(true)
}

/** The ids of `aria-describedby`, in order, and their text. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** Asserts `el` is marked invalid with the error `text` under it (142, 144). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
}

/** The elements Tab reaches inside `root`, in DOM order (91, 196, 197). The
 * elements, not their names: a jump button carries a card title (196), so a
 * name cannot tell a card's h3 from a button. */
function tabbableElements(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

/** Their names, in DOM order (91, 197). */
function mainTabbable(root: HTMLElement): string[] {
  return tabbableElements(root).map((el) => nameOf(el))
}

/** The name Tab shows: a control's text, or the label that names an input. */
function nameOf(el: HTMLElement): string {
  if (!['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName))
    return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((part) => document.getElementById(part)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const id = el.getAttribute('id')
  const label = id ? document.querySelector(`label[for="${id}"]`) : null
  const wrap = el.closest('label')
  return (label?.textContent ?? wrap?.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

// ── 194: the Notice goes, and nothing new is enabled ──────────────────

test('2h.194: the paragraph model-profile-partly-editable is gone from the page, and no read-only sentence of 2e is shown', async () => {
  const { main } = await form()
  expect(
    within(main).queryByText(NOTICE),
    '194: the Notice of 2h.1 is off the page',
  ).toBeNull()
  expect(
    within(main).queryByText(/greyed out/),
    '194: no sentence about greyed-out controls',
  ).toBeNull()
  // `model-profile-read-only` went in 2h.1: the key shows as itself, so no text of it.
  expect(
    t('model-profile-read-only'),
    'model-profile-read-only is out of the strings',
  ).toBe('model-profile-read-only')
})

test('2h.194: this sub-slice enables nothing new: every control 194 lists as greyed out is disabled on a read that locks it, and a ticked default box still disables its input', async () => {
  const { main } = await form()
  const locked = [
    'kv-unified',
    'mmproj-offload',
    'mmproj-device',
    'image-min-tokens',
    'image-max-tokens',
    'device-draft',
    'n-gpu-layers-draft',
    'n-cpu-moe',
    'moe-cache-mib',
    'fit-ctx',
    'fit-target',
    'spec-draft-n-max',
    'spec-draft-type-k',
    'spec-ngram-mod-n-match',
  ]
  for (const key of locked)
    expect(control(key).disabled, `${key}: greyed out (194)`).toBe(true)
  // The Engine select keeps 2e's shape: enabled, with its one option.
  const engine = engineSelect(main)
  expect(engine.disabled, 'the Engine select stays enabled').toBe(false)
  expect(engine.options.length, 'its one option').toBe(1)
  // A field with its own default box: the box enabled, the input disabled
  // while the box is ticked (156).
  const box = boxOf('ctx-size')
  fireEvent.click(box)
  expect(box.disabled, 'the default box stays enabled').toBe(false)
  expect(
    control('ctx-size').disabled,
    '156: a ticked box disables its input',
  ).toBe(true)
})

// ── 195: the Assets card says why it is text ──────────────────────────

test('2h.195: the Assets card intro model-profile-assets-read-only is the first thing after its h3, before the list, and names the files, the Project URL and the drafters', async () => {
  const { main } = await form()
  const card = cardOf(main, ASSETS)
  const h3 = headingOf(main, ASSETS)
  const paras = [...card.querySelectorAll('p')]
  expect(paras.length, 'the card has one paragraph (D5.2)').toBe(1)
  const intro = paras[0]!
  expect(intro.textContent, 'the intro of 195').toBe(
    en('model-profile-assets-read-only'),
  )
  expect(isHidden(intro), 'the intro is shown').toBe(false)
  expect(before(h3, intro), 'the intro follows the h3').toBe(true)
  const list = card.querySelector('dl')
  expect(list, 'the Assets list').not.toBeNull()
  expect(before(intro, list!), 'the intro is before the list').toBe(true)
})

// ── 196: the section jump list ────────────────────────────────────────

test('2h.196: the jump nav named model-profile-jump-label sits between the h2 and the Engine field, and is a ul of ten buttons named by the ten card titles in card order, no link', async () => {
  const { main } = await form()
  const nav = navOf(main)
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  expect(nav.tagName, '196: a nav element').toBe('NAV')
  expect(before(h2, nav), 'the nav follows the h2').toBe(true)
  expect(
    before(nav, engineSelect(main)),
    'the nav is before the Engine field',
  ).toBe(true)
  const list = nav.querySelector('ul')
  expect(list, 'a ul').not.toBeNull()
  const buttons = jumpButtons(main)
  expect(buttons.length, 'one button per card').toBe(TITLES.length)
  expect(
    buttons.map((b) => (b.textContent ?? '').replace(/\s+/g, ' ').trim()),
    'each named by its card title, in card order',
  ).toEqual(TITLES)
  for (const b of buttons) {
    expect(b.tagName, 'a button, not a link').toBe('BUTTON')
    expect(list!.contains(b), `${b.textContent}: inside the ul`).toBe(true)
  }
  expect(nav.querySelectorAll('a[href]').length, 'no in-page link').toBe(0)
})

test.each([
  ['the first card', ASSETS, 0],
  ['a middle card', KV, 4],
  ['the last card', SPEC, TITLES.length - 1],
])(
  '2h.196: activating the %s jump button moves focus to that card h3 and scrolls it into view, and location.hash is unchanged',
  async (what, title, n) => {
    const { main } = await form()
    const h3 = headingOf(main, title)
    expect(h3.getAttribute('tabindex'), `${title}: the h3 takes focus`).toBe(
      '-1',
    )
    const button = jumpButtons(main)[n]!
    fireEvent.click(button)
    expect(document.activeElement, `${what}: focus on the h3`).toBe(h3)
    expect(scrolled, `${what}: scrollIntoView was called on the h3`).toContain(
      h3,
    )
    expect(window.location.hash, `${what}: the hash is untouched`).toBe(HASH)
    expect(dialogs.opened.length, `${what}: no dialog`).toBe(0)
  },
)

test('2h.196: with a dirty form a jump opens no discard dialog, keeps the edit, moves focus to the card h3 and leaves the hash alone', async () => {
  const { main } = await form()
  const ctx = control('ctx-size') as HTMLInputElement
  type(ctx, '65536')
  const button = jumpButtons(main)[4]!
  fireEvent.click(button)
  const h3 = headingOf(main, KV)
  expect(dialogs.opened.length, '196: no dialog while dirty').toBe(0)
  expect(document.activeElement, 'focus on the KV cache h3').toBe(h3)
  expect(scrolled, 'the card scrolled into view').toContain(h3)
  expect(window.location.hash, 'no route change, so no guard').toBe(HASH)
  expect(
    (control('ctx-size') as HTMLInputElement).value,
    'the draft is kept',
  ).toBe('65536')
  expect(status(main).textContent, 'the form is still dirty').toBe(
    en('save-unsaved'),
  )
})

test('2h.196: a dirty form built from a select, a checkbox and a number on three cards survives a jump: all three controls keep the changed value, the form stays dirty, no dialog', async () => {
  // The select and the checkbox live in the draft; the number lives in the
  // box state. A jump that reset the draft to the loaded values (leaving the
  // boxes alone) puts the select and the checkbox back, so the value asserts
  // on them are the ones that catch it: the boxed number alone (the test
  // above) cannot, its value is not in the draft.
  const { main } = await form()
  const pooling = en('model-profile-pooling')
  const jinja = en('model-profile-chat-template-toggle')
  // A select on the Embeddings card, a checkbox on the Chat / reasoning card,
  // a boxed number on the Resource / context card: three different cards.
  pick(main, pooling, 'cls')
  fireEvent.click(checkBoxOf(main, jinja))
  type(control('ctx-size'), '65536')
  // The three edits are there before the jump.
  expect(
    (within(main).getByRole('combobox', { name: pooling }) as HTMLSelectElement)
      .value,
    'the select picked cls',
  ).toBe('cls')
  expect(checkBoxOf(main, jinja).checked, 'the checkbox was unticked').toBe(
    false,
  )
  fireEvent.click(jumpButtons(main)[4]!)
  expect(dialogs.opened.length, '196: no dialog while dirty').toBe(0)
  expect(window.location.hash, '196: no route change, so no guard').toBe(HASH)
  expect(
    (within(main).getByRole('combobox', { name: pooling }) as HTMLSelectElement)
      .value,
    '196: the select keeps cls, its draft survived the jump',
  ).toBe('cls')
  expect(
    checkBoxOf(main, jinja).checked,
    '196: the checkbox keeps the untick, its draft survived the jump',
  ).toBe(false)
  expect(
    (control('ctx-size') as HTMLInputElement).value,
    '196: the number keeps what was typed',
  ).toBe('65536')
  expect(status(main).textContent, 'the form is still dirty').toBe(
    en('save-unsaved'),
  )
})

test('2h.196: no card h3 is a tab stop: each carries tabindex -1 and none is in the elements Tab reaches', async () => {
  const { main } = await form()
  const tabbed = tabbableElements(main)
  for (const title of TITLES) {
    const h3 = headingOf(main, title)
    expect(h3.getAttribute('tabindex'), `${title}: tabindex -1`).toBe('-1')
    // Compared by element: the jump button of 196 carries this very title.
    expect(tabbed, `${title}: not a tab stop`).not.toContain(h3)
  }
})

// ── 197: the tab order of the page start ──────────────────────────────

test('2h.197: Tab from the top reaches the back link, then the jump toggle of H10.10, then the ten jump buttons in card order, then the Engine field', async () => {
  const { main } = await form()
  const tabbed = mainTabbable(main)
  expect(
    tabbed.slice(0, TITLES.length + 4),
    '197 with H10.10: back, the disclosure, the ten jumps in card order, the Engine About toggle and the Engine select',
  ).toEqual([
    en('model-profile-back'),
    en('model-profile-jump-label'),
    ...TITLES,
    `${en('field-about')} ${ENGINE}`,
    ENGINE,
  ])
})

// ── 198: the strings, and the changed one ─────────────────────────────

test('2h.198: the two new strings and the changed model-profile-draft-group-off are the brief English in the test table and in the fe strings file, and model-profile-partly-editable is out of it', () => {
  expect(en('model-profile-jump-label'), '196, 198: the nav name').toBe(
    JUMP_LABEL,
  )
  expect(t('model-profile-jump-label'), '198: in the fe strings file').toBe(
    JUMP_LABEL,
  )
  expect(en('model-profile-assets-read-only'), '195: the intro').toBe(
    ASSETS_INTRO,
  )
  expect(
    t('model-profile-assets-read-only'),
    '195: in the fe strings file',
  ).toBe(ASSETS_INTRO)
  expect(en('save-refused-at'), '198a: the alert').toBe(REFUSED_AT)
  expect(t('save-refused-at'), '198a: in the fe strings file').toBe(REFUSED_AT)
  expect(en('model-profile-draft-group-off'), '193 as revised in 198').toBe(
    DRAFT_OFF,
  )
  expect(
    t('model-profile-draft-group-off'),
    '198: in the fe strings file',
  ).toBe(DRAFT_OFF)
  expect(
    t('model-profile-partly-editable'),
    '198: the removed Notice shows as itself',
  ).toBe('model-profile-partly-editable')
})

test('2h.198: the page shows the new strings: the nav name, the Assets intro, and the draft group line naming the Draft model row', async () => {
  const { main } = await form()
  expect(
    navOf(main).getAttribute('aria-labelledby'),
    'H10.1: the nav is named by the label p, and carries no aria-label',
  ).toBe('model-profile-jump-label')
  expect(
    navOf(main).getAttribute('aria-label'),
    'H10.1: no aria-label: the label p and the toggle carry the one text',
  ).toBeNull()
  expect(
    within(main)
      .getAllByText(JUMP_LABEL)
      .map((el) => el.tagName),
    '198 as H10.5 rewords it: one text, two places (H10.1: the label p and the toggle)',
  ).toEqual(['P', 'BUTTON'])
  expect(
    within(main).getByText(ASSETS_INTRO),
    'the Assets intro on the page',
  ).toBeTruthy()
  expect(
    within(cardOf(main, SPEC)).getByText(DRAFT_OFF),
    '193 as revised: the line on the page',
  ).toBeTruthy()
})

// ── 198a: a refusal on a field that cannot take focus says where it is ──

/** A 422 `body`, on a form made dirty by `dirty`, from the BASE read. */
async function refused(
  body: Record<string, unknown>,
  dirty: (main: HTMLElement) => void,
) {
  const { bff, main } = await form(BASE, {
    modelProfileSave: () => reply(422, body),
  })
  dirty(main)
  saveButton(main).focus()
  await save(main, bff)
  return { bff, main }
}

test('2h.198a: a 422 naming moe-cache-mib, a settings field whose control and box are both disabled, reads save-refused-at with the check text, the label MoE cache and the card Hardware config, and the focus stays on Save changes', async () => {
  const { main } = await refused(
    {
      key: 'moe-cache-with-tensor-split',
      args: [],
      field: 'moe-cache-mib',
    },
    () => type(control('ctx-size'), '65536'),
  )
  const at = en(
    'save-refused-at',
    MOE_SPLIT,
    en('model-profile-moe-cache'),
    en('model-profile-hardware-config'),
  )
  await within(main).findByText(at)
  expectAlert(main, at, '198a: where the field is')
  expect(
    within(main).queryByText(en('save-refused', MOE_SPLIT)),
    '198a: not the bar alert of 144 alone',
  ).toBeNull()
  expect(document.activeElement, '144: focus stays on Save changes').toBe(
    saveButton(main),
  )
  // 144 keeps the mark on the field, as on any field.
  expectError(control('moe-cache-mib'), MOE_SPLIT, 'MoE cache (144)')
  expect(status(main).textContent, 'the draft stays').toBe(en('save-unsaved'))
})

test('2h.198a: a 422 naming pooling, a field that can take focus, keeps save-refused and focuses the field (144)', async () => {
  const body = {
    key: 'not-one-of',
    args: ['pooling', 'weird', 'default, none, mean'],
    field: 'pooling',
  }
  const { main } = await refused(body, (main) =>
    pick(main, en('model-profile-pooling'), 'rank'),
  )
  const text = en('api-not-one-of', ...body.args)
  await within(main).findByText(en('save-refused', text))
  expectAlert(main, en('save-refused', text), '144: the plain alert')
  expect(
    within(main).queryByText(
      en('save-refused-at', text, 'Pooling', 'Embeddings'),
    ),
    '198a: no where for a field that can take focus',
  ).toBeNull()
  const pool = within(main).getByRole('combobox', {
    name: en('model-profile-pooling'),
  })
  expect(document.activeElement, '144: focus on the enabled control').toBe(pool)
  expectError(pool, text, 'Pooling (144)')
})

test('2h.198a: a 422 on a field whose default box is ticked keeps save-refused and focuses the box (144): the where of 198a is only for a field with nothing to focus', async () => {
  const body = {
    key: 'not-whole-number-in-range',
    args: ['ctx-size', '-1', '0', '2147483647'],
    field: 'ctx-size',
  }
  const { main } = await refused(body, () => {
    // The edit first, then the box: a ticked box empties and disables its
    // input (156), so the dirty form is the box itself.
    type(control('ctx-size'), '-1')
    fireEvent.click(boxOf('ctx-size'))
  })
  const text = en('api-not-whole-number-in-range', ...body.args)
  await within(main).findByText(en('save-refused', text))
  expectAlert(main, en('save-refused', text), '144: the plain alert')
  expect(document.activeElement, '144, 161: focus on the default box').toBe(
    boxOf('ctx-size'),
  )
})

test('2h.198a: a 422 naming an Assets key keeps save-refused, marks nothing and leaves the focus on Save changes (143)', async () => {
  const { main } = await refused(
    { key: 'model-profile-no-model', args: [ID], field: 'model' },
    () => type(control('ctx-size'), '65536'),
  )
  const text = en('api-model-profile-no-model', ID)
  await within(main).findByText(en('save-refused', text))
  expectAlert(main, en('save-refused', text), '143: the Assets key')
  const invalid = [...main.querySelectorAll('[aria-invalid="true"]')]
  expect(
    invalid.map((e) => e.id),
    '143: no field marked',
  ).toEqual([])
  expect(document.activeElement, '143: focus stays on Save changes').toBe(
    saveButton(main),
  )
})
