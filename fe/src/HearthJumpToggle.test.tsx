// Hearth H10, the model profile page's jump list under a visible "On this page"
// label, collapsed behind a disclosure below 1280 px
// (fe/briefs/08-hearth-h10-h11.md, section 1): the DOM half. H10.1 to H10.5,
// H10.8, H10.9 (the DOM part: the classes and attributes the global target and
// ring rules need) and H10.10 are pinned here on the rendered page; H10.6 and
// H10.7 are stylesheet declarations, pinned in HearthH10.test.ts, and the
// toggle's focus ring and the chevron's turn are for the designer's UX pass
// (section 5).
//
// The nav is located as `main.querySelector('nav.jump')`: H10.1 replaces the
// `aria-label` H9a gave it with `aria-labelledby` the label `p`, so a role
// query by name depends on the accessible-name computation reaching a referenced
// element the CSS hides (section 0). The name is pinned by its attributes, and
// once also as a role query, which is the reading a screen reader takes.
//
// The ten entries' names, order, focus target and class-free buttons are
// section 6's "must not change": pinned here as guards, and by 2h.196 and
// 2h.197 of ModelProfileSave16.test.tsx.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import { t } from './strings.ts'
import {
  before,
  en,
  fakeBff,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'
import { cardHeadings } from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
/** H10.1: the three ids the slice gives the label, the toggle and the list. */
const LABEL_ID = 'model-profile-jump-label'
const TOGGLE_ID = 'model-profile-jump-toggle'
const LIST_ID = 'model-profile-jump-list'
/** H10.5: the English the brief gives the key. */
const LABEL = 'On this page'
/** H10.1: the chevron the disclosure carries (Icon.tsx, H8a's shape). */
const CHEVRON = 'm6 9 6 6 6-6'
/** Section 1: the cards in page order; the ten entries take their titles. */
const TITLES = cardHeadings()
const MIDDLE = 4

let scrolled: Element[] = []

beforeEach(() => {
  startAt(HASH)
  scrolled = []
  // jsdom has no scrollIntoView; the jump calls it on the heading.
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this)
  }
})

afterEach(() => {
  cleanup()
  startAt('')
})

/** A read of the model profile, awaited to the h1 and the first card's h3. */
async function form(options: BffOptions = {}) {
  const bff = fakeBff({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc(
          {},
          { revision: 'r1', values: { 'ctx-size': '172032' } },
        ),
      ),
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: TITLES[0] })
  return { bff, main }
}

/** The element H10.1 gives the page, by its id, so a miss names the id. */
function byId(id: string, what: string): HTMLElement {
  const el = document.getElementById(id)
  expect(el, `${what}: #${id}`).not.toBeNull()
  return el as HTMLElement
}

/** The jump nav, located by its class (the header note on the name). */
function navOf(main: HTMLElement): HTMLElement {
  const nav = main.querySelector('nav.jump')
  expect(nav, 'the page renders nav.jump (section 6: the nav)').not.toBeNull()
  return nav as HTMLElement
}

/** The toggle. */
const toggleOf = (): HTMLElement => byId(TOGGLE_ID, 'the disclosure')

/** The list. */
const listOf = (): HTMLElement => byId(LIST_ID, 'the jump list')

/** The ten entries, in the list's order. */
function entries(): HTMLElement[] {
  return within(listOf()).getAllByRole('button')
}

const textOf = (el: HTMLElement): string =>
  (el.textContent ?? '').replace(/\s+/g, ' ').trim()

/** The list's state, as the two attributes and the class H10.1 pins them. */
function state(): [string | null, string[]] {
  return [
    toggleOf().getAttribute('aria-expanded'),
    Array.from(listOf().classList),
  ]
}

/** The list open: the toggle's expanded state and the class gone. */
function expectOpen(): void {
  const [expanded, classes] = state()
  expect(expanded, 'H10.3: the toggle says the list is open').toBe('true')
  expect(classes, 'H10.1: an open list carries no class').toEqual([])
}

/** The list closed: as the page mounts it, and as a jump leaves it. */
function expectClosed(): void {
  const [expanded, classes] = state()
  expect(expanded, 'the toggle says the list is closed').toBe('false')
  expect(classes, 'H10.1: a closed list carries the collapsed class').toEqual([
    'collapsed',
  ])
}

// ── H10.1: the DOM ────────────────────────────────────────────────────────

test('H10.1: nav.jump is labelled by p#model-profile-jump-label.jump-label and has no aria-label; then button#model-profile-jump-toggle.jump-toggle.secondary of type button, aria-controls the list and aria-expanded false, naming itself with the label and the chevron-down Icon after it; then ul#model-profile-jump-list.collapsed holding the ten class-free buttons in card order', async () => {
  const { main } = await form()
  const nav = navOf(main)
  expect(nav.tagName, 'section 6: the nav').toBe('NAV')
  expect(
    nav.getAttribute('aria-labelledby'),
    'H10.1: labelled by the label p',
  ).toBe(LABEL_ID)
  expect(
    nav.getAttribute('aria-label'),
    'H10.1: the aria-label of H9a is gone: one text, two places',
  ).toBeNull()
  // H10.5: the nav is still addressed by the key's English, which is how every
  // older test finds it: the name comes from the referenced element.
  expect(
    within(main).getByRole('navigation', {
      name: en('model-profile-jump-label'),
    }),
    'H10.5: the nav is named by model-profile-jump-label',
  ).toBe(nav)

  const kids = Array.from(nav.children)
  expect(
    kids.map((k) => `${k.tagName}#${k.id}`),
    'H10.1: the label p, then the button, then the ul, in that order',
  ).toEqual([`P#${LABEL_ID}`, `BUTTON#${TOGGLE_ID}`, `UL#${LIST_ID}`])

  const label = kids[0] as HTMLElement
  expect(label.tagName, 'H10.1: the label is a p').toBe('P')
  expect(
    Array.from(label.classList),
    'H10.1: the label carries the jump-label class',
  ).toEqual(['jump-label'])
  expect(
    textOf(label),
    'H10.1: the label shows the key, whose English is "On this page"',
  ).toBe(LABEL)

  const toggle = kids[1] as HTMLElement
  expect(toggle.tagName, 'H10.1: the disclosure is a button').toBe('BUTTON')
  expect(
    Array.from(toggle.classList).sort(),
    'H10.1: the toggle is .jump-toggle and .secondary (H10.9: the 40 px pill)',
  ).toEqual(['jump-toggle', 'secondary'])
  expect(
    toggle.getAttribute('type'),
    'section 0: a native button, so Enter and Space come with it',
  ).toBe('button')
  expect(
    toggle.getAttribute('aria-controls'),
    'H10.1: it controls the list',
  ).toBe(LIST_ID)
  expect(toggle.getAttribute('aria-expanded'), 'H10.2: closed on mount').toBe(
    'false',
  )
  expect(
    textOf(toggle),
    'H10.1: one key, two places: the toggle names itself with the label',
  ).toBe(LABEL)
  const icon = toggle.querySelector('svg.icon')
  expect(icon, 'H10.1: the toggle carries the Icon chevron-down').not.toBeNull()
  expect(
    icon!.getAttribute('aria-hidden'),
    'an Icon is decoration: hidden from the accessibility tree',
  ).toBe('true')
  expect(
    icon!.querySelector('path')?.getAttribute('d'),
    'the chevron-down shape of H8a',
  ).toBe(CHEVRON)
  expect(
    Array.from(toggle.children).indexOf(icon as unknown as Element),
    'section 0: the chevron comes after the text, unlike H8 (the turn shows the state)',
  ).toBe(toggle.children.length - 1)

  const list = kids[2] as HTMLElement
  expect(list.tagName, 'H10.1: the list is a ul').toBe('UL')
  expect(
    Array.from(list.classList),
    'H10.2: closed on mount, by a class and not by hidden (section 0: the rail shows it at 1280 px)',
  ).toEqual(['collapsed'])
  expect(
    list.hasAttribute('hidden'),
    'H10.1: hidden is not used, so the 1280 px rail wins over the state',
  ).toBe(false)
  const buttons = entries()
  expect(buttons.length, 'section 6: one entry per card').toBe(TITLES.length)
  expect(
    buttons.map(textOf),
    'section 6: each entry named by its card title, in card order',
  ).toEqual(TITLES)
  for (const b of buttons) {
    expect(b.tagName, 'section 6: a button, not a link').toBe('BUTTON')
    expect(b.getAttribute('type'), 'section 6: type=button').toBe('button')
    expect(
      b.getAttribute('class'),
      'H9a.1: an entry carries no class',
    ).toBeNull()
    expect(b.parentElement?.parentElement, 'each entry is in the ul').toBe(list)
  }
  expect(
    nav.querySelectorAll('a[href]').length,
    'section 6: no in-page link',
  ).toBe(0)
})

// ── H10.2: closed on each mount ──────────────────────────────────────────

test('H10.2: the list is closed on every mount of the form, and a mount after the toggle was used starts closed too (no memory of the state)', async () => {
  const first = await form()
  expectClosed()
  fireEvent.click(toggleOf())
  expectOpen()
  // A second mount of the page: the state of the first one is not remembered
  // (section 0: no storage for a view state).
  cleanup()
  startAt(HASH)
  await form()
  expectClosed()
  expect(first.main.isConnected, 'the first page is gone').toBe(false)
})

// ── H10.3: the toggle ────────────────────────────────────────────────────

test('H10.3: activating the toggle opens the list and the focus stays on the toggle, and activating it again closes it; the page adds no live region, so nothing beyond the expanded state is announced', async () => {
  const { main } = await form()
  const toggle = toggleOf()
  fireEvent.click(toggle)
  expectOpen()
  expect(document.activeElement, 'H10.3: the focus stays on the toggle').toBe(
    toggle,
  )
  fireEvent.click(toggle)
  expectClosed()
  expect(
    document.activeElement,
    'H10.3: closing keeps the focus where the user left it',
  ).toBe(toggle)
  // H10.3: no live region: the expanded state is the whole announcement. The
  // scan is the nav's: the save bar carries a role="status" of its own as
  // today (SaveBar.tsx), which is not the disclosure announcing.
  expect(
    navOf(main).querySelectorAll(
      '[aria-live], [role="status"], [role="alert"]',
    ),
    'H10.3: the disclosure adds no live region: nothing beyond the expanded state is announced',
  ).toHaveLength(0)
  // Enter and Space are what the native button gives (section 0, WAI-ARIA APG
  // Disclosure): fireEvent fires no key activation in jsdom, so the type is the
  // proxy the test can reach.
  expect(
    toggle.getAttribute('type'),
    'H10.3: a type=button gives Enter and Space (reported as open for the UX pass)',
  ).toBe('button')
})

// ── H10.4: a jump closes it ──────────────────────────────────────────────

test('H10.4: opening the list and activating an entry moves focus to that card h3, scrolls it into view, leaves the hash alone, closes the list, and asks nothing while the form is dirty', async () => {
  const { main } = await form()
  const title = TITLES[MIDDLE]!
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  expect(h3.getAttribute('tabindex'), `${title}: the h3 takes focus`).toBe('-1')
  const ctx = document.getElementById(
    'model-profile-ctx-size-control',
  ) as HTMLInputElement
  fireEvent.change(ctx, { target: { value: '65536' } })
  expect(ctx.value, 'the form is dirty before the jump').toBe('65536')

  fireEvent.click(toggleOf())
  expectOpen()
  fireEvent.click(entries()[MIDDLE]!)
  expect(document.activeElement, `${title}: focus on the card h3`).toBe(h3)
  expect(scrolled, `${title}: scrolled into view`).toContain(h3)
  expect(window.location.hash, 'a jump writes no hash').toBe(HASH)
  expectClosed()
  expect(
    (
      document.getElementById(
        'model-profile-ctx-size-control',
      ) as HTMLInputElement
    ).value,
    'H10.4: the jump keeps the edit',
  ).toBe('65536')
  expect(
    screen.queryByRole('dialog'),
    'H10.4: a dirty form asks nothing (as today)',
  ).toBeNull()
  expect(
    screen.queryByRole('alertdialog'),
    'H10.4: and opens no alert dialog',
  ).toBeNull()
})

// The same criterion read as an ORDER, the correction of the review (section 7):
// closing the list after the scroll pulls the heading out of view, so the close
// must be in the DOM before the focus moves and before the scroll runs. The two
// spies record the list's class at the moment each call happens, which is the
// moment the review measured.

test('H10.4 order: the list is closed in the DOM before the focus moves and before the scroll runs (the corrected order of section 7)', async () => {
  const { main } = await form()
  const title = TITLES[MIDDLE]!
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  // Was the list closed, in the DOM, at the moment of each call?
  const closed = () => listOf().classList.contains('collapsed')
  const focusSaw: boolean[] = []
  const scrollSaw: boolean[] = []
  h3.focus = () => {
    focusSaw.push(closed())
    HTMLElement.prototype.focus.call(h3)
  }
  h3.scrollIntoView = () => {
    scrollSaw.push(closed())
    Element.prototype.scrollIntoView.call(h3)
  }
  fireEvent.click(toggleOf())
  expectOpen()
  fireEvent.click(entries()[MIDDLE]!)
  // Both spies ran, so the two records below are a jump, not an empty list.
  expect(focusSaw, `${title}: the jump moved the focus to the h3`).toHaveLength(
    1,
  )
  expect(
    scrollSaw,
    `${title}: the jump scrolled the h3 into view`,
  ).toHaveLength(1)
  expect(scrolled, `${title}: the scroll reached the heading`).toContain(h3)
  expect(
    focusSaw[0],
    'H10.4: the list already had the collapsed class when the focus moved',
  ).toBe(true)
  expect(
    scrollSaw[0],
    'H10.4: and when the scroll ran (closing after the scroll pulls the heading away)',
  ).toBe(true)
  // What the jump leaves is unchanged: the focus on the h3, the list closed.
  expect(document.activeElement, `${title}: focus on the card h3`).toBe(h3)
  expectClosed()
})

// ── H10.5: the strings ───────────────────────────────────────────────────

test('H10.5: model-profile-jump-label is "On this page" in the test table and in the fe strings file, and H10 adds no key: the label and the toggle share it', async () => {
  expect(en('model-profile-jump-label'), 'H10.5: in the test table').toBe(LABEL)
  expect(t('model-profile-jump-label'), 'H10.5: in en_US.json').toBe(LABEL)
  // No key added: the toggle has no text of its own, so a key named for it is
  // absent and t() shows it as itself.
  expect(
    t('model-profile-jump-toggle'),
    'H10.5: no key added for the toggle',
  ).toBe('model-profile-jump-toggle')
  const { main } = await form()
  expect(textOf(byId(LABEL_ID, 'the label')), 'the label on the page').toBe(
    LABEL,
  )
  expect(textOf(toggleOf()), 'and the toggle shows it too').toBe(LABEL)
  expect(navOf(main).childElementCount, 'the nav holds the three').toBe(3)
})

// ── H10.8: order and reading ─────────────────────────────────────────────

test('H10.8: the label, the toggle and the list follow each other in the DOM in that reading order, the nav is a navigation region named by the label, and the entries come after the toggle', async () => {
  const { main } = await form()
  const nav = navOf(main)
  const label = byId(LABEL_ID, 'the label')
  const toggle = toggleOf()
  const list = listOf()
  expect(
    [nav.tagName, label.tagName, toggle.tagName, list.tagName],
    'H10.8: navigation, label, button, list',
  ).toEqual(['NAV', 'P', 'BUTTON', 'UL'])
  expect(before(label, toggle), 'H10.8: the label is before the toggle').toBe(
    true,
  )
  expect(before(toggle, list), 'H10.8: the list follows its toggle').toBe(true)
  expect(
    before(
      nav,
      within(main).getByRole('heading', { level: 3, name: TITLES[0] }),
    ),
    'H10.8: the whole nav comes before the cards it points at',
  ).toBe(true)
  expect(
    within(main).getByRole('navigation', {
      name: en('model-profile-jump-label'),
    }),
    'H10.8: "On this page, navigation"',
  ).toBe(nav)
  const buttons = entries()
  expect(before(toggle, buttons[0]!), 'the toggle precedes the entries').toBe(
    true,
  )
  expect(
    buttons.every((b) => list.contains(b)),
    'the entries are all in the list the toggle controls',
  ).toBe(true)
})

// ── H10.9: the DOM half of the target and the ring ───────────────────────

test('H10.9: the toggle carries the secondary class that makes it the 40 px pill and no tabindex, so the global button min-height, the .secondary pill and the global :focus-visible ring all reach it', async () => {
  await form()
  const toggle = toggleOf()
  expect(
    toggle.classList.contains('secondary'),
    'H10.9: the .secondary pill: border, pill radius, --bg and --text',
  ).toBe(true)
  expect(
    toggle.getAttribute('tabindex'),
    'H10.9: no tabindex: [tabindex="-1"]:focus would take the ring away',
  ).toBeNull()
  expect(
    (toggle as HTMLButtonElement).disabled,
    'H10.9: a disabled control takes no ring',
  ).toBe(false)
})

// ── H10.10: the tab order ────────────────────────────────────────────────

/** The elements Tab reaches, in DOM order (as ModelProfileSave16 reads them;
 * a test file imports no other test file). */
function tabbableElements(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      el.closest('[hidden]') === null,
  )
}

/** Their names, in DOM order. */
const tabNames = (root: HTMLElement): string[] =>
  tabbableElements(root).map((el) => textOf(el))

test('H10.10: in jsdom, with no CSS, Tab from the top reaches the back link, then the jump toggle, then the ten jump buttons in card order (the page start of 2h.197 with the toggle inserted)', async () => {
  const { main } = await form()
  expect(
    tabNames(main).slice(0, TITLES.length + 2),
    'H10.10: back link, toggle, the ten entries in card order',
  ).toEqual([
    en('model-profile-back'),
    en('model-profile-jump-label'),
    ...TITLES,
  ])
})
