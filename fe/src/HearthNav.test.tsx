// fe/src/HearthNav.test.tsx
// The shell's navigation, as the brief of the Hearth shell states it
// (fe/briefs/07-hearth-h5-shell.md, H5b.7): one landmark region, the groups
// inside it named, the links in the order the list reads, and the Menu button
// that opens the region naming what it opens. jsdom does no layout, so what
// the nav looks like, the pills, the 32 px target, the wide and narrow shape,
// is pinned in the CSS text, in HearthH5b.test.ts.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { EN, fakeBff, startAt } from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  startAt('')
})

/** The nav's links, in the order the document holds them: the tab order. */
const navLinks = (nav: HTMLElement): HTMLAnchorElement[] =>
  Array.from(nav.querySelectorAll('a'))

const navOf = async (): Promise<HTMLElement> =>
  (await screen.findByRole('navigation', {
    name: EN['nav-label'],
  })) as HTMLElement

/** The Menu button, by the name the strings file gives it. */
const menuButton = (): HTMLElement =>
  screen.getByRole('button', { name: EN['nav-menu'] })

const expanded = (button: HTMLElement): string | null =>
  button.getAttribute('aria-expanded')

test('H5b.7: one nav landmark named Main, each group named by a .nav-label above it, the links in the reading order', async () => {
  fakeBff()
  render(<App />)
  const nav = await navOf()
  expect(
    nav.id,
    'the region has an id, so the Menu button can name it (aria-controls)',
  ).toBe('nav')
  // The instances list arrives with the read, so the groups are counted after
  // the first of its links is there. The Overview table names the same
  // instances, so every nav query is scoped to the region.
  await within(nav).findByRole('link', { name: 'default' })
  await within(nav).findByRole('link', { name: EN['nav-model-profiles'] })
  const groups = Array.from(nav.querySelectorAll(':scope > ul'))
  expect(
    groups.length,
    'four groups: Overview, Nodes, the model profiles, and the last one with About only (13 N.6, 18 AN.6, 20 N.3: the host list is inside the Nodes list, so it is not a group of the nav)',
  ).toBe(4)
  expect(
    groups[0].hasAttribute('aria-labelledby'),
    'the Overview list needs no label: the nav names it (H5b.7)',
  ).toBe(false)
  const named: string[] = []
  // About's list has no label of its own (13 N.6), so only the labelled groups
  // are walked: the label rule of H5b.4 is about the groups that have one.
  for (const group of groups
    .slice(1)
    .filter((group) => group.hasAttribute('aria-labelledby'))) {
    const id = group.getAttribute('aria-labelledby') ?? ''
    const label = document.getElementById(id)
    expect(
      label,
      `${id}: the group names the element that names it`,
    ).not.toBeNull()
    expect(
      label?.classList.contains('nav-label'),
      `${id}: named by a .nav-label, not by a bare heading (H5b.4)`,
    ).toBe(true)
    expect(
      label?.parentElement,
      `${id}: the label sits in the nav, right above its list`,
    ).toBe(nav)
    named.push(label?.textContent?.trim() ?? '')
  }
  expect(
    named,
    'the groups are named in the order they read: Nodes, then Profiles (brief 20 20 N.3; the host label is inside the Nodes list, so this walk does not see it)',
  ).toEqual([EN['nav-nodes'], EN['nav-profiles']])
  const links = navLinks(nav)
  expect(
    links.map((a) => a.textContent),
    'the tab order follows the list: Overview, the instances, Node info under them, the model profiles, About (13 N.11, 18 AN.6, 20 N.7)',
  ).toEqual([
    'Overview',
    'default',
    'coder',
    EN['nav-node'],
    EN['nav-model-profiles'],
    EN['nav-about'],
  ])
})

test('H5b.7: the Menu button names the region it opens, and a link, Escape on the button and Escape in the nav close it', async () => {
  fakeBff()
  render(<App />)
  const nav = await navOf()
  await within(nav).findByRole('link', { name: 'default' })
  await within(nav).findByRole('link', { name: EN['nav-model-profiles'] })
  const button = menuButton()
  const target = button.getAttribute('aria-controls') ?? ''
  expect(target, 'the button names the region it opens').toBe(nav.id)
  expect(
    document.getElementById(target),
    'the id it names is the navigation region, not the layout, not the banner',
  ).toBe(nav)
  expect(expanded(button), 'the nav is closed at first').toBe('false')
  fireEvent.click(button)
  expect(expanded(button), 'the click opens it').toBe('true')
  expect(
    nav.classList.contains('open'),
    'and the nav carries the open class, what the narrow CSS shows it by',
  ).toBe(true)
  fireEvent.keyDown(nav, { key: 'Escape' })
  expect(expanded(button), 'Escape inside the nav closes it').toBe('false')
  expect(
    document.activeElement,
    'and focus goes back to the Menu button, where the user is',
  ).toBe(button)
  fireEvent.click(button)
  expect(expanded(button), 'opened again').toBe('true')
  fireEvent.keyDown(button, { key: 'Escape' })
  expect(expanded(button), 'Escape on the button closes it too').toBe('false')
  fireEvent.click(button)
  expect(expanded(button), 'opened again').toBe('true')
  // A link under the open drawer: the drawer is out of the way once it is
  // followed. Overview, the first link, leads to the page already shown, so
  // what this pins is the close; following an entry that leads elsewhere is
  // 5b.4 of 05b, pinned there.
  fireEvent.click(navLinks(nav)[0])
  expect(expanded(button), 'a link click closes the nav').toBe('false')
  await screen.findByRole('heading', {
    level: 1,
    name: EN['overview-title'],
  })
})
