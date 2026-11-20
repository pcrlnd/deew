//! Output file names: `-on/--output-name` (a literal name) and
//! `-ot/--output-template` (a template with placeholders, also the config's
//! `[output] template`).
//!
//! The rules are mirrored by `gui/src/lib/output-name.ts`; both are tested with
//! the vectors of `tests/fixtures/output_names.json`.

/// The placeholders of a template, in the order they are listed in errors.
pub const PLACEHOLDERS: [&str; 9] = [
    "stem", "name", "format", "codec", "channels", "bitrate", "lang", "title", "track",
];

/// The extensions of deew's outputs (what `Format::extension` gives).
pub const OUTPUT_EXTENSIONS: [&str; 6] = ["ac3", "ec3", "eb3", "thd", "ac4", "aac"];

/// The template of the config when it has none.
pub const DEFAULT_TEMPLATE: &str = "{stem}";

/// What separates the parts of a name; the clean-up of empty values works
/// on these.
const SEPARATORS: [char; 4] = ['.', ' ', '_', '-'];

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c)
}

/// The values of the placeholders for one output. Empty means unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values {
    /// The input's name without extension and without the delay tag.
    pub stem: String,
    /// The input's name without extension, delay tag kept.
    pub name: String,
    /// The `-f` value (`ddp`, `ddp-atmos`, ...).
    pub format: String,
    /// The label of the output format as the summary shows it.
    pub codec: String,
    /// The channel layout of the output (`2.0`, `5.1`, `7.1`).
    pub channels: String,
    /// The bitrate in kbps, empty for TrueHD.
    pub bitrate: String,
    /// The language tag of the audio track.
    pub lang: String,
    /// The title tag of the audio track.
    pub title: String,
    /// The `-ti` value.
    pub track: String,
}

impl Values {
    fn get(&self, placeholder: &str) -> &str {
        match placeholder {
            "stem" => &self.stem,
            "name" => &self.name,
            "format" => &self.format,
            "codec" => &self.codec,
            "channels" => &self.channels,
            "bitrate" => &self.bitrate,
            "lang" => &self.lang,
            "title" => &self.title,
            "track" => &self.track,
            _ => unreachable!("validated placeholder {placeholder}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    /// A placeholder, one of [`PLACEHOLDERS`].
    Field(&'static str),
}

/// A parsed output name: a template, or a literal name (`-on`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    parts: Vec<Part>,
}

fn valid_list() -> String {
    PLACEHOLDERS
        .iter()
        .map(|p| format!("{{{p}}}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Template {
    /// A name as it is, braces are ordinary characters.
    pub fn literal(text: &str) -> Template {
        Template {
            parts: vec![Part::Text(text.to_string())],
        }
    }

    /// Parse a template. The error is plain text (not markup).
    pub fn parse(text: &str) -> Result<Template, String> {
        if text.contains(['/', '\\']) {
            return Err(
                "the template can't contain a path separator, use -o for the folder".to_string(),
            );
        }
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    let mut field = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some('{') | None => {
                                return Err(format!(
                                    "unbalanced brace in the template, the valid placeholders are {}",
                                    valid_list()
                                ));
                            }
                            Some(other) => field.push(other),
                        }
                    }
                    let Some(known) = PLACEHOLDERS.iter().find(|p| **p == field) else {
                        return Err(format!(
                            "unknown placeholder {{{field}}}, the valid placeholders are {}",
                            valid_list()
                        ));
                    };
                    if !literal.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut literal)));
                    }
                    parts.push(Part::Field(known));
                }
                '}' => {
                    return Err(format!(
                        "unbalanced brace in the template, the valid placeholders are {}",
                        valid_list()
                    ));
                }
                other => literal.push(other),
            }
        }
        if !literal.is_empty() {
            parts.push(Part::Text(literal));
        }
        Ok(Template { parts })
    }

    /// The output extension the text ends with (lowercase), if it is one of
    /// deew's: `Movie.EC3` gives `ec3`. Only text of the template counts,
    /// never a value.
    fn tail_extension(&self) -> Option<String> {
        let Some(Part::Text(text)) = self.parts.last() else {
            return None;
        };
        let (_, ext) = text.rsplit_once('.')?;
        let ext = ext.to_ascii_lowercase();
        OUTPUT_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
    }

    /// The file name for `values` with the extension `ext` (without the dot).
    pub fn render(&self, values: &Values, ext: &str) -> Result<String, String> {
        // the default name stays what it always was, whatever characters the
        // input's name has
        if self.parts == [Part::Field("stem")] && !values.stem.is_empty() {
            return Ok(format!("{}.{ext}", values.stem));
        }
        let name = self.render_base(values);
        if name.is_empty() {
            return Err("the output name is empty".to_string());
        }
        match self.tail_extension() {
            Some(tail) if tail == ext => Ok(name),
            Some(tail) => Err(format!(
                "the name ends with .{tail}, but the extension of this output is .{ext}, leave it out or end the name with .{ext}"
            )),
            None => Ok(format!("{name}.{ext}")),
        }
    }

    /// The name without the extension handling.
    fn render_base(&self, values: &Values) -> String {
        enum Item {
            Text(String),
            Value(String),
            Empty,
        }
        let mut items: Vec<Item> = self
            .parts
            .iter()
            .map(|part| match part {
                Part::Text(text) => Item::Text(sanitize(text)),
                Part::Field(field) => {
                    let value = sanitize(values.get(field));
                    if value.is_empty() {
                        Item::Empty
                    } else {
                        Item::Value(value)
                    }
                }
            })
            .collect();
        let any_empty = items.iter().any(|i| matches!(i, Item::Empty));
        // an empty value drops out with one adjacent separator: the
        // separators on both sides of it become the one on its left (the one
        // on its right if there is none)
        let mut done: Vec<Item> = Vec::new();
        for index in 0..items.len() {
            match std::mem::replace(&mut items[index], Item::Empty) {
                Item::Empty => {
                    let mut left = String::new();
                    if let Some(Item::Text(text)) = done.last_mut() {
                        let kept = text.trim_end_matches(is_separator).len();
                        left = text.split_off(kept);
                    }
                    let mut right = String::new();
                    if let Some(Item::Text(text)) = items.get_mut(index + 1) {
                        let cut = text.len() - text.trim_start_matches(is_separator).len();
                        right = text[..cut].to_string();
                        text.drain(..cut);
                    }
                    let merged = if left.is_empty() { right } else { left };
                    done.push(Item::Text(merged));
                }
                Item::Text(text) if text.is_empty() => {}
                other => done.push(other),
            }
        }
        if any_empty {
            // separators of the template at the ends of the name
            for item in &mut done {
                match item {
                    Item::Text(text) => {
                        let trimmed = text.trim_start_matches(is_separator).to_string();
                        *text = trimmed;
                        if !text.is_empty() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            for item in done.iter_mut().rev() {
                match item {
                    Item::Text(text) => {
                        let trimmed = text.trim_end_matches(is_separator).to_string();
                        *text = trimmed;
                        if !text.is_empty() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        }
        let name: String = done
            .into_iter()
            .map(|item| match item {
                Item::Text(text) | Item::Value(text) => text,
                Item::Empty => String::new(),
            })
            .collect();
        name.trim_end_matches(['.', ' ']).to_string()
    }
}

/// Characters that are illegal on Windows (`<>:"/\|?*`, control characters)
/// become `_`, on every platform.
pub fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Whether two output paths are the same file. Case-insensitive on every
/// platform: two outputs that differ only in case overwrite each other on
/// Windows and macOS, and are certainly not wanted on Linux either.
pub fn same_path(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn values(case: &Value) -> Values {
        let get = |key: &str| case["values"][key].as_str().unwrap_or_default().to_string();
        Values {
            stem: get("stem"),
            name: get("name"),
            format: get("format"),
            codec: get("codec"),
            channels: get("channels"),
            bitrate: get("bitrate"),
            lang: get("lang"),
            title: get("title"),
            track: get("track"),
        }
    }

    /// The vectors shared with the GUI's TypeScript renderer.
    #[test]
    fn shared_vectors() {
        let cases: Value =
            serde_json::from_str(include_str!("../tests/fixtures/output_names.json")).unwrap();
        let cases = cases.as_array().unwrap();
        assert!(cases.len() > 20);
        for case in cases {
            let text = case["text"].as_str().unwrap();
            let ext = case["ext"].as_str().unwrap_or("ec3");
            let parsed = if case["literal"].as_bool() == Some(true) {
                Ok(Template::literal(text))
            } else {
                Template::parse(text)
            };
            let result = parsed.and_then(|t| t.render(&values(case), ext));
            match (case["expected"].as_str(), case["error"].as_str()) {
                (Some(expected), None) => {
                    assert_eq!(result.as_deref(), Ok(expected), "{text:?}");
                }
                (None, Some(error)) => {
                    let message = result.expect_err(text);
                    assert!(message.contains(error), "{text:?}: {message}");
                }
                _ => panic!("bad vector {case}"),
            }
        }
    }

    fn render(text: &str, edit: impl FnOnce(&mut Values)) -> Result<String, String> {
        let mut v = Values {
            stem: "Movie".into(),
            name: "Movie DELAY -5ms".into(),
            format: "ddp".into(),
            codec: "DDP".into(),
            channels: "5.1".into(),
            bitrate: "1024".into(),
            lang: "eng".into(),
            title: "Main".into(),
            track: "0".into(),
        };
        edit(&mut v);
        Template::parse(text)?.render(&v, "ec3")
    }

    #[test]
    fn every_placeholder() {
        let all = "{stem}|{name}|{format}|{codec}|{channels}|{bitrate}|{lang}|{title}|{track}";
        // `|` is illegal, so use spaces
        let text = all.replace('|', " ");
        assert_eq!(
            render(&text, |_| {}).unwrap(),
            "Movie Movie DELAY -5ms ddp DDP 5.1 1024 eng Main 0.ec3"
        );
    }

    #[test]
    fn empty_values_drop_out() {
        let no_lang = |v: &mut Values| v.lang.clear();
        assert_eq!(
            render("{stem}.{lang}.ddp", no_lang).unwrap(),
            "Movie.ddp.ec3"
        );
        assert_eq!(render("{lang} - {stem}", no_lang).unwrap(), "Movie.ec3");
        assert_eq!(
            render("{stem} - {lang} - {title}", no_lang).unwrap(),
            "Movie - Main.ec3"
        );
        assert_eq!(render("{stem}_{lang}", no_lang).unwrap(), "Movie.ec3");
        // nothing is trimmed when nothing is empty
        assert_eq!(render("-{stem}-", |_| {}).unwrap(), "-Movie-.ec3");
        assert!(render("{lang}", no_lang).is_err());
    }

    #[test]
    fn illegal_characters() {
        assert_eq!(
            render("{stem} {lang}", |v| v.stem =
                "a:b/c\\d*e?f\"g<h>i|j\u{1}".into())
            .unwrap(),
            "a_b_c_d_e_f_g_h_i_j_ eng.ec3"
        );
        // only the default template leaves the input's name as it is
        assert_eq!(
            render("{stem}", |v| v.stem = "a<b>".into()).unwrap(),
            "a<b>.ec3"
        );
        assert_eq!(
            render("{stem}: {title}", |_| {}).unwrap(),
            "Movie_ Main.ec3"
        );
        assert_eq!(
            render("{stem}.{title}.", |v| v.title = "x. ".into()).unwrap(),
            "Movie.x.ec3"
        );
    }

    #[test]
    fn extensions() {
        assert_eq!(render("{stem}.ec3", |_| {}).unwrap(), "Movie.ec3");
        assert_eq!(render("{stem}.EC3", |_| {}).unwrap(), "Movie.EC3");
        assert_eq!(
            render("{stem}.commentary", |_| {}).unwrap(),
            "Movie.commentary.ec3"
        );
        let error = render("{stem}.ac3", |_| {}).unwrap_err();
        assert!(error.contains(".ec3"), "{error}");
        // a value never counts as an extension
        assert_eq!(
            render("{stem}", |v| v.stem = "a.ac3".into()).unwrap(),
            "a.ac3.ec3"
        );
        let literal = Template::literal("Movie.thd");
        assert_eq!(
            literal.render(&Values::default(), "thd").unwrap(),
            "Movie.thd"
        );
        assert!(literal.render(&Values::default(), "ec3").is_err());
    }

    #[test]
    fn invalid_templates() {
        for bad in [
            "{stem",
            "stem}",
            "{unknown}",
            "{}",
            "{{stem}}",
            "a/{stem}",
            "a\\b",
        ] {
            assert!(Template::parse(bad).is_err(), "{bad}");
        }
        let error = Template::parse("{nope}").unwrap_err();
        assert!(
            error.contains("{stem}") && error.contains("{track}"),
            "{error}"
        );
        assert!(Template::parse("{stem}").is_ok());
    }

    #[test]
    fn literal_braces() {
        let t = Template::literal("a{b}");
        assert_eq!(t.render(&Values::default(), "ec3").unwrap(), "a{b}.ec3");
    }

    #[test]
    fn paths() {
        assert!(same_path("/a/B.ec3", "/A/b.EC3"));
    }
}
