// The banner's theme switch (fe/briefs/07-hearth-h4-theme.md): one button that
// turns the dark theme on and off, named the same word in both states, its
// state carried by aria-pressed. It is the page's own choice: it reads and
// writes localStorage and asks the server nothing, so it shows in every session
// state, signed in, signed out and while the session is read.
import { useEffect, useState } from 'react'
import { Icon } from './Icon.tsx'
import {
  applyTheme,
  schemeQuery,
  storeTheme,
  storedTheme,
  systemDark,
  type Theme,
} from './theme.ts'
import { t } from './strings.ts'

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
      <Icon name="moon" />
      <span className="theme-switch-label">{t('theme-dark')}</span>
    </button>
  )
}
