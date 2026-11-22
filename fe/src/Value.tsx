// The value of a row, as the two version pages show it (fe/briefs/13-node-about.md,
// 13 N.8, N.9, and fe/briefs/18-about-node-info.md, 18 AN.3, decision 5): a value
// is data, shown as sent in monospace, and a fact the reply does not carry is the
// muted word, never a blank. The two pages share these, and the sheet gains
// nothing: the classes are the lists' own.
import { t } from './strings.ts'

/** The parts of a joined value, in the reply's order: the ones that are there,
 * joined by one space (18 decision 5). Joined text is never read back. */
export function joined(parts: Array<string | null>): string | null {
  const some = parts.filter((part) => part !== null)
  return some.length === 0 ? null : some.join(' ')
}

/** One value of a list: shown as sent, in monospace; the missing one is the
 * muted word, never mono. */
export function Value({ value }: { value: string | null }) {
  return value === null ? (
    <dd className="muted">{t('about-version-unknown')}</dd>
  ) : (
    <dd className="mono">{value}</dd>
  )
}
