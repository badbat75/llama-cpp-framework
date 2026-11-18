# fe/ux: the web UI's UX check

Renders the built web UI (`fe/dist`) against a stub bff and, for every browser
that starts, at 1280 and 320 px, in light and dark, reads each page with
[axe-core](https://github.com/dequelabs/axe-core) (the `wcag2a`, `wcag2aa`,
`wcag21aa`, `wcag22aa` tags) and compares `scrollWidth` with the viewport. The
states: Overview signed out, the sign-in page, Overview signed in, the instance
page with the Server form, the Models list, a model profile page that is there, a
model profile page that is not. Each state names an `expect`: the selectors, with the
text they carry, only that state shows. The page is waited for up to 5 s, so a
blank page, a page still loading, an error banner or the wrong route is reported
as `not rendered` and not read as a clean page. Once per browser the check also
proves the dark colour scheme changes the page (the computed background of
`body` must differ from the light one).

The stub binds port 0 (the OS picks a free one) and prints the port it got on
its first stdout line; the check reads it there, so it can never run against a
foreign server on a fixed port. A stub that exits, errors, prints no port or
stays silent for 5 s stops the run at once with its own message and exit 1.

One line per failing state (browser, width, theme, state, the axe rule ids, the
overflow, or `not rendered`), then a summary line naming the browsers that ran.
Exit 0 with nothing failing, 1 otherwise. A browser that does not start is
reported and skipped: the check still runs on the others and a skipped browser
is no failure. When no browser starts, nothing has been checked: the run says
`no browser started: nothing was checked` and exits 1. Node 18 or newer: the
check uses the global `fetch`.

## What it does not check

- Not the real bff, the manager, a server.ini, a model: `stub.py` answers the
  API routes with fixed shapes (its states are set with `GET /__set?name=value`).
- No screen reader: axe reads the DOM, it does not hear the page.
- Pages at rest only: no click, no typing, no save, no focus walk, no busy,
  error or long-request shape beyond the two model profile answers.
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

Add a line to `STATES` in `check.js`: the stub's `/__set` query, the hash route
and the `expect` (the selectors, with their text, that only that state shows:
`check.js` reads the words from `fe/src/strings/en_US.json`, the rest are the
stub's data). A state whose marker never appears is `not rendered`. The stub's
answers live in `stub.py` (`STATE` for the names it sets, `MODEL_PROFILES`,
`MODEL_PROFILE_DOC`, `SERVER_DOC` for the bodies); a route the pages read that
`stub.py` does not answer goes there too.
