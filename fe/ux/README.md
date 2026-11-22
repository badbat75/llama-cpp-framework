# fe/ux: the web UI's UX check

Renders the built web UI (`fe/dist`) against a stub bff and, for every browser
that starts, at 1280 and 320 px, in light and dark, reads each page with
[axe-core](https://github.com/dequelabs/axe-core) (the `wcag2a`, `wcag2aa`,
`wcag21aa`, `wcag22aa` tags) and compares `scrollWidth` with the viewport. The
states: the sign-in page at the home address, a protected address that shows
the sign-in form with the notice saying why, the sign-in route, Overview signed
in, the Overview and the boot screen while their first read is still open (the
two `load=` states of brief 17 U17a, the skeleton pages), Overview with the
tiles of four instances, the menu opened at 320 px (`nav-open`: the Nodes
group's instance links with their server profiles and a 40-character id), the instance page with the Server
form, the Model profiles list, a model profile page that is there, the same page
with its jump list and its Assets card opened, a model profile page that is not.
Each state names an `expect`: the selectors, with the text they carry, only that
state shows; a state may also name a `click`, the selectors to click in order
once it has rendered, each one only where it is visible and then awaited open
(`aria-expanded="true"`), so the page is read at rest with the disclosures
done, or a `hold`, which waits for the page parsed and not for the network idle
(a held read never ends, so the page at rest IS the page waiting). The page is
waited for up to 5 s, so a blank page, a page still loading,
an error banner or the wrong route is reported as `not rendered` and not read as
a clean page. Once per browser the check also proves the dark colour scheme
changes the page (the computed background of `body` must differ from the light
one).

On the three signed-in states listed in `CHOICE_STATES` (Overview, the
instance page, a model profile page: the Overview of tiles and the opened model
profile page are not among them) the run is repeated with a theme choice
stored in `localStorage` (`pai-theme`) before the page opens, always the
opposite of the context's scheme: the page must show the stored choice, with
`data-theme` on `<html>`, `aria-pressed` on `.theme-switch` and the computed
`body` background of the chosen scheme (the one the same browser paints for
that scheme with nothing stored). That is 3 states x 2 widths x 2 schemes more
per browser, counted in the summary line.

The text pass of `fe/briefs/10-reflow.md` section 4 runs last, per browser, in
the light scheme at 320 px only: every state loaded once more with
`html { font-size: 200% !important }` added after the page settles, and once
more with that plus the spacing of
[WCAG 1.4.12 Text Spacing](https://www.w3.org/TR/WCAG22/#text-spacing) (line
height 1.5, letter spacing 0.12em, word spacing 0.16em, paragraph margin 2em),
all as `!important`. A mode changes no colour, so the pass reads the
`scrollWidth` alone: no axe. That is 10 states x 2 modes more per browser,
counted in the summary line.

The stub binds port 0 (the OS picks a free one) and prints the port it got on
its first stdout line; the check reads it there, so it can never run against a
foreign server on a fixed port. A stub that exits, errors, prints no port or
stays silent for 5 s stops the run at once with its own message and exit 1.

One line per failing state (browser, width, theme, state, the axe rule ids, the
overflow, `not rendered`, a `click` that never opened, or what a stored choice
left unseen; the theme of a stored pass is tagged `light+dark`, a line of the
text pass names its mode `200%` or `200%+1.4.12`), then a summary
line naming the browsers that ran.
Exit 0 with nothing failing, 1 otherwise. A browser that does not start is
reported and skipped: the check still runs on the others and a skipped browser
is no failure. When no browser starts, nothing has been checked: the run says
`no browser started: nothing was checked` and exits 1. Node 18 or newer: the
check uses the global `fetch`.

## What it does not check

- Not the real bff, the manager, a server.ini, a model: `stub.py` answers the
  API routes with fixed shapes (its states are set with `GET /__set?name=value`).
  Signed out (`session=out`) it answers 401 `session-required` on every `/api/`
  path, `/api/session` included, so a signed-out page can only be a sign-in one.
- No screen reader: axe reads the DOM, it does not hear the page.
- Pages at rest only: the only clicks are the `click` list of a state, today the
  two disclosures of the opened model profile page. No typing, no save, no focus
  walk, no busy, error or long-request shape beyond the two model profile
  answers.
- Not the layout's quality: the overflow check only says a page is wider than
  the viewport.

## Set up once

```
npm --prefix fe/ux ci
npx --prefix fe/ux playwright install chromium firefox webkit
```

WebKit needs system libraries no npm package installs, `libevent-2.1-7t64`,
`libavif16`, `libmanette-0.2-0` on this Debian: the user installs them with
`sudo apt install`. Without them WebKit does not start, the check says so and
runs on Chromium and Firefox.

`fe/ux` is its own npm package because every `xtask build` runs `npm ci` in
`fe/`, on every OS and in CI: Playwright must not join that install. Nothing
outside `fe/ux` reads it.

## Run

```
npm --prefix fe run build
npm --prefix fe/ux run check
```

## Adding a state

Add a line to `STATES` in `check.js`: the stub's `/__set` query, the hash route,
the `expect` (the selectors, with their text, that only that state shows:
`check.js` reads the words from `fe/src/strings/en_US.json`, the rest are the
stub's data; one with a `count` is rendered when the list has that many) and,
for a page with a disclosure to open, the `click` list. The check sets
`list=one` before the state's own query, so a state that wants the many tiles
asks for them itself. A state whose marker never appears is `not rendered`. The
stub's answers live in `stub.py` (`STATE` for the names it sets, `MODEL_PROFILES`,
`MODEL_PROFILE_DOC`, `SERVER_DOC` for the bodies); a route the pages read that
`stub.py` does not answer goes there too.
