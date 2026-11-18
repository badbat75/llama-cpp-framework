// Hearth H3: the mark in the banner. Brief fe/briefs/07-hearth-h3-mark.md,
// sections 3.1 to 3.3 and 3.9: what the banner element is and what it does to
// the accessibility tree. The geometry of the mark (the shape, the 2rem, the
// gap) is the stylesheet's, and jsdom lays nothing out: that lives in
// HearthH3.test.ts, which reads the text of index.css and index.html.
import { cleanup, render, screen } from '@testing-library/react'
import { beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import { en, fakeBff, startAt } from './test/bff.ts'

/** The banner's home link, once the first replies are in. */
async function homeLink() {
  await screen.findByRole('link', { name: en('app-name') })
  return screen.getByRole('link', { name: en('app-name') })
}

/** The banner's mark: the one `svg.mark`, and nothing beside it. */
function markIn(el: Element) {
  const marks = el.querySelectorAll('svg.mark')
  expect(marks.length, 'one mark in the banner').toBe(1)
  return marks[0]!
}

beforeEach(() => {
  cleanup()
  fakeBff()
  startAt('#/')
})

// ── The mark itself ─────────────────────────────────────────────────────

test('H3.1: the home link holds one inline mark before the product name, with the markup of the brief', async () => {
  render(<App />)
  const home = await homeLink()
  const svg = markIn(home)
  // The attributes the brief lists, and nothing of the sort it does not list.
  expect(svg.getAttribute('viewBox')).toBe('0 0 32 32')
  expect(svg.getAttribute('aria-hidden')).toBe('true')
  expect(svg.getAttribute('focusable')).toBe('false')
  expect(
    svg.getAttribute('width'),
    'the size comes from the stylesheet',
  ).toBeNull()
  expect(
    svg.getAttribute('height'),
    'the size comes from the stylesheet',
  ).toBeNull()
  // One path: the outline path and the even-odd rule that leaves the door hole.
  const paths = svg.querySelectorAll('path')
  expect(paths.length).toBe(1)
  const path = paths[0]!
  expect(path.getAttribute('d')).toBe(
    'M5 30V15a11 11 0 0 1 22 0v15zM17.79 18.12 18.8 24h-5.6l1.01-5.88a3.6 3.6 0 1 1 3.58 0z',
  )
  expect(path.getAttribute('fill')).toBe('currentColor')
  expect(path.getAttribute('fill-rule')).toBe('evenodd')
  // The order inside the link: the mark first, then the name and nothing else.
  expect([...home.childNodes][0], 'the mark comes before the name').toBe(svg)
  const text = [...home.childNodes].filter((n) => n.nodeType === Node.TEXT_NODE)
  expect(text.length, 'one text node, the name').toBe(1)
  expect(text[0]!.textContent).toBe(en('app-name'))
  expect(svg.textContent, 'the svg holds no text').toBe('')
})

test('H3.3: the mark shows on every route and nowhere twice', async () => {
  // Signed in, the Overview.
  render(<App />)
  await screen.findByRole('button', { name: en('session-sign-out') })
  expect(document.querySelectorAll('svg.mark').length, 'the Overview').toBe(1)
  // The sign-in page, signed out.
  cleanup()
  fakeBff({ session: 'signed-out' })
  startAt('#/sign-in')
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('signin-title') })
  expect(document.querySelectorAll('svg.mark').length, 'the sign-in page').toBe(
    1,
  )
  // The model profile list, signed in.
  cleanup()
  fakeBff()
  startAt('#/profiles/model')
  render(<App />)
  await screen.findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  expect(document.querySelectorAll('svg.mark').length, 'the list page').toBe(1)
})

// ── The guards (the brief: GREEN) ───────────────────────────────────────

test('H3.2: the mark is not announced: the link keeps its name and its text, the svg has no name of its own', async () => {
  render(<App />)
  const home = await homeLink()
  // The accessible name is the text alone, so the role query above found the
  // link by the name, and the text the user reads is unchanged.
  expect(home.textContent).toBe(en('app-name'))
  const svg = markIn(home)
  expect(svg.getAttribute('title'), 'no title to announce').toBeNull()
  expect(svg.getAttribute('aria-label'), 'no aria-label').toBeNull()
  expect(svg.getAttribute('aria-labelledby'), 'no aria-labelledby').toBeNull()
  expect(svg.getAttribute('role'), 'no role: it is inside the link').toBeNull()
})

test('H3.9: the mark adds no tab stop, and needs no string of its own', async () => {
  render(<App />)
  const skip = screen.getByRole('link', { name: en('skip-to-content') })
  // The skip link is still the first focusable thing on the page.
  const focusable = document.querySelectorAll('a[href], button, input, select')
  expect(focusable[0]).toBe(skip)
  const home = await homeLink()
  expect([...focusable].includes(home), 'the home link stays tabbable').toBe(
    true,
  )
  // The mark is not one of them: no tabindex, no link inside it, no text.
  const svg = markIn(home)
  expect(svg.getAttribute('tabindex')).toBeNull()
  expect(svg.querySelector('a'), 'no link inside the mark').toBeNull()
  expect(svg.textContent).toBe('')
})

test('H3.9: a decorative mark needs no text: the strings table carries no word for it', () => {
  // The name the banner shows is the product name, the key of 1a.1. The mark
  // adds nothing to read, so no key was added for it (strings.test.ts pins
  // that every key has a value and every value is used, and that no component
  // writes text of its own).
  expect(strings['app-name']).toBe(en('app-name'))
  const added = Object.keys(strings).filter((k) =>
    /(^|-)(mark|hearth|logo|emblem)(-|$)/.test(k),
  )
  expect(added, 'no strings key for a mark that says nothing').toEqual([])
})
