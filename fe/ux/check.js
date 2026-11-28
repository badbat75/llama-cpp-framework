// The UX check: the built web UI's pages at rest, in every browser that
// starts, at 1280 and 320 px, in light and dark, and on three of them with a
// theme choice stored against the OS scheme. Each state is proven rendered
// (its own marker, `expect`), then read by axe-core (the WCAG 2a, 2aa, 21aa and
// 22aa tags) and its scrollWidth compared with the viewport. At 320 px, light
// only, every state is loaded once more per text mode (200 % text, and that
// with the spacing of WCAG 1.4.12), where only the scrollWidth is read. The
// stub bff (stub.py) answers for the server: one per browser, each printing
// the port it bound, because the stub's /__set state is shared and two pages
// on one stub would race. The three browsers run at the same time, and their
// lines print in the fixed browser order.
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
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']
const BROWSERS = ['chromium', 'firefox', 'webkit']
const WIDTHS = [1280, 320]
const THEMES = ['light', 'dark']
const HEIGHT = 900

// The theme axis of 07-hearth-h4-theme.md 4a: nothing stored, so the page
// follows the OS scheme, and a stored choice opposite to it. A choice is only
// read where a session shows the theme button.
const CHOICES = [null, 'opposite']
const CHOICE_STATES = ['overview', 'server-profile', 'model-profile']

// The text pass of 10-reflow.md section 4: every state loaded once more per
// mode, at 320 px in the light scheme only (the modes change no colour, so
// there is nothing for axe to read and nothing for the dark scheme to show).
const TEXT_WIDTH = 320
const TEXT_THEME = 'light'
// The fold of 22 D4.35: below this width the Models table folds its Engine
// column into the name cell, which is where a probe of the folded layout reads.
const NARROW_MAX = 480
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
// Brief 16, 17a.11: the widest server profile of the pool list (stub.py
// `pool=long`), a 32-character id with the 8 tags of 32 characters, run by no
// instance: the row the list page has to fit at 320 px and at 200 % text.
const LONG_POOL_ID = 'abcdefghijklmnopqrstuvwxyz01234x'
// The host's name (stub.py `VERSION`), which the nav's host label reads (20 N.4).
const HOST = 'gpu-box'
// The instances of stub.py `list=many`, as the stub spells them (brief 20
// section 6, and brief 16 17f.12): the 40-character id with no break point, and
// the id of the instance the stub answers by default.
const MANY_ID = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567890123'
const DEFAULT_ID = 'default'
// The label stub.py `list=many` gives the `MANY_ID` row (16 17f.12): the longest
// the rule allows, 64 characters with no break point, so the nav pill and the
// tile have to break the name inside them.
const LONG_LABEL =
  'QWEN327BTHIRTYCHARACTERSTILELABEL0123456789012345678901234567890'
// The label stub.py `list=many` gives the `default` row: a plain one with
// spaces, the shape a name has when it is read.
const OWNED_LABEL = 'Coding assistant'
// The name of the stub's `default` instance (stub.py's one-instance answer,
// port 9931, no label): `instance-name-auto` with the engine and the port,
// which 16 17f.1 puts in the page's heading, the nav and the tile.
const DEFAULT_NAME = 'llama.cpp :9931'
// The id line of 16 17f.3 and 17f.4, as the strings file spells it. Absent
// from the strings file until 17f.11 adds the key, where the marker's text is
// the key's own placeholder, so the state fails, as it asks.
const ID_LINE = (id) => (S['instance-id-shown'] ?? '{0}').replace('{0}', id)
// The instance page's own id line, which 16 17f2.2 reads `instance-id-fixed`
// ("ID {0}, does not change"): the tiles and the Used by link keep the short
// key above. Absent from the strings file until 17f2 lands, where the state
// fails, as it asks.
const ID_FIXED = (id) => (S['instance-id-fixed'] ?? '{0}').replace('{0}', id)
// The host's list, where the nav's instance links live (20 N.3).
const NAV_HOST = '#nav ul[aria-labelledby="nav-host"]'
// The display name pencil of 16 17f2.1, named by the key 17f2.10 adds: absent
// from the strings file it names a button of the text "undefined", so the state
// fails until the slice lands, as it asks.
const NAME_EDIT = `main button:has-text("${S['instance-name-edit']}")`
// The instance page's server profile switch of 16 17d.6a, named by the key
// 17d.11 adds: absent from the strings file it names a button of the text
// "undefined", so the state fails until the slice lands, as it asks.
const SWITCH_BUTTON = `main button:has-text("${S['instance-server-switch']}")`
// The start of the delete's question (brief 22, 22 D.2): the string
// `model-profile-delete-question` up to its `{0}`, which the page fills with the
// profile's name. It names the Confirm group, whose selector alone is not the
// page's only group (every Field is a `fieldset`). Absent from the strings file
// until 22D lands, where the marker names a text of "undefined" and the state
// fails, as it asks.
const DELETE_QUESTION = S['model-profile-delete-question']?.split('{0}')[0]

// The refusal of a delete an instance answers with 409 `instance-profile-in-use`
// (brief 26, 26.3): the bff's words, with the id in place.
const IN_USE = (id) =>
  (S['api-instance-profile-in-use'] ?? '{0}').replace('{0}', id)

// Brief 26, slice IP6b (26.8): the add of the Model profiles section, named by
// the key the slice adds. Absent from the strings file it names a button of the
// text "undefined", so the state fails until the slice lands, as it asks.
const IP_ADD = `main button:has-text("${S['instance-profile-models-add']}")`

// The states to look at: the stub's /__set query, the route, the expect: the
// selectors (with the text they carry) only that state shows, the optional
// click: selectors clicked in order once the state is rendered (each one only
// when it is visible, and then awaited open), the optional open: one control
// clicked and the element its click brings awaited, the optional hover: the
// pointer entering one element and the element its cut text brings awaited, the
// optional menu: the Menu button clicked before the markers are read, for a
// state that reads the nav in the narrow layout, where the nav is `display:
// none` until it opens (at 1280 px the nav is the column, the button is not
// there, and the markers are read as they stand), and
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
      {
        // 16 17f.3: the tile's link reads the name, which is the label where
        // there is one, so the widest name the tile has to fit is the 64-character
        // label with no break point.
        selector: '.tile-id',
        count: 4,
        message: 'one name per tile',
      },
      {
        selector: '.tile-id',
        text: LONG_LABEL,
        message: 'the 64-character label, unbroken, inside its tile',
      },
      {
        selector: '.tile-id',
        text: OWNED_LABEL,
        message: 'the label of `default`, with its spaces',
      },
      {
        // 16 17f.3: the id rides under the name, and is absent from the tile
        // whose name IS the id (`embed`, port 0, no label): 4 tiles, 3 lines.
        selector: '.tile .instance-id',
        count: 3,
        message: 'the id line of every tile whose name is not the id',
      },
      {
        selector: '.tile .instance-id',
        text: ID_LINE(MANY_ID),
        message: 'the 40-character id, unbroken, under its label',
      },
      {
        // 24 A1.1: every tile ends with the instance page's three run verbs,
        // in the row that wraps inside the tile: 4 tiles, 12 verbs of 40 px,
        // the targets this check reads at 320 px and at 200 % text.
        selector: '.tile .run-actions button',
        count: 12,
        message: 'the three verbs of every tile',
      },
    ],
  },
  {
    // Brief 20 section 6 (the N1 review, D2), as brief 16 17f.2 amends it: the
    // nav with the menu opened. At 1280 px the nav is the column and its markers
    // are read as they stand; at 320 px the nav is `display: none` until it
    // opens (index.css 627), so `menu: true` clicks the Menu button first.
    // `list=many` gives the links the 40-character id and the ids with a server
    // profile, so the check reads the open menu and the long id at 320 px in
    // both themes (axe and the scrollWidth), and again at 200 % text, where the
    // text pass runs every state. The link's text is the name alone, so the long
    // id is the whole of the widest pill.
    name: 'nav-open',
    set: 'session=admin&list=many',
    hash: '#/',
    menu: true,
    // The IP6a review R1: the nav column at the wide widths, read by `navColumn`.
    probe: 'nav-column',
    expect: [
      { selector: '#nav-nodes', text: S['nav-nodes'] },
      { selector: '#nav-host', text: HOST },
      {
        selector: `${NAV_HOST} a[href^="#/instances/"]`,
        count: 4,
        message: 'one link per instance of list=many, in the manager order',
      },
      {
        selector: `${NAV_HOST} .nav-instance`,
        count: 4,
        message: 'the name of each link, in the manager order',
      },
      {
        // 16 17f.2: the entry is the name alone, so the widest pill is the
        // 64-character label with no break point to find.
        selector: `${NAV_HOST} .nav-instance`,
        text: LONG_LABEL,
        message: 'the 64-character label, unbroken, inside its pill',
      },
      {
        selector: `${NAV_HOST} .nav-instance`,
        text: OWNED_LABEL,
        message: 'the label of `default`, with its spaces',
      },
      {
        selector: `${NAV_HOST} .nav-instance`,
        text: 'llama.cpp :8580',
        message: '`coder`, unlabelled with a port, named by engine and port',
      },
      {
        selector: `${NAV_HOST} .nav-instance`,
        text: 'embed',
        message: '`embed`, unlabelled and with no port, named by its id',
      },
      // The entry holds the name and nothing else (17f.2: no server profile, no
      // id, no run only a reader says). A marker proves a thing is there, so the
      // check reads the four names above; the absence of the rest is pinned by
      // InstanceName17f.test.tsx.
    ],
  },
  {
    // Brief 16, slice 17a: the Server form moved to the server profile's own
    // page, so the form's state is that page (the markers are the same: the h1
    // and the first field's label). key=shown: with the API key withheld from
    // the session the bff fails the whole read, and the form never shows.
    name: 'server-profile',
    set: 'session=admin&key=shown',
    hash: '#/profiles/server/default',
    expect: [
      { selector: '#page-heading', text: 'default' },
      { selector: '[id$="-Port-label"]', text: S['server-port'] },
    ],
  },
  {
    // Brief 16, 17a.10: the instance page as it is now, with no Server form:
    // its header, its tags field and the section naming its server profile.
    // 16 17f.1: the heading is the instance's name, which the stub's port spells
    // out of the id.
    name: 'instance-page',
    set: 'session=admin',
    hash: '#/instances/default',
    expect: [
      { selector: '#page-heading', text: DEFAULT_NAME },
      // 16 17f.4, as 17f2.2 rewrites it: the id rides right after the name and
      // the page says it does not change.
      { selector: 'p.instance-id', text: ID_FIXED(DEFAULT_ID) },
      { selector: 'h2', text: S['instance-server-title'] },
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
      { selector: '#page-heading', text: DEFAULT_NAME },
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
      { selector: '#page-heading', text: DEFAULT_NAME },
      { selector: '.tag-add' },
    ],
    open: { click: '.tag-add', then: '.tag-new input' },
  },
  {
    // Brief 16, slice 17f2 (17f2.10): the display name form opened from the
    // pencil on the instance page, on the name the stub's port spells. The
    // marker names the key 17f2.10 adds: absent from the strings file it names
    // a button of the text "undefined", so the state fails until the slice
    // lands, as it asks. The box the click opens is the page's only form input:
    // the tags field is chips, its pill closed.
    name: 'instance-name-edit',
    set: 'session=admin',
    hash: '#/instances/default',
    expect: [
      { selector: '#page-heading', text: DEFAULT_NAME },
      { selector: NAME_EDIT, message: 'the pencil, right after the name' },
    ],
    open: { click: NAME_EDIT, then: 'main form input' },
  },
  {
    // Brief 16, slice 17e (17e.10, as its 17e check of 2026-11-25 amends it):
    // the instance page's Model profiles section, on the stub's three rows
    // (stub.py `INSTANCE_MODELS`), one of them the widest id of the pool. The
    // markers name no class of the slice: the section's h2, its table's rows,
    // the row switches and the section's checkbox beside the page's own Start
    // at boot. The table box scrolls inside itself at 320 px and at 200 % text,
    // and every control stays within 1 px of its row's centre.
    name: 'instance-models',
    set: 'session=admin',
    hash: '#/instances/default',
    expect: [
      { selector: 'h2', text: S['instance-models-title'] },
      { selector: 'main tbody tr', count: 3, message: 'one row per profile' },
      {
        selector: 'main [role="switch"]',
        count: 3,
        message: 'the switch of each row (17e.4a)',
      },
      {
        selector: 'main input[type="checkbox"]',
        count: 2,
        message: "the section's checkbox, beside Start at boot",
      },
    ],
  },
  {
    // Brief 16, slice 17d (as its 17d check of 2026-11-26 amends it, 17d.6a,
    // 17d.10): the instance page's Server profile section with a free server
    // profile to switch to, at rest. stub.py `pool=long` adds the widest row
    // the rule allows, run by no instance (its id is LONG_POOL_ID, 32
    // characters, port 9931), so the select has one option to cut after the id
    // at 320 px (17c.11a) and the button sits under it. The button's text is
    // the key 17d.11 adds: absent from the strings file it names a button of
    // the text "undefined", so the state fails until the slice lands, as it
    // asks. The page carries its own Restart in the head, so the section shows
    // no offer (17d.6a).
    name: 'instance-server-switch',
    set: 'session=admin&pool=long',
    hash: '#/instances/default',
    expect: [
      { selector: 'h2', text: S['instance-server-title'] },
      {
        selector: 'main select',
        message: 'the switch’s select, in the field layout under the row',
      },
      {
        selector: SWITCH_BUTTON,
        message: 'the switch button, after the select',
      },
    ],
  },
  {
    // Brief 16, slice 17c (17c.1a, as the 17c check of 2026-11-25 amends it):
    // the Overview's create form, opened from the `+` that ends the instances
    // list. The opener carries the plus of Icon.tsx, so its selector names the
    // icon and not a text the strings file does not carry yet (the tile's run
    // verbs are text buttons, so the vertical stroke is the `+`'s own);
    // `main form` is the form 17c.1a asks for, the page's only form.
    name: 'instances-new',
    set: 'session=admin',
    hash: '#/',
    expect: [
      { selector: '#page-heading', text: S['overview-title'] },
      {
        selector: 'main button:has(svg path[d="M12 5v14"])',
        message: 'the opener of the create form: the button with the plus',
      },
    ],
    open: {
      click: 'main button:has(svg path[d="M12 5v14"])',
      then: 'main form',
    },
  },
  {
    // Brief 16, slice 17c (17c.6a and 17c.7a, as the 17c check amends them):
    // the instance page of a stopped instance whose record can be removed
    // (stub.py `inst=inactive&removable=1`), with the Confirm in place of the
    // remove open. The verb is the last button of the head's run row, and the
    // group opens on the line under it: its markers are the ids 17c.7a gives
    // the two buttons, so no state names a class of the slice.
    name: 'instance-remove',
    set: 'session=admin&inst=inactive&removable=1',
    hash: '#/instances/default',
    expect: [
      { selector: '#page-heading', text: DEFAULT_NAME },
      {
        selector: 'main .run-actions button',
        count: 4,
        message: 'Start, Stop, Restart and the remove verb, last (17c.6a)',
      },
    ],
    open: {
      click: 'main .run-actions button:last-of-type',
      then: '#confirm-cancel',
    },
  },
  {
    // Brief 16, slice 17a (17a.11): the pool list at its widest row, so the
    // check reads the table box and the tags cell with a 32-character id and
    // 8 tags of 32 characters in it.
    name: 'server-profiles',
    set: 'session=admin&pool=long',
    hash: '#/profiles/server',
    expect: [
      { selector: '#page-heading', text: S['server-profiles-title'] },
      {
        selector: `a[href="#/profiles/server/${LONG_POOL_ID}"]`,
        text: LONG_POOL_ID,
      },
    ],
  },
  {
    // Brief 16, slice 17b (17b.11): the create form of the list page, opened
    // on the widest pool, so the check reads the fields, the buttons and the
    // long table together at 320 px and at 200 % text. The opener carries the
    // plus of Icon.tsx, so its selector names the icon, not a text the strings
    // file does not carry yet; `main form` is the form 17b.1 asks for, the
    // page's only form.
    name: 'server-profiles-new',
    set: 'session=admin&pool=long',
    hash: '#/profiles/server',
    expect: [
      { selector: '#page-heading', text: S['server-profiles-title'] },
      {
        // The plus of Icon.tsx has two strokes, and the vertical one is the
        // plus's own, so this is the only button it finds: since 22 D4.21 the
        // rows' delete is the trash can, so no row of the widest pool carries
        // the horizontal stroke any more (22 D4 review D3, 22 D4.21).
        selector: 'main button:has(svg path[d="M12 5v14"])',
        message: 'the opener of the create form: the button with the plus',
      },
    ],
    open: {
      click: 'main button:has(svg path[d="M12 5v14"])',
      then: 'main form',
    },
  },
  {
    // Brief 26, slice IP6a (26.2, the brief's item 15): the instance profile
    // list at its widest row, so the check reads the table box, the count
    // cells and the Used by cell at 320 px and at 200 % text. stub.py answers
    // GET /api/profiles/instance with three profiles, one of them used by an
    // instance (its trash can is switched off) and one with no model.
    name: 'instance-profiles',
    set: 'session=admin',
    hash: '#/profiles/instance',
    // The IP6a review R2 and R3: the folded list below 480 px and the words of
    // its cells at 320 px, read by `instanceListNarrow`.
    probe: 'instance-profiles-narrow',
    expect: [
      { selector: '#page-heading', text: S['instance-profiles-title'] },
      {
        selector: 'a[href="#/profiles/instance/coder"]',
        text: 'coder',
      },
    ],
  },
  {
    // Brief 26, slice IP6a (26.4, the brief's item 15): the create form of the
    // list page, opened in place. As `server-profiles-new`, the opener carries
    // the plus of Icon.tsx, so its selector names the icon.
    name: 'instance-profiles-new',
    set: 'session=admin',
    hash: '#/profiles/instance',
    expect: [
      { selector: '#page-heading', text: S['instance-profiles-title'] },
      {
        selector: 'main button:has(svg path[d="M12 5v14"])',
        message: 'the opener of the create form: the button with the plus',
      },
    ],
    open: {
      click: 'main button:has(svg path[d="M12 5v14"])',
      then: 'main form',
    },
  },
  {
    // Brief 26, 26.3, as the IP6a review reads it: the refusal of a delete the
    // bff answers 409 `instance-profile-in-use`. The pool at rest has a row whose
    // trash acts (`coder`: the stub lists no instance running it), and its
    // DELETE the stub refuses (`instuse=race`), the answer of a page that raced
    // an instance taking the profile. The chain opens the confirm and presses
    // Confirm, which deletes nothing, so the row stays and the list is read
    // again. The words are the bff's `{key, args}` body: a refusal in any other
    // shape shows the generic "could not be deleted" here, not this sentence.
    name: 'instance-profiles-in-use',
    set: 'session=admin&instuse=race',
    hash: '#/profiles/instance',
    expect: [
      { selector: '#page-heading', text: S['instance-profiles-title'] },
      {
        selector: 'main tbody .ot-add:not([aria-disabled="true"])',
        message: 'the − of a row no instance runs, which can act (26.3)',
      },
    ],
    opens: [
      {
        click: 'main tbody .ot-add:not([aria-disabled="true"])',
        then: 'main tr.confirm-row [role="group"]',
      },
      {
        click: 'main #confirm-delete',
        then: `main [role="alert"]:has-text("${IN_USE('coder')}")`,
      },
    ],
  },
  {
    // Brief 26, slice IP6b (26.6 and 26.7, the brief's item 15): one instance
    // profile's page with the two sections under its head, on the stub's `coder`
    // profile: the Server profile section, whose field is on screen because
    // stub.py's pool has a server profile besides the one it runs, and the
    // Model profiles section with its three entries, one of them the widest id
    // of the pool. The check reads the page at 1280 px and at 320 px, and at
    // 200 % text: the two sections, the switch's field and the table box.
    name: 'instance-profile',
    set: 'session=admin',
    hash: '#/profiles/instance/coder',
    // The IP6b review R7: the Model profiles table at 320 px, read by
    // `instanceProfileModelsNarrow` (no cell breaks inside a word, the table no
    // wider than its box).
    probe: 'instance-profile-models-narrow',
    expect: [
      { selector: '#page-heading', text: 'coder' },
      { selector: `main h2:has-text("${S['instance-profile-server-title']}")` },
      { selector: `main h2:has-text("${S['instance-profile-models-title']}")` },
      {
        selector: 'main section select',
        count: 1,
        message: 'the Server profile section has its one field (26.6)',
      },
      {
        selector: 'main table tbody tr',
        count: 3,
        message: 'one row per model entry of the profile (26.7)',
      },
    ],
  },
  {
    // Brief 26, slice IP6b (26.8, the brief's item 15): the add form of the
    // Model profiles section, opened in place. The chain is the section's `+`,
    // then the form's primary verb with nothing chosen: the page check says
    // `field-error-choose` under the select and sends nothing, so the rows
    // stay. `office` is the profile with no entry, so the select lists the
    // whole pool and the form has room to show its words at 320 px.
    name: 'instance-profile-add',
    set: 'session=admin',
    hash: '#/profiles/instance/office',
    expect: [
      { selector: '#page-heading', text: 'office' },
      { selector: `main h2:has-text("${S['instance-profile-models-title']}")` },
      { selector: IP_ADD, message: 'the section has its add (26.8)' },
    ],
    opens: [
      { click: IP_ADD, then: 'main form select' },
      {
        click: 'main form button.primary',
        then: `main form:has-text("${S['field-error-choose']}")`,
      },
    ],
  },
  {
    // Brief 26, slice IP6b (26.8, the brief's item 15): the confirm of a row's
    // minus, opened in place under the row of the widest model id of the pool.
    // The chain stops at the red verb: the check never removes an entry, so the
    // rows stay for every pass. The words name the row, so the group has a name
    // of its own and the question wraps at 320 px.
    name: 'instance-profile-remove',
    set: 'session=admin',
    hash: '#/profiles/instance/coder',
    expect: [
      { selector: '#page-heading', text: 'coder' },
      {
        selector: 'main table tbody tr',
        count: 3,
        message: 'the rows are there before the minus is pressed',
      },
    ],
    opens: [
      {
        click: 'main tbody .ot-add:not([aria-disabled="true"])',
        then: 'main [role="group"] button.danger',
      },
    ],
  },
  {
    // Brief 16, slice 17b (17b.7) as 22D4 amends it (22 D4.9, 22 D4.10): the
    // Confirm in place of the delete, opened from the Save bar's Delete, on the
    // page of the widest profile of `pool=long`, which runs nowhere (stub.py
    // answers that id with `instance` null). 22 D2.8 put the opener in a status
    // row under the heading; 22 D4.9 takes that row away and 22 D4.10 makes the
    // opener the bar's first button, a text button keeping the id the brief
    // gives it.
    name: 'server-profile-delete',
    set: 'session=admin&pool=long',
    hash: `#/profiles/server/${LONG_POOL_ID}`,
    expect: [
      { selector: '#page-heading', text: LONG_POOL_ID },
      {
        selector: '#server-profile-delete',
        message:
          "the Save bar's Delete, offered while the profile runs nowhere",
      },
    ],
    open: {
      click: '#server-profile-delete',
      then: 'main [role="group"]',
    },
  },
  {
    // Brief 22, 22 D4.10 and 22 D4.3, as the 22D4 review's D3 asks, as 22 D4.14
    // and 22 L2.B2 (the user's ruling of 2026-11-26) amend it: the Save bar's
    // Delete of a profile an instance runs, switched off, with its reason. The
    // page of `default` is the one where the verb cannot act (stub.py answers
    // its read with `instance` "default", under `pool=long` as under every
    // pool). 22 L2.B2 takes the reason line away: the reason is a visually
    // hidden element the Delete names with aria-describedby, and the eye reads
    // it only as the floating .hint-tip, on hover and on focus, so the check
    // opens the tip with the keyboard and reads it there, at 320 px and at
    // 200 % text, where it must not push the page sideways and must stay in
    // the viewport.
    name: 'server-profile-delete-disabled',
    set: 'session=admin&pool=long',
    hash: '#/profiles/server/default',
    expect: [
      { selector: '#page-heading', text: 'default' },
      {
        selector: '#server-profile-delete[aria-disabled="true"]',
        message:
          "the Save bar's Delete, switched off: an instance runs the profile",
      },
    ],
    focus: {
      on: '#server-profile-delete',
      then: 'main .hint-tip',
    },
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
    // Brief 22, slice IP5, review A3 to A5: the Models list in the narrow
    // layout, its rows listed by the widest instance profile ids the pool takes
    // (stub.py `usedby=long`). The state's `probe` reads the three rules the
    // folded table owes the eye below 480 px: the engine line under each name
    // (22 D4.35, kept by IP5.1), the head of a column standing over its column
    // (22 D4.25), and a Used by id broken only at a break point of its own
    // (IP5.2, with the D4.37 rule that names it).
    name: 'model-profiles-narrow',
    set: 'session=admin&usedby=long',
    hash: '#/profiles/model',
    expect: [
      { selector: '#page-heading', text: S['model-profiles-title'] },
      {
        selector: 'main table.model-profiles tbody tr',
        message: 'a row of the Models list',
      },
    ],
    probe: 'model-profiles-narrow',
  },
  {
    // Brief 22, slice 22C1 (22 C.10a): the Models list's New model profile form,
    // opened on the file choice, so the check reads the picker, the Model file
    // combobox and its options at 320 px and at 200 % text. The stub answers the
    // folder read with the 40-character path 22 C.10a names (stub.py
    // `MODEL_FILES`), beside a model in a subfolder and a split model.
    // The opener is the + the brief names `model-profiles-new` (22 C.1a), after
    // the note under the table; `main form` is the form C.1 asks for, the page's
    // only form. The state names the opener by its id, so it fails until the
    // slice lands, as 22 C.10a asks.
    name: 'model-profiles-new',
    set: 'session=admin',
    hash: '#/profiles/model',
    expect: [
      { selector: '#page-heading', text: S['model-profiles-title'] },
      {
        selector: 'main #model-profiles-new',
        message:
          'the opener of the create form: the + after the note (22 C.1a)',
      },
    ],
    open: {
      click: 'main #model-profiles-new',
      then: 'main form',
    },
  },
  {
    // Brief 22, slice 22C2 (22 C.10a): the New model profile form on the
    // Hugging Face choice, with a repo listed, so the check reads the repo
    // field, the Show quants button and the quants fieldset of 12 radios (one
    // of them split in 3 parts, one under 0.1 GiB, one llama.cpp's default) at
    // 320 px and at 200 % text, where 22 P.12 asks that they wrap with no
    // sideways scroll. The stub answers the listing (stub.py `HF_FILES`).
    // The chain is the opener, the radio, the repo written, Show quants: the
    // keys the steps name are the ones 22 P.12 adds, so a missing key names a
    // control of the text "undefined" and the state fails, as it asks.
    name: 'model-profiles-new-hf',
    set: 'session=admin',
    hash: '#/profiles/model',
    // 22C2 review D1: the expect list is read on the loaded page, before the
    // chain runs (rendered() then the opens below), so the quants fieldset is
    // not here yet: the chain's last step awaits it (22 P.9).
    expect: [{ selector: '#page-heading', text: S['model-profiles-title'] }],
    opens: [
      { click: 'main #model-profiles-new', then: 'main form' },
      {
        click: `main form label:has-text("${S['model-source-hf']}")`,
        then: `main form input:below(label:has-text("${S['model-profile-hf-repo']}"))`,
      },
      {
        fill: `main form input:below(label:has-text("${S['model-profile-hf-repo']}"))`,
        value: 'ggml-org/gemma-3-1b-it-GGUF',
      },
      {
        click: `main form button:has-text("${S['hf-show-quants']}")`,
        then: `main form fieldset:has(legend:has-text("${S['hf-quant-legend']}"))`,
      },
    ],
  },
  {
    // Brief 22, slice 22D4 (22 D4.1, 22 D4.4) as 22 IP5.3 amends it: the Models
    // list with the confirm row of a profile no instance profile lists open. The
    // pool at rest has profiles the stub's `usedBy` leaves empty (stub.py
    // `MODEL_PROFILES`), so their − can act with no set beyond the session. The
    // row's − has an id the page draws from
    // React's own counter (no id of the brief), so the state names it by the
    // cell's box: the − that can act carries no `aria-disabled`. Its confirm is
    // a `tr` of the table, its group named by the list's question.
    // 22C1 review D3: the list's + for a new profile (22 C.1) carries `ot-add`
    // too and sits after the table, so both selectors are scoped to `tbody`,
    // which holds the rows' − only.
    name: 'model-profiles-delete',
    set: 'session=admin',
    hash: '#/profiles/model',
    expect: [
      { selector: '#page-heading', text: S['model-profiles-title'] },
      {
        selector: 'main tbody .ot-add:not([aria-disabled="true"])',
        message: 'the − of a switched-off row (22 D4.1)',
      },
    ],
    open: {
      click: 'main tbody .ot-add:not([aria-disabled="true"])',
      then: 'main tr.confirm-row [role="group"]',
    },
  },
  {
    // Brief 22, slice 22D4 (22 D4.2, 22 D4.3) as 22 IP5.3 amends it: the − that
    // cannot act, with its reason open. The pool at rest has profiles an instance
    // profile lists, so their − are `aria-disabled` and keep the focus; the
    // keyboard reaches one and the
    // reason shows under it. The tip is what the check reads: at 320 px and at
    // 200 % text it must not push the page sideways.
    name: 'model-profiles-delete-tip',
    set: 'session=admin',
    hash: '#/profiles/model',
    expect: [
      { selector: '#page-heading', text: S['model-profiles-title'] },
      {
        selector: 'main .ot-add[aria-disabled="true"]',
        message:
          'the − of a row an instance profile uses, blocked but focusable',
      },
    ],
    focus: {
      on: 'main .ot-add[aria-disabled="true"]',
      then: 'main .hint-tip',
    },
  },
  {
    // Brief 22, slice IP5 (IP5.4, IP5.5): the page of a model profile. The
    // state line's switch, the state line's words and the four placement cards
    // are gone, so the marker the check reads is the note the page puts under
    // the head, beside the first card (IP5.5). The key is IP5.8's new one, so
    // the state fails until the slice lands, as it asks.
    //
    // IP5 retires two states of earlier slices and moves two: 16 17d's
    // model-profiles-restart-offer and 24 A2's model-profile-switch, whose
    // switch the slice removes, and 19 19 G.4's model-profile-blocks and 21
    // section 7's model-profile-placement, whose cards brief 26 gives to the
    // instance profile's entry page.
    name: 'model-profile',
    set: 'session=admin',
    hash: `#/profiles/model/${MODEL_PROFILE_ID}`,
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      { selector: 'h3', text: S['model-profile-assets'] },
      {
        selector: 'main p.intro',
        text: `${S['model-profile-placement-elsewhere']}`,
        message:
          'the note that says where the placement is set (IP5.5): the key is' +
          ' IP5.8\'s new one, so the state reads the text "undefined" and fails' +
          ' until the slice lands, as the check asks (a raw `S[key]` here would be' +
          ' undefined and the harness would read the selector alone).',
      },
    ],
  },
  {
    // Brief 22, slice IP5 (IP5.6), as 22 L2.B2 keeps it: the page of a model
    // profile an instance profile lists, whose Save bar Delete cannot act. The
    // stub answers the read with usedBy naming the instance profile (stub.py,
    // `used=1`), so the Delete is `aria-disabled` and keeps the focus, and its
    // reason, the IP5.3 `model-profiles-delete-in-use` the check reads at 320 px
    // and at 200 % text, shows as the floating tip. The reason's key is IP5.8's
    // new one, so the state fails until the slice lands, as it asks.
    name: 'model-profile-delete-tip',
    set: 'session=admin&used=1',
    hash: `#/profiles/model/${MODEL_PROFILE_ID}`,
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      {
        selector: '#model-profile-delete[aria-disabled="true"]',
        message: "the Save bar's Delete of a model an instance profile uses",
      },
    ],
    focus: {
      on: '#model-profile-delete',
      then: 'main .hint-tip',
    },
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
    // Brief 22, slice 22D (22 D.1 and 22 D.2) as 22D4 amends it (22 D4.10): the
    // Confirm in place of the delete, opened from the Save bar's Delete, on the
    // page of a model profile no instance profile lists (stub.py's document
    // reads `usedBy` empty, and answers the DELETE with 204, IP5.6). 22 D2.2
    // put the opener in the
    // status row under the summary line; 22 D4.9 takes that row away and 22
    // D4.10 makes the opener the bar's first button, a text button keeping the
    // id the brief gives it.
    name: 'model-profile-delete',
    set: 'session=admin',
    hash: `#/profiles/model/${MODEL_PROFILE_ID}`,
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      {
        selector: '#model-profile-delete',
        message:
          "the Save bar's Delete, offered while no instance profile lists the model",
      },
    ],
    open: {
      click: '#model-profile-delete',
      // The Confirm group, named by its question: `main [role="group"]` alone
      // is not the page's only group (every Field is a `fieldset`), so the
      // marker asks for the group holding the question's text.
      then: `main [role="group"]:has(p:has-text("${DELETE_QUESTION}"))`,
    },
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
    // 18 AN.2 and AN.3: the framework's version and the operating system, read
    // again at every arrival, in one list of two rows. The host's engines and
    // GPUs are the node page's now (18 AN.8), not this page's.
    set: 'session=admin',
    hash: '#/about',
    expect: [
      { selector: '#page-heading', text: S['about-title'] },
      { selector: 'dl.assets', count: 1 },
    ],
  },
  {
    name: 'node',
    // 18 AN.8: the host's top list, then the `node-engines` heading with one
    // list per engine, then the `node-gpus` heading with one block per GPU.
    // The stub's node has one engine and three GPUs: five `dl.assets`.
    set: 'session=admin&node=ok',
    hash: '#/node',
    expect: [
      { selector: '#page-heading', text: S['node-title'] },
      { selector: 'h2', text: S['node-engines'] },
      { selector: 'h2', text: S['node-gpus'] },
      { selector: 'dl.assets', count: 5 },
    ],
  },
  {
    name: 'node-down',
    // 18 AN.13: the manager did not answer, so `os`, `vulkan` and `gpus` are
    // null: the top and the engine lists stay (2) and the notice says why.
    set: 'session=admin&node=down',
    hash: '#/node',
    expect: [
      { selector: '#page-heading', text: S['node-title'] },
      { selector: 'dl.assets', count: 2 },
      { selector: 'p', text: S['node-info-unavailable'] },
    ],
  },
  {
    name: 'node-long',
    // 18 AN.14: the widest the page takes, 32 GPUs of 256 unbroken characters
    // and a host name of 253. Every text has to break inside its block: the
    // check's own scrollWidth read, and the 200 % text pass, prove no sideways
    // scroll at 320 px.
    set: 'session=admin&node=long',
    hash: '#/node',
    expect: [
      { selector: '#page-heading', text: S['node-title'] },
      { selector: 'h2', text: S['node-gpus'] },
      { selector: 'h3', count: 32, message: 'one heading per GPU row' },
    ],
  },
  {
    name: 'loading-node',
    // 18 AN.9 (17a): the node read held, the session answering, so the page is
    // the skeleton of its first read: six lines, the loading text in the DOM
    // and out of sight.
    set: 'session=admin&load=slow',
    hash: '#/node',
    hold: true,
    expect: [
      { selector: '#page-heading', text: S['node-title'] },
      { selector: 'main .skeleton', count: 6 },
      { selector: '[role="status"]' },
      { selector: '.visually-hidden' },
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

/** The state's `menu`: the Menu button, clicked before the markers are read.
 *  Below 960 px the nav is `display: none` until the menu opens (index.css
 *  627), so a state that reads the nav at 320 px has to open it first, and its
 *  markers are the open nav. From 960 px the button is `display: none` (index.css
 *  668) and the nav is the column, so the click is left alone there. A visible
 *  button that never reports `aria-expanded="true"` is a finding, as in
 *  `clicked`. */
async function menuOpened(page, state) {
  if (state.menu !== true) return []
  const loc = page.locator('#menu-button')
  if (!(await loc.isVisible())) return []
  try {
    await loc.click()
    await page
      .locator('#menu-button[aria-expanded="true"]')
      .waitFor({ timeout: RENDER_MS })
  } catch {
    return ['menu: "#menu-button" visible, never aria-expanded="true"']
  }
  return []
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

/** The state's `opens`: the steps of it run in order, each `click` one control
 *  clicked and the element its click brings awaited, each `fill` one input
 *  written. It is `open` for a chain: the New model profile form's Hugging Face
 *  half needs the opener, the radio, the repo and Show quants in a row
 *  (22 C.10a, the state `model-profiles-new-hf`). A control that is not visible
 *  at this width ends the chain; one that is visible and brings nothing is a
 *  finding, and the chain stops there. */
async function openedChain(page, state) {
  const found = []
  for (const step of state.opens ?? []) {
    const sel = step.click ?? step.fill
    const loc = page.locator(sel).first()
    if (!(await loc.isVisible())) return found
    try {
      if (step.fill !== undefined) await loc.fill(step.value)
      else {
        await loc.click()
        await page.locator(step.then).first().waitFor({ timeout: RENDER_MS })
      }
    } catch {
      return [
        `opens: "${sel}" visible, "${step.then ?? step.value}" never used`,
      ]
    }
  }
  return found
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

/** The state's `focus`: the keyboard focus moves to `on` and `then` is awaited.
 *  The reason a − that cannot act shows on focus (22 D4.3) is the moment this
 *  reads: `aria-disabled` keeps the control focusable (22 D4.2), so the keyboard
 *  reaches it and the tip opens. A control that is not visible at this width is
 *  left alone; one that is visible and brings nothing is a finding. */
async function focused(page, state) {
  if (state.focus === undefined) return []
  const { on, then } = state.focus
  const loc = page.locator(on).first()
  if (!(await loc.isVisible())) return []
  try {
    await loc.focus()
    await page.locator(then).waitFor({ timeout: RENDER_MS })
  } catch {
    return [`focus: "${on}" visible, "${then}" never appeared`]
  }
  return []
}

/** The Models list in the narrow layout, read as the page stands below the
 *  480 px fold where the table folds (22 D4.35): the three rules brief 22's IP5
 *  review adds, on the rows the stub lists by the widest instance profile ids
 *  (stub.py `usedby=long`).
 *  - A3 (22 D4.35, kept by IP5.1): the Engine column is folded away, so every
 *    row says what runs it: the name cell's engine line is visible and holds
 *    the words of the Engine column's cell.
 *  - A4 (22 D4.25): the head of a column stands over its column: a header
 *    cell's left and right edges are its body cells' edges, within 1 px.
 *  - A5 (IP5.2, with the D4.37 rule that names the id): a Used by id breaks
 *    only at a break point of its own, a hyphen or a space: no line of an id
 *    starts inside a word. */
async function modelsListNarrow(page, width) {
  if (width > NARROW_MAX) return []
  return page.evaluate(() => {
    const found = []
    const table = document.querySelector('table.model-profiles')
    if (table === null) return ['models list: no table.model-profiles']
    const taken = (el) => {
      const s = getComputedStyle(el)
      const r = el.getBoundingClientRect()
      return s.display === 'none' || r.width < 2 || r.height < 2
    }
    const word = (el) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
    const rows = [...table.querySelectorAll('tbody tr')].filter(
      (tr) => !tr.classList.contains('confirm-row'),
    )
    const cells = (tr) => [...tr.children].filter((c) => c.tagName === 'TD')
    // A3: the engine line of each row.
    for (const tr of rows) {
      const td = cells(tr)
      const line = td[0]?.querySelector('.muted')
      if (line === null || line === undefined) {
        found.push(`engine line: the name cell of "${word(td[0])}" holds none`)
        continue
      }
      if (taken(line)) {
        found.push(`engine line: not visible in the row "${word(td[0])}"`)
        continue
      }
      const column = td[1]
      if (column !== undefined && word(column) !== word(line)) {
        found.push(
          `engine line: "${word(line)}" is not the Engine column "${word(column)}" of the row "${word(td[0])}"`,
        )
      }
    }
    // A4: each head over its column.
    const heads = [...table.querySelectorAll('thead th')]
    heads.forEach((th, n) => {
      if (taken(th)) return
      const head = th.getBoundingClientRect()
      for (const tr of rows) {
        const td = cells(tr)[n]
        if (td === undefined || taken(td)) continue
        const cell = td.getBoundingClientRect()
        if (
          Math.abs(head.left - cell.left) > 1 ||
          Math.abs(head.right - cell.right) > 1
        ) {
          found.push(
            `column ${n + 1} "${word(th)}": head ${Math.round(head.left)}..${Math.round(head.right)}, body cell of the row "${word(cells(tr)[0])}" ${Math.round(cell.left)}..${Math.round(cell.right)}`,
          )
        }
      }
    })
    // A5: each Used by id breaks only at a hyphen or a space.
    for (const tr of rows) {
      const td = cells(tr)[2]
      if (td === undefined) continue
      for (const a of td.querySelectorAll('a')) {
        const text = a.textContent ?? ''
        const node = [...a.childNodes].find((n) => n.nodeType === 3)
        if (node === undefined || text === '') continue
        const tops = []
        for (let i = 0; i < text.length; i++) {
          const r = a.ownerDocument.createRange()
          r.setStart(node, i)
          r.setEnd(node, i + 1)
          tops.push(r.getBoundingClientRect().top)
        }
        for (let i = 1; i < text.length; i++) {
          if (
            tops[i] !== tops[i - 1] &&
            text[i - 1] !== '-' &&
            text[i - 1] !== ' '
          ) {
            found.push(
              `used by: "${text}" breaks inside a word, "${text.slice(i)}" starts a line`,
            )
            break
          }
        }
      }
    }
    return found
  })
}

/** The nav column at the wide widths (brief 26, the IP6a review R1): the
 *  `.layout` grid gives the nav 248 px, so every entry of the Profiles group
 *  keeps its label beside its icon on one line (at the 224 px the sheet had,
 *  "Instance profiles" moved whole under its icon and the pill grew to 64 px
 *  while its siblings stayed 34). Below 960 px the nav is not a column (index.css
 *  784), so the state says nothing there. */
async function navColumn(page, width) {
  if (width < 960) return []
  return page.evaluate(() => {
    const found = []
    const layout = document.querySelector('.layout')
    if (layout === null) return ['nav column: no .layout on the page']
    const column = getComputedStyle(layout).gridTemplateColumns.split(' ')[0]
    if (column !== '248px') {
      found.push(`nav column: ${column} (expected 248px, R1)`)
    }
    const entries = [
      ...document.querySelectorAll('ul[aria-labelledby="nav-profiles"] a'),
    ]
    if (entries.length === 0) {
      return [...found, 'nav column: the Profiles group holds no entry']
    }
    const heights = []
    for (const a of entries) {
      const label = [...a.childNodes].find(
        (n) => n.nodeType === 3 && (n.textContent ?? '').trim() !== '',
      )
      const icon = a.querySelector('.icon')
      if (label === undefined || icon === null) continue
      const range = a.ownerDocument.createRange()
      range.selectNodeContents(label)
      const text = range.getBoundingClientRect()
      const pill = a.getBoundingClientRect()
      heights.push(Math.round(pill.height))
      const words = (label.textContent ?? '').trim()
      // The label is whole on a line of its own when it has no room beside the
      // icon (index.css 588): its box sits under the icon's.
      if (text.top > icon.getBoundingClientRect().bottom) {
        found.push(
          `nav "${words}": the label is under the icon, the pill is ${Math.round(pill.height)} px (R1: one line beside the icon)`,
        )
      }
    }
    const widest = Math.max(...heights)
    const narrowest = Math.min(...heights)
    if (widest - narrowest > 1) {
      found.push(
        `nav entries: heights ${heights.join(' ')} px, the group is not one line each (R1)`,
      )
    }
    return found
  })
}

/** The Instance profiles list in the narrow layout, read as the page stands
 *  below the 480 px fold (brief 26, the IP6a review R2 and R3):
 *  - R2: the Server profile and the Model profiles columns are folded away, so
 *    each row's muted second line under the name carries both: the server
 *    profile link, ` · `, the count text of 26.2. Its link takes `--muted`, the
 *    line's own colour, so a row shows one purple link and not two.
 *  - R3: no cell of the table breaks inside a word (the rule D4.37 states for
 *    an id: a line may start after a space or a hyphen, nowhere inside a word),
 *    and a head reads its two words whole.
 *  - R5: the table is no wider than the box it scrolls inside, so the box has
 *    nothing to scroll: a box whose child overflows is a Tab stop in Firefox
 *    and takes a scrollbar, which narrows the box further. */
async function instanceListNarrow(page, width) {
  if (width > NARROW_MAX) return []
  return page.evaluate(() => {
    const found = []
    const table = document.querySelector('table.instance-profiles')
    if (table === null) {
      return ['instance profiles: no table.instance-profiles']
    }
    // R5: the width the eye has, and the width the table takes. The box's
    // clientWidth is its content less its border and its scrollbar.
    const box = table.parentElement
    if (box === null) {
      found.push('width: the table has no scroll box (R5)')
    } else if (table.scrollWidth > box.clientWidth) {
      found.push(
        `width: the table is ${table.scrollWidth} px, its scroll box ${box.clientWidth} px (R5)`,
      )
    }
    const taken = (el) => {
      const s = getComputedStyle(el)
      const r = el.getBoundingClientRect()
      return s.display === 'none' || r.width < 2 || r.height < 2
    }
    const word = (el) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
    const rows = [...table.querySelectorAll('tbody tr')].filter(
      (tr) => !tr.classList.contains('confirm-row'),
    )
    const cells = (tr) => [...tr.children].filter((c) => c.tagName === 'TD')
    const hidden = (el) => el?.closest('.visually-hidden') !== null
    // R3: every word of a cell ends on the line it starts on.
    const midWord = (el) => {
      const doc = el.ownerDocument
      const walker = doc.createTreeWalker(el, NodeFilter.SHOW_TEXT)
      const out = []
      for (let n = walker.nextNode(); n !== null; n = walker.nextNode()) {
        const text = n.textContent ?? ''
        if (text.trim() === '' || hidden(n.parentElement)) continue
        const tops = []
        for (let i = 0; i < text.length; i++) {
          const r = doc.createRange()
          r.setStart(n, i)
          r.setEnd(n, i + 1)
          tops.push(r.getBoundingClientRect().top)
        }
        for (let i = 1; i < text.length; i++) {
          if (
            tops[i] !== tops[i - 1] &&
            text[i - 1] !== '-' &&
            text[i - 1] !== ' '
          ) {
            out.push(`"${text}" breaks inside a word, "${text.slice(i)}" starts a line`)
            break
          }
        }
      }
      return out
    }
    for (const el of table.querySelectorAll('th, td')) {
      if (taken(el)) continue
      for (const bad of midWord(el)) {
        found.push(`${word(el).slice(0, 24) || 'cell'}: ${bad} (R3)`)
      }
    }
    // R2: the two folded columns speak in the name cell.
    for (const tr of rows) {
      const td = cells(tr)
      const id = word(td[0]?.querySelector('a'))
      const line = td[0]?.querySelector('.muted')
      if (line === null || line === undefined) {
        found.push(`fold: the name cell of "${id}" holds no second line`)
        continue
      }
      if (taken(line)) {
        found.push(`fold: the second line of "${id}" is not visible`)
        continue
      }
      for (const n of [1, 2]) {
        if (td[n] !== undefined && !taken(td[n])) {
          found.push(
            `fold: column ${n + 1} of "${id}" is still laid out (R2 folds it away)`,
          )
        }
      }
      const link = line.querySelector('a')
      if (link === null) {
        found.push(`fold: the second line of "${id}" holds no server profile link`)
        continue
      }
      const wanted = `${word(link)} · ${word(td[2])}`
      if (word(line) !== wanted) {
        found.push(
          `fold: the second line of "${id}" is "${word(line)}", expected "${wanted}" (R2)`,
        )
      }
      const muted = getComputedStyle(line).color
      const own = getComputedStyle(link).color
      if (own !== muted) {
        found.push(
          `fold: the second line's link of "${id}" is ${own} and the line's words are ${muted} (R2: the link takes --muted)`,
        )
      }
    }
    return found
  })
}

/** The Model profiles section of one instance profile, in the narrow layout
 *  (brief 26, the IP6b review R7, 2026-11-28): the table of 26.7 is the widest
 *  thing on that page, and at 320 px it broke the head "Placement" into
 *  "Placemen t" and the head "Model profile" into "pro file". R7 asks for R3 and
 *  R5's rules on this section:
 *  - nothing in the section breaks inside a word. A line may start after a
 *    space, a hyphen, a dot or an underscore: an id breaks at its own
 *    punctuation (D4.37), nowhere inside a word;
 *  - the table is no wider than the box it scrolls inside, and nothing is wider
 *    than the section, so the box has nothing to scroll and the heads have room
 *    to wrap between words.
 *  The section is read as it stands, so a layout that folds the columns into
 *  stacked rows (what R3 did to the list) passes on the same words. */
async function instanceProfileModelsNarrow(page, width) {
  if (width > NARROW_MAX) return []
  return page.evaluate((title) => {
    const found = []
    const head = [...document.querySelectorAll('main h2')].find(
      (h) => (h.textContent ?? '').trim() === title,
    )
    if (head === undefined) {
      return [`models of a profile: no "${title}" heading on the page`]
    }
    const section = head.closest('section')
    if (section === null || section === undefined) {
      return [`models of a profile: "${title}" is not in a section`]
    }
    const taken = (el) => {
      const s = getComputedStyle(el)
      const r = el.getBoundingClientRect()
      return s.display === 'none' || r.width < 2 || r.height < 2
    }
    const word = (el) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim()
    // R5: the width the eye has, and the width the table takes. The box's
    // clientWidth is its content less its border and its scrollbar.
    const table = section.querySelector('table')
    if (table !== null) {
      const box = table.parentElement
      if (box === null) {
        found.push('models of a profile: the table has no scroll box (R5)')
      } else if (table.scrollWidth > box.clientWidth) {
        found.push(
          `models of a profile: the table is ${table.scrollWidth} px, its scroll box ${box.clientWidth} px (R5)`,
        )
      }
    }
    // R5: nothing of the section reaches past the section's own box.
    const room = section.clientWidth
    for (const el of section.querySelectorAll('*')) {
      if (taken(el)) continue
      const w = Math.round(el.getBoundingClientRect().width)
      if (w > room + 1) {
        found.push(
          `models of a profile: a ${el.tagName.toLowerCase()} is ${w} px, the section ${room} px (R5)`,
        )
        break
      }
    }
    const hidden = (el) => el?.closest('.visually-hidden') !== null
    // R7: every word of a visible piece of text ends on the line it starts on.
    // A break after one of these is the id's own (D4.37), so it is allowed: an
    // id keeps breaking at its hyphen, its dot and its underscore.
    const BREAKS = [' ', '-', '.', '_']
    const doc = section.ownerDocument
    const walker = doc.createTreeWalker(section, NodeFilter.SHOW_TEXT)
    for (let n = walker.nextNode(); n !== null; n = walker.nextNode()) {
      const text = n.textContent ?? ''
      const host = n.parentElement
      if (text.trim() === '' || host === null || hidden(host) || taken(host)) {
        continue
      }
      const tops = []
      for (let i = 0; i < text.length; i++) {
        const r = doc.createRange()
        r.setStart(n, i)
        r.setEnd(n, i + 1)
        tops.push(r.getBoundingClientRect().top)
      }
      for (let i = 1; i < text.length; i++) {
        if (tops[i] !== tops[i - 1] && !BREAKS.includes(text[i - 1])) {
          const cell = host.closest('th, td, li, p')
          const rest = text.slice(i)
          found.push(
            `models of a profile: ${word(cell).slice(0, 24) || 'text'}: breaks inside a word, "${rest}" starts a line (R7)`,
          )
          break
        }
      }
    }
    return found
  }, S['instance-profile-models-title'])
}

/** The probes, by the name a state's `probe` gives. */
const PROBES = {
  'model-profiles-narrow': modelsListNarrow,
  'nav-column': navColumn,
  'instance-profiles-narrow': instanceListNarrow,
  'instance-profile-models-narrow': instanceProfileModelsNarrow,
}

/** What the state's `probe` measures, on the page at rest, at this width. Each
 *  probe gates itself: the narrow-layout ones say nothing at or above the
 *  480 px fold, where the layout does not fold, and `nav-column` says nothing
 *  below 960 px, where the nav is not a column. */
async function probed(page, state, width) {
  if (state.probe === undefined) return []
  return PROBES[state.probe](page, width)
}

/** The state on the page: the stub's set, then the route. The stub keeps every
 *  key a /__set did not carry, so the state's own set goes after a reset to
 *  the default list of instances, the default tags, the fast reads and the
 *  answering node: the two tile states ask for list=many themselves, the tags
 *  state for tags=long, the loading ones for load=session or load=slow and the
 *  node states for node=down or node=long, the blocks state for split=layer,
 *  the placement state for ot=1, the delete state for used=1, the in-use state
 *  for instuse=race, and a state that
 *  ran before them leaves none of them behind for the pages after it. A hash
 *  change does not reload the app, so every state starts from a blank page. */
async function load(page, state, base) {
  const found = []
  for (const q of [
    'list=one&tags=few&load=fast&node=ok&split=tensor&ot=none&used=0&usedby=short&instuse=free',
    state.set,
  ]) {
    const res = await fetch(`${base}/__set?${q}`)
    if (!res.ok) {
      found.push(`set request: ${res.status} ${res.statusText}`)
    }
  }
  await page.goto('about:blank')
  // A `hold` state: the stub keeps its reads open for seconds, so the network
  // never goes idle and 'networkidle' would only end at the navigation timeout.
  // The page is at rest once it is parsed: its skeleton is the state to read.
  await page.goto(`${base}${state.hash}`, {
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
async function bodyBackground(browser, theme, base) {
  const ctx = await browser.newContext({
    viewport: { width: WIDTHS[0], height: HEIGHT },
    colorScheme: theme,
  })
  try {
    const page = await ctx.newPage()
    await page.goto(`${base}/#/`, { waitUntil: 'networkidle' })
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
async function textPass(browser, name, base, lines) {
  const ctx = await browser.newContext({
    viewport: { width: TEXT_WIDTH, height: HEIGHT },
    colorScheme: TEXT_THEME,
  })
  try {
    const page = await ctx.newPage()
    for (const [mode, css] of TEXT_MODES) {
      for (const state of STATES) {
        const found = await load(page, state, base)
        found.push(...(await menuOpened(page, state)))
        const missing = await rendered(page, state)
        if (missing !== null) {
          found.push(missing)
        } else {
          found.push(...(await clicked(page, state)))
          found.push(...(await opened(page, state)))
          found.push(...(await openedChain(page, state)))
          found.push(...(await hovered(page, state)))
          found.push(...(await focused(page, state)))
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
async function check(browser, name, base, lines) {
  // The dark scheme has to reach the page: index.css sets --bg under
  // prefers-color-scheme, so body's background differs between the themes.
  const light = await bodyBackground(browser, 'light', base)
  const dark = await bodyBackground(browser, 'dark', base)
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
            const found = await load(page, state, base)
            found.push(...(await menuOpened(page, state)))
            const missing = await rendered(page, state)
            if (missing !== null) {
              found.push(missing)
            } else {
              // The clicks run on the rendered page, before the settle: the
              // page at rest is read with them done.
              found.push(...(await clicked(page, state)))
              found.push(...(await opened(page, state)))
              found.push(...(await openedChain(page, state)))
              found.push(...(await hovered(page, state)))
              found.push(...(await focused(page, state)))
              await sleep(SETTLE_MS)
              found.push(...(await probed(page, state, width)))
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
  await textPass(browser, name, base, lines)
}

async function main() {
  if (!existsSync(path.join(DIST, 'index.html'))) {
    console.log(`no web UI in ${DIST}: run npm --prefix fe run build first`)
    return 1
  }
  // One stub per browser: the stub's /__set state is shared, so two pages on
  // one stub would race. Each stub binds its own free port, and the three
  // browsers run at the same time, each against its own.
  const stubs = []
  const killStubs = () => {
    for (const stub of stubs) stub.kill()
  }
  // Ctrl+C and a kill must not leave a stub behind.
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => {
      killStubs()
      process.exit(1)
    })
  }
  let bases
  try {
    const ports = await Promise.all(
      BROWSERS.map(async () => {
        const stub = spawn(
          'python3',
          ['-I', path.join(__dirname, 'stub.py'), DIST, '0'],
          { stdio: ['ignore', 'pipe', 'inherit'] },
        )
        stubs.push(stub)
        return stubPort(stub)
      }),
    )
    bases = ports.map((port) => `http://127.0.0.1:${port}`)
    BROWSERS.forEach((name, i) => console.log(`stub bff ${name}: ${bases[i]}`))
  } catch (e) {
    killStubs()
    console.log(`stub bff ${e.message}: nothing to check`)
    return 1
  }
  const linesByBrowser = BROWSERS.map(() => [])
  const started = BROWSERS.map(() => false)
  try {
    await Promise.all(
      BROWSERS.map(async (name, i) => {
        let browser
        try {
          browser = await pw[name].launch()
        } catch (e) {
          console.log(`${name}: did not start (${e.message.split('\n')[0]})`)
          return
        }
        started[i] = true
        try {
          await check(browser, name, bases[i], linesByBrowser[i])
        } catch (e) {
          // One throw in one browser must not lose what every state of every
          // browser produced: the failing line is recorded like a finding,
          // the other browsers run, and the summary prints.
          linesByBrowser[i].push(
            `${name}: check threw: ${e.message.split('\n')[0]}`,
          )
        } finally {
          await browser.close()
        }
      }),
    )
  } finally {
    killStubs()
  }
  const ran = BROWSERS.filter((name, i) => started[i])
  const skipped = BROWSERS.filter((name, i) => !started[i])
  // The lines print in the fixed browser order (chromium, firefox, webkit),
  // whatever order the concurrent browsers finished in.
  const lines = linesByBrowser.flat()
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
