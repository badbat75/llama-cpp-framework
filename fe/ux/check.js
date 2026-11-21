// The UX check: the built web UI's pages at rest, in every browser that
// starts, at 1280 and 320 px, in light and dark, and on three of them with a
// theme choice stored against the OS scheme. Each state is proven rendered
// (its own marker, `expect`), then read by axe-core (the WCAG 2a, 2aa, 21aa and
// 22aa tags) and its scrollWidth compared with the viewport. At 320 px, light
// only, every state is loaded once more per text mode (200 % text, and that
// with the spacing of WCAG 1.4.12), where only the scrollWidth is read. The
// stub bff (stub.py) answers for the server and prints the port it bound.
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

// The text pass of 10-reflow.md section 4: every state loaded once more per
// mode, at 320 px in the light scheme only (the modes change no colour, so
// there is nothing for axe to read and nothing for the dark scheme to show).
const TEXT_WIDTH = 320
const TEXT_THEME = 'light'
// The spacing of WCAG 1.4.12 Text Spacing: line height 1.5, letter 0.12em,
// word 0.16em, paragraph 2em.
const SPACING =
  '* { line-height: 1.5 !important; letter-spacing: 0.12em !important; word-spacing: 0.16em !important } ' +
  'p { margin-bottom: 2em !important }'
const TEXT_MODES = [
  ['200%', 'html { font-size: 200% !important }'],
  ['200%+1.4.12', `html { font-size: 200% !important } ${SPACING}`],
]
// What the enlarged text needs to reach its final layout before it is read.
const TEXT_SETTLE_MS = 150

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
// The start of the GPU card's as-of line (brief 14 GD1.7): the sentence of
// `model-profile-gpu-probed-at` up to its `{0}`, which the page fills with the
// probe's moment in the browser's own locale, so no state can name the whole
// sentence. The stub's probe answers, so the line lands.
const AS_OF = S['model-profile-gpu-probed-at'].split('{0}')[0]

// The states to look at: the stub's /__set query, the route, the expect: the
// selectors (with the text they carry) only that state shows, the optional
// click: selectors clicked in order once the state is rendered (each one only
// when it is visible, and then awaited open), the optional open: one control
// clicked and the element its click brings awaited, the optional hover: the
// pointer entering one element and the element its cut text brings awaited, and
// the optional hold: the stub keeps its reads open (stub.py `load=`) so the
// state is the page while it waits, where no idle network exists to wait for. A page
// that is blank, still loading, showing an error or the wrong route has no axe
// violation, so each state proves itself rendered before it is read.
const STATES = [
  // 09 S1a.1: signed out, the sign-in page is the whole of main at every
  // address, so these three are what a visitor meets at rest there.
  {
    name: 'sign-in-at-home',
    set: 'session=out',
    hash: '#/',
    expect: [
      { selector: '#page-heading', text: S['signin-title'] },
      { selector: 'input[type="password"]' },
    ],
  },
  {
    name: 'sign-in-protected',
    // A protected address: the form, and the notice that says why (09 S1a.7).
    set: 'session=out',
    hash: '#/instances/default',
    expect: [
      { selector: 'input[type="password"]' },
      { selector: 'p.notice', text: S['signin-required'] },
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
    name: 'loading-boot',
    // Brief 17 U17a: the session read held, so the page is the skeleton of 300
    // ms of waiting (fe/DESIGN.md 5.2 Skeleton): a line and two boxes in main,
    // with the loading text in the DOM and out of sight.
    set: 'session=admin&load=session',
    hash: '#/',
    hold: true,
    expect: [
      { selector: 'main .skeleton' },
      { selector: 'main .skeleton-box' },
      { selector: '[role="status"]' },
      { selector: '.visually-hidden' },
    ],
  },
  {
    name: 'loading-overview',
    // The instances read held, the session answering: the tiles' skeleton, the
    // shape of the list to come (17a.3), aria-hidden.
    set: 'session=admin&load=slow',
    hash: '#/',
    hold: true,
    expect: [
      { selector: '.tile .skeleton', count: 3 },
      { selector: '[aria-hidden="true"] .skeleton' },
    ],
  },
  {
    name: 'overview-tiles',
    // The tiles of four instances (stub.py list=many): the 40-character id, the
    // badge, the engine line the second engine brings, and the boot row of a
    // signed-in session, the `dl.tile-facts` of H6b. Brief 17 U17b (17b.4): the
    // fourth tile's tags are the widest the rule allows, 8 tags of 32
    // characters, and each shows as a fixed chip of its Tags row, so the state
    // proves the chips rendered inside their tiles. The 320 px and 200 % text
    // reads of 17b.4 (no sideways scroll) are the check's own; the expanded pill
    // of 15 section 10 R5 is pinned on a tile in InstanceTileChips.test.tsx,
    // where the test can say which chip is cut (jsdom lays out nothing).
    set: 'session=admin&list=many',
    hash: '#/',
    expect: [
      { selector: 'li.tile', count: 4 },
      { selector: 'dl.tile-facts' },
      {
        selector: '.tile .chip',
        count: 13,
        message: 'the 13 chips of the four tiles: 3 + 2 + 0 + 8',
      },
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
    // Brief 15 section 7, section 10 R3 (the tags as chips): the widest the
    // rule allows, both lists at its cap, 8 tags of 32 characters the
    // instance's own and 8 the profile brings. A chip's text has no break
    // point, so at 320 px, and at 200 % text, it has to break inside the chip.
    name: 'instance-tags-long',
    set: 'session=admin&key=shown&tags=long',
    hash: '#/instances/default',
    expect: [
      { selector: '#page-heading', text: 'default' },
      { selector: 'li.chip', count: 8 },
      { selector: '.chips-profile li.chip', count: 8 },
    ],
    // Its section 10 R5: where the text is cut, the pointer entering the chip
    // opens the expanded pill, and the read that follows sees it open (its
    // contrast, and that it adds no sideways scroll). At 1280 px a 32-character
    // tag is whole and nothing opens: the hover is a finding only where the
    // text is cut, which is the rule.
    hover: { on: 'li.chip', then: 'li.chip .chip-full' },
  },
  {
    // The new-tag pill opened, in the chip row (brief 15 section 7): the box
    // with its Add and Cancel, opened on the widest tags. R3 measured 152 px
    // of sideways scroll at 200 % text here, from a box that would not shrink.
    name: 'instance-tags-pill',
    set: 'session=admin&key=shown&tags=long',
    hash: '#/instances/default',
    expect: [
      { selector: '#page-heading', text: 'default' },
      { selector: '.tag-add' },
    ],
    open: { click: '.tag-add', then: '.tag-new input' },
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
    // The GPU distribution card is the last read of the page (brief 14
    // GD1.3): its as-of line lands once the device probe answered, so the
    // check reads the page with the GPU rows, their memory lines and their
    // weights there. The text is the sentence up to its `{0}`, whose date
    // depends on the browser's locale.
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      { selector: 'h3', text: S['model-profile-assets'] },
      { selector: 'p', text: AS_OF },
    ],
  },
  {
    name: 'model-profile-open',
    set: 'session=admin',
    hash: `#/profiles/model/${MODEL_PROFILE_ID}`,
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      { selector: 'h3', text: S['model-profile-assets'] },
    ],
    // H10 (brief 08 section 4): the page with its jump list opened, and H11a:
    // with the Assets card opened too, so the check sees the rows behind the
    // disclosure. The jump toggle is display: none from 1280 px, where the
    // list shows whatever its class, so a state click is only taken where it
    // is visible.
    click: ['#model-profile-jump-toggle', '#model-profile-assets-toggle'],
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
  {
    name: 'about',
    // 13 N.7 to N.9: the versions installed on the host, read again at every
    // arrival. The stub's node has one engine with three backend packages, so
    // the page shows the framework's list and the engine's: two `dl.assets`,
    // the engine's `h3` under the first with the gap of 13 N.12.
    set: 'session=admin',
    hash: '#/about',
    expect: [
      { selector: '#page-heading', text: S['about-title'] },
      { selector: 'dl.assets', count: 2 },
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

/** The state's own marker, on the page, or the reason it is not there. A
 *  marker with a `count` is proven rendered when its Nth element is there: the
 *  list has N. */
async function rendered(page, state) {
  for (const want of state.expect) {
    const loc =
      want.text === undefined
        ? page.locator(want.selector)
        : page.locator(want.selector).filter({ hasText: want.text })
    const target =
      want.count === undefined ? loc.first() : loc.nth(want.count - 1)
    try {
      await target.waitFor({ timeout: RENDER_MS })
    } catch {
      const what =
        want.text === undefined
          ? want.selector
          : `${want.selector} "${want.text}"`
      const times = want.count === undefined ? '' : ` x${want.count}`
      return `not rendered: ${what}${times}`
    }
  }
  return null
}

/** The state's `click` list, in order: a selector is clicked only when it is
 *  visible (the jump toggle is `display: none` from 1280 px, where the list
 *  shows whatever its class) and its `aria-expanded="true"` is then awaited. A
 *  visible control that never opens is a finding; one that is not there at this
 *  width is left alone. */
async function clicked(page, state) {
  const found = []
  for (const sel of state.click ?? []) {
    const loc = page.locator(sel)
    if (!(await loc.isVisible())) continue
    try {
      await loc.click()
      await page.locator(`${sel}[aria-expanded="true"]`).waitFor({
        timeout: RENDER_MS,
      })
    } catch {
      found.push(`click: "${sel}" visible, never aria-expanded="true"`)
    }
  }
  return found
}

/** The state's `open`: one control is clicked and the element its click brings
 *  is awaited. The new-tag pill has no `aria-expanded`, so `click` cannot read
 *  it. A control that is not visible at this width is left alone; one that is
 *  visible and brings nothing is a finding. */
async function opened(page, state) {
  if (state.open === undefined) return []
  const { click, then } = state.open
  const loc = page.locator(click)
  if (!(await loc.isVisible())) return []
  try {
    await loc.click()
    await page.locator(then).waitFor({ timeout: RENDER_MS })
  } catch {
    return [`open: "${click}" visible, "${then}" never appeared`]
  }
  return []
}

/** The state's `hover`: the pointer enters `on` and `then` is awaited. The cut
 *  is measured the way 15 section 10 R5 measures it (`scrollWidth >
 *  clientWidth` on the chip's text), because it is the only moment the pill
 *  opens: where the text is whole, opening nothing is what the rule asks, and
 *  that is no finding. The pill stays open for the read that follows. */
async function hovered(page, state) {
  if (state.hover === undefined) return []
  const { on, then } = state.hover
  // `on` names a kind of element the state has several of (the chips of both
  // rows), and strict mode throws on `isVisible` for a selector that matches
  // more than one, so the pointer enters the first: the same chip the cut probe
  // below reads, whose span `querySelector` answers.
  const loc = page.locator(on).first()
  if (!(await loc.isVisible())) return []
  const cut = await page.evaluate((sel) => {
    const span = document.querySelector(`${sel} span`)
    return span !== null && span.scrollWidth > span.clientWidth
  }, on)
  if (!cut) return []
  try {
    // The pill is drawn over its chip (15 section 10 R5), so the open pill
    // covers the target the pointer sits on: the hover is forced, and `then` is
    // the read that says the pill opened.
    await loc.hover({ force: true })
    await page.locator(then).waitFor({ timeout: RENDER_MS })
  } catch {
    return [`hover: "${on}" cut, "${then}" never appeared`]
  }
  return []
}

/** The state on the page: the stub's set, then the route. The stub keeps every
 *  key a /__set did not carry, so the state's own set goes after a reset to
 *  the default list of instances, the default tags and the fast reads: the two
 *  tile states ask for list=many themselves, the tags state for tags=long and
 *  the loading ones for load=session or load=slow, and a state that ran before
 *  them leaves none of them behind for the pages after it. A hash change does
 *  not reload the app, so every state starts from a blank page. */
async function load(page, state) {
  const found = []
  for (const q of ['list=one&tags=few&load=fast', state.set]) {
    const res = await fetch(`${BASE}/__set?${q}`)
    if (!res.ok) {
      found.push(`set request: ${res.status} ${res.statusText}`)
    }
  }
  await page.goto('about:blank')
  // A `hold` state: the stub keeps its reads open for seconds, so the network
  // never goes idle and 'networkidle' would only end at the navigation timeout.
  // The page is at rest once it is parsed: its skeleton is the state to read.
  await page.goto(`${BASE}${state.hash}`, {
    waitUntil: state.hold === true ? 'domcontentloaded' : 'networkidle',
  })
  return found
}

/** The page wider than its viewport: the one line the text pass reads. */
async function sideways(page, width) {
  const scrollWidth = await page.evaluate(
    () => document.documentElement.scrollWidth,
  )
  if (scrollWidth > width) {
    return [`overflow: scrollWidth ${scrollWidth} > ${width}`]
  }
  return []
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
  found.push(...(await sideways(page, width)))
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

/** The text pass: every state, at TEXT_WIDTH in TEXT_THEME, once per mode. The
 *  mode's CSS is added after the page settles, and only its scrollWidth is
 *  read. One line per state with something to say, naming the mode. */
async function textPass(browser, name, lines) {
  const ctx = await browser.newContext({
    viewport: { width: TEXT_WIDTH, height: HEIGHT },
    colorScheme: TEXT_THEME,
  })
  try {
    const page = await ctx.newPage()
    for (const [mode, css] of TEXT_MODES) {
      for (const state of STATES) {
        const found = await load(page, state)
        const missing = await rendered(page, state)
        if (missing !== null) {
          found.push(missing)
        } else {
          found.push(...(await clicked(page, state)))
          found.push(...(await opened(page, state)))
          found.push(...(await hovered(page, state)))
          await sleep(SETTLE_MS)
          await page.addStyleTag({ content: css })
          await sleep(TEXT_SETTLE_MS)
          found.push(...(await sideways(page, TEXT_WIDTH)))
        }
        if (found.length > 0) {
          lines.push(
            `${name} ${TEXT_WIDTH}px ${TEXT_THEME} ${mode} ${state.name}: ${found.join('; ')}`,
          )
        }
      }
    }
  } finally {
    await ctx.close()
  }
}

/** Every width, theme, choice and state in one browser; one line per failing
 *  state, its theme tag `light+dark` when a choice is stored. The text pass
 *  runs last and names its mode. */
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
            const found = await load(page, state)
            const missing = await rendered(page, state)
            if (missing !== null) {
              found.push(missing)
            } else {
              // The clicks run on the rendered page, before the settle: the
              // page at rest is read with them done.
              found.push(...(await clicked(page, state)))
              found.push(...(await opened(page, state)))
              found.push(...(await hovered(page, state)))
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
  await textPass(browser, name, lines)
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
      } catch (e) {
        // One throw in one browser must not lose what every state of every
        // browser produced: the failing line is recorded like a finding, the
        // next browser runs, and the summary prints.
        lines.push(`${name}: check threw: ${e.message.split('\n')[0]}`)
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
  const text = TEXT_MODES.length * STATES.length
  console.log(
    `summary: browsers ${ran.join(' ') || 'none'}${
      skipped.length ? ` (skipped: ${skipped.join(' ')})` : ''
    }; ${STATES.length} states x ${WIDTHS.length} widths x ${
      THEMES.length
    } themes = ${plain}, plus the ${
      CHOICE_STATES.length
    } stored-choice states and the ${TEXT_MODES.length} text modes x ${
      STATES.length
    } states = ${plain + stored + text} per browser; ${lines.length} failing`,
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
