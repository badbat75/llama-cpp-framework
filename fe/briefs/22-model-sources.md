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
  models folder for every engine" (3)). Both are served since 2026-11-24 (M2 04e085d, M3 da525bd); where
  the served shape differs from the design's below, "22P check (2026-11-25)" at the end wins:
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
- **17f, built** (f70d631): its `delete-unknown` text and the 504 rule of 17f.10 (a 504 is "not
  confirmed", never `request-failed`). MP3 was built first, so 22D brought `delete-unknown` and 22C and
  22D applied 17f.10's rule to their own writes; 17f reused both. 17f's **Rename** is gone (17f2,
  4c53fb5): the display name is set from a pencil after the instance page's heading.
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

**22 P.5 The options.** (Amended 2026-11-25: the `category` filter is replaced, see "22P check" P.5a.)
From the files whose `category` is `models` or `null`, those whose `path`
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
fieldset. (Amended 2026-11-25, "22P check" P.9a: the bff answers a repo with no GGUF as 502
`hf-bad-answer`.)

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

## 22D4: Delete as a − on each list row, and a Delete button in the profile pages' Save bar (user, 2026-11-24)

The user, after the 22D2 deploy: "you did not understand, the − must be in the model profiles LIST
beside each model, not in the model page; in the model page you could leave the Delete button beside
Save and Discard". His answers to the coordinator: the same for server profiles; on the lists, the
− of a profile that cannot be deleted is shown disabled, with a hint like "Switch it off to delete",
not hidden, so the rows stay aligned.

**What 22D4 supersedes:**
- 22 D2.1 to D2.4 and D2.8 (the status row with its −, on both pages);
- 22D3 entirely (it fixed a repetition the status row made);
- 22 D.1's place of the opener.
**What it keeps:**
- 22 D2.5 to D2.7: the Save bar at the top, sticky;
- 22 D.2 to D.7: the confirm's words, focus and answers, adapted below;
- 22D's endpoint and texts.
One sub-slice, **12 criteria (22 D4.1 to D4.12)**: past 12 the playbook splits, so 22 D4.1 to D4.8
(the lists) may run before 22 D4.9 to D4.12 (the pages).

**Decisions** (preferences, following the user and `fe/DESIGN.md` principles 6 to 14):
- **The − sits in the row's status cell**, after the switch on the Models list and after the Used by
  value on the Server profiles list. The status is beside the control that acts on it (principle
  10), and no column is added.
- **A − that cannot act is `aria-disabled`, not `disabled`.** It stays focusable, so a keyboard or
  screen reader user meets its reason. The reason is its description, and a hint shows on hover and
  focus.
- **The confirm opens in a row of its own under the profile's row** (`tr` > one `td` spanning every
  column). It is one group at a time, readable at 320 px inside the table's scroll box.
- **On the pages, Delete is a text button in the Save bar**, at the start of the button group,
  before Discard and Save changes, with a `--space-5` gap after it. The destructive verb sits apart
  from Save (principle 9).

### The lists (22 D4.1 to D4.8)

**22 D4.1 The − on every row.** In the Status cell of each Models list row, after the switch, a button
`.ot-add`-styled (the shared 40 px rule, `flex: none`) with the `minus` icon (`aria-hidden`) and a
visually hidden `model-profiles-delete-row` (`Delete model profile {0}`, the row's id), so each row's
name is unique ([2.4.6](https://www.w3.org/TR/WCAG22/#headings-and-labels),
[4.1.2](https://www.w3.org/TR/WCAG22/#name-role-value)). In the Used by cell of each Server profiles
row, after the value, the same button named `server-profiles-delete-row` (`Delete server profile
{0}`). The cell is a flex row that wraps (`gap: var(--space-3)`, `align-items: center`), so the −
lines up with the switch or the value.

**22 D4.2 Enabled only when deletable.** The − is active for a switched-off model profile
(`enabled: false`) and for a server profile no instance uses (`usedBy` empty, as the list row
answers). Otherwise it carries `aria-disabled="true"` and `aria-describedby`, pointing to a
visually hidden span holding:
- `model-profiles-delete-needs-off` (`Switch it off to delete it.`), or
- `server-profiles-delete-in-use` (`An instance uses it: give that instance another server profile
  to delete this one.`).
A press of a disabled − sends nothing and opens nothing. It follows each list read and each switch.

**22 D4.3 The hint.** On hover, and on keyboard focus, of a disabled −, its description also shows
as a small `.hint-tip` under the button:
- `--surface` fill, `--control` border, the body text size, at most 16rem wide and wrapping;
- it never covers the button itself;
- it stays while the pointer is over the button or the tip, and Escape hides it
  ([1.4.13](https://www.w3.org/TR/WCAG22/#content-on-hover-or-focus)).
An active − shows no tip: its name says what it does.

**22 D4.4 The confirm row.** Pressing an active − inserts right after the profile's row a
`tr.confirm-row` with one `td` (`colspan` the column count). The `td` holds the 22 D.2 group:
`role="group"`, named by its question, then `confirm-delete`, `confirm-cancel`, a `role="status"` line
and the alert place. The question is the list's own:
- `model-profiles-delete-question`: `Delete the model profile {0}? Its settings are removed; the model
  file is not deleted. This cannot be undone.`
- `server-profiles-delete-question`: `Delete the server profile {0}? Its settings file is removed.
  This cannot be undone.`
The − takes `aria-expanded="true"`, and the focus moves to Cancel. Cancel or Escape removes the row
and puts the focus on the −. Opening another row's confirm closes the first, its focus going to the
new Cancel. The filter hiding the row closes it.

**22 D4.5 One delete at a time.** While a delete runs, the group's status reads `confirm-deleting`,
its buttons and every row's − and switch are `aria-disabled`, and a second press sends nothing
(`fe/DESIGN.md` 5.2, action feedback in a list).

**22 D4.6 Done.** A 204 removes the profile's row and the confirm row. The list's region above the
table (the switch's `role="status"` line on the Models list; the same line added above the Server
profiles table) says `model-profiles-deleted` / `server-profiles-deleted` (`The model profile {0} was
deleted.` / `The server profile {0} was deleted.`). The focus moves to the next row's −, else the
previous row's −, else the list's +
([2.4.3](https://www.w3.org/TR/WCAG22/#focus-order)). The list is not read again for a 204.

**22 D4.7 Refusals.** Unless said, the text goes in the group's alert and the focus stays on
`confirm-delete`; the 22 D.7 mapping holds (401 with Sign in, 403, 503, `api-other`,
`request-failed`).
- **409 `model-profile-enabled`** (the page was stale): the list is read again, the confirm row goes,
  and the row now shows the profile switched on with its − disabled.
  `api-model-profile-enabled` goes in the region's `role="alert"` line. The focus goes to the row's
  −, which is still focusable.
- **409 `model-profile-assigned`**: `api-model-profile-assigned` in the group.
- **409 `client-server-profile-in-use`**: as `model-profile-enabled`, with
  `api-client-server-profile-in-use`.
- **404**: the list is read again. `api-no-model-profile-named` (or `api-client-no-server-profile`)
  goes in the region's alert, and the focus moves as after a 204.
- **504**: `delete-unknown` in the group, then the list is read again. If the row is gone, the same
  text moves to the region's alert and the focus moves as after a 204.

**22 D4.8 Widths.** At 320 px and at 200 % text the − wraps under the switch or value inside its
cell, and the confirm row's group wraps inside the table's scroll box. The page has no sideways
scroll ([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)). The 40 px target holds
([2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)).

### The profile pages (22 D4.9 to D4.12)

**22 D4.9 The status row goes.**
- **Model profile page:** `div.page-status` and its − leave. The status paragraph stays, a plain
  `p` under the summary line: `model-profile-off`, or `model-profile-on`, whose last sentence now
  reads `To delete it, switch it off first.`
  *Amended 2026-11-25 (brief 24, 23 A2.1, built in b3b1446):* the paragraph is now the state line,
  with the page's own switch and the short texts `model-profile-on-short` and
  `model-profile-off-short`. `model-profile-on` and `model-profile-off` are gone, and so is the
  sentence "To delete it, switch it off first.": the switch is right there, and the Save bar's
  reason line (D4.14) says why Delete cannot act.
- **Server profile page:** the status row and its sentence leave (`server-profile-unused` and
  `server-profile-used-by` leave the strings file). The Value list's Used by row stays as 17a has it,
  and 22D3 is not built.

**22 D4.10 Delete in the Save bar.** Both pages' Save bar holds, at the start of its button group, a
secondary text button: `model-profile-delete` (`Delete model profile`) on the model page,
`server-profile-delete` (`Delete server profile`) on the server page. It keeps the ids
`model-profile-delete` and `server-profile-delete`. After it comes a `--space-5` gap, then Discard and
Save changes.
- It is **active** when the profile can be deleted: switched off, or no instance.
- Otherwise it is `aria-disabled`, with the list's description and the 22 D4.3 tip:
  `model-profiles-delete-needs-off` and `server-profiles-delete-in-use`.
- Its state does not follow the form's dirty state.

**22 D4.11 The confirm in the bar.** Pressing it opens the 22 D.2 group inside the bar, under its
buttons, using the page's question (`model-profile-delete-question`, `server-profile-delete-question`),
and the focus moves to Cancel. The bar grows, and its scroll padding follows its height
(`--save-bar-height`, 22 D2.6). Escape and Cancel close it, the focus returning to Delete. The
answers are 22 D.3 to D.7:
- a 204 replaces everything after the `h1`;
- a 409 `model-profile-enabled` or in-use reads the page again, leaves Delete disabled, puts its text
  in the bar's alert and moves the focus to Delete;
- a 409 `model-profile-assigned` goes in the group;
- the 404 and 504 rules stand.

**22 D4.12 Strings and tests.**
- **Strings gained:** `model-profiles-delete-row`, `server-profiles-delete-row`,
  `model-profiles-delete-needs-off`, `server-profiles-delete-in-use`,
  `model-profiles-delete-question`, `server-profiles-delete-question`, `model-profiles-deleted`,
  `server-profiles-deleted`.
- **Strings removed:** `server-profile-unused` and `server-profile-used-by`.
- **Strings changed:** the `model-profile-on` text.
- **Tests that change:**
  - `ModelProfileDelete.test.tsx` and `ModelProfileDelete2.test.tsx` (22D and 22D2: the opener's
    place, the status row);
  - `ModelProfiles.test.tsx` and `ModelProfilesSwitch.test.tsx` (the Status cell's content, a write
    blocking the − and the switch);
  - `ServerProfiles1.test.tsx`, `ServerProfiles2.test.tsx` and `ServerProfiles3.test.tsx` (the Used by
    cell, the region line);
  - `ServerProfilePage1.test.tsx` and `ServerProfilePage2.test.tsx` (no status row, Delete in the
    bar);
  - the Save bar tests that count its buttons (`HearthSaveBarScroll.test.tsx` and the save tests that
    find Discard as the first button);
  - `InstanceName17f.test.tsx`, only if it reads the status sentence.
- **The UX check:** its states `model-profile-delete` and `server-profile-delete` (`fe/ux/check.js`,
  edited by another agent now) must open the confirm from the bar's Delete. It gains
  `model-profiles-delete` (the Models list with a confirm row open) and the disabled −'s tip on
  focus.

**Open for the user** (nothing waits on it): on the profile pages a disabled Delete with a tip
mirrors the lists. If he would rather the page's Delete be hidden when it cannot act, 22 D4.10 drops
its disabled state.

## 22D4 review (designer, 2026-11-24)

Driven in Chromium at 1280 and 320 px, light and dark, 200 % text and forced colours
(`build/ux-review/22d4.js`); Firefox and WebKit wait for the fixes below. The UX check
(`npm --prefix fe/ux run check`) did not finish a pass: it threw in all three browsers.

Met: D4.1 (40 px −, its name "Delete model profile {id}" / "Delete server profile {id}"), D4.2
(`aria-disabled` and the description on switched-on and used rows; Enter on a disabled − opens
nothing), D4.4 (the confirm row spans every column, the focus goes to Cancel; Escape and Cancel
return it to the −), D4.6 (a 204 removes the row, the region says `server-profiles-deleted`, the
focus goes to the previous row's −), D4.7 for 409 `model-profile-enabled` (the alert in the region,
the focus on the row's −), D4.8 (no sideways scroll at 320 px or 200 %, with the confirm open), D4.9
to D4.11 on both pages where the profile can be deleted (Delete first in the bar, a 32 px gap before
Discard, the confirm inside the bar, the focus on Cancel, Escape back to Delete).

Not met:
- **D1 (D4.3) The tip is not under the −.** `.hint-tip` is `position: absolute` (`fe/src/index.css`
  2125-2129), and no ancestor of the − is positioned (`fe/src/RowDelete.tsx` 162-197 renders the
  button and the tip as siblings in the `.status-cell` td). The tip lands at the page's left edge,
  432 px below the − at 1280 px, below the fold, and 650 px above it, out of view, at 200 %. A
  pointer user never sees the reason. Fix: wrap the − and its tip in a `position: relative` box.
  The Save bar's tip (`fe/src/SaveBar.tsx` 332-342) anchors to `.save-bar` (sticky, so positioned):
  it starts at the bar's left edge, not under Delete; the same wrapper fixes it.
- **D2 (`fe/DESIGN.md`, "aria-disabled ... drawn inactive") The disabled − looks active.** Its border
  and icon match the active −'s (computed `rgb(140, 125, 112)` on both). `fe/src/index.css`
  2397-2401 draws `.primary` and `.secondary` inactive, not `.ot-add`. Fix: add
  `.ot-add[aria-disabled='true']` to that rule (the `--muted` icon).
- **D3 The UX check throws.** `fe/ux/check.js` 415 and 420 pick the + by `path[d="M5 12h14"]`, which
  the minus icon (`fe/src/Icon.tsx` 186) shares, so the state `server-profiles-new` hits a strict-mode
  violation in every browser. Fix: `path[d="M12 5v14"]`, the plus's second stroke.
- **D4 (D4.7, minor) A region alert outlives the next delete.** After a 404 the region's alert stays
  when a later delete succeeds: the status says "was deleted" beside "There is no server profile of
  that name." Clear the region's alert when a confirm opens.

Not checked in a browser: the server page's disabled Delete (the stub answers the `default` page's
server read with an error under `pool=long`, so the bar does not render); the vitest suite covers it.

### 22D4 re-check (designer, 2026-11-24)

Driven in Chromium, Firefox and WebKit at 1280 and 320 px, light, dark, 200 % text and (Chromium)
forced colours (`build/ux-review/22d4r.js`, `22d4s.js`; the used server page needs the stub's
`key=set`, since its default `withheld` makes the page say `server-load-failed`).

- D2 met: the disabled − draws `--muted` (107, 93, 82 against 42, 33, 28 on the active one; GrayText
  under forced colours) in all three engines.
- D3 met by the code (`fe/ux/check.js` picks the + by `M12 5v14`); the coder's pass was 0 failing.
- D4 met: after a 404 the region's alert is cleared when the next confirm opens.
- The used server page's Delete is `aria-disabled`, described by `server-profiles-delete-in-use`, and
  Enter opens nothing.
- **D1 still not met**, in all three engines:
  - **D1a: the list tip is clipped by the table's scroll box.** `.table-scroll` has
    `overflow-x: auto` (`fe/src/index.css` 714-715). The − sits in the last column and the tip opens
    to its right (`left: 0`, `fe/src/index.css` `.hint-tip`), so at 1280 px it runs to 1263 px while
    the box ends at 1144 px: only "An instance uses" shows. At 320 px it is cut at 304 px, and at
    200 % its 512 px run to 652 px. An absolute tip inside that box will always be clipped there (a
    scroll box clips both axes). Fix: show the tip in the top layer, out of the box (`popover="manual"`,
    placed under the − from its `getBoundingClientRect`, its right edge at the −'s right edge and
    kept inside the viewport), or in the flow under the − if the coder prefers no script placement.
  - **D1b: the tip cannot be hovered (1.4.13).** Moving the pointer straight down from the − onto the
    tip hides it: the `margin-top: var(--space-1)` gap between them means the pointer first leaves
    to the cell, and `leave` (`fe/src/Hint.tsx` 33-41) hides it. Fix: no gap the pointer crosses
    (the tip's top edge touching the control, or its room given as transparent padding of the tip).
  - **D1c: the Save bar's tip is not under Delete once the bar wraps** (the reviewer's point,
    confirmed). `SaveBar.tsx` 300-331 renders Delete and the tip as siblings in `.save-actions`, the
    positioned box (`fe/src/index.css` about 2417), not in a `.hint-box`. At 320 px Delete is alone on
    the first line and the tip lands 52 px under it (dx -46), below Discard and Save; at 200 % 145 px
    under it. Fix: wrap Delete and its tip in a `.hint-box`, as `RowDelete.tsx` 155 does (and move the
    `.save-actions > [aria-expanded]:first-child` gap to that box).
- Cleanup: `.hint-box { position: relative }` is declared twice (`fe/src/index.css` 2134 and 2143).

### 22D4 D1 decision: the reason in the flow (designer, 2026-11-24)

Choice: the reason in the normal flow, no top layer. It cannot be clipped by the table's scroll box,
leaves no gap for the pointer to cross, and follows the bar when it wraps, with no script placement.
On the pages, where there is one Delete, the reason is always shown while Delete cannot act, so it is
not content on hover at all. On the lists, where most rows can be switched on, it shows on hover and
focus only, so the list is not filled with the same sentence on every row. These criteria replace the
tip parts of 22 D4.3 and 22 D4.10.

**22 D4.13 The list's reason.** While a row's − is disabled and hovered or focused, a
`span.hint-tip` with `aria-hidden="true"` and the reason's text is the next sibling of the − inside
the same `span.hint-box`, in the same cell. The hint box, not the −, owns the pointer handlers: the
reason goes on `mouseleave` of the hint box, on blur of the −, and on Escape. Moving the pointer from
the − onto the reason keeps it ([1.4.13](https://www.w3.org/TR/WCAG22/#content-on-hover-or-focus)).
The −'s `aria-describedby` still points to the hidden `${id}-reason` span (22 D4.2).

**22 D4.14 The page's reason.** While the Save bar's Delete cannot act, a `p` with id
`<delete-id>-reason` (`model-profile-delete-reason`, `server-profile-delete-reason`) and the reason's
text (`model-profiles-delete-needs-off`, `server-profiles-delete-in-use`) is shown in the bar, after
its buttons, where the confirm opens (22 D4.11). It is shown all the time, not on hover or focus,
and Delete's `aria-describedby` names it, so it is read once and seen without pointing. Delete has
no `.hint-tip`. When Delete can act the `p` is absent.

**22 D4.15 Layout (the designer checks it in the three browsers; jsdom does not apply CSS).**
`.hint-tip` is in the flow (no `position: absolute`, no outer margin between it and the −; its room
is its own padding). In a list the hint box is a column inside the status cell, so the reason sits
under the − and wraps within the cell's width; showing it makes the row taller and moves nothing above
it. At 320 px and 200 % text no part of the reason is cut off, and the page does not scroll sideways
([1.4.10](https://www.w3.org/TR/WCAG22/#reflow)).

Tests to change: `fe/src/ModelProfilesDelete1.test.tsx` and `fe/src/ServerProfilesDelete1.test.tsx`
(22 D4.13: the hint box's `mouseleave` hides the reason; `mouseover` of the reason keeps it; the
`mouseout` of the − alone does not hide it), and `fe/src/ModelProfileDelete3.test.tsx` and
`fe/src/ServerProfilePage3.test.tsx` (22 D4.14: the always-shown `p#...-reason` replaces the hover and
focus tip assertions).

### 22D4 D1 decision review (designer, 2026-11-24)

Driven in Chromium, Firefox and WebKit at 1280 and 320 px, light, dark, 200 % text and (Chromium)
forced colours (`build/ux-review/22d4t.js`).

Met:
- 22 D4.13: the reason is in the flow, directly under the − (dx 0, dy 0). It stays while the pointer
  moves onto it, goes when the pointer leaves the box, and Escape hides it.
- 22 D4.14: the used server page's Delete is `aria-disabled` and described by
  `server-profile-delete-reason`, a `p` in the bar under its buttons, always shown, with no tip. A
  free profile's page has no such `p`.
- D2: the disabled − is `--muted` (GrayText under forced colours). D4: the region's alert is cleared
  when the next confirm opens.
- 1280 px in every engine: the reason fits inside the table's box.

Not met:
- **D5 (22 D4.15, [1.4.10](https://www.w3.org/TR/WCAG22/#reflow)): the list reason does not wrap
  at 320 px or 200 %.** The status cell of the Models list (`fe/src/index.css` 795-797,
  `.model-profiles td:last-child`) and the Used by cell of the server list (816-819) are
  `white-space: nowrap`, and `.hint-tip` (2143-2150) inherits it. The reason stays on one line up to
  16rem, wider than the cell: it runs past the table's box, and the page scrolls sideways (311 px on
  the server list at 320 px, 809 px at 200 %; 189 px on the Models list at 200 %). Fix:
  `white-space: normal` on `.hint-tip`, as `.confirm-row td` does for the question.

### 22D4 D5 re-check (designer, 2026-11-24)

Checked in Chromium, Firefox and WebKit at 320 px and at 200 % text (`build/ux-review/22d5.js`,
`22d5p.js`).
- D5 met: the reason now wraps inside its cell, directly under the −, on both lists.
- **D6 (22 D4.8, [1.4.10](https://www.w3.org/TR/WCAG22/#reflow)): the page scrolls sideways
  whether or not a reason shows.** On the server list (`pool=long`) the page scrolls 208 px at
  320 px; on the Models list it scrolls 29 px at 200 %. `window.scrollTo(500, 0)` moves it, so the
  scroll is real. The cause is each row −'s visually hidden name: `.visually-hidden` is
  `position: absolute` (`fe/src/index.css` 332-333), and `.table-scroll` (714) is not positioned, so
  those spans are not clipped by the scroll box. They sit where the − sits in the wide table (right
  edge at 528 px) and widen the document. Fix: `position: relative` on `.table-scroll`.

### 22D4 D6 re-check (designer, 2026-11-24)

Checked in Chromium, Firefox and WebKit at 320 px and at 200 % text (`build/ux-review/22d5.js`,
`22d6p.js`).
- D6 met: no sideways scroll on either list, reason shown or not (document scroll 0 everywhere). On
  the server list at 200 % the reason sits inside the table's own sideways scroll, with the long
  rows. That is the table's scroll box doing its job, not the page.
- **D7 (22 D4.15: "moves nothing above it"): on the Models list at 320 px, showing the reason
  moves the − itself.** It moves 13 px left in Chromium, and 27 px left and 17 px up in WebKit, so
  the pointer can lose the −, and the reason comes and goes (Chromium: moving onto the reason
  hid it). The reason's narrowest width (about 74 px, its longest word in the `.hint-box` column,
  `fe/src/index.css` 2130-2150) is wider than the −, so the status column grows and the table
  shares its width out again. The server list does not move (its Used by column is already wider).
  Requirement: showing the reason changes no column's width. Candidate fix, for the coder to try:
  `contain: inline-size` on `.hint-tip`, so its words no longer size the cell, with the tip as wide
  as the cell's free room. Then check that the − stays put in the three engines.

Full UX check: held until D7 is fixed, as the slice's last pass.

### 22D4 D7 re-check (designer, 2026-11-25)

Checked in Chromium, Firefox and WebKit at 320 px (`build/ux-review/22d6p.js`).
- The − no longer moves (dx 0, dy 0 on both lists).
- **D8 (22 D4.13, 22 D4.15): `contain: inline-size` collapses the reason.** In all three engines the
  tip is 18 px wide, which is its padding alone. Its text breaks one letter per line, 590 px tall on
  the Models list and 379 px on the server list, and the pointer cannot stay on it. With inline-size
  containment the box has no content width of its own, so the flex column (`.hint-box`) gives it
  none. Fix: drop `contain: inline-size`. Instead give `.hint-box` a `min-width` that holds the
  reason's longest word plus the tip's padding (7rem fits "instance" and "Switch" at any text size,
  since rem follows the text size). The column is then as wide with the reason shown as hidden, so
  the − stays put, and the tip wraps inside that width or the cell's, whichever is wider.

Full UX check: still held for the last pass after D8.

D8 fix, measured on 2026-11-25 by injecting it into the built page (`build/ux-review/22d8.js`) in
Chromium, Firefox and WebKit at 320 px and at 200 % text, on both lists:
`.hint-tip { contain: none; }` (that is, drop `contain: inline-size`) and
`.hint-box { min-width: 7rem; }`. The − stays put (0, 0). The reason is 112 px wide over 2 lines on
the Models list (224 px at 200 %) and 117 px over 7 lines on the server list (235 px at 200 %),
with no overflow inside the tip and no sideways scroll. 8rem also holds; 7rem is the smaller one.

### 22D4 last pass (designer, 2026-11-25)

- D8 met as built, in Chromium, Firefox and WebKit at 320 px and 200 % text on both lists (the
  `built` rows of `build/ux-review/22d8b.js`). The − stays put (0, 0). The reason measures
  112 px × 2 lines and 117 px × 7 lines, 224 and 235 px at 200 %, with no overflow and no sideways
  scroll.
- UX check (`npm --prefix fe/ux run check`, load average 14): 20 failing.
  - 16 are one stale state. `server-profile-delete-disabled` (`fe/ux/check.js` 471-473) focuses
    Delete and waits for `main .save-bar .hint-tip`. Under 22 D4.14 the page has no tip: the reason
    is the always-shown `p#server-profile-delete-reason`. Fix: expect that `p` in the bar on load,
    with no focus step.
  - 4 are load timing: `sign-in-at-home` and `model-profile` were not rendered in time. Firefox's
    page also crashed, so its last states did not run. The check needs a clean rerun after the
    state fix.

### 22D4 UX check rerun (designer, 2026-11-25)

- After the `server-profile-delete-disabled` fix: Chromium and WebKit pass all 198 states each. The
  four load-timing failures of the last run did not recur.
- **Firefox does not finish.** Its content process dies on signal 11 (SIGSEGV, from Playwright's
  `pw:browser` log). It crashed three times out of three, at load averages of 7 down to 4, so load
  is not the cause. Narrowed with a copy of the check (`build/ux-review/check-ff3.js`), it crashes
  only on this sequence, at 320 px light: the state `model-profiles-delete` opens a confirm row, then
  `model-profiles-delete-tip` loads the list again, focuses a disabled − (its reason showing), and
  runs axe. Without that open, or without the focus, the same pair passes, and every state passes on
  its own. A plain script that replays the sequence step by step did not crash.
- A page cannot cause a segfault by design, so this is a fault in the engine. The bundled Firefox is
  Playwright's patched build, so stock Firefox may not crash at all. Still, only 22D4's layout
  triggers it (the coder's pass before the D1 decision was green in Firefox).

### 22D4 amendment: the list's reason floats (user, 2026-11-25)

The user tested the deployed lists (Models and server profiles). Hovering a disabled − shows its
reason in the cell's flow (22 D4.13, D4.15), and that makes the row taller and shifts the table. They
want a small floating box over the table instead, on hover and on focus. D4.16 to D4.20 replace
D4.15 and the "next sibling, in the flow" placement of D4.13. The rest of D4.13 stands: the hint
box owns the pointer handlers, and the tip is `aria-hidden` and the −'s next sibling inside the
`span.hint-box`. The pages' Save bar line (D4.14) is unchanged.

Why `position: fixed`, not `absolute` against the cell: `.table-scroll` has `overflow-x: auto`, so
it clips both axes ([CSS Overflow 3](https://www.w3.org/TR/css-overflow-3/#overflow-properties):
`visible` computes to `auto` when the other axis is not `visible`). An absolute box would be cut at
the scroll box's edge, or would add to its scroll area. A fixed box takes the viewport as its
containing block ([CSS Position 3](https://www.w3.org/TR/css-position-3/#fixed-pos)), as long as no
ancestor has a `transform`, `filter` or `contain` (none of the table's ancestors does in `index.css`
today). So it is neither clipped by nor counted in the scroll box, and it adds no page scroll. The
top layer (`popover`) would also work. I chose fixed (my preference) because it needs no API that
jsdom lacks. Unverified: whether jsdom implements `showPopover`.

**22 D4.16 Out of the flow.** `.hint-tip` has `position: fixed` and a `z-index` above the sticky
Save bar and table head (both `z-index: 1`). Showing it changes no layout. These are measured by the
designer in the three browsers, before and after the tip shows (unit: px, tolerance 0.5):
- the −'s row height;
- every column's width (each `th` of the table);
- the −'s own `getBoundingClientRect()`;
- the `.table-scroll`'s `scrollWidth` and `scrollHeight`;
- the document's `scrollWidth - clientWidth`, which stays 0 at 320 px and at 1280 px.

`.hint-box` may drop its `min-width: 7rem`, which existed only to make room for the reason in the
flow. Pin in jsdom: the sheet's `.hint-tip` rule has `position: fixed` and a numeric `z-index` of 2
or more, and the tip is still the −'s next sibling in `.hint-box`.

**22 D4.17 Where it opens and how it flips.** A script sets `top` and `left` inline from the −'s
`getBoundingClientRect()` (`r`), the viewport size (`innerWidth`, `innerHeight`) and the tip's own
size, with an 8 px gutter:
- Default: under the −, touching it (no gap, so the pointer crosses nothing: 1.4.13 hoverable).
  `top = r.bottom`, `left = r.left`.
- Right edge: when `left + tipWidth > innerWidth - 8`, it aligns its right edge to the −'s right
  edge (`left = r.right - tipWidth`), then clamps `left >= 8`.
- Bottom edge: when `r.bottom + tipHeight > innerHeight - 8`, it opens above, touching it
  (`top = r.top - tipHeight`).
- Width: `max-width: min(16rem, 100vw - 16px)`, wrapping, so at 320 px it fits between the gutters.
- While it shows, a scroll anywhere (a capturing `scroll` listener on `window`, which covers
  `.table-scroll`) or a `resize` places it again. It never covers the − itself, so the focused −
  stays in view ([2.4.11](https://www.w3.org/TR/WCAG22/#focus-not-obscured-minimum)).

Pin in jsdom with a stubbed `getBoundingClientRect` and window size:
- a − at the top left gives `top = r.bottom` and `left = r.left`;
- a − near the right edge gives `left = r.right - width`;
- a − near the bottom gives `top = r.top - height`;
- a `scroll` event re-places it.

**22 D4.18 Showing, keeping, dismissing ([1.4.13](https://www.w3.org/TR/WCAG22/#content-on-hover-or-focus)).**
- It shows only while the − cannot act, on `mouseover` and on focus of the −.
- It stays while the pointer is over the − or the tip. The tip is a DOM child of the hint box, so
  `mouseleave` of the box does not fire on the way to it.
- It goes on `mouseleave` of the hint box, on blur, and on Escape. Escape keeps the focus on the −.
- Touch: a tap on the inactive − (its click does nothing else) shows it, and a `pointerdown`
  outside the hint box hides it.
- An active − never shows it.

**22 D4.19 Look.**
- The tip keeps D4.13's look: a `--surface` fill, a `--border-w` solid `--control` border,
  `--radius-check`, and `--text` on `--surface` (contrast already met by the tokens,
  [1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)).
- It adds `box-shadow: var(--shadow)`, so it reads as above the rows it covers.
- Forced colours: `forced-color-adjust` stays `auto`. The shadow drops and the border stays, as
  `CanvasText`, so the box keeps a visible edge. The designer checks this in Chromium forced colours.
- Dark mode works through the tokens.

**22 D4.20 Screen readers, unchanged.** The tip stays `aria-hidden="true"` and is never a live
region. The −'s `aria-describedby` names the hidden `${id}-reason` span (22 D4.2), so the reason is
read once with the button.

Tests to change: `fe/src/ModelProfilesDelete1.test.tsx` and `fe/src/ServerProfilesDelete1.test.tsx`.
- The tests at 406 and 360: "a line under the −, in the same cell" becomes "a fixed box placed by
  D4.17". Escape, touch and pointerdown outside are added.
- The tests at 496 and 449: the "no rule takes it out of the flow" assertion becomes D4.16's
  `position: fixed`.
- The test at 582 (D7, no `contain`) stays.

The pages' tests (`ModelProfileDelete3`, `ServerProfilePage3`, D4.14) stay. Out of scope: the
pages' Delete reason, which stays a line in the bar.

**22 D4.21 The icons: a plus and a trash can, in round outlined buttons (user, 2026-11-25, option B
of `build/ux-review/icons-preview.html`).**

The user found the bare + and − buttons poor. A trash can is the common sign for deleting a stored
thing, and a profile is a file. A bare minus reads as "remove from this list".

Where it applies:
- The row − of the Models list and of the server profile list (`RowDelete.tsx` 170-178) changes
  its icon from `minus` to `trash-2`.
- The server profile list's + (`ServerProfiles.tsx` 461-465) keeps `plus`. The Models list has no
  + today: create has no page.
- The Save bar's Delete on both profile pages is a text button with no icon (`SaveBar.tsx` about
  96), and stays so.
- The round shape goes on the shared `.ot-add` class, so the tensor rule's + (`TensorPlacement.tsx`
  335) and the instance name pencil (`InstancePage.tsx` 845) turn round too. This is on purpose:
  one look for every icon button, and one rule, not a copy (brief 16, 17b2.2).
- `.tag-add` is not `.ot-add`, and is out of scope.

The icon:
- `trash-2` joins `Icon.tsx`'s list, with lucide-static 1.74.0's path data (ISC, the version the
  file already copies). Its five paths, in order:
  `M10 11v6`, `M14 11v6`, `M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6`, `M3 6h18`,
  `M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2`.
  Source: https://cdn.jsdelivr.net/npm/lucide-static@1.74.0/icons/trash-2.svg
- `minus` leaves the list once nothing uses it.
- The svg keeps `Icon.tsx`'s attributes: 24 grid, stroke 2, round caps and joins, `currentColor`,
  `aria-hidden`.

The button:
- Box: 40 x 40 px (`width: 40px; min-height: 40px`, as now) with `border-radius:
  var(--radius-pill)`, which replaces the button default's `--radius`.
- Edge and fill as now: a `--border-w` solid `--control` edge on `--bg`, the icon `--text` at the
  `.icon` size (1.25em), centred. The target is 40 px, above
  [2.5.8](https://www.w3.org/TR/WCAG22/#target-size-minimum)'s 24 px.
- The row delete carries a second class, `ot-delete`, beside `ot-add`.

Hover, only while the button can act:
- every `.ot-add` takes a `--surface` fill;
- `.ot-add.ot-delete` also takes `--danger` for its icon and its edge;
- an `.inactive` button changes nothing on hover.
  This is my preference: colour on hover only, so the table is not a column of red.

Inactive: as now (`.ot-add.inactive`): the icon is `--muted`, `cursor: default`, and the tip of
D4.16 to D4.20 shows.

Focus: the shared focus ring, unchanged; the outline follows the round edge
([2.4.7](https://www.w3.org/TR/WCAG22/#focus-visible)).

Dark: through the tokens. `--danger` in dark is `#ff9a8a` on `--bg` `#231d19`. Non-text contrast
of the icon and the edge against `--bg` is met by `--text` and `--control` as today
([1.4.11](https://www.w3.org/TR/WCAG22/#non-text-contrast)).

Forced colours: the existing block stands (`ButtonText` for the icon, `GrayText` for an inactive
one and its edge). The `--danger` hover is replaced by the system colours, so a hover there shows
no red. That is acceptable: the icon and the name say "delete". The designer checks in Chromium
that the round edge stays visible.

Accessible names: unchanged ("Delete model profile x", "Delete server profile x", the add's hidden
text), the same keys.

The designer measures in the three browsers:
- the button is 40 x 40 and round;
- the row height of the Models list and of the server profile list is unchanged by the new icon,
  measured before and after the change (tolerance 0.5 px);
- no sideways scroll at 320 px and 200 % text.

Tests to amend:
- `fe/src/ModelProfilesDelete1.test.tsx` and `fe/src/ServerProfilesDelete1.test.tsx`: their
  `MINUS = 'M5 12h14'` becomes the trash's paths, and the row delete carries `ot-delete`.
- `fe/src/ServerProfiles2.test.tsx` 201: the + still has `M5 12h14` and `M12 5v14`, so it stays.
  Any assertion there that a row delete is a minus changes.
- `fe/src/ServerProfiles3.test.tsx` about 650: its comment about the minus's `M5 12h14` changes.
  The `M12 5v14` selector of `fe/ux/check.js` 433 and 438 still matches only the +, so it stays.
- New pins in the sheet: `.ot-add` has `border-radius: var(--radius-pill)`; there is an
  `.ot-add:not(.inactive):hover` rule with `--surface`; and an `.ot-add.ot-delete:not(.inactive):hover`
  rule with `--danger` for `color` and `border-color`.
- `fe/src/TensorPlacement*.test.tsx` and `fe/src/InstanceNameEdit17f2.test.tsx` only if they pin
  `.ot-add`'s radius (none found by grep).

**22 D4.22 The refusal of a switched-on delete (2026-11-25).** The page has its own switch since
brief 24 A2, so "switch it off in the list" is wrong there.
- `api-model-profile-enabled` (`RowDelete.tsx` 49, and the page's delete) becomes:
  `{0} is switched on, so it cannot be deleted. Switch it off first.`
- It is the same text on the list and on the page. The list row's switch sits beside its trash
  can, and the page's switch is in the state line, so "first" is enough in both places
  ([3.3.3](https://www.w3.org/TR/WCAG22/#error-suggestion): the refusal says what to do).
- Tests to amend: the old text appears only in `fe/src/test/bff.ts` (grep, 2026-11-25), besides
  the strings file. Tests that read the text through `t()` need no change.

## 22P check against the current UI (designer, 2026-11-25)

I checked 22P against the served routes (`fe/briefs/api-contract.md` 228-245 and 735-755,
`crates/bff/README.md` 410-428 and 1191-1199) and the UI as it stands: 22D4 and its amendment
(D4.16 to D4.21), A2's switch on the page (brief 24), and the round `.ot-add` buttons.

Still true:
- P.1 to P.4, P.6 to P.8, P.10 and P.11.
- P.3's combobox is still `fe/DESIGN.md`'s tag combobox (Chips, the list in the flow).
- 22 C.1's + after the table is the list's `.ot-add`, round since D4.21, with no change to its text.
- Nothing in 22P touches the delete, the switch or the Save bar. On the page, the picker lives in the
  Assets card (22E), below A2's state line, and the switch's dirty rule (23 A2.3) covers a picker
  edit like any other edit.

What changed, and the criteria replaced:

**22 P.5a The options (replaces P.5's first sentence).** `category` is the first subfolder's name
under `dir`, any text, or `null`. It is not the design's four values, so it cannot pick the main
models. Each option comes from a row whose file name (the part of `path` after its last `/` or `\`)
contains none of `mmproj`, `imatrix`, `mtp-`, `eagle3-`, `dflash-` and `dspark-`, compared with
letter case kept. This is llama.cpp's own rule for a main model file (`gguf_filename_is_model` in
`common/download.cpp`), which pai-core copies as `NOT_A_MODEL` (`crates/core/src/hf.rs` 69-71,
990). Of those rows, the options are the ones whose `path` relative to `dir` contains the typed
text, letter case aside, in the reply's order, at most 50. The rest of P.5 stands.
`model-files-count`, `model-files-more` and `model-files-empty` count these rows only. A folder
holding only projector or draft files reads as empty.

**22 P.6a Truncated (changes one text).** `truncated` is true when the walk kept 5 000 rows or
visited 50 000 entries. The old text promised "the first 5000", which is wrong in the second case.
`model-files-truncated` becomes:
`The models folder holds more than the list can read, so some files are missing from it. Move the files no profile uses out of it.`

**22 P.9a A repo with no GGUF.** The bff answers it 502 `hf-bad-answer`, never a 200 with no files.
- `hf-quants-empty` leaves the strings list.
- A 200 with an empty `files` (not expected) reads as `api-hf-bad-answer`.
- `api-hf-bad-answer` becomes:
  `Hugging Face's answer lists no GGUF file llama.cpp can load: the repo may hold none, or the answer could not be read. Check the repo's name, or try again later.`

**22 P.10a Arguments.** `hf-busy`, `hf-bad-answer`, `hf-tool-missing` and `hf-unreachable` carry no
`args`, so their texts take none, as the 22P table already has. `hf-repo-invalid`,
`hf-repo-unavailable` and `hf-repo-moved` carry `[repo]`.

**Run order.** 22P has no user until 22C. Its tester mounts `ModelSource` alone in its tests,
stubbing both routes with the served shapes above. 22C follows as its own run (section 0's order).

Already stale for 22C and 22E, not 22P (recorded here so their runs do not pick it up):
- Decision 7 and the 22D text table are superseded by 22D4 (Delete on the rows and in the Save bar)
  and A2 (the page's own switch). `model-profile-delete-needs-off` is gone from the strings file.
  `api-model-profile-enabled` (used by `RowDelete.tsx` 49 and the page's delete) still says
  "Switch it off in the list of model profiles first", which is wrong on the page, where the switch
  is right there. Its new text is 22 D4.22, built with D4.16 to D4.21.
- Section 0's "The page today" now also has A2's state line between the summary and the settings.
  The model file is still a read-only row of the Assets card (`ModelProfilePage.tsx` `assetRows`,
  223).

The tester pins these first, in this order:
1. P.1 and P.2: the shape and the value.
2. P.3 and P.5a: the combobox and its options, with the file-name rule.
3. P.6 with P.6a: the state line.
4. P.8 and P.9: Show quants and the quants.
5. P.10 with P.9a and P.10a: the refusals.
6. Then P.4, P.7, P.11 and P.12.

### 22D4 amendment review (designer, 2026-11-25, partial)

Measured in Chromium with `build/ux-review/d4amend.js`. The Firefox and WebKit runs were stopped
before they finished.

- D4.16 met. When the tip shows, the row height, the `th` widths, the −'s position, the scroll
  box's `scrollWidth` and `scrollHeight`, and the page's sideways scroll are all unchanged: in both
  lists, at 1280 and 320 px, light, dark and forced colours.
- D4.18 met at 1280 and 320 px:
  - hover shows the tip, and moving onto it keeps it;
  - leaving the box hides it;
  - focus shows it, and Escape hides it with the focus kept on the button.
  - Touch was not measured: the tap timed out in the script.
- D4.19 to D4.22 met for the look and the icons:
  - the tip has a `--surface` fill and a `--control` border, which turns black in forced colours;
  - every `.ot-add` button is 40 x 40 and round: the server profile +, the row trash can (5 paths),
    the tensor rule + and the pencil;
  - the delete turns `--danger` on hover and an inactive one does not change.
- D4.17 not met near the edges at 320 px (D1). `place()` (`fe/src/Hint.tsx` 97-104) reads the tip's
  `offsetWidth` and `offsetHeight` before giving it a position. The tip is then measured where it
  sits in the flow, narrowed by the space left to the viewport's right edge.
  - The models list at the bottom edge: the tip opens above with a 27 px gap to the − (the height
    measured while narrow), which breaks "touching" (1.4.13 hoverable).
  - The server profile list at the right edge: the tip is squeezed to 104 px wide and 273 px tall,
    its right edge on the viewport edge, with no 8 px gutter.
  - Fix: set `left: 0; top: 0` before measuring, either in `place()` or as `.hint-tip { top: 0;
    left: 0; }` in the sheet. Not yet measured as a candidate: `build/ux-review/d4fix.js` is written
    for it.

**D1 fix measured (Chromium, `build/ux-review/d4fix.js`, 2026-11-25).** The candidate is `top: 0;
left: 0` set on the sheet's own `.hint-tip` rule, at its real place (CSSOM `setProperty`), at 320 and
1280 px with the viewport 500 px tall, in both lists, at the right and bottom edges.
- Built, at 320 px:
  - models list, bottom edge: a 26 px gap above the − (tip 414, − 440);
  - server profile list: the tip is 100 x 273 px, its right edge at 320 (no gutter), with gaps of
    78 px and more.
- With the fix, every case touches the − (above or below), aligned to its end at 320 px and to its
  start at 1280 px. The sizes are 197 x 36 (models) and 256 x 89 (servers), inside the 8 px
  gutters, with no sideways scroll. 1280 px was already right.
- The form the coder should use: the reset in `place()`, setting `tip.style.top` and
  `tip.style.left` to `0px` before reading `offsetWidth` and `offsetHeight`. The sheet form fixes
  only the first placement, while inline styles are absent. A re-placement on scroll or resize
  would still measure the tip at its previous place. The two are identical for the first placement,
  which is what was measured.
- Same round (from the reviewer):
  - Escape hides a hover-shown tip even when the focus is elsewhere (a document `keydown` listener
    while the tip shows);
  - after a flip above, `top` is clamped to the 8 px gutter.

**D1 re-measured on the built code (Chromium, 2026-11-25).** At 320 and 1280 px, in both lists, at
the right and bottom edges, the tip touches the − (below, or above after a flip). It is aligned to
the −'s end at 320 px and its start at 1280 px, 197 x 36 or 256 x 89 inside the 8 px gutters, with
no sideways scroll. D1 met. Firefox, WebKit, touch and the full UX check are still to run.

### 22D4 amendment full pass (designer, 2026-11-25)

Built code; `build/ux-review/d4amend.js`, `dbg.js` and `dbg2.js` in Chromium, Firefox and WebKit.
- D4.16 met in all three browsers. When the tip shows, the row height, the column widths, the −'s
  position, the scroll box's size and the page's sideways scroll are unchanged: in both lists, at
  1280 and 320 px, light, dark (and Chromium forced colours), and at the right and bottom edges.
- D4.17 met. The tip touches the − below it, or above it after a flip, aligned to its start (1280 px)
  or its end (320 px), inside the gutters.
- D4.18 met.
  - Hover shows the tip. Moving the pointer straight down onto it keeps it. (A diagonal toward the
    centre of an end-aligned tip crosses the status cell first, a path the criterion does not ask
    for.) Leaving hides it.
  - Focus shows it, and Escape hides it with the focus kept on the −.
  - Touch in WebKit: a tap shows it, and a tap elsewhere hides it.
  - Chromium's touch emulation sends the mouse back to its earlier point after the tap (a
    `mouseleave`), so the tip closes there. This is my reading of the event log, not checked on a
    real Android phone.
- D4.19 to D4.22 met in all three browsers. The server profile +, the trash can (5 paths), the
  tensor rule + and the pencil are all 40 x 40 and round. An active delete turns `--danger` on hover
  (`#b3261e`, dark `#ff9a8a`); an inactive one stays `--muted`.
- UX check: 5 failing on the first run.
  - `sign-in-at-home` failed in all three browsers and `overview-tiles` in WebKit, as "not
    rendered", with the machine's load average near 10. Both pass when rerun alone in all three
    browsers (filtered copy `build/ux-review/check-only.js`).
  - Firefox has its known 22D4 page crash.
  - No state of the amendment failed.

22D4 amendment accepted.
