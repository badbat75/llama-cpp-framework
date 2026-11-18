// The default box of a text input (fe/DESIGN.md 5.2, the Server page's 1b.28a
// and the model profile page's 2h.156): the state of a field with a `default`,
// `auto` or `No key` box, the text its input shows, and what one tick of the
// box does.
// The text the input held is kept while the box is ticked and comes back when
// it is unticked; which untick takes the focus is the rule the caller passes.
import { flushSync } from 'react-dom'

/** A text input behind a default box. */
export type Boxed = { ticked: boolean; text: string }

/** The box's state of a value the file holds: a key that is not there has its
 * box ticked, with nothing to show. */
export function boxedFrom(value: string | undefined): Boxed {
  return { ticked: value === undefined, text: value ?? '' }
}

/** The text a ticked input shows when the field has no default to show: the
 * box covers the value, so the input is empty. */
export function boxText(b: Boxed): string {
  return b.ticked ? '' : b.text
}

/** One tick of the box: the text stays as it is, so unticking gives the input
 * back what it held before the tick. */
function boxTick(b: Boxed, ticked: boolean): Boxed {
  return { ...b, ticked }
}

/** When an untick moves the focus to the input: `always` where the box covers
 * a value the user is expected to replace (the Server page's thread counts,
 * 5.2), `when-empty` where an input that comes back with a value is already
 * filled (the model profile page, 2h.156). */
export type FocusOnUntick = 'always' | 'when-empty'

/** One tick of a box: renders it, then applies the focus rule `focusOn`, so the
 * user can type at once. `apply` writes the new state where the form keeps it. */
export function tickBox(
  b: Boxed,
  ticked: boolean,
  apply: (next: Boxed) => void,
  input: HTMLElement | null,
  focusOn: FocusOnUntick,
) {
  flushSync(() => apply(boxTick(b, ticked)))
  if (!ticked && (focusOn === 'always' || b.text === '')) input?.focus()
}
