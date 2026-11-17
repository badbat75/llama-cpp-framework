# Brief 03: the sign-in says when the password crosses the network in clear

Written by the designer on 2026-11-17 for `PLAN.md` step 10, item 12 (LAN access). Applies
`fe/DESIGN.md` ("D5.2" points to its sections) and changes the sign-in of
[01-server.md](01-server.md) section 2.2 (criteria 9 to 14b) and its dialog (01's 36, 36a; 2h's
151). Criteria are numbered I1 to I9 so they collide with no other brief. One sub-slice: one tester
run, one coder run.

**Why (user, 2026-11-17).** The bff will listen on the LAN over plain HTTP, the user's choice, made
knowing the sign-in password then crosses the network in clear (`PLAN.md` step 10, item 12). The
sign-in must say so when it is the case, and must not block signing in.

**Grounding.** The bff's `Host` guard accepts any IP literal, the machine's own names (its host
name, `<name>.local`, `<name>.<domain>` for each search domain; or, when set, the names of `[Bff]
Hosts` in `bff.ini` instead) and, when it listens on loopback or all interfaces, `localhost`; it
refuses every other name (403 `request-bad-host`, `PLAN.md` step 10 items 12 and 12b). So the page is
reached as `http://localhost:<port>`, `http://127.0.0.1:<port>`, `http://[::1]:<port>`, `http://<an
IP of the machine>:<port>` or `http://<a name of the machine>:<port>` (for example
`http://myhost.local:8590`). The bff serves no HTTPS today. `pai-config` has no sign-in (it talks to the
manager on the same machine), so there is nothing to agree with on the GUI side.

## 0. The decision: one rule, `window.isSecureContext === false`

`window.isSecureContext` "returns a boolean indicating whether the current context is secure"
([MDN, `Window.isSecureContext`](https://developer.mozilla.org/en-US/docs/Web/API/Window/isSecureContext)).
A document is secure when it is "delivered over an HTTPS connection, or ... from a loopback (local)
address" ([MDN, Secure contexts](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts)).
The spec's "Is origin potentially trustworthy?" returns trustworthy for the schemes `https` and
`wss`, for a host in `127.0.0.0/8` or `::1/128`, for `localhost` and names ending in `.localhost`
when the browser resolves them to loopback ("let-localhost-be-localhost"), and for an origin the
user configured as trustworthy; anything else is not trustworthy
([Secure Contexts, 3.1](https://w3c.github.io/webappsec-secure-contexts/#is-origin-trustworthy)).

So **the notice shows exactly when `window.isSecureContext === false`**, and that is the whole rule.
It is what the page can know: whether the connection is encrypted or stays on the machine. Read
once when the sign-in renders (it cannot change without a new page load).

What it gets wrong, accepted:

- **False alarm, same machine by its LAN address or name**: `http://192.168.1.10:8590` or
  `http://myhost.local:8590` opened on the server machine itself shows the notice, though the
  password never leaves the machine. Harmless: the text tells the user to sign in on that machine,
  which they can do through `localhost` (when the bff listens on loopback or all interfaces; bound to
  one LAN address only, `localhost` is refused, and the false alarm stays: accepted). From another
  device, a machine name over `http` is not trustworthy either (the spec's steps 3 to 5 cover only
  loopback hosts and `localhost`), so the notice shows, which is right; measured in the three engines
  for `http://aorus.bombicci.local:8590` (section 3).
- **False alarm, `localhost` in a browser that does not resolve it to loopback by rule**: the spec
  makes `localhost` trustworthy only then (step 5); `127.0.0.1` is trustworthy always. Harmless.
- **Missed: an HTTPS proxy in front of the bff on another machine**: the page sees `https`, the hop
  from the proxy to the bff is in clear. The page cannot see that hop; it is the deployer's choice.
- **Missed: an origin the user marked trustworthy** (the spec's step 8, a browser setting or flag):
  the user's own choice.
- **A browser without the property** (`undefined`): no notice. The UX check confirms the three engines
  define it (section 3); a browser that does not is not worth a false alarm. Comparing with `false`, not "not `true`", also keeps jsdom, which
  does not define the property, free of the notice in every existing test.

## 1. Criteria

I1. **Where, on the sign-in page.** At `#/sign-in` and in place of a protected page (01's 9, 10):
    after the intro `signin-intro` and after the `signin-required` Notice or the `session-revoked`
    alert when one shows, before the `<form>`, a paragraph with the text `signin-insecure`. Before
    the fields, so it is read before the password is typed, by sight and in reading order.
    ([1.3.2](https://www.w3.org/TR/WCAG22/#meaningful-sequence))
I2. **Where, in the sign-in dialog** (the session-ended flow: the Server page's bar, 36; the run
    controls, 43a; Start at boot, 45e; the preset page, 2h's 151; the Models switch): the same
    paragraph at the same place inside the dialog (after the intro, before the form). The dialog
    shows the same sign-in, so the rule is one rule.
I3. **Never blocks.** The fields, Sign in and the dialog's Cancel stay as 01's 9 to 14b have them:
    enabled, same order, same answers. No checkbox to acknowledge, no confirm step.
I4. **Role and announcement.** A plain `<p>`: no `role="alert"`, no `role="status"`, no
    `aria-live`. It is static content present when the form renders, not a status message
    ([4.1.3](https://www.w3.org/TR/WCAG22/#status-messages) covers content that changes; this does
    not). No icon: the text says it all, and an icon would need a text alternative that repeats it
    **(preference)**.
I5. **Read with the password field.** The paragraph has an id, and the password input's
    `aria-describedby` lists it: after the empty-password error's id when that error shows (01's 11),
    else alone. So a screen-reader user who tabs straight into the password field hears the notice at
    the moment it matters. The user name field is not described by it.
    ([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
    [3.3.2](https://www.w3.org/TR/WCAG22/#labels-or-instructions))
I6. **Look.** The existing Notice of D5.2 (`.notice`): `--surface` background, 1px `--control`
    border, radius 6px, padding `--space-2` `--space-3`, text `--text`. No new token, no warning
    colour: `--warning` means a server state in progress (D5.1), and the notice is advice, not an
    error **(preference)**. Contrast: `--text` on `--surface` 15.5:1 light, 14.0:1 dark (D5.1 table;
    floor 4.5:1, [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)). At 320 px it wraps inside
    the main column, no fixed width, no sideways scroll
    ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)); text resizes to 200%
    ([1.4.4](https://www.w3.org/TR/WCAG22/#resize-text)).
I7. **Nowhere else.** The Overview (visible without sign-in), the banner's Sign in link and every
    signed-in page show nothing: no secret is typed there. The page after sign-in carries a session
    cookie in clear too, but the user chose plain HTTP knowing it; one notice at the moment of the
    password is enough **(preference)**.
I8. **Secure: nothing.** `isSecureContext` `true` (https, loopback) or `undefined`: no paragraph, no
    id in the password's `aria-describedby`; the sign-in is exactly as today.
I9. **Tests** (Vitest, jsdom; jsdom does not define `isSecureContext`, so each test sets it with
    `Object.defineProperty(window, 'isSecureContext', { value: false, configurable: true })` and an
    `afterEach` removes it with `delete (window as { isSecureContext?: boolean }).isSecureContext`):
    - `false`, at `#/sign-in`: the text `signin-insecure` shows; it follows the intro and precedes
      the form (`compareDocumentPosition`); no `role="alert"` or `role="status"` element holds it;
    - `false`, a protected route signed out: it follows the `signin-required` Notice;
    - `false`: the password field's accessible description is the notice's text; after an empty
      submit it holds the error's text then the notice's;
    - `false`: filling both fields and submitting sends `POST /api/session` and a 200 signs in, as
      with no notice (I3);
    - `false`, a Server save answered 401, then the bar's Sign in: the dialog holds the notice;
    - `false`, signed out on the Overview: no `signin-insecure` text;
    - `true`, and the property absent: no notice, and the password field has no description.

**String (1 new)**: `signin-insecure` | This connection is not encrypted, so your password is sent
over the network as plain text. To keep it private, sign in on the computer that runs Private AI
Framework, or only over a network you trust.

## 2. Out of scope

HTTPS in the bff (none today); a link to setup help (none exists); blocking or delaying sign-in on
an insecure connection; a notice on signed-in pages or the Overview; telling apart "same machine by
its LAN address" from another machine (the page cannot know it); changing `signin-intro`.

## 3. What the designer checks afterwards (Playwright, `fe/ux`)

The UX check's stub listens on `127.0.0.1`, a secure context, so the notice is absent there: the
existing states must still show none (I8). To see the notice without a LAN bff and without changing
the stub: in each engine, open `http://192.0.2.10:8590/#/sign-in` (an address of the documentation
range TEST-NET-1, [RFC 5737](https://www.rfc-editor.org/rfc/rfc5737), never routed) and answer
every request with `page.route('http://192.0.2.10:8590/**', ...)`, fetching the same path from the
stub with `route.fetch({ url })` and `route.fulfill({ response })`
([Playwright, `Route`](https://playwright.dev/docs/api/class-route): `fetch`'s `url` "changes the
request URL. New URL must have same protocol as original one"). The origin is then an IP outside
loopback over `http`, so the page is not a secure context (a machine name over `http`, routed the
same way, is the same case); the check first asserts
`window.isSecureContext === false` in the page, so a browser that thinks otherwise fails loudly
rather than passing on an absent notice. Then: the notice present, before the form; its computed
background `--surface` and colour `--text`, light and dark; axe-core clean; no sideways scroll at
320 px; Chromium, Firefox, WebKit. The dialog case is driven the same way (a Server save answered
401 by the stub, then the bar's Sign in).

Measured 2026-11-17 (Playwright's Chromium, Firefox, WebKit of `fe/ux`): `isSecureContext` is `false`
at `http://192.0.2.10:8590` and `http://aorus.bombicci.local:8590`, `true` at `http://127.0.0.1:<port>`
and `http://localhost:<port>`, in all three.

The user checks once on the real LAN bff after the install of item 12: from another device,
`http://<server IP>:8590/#/sign-in` shows the notice; on the server, `http://localhost:8590` does not.
