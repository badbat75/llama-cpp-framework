//! Upstream's per-architecture lists, read at build time out of the llama.cpp
//! clone's `src/llama-arch.cpp`: they are compiled into llama.cpp and no public
//! API exposes them by name, so copying them by hand would go stale at every
//! bump. Shared by build.rs, which writes the result to `OUT_DIR`, and the unit
//! tests below. A source that no longer has the expected shape is an error, so
//! the build stops rather than compile a wrong list.

use std::collections::HashMap;

/// The Rust source of the three tables, from the text of `llama-arch.cpp`.
pub fn render(source: &str) -> Result<String, String> {
    let names = arch_names(source)?;
    let mut out = String::new();
    for (table, function, listed) in [
        ("RECURRENT", "llm_arch_is_recurrent", true),
        ("HYBRID", "llm_arch_is_hybrid", true),
        ("NO_SM_TENSOR", "llm_arch_supports_sm_tensor", false),
    ] {
        let mut archs = Vec::new();
        for case in switch_cases(source, function, listed)? {
            let name = names
                .get(case)
                .ok_or_else(|| format!("{function}: {case} is not in LLM_ARCH_NAMES"))?;
            archs.push(format!("{name:?}"));
        }
        out.push_str(&format!(
            "pub const {table}: &[&str] = &[{}];\n",
            archs.join(", ")
        ));
    }
    Ok(out)
}

/// `LLM_ARCH_NAMES`: enum constant to `general.architecture` spelling.
fn arch_names(source: &str) -> Result<HashMap<&str, &str>, String> {
    let body = source
        .split_once("LLM_ARCH_NAMES = {")
        .and_then(|(_, rest)| rest.split_once("\n};"))
        .ok_or("LLM_ARCH_NAMES not found")?
        .0;
    let names: HashMap<&str, &str> = body
        .lines()
        .filter_map(|line| {
            let (constant, rest) = line.trim().strip_prefix('{')?.split_once(',')?;
            let name = rest.split('"').nth(1)?;
            Some((constant.trim(), name))
        })
        .collect();
    if names.is_empty() {
        return Err("LLM_ARCH_NAMES has no entries".into());
    }
    Ok(names)
}

/// The `case` labels of `bool <function>(const llm_arch & arch)`, whose switch
/// must return `listed` for them and the opposite by default. Anything else in
/// the body is refused: a condition added to one arch would otherwise be lost.
fn switch_cases<'a>(source: &'a str, function: &str, listed: bool) -> Result<Vec<&'a str>, String> {
    let head = format!("bool {function}(const llm_arch & arch) {{");
    let body = source
        .split_once(head.as_str())
        .and_then(|(_, rest)| rest.split_once("\n}"))
        .ok_or_else(|| format!("{function} not found"))?
        .0;
    let lines: Vec<&str> = body
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .collect();
    let cases: Vec<&str> = lines
        .iter()
        .skip(1)
        .map_while(|line| line.strip_prefix("case ")?.strip_suffix(':'))
        .collect();
    let (ret, other) = (format!("return {listed};"), format!("return {};", !listed));
    let expected_tail = [ret.as_str(), "default:", other.as_str(), "}"];
    if lines.first() != Some(&"switch (arch) {")
        || cases.is_empty()
        || lines[1 + cases.len()..] != expected_tail
    {
        return Err(format!("{function} no longer has the expected shape"));
    }
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(sm_tensor_body: &str) -> String {
        format!(
            r#"
static const std::map<llm_arch, const char *> LLM_ARCH_NAMES = {{
    {{ LLM_ARCH_A,    "a-a"  }}, // a comment
    {{ LLM_ARCH_B,    "b_b"  }},
}};
bool llm_arch_is_recurrent(const llm_arch & arch) {{
    switch (arch) {{
        case LLM_ARCH_A:
            return true;
        default:
            return false;
    }}
}}
bool llm_arch_is_hybrid(const llm_arch & arch) {{
    switch (arch) {{
        case LLM_ARCH_A:
        case LLM_ARCH_B: // a comment
            return true;
        default:
            return false;
    }}
}}
bool llm_arch_supports_sm_tensor(const llm_arch & arch) {{
{sm_tensor_body}
}}
"#
        )
    }

    const DENY_B: &str = "    switch (arch) {
        case LLM_ARCH_B:
            return false;
        default:
            return true;
    }";

    #[test]
    fn tables_carry_the_spellings_of_the_names_map() {
        let out = render(&source(DENY_B)).unwrap();
        assert!(out.contains(r#"RECURRENT: &[&str] = &["a-a"];"#), "{out}");
        assert!(
            out.contains(r#"HYBRID: &[&str] = &["a-a", "b_b"];"#),
            "{out}"
        );
        assert!(
            out.contains(r#"NO_SM_TENSOR: &[&str] = &["b_b"];"#),
            "{out}"
        );
    }

    #[test]
    fn a_rewritten_switch_stops_the_build() {
        for (body, why) in [
            (DENY_B.replace("return false;", "return true;"), "polarity"),
            (
                DENY_B.replace("return false;", "return x > 1;"),
                "condition",
            ),
            ("    return arch != LLM_ARCH_B;".to_string(), "no switch"),
            (
                DENY_B.replace("case LLM_ARCH_B:", "case LLM_ARCH_C:"),
                "unknown arch",
            ),
        ] {
            assert!(render(&source(&body)).is_err(), "{why}");
        }
        assert!(render("").is_err());
    }
}
