//! The scan shared by the two lints that pin "the slot snapshot is saved by
//! whoever stops llama-server": pai-cli's `snapshot_before_stop` (in `cli.rs`)
//! and the GUI's stop worker (`gui.rs` `stop_server_async_with`). Under the
//! manager the instance saves at its own stop, so both must ask
//! `server_control::client_saves_snapshots` and save only when it says yes.
//!
//! A scan of the text, so the functions it reads stay small: the brace
//! counting takes a `{` in a signature (an array-typed parameter) or in a
//! closure inside a condition for the start of a block. Comments are dropped
//! first, so a doc comment naming the decision cannot satisfy a scan;
//! `strip_comments` cuts at the first `//`, so a URL in a string literal would
//! be cut too, and neither function scanned here holds one.

/// `code` with every `//` comment dropped, doc comments included.
pub(crate) fn strip_comments(code: &str) -> String {
    code.lines()
        .map(|line| line.split("//").next().unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The offset just past the `}` matching the `{` that starts `s`, `None` when
/// the braces never close.
fn block_end(s: &str) -> Option<usize> {
    let mut depth = 0_i64;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The body of the top-level `fn <name>` in `code`, without its braces.
pub(crate) fn fn_body(code: &str, name: &str) -> String {
    let head = format!("\nfn {name}(");
    let at = code
        .find(&head)
        .unwrap_or_else(|| panic!("no top-level `fn {name}` in the source"));
    let rest = &code[at + head.len()..];
    let open = rest
        .find('{')
        .unwrap_or_else(|| panic!("no body for `fn {name}`"));
    let body = &rest[open..];
    let end = block_end(body).unwrap_or_else(|| panic!("unbalanced body for `fn {name}`"));
    body[1..end - 1].to_string()
}

/// Whether `c` is an identifier character.
fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The byte offsets where `word` sits in `s` as a whole identifier, with no
/// identifier character on either side. A name inside a longer name (the `on`
/// of `on_the_fly`) and a name inside a word (the `if` of `verify`) are not
/// occurrences: a scan that matches them reads an unrelated word as the name
/// it is looking for. A needle ending at punctuation (the call `saves(`)
/// already ends an identifier, so only its left side is bounded.
fn identifier_at(s: &str, word: &str) -> Vec<usize> {
    let check_right = word.chars().next_back().is_some_and(is_ident_char);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = s[from..].find(word) {
        let at = from + k;
        let left = s[..at].chars().next_back();
        let right = s[at + word.len()..].chars().next();
        let whole =
            !left.is_some_and(is_ident_char) && !(check_right && right.is_some_and(is_ident_char));
        if whole {
            out.push(at);
        }
        from = at + word.len();
    }
    out
}

/// The text before the call at `at` in `s`, up to and including the first
/// character that is not part of a path (spaces aside, and of a qualified
/// call's `crate::server_control::`), so what governs the call is the last
/// thing left.
fn before_call(s: &str, at: usize) -> &str {
    let head = s[..at].trim_end();
    match head
        .char_indices()
        .rev()
        .find(|(_, c)| *c != ':' && !is_ident_char(*c))
    {
        Some((i, c)) => &head[..i + c.len_utf8()],
        None => head,
    }
}

/// Whether the token before `at` in `s` is a `!`, so the call at `at` is
/// tested negated.
fn negated_before(s: &str, at: usize) -> bool {
    before_call(s, at).trim_end().ends_with('!')
}

/// Whether `s` ends with `word` as a whole word.
fn ends_with_word(s: &str, word: &str) -> bool {
    let Some(head) = s.strip_suffix(word) else {
        return false;
    };
    !head.chars().next_back().is_some_and(is_ident_char)
}

/// The name a `let` binds the value of the call at `at` to, `None` when the
/// call is not the value of a `let` (its answer is tested where it is asked).
/// The call may be qualified by a path.
fn bound_to(body: &str, at: usize) -> Option<String> {
    let head = before_call(body, at).trim_end().strip_suffix('=')?;
    let head = head.trim_end();
    let keep = head
        .char_indices()
        .rev()
        .find(|(_, c)| !is_ident_char(*c))
        .map(|(i, c)| (i + c.len_utf8(), i));
    let (rest, name) = match keep {
        Some((from, to)) => (head[..to].trim_end(), &head[from..]),
        None => ("", head),
    };
    if name.is_empty() {
        return None;
    }
    ends_with_word(rest, "let").then_some(name.to_string())
}

/// Every `if` of `body` as `(condition, block, block_start)`: the text from
/// `if` to the `{` at paren depth 0 that opens its block, that `{` to its
/// matching `}`, and the offset the block starts at in `body`. An `if` whose
/// condition reaches no `{`, or whose block never closes, is skipped.
fn if_conditions(body: &str) -> Vec<(String, String, usize)> {
    let mut out = Vec::new();
    for at in identifier_at(body, "if") {
        let rest = &body[at + 2..];
        let mut depth = 0_i64;
        let mut open = None;
        for (i, c) in rest.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ';' if depth == 0 => break,
                '{' if depth == 0 => {
                    open = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(e) = open else { continue };
        let start = at + 2 + e;
        let Some(end) = block_end(&body[start..]) else {
            continue;
        };
        out.push((
            rest[..e].to_string(),
            body[start..start + end].to_string(),
            start,
        ));
    }
    out
}

/// Whether the call `call` sits behind the decision `gate` makes inside
/// `body`, with the polarity that saves when the decision asks for it. Three
/// shapes are sanctioned:
///
/// - the gate inside the condition of the `if` whose block holds the call,
///   tested as it is (`if gate(..)`), not negated;
/// - a guard clause (`if !gate(..) { return; }`), the call below the block, so
///   the gate is tested negated;
/// - the gate bound to a name (`let on = gate(..)`), and that name, as a whole
///   identifier, in the condition of the `if` whose block holds the call
///   (`if on`, `if on && ..`, `.. && on`), negated only in a guard clause.
///
/// A gate whose answer nothing uses, a gate guarding only some other call, a
/// gate written after the call, and either polarity inverted (a guard that
/// returns when the decision says yes, a block that saves when it says no) are
/// not.
pub(crate) fn call_behind_gate(body: &str, gate: &str, call: &str) -> bool {
    let (Some(g), Some(c)) = (body.find(gate), body.find(call)) else {
        return false;
    };
    if g > c {
        return false;
    }
    // The name the gate's answer is bound to, when it is bound.
    let bound: Vec<String> = identifier_at(body, gate)
        .iter()
        .filter_map(|at| bound_to(body, *at))
        .collect();
    for (cond, block, start) in if_conditions(body) {
        let holds_call = start <= c && c < start + block.len();
        let has_return = block.contains("return");
        // Where this condition tests the gate: the gate called inline, and the
        // name its answer was bound to.
        let mut tests = identifier_at(&cond, gate);
        for name in &bound {
            tests.extend(identifier_at(&cond, name));
        }
        for at in tests {
            // The block that holds the call is entered when the condition is
            // true, so the gate must stand there; a guard clause skips what
            // follows when the condition is true, so the gate must be negated.
            let neg = negated_before(&cond, at);
            if (holds_call && !neg) || (!holds_call && neg && has_return) {
                return true;
            }
        }
    }
    false
}
