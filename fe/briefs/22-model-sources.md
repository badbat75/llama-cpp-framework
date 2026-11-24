# Brief 22: create and delete a model profile, and pick its model (slices MP3 and M4)

`PLAN.md` step 10: "Create and delete model profiles from the web" (slice MP3 of `ARCHITECTURE.md`
"Create, clone and delete model profiles from the web", accepted 2026-11-22) and "Choosing the model
file on the web model profile page" (slice M4 of `ARCHITECTURE.md` "Choosing the model file on the
web", accepted 2026-11-21, its M3 amended by "One models folder for every engine"). One brief for both
because the create form's model field and the profile page's model field are **one component**, the
model source picker (coordinator, 2026-11-23). Modelled on brief 16's 17b (create and delete a server
profile) so the two read the same; where they differ, section 1 says so. Criteria are numbered
**22 X.n**; test names cite them so (`22 D.3`).

**Four sub-slices, one tester and one coder run each** (designer playbook: more than about 12
criteria per run fails):

| Sub-slice | Criteria | Holds | Needs |
|---|---|---|---|
| **22D** | 22 D.1 to 22 D.8 | Delete a model profile, with a Confirm in place, on its page | Nothing: MP2 is served (76163d1). Can run now |
| **22P** | 22 P.1 to 22 P.12 | The model source picker (`ModelSource`): a model file from the models folder, or a Hugging Face repo and its quant | M2 `GET /api/hf/files` and M3 `GET /api/node/model-files` served (the tester stubs them from section 0 meanwhile, so it can run beside them) |
| **22C** | 22 C.1 to 22 C.10 | The New model profile form on the list: a name, then a model file, a Hugging Face repo, or a copy of another profile | 22P |
| **22E** | 22 E.1 to 22 E.7 | The picker on the profile page's Assets card, saved with the page | 22P, and the bff accepting `model`, `hf-repo` and `hf-file` in the save (section 0: a bff change first) |

Order: 22D now; 22P and 22C once M2 and M3 are served (22C may share 22P's run if the coordinator
prefers, 22 criteria then is too many: keep two runs); 22E last. A tester on 22C or 22E reads sections
0, 1, 2 and 22 P.1 to 22 P.3 (the picker's shape) and its own criteria.

## 0. What exists (read on main at 9675d0d)

- **Create and delete** (`fe/briefs/api-contract.md` "`POST /api/profiles/model` and `DELETE
  /api/profiles/model/{id}`", `crates/bff/README.md` "The create and the delete"):
  - `POST /api/profiles/model` takes `{"id", "from"}` (a copy of the pool profile `from`) or `{"id",
    "values"}`, `values` being `{"model"}`, or `{"hf-repo"}` with an optional `"hf-file"`. 201 `{"id"}`.
    The new profile lands switched off (`enabled: false`) with no assignments. Texts are trimmed of
    spaces and tabs.
  - A copy takes every line of the source section (comments, every key) and never its assignments
    or its on/off state.
  - Create refusals: 400 `model-profiles-malformed` (a body it cannot read, an empty text, a control
    character); 413 `payload-too-large`; 422 `invalid-model-profile-id` (`field` `id`),
    `no-model-profile-named` (`field` `from`), `comment-markers` (`field` the key), and
    `model-profile-no-model` or `model-profile-bad-source` (`field` `values`; the page's checks keep
    both from reaching the manager); 409 `model-profile-name-taken` `[id]` with no `field` (taken in
    any letter case, in either file).
  - `DELETE /api/profiles/model/{id}`: 204. Refusals: 404 `no-model-profile-named` `[id]`; 409
    `model-profile-enabled` `[id]` (switched on); 409 `model-profile-assigned` `[id, instances]` (an
    instance's section of `assignments.ini` names it with `inherit`, `on` or `off`, or a `* = on`
    serves it; `instances` the ids joined by `, `).
  - Both: 401 `session-required` (nothing asked); 403 `session-not-administrator` (the session ended);
    503 `client-unavailable` (nothing done); 504 `client-timeout` (the call was handed over, the outcome
    unknown: a retried create is then 409, a retried delete 404); 502 `client-other`.
- **The lists the picker reads** (`ARCHITECTURE.md` "Choosing the model file on the web" (5), and "One
  models folder for every engine" (3)). Neither is served yet (M2, M3); the shapes are the design's:
  - `GET /api/hf/files?repo=<owner>/<name>`:
    `{"source":"hf","repo","revision","files":[{"tag","file","shards","size","sha256","sidecars":{"mmproj"},"value":{"hf-repo":"o/r:TAG"[,"hf-file":"<path>"]},"default":true|false}]}`.
    `value` is exactly what a pick writes. Refusals: 400 `hf-repo-invalid`, 404 `hf-repo-unavailable`
    (missing, gated or private: no token yet), 502 `hf-repo-moved`, 504 `hf-unreachable`, 502
    `hf-bad-answer`, 503 `hf-tool-missing`, 429 `hf-busy`, each with args `[repo]` where the design
    gives one; and the session's 401 and 403. The bff caches a repo's list for 10 minutes.
  - `GET /api/node/model-files`:
    `{"source":"local","dir":"/abs","files":[{"path":"/abs/...gguf","size","shards","category":"models"|"mmprojs"|"mtps"|"dflashs"|null}],"truncated":false,"error":null|{"key":"models-dir-unreadable","args":["/abs"]}}`.
    `path` is absolute; a split model is listed once, as its first shard; at most 5000 files
    (`truncated`). The session's 401 and 403, and the manager's `client-*` (503, 502), as every `As`
    route. No cache: each read walks the folder.
- **The save** (`fe/briefs/api-contract.md` "Accepted keys"): `model` is one of the five keys not on
  the web (`MODEL_PROFILE_KEYS_NOT_ON_WEB`), and `hf-repo` and `hf-file` are not on the list either. A
  save naming them is 400 today. **22E needs a bff change first**: the three join
  `WEB_MODEL_PROFILE_KEYS` (the coordinator plans it; the save's checks already judge `model`, and the
  loosened section rule accepts `hf-repo` without `model`). `model` refusals carry `field` `model`
  (`model-profile-no-model`, `model-profile-model-missing`, `comment-markers`).
- **The profile document** (`GET /api/profiles/model/{id}`) carries `enabled` and `values`, and **no
  assignments**; the list rows (`GET /api/profiles/model`) carry `assignments`.
- **The page today**: the model file is a read-only row of the collapsed Assets card
  (`ModelProfilePage.tsx` `assetRows` 211), and the summary line under the `h1` says "Engine x, model
  y" or `model-profile-summary-no-model` (H11a.1). `hf-repo` shows nowhere.
- **The list page** (`ModelProfiles.tsx` 261): `h1`, intro, the filter, the off-note, the table. No
  create.
- **17b, built** (brief 16): the Create form in place and the Confirm in place of `fe/DESIGN.md` 5.2,
  `writeFailure`, the keys `create-*`, `confirm-*`, `field-error-required`. Its review settled two
  rules this brief keeps: the focus moves to a field only after its error renders (`flushSync`, D1),
  and a refusal whose read-again unmounts the control keeps its text in a focused
  `p[role="alert"][tabindex="-1"]` where the control was (D2).
- **17f, not built**: its `delete-unknown` text and the 504 rule of 17f.10 (a 504 is "not confirmed",
  never `request-failed`). MP3 is built first, so 22D brings `delete-unknown` and 22C and 22D apply
  17f.10's rule to their own writes; 17f then reuses both.
- **The GUI** (`fe/briefs/configurator-inventory.md` 1.2, 1.7, G14 to G16): the Models tab has New…,
  Clone… and Delete. New picks a model file or runs the Hugging Face dialog (search, a repo's
  candidates, download) and derives the id itself (silently made unique); Clone copies the selected
  profile; Delete removes the profile whatever its state and offers to delete its files.
- **Existing patterns reused**: the APG combobox of the tag suggestions (`fe/DESIGN.md` 5.2 Chips:
  `ul.tag-options`, no automatic selection, the list in the flow), Field (label, control, About hint,
  error), the Save bar, the collapsed card's "a jump opens it" rule.

## 1. Decisions (each a preference unless it cites a source)

1. **Copy is a third "Start from" choice of New**, not a Clone button on a row. The design has one
   create call with an optional source; the form follows it. Differs on purpose from the GUI, whose
   Clone acts on a selected row: the web list has no selection.
2. **The administrator types the name.** The GUI derives it from the file; the web asks, as 17b asks
   for a server profile's name, because the name is what clients (OpenCode) ask for and cannot be
   changed on the web. Differs on purpose from the GUI.
3. **Only listed items, no free text** (user, 2026-11-23, against the recommendation to keep free
   text). A new value is a file picked from the models folder's list, or a quant picked from a repo's
   list; typing only filters the files or names the repo to list. What follows from it:
   - **A saved value the lists cannot show is kept**: a `model` outside the folder (or any `model`
     while the folder cannot be read), an `hf-repo` the list cannot read (private, gated, moved,
     network down). The field shows it with a line saying it is kept; the picker's value stays the
     saved one, so the page saves its other edits unchanged and sends no picker key. Editing the field
     drops the kept value (Discard brings it back); it cannot be typed back by hand.
   - **New needs a list that answers**: with the folder empty or unreadable and the repo list failing,
     "A model file" and "A Hugging Face repo" cannot be completed and Create says so on the field
     (`field-error-pick-file`, `field-error-pick-quant`); "A copy of another model profile" still works
     when the pool has a profile. The texts say what to do (put a file in the folder, read it again,
     try another repo). A private repo cannot be added from the web until the token (M5).
4. **The model file field is the existing APG combobox** (list in the flow, no automatic selection),
   filtering the folder's files by what is typed (substring, letter case aside), at most **50**
   options shown, a status line counting the rest. Not a `select` (5000 files) and not a `datalist`
   (screen reader support uneven: [MDN datalist accessibility](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/datalist#accessibility)).
5. **The Hugging Face repo is a text field and a "Show quants" button**, never a fetch per keystroke
   (each fetch is a call from the node to huggingface.co, at most two at once). The quants are a
   radio group (a repo has tens, not thousands), the one already in the value, else the one
   `default: true` marks, checked when the list arrives. A pasted `https://huggingface.co/<o>/<r>`
   (or `hf.co`) is reduced to `<o>/<r>` in the field on Show quants, as the GUI's `parse_spec` accepts
   a URL.
6. **The repo field holds exactly what `hf-repo` will hold** (`owner/name`, or `owner/name:TAG` after
   a pick), so there is no hidden state beside it; only `hf-file` is kept apart, from a pick, and any
   edit of the field drops it with the quant list.
7. **Delete is offered while the profile is switched off**; when it is switched on, a muted line says
   why there is no button. The document carries no assignments, so an assigned profile still shows
   Delete and the manager's 409 names the instances. Reading the list on every page arrival to hide
   the button was weighed and left out (one more read per page for a rare case). Differs from 17b,
   whose document names its instance.
8. **Delete never deletes files.** The GUI offers to; the web's route removes only the section, so
   the question says the model file stays. Differs on purpose from the GUI.
9. **A taken name is checked in the page first**, against the list it read (letter case aside, as
   the manager compares), and a 409 `model-profile-name-taken` goes **under Name**, not in the form's
   alert: the text is about the Name field. Differs on purpose from 17b.5, which put its 409 in the
   alert.
10. **On the profile page the picker sits in the Assets card**, which stays collapsed on arrival, and
    opens for a refusal or page check on `model`, `hf-repo` or `hf-file` (the collapsed card's jump
    rule). `fe/DESIGN.md`'s "holds nothing focusable" is amended for this one card.
11. **A profile holding both `model` and `hf-repo`** (a hand edit) shows "A model file" checked
    (`model` is the key the GUI writes); saving either choice removes the other's keys. Unverified
    which one llama.cpp loads when both are set; the page does not need to know.
12. **A profile with neither, but `model-url` or `docker-repo`** (a hand edit; the manager's
    `FETCH_KEYS`) shows the picker's radios with none checked and a note naming the key; picking a
    choice and saving leaves that key alone (out of scope to change it).

## 2. The texts

Keys and English for `fe/src/strings/en_US.json`. `{n}` placeholders are filled by the code. Sizes are
written by the code (numbers and units stay out of the strings file, `AGENTS.md`): bytes / 2^30 with one
decimal and ` GiB`, below 0.1 GiB bytes / 2^20 with no decimal and ` MiB`.

### 22D

| Key | English |
|---|---|
| `model-profile-delete` | `Delete model profile` |
| `model-profile-delete-needs-off` | `To delete this model profile, switch it off first in the list of model profiles.` |
| `model-profile-delete-question` | `Delete the model profile {0}? Its settings are removed, with any unsaved changes on this page. The model file is not deleted. This cannot be undone.` |
| `model-profile-deleted` | `The model profile {0} was deleted.` |
| `api-model-profile-enabled` | `{0} is switched on, so it cannot be deleted. Switch it off in the list of model profiles first.` |
| `api-model-profile-assigned` | `{0} is given to the instances {1}, so it cannot be deleted. Remove it from each of them first, on the instance's page; a row set to Off counts too.` |
| `delete-unknown` | `Not confirmed: the manager did not answer in time, so it may or may not have been removed. The list shows it if it is still there.` (17f's text, unchanged) |

Reused: `confirm-delete`, `confirm-cancel`, `confirm-deleting`, `model-profile-back`,
`model-profile-not-found`, `api-no-model-profile-named`, `api-client-unavailable`, `session-ended-retry`,
`session-sign-in`, `session-revoked`, `api-other`, `request-failed`.

### 22P

| Key | English |
|---|---|
| `model-source-file` | `A model file` |
| `model-source-hf` | `A Hugging Face repo` |
| `model-source-legend` | `Where the model comes from` |
| `model-source-other` | `This profile downloads its model from {0} = {1}, which this page cannot change. Pick a model file or a Hugging Face repo to use instead; {0} stays in the file.` |
| `model-file-hint` | `A .gguf file in this machine's models folder: type part of its name to narrow the list, then pick it. For a model split in parts, the list shows the first part, which loads them all.` |
| `model-files-loading` | `Reading the models folder…` |
| `model-files-count` | `{0} files in the models folder match.` |
| `model-files-more` | `Showing {0} of {1} matching files: type more of the name to narrow the list.` |
| `model-files-no-match` | `No file in the models folder matches.` |
| `model-file-kept` | `Saved as {0}, which the list does not show. It stays as it is unless you pick a file from the list.` |
| `model-files-retry` | `Read the folder again` |
| `field-error-pick-file` | `Pick a file from the list.` |
| `model-files-empty` | `The models folder {0} holds no .gguf file yet. Put one there (pai-config can download one), then read the folder again; or use a Hugging Face repo.` |
| `model-files-truncated` | `The models folder holds more files than the list can offer: only the first 5000 can be picked.` |
| `model-files-unreadable` | `The models folder {0} could not be read, so no file can be picked now. Read it again later, or use a Hugging Face repo.` |
| `model-files-failed` | `The list of model files could not be read, so no file can be picked now. Read it again later, or use a Hugging Face repo.` |
| `model-file-parts` | `{0} parts` |
| `model-profile-hf-repo` | `Hugging Face repo` |
| `model-profile-hf-repo-hint` | `The repo as owner/name, such as ggml-org/gemma-3-1b-it-GGUF, or its huggingface.co address. Show quants lists its GGUF files: pick one. The instance downloads it the first time it loads this profile. Only public repos can be listed for now.` |
| `hf-show-quants` | `Show quants` |
| `hf-quants-loading` | `Reading the files of {0} from Hugging Face…` |
| `hf-quant-legend` | `Quant` |
| `hf-quant-default` | `llama.cpp's default` |
| `hf-quants-empty` | `{0} holds no GGUF file llama.cpp can load. Check the repo's name.` |
| `hf-repo-kept` | `Saved as {0}, kept as it is. To change it, show the quants of a repo and pick one.` |
| `field-error-pick-quant` | `Show the quants of a repo, then pick one.` |
| `field-error-hf-repo` | `Enter the repo as owner/name, such as ggml-org/gemma-3-1b-it-GGUF.` |
| `api-hf-repo-invalid` | same text as `field-error-hf-repo` (the bff's check of the same rule) |
| `api-hf-repo-unavailable` | `Hugging Face shows no public repo {0}. Check the name; gated and private repos cannot be added from here yet.` |
| `api-hf-repo-moved` | `The repo {0} has moved on Hugging Face: enter its new name.` |
| `api-hf-unreachable` | `Hugging Face did not answer in time. Check this machine's network, then try again.` |
| `api-hf-bad-answer` | `Hugging Face's answer could not be read. Try again later.` |
| `api-hf-tool-missing` | `This machine has no curl, which the web service needs to ask Hugging Face.` |
| `api-hf-busy` | `Two lists are already being read from Hugging Face. Try again in a moment.` |

Reused: `model-profile-model-file` (`Model file`), `field-error-comment-markers`,
`session-ended-retry`, `session-sign-in`, `session-revoked`, `api-client-unavailable`, `request-failed`.

### 22C

| Key | English |
|---|---|
| `model-profiles-new` | `New model profile` |
| `model-profiles-new-title` | `New model profile` |
| `model-profiles-new-note` | `A new model profile starts switched off and given to no instance.` |
| `model-profile-name` | `Name` |
| `model-profile-name-hint` | `Letters, digits, '.', '-' and '_'. It is the model's name for clients such as OpenCode, and cannot be changed here later.` |
| `model-profile-start-from` | `Start from` |
| `model-source-copy` | `A copy of another model profile` |
| `model-profile-copy-from` | `Model profile to copy` |
| `model-profile-copy-hint` | `Every setting of that profile is copied, comments included. Its instances are not: the copy is given to no instance and starts switched off.` |
| `model-profile-copy-choose` | `Choose a model profile` |
| `field-error-choose` | `Choose one.` |
| `api-model-profile-name-taken` | changed to `There is already a model profile named {0} (names are compared without letter case). Pick another name.` (today `a model profile named {0} already exists`, the manager's words) |
| `api-invalid-model-profile-id` | changed to `{0} cannot be a model profile name: use letters, digits, '.', '-' and '_'.` |

Reused: `create-button`, `create-cancel`, `create-creating`, `create-unknown`,
`api-no-model-profile-named`, `api-comment-markers`, and 22P's keys.

### 22E

No new key. Reused: `model-profile-summary` (its `{1}` is the `hf-repo` value when `model` is empty),
the save bar's keys, 22P's keys.

## 3. Criteria

### 22D: delete a model profile (8)

**22 D.1 Offered while switched off.** On a profile's page, when the document's `enabled` is `false`, a
secondary button `model-profile-delete` (no icon, as 17b's) comes after the
last card and before the save bar, outside the form. When `enabled` is `true` there is no button and a
`p.muted` `model-profile-delete-needs-off` stands in its place. It follows each document read.

**22 D.2 Confirm in place.** As 17b.7: pressing it shows right after it a `div role="group"` named
(`aria-labelledby`) by its paragraph `model-profile-delete-question` (the id filled in), then
`confirm-delete` and `confirm-cancel`, a `role="status"` line and the alert place; the focus moves to
`confirm-cancel`. Cancel or Escape inside the group hides it and puts the focus on the opener. Nothing is
sent before `confirm-delete`.

**22 D.3 The delete.** `confirm-delete` sends `DELETE /api/profiles/model/<id>` (the id
percent-encoded); meanwhile the status reads `confirm-deleting` and both buttons are `aria-disabled`; a
second press sends nothing. 204 replaces everything after the `h1` (the summary line, jump list,
cards, save bar) by a paragraph `model-profile-deleted` (`tabindex="-1"`, focused) and a link
`model-profile-back`; the unsaved-changes question no longer asks.

**22 D.4 Switched on meanwhile.** 409 `model-profile-enabled`: the document is read again; when it reads
`enabled: true` the group and the button go, and `api-model-profile-enabled` stays in a
`p[role="alert"][tabindex="-1"]` where the button was, focused (17b review D2), until the next read.

**22 D.5 Assigned.** 409 `model-profile-assigned`: `api-model-profile-assigned` with both args in the
group's alert; the group stays open and the focus stays on `confirm-delete`.

**22 D.6 Gone.** 404 `no-model-profile-named`: the page reads again and shows its not-found state, the
`model-profile-not-found` paragraph `tabindex="-1"` and focused.

**22 D.7 Other failures**, in the group's alert, focus on `confirm-delete`: 504 `client-timeout` →
`delete-unknown`, then the document is read again (gone: 22 D.6's state with its paragraph focused;
still there: the alert stays); 503 → `api-client-unavailable`; 401 → `session-ended-retry` with the
Sign in button opening the sign-in Dialog, the focus back on `confirm-delete` after a sign-in; 403
`session-not-administrator` → `session-revoked`; another 422 → `api-other` with the key; anything else
→ `request-failed`. No 504 reaches `request-failed` (17f.10).

**22 D.8 Strings.** The strings file gains exactly 22D's keys (section 2).

### 22P: the model source picker (12)

**22 P.1 The shape.** `ModelSource` is a `fieldset` whose `legend` is given by its user (22C:
`model-profile-start-from`; 22E: `model-source-legend`), holding radios `model-source-file` and
`model-source-hf` (22C adds `model-source-copy` third, 22 C.1), then, right after the radios, only the
controls of the checked choice: for a file, the Model file field (22 P.3); for Hugging Face, the
repo field and its quants (22 P.7). Switching the radio keeps what each choice holds (switch back and
the text is still there) and moves no focus ([3.2.2](https://www.w3.org/TR/WCAG22/#on-input)). The
radios are native inputs with their labels as their names
([1.3.1](https://www.w3.org/TR/WCAG22/#info-and-relationships),
[4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)).

**22 P.2 Its value** (decision 3: only listed items). The picker gives its user one of: `{model:
<the picked file's path>}`, `{hf-repo: <the picked quant's value["hf-repo"]>, hf-file?: <its
value["hf-file"]>}`, the **kept** saved value (22 E.2), or nothing. Typed text alone is never a value:
a file choice with no pick, or a Hugging Face choice with no quant picked, gives nothing, and its user
says so (22 C.4, 22 E.4).

**22 P.3 The Model file field.** A Field labelled `model-profile-model-file`, hint `model-file-hint`
behind its About toggle, the input `role="combobox"`, `aria-autocomplete="list"`,
`aria-expanded`, `aria-controls` the listbox, `autocomplete="off"`, `spellcheck="false"`, as the tag
suggestions' combobox (`fe/DESIGN.md` Chips): the `listbox` in the flow under the input, never
floating; Down opens it and moves `aria-activedescendant`, Enter or a click picks the active option
(its full `path` into the input, the list closes; that option is the pick), Escape closes it and keeps
the text; any edit of the text after a pick drops the pick (and a kept value, 22 E.2); no option is
picked automatically ([APG Combobox](https://www.w3.org/WAI/ARIA/apg/patterns/combobox/),
[2.1.1](https://www.w3.org/TR/WCAG22/#keyboard)).

**22 P.4 The folder is read once per showing.** `GET /api/node/model-files` is sent the first time the
Model file field is shown after the picker mounts (the create form opening with the file choice; the
Assets card opening, 22 E.1), and again by `model-files-retry` (22 P.6); not otherwise while it stays
mounted. The text filters only once the reply is in.

**22 P.5 The options.** From the files whose `category` is `models` or `null`, those whose `path`
relative to `dir` contains the typed text (letter case aside; all of them for an empty text), in the
reply's order, at most 50. Each option reads the relative path, then `, ` and the size, then, for
`shards` above 1, `, ` and `model-file-parts`; the relative path wraps anywhere
([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)). The picked option has `aria-selected="true"`; a
saved `model` equal to a listed `path` counts as picked on arrival.

**22 P.6 The folder's state line**, one `role="status"` line under the field (polite, so it does not
interrupt typing: [4.1.3](https://www.w3.org/TR/WCAG22/#status-messages)), one text at a time:
reading → `model-files-loading`; more than 50 match → `model-files-more` (50, the count); 1 to 50 →
`model-files-count`; none match a non-empty text → `model-files-no-match`; `files` empty and no `error`
→ `model-files-empty` (`dir`); `error` `models-dir-unreadable` → `model-files-unreadable` (its arg);
the read failed (503, 502, a network failure, a malformed body) → `model-files-failed`; 401 →
`session-ended-retry` with a Sign in button (after a sign-in the folder is read again); 403 →
`session-revoked`. When `truncated` is true, `model-files-truncated` follows the count in the same
line. For the empty, unreadable and failed states a secondary button `model-files-retry` follows the
line and reads the folder again (the focus stays on it). With a kept value (22 E.2) the line reads
`model-file-kept` (the saved path) first, then the folder's state.

**22 P.7 The Hugging Face fields.** A Field labelled `model-profile-hf-repo`, hint
`model-profile-hf-repo-hint`, text input `autocomplete="off"`, `spellcheck="false"`, then a secondary
button `hf-show-quants`, then a `role="status"` line and an alert place (Action feedback,
`fe/DESIGN.md` 5.2). The button is a separate tab stop after the input; Enter in the input does not
press it (Enter submits the create form in 22C, and the page's form in 22E).

**22 P.8 Show quants.** On press: a text starting with `http://` or `https://` whose host is
`huggingface.co`, `www.huggingface.co` or `hf.co` is first reduced to its first two path segments;
then a `:TAG` end is set apart; the rest must pass the repo id rule (pai-core's `valid_repo_id`, M1:
ASCII letters, digits and `_`, single `/`, `.` or `-` between them, exactly one `/`, at most 256
characters), else `field-error-hf-repo` under the field, `aria-invalid`, nothing sent. Valid: `GET
/api/hf/files?repo=<owner/name>` (URL-encoded); meanwhile the status reads `hf-quants-loading` (the
repo) and the button is `aria-disabled`; a second press sends nothing.

**22 P.9 The quants.** A 200 with files shows, after the button, a `fieldset` with `legend`
`hf-quant-legend` and one radio per file in the reply's order, each named by its `tag`, `, ` the size,
`, ` `model-file-parts` when `shards` is above 1, and for `default: true` `, ` `hf-quant-default`. Checked
on arrival: the file whose tag equals the field's `:TAG` (letter case aside), else the default one,
else none. The checked one is the pick: checking one (on arrival too) sets the field's text to its
`value["hf-repo"]` and keeps its `value["hf-file"]` (or none); the picker's value is that pair. The focus stays on the button; the status line says nothing more (the
radios are the answer). A 200 with no files: `hf-quants-empty` (the repo) in the status line, no
fieldset.

**22 P.10 Hugging Face refusals**, in the alert place (`role="alert"`), the field keeping its text,
the focus on the button: 400 `hf-repo-invalid` → under the field, as 22 P.8's check, focus on the
field; 404 `hf-repo-unavailable`, 502 `hf-repo-moved`, 504 `hf-unreachable`, 502 `hf-bad-answer`, 503
`hf-tool-missing`, 429 `hf-busy` → their `api-` texts with the repo; 401 → `session-ended-retry` with
Sign in (after a sign-in the focus returns to the button, nothing re-sent); 403 → `session-revoked`;
anything else → `request-failed`. No quant is then picked, so the choice has no new value (decision
3); a kept value stays kept (22 E.2).

**22 P.11 Edits drop what was listed.** Any edit of the repo field removes the quants fieldset, the
pick, a kept value (Discard brings it back on the page), the status text and the alert, silently and
without moving the focus. With a kept `hf-repo` the status line reads `hf-repo-kept` (the saved value)
until then.

**22 P.12 Strings and widths.** The strings file gains exactly 22P's keys. At 320 px and at 200 % text,
the radios, the field, the button, the listbox and the quants wrap with no sideways scroll; each control
is at least 24 by 24 px ([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

### 22C: the New model profile form (10)

**22 C.1 The form opens.** As 17b.1 with 17b2 (brief 16): on the list page, after the table (or the
empty line), the list's + icon button named `model-profiles-new` with `aria-expanded="false"`; pressing it sets
`true` and shows right after it a `form` named by its `h2` `model-profiles-new-title`, holding:
`p` `model-profiles-new-note`; the Name field (`model-profile-name`, hint `model-profile-name-hint`, text
input, `autocomplete="off"`, `spellcheck="false"`); the picker (22P) with legend
`model-profile-start-from` and a third radio `model-source-copy`; Create (primary) and Cancel; a
`role="status"` line and the alert place. "A model file" is checked on opening. The focus moves to
Name. Shown also when the list is empty, then without the copy radio (nothing to copy); not shown while
loading or after a failed read.

**22 C.2 The copy choice.** Checked, it shows a Field `model-profile-copy-from` (hint
`model-profile-copy-hint`), a native `select` whose first option is `model-profile-copy-choose` (value
`""`, selected on opening) followed by every id of the list the page last read, in the list's file
order, each option's text the id alone.

**22 C.3 Cancel.** Cancel, Escape inside the form, or the opener again hides the form, clears every
field, error, quant list and alert, sets `aria-expanded="false"` and puts the focus on the opener. A
new opening starts empty with "A model file" checked (and reads the folder again, 22 P.4).

**22 C.4 Page checks on Create**, in this order, nothing sent while any fails: Name empty after trimming
→ `field-error-required`; Name holding a character outside `A-Z a-z 0-9 . - _` →
`api-invalid-model-profile-id` (the name); Name equal to an id of the list, letter case aside →
`api-model-profile-name-taken` (decision 9). Then the checked choice: a file choice with no pick →
`field-error-pick-file` under Model file (whatever is typed, and also when the folder is empty,
unreadable or failed: its state line and Read again already say why); a Hugging Face choice with no
quant picked → `field-error-pick-quant` under the repo field (also when the listing failed); a picked
value holding `;` or `#` → `field-error-comment-markers`; copy with `""` selected →
`field-error-choose`. Each
invalid field gets `aria-invalid="true"` and `aria-describedby` its error, rendered before the focus
moves to the first invalid field (17b review D1;
[3.3.1](https://www.w3.org/TR/WCAG22/#error-identification),
[3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion)).

**22 C.5 The request.** A valid form sends `POST /api/profiles/model` with `{"id": <Name trimmed>,
"values": {"model": <the picked path>}}`, or `{"id", "values": {"hf-repo": <the pick's value>[,
"hf-file": <the pick's value>]}}`, or `{"id", "from": <the selected id>}`. Meanwhile the status reads
`create-creating` and Create and Cancel are `aria-disabled`; a second Create sends nothing. 201 goes to
`#/profiles/model/<id>` (the reply's `id`), whose `h1` takes the focus.

**22 C.6 Refusals on a field.** 422 with `field` `id` → its text under Name; `field` `from` → under the
copy select; `field` `model` → under Model file; `field` `hf-repo` or `hf-file` → under the repo field;
409 `model-profile-name-taken` → `api-model-profile-name-taken` under Name. Each marked as 22 C.4, the
focus on that field after its error renders. The texts by `writeFailure` (brief 16 section 2).

**22 C.7 Refusals in the alert.** `field` `values`, a 422 with no field, and every other failure go in
the form's alert (`role="alert"`): 504 `client-timeout` → `create-unknown`, then the list is read again
(17f.10); 503 → `api-client-unavailable` (nothing done); 401 → `session-ended-retry` with a Sign in
button opening the sign-in Dialog, the focus back on Create after a sign-in; 403 → `session-revoked`;
400, 413, 502, an unknown key, a network failure → `request-failed`. The fields keep what was typed
through every failure.

**22 C.8 Keyboard order.** Tab goes: opener, Name and its About toggle, the radios (one stop, arrows
between them), the checked choice's controls, Create, Cancel
([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)). Enter in Name or in the model file field (list
closed) submits the form.

**22 C.9 Strings.** The strings file gains exactly 22C's new keys and changes the two `api-` texts of
section 2.

**22 C.10 The UX check.** `fe/ux/check.js` `STATES` gains `model-profiles-new` (the form open on the
long list, "A model file" checked, the folder answered with the 40-character-path fixture),
`model-profiles-new-hf` (the Hugging Face choice with a listed repo of 12 quants, one shard count of 3),
and `model-profile-delete` (the group open on a switched-off profile); `fe/ux/stub.py` answers the two
list routes.

### 22E: the picker on the profile page (7)

**22 E.1 Where.** In the Assets card, the Model file row is replaced by the picker (legend
`model-source-legend`, two radios), right after the Engine row; the other rows stay read-only. The card
stays collapsed on arrival (decision 10); the folder is read when it first opens with "A model file"
checked (22 P.4).

**22 E.2 From the document: the kept value.** On each document read: `model` non-empty → "A model
file" checked, its text the value; else `hf-repo` non-empty → "A Hugging Face repo" checked, its text
the value, with `hf-file` as read; else none checked, and when `model-url` or `docker-repo` is set a
`p.notice` `model-source-other` (key, value) leads the fieldset (decision 12). The saved value is the
picker's **kept** value (decision 3) until an edit of that field drops it: a saved `model` equal to a
listed `path` is shown as that option picked; one the list does not show (outside the folder, or the
folder unreadable or failed) reads `model-file-kept`; a saved `hf-repo` reads `hf-repo-kept` until Show
quants answers (then the quant matching its `:TAG` is picked, which leaves the value unchanged when
`value` equals the saved keys). The page is clean while the picker's value equals the saved one, so a
profile with a kept value saves its other edits with no picker key sent.

**22 E.3 What Save sends**, among the page's other edits, only when the picker's value changed: a
picked file → `model` its path, and `hf-repo: null`, `hf-file: null` for those the document holds; a
picked quant → `hf-repo` and `hf-file` from its `value` (`hf-file: null` when the document holds one and
the `value` has none), and `model: null` when the document holds one. Discard puts the kept value back
and drops any quant list.

**22 E.4 Page checks at Save**, with the page's other checks (`save-invalid`), only when the kept value
was dropped: a file choice with no pick → `field-error-pick-file`; a Hugging Face choice with no quant
picked → `field-error-pick-quant`; a picked value holding `;` or `#` → `field-error-comment-markers`. A
failing check opens the card, the error renders, then the focus moves. A kept value is never checked
(the page sends nothing for it).

**22 E.5 Refusals.** A save refusal with `field` `model`, `hf-repo` or `hf-file` (`model-profile-no-model`,
`model-profile-model-missing`, `comment-markers`) marks that field with its text, opens the card and
follows the page's existing refusal rule (`save-refused`, 02h).

**22 E.6 The summary line.** When `model` is empty and `hf-repo` is set, the line reads
`model-profile-summary` with `{1}` the `hf-repo` value; `model-profile-summary-no-model` only when both
are empty. It follows the saved values, not the edits.

**22 E.7 The UX check.** `STATES` gains `model-profile-source` (the Assets card open, Hugging Face
checked with a listed repo) on the long profile.

Tests that change: 22E changes the Assets card tests that assert the Model file row as read-only text
(`ModelProfilePage.test.tsx`, the H11a tests), and the collapsed card's "nothing focusable" assertion
for this card.

## 4. Out of scope

- Searching Hugging Face by words (the GUI's search), downloading from the page, the HF token (M5),
  the director's store source.
- A path or repo typed by hand as the value (decision 3); a private or gated repo (M5).
- Renaming a model profile; deleting its model file; deleting a switched-on or assigned profile.
- `mmproj` and drafter pickers (M3's `category` feeds them later), `model-url` and `docker-repo` editing.
- A Delete or Copy on the list's rows.
- Showing a profile's assignments on its page (decision 7).

## 5. The user's answers (2026-11-23)

1. Copy is a third "Start from" choice of New (as recommended): decision 1.
2. Delete is shown whenever the profile is switched off, the 409 naming the instances (as
   recommended): decision 7.
3. Only listed items, no free text beside the lists (not the recommendation): decision 3, 22 P.2,
   22 P.3, 22 P.6, 22 P.9 to 22 P.11, 22 C.4, 22 C.5, 22 E.2 to 22 E.4.

## 22D review (designer, 2026-11-23)

I read the 22D diff (`ModelProfilePage.tsx` 298 to 431, 936 to 962, 1630 to 1740, 1943 to 1990, 2010 to
2100; `en_US.json`; `fe/ux/`). I drove it in Chromium, Firefox and WebKit at 1280 and 320 px, light and
dark, at 100 % and 200 % text (`build/ux-review/22d.js`, `22d2.js`, `22d3.js`; shots in
`build/ux-review/22d/`). **Verdict: one defect, D1, and one small one, D2.**

- **Met: 22 D.1 and 22 D.2.** Delete shows only while the profile is switched off; when it is on,
  the muted line takes its place. The group is named by its question, and the focus lands on Cancel,
  with Delete one Shift+Tab away. Escape closes the group and puts the focus back on the opener.
- **Met: 22 D.3.** While the delete runs, the status reads "Deleting…" and both buttons are
  `aria-disabled`. Playwright waits through an `aria-disabled` button, so the one request a second
  press sent was a test artifact, not the page's. A 204 shows the focused paragraph and the back link.
- **Met: 22 D.4 to 22 D.7, the same in all three engines.**
  - A 409 `model-profile-enabled` (the read again answering `enabled: true`) moves the focus to
    `p#model-profile-delete-refused[role=alert]`, and the opener is gone.
  - A 409 `model-profile-assigned` puts its text, naming both instances, in the group's alert, with
    the focus on Delete.
  - A 404 moves the focus to the `model-profile-not-found` paragraph.
  - A 504 shows `delete-unknown` with the focus on Delete when the profile is still there, and lands
    on the focused not-found paragraph when it is gone.
- **Met: 22 D.8.** The strings are in place. No sideways scroll at 320 px or 200 %, and every button
  is at least 24 px.
- **D1 (22 D.1, layout, 1280 px and wider): the opener is a 696 px pill across the whole content
  column.** At 1280 px and wider the settings `section` becomes the page's two-column grid
  (`index.css` ~1270, `section:has(> .jump)`). The button is a direct grid item there, so it
  stretches; the same button on the server profile page and at 320 px is as wide as its words. Fix:
  `#model-profile-delete { justify-self: start }`, or wrap the opener and its group in one `div`
  (`ModelProfilePage.tsx` ~1963).
- **D2 (small, both widths): the question touches the opener.** The group starts at the button's
  bottom edge (gap 0, the `p` margin 0). Give the group `margin-top: var(--space-3)`, here and in
  17b's `ConfirmDelete` (`ServerProfilePage.tsx` ~507), so the two pages stay alike.
- `npm --prefix fe/ux run check`: 174 states per browser, 0 failing, the new `model-profile-delete`
  state included (`build/ux-review/check-22d.txt`). After D1 and D2 are fixed, a re-check of the
  layout is enough (`build/ux-review/22d.js`).

**Re-check (2026-11-23): D1 and D2 fixed, 22D accepted.** Checked in Chromium, Firefox and WebKit at
1280, 1600 and 320 px and at 200 % text, on the model profile page and 17b's server profile page
(`build/ux-review/22d4.js`):
- The opener is as wide as its words on both pages (185 to 187 px at 100 %), and it wraps inside the
  column at 200 %.
- The confirm group starts 12 px (`--space-3`) under the opener on both pages.
- Cancel takes the focus, and nothing scrolls sideways.

No UX check state changed, so the check was not run again.

## 22D2: Delete as a − beside the status, and the edit commands at the top (user feedback, 2026-11-23)

The user, on the deployed 22D, verbatim: "si ok delete, ma metti un meno alla destra dello status e i
comandi di modifica in alto nelle pagine del modello, così [non] devo scorrere fino alla fine". In
English: yes, Delete works, but put a minus to the right of the status and the edit commands at the
top of the model profile pages, so the user does not have to scroll to the end. One sub-slice, **10
criteria (22 D2.1 to 22 D2.10)**. It amends 22 D.1, 17b2.4's sentence "destructive actions stay text
buttons", and `fe/DESIGN.md`'s Save bar, Confirm in place and Icon buttons of a list.

**Decisions** (preferences, each following the user's words):
- **The − goes on the model profile page only, in a status row under the summary line.** The page
  is where the profile's status is a sentence. The Models list's rows already carry the on/off switch
  in a dense table, and a destructive icon on every row, a click away from the switch, would invite
  mistakes. The list keeps no delete.
- **The server profile page follows** (17b2.6 below): the same row, with its "used by" status and a
  −. The two delete flows keep reading the same.
- **The Save bar moves to the top of every form page** (model profile, server profile): the first
  thing after the page's status row, sticky at the viewport's top. It is one component (`SaveBar.tsx`,
  used by `ModelProfilePage.tsx` and `ServerForm.tsx`), so one rule serves both. The Instance page has
  no Save bar.
- **Delete keeps a confirm.** The − only opens the Confirm in place, which keeps its words, its focus
  rules and 22D's answers.

**22 D2.1 The status row.** Right after the summary line (`p.model-profile-summary`), a `div.page-status`:
a flex row that wraps, `gap: var(--space-3)`, `align-items: center`. It holds:
- the status paragraph (`flex: 1 1 16rem`): `model-profile-off` when switched off (the existing
  text); when switched on, `model-profile-on` (new: `This model profile is switched on: its engine
  serves it. To delete it, switch it off first in the list of all model profiles.`);
- then, only when switched off, the − button (22 D2.2), last in the row so it sits at the
  paragraph's right, or under it when the row wraps (320 px, 200 %).
The old bottom paragraphs go: `model-profile-off` before the settings `section`, the muted
`model-profile-delete-needs-off` and the bottom Delete button. The key `model-profile-delete-needs-off`
leaves the strings file.

**22 D2.2 The − button.** `id="model-profile-delete"`, `type="button"`, `aria-expanded`. Its content:
- a new `minus` icon (`aria-hidden`; `Icon.tsx` gains it, the same 24-unit stroke family as `plus`);
- a `span.visually-hidden` holding `model-profile-delete` ("Delete model profile"), so its accessible
  name stays exactly that ([4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)). There is no
  visible text, so 2.5.3 does not apply.
Its box is a 40 px square with the `--control` outline, as `.ot-add`, sharing that rule, plus
`flex: none`. Under forced colours the icon is `ButtonText`.

**22 D2.3 The confirm opens under the row.** The 22 D.2 group comes right after `div.page-status`, full
width, with `margin-top: var(--space-3)`. Everything else of 22 D.2 to 22 D.7 stands:
- the focus moves to Cancel; Cancel or Escape returns it to the −;
- the status line and the alert live in the group;
- a 204 replaces everything after the `h1`;
- the 404 and 504 rules stand.

**22 D2.4 Switched on meanwhile.** After the 409 `model-profile-enabled` and its read again, the row
says `model-profile-on` and the − is gone. The refusal's `p#model-profile-delete-refused[role=alert]
[tabindex=-1]` comes right after the row and takes the focus (17b review D2).

**22 D2.5 The Save bar at the top.** On the model profile page, `SaveBar` renders inside the settings
`section`, right after its `h2` (before the jump list and the cards), with the same content: the status
line, the alert place, Discard and Save changes. At 1280 px and wider it is the first item of the
content column (`grid-column: 1`); the rail starts beside it.

**22 D2.6 Sticky at the top.** `.save-bar` changes:
- `position: sticky; top: 0`;
- the border on its bottom edge (`border-bottom: 1px solid var(--control)`), only its bottom corners
  round;
- the same shadow, background and wrap.
The scroll padding moves with it: `html:has(.save-bar)` sets `scroll-padding-top` (not bottom) to
`calc(max(var(--save-bar-height, 7rem), 7rem) + var(--space-3))`, and `.save-bar button` takes the
equal negative `scroll-margin-top`. A field focused under the bar is then scrolled clear of it, and
focusing a bar button scrolls nothing
([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)). Below a viewport height of
32rem the bar is static and both rules are 0, as today; it then sits at the top of the settings.

**22 D2.7 The focus order** ([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)): back link, the
page's `h1` (when focused), the status row's −, the confirm group when open, the Save bar (Discard,
Save changes, and its alert's button), the jump list, then the cards. Unchanged:
- a save's own checks and refusals still move the focus to the marked field (the bar's top padding
  keeps the field clear of the bar);
- a save that succeeds leaves the focus on Save changes;
- the bar's alerts keep their roles.

**22 D2.8 The server profile page follows** (amends 16 17b.6 to 17b.9 and 17b2.4). Under its `h1`, a
`div.page-status` holds a paragraph and a − (`id="server-profile-delete"`):
- the paragraph is `server-profile-unused` (new: `No instance uses this server profile.`) when the
  document's `instance` is `null`, and `server-profile-used-by` (new: `Used by instance {0}. To
  delete this server profile, give that instance another one first.`) when it names one;
- the − is there only when unused, named by the visually hidden `server-profile-delete`;
- the confirm opens under the row; the 409's `server-profile-in-use` paragraph comes after the row
  and takes the focus.
The bottom Delete button after the Value list goes. The Value list's "Used by" row stays (it is
data).

**22 D2.9 Widths.** At 320 px, at 200 % text, and at 1280 and 1600 px, in both themes: no sideways
scroll ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)). The − wraps under the status text without
leaving its row. The sticky bar never covers a focused control.

**22 D2.10 Strings and tests.** The strings file gains `model-profile-on`, `server-profile-unused`
and `server-profile-used-by`, and loses `model-profile-delete-needs-off`. These tests change:
- 22D's tests that find the bottom button or the muted line;
- 17b's that assert the button after the Value list;
- the Save bar tests that pin `bottom: 0`, `scroll-padding-bottom` and `scroll-margin-bottom`
  (`index.css` 2165 to 2222);
- the UX check states `model-profile-delete` and `server-profile-delete`, which find the opener by
  id.

**Open for the user (the build proceeds on this reading):** the Save bar has been sticky at the
bottom of the viewport since 2h, so in a normal window Save and Discard were always on screen. If
"the edit commands" meant only Delete, 22 D2.5 to 22 D2.7 can be dropped and the bar stays at the
bottom.

## 22D2 review (designer, 2026-11-24)

I drove both profile pages in Chromium, Firefox and WebKit at 1280 and 320 px, light and dark, at
200 % text and under forced colours (Chromium), with keyboard (`build/ux-review/22d2.js`, `22d2s.js`,
`22d2c.js`; shots and the run log in `build/ux-review/22d2/`). The machine was loaded (load average
about 13), so I used a longer navigation timeout and a settle time after each key press.
**Verdict: accepted, no defect.**
- **22 D2.1 and D2.2 met.**
  - The − is 40 px (40 by 45 px at 200 % text) and is the last item of `div.page-status`. At 1280 px
    it sits on the paragraph's row at the right; at 320 px and 200 % it wraps under it.
  - It is announced as `button "Delete model profile"` and `button "Delete server profile"`.
  - The icon and outline follow the theme, and are `ButtonText` under forced colours.
  - No sideways scroll anywhere.
- **22 D2.5 and D2.6 met in all three engines.** The bar starts after the settings `h2` (the model
  page) or the Server intro (the server page). While the page scrolls (y = 800, 2000, 4000) its top
  stays at 0, at 1280, 1600 and 320 px.
  - **The reviewer's suspected D1 is refuted:** as a grid item of `section:has(> .jump)` at 1280 and
    1600 px, the bar still sticks for the whole scroll in Chromium, Firefox and WebKit. No wrapper is
    needed.
- **22 D2.7 met.** From the `h1`, Tab goes: the −, Discard, Save changes, then the jump list or the
  first field, on both pages and at both widths.
- **2.4.11, no focused control under the bar.** I Shift+Tabbed 60 steps up from the end of each page
  with a 250 ms settle. Every control focused after the page had scrolled lay clear of the bar, in all
  three engines. The first run, without a settle, reported controls under or beyond the bar in Firefox
  and WebKit. They were measured before the scroll that focus starts had ended, and they did not
  repeat with the settle.
- 22 D2.3, D2.4 and D2.8 behave as 22D's re-checked rules: the group opens under the row, and the
  focus goes to Cancel and back to the −.
- A note, not a defect: on the server page, the status sentence and the Value list's "Used by" row say
  the same thing one line apart. The brief keeps the row as data; whether to drop it is the user's
  call.
- `npm --prefix fe/ux run check`: 180 states per browser. One fails: WebKit 1280 px light
  `sign-in-at-home`, "not rendered: #page-heading Sign in" (`build/ux-review/check-22d2.txt`). That is
  the sign-in page, which 22D2 does not touch; its other three variants and both 22D2 states pass. I
  read it as the heading missing the check's wait while the machine was loaded, not as a defect. A
  rerun on a quiet machine should confirm it. The first attempt did not start: its stub printed
  nothing within the check's 5 s.

## 22D3: the server profile page says who uses it once (user, 2026-11-24)

The user agreed with the 22D2 review's note: the status sentence and the Value list's "Used by" row,
one line apart, say the same thing. **Decision (preference): the status sentence stays and the
Value list goes.** The sentence carries the − and the delete rule, and it gains what the row had
that the sentence lacks: the instance as a link, in its 17f name. One sub-slice, **5 criteria**.

**22 D3.1 The Value list goes.** `ServerProfilePage.tsx` no longer renders the `dl.assets` with
`server-profiles-column-used-by` (~459 to 473). Nothing stands between the status row (and its
confirm group or refusal paragraph) and the Server section.

**22 D3.2 The instance in the sentence is a link.** When the document names an instance, the
paragraph reads `server-profile-used-by` with `{0}` drawn as the same `InstanceLink` the row used
(the instance's 17f name, its id small beside it, going to its page). The text around it comes from
the string split at `{0}`, so the words stay in the strings file. `server-profile-unused` is
unchanged. The − stays the row's last item, at the paragraph's right (22 D2.1).

**22 D3.3 The skeleton.** The 17a.6 skeleton line that stood for the Used by row now stands where the
status row will be (one `.skeleton` line after 300 ms, the read's words in the hidden `role="status"`
as before).

**22 D3.4 The 409 in use.** After `client-server-profile-in-use` and its read again, the status row
names the instance (a link), the − is gone, and `p#server-profile-in-use[role=alert][tabindex=-1]`
right after the row takes the focus (17b review D2, 22 D2.8, unchanged).

**22 D3.5 Strings and tests.** No key is added or removed: `server-profiles-column-used-by` and
`server-profiles-unused` stay in use on the Server profiles list. The tests that change are those
asserting the page's Used by row:
- `ServerProfilePage1.test.tsx` and `ServerProfilePage2.test.tsx` (the row, its muted "No instance"
  and its link);
- `InstanceName17f.test.tsx` (the instance's name on the profile page, now inside the status
  sentence);
- `U17aSkeletons.test.tsx` (the skeleton's place);
- `ModelProfileDelete2.test.tsx` (its server profile page checks of 22 D2.8, when they find the row).
`ServerProfiles1.test.tsx` keeps its list's Used by column. The UX check's `server-profile-delete`
state needs no change (it finds the − by id).
