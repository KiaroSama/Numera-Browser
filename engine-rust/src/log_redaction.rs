use regex::{Captures, Regex};
use std::sync::OnceLock;

struct Rules {
    patterns: [Regex; 4],
    word: Regex,
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| {
        // Python's whitespace includes four separators beyond Unicode White_Space.
        let space = r"[\s\x1c-\x1f]";
        let value = r#"[^\s\x1c-\x1f,;"'})\]]+"#;
        Rules {
            patterns: [
                format!(r"(?P<pre>(?P<key>(?i:(?:proxy-)?author[iİı]zat[iİı]on)){space}*[=:]{space}*)[^\s\x1c-\x1f].*"),
                format!(r"(?P<pre>(?P<key>(?i:bearer|bas[iİı]c)){space}+)[A-Za-zİıſK0-9._~+/=-]{{8,}}"),
                r"(?P<pre>://)[^/\s\x1c-\x1f:@]+:[^/\s\x1c-\x1f@]+(?P<post>@)".into(),
                format!(r#"(?P<pre>["']?(?P<key>(?i:pass|password|passwd|pwd|token|secret|ap[iİı][_-]?key))["']?{space}*[=:]{space}*)(?:(?P<dq>"){value}"|(?P<sq>'){value}'|{value})"#),
            ]
            .map(|pattern| Regex::new(&pattern).expect("Fixed log redaction expression is valid")),
            // The frozen Python 3.13 oracle uses Unicode 15.1 isalnum + underscore,
            // not regex's Alphabetic/Mark/Join_Control word definition.
            word: Regex::new(r"\A[[\p{L}\p{N}]&&\p{Age:15.1}]\z")
                .expect("Frozen log word expression is valid"),
        }
    })
}

pub(crate) fn redact(text: &str) -> String {
    let rules = rules();
    let mut text = text.to_owned();
    for pattern in &rules.patterns {
        text = replace(pattern, &rules.word, &text);
    }
    text
}

fn replace(pattern: &Regex, word: &Regex, text: &str) -> String {
    let mut output = String::new();
    let (mut search, mut copied) = (0, 0);
    while let Some(captures) = pattern.captures_at(text, search) {
        let matched = captures.get(0).expect("A capture has a complete match");
        if let Some(key) = captures.name("key")
            && text[..key.start()]
                .chars()
                .next_back()
                .is_some_and(|character| {
                    character == '_'
                        || character.is_ascii_alphanumeric()
                        || (!character.is_ascii()
                            && word.is_match(character.encode_utf8(&mut [0; 4])))
                })
        {
            // A rejected outer match can contain a later valid credential key.
            search = matched.start()
                + text[matched.start()..]
                    .chars()
                    .next()
                    .expect("Credential expressions are nonempty")
                    .len_utf8();
            continue;
        }
        output.push_str(&text[copied..matched.start()]);
        append(&captures, &mut output);
        search = matched.end();
        copied = search;
    }
    output.push_str(&text[copied..]);
    output
}

fn append(captures: &Captures<'_>, output: &mut String) {
    let quote = captures
        .name("dq")
        .or_else(|| captures.name("sq"))
        .map_or("", |matched| matched.as_str());
    output.push_str(
        captures
            .name("pre")
            .expect("Credential prefix exists")
            .as_str(),
    );
    output.push_str(quote);
    output.push_str("[REDACTED]");
    output.push_str(quote);
    if let Some(post) = captures.name("post") {
        output.push_str(post.as_str());
    }
}
