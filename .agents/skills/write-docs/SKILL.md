---
name: write-docs
description: Create, rewrite or update a guide in docs/ (a card or model page, or a change from a new benchmark campaign) so it reads as one complete review, not a log of additions. Use when asked to write, document, review or simplify documentation, to add measured findings to a guide, or to turn benchmark results into a page.
---

# Writing the guides: one complete review

A guide answers one question: how do I run this card or this model for speed today? The reader
has not followed the work and does not care in what order it happened. Rules a program can check
are in `docs/AGENTS.md`; this skill is how to make the page worth reading.

## 0. Read first

- `docs/AGENTS.md`: page types, required sections, table columns, charts, citations.
- The whole page you will change, not only the section. Then query what it should say:
  `llama-cpp-config bench query campaigns --db benchmarks/bench.db`, `... query run <id>`.
- `docs/templates/<type>.md` for a new page.

## 1. A finding is merged, never appended

Each new result changes the review. Do not add a section for it, or a dated paragraph at the end.

1. Find the topic it belongs to (drafters, KV cache, split, Windows). A new section only for a
   topic the page does not have.
2. Rewrite that topic so it reads as if written today with everything known. Update "In short"
   and the Settings row if the recommendation moved, and delete what the result made irrelevant.
3. Reorder or merge sections if the page reads better that way, and update citations to them
   (`grep -rnE "(qwen3|amd_ai|nvidia_rtx).*section" .`, excluding the frozen `benchmarks/prompts/`).

Test: read the page top to bottom. If you can tell the order the work happened in, it is still
layered. Rewrite it.

## 2. The story

- **Open with the answer.** Under the title: what it is, the setups it covers, then a short
  "**In short.**" paragraph with what to do and the size of the gain.
- **Conclusion first, in each section**: a bold sentence saying what to do, then the table that
  proves it, then why. Mechanism (profiles, kernels, patches) only where it explains a choice, in
  plain words; fold the machine and method detail in `<details>`.
- **Organize by what the reader looks for**, not by machine history or by campaign.
- **Group the settings by setup** (`### Any setup`, `### Two R9700s on Linux`), not by date.

## 3. What to leave out

- **The past.** No "was", "now", "no longer", "since", "before the patch", "(was 7)", dates of the
  work, or superseded values. State what holds today. A release tag belongs only where it scopes a
  figure.
- **Failed tests.** No legs that did not load, ran out of memory or were cancelled, unless they set
  a limit the reader must respect. Then state the limit ("23 blocks is this card's limit"), not the
  failure log.
- **Noise.** Run ids, commit hashes, log excerpts, lists of every value tried when one comparison
  makes the point. `#12345` becomes a worded link to what it is.

## 4. Merge results across campaigns

If one benchmark lacks a figure that another run has, on the same preset and machine, merge it
into the same table rather than leaving a hole or splitting the table. Mark the cell, `937.5 (a)`,
and say under the table where it comes from and how far it compares: repetitions, split, sampling.
Cite both campaigns. Never invent a figure, and never merge across a change of machine, build or
spec without saying so. For a spec's versions, merge the legs a later version added with those it kept
unchanged, and name which campaign holds which.

## 5. Tables

- A table compares; prose explains. A settings evidence cell holds one headline comparison, under
  about 200 characters; anything longer goes under the table.
- Few columns. Split a table when many cells would be empty; write `n/a` where one remains.
- Say gains as throughput in words: "21% faster", "prefill 3% slower". Give the two figures
  compared, the better one first. Bold the chosen row.
- A raw `|` in a table cell breaks the row: reword, or escape it as `\|`.
- A `bench-chart` follows the table holding its numbers. Check that its bars match the table: a run
  holding several prompts pools them.

## 6. References

- A section reference is a link: `[section 4](#4-linux-against-windows)`. Use few; a page that
  needs many is badly ordered.
- Another page by name, as a link. An upstream PR as a worded link.
- A figure from bench.db cites "(campaign `name`)", singular, once per campaign: only that form
  is linked by the site. Hand-read or server-log figures say so ("read by hand, not recorded").

## 7. Placeholders

Audit "Not yet measured (placeholders)" against bench.db and the code, not against the page's
memory. Answered: move it to the settings or the choices. Partly answered: `partly: ...`. Needs
missing hardware: `TBD: needs ...`. No longer applicable: delete it. Current values must match the
live preset.

## 8. Check before you finish

1. `cargo run -p docs-site -- --check`, then build the site into the scratchpad with `--out <dir>`.
2. Every internal `#` link resolves to a generated heading id.
3. Every number you wrote traces to bench.db or the previous page. Recompute means and percentages.
4. Compare cited campaigns against the previous version; each one dropped is a deliberate choice.
5. No em-dashes, no absolute paths, front matter `updated` set.
6. Read the page once as a new reader. If a sentence needs the history to make sense, rewrite it.

## 9. Improving this skill

When a page's review teaches something about writing these guides, fold it in here, in the same
change.
