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
const LONG_LABEL = 'QWEN327BTHIRTYCHARACTERSTILELABEL0123456789012345678901234567890'
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
// The host's list, where the nav's instance links live (20 N.3).
const NAV_HOST = '#nav ul[aria-labelledby="nav-host"]'
// The Rename of 16 17f.5, named by the key 17f.11 adds: absent from the strings
// file it names a button of the text "undefined", so the state fails until the
// slice lands, as it asks.
const RENAME = `main button:has-text("${S['instance-rename']}")`
// The start of the GPU card's as-of line (brief 14 GD1.7): the sentence of
// `model-profile-gpu-probed-at` up to its `{0}`, which the page fills with the
// probe's moment in the browser's own locale, so no state can name the whole
// sentence. The stub's probe answers, so the line lands.
const AS_OF = S['model-profile-gpu-probed-at'].split('{0}')[0]
// The start of the GPU card's budget line (brief 19, 19 G.5): the sentence of
// `model-profile-gpu-blocks-budget` up to its `{0}`, which the page fills with
// the places the facts read gives. Absent from the strings file until LS1
// lands, where the state's marker is the slider alone.
const BUDGET = S['model-profile-gpu-blocks-budget']?.split('{0}')[0]
// The legend of the third rule of brief 21's placement state (section 7): the
// string `model-profile-ot-rule` with the rule's position. Absent from the
// strings file until TP1 lands, where the state's marker is a legend alone.
const RULE3 = S['model-profile-ot-rule']?.replace('{0}', '3')
// The start of the delete's question (brief 22, 22 D.2): the string
// `model-profile-delete-question` up to its `{0}`, which the page fills with the
// profile's name. It names the Confirm group, whose selector alone is not the
// page's only group (every Field is a `fieldset`). Absent from the strings file
// until 22D lands, where the marker names a text of "undefined" and the state
// fails, as it asks.
const DELETE_QUESTION = S['model-profile-delete-question']?.split('{0}')[0]

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
      // 16 17f.4: the id rides right after the name, so the page says which
      // instance it is by the word the menu uses.
      { selector: 'p.instance-id', text: ID_LINE(DEFAULT_ID) },
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
    // Brief 16, slice 17f (17f.12): the rename form opened on the instance page,
    // on the name the stub's port spells. The marker names the key 17f.11 adds:
    // absent from the strings file it names a button of the text "undefined", so
    // the state fails until the slice lands, as it asks. The box the click opens
    // is the page's only form input: the tags field is chips, its pill closed.
    name: 'instance-rename',
    set: 'session=admin',
    hash: '#/instances/default',
    expect: [
      { selector: '#page-heading', text: DEFAULT_NAME },
      { selector: RENAME, message: 'the Rename, beside the name' },
    ],
    open: { click: RENAME, then: 'main form input' },
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
        selector: 'main button:has(svg path[d="M5 12h14"])',
        message: 'the opener of the create form: the button with the plus',
      },
    ],
    open: {
      click: 'main button:has(svg path[d="M5 12h14"])',
      then: 'main form',
    },
  },
  {
    // Brief 16, slice 17b (17b.7): the Confirm in place of the delete, on the
    // page of the widest profile of `pool=long`, which runs nowhere
    // (stub.py answers that id with `instance` null). 22 D2.8 makes the opener
    // a − icon button with no visible text, so the state finds it by the id the
    // brief gives it: absent from the page until the slice lands, the state
    // fails until then, as it asks.
    name: 'server-profile-delete',
    set: 'session=admin&pool=long',
    hash: `#/profiles/server/${LONG_POOL_ID}`,
    expect: [
      { selector: '#page-heading', text: LONG_POOL_ID },
      {
        selector: '#server-profile-delete',
        message: 'the − of 22 D2.8, offered while the profile runs nowhere',
      },
    ],
    open: {
      click: '#server-profile-delete',
      then: 'main [role="group"]',
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
    name: 'model-profile-blocks',
    // Brief 19, 19 G.4 and 19 G.5: the same page with the split in the file's
    // mode and a ratio the facts read projects to block counts (stub.py
    // `split=layer`, and the model-info read with 64 blocks), so the GPU rows
    // carry a block number field, a range slider, and the range and share text
    // beside them, under the budget line. The text is the budget's sentence up
    // to its `{0}`, whose places come from the facts read.
    set: 'session=admin&split=layer',
    hash: `#/profiles/model/${MODEL_PROFILE_ID}`,
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      { selector: 'p', text: BUDGET },
      { selector: 'input[type="range"]' },
    ],
  },
  {
    name: 'model-profile-placement',
    // Brief 21, section 7: the Tensor placement card with three rules (the stub
    // answers `ot=1`): the experts of a run of layers on the CPU, the embedding
    // table on a GPU, and a hand-written pattern no kind owns, so a Custom row
    // with its Pattern input shows. The marker is the legend of the last rule;
    // the device read's as-of line is waited for, the rule selects list the
    // probe's GPUs.
    set: 'session=admin&ot=1',
    hash: `#/profiles/model/${MODEL_PROFILE_ID}`,
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      { selector: 'p', text: AS_OF },
      { selector: 'legend', text: RULE3 },
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
    // Brief 22, slice 22D (22 D.1 and 22 D.2, 22 C.10's state): the Confirm in
    // place of the delete, on the page of a switched-off model profile (stub.py
    // `off=1` reads `enabled` false, and answers the DELETE with 204). 22 D2.1
    // and D2.2 move the opener into the status row under the summary line, as a
    // − icon button with no visible text, so the state finds it by the id the
    // brief gives it: absent from the page until the slice lands, the state
    // fails until then, as it asks.
    name: 'model-profile-delete',
    set: 'session=admin&off=1',
    hash: `#/profiles/model/${MODEL_PROFILE_ID}`,
    expect: [
      { selector: '#page-heading', text: MODEL_PROFILE_ID },
      {
        selector: '#model-profile-delete',
        message: 'the − of 22 D2.2, offered while the profile is switched off',
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
 *  the default list of instances, the default tags, the fast reads and the
 *  answering node: the two tile states ask for list=many themselves, the tags
 *  state for tags=long, the loading ones for load=session or load=slow and the
 *  node states for node=down or node=long, the blocks state for split=layer,
 *  the placement state for ot=1, the delete state for off=1, and a state that
 *  ran before them leaves none of them behind for the pages after it. A hash
 *  change does not reload the app, so every state starts from a blank page. */
async function load(page, state, base) {
  const found = []
  for (const q of [
    'list=one&tags=few&load=fast&node=ok&split=tensor&ot=none&off=0',
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
