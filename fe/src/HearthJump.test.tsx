// Hearth H9a, the model profile page's jump list drawn as an index
// (fe/briefs/08-hearth-h9-jump.md, section 1): the DOM half. H9a.1 is the one
// DOM change of the slice, the ten buttons lose their class, and the criteria
// that are stylesheet declarations (H9a.2, H9a.3, H9a.5, H9a.6) are pinned in
// HearthH9a.test.ts, which reads index.css as text (a test file imports no
// other test file, so the reader is not shared). The names, the order, the
// handler, the focus target and the nav's name are section 4's "must not
// change": pinned here as guards, and by 2h.196 and 2h.197 in
// ModelProfileSave16.test.tsx.
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
const MIDDLE = 4

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

/** The jump nav, found by its name (2h.196). */
function navOf(main: HTMLElement): HTMLElement {
  return within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
}

/** The jump entries, in the nav's order. */
function entries(nav: HTMLElement): HTMLElement[] {
  return within(nav).getAllByRole('button')
}

const textOf = (el: HTMLElement): string =>
  (el.textContent ?? '').replace(/\s+/g, ' ').trim()

test('H9a.1: the nav, its name, the ul and li, the ten buttons, their names in card order and their type stay; the ten buttons carry no class attribute', async () => {
  const { main } = await form()
  const nav = navOf(main)
  // Everything the brief says is unchanged, so the red below is the class.
  expect(nav.tagName, 'a nav element (section 4: the nav)').toBe('NAV')
  expect(
    nav.getAttribute('aria-label'),
    'named by model-profile-jump-label (section 4: the nav name)',
  ).toBe(en('model-profile-jump-label'))
  const items = Array.from(nav.children)
  expect(items.length, 'the nav holds the ul alone').toBe(1)
  expect(items[0]?.tagName, 'the list is a ul').toBe('UL')
  const buttons = entries(nav)
  expect(buttons.length, 'one entry per card').toBe(TITLES.length)
  expect(
    buttons.map(textOf),
    'each entry named by its card title, in card order (section 4)',
  ).toEqual(TITLES)
  for (const b of buttons) {
    expect(b.tagName, 'a button, not a link (section 0)').toBe('BUTTON')
    expect(b.getAttribute('type'), 'type=button').toBe('button')
    const li = b.parentElement
    expect(li?.tagName, `${textOf(b)}: its li`).toBe('LI')
    expect(li?.parentElement, `${textOf(b)}: the li is the ul's child`).toBe(
      items[0],
    )
  }
  expect(Array.from(items[0]!.children).length, 'one li per entry').toBe(
    TITLES.length,
  )
  // H9a.1, the one DOM change of the slice.
  const classed = buttons
    .filter((b) => b.getAttribute('class') !== null)
    .map((b) => `${textOf(b)}: class="${b.getAttribute('class')}"`)
  expect(
    classed,
    'H9a.1: no jump button carries a class, so .jump button (0,1,1) owns the look and no .secondary declaration has to be overridden',
  ).toEqual([])
})

test('H9a.7: activating an entry moves focus to that card h3, leaves the hash alone, and opens no dialog', async () => {
  const { main } = await form()
  const title = TITLES[MIDDLE]!
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  expect(h3.getAttribute('tabindex'), `${title}: the h3 takes focus`).toBe('-1')
  fireEvent.click(entries(navOf(main))[MIDDLE]!)
  expect(document.activeElement, 'focus on the card h3').toBe(h3)
  expect(window.location.hash, 'the jump writes no hash').toBe(HASH)
  expect(
    screen.queryByRole('dialog'),
    'a jump is a focus move inside the page: no dialog opens',
  ).toBeNull()
  expect(
    screen.queryByRole('alertdialog'),
    'a jump is a focus move inside the page: no alert dialog opens',
  ).toBeNull()
})
