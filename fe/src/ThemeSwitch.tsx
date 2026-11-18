// The banner's theme switch (fe/briefs/07-hearth-h4-theme.md): one button that
// turns the dark theme on and off, named the same word in both states, its
// state carried by aria-pressed. It is the page's own choice: it reads and
// writes localStorage and asks the server nothing, so it shows in every session
// state, signed in, signed out and while the session is read.
import { useEffect, useState } from 'react'
import {
  applyTheme,
  schemeQuery,
  storeTheme,
  storedTheme,
  systemDark,
  type Theme,
} from './theme.ts'
import { t } from './strings.ts'

/** Lucide's `moon`, its path written inline: the brief's section 0 takes one
 * icon now, so the label can go on a narrow screen later. Lucide's licence text
 * ships with the installer. */
const MOON =
  'M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401'

export function ThemeSwitch() {
  const [choice, setChoice] = useState<Theme | null>(storedTheme)
  const [system, setSystem] = useState(systemDark)

  // The attribute mirrors the choice, in the same tick the app renders.
  useEffect(() => applyTheme(choice), [choice])

  // With nothing stored the button follows the system while the page is open
  // (the stylesheet follows it on its own); the listener leaves with the app.
  useEffect(() => {
    const mq = schemeQuery()
    if (mq === null) return
    const onChange = (e: MediaQueryListEvent) => setSystem(e.matches)
    mq.addEventListener('change', onChange)
    return () => {
      mq.removeEventListener('change', onChange)
    }
  }, [])

  const pressed = choice === null ? system : choice === 'dark'

  /** Flips the theme in effect. One press back to the system's own theme drops
   * the choice, which is the way to follow the OS again. */
  const flip = () => {
    const next: Theme = pressed ? 'light' : 'dark'
    const chosen: Theme | null =
      next === (system ? 'dark' : 'light') ? null : next
    setChoice(chosen)
    storeTheme(chosen)
  }

  return (
    <button
      type="button"
      className="secondary theme-switch"
      aria-pressed={pressed}
      onClick={flip}
    >
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
        focusable="false"
      >
        <path d={MOON} />
      </svg>
      <span className="theme-switch-label">{t('theme-dark')}</span>
    </button>
  )
}
