// Hearth H9b, the model-profile jump list as a sticky right rail from 1280px
// (fe/briefs/08-hearth-h9-jump.md, section 2): the DOM half. H9b is CSS only,
// so what is testable in the DOM is what the slice must NOT change: the rail
// stays the region the settings heading names, in the reading order H9a built,
// with no new element, no new class, no new text and no new tab stop, and a
// press on an entry opens no dialog. The stylesheet face, the grid, the sticky
// range and the cascade, is pinned in HearthH9b.test.ts.
//
// A test file imports no other test file, so the readers and helpers here are
// copies of the ones in HearthJump.test.tsx.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
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
/** 2h.196: the cards in page order; the jump entries take their titles. */
const TITLES = cardHeadings()

beforeEach(() => {
  startAt(HASH)
  // jsdom has no scrollIntoView; the jump calls it on the heading.
  Element.prototype.scrollIntoView = () => {}
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

/** The rail as the page renders it: the nav, its section, its entries. H10.1
 * adds the label p and the disclosure to the nav, so the entries are read
 * from the list the disclosure controls. */
function railOf(main: HTMLElement) {
  const nav = within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
  const list = nav.querySelector('ul')
  expect(list, 'H10.1: the nav holds the ul of entries').not.toBeNull()
  const buttons = within(list as HTMLElement).getAllByRole('button')
  const toggle = nav.querySelector('button.jump-toggle')
  expect(toggle, 'H10.1: the nav holds the disclosure').not.toBeNull()
  return {
    nav,
    buttons,
    toggle: toggle as HTMLElement,
    section: nav.parentElement as HTMLElement,
  }
}

/** The focusable elements of `root`, in document order. */
function tabbables(root: HTMLElement): HTMLElement[] {
  return Array.from(
    root.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), ' +
        'select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ),
  )
}

/** The element pairs that are out of document order, named. */
function outOfOrder(landmarks: [string, HTMLElement][]): string[] {
  const bad: string[] = []
  for (let i = 1; i < landmarks.length; i++) {
    const [prevName, prev] = landmarks[i - 1]!
    const [name, el] = landmarks[i]!
    if (!before(prev, el)) bad.push(`${name} does not follow ${prevName}`)
  }
  return bad
}

test('H9b.4: the rail is the region the settings heading names, and a press opens no dialog', async () => {
  // The engine-unavailable page: the state where the region sits between the
  // heading and the notice, the order the brief's section 4 keeps.
  const { bff, main } = await form({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { engineAvailable: false, revision: 'r1' }),
      ),
  })
  const { nav, buttons, section } = railOf(main)

  // The name the region is addressed by, the existing h2 text, is in the
  // strings file under the key that already carries it (no new string).
  const h2 = within(section).getByRole('heading', { level: 2 })
  expect(h2.textContent, 'the heading names the region').toBe(
    en('model-profile-settings-title'),
  )
  // H9b adds no role: a CSS-only slice cannot, and the region stays the
  // section H9a renders, addressed by that heading.
  expect(section.getAttribute('role'), 'H9b.4: no ARIA role added').toBeNull()
  expect(section.tagName, 'the rail lives in a section').toBe('SECTION')
  expect(
    nav.getAttribute('aria-labelledby'),
    'the nav keeps the H9a name, now via the label p (H10.1)',
  ).toBe('model-profile-jump-label')

  // The reading flow of the narrow page, in document order: heading, region,
  // engine notice, Assets card (H11b.1: the Engine row is inside it).
  const notice = section.querySelector('p.notice') as HTMLElement
  expect(notice, 'the engine notice is there').not.toBeNull()
  const assets = within(section).getByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  expect(
    outOfOrder([
      ['h2', h2],
      ['nav.jump', nav],
      ['notice', notice],
      ['Assets card', assets],
    ]),
    'H9b.4: heading, region, notice, card, in that order',
  ).toEqual([])

  // The press of an entry moves focus to its card, and opens nothing.
  const title = TITLES[4]!
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  fireEvent.click(buttons[4]!)
  expect(document.activeElement, `a press focuses ${title}`).toBe(h3)
  expect(
    screen.queryByRole('dialog'),
    'H9b.4: a jump on the rail opens no dialog',
  ).toBeNull()
  expect(
    screen.queryByRole('alertdialog'),
    'H9b.4: a jump on the rail opens no alert dialog',
  ).toBeNull()
  expect(
    bff.calls.map((c) => c.method).filter((m) => m !== 'GET'),
    'a press writes nothing',
  ).toEqual([])
})

test('H9b.4: the disclosure and the entries are the stops right after the back link, in the cards order, and add no others', async () => {
  const { main } = await form()
  const { nav, buttons, toggle } = railOf(main)
  expect(buttons.length, 'one entry per card').toBe(TITLES.length)
  expect(
    buttons.map((b) => (b.textContent ?? '').replace(/\s+/g, ' ').trim()),
    'the entries carry the card titles, in the page order',
  ).toEqual(TITLES)

  // The stops of the nav, in order: H10.10 gives the nav the disclosure
  // before its entries (jsdom, no CSS).
  expect(
    tabbables(nav),
    "the nav's stops are the disclosure, then its entries",
  ).toEqual([toggle, ...buttons])

  // In the page, the disclosure and the entries are the stops right after
  // the back link (H10.10).
  const stops = tabbables(main)
  expect(stops[0], 'the back link stays the first stop of the page').toBe(
    screen.getByRole('link', { name: en('model-profile-back') }),
  )
  expect(
    stops.slice(1, buttons.length + 2),
    'H10.10: the disclosure then the entries are the next stops, in their order',
  ).toEqual([toggle, ...buttons])

  // The reading order of the rail page: back link, region, Assets card,
  // cards, save bar.
  const section = railOf(main).section
  const assets = within(section).getByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  // The card that follows the Assets card: TITLES[0] is Assets, the first of
  // the ten (H11a.1), so the first 2e card is TITLES[1].
  const firstCard = within(main).getByRole('heading', {
    level: 3,
    name: TITLES[1],
  })
  const bar = within(section)
    .getByRole('button', { name: en('save-button') })
    .closest('.save-bar') as HTMLElement
  expect(
    outOfOrder([
      [
        'back link',
        screen.getByRole('link', { name: en('model-profile-back') }),
      ],
      ['nav.jump', nav],
      ['Assets card', assets],
      ['first card', firstCard],
      ['save bar', bar],
    ]),
    'H9b.4: the rail sits between the back link and the fields it jumps to',
  ).toEqual([])
})

test('H9b.6 and H9b.5: no new element, class or text, and the cards and the save bar keep their place', async () => {
  const { bff, main } = await form()
  const { nav, buttons, section } = railOf(main)
  expect(
    bff.calls.filter((c) => c.method !== 'GET').map((c) => c.path),
    'H9b.6: the rail page writes nothing',
  ).toEqual([])
  expect(
    bff.to('GET', `/api/profiles/model/${ID}`).length,
    'H9b.6: the rail is built from the one read, not from an extra call',
  ).toBe(1)

  // H9b.6: the rail is the nav H9a renders: one nav, no wrapper, no extra.
  expect(nav.tagName, 'H9b.6: a nav').toBe('NAV')
  expect(nav.className, 'H9b.6: the class is the H9a one').toBe('jump')
  expect(
    Array.from(nav.children).map((el) => el.tagName),
    'H10.1: the nav holds the label p, the disclosure and the list (the H9b.6 guard: the three are exactly what H10 adds, no wrapper)',
  ).toEqual(['P', 'BUTTON', 'UL'])
  const items = Array.from(nav.querySelector('ul')!.children)
  expect(
    items.map((li) => Array.from(li.children).map((c) => c.tagName)),
    'H9b.6: one button per li, and nothing else',
  ).toEqual(TITLES.map(() => ['BUTTON']))
  expect(
    section.className,
    'H9b.6: H9b adds no class to the section the rail sits in',
  ).toBe('')
  expect(
    buttons.filter((b) => b.getAttribute('class') !== null),
    'H9b.6: no class on an entry, so .jump ul button (0,1,2) owns its look',
  ).toEqual([])

  // H9b.5: the cards and the save bar are the same markup the rail shares the
  // grid with: cards as the sections of the rail section, bar its last child.
  const cards = Array.from(section.children).filter(
    (el) => el.tagName === 'SECTION' && el !== section,
  )
  expect(
    cards.length,
    'the cards are sections of the rail section',
  ).toBeGreaterThan(1)
  expect(
    cards.map((card) => card.parentElement),
    'H9b.5: the cards stay children of the rail section',
  ).toEqual(cards.map(() => section))
  const children = Array.from(section.children)
  expect(
    (children[children.length - 1] as HTMLElement).className,
    'H9b.5: the save bar is still the last child of the section',
  ).toContain('save-bar')
})
