// The light/dark choice of the banner's switch (fe/briefs/07-hearth-h4-theme.md,
// section 0): the choice lives in localStorage under `pai-theme`, `<html>`
// carries `data-theme` only while a choice is stored, and with no attribute the
// stylesheet follows the system on its own. Nothing of it is a server state, a
// cookie or a request, and a load never writes.
export type Theme = 'light' | 'dark'

/** The localStorage key of the choice. */
const KEY = 'pai-theme'

/** The stored choice: null for nothing stored, for a value that is neither
 * theme, and for a read the browser refuses. */
export function storedTheme(): Theme | null {
  try {
    const value = localStorage.getItem(KEY)
    return value === 'light' || value === 'dark' ? value : null
  } catch {
    return null
  }
}

/** The media query of the system's scheme, and null where the browser has no
 * `matchMedia` (the test harness). */
export function schemeQuery(): MediaQueryList | null {
  return window.matchMedia?.('(prefers-color-scheme: dark)') ?? null
}

/** The system's preference: true for dark. No `matchMedia` counts as light. */
export function systemDark(): boolean {
  return schemeQuery()?.matches ?? false
}

/** Mirrors `theme` on `<html>`: the attribute is there only while a choice is,
 * so the stylesheet carries the common case (nothing stored). */
export function applyTheme(theme: Theme | null) {
  if (theme === null) document.documentElement.removeAttribute('data-theme')
  else document.documentElement.setAttribute('data-theme', theme)
}

/** Puts the stored choice on `<html>`, before the first render, and writes
 * nothing. */
export function startTheme() {
  applyTheme(storedTheme())
}

/** Remembers the choice, null to stop choosing: a write the browser refuses is
 * left alone, the choice keeps its life of the page. */
export function storeTheme(theme: Theme | null) {
  try {
    if (theme === null) localStorage.removeItem(KEY)
    else localStorage.setItem(KEY, theme)
  } catch {
    // Refused: the page keeps the choice, nothing else changes.
  }
}
