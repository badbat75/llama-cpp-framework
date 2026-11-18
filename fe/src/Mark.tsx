// The Hearth mark (fe/briefs/07-hearth-h3-mark.md, section 1): the arched
// doorway with the keyhole cut out of it. One path with the even-odd rule, so
// the keyhole shows what is behind the mark, and a fill of currentColor, so the
// colour is the CSS's (the .mark rule) and in forced colours the system's.
// Decorative: the product name beside it is what is announced, so it carries no
// title and is hidden from assistive technology.

/** The outline of the doorway and the keyhole, the same path the favicon of
 *  index.html draws. */
const D =
  'M5 30V15a11 11 0 0 1 22 0v15zM17.79 18.12 18.8 24h-5.6l1.01-5.88a3.6 3.6 0 1 1 3.58 0z'

/** The mark, sized by the stylesheet, never by an attribute. */
export function Mark() {
  return (
    <svg
      className="mark"
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
    >
      <path fill="currentColor" fillRule="evenodd" d={D} />
    </svg>
  )
}
