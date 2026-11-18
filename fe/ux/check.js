// The UX check: the built web UI's pages at rest, in every browser that
// starts, at 1280 and 320 px, in light and dark, and on three of them with a
// theme choice stored against the OS scheme. Each state is proven rendered
// (its own marker, `expect`), then read by axe-core (the WCAG 2a, 2aa, 21aa and
// 22aa tags) and its scrollWidth compared with the viewport. The stub bff
// (stub.py) answers for the server and prints the port it bound.
// Run: npm --prefix fe run build, then npm --prefix fe/ux run check (README.md).
const { spawn } = require('child_process')
const { existsSync } = require('fs')
const path = require('path')
const { AxeBuilder } = require('@axe-core/playwright')
const pw = require('playwright')

// The pages' own words, read from the strings file so a state's expect is what
// the app shows.
const S = require('../src/strings/en_US.json')

const DIST = path.resolve(__dirname, '..', 'dist')
// Set once the stub has printed its port: the check talks to that stub alone.
let BASE = ''
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']
const BROWSERS = ['chromium', 'firefox', 'webkit']
const WIDTHS = [1280, 320]
const THEMES = ['light', 'dark']
const HEIGHT = 900

// The theme axis of 07-hearth-h4-theme.md 4a: nothing stored, so the page
// follows the OS scheme, and a stored choice opposite to it. A choice is only
// read where a session shows the theme button.
const CHOICES = [null, 'opposite']
const CHOICE_STATES = ['overview', 'instance-server', 'model-profile']

// How long the stub may take to print its port. It binds before printing, so
// this covers the interpreter start, not the requests.
const STUB_START_MS = 5000
// How long a state's marker may take to appear: the app reads the API, so a
// rendered page is a round trip away.
const RENDER_MS = 5000
// What a page needs after its marker to be at rest: the last fetch's reply
// rendered (the server document, the model profile, arrive after the heading).
const SETTLE_MS = 400

// The stub's data, as stub.py gives it: the signed-in user name in the banner,
// the model profile that is there and the one that is not.
const USER = 'abcdefghijklmnopqrstuvwxyz012345'
const MODEL_PROFILE_ID = 'Qwen3-27B'
const MISSING_ID = 'No-Such-Model-Profile'

// The states to look at: the stub's /__set query, the route, and the expect:
// the selectors (with the text they carry) only that state shows. A page that
// is blank, still loading, showing an error or the wrong route has no axe
// violation, so each state proves itself rendered before it is read.
const STATES = [
  {
    name: 'overview-signed-out',
    set: 'session=out',
    hash: '#/',
    expect: [
      { selector: '#page-heading', text: S['overview-title'] },
      { selector: 'a.session', text: S['session-sign-in'] },
    ],
  },
  {
    name: 'sign-in',
    set: 'session=out',
    hash: '#/sign-in',
    // Only the sign-in form has a password field.
    expect: [{ selector: 'input[type="password"]' }],
  },
  {
    name: 'overview',
    set: 'session=admin',
    hash: '#/',
    // The banner of a signed-in session, with the stub's user name.
    expect: [
      { selector: '#page-heading', text: S['overview-title'] },
      { selector: 'div.session span', text: USER },
    ],
  },
  {
    name: 'instance-server',
    // key=shown: with the API key withheld from the session the bff fails the
    // whole read, and the Server form never shows.
    set: 'session=admin&key=shown',
    hash: '#/instances/default',
    // The Server form's first field: the Port one, whose label id is React's
    // own prefix plus the key.
    expect: [
      { selector: '#page-heading', text: 'default' },
      { selector: '[id$="-Port-label"]', text: S['server-port'] },
    ],
  },
  {
    name: 'model-profiles',
    set: 'session=admin',
    hash: '#/profiles/model',
    expect: [
      { selector: '#page-heading', text: S['model-profiles-title'] },
      { selector: '#model-profiles-filter' },
    ],
  },
  {
    name: 'model-profile',
    set: 'session=admin',
    hash: `#/profiles/model/${MODEL_PROFILE_ID}`,
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      { selector: 'h3', text: S['model-profile-assets'] },
    ],
  },
  {
    name: 'model-profile-missing',
    set: 'session=admin',
    hash: `#/profiles/model/${MISSING_ID}`,
    expect: [
      { selector: '#page-heading', text: MISSING_ID },
      {
        selector: 'p',
        text: S['model-profile-not-found'].replace('{0}', MISSING_ID),
      },
    ],
  },
]

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/** The port the stub printed on its first stdout line. Rejects when the stub
 * exits, errors, prints no port, or stays silent past STUB_START_MS. */
function stubPort(stub) {
  return new Promise((resolve, reject) => {
    let text = ''
    let timer
    const done = (ok, value) => {
      clearTimeout(timer)
      stub.off('exit', onExit)
      stub.off('error', onError)
      stub.stdout.off('data', onData)
      if (ok) resolve(value)
      else reject(new Error(value))
    }
    const onData = (chunk) => {
      text += chunk
      const nl = text.indexOf('\n')
      if (nl < 0) return
      const line = text.slice(0, nl).trim()
      const port = Number(line)
      if (Number.isInteger(port) && port > 0 && port < 65536) done(true, port)
      else done(false, `its first line was "${line}"`)
    }
    const onExit = (code) =>
      done(false, `exited (${code}) before printing a port`)
    const onError = (e) => done(false, `did not start: ${e.message}`)
    timer = setTimeout(
      () => done(false, `printed nothing in ${STUB_START_MS / 1000}s`),
      STUB_START_MS,
    )
    stub.stdout.setEncoding('utf8')
    stub.stdout.on('data', onData)
    stub.on('exit', onExit)
    stub.on('error', onError)
  })
}

/** The state's own marker, on the page, or the reason it is not there. */
async function rendered(page, state) {
  for (const want of state.expect) {
    const loc =
      want.text === undefined
        ? page.locator(want.selector)
        : page.locator(want.selector).filter({ hasText: want.text })
    try {
      await loc.first().waitFor({ timeout: RENDER_MS })
    } catch {
      const what =
        want.text === undefined
          ? want.selector
          : `${want.selector} "${want.text}"`
      return `not rendered: ${what}`
    }
  }
  return null
}

/** What is wrong with the page as it stands: the axe rule ids, a wide page. */
async function problems(page, width) {
  const found = []
  const report = await new AxeBuilder({ page }).withTags(TAGS).analyze()
  if (report.violations.length > 0) {
    found.push(
      'axe: ' +
        report.violations.map((v) => `${v.id} x${v.nodes.length}`).join(' '),
    )
  }
  const scrollWidth = await page.evaluate(
    () => document.documentElement.scrollWidth,
  )
  if (scrollWidth > width) {
    found.push(`overflow: scrollWidth ${scrollWidth} > ${width}`)
  }
  return found
}

/** The computed background of body in one theme, on the Overview page. */
async function bodyBackground(browser, theme) {
  const ctx = await browser.newContext({
    viewport: { width: WIDTHS[0], height: HEIGHT },
    colorScheme: theme,
  })
  try {
    const page = await ctx.newPage()
    await page.goto(`${BASE}/#/`, { waitUntil: 'networkidle' })
    return await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    )
  } finally {
    await ctx.close()
  }
}

/** What the stored choice left on the page: the attribute the app wrote, the
 *  button's pressed state, and the body background, which must be the chosen
 *  scheme's and not the OS's. `background` is what this browser paints for that
 *  scheme with nothing stored. */
async function chosen(page, other, background) {
  const seen = await page.evaluate(() => ({
    attr: document.documentElement.dataset.theme ?? '',
    pressed:
      document.querySelector('.theme-switch')?.getAttribute('aria-pressed') ??
      '',
    body: getComputedStyle(document.body).backgroundColor,
  }))
  const want = String(other === 'dark')
  const found = []
  if (seen.attr !== other) {
    found.push(`data-theme: "${seen.attr}" (expected "${other}")`)
  }
  if (seen.pressed !== want) {
    found.push(`aria-pressed: "${seen.pressed}" (expected "${want}")`)
  }
  if (seen.body !== background) {
    found.push(`body background: ${seen.body} (expected ${background})`)
  }
  return found
}

/** Every width, theme, choice and state in one browser; one line per failing
 *  state, its theme tag `light+dark` when a choice is stored. */
async function check(browser, name, lines) {
  // The dark scheme has to reach the page: index.css sets --bg under
  // prefers-color-scheme, so body's background differs between the themes.
  const light = await bodyBackground(browser, 'light')
  const dark = await bodyBackground(browser, 'dark')
  if (light === dark) {
    lines.push(
      `${name} theme: dark did not change the page (body background ${dark}, as in light)`,
    )
  }
  for (const width of WIDTHS) {
    for (const theme of THEMES) {
      // The scheme a stored choice goes against, and what this browser paints
      // for it with nothing stored.
      const other = theme === 'light' ? 'dark' : 'light'
      const otherBackground = other === 'dark' ? dark : light
      for (const choice of CHOICES) {
        const stored = choice !== null
        const states = stored
          ? STATES.filter((s) => CHOICE_STATES.includes(s.name))
          : STATES
        const tag = stored ? `${theme}+${other}` : theme
        const ctx = await browser.newContext({
          viewport: { width, height: HEIGHT },
          colorScheme: theme,
        })
        try {
          // The choice is in localStorage before the page opens, so the app
          // reads it at its own start and the button shows it pressed.
          if (stored) {
            await ctx.addInitScript((v) => {
              try {
                localStorage.setItem('pai-theme', v)
              } catch {}
            }, other)
          }
          const page = await ctx.newPage()
          for (const state of states) {
            const found = []
            const res = await fetch(`${BASE}/__set?${state.set}`)
            if (!res.ok) {
              found.push(`set request: ${res.status} ${res.statusText}`)
            }
            // A hash change does not reload the app: every state starts from a
            // blank page, so the page is built for it.
            await page.goto('about:blank')
            await page.goto(`${BASE}${state.hash}`, {
              waitUntil: 'networkidle',
            })
            const missing = await rendered(page, state)
            if (missing !== null) {
              found.push(missing)
            } else {
              await sleep(SETTLE_MS)
              found.push(...(await problems(page, width)))
              if (stored) {
                found.push(...(await chosen(page, other, otherBackground)))
              }
            }
            if (found.length > 0) {
              lines.push(
                `${name} ${width}px ${tag} ${state.name}: ${found.join('; ')}`,
              )
            }
          }
        } finally {
          await ctx.close()
        }
      }
    }
  }
}

async function main() {
  if (!existsSync(path.join(DIST, 'index.html'))) {
    console.log(`no web UI in ${DIST}: run npm --prefix fe run build first`)
    return 1
  }
  const stub = spawn(
    'python3',
    ['-I', path.join(__dirname, 'stub.py'), DIST, '0'],
    { stdio: ['ignore', 'pipe', 'inherit'] },
  )
  let port
  try {
    port = await stubPort(stub)
  } catch (e) {
    stub.kill()
    console.log(`stub bff ${e.message}: nothing to check`)
    return 1
  }
  BASE = `http://127.0.0.1:${port}`
  console.log(`stub bff: ${BASE}`)
  const lines = []
  const ran = []
  const skipped = []
  try {
    for (const name of BROWSERS) {
      let browser
      try {
        browser = await pw[name].launch()
      } catch (e) {
        console.log(`${name}: did not start (${e.message.split('\n')[0]})`)
        skipped.push(name)
        continue
      }
      ran.push(name)
      try {
        await check(browser, name, lines)
      } finally {
        await browser.close()
      }
    }
  } finally {
    stub.kill()
  }
  for (const line of lines) console.log(line)
  const plain = WIDTHS.length * THEMES.length * STATES.length
  const stored = WIDTHS.length * THEMES.length * CHOICE_STATES.length
  console.log(
    `summary: browsers ${ran.join(' ') || 'none'}${
      skipped.length ? ` (skipped: ${skipped.join(' ')})` : ''
    }; ${STATES.length} states x ${WIDTHS.length} widths x ${
      THEMES.length
    } themes = ${plain}, plus the ${
      CHOICE_STATES.length
    } stored-choice states = ${plain + stored} per browser; ${
      lines.length
    } failing`,
  )
  if (ran.length === 0) {
    console.log('no browser started: nothing was checked')
    return 1
  }
  return lines.length > 0 ? 1 : 0
}

main().then(
  (code) => process.exit(code),
  (e) => {
    console.log(e.stack || String(e))
    process.exit(1)
  },
)
