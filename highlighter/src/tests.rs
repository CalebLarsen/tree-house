use std::borrow::Cow;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use indexmap::{IndexMap, IndexSet};
use once_cell::sync::Lazy;
use once_cell::unsync::OnceCell;
use skidder::Repo;
use tree_sitter::{Grammar, InputEdit, Point};

use crate::config::{LanguageConfig, LanguageLoader};
use crate::fixtures::{check_highlighter_fixture, check_injection_fixture};
use crate::highlighter::Highlight;
use crate::injections_query::InjectionLanguageMarker;
use crate::{Language, Layer, Syntax};

const PARSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

static GRAMMARS: Lazy<Vec<PathBuf>> = Lazy::new(|| {
    let skidder_config = skidder_config();
    skidder::fetch(&skidder_config, false).unwrap();
    skidder::build_all_grammars(&skidder_config, false, None).unwrap();
    let grammars = skidder::list_grammars(&skidder_config).unwrap();
    assert!(!grammars.is_empty());
    grammars
});

fn skidder_config() -> skidder::Config {
    skidder::Config {
        repos: vec![Repo::Local {
            // `./test-grammars` in the root of the repo.
            path: Path::new("../test-grammars").canonicalize().unwrap(),
        }],
        index: PathBuf::new(),
        verbose: true,
    }
}

#[derive(Debug, Clone, Default)]
struct Overwrites {
    highlights: Option<String>,
    locals: Option<String>,
    injections: Option<String>,
}

fn get_grammar(lang_name: &str, overwrites: &Overwrites) -> LanguageConfig {
    let skidder_config = skidder_config();
    let grammar_dir = skidder_config.grammar_dir(lang_name).unwrap();
    let parser_path = skidder::build_grammar(&skidder_config, lang_name, false).unwrap();
    let grammar = unsafe { Grammar::new(lang_name, &parser_path).unwrap() };
    let highlights_query_path = grammar_dir.join("highlights.scm");
    let injections_query_path = grammar_dir.join("injections.scm");
    if !injections_query_path.exists() {
        println!("\x1b[36mskipping loading of injections for {lang_name:?} since {injections_query_path:?} does not exist\x1b[0m");
    }
    let locals_query_path = grammar_dir.join("locals.scm");
    if !locals_query_path.exists() {
        println!("\x1b[36mskipping loading of locals for {lang_name:?} since {locals_query_path:?} does not exist\x1b[0m");
    }
    LanguageConfig::new(
        grammar,
        &overwrites.highlights.clone().unwrap_or_else(|| {
            fs::read_to_string(&highlights_query_path)
                .map_err(|err| {
                    format!(
                        "failed to read highlights in {}: {err}",
                        highlights_query_path.display()
                    )
                })
                .unwrap()
        }),
        &overwrites
            .injections
            .clone()
            .unwrap_or_else(|| fs::read_to_string(&injections_query_path).unwrap_or_default()),
        &overwrites
            .locals
            .clone()
            .unwrap_or_else(|| fs::read_to_string(&locals_query_path).unwrap_or_default()),
    )
    .unwrap()
}

#[derive(Debug)]
struct TestLanguageLoader {
    languages: IndexMap<String, Language>,
    lang_config: Box<[OnceCell<LanguageConfig>]>,
    overwrites: Box<[Overwrites]>,
    test_theme: RefCell<IndexSet<String>>,
}

impl TestLanguageLoader {
    fn new() -> Self {
        let grammars = &GRAMMARS;

        Self {
            lang_config: (0..grammars.len()).map(|_| OnceCell::new()).collect(),
            overwrites: vec![Overwrites::default(); grammars.len()].into_boxed_slice(),
            test_theme: RefCell::default(),
            languages: grammars
                .iter()
                .enumerate()
                .map(|(i, grammar)| {
                    (
                        grammar.file_name().unwrap().to_str().unwrap().to_owned(),
                        Language::new(i as u32),
                    )
                })
                .collect(),
        }
    }

    fn get(&self, name: &str) -> Language {
        self.languages[name]
    }

    // TODO: remove on first use.
    #[allow(dead_code)]
    fn overwrite_injections(&mut self, lang: &str, content: String) {
        let lang = self.get(lang);
        self.overwrites[lang.idx()].injections = Some(content);
        self.lang_config[lang.idx()] = OnceCell::new();
    }

    fn overwrite_highlights(&mut self, lang: &str, content: String) {
        let lang = self.get(lang);
        self.overwrites[lang.idx()].highlights = Some(content);
        self.lang_config[lang.idx()] = OnceCell::new();
    }

    fn shadow_injections(&mut self, lang: &str, content: &str) {
        let lang = self.get(lang);
        let skidder_config = skidder_config();
        let grammar = self.languages.get_index(lang.idx()).unwrap().0;
        let grammar_dir = skidder_config.grammar_dir(grammar).unwrap();
        let mut injections =
            fs::read_to_string(grammar_dir.join("injections.scm")).unwrap_or_default();
        injections.push('\n');
        injections.push_str(content);
        self.overwrites[lang.idx()].injections = Some(injections);
        self.lang_config[lang.idx()] = OnceCell::new();
    }

    fn shadow_highlights(&mut self, lang: &str, content: &str) {
        let lang = self.get(lang);
        let skidder_config = skidder_config();
        let grammar = self.languages.get_index(lang.idx()).unwrap().0;
        let grammar_dir = skidder_config.grammar_dir(grammar).unwrap();
        let mut highlights = fs::read_to_string(grammar_dir.join("highlights.scm")).unwrap();
        highlights.push('\n');
        highlights.push_str(content);
        self.overwrites[lang.idx()].highlights = Some(highlights);
        self.lang_config[lang.idx()] = OnceCell::new();
    }
}

impl LanguageLoader for TestLanguageLoader {
    fn language_for_marker(&self, marker: InjectionLanguageMarker) -> Option<Language> {
        match marker {
            InjectionLanguageMarker::Name(name) => self.languages.get(name).copied(),
            InjectionLanguageMarker::Match(text) => {
                let name: Cow<str> = text.into();
                self.languages.get(name.as_ref()).copied()
            }
            _ => unimplemented!(),
        }
    }

    fn get_config(&self, lang: Language) -> Option<&LanguageConfig> {
        let config = self.lang_config[lang.idx()].get_or_init(|| {
            let config = get_grammar(
                self.languages.get_index(lang.idx()).unwrap().0,
                &self.overwrites[lang.idx()],
            );
            let mut theme = self.test_theme.borrow_mut();
            config.configure(|scope| {
                Some(Highlight::new(theme.insert_full(scope.to_owned()).0 as u32))
            });
            config
        });
        Some(config)
    }
}

fn lang_for_path(path: &Path, loader: &TestLanguageLoader) -> Language {
    match path
        .extension()
        .and_then(|it| it.to_str())
        .unwrap_or_default()
    {
        "rs" => loader.get("rust"),
        "html" => loader.get("html"),
        "css" => loader.get("css"),
        "erl" => loader.get("erlang"),
        "md" => loader.get("markdown"),
        extension => panic!("unknown file type .{extension}"),
    }
}

fn highlight_fixture(loader: &TestLanguageLoader, fixture: impl AsRef<Path>) {
    let path = Path::new("../fixtures").join(fixture);
    let lang = lang_for_path(&path, loader);
    check_highlighter_fixture(
        path,
        "// ",
        lang,
        loader,
        |highlight| loader.test_theme.borrow()[highlight.idx()].clone(),
        |_| ..,
    )
}

fn injection_fixture(loader: &TestLanguageLoader, fixture: impl AsRef<Path>) {
    let path = Path::new("../fixtures").join(fixture);
    let lang = lang_for_path(&path, loader);
    check_injection_fixture(
        path,
        "// ",
        lang,
        loader,
        |lang| loader.languages.get_index(lang.idx()).unwrap().0.clone(),
        |_| ..,
    )
}

#[test]
fn highlight() {
    let loader = TestLanguageLoader::new();
    highlight_fixture(&loader, "highlighter/hello_world.rs");
}

#[test]
fn layers() {
    let loader = TestLanguageLoader::new();

    let input = "/// Says hello.
///
/// this is *markdown-inline* markdown
/// 
/// # Example
///
/// ```rust
/// fn add(left: usize, right: usize) -> usize {
///     left + right
/// }
/// ```
pub fn hello() {}";

    let syntax = Syntax::new(input.into(), loader.get("rust"), PARSE_TIMEOUT, &loader).unwrap();

    let assert_injection = |snippet: &str, expected: &[&str]| {
        assert!(!expected.is_empty(), "all layers have at least 1 injection");

        let layer_lang_name = |layer: Layer| {
            loader
                .languages
                .get_index(syntax.layer(layer).language.idx())
                .unwrap()
                .0
                .clone()
        };

        let snippet_start = input.find(snippet).unwrap() as u32;
        let snippet_end = snippet_start + snippet.len() as u32;

        let layers = syntax
            .layers_for_byte_range(snippet_start, snippet_end)
            .map(layer_lang_name)
            .collect::<Vec<_>>();

        assert_eq!(&layers, expected, r#"snippet: "{snippet}""#);

        let layer = syntax.layer_for_byte_range(snippet_start, snippet_end);
        assert_eq!(
            &layer_lang_name(layer),
            expected.last().unwrap(),
            "last layer is the smallest fully encompassing layer"
        );
    };

    // Rust function in a code block in the rust documentation
    assert_injection("fn add(left: usize, ri", &["rust", "markdown", "rust"]);

    // Markdown heading  `# Example`
    assert_injection("# Example", &["rust", "markdown"]);

    // Outer-most Rust function `hello`
    assert_injection("pub fn hello() {}", &["rust"]);

    // Paragraph in the rust documentation
    assert_injection("markdown-inline", &["rust", "markdown", "markdown-inline"]);
}

#[test]
fn highlight_overlaps_with_injection() {
    let loader = TestLanguageLoader::new();
    // The comment node is highlighted both by the comment capture and as an injection for the
    // comment grammar.
    highlight_fixture(&loader, "highlighter/comment.html");
}

#[test]
fn rust_parameter_locals() {
    let loader = TestLanguageLoader::new();
    highlight_fixture(&loader, "highlighter/rust_parameter_locals.rs");
}

#[test]
fn codefence_rust_doc_comments() {
    let loader = TestLanguageLoader::new();
    highlight_fixture(&loader, "highlighter/codefence_rust_doc_comments.md");
}

#[test]
fn parameters_within_injections_within_injections() {
    let loader = TestLanguageLoader::new();
    // The root language is Rust. Then markdown is injected in a doc comment. Then within that
    // we have a code fence which is Rust again. Within that block we check that locals are
    // highlighted as expected.
    highlight_fixture(&loader, "highlighter/injectionception.rs");
    injection_fixture(&loader, "injections/injectionception.rs");
}

#[test]
fn html_in_edoc_in_erlang() {
    let loader = TestLanguageLoader::new();
    // This fixture exhibited a bug (which has been fixed) where a combined injection became
    // dormant at the same time as a new highlight started, causing a total reset of all
    // highlights (incorrectly).
    highlight_fixture(&loader, "highlighter/html_in_edoc_in_erlang.erl");
    injection_fixture(&loader, "injections/html_in_edoc_in_erlang.erl");
}

#[test]
fn non_local_pattern() {
    let mut loader = TestLanguageLoader::new();
    // Pretend that `this` is a builtin like `self`, but only when it is not a parameter.
    loader.shadow_highlights(
        "rust",
        r#"
((identifier) @variable.builtin
 (#eq? @variable.builtin "this")
 (#is-not? local))
"#,
    );
    highlight_fixture(&loader, "highlighter/non_local.rs");
}

#[test]
fn reference_highlight_starts_after_definition_ends() {
    let loader = TestLanguageLoader::new();
    // In this example the function name matches one of the parameters. The function name can be
    // a reference but since the definition occurs after the function name it, the function name
    // should not be highlighted as a parameter.
    highlight_fixture(
        &loader,
        "highlighter/reference_highlight_starts_after_definition_ends.rs",
    );
}

#[test]
fn combined_injection() {
    let mut loader = TestLanguageLoader::new();
    loader.shadow_injections(
        "rust",
        r#"
((doc_comment) @injection.content
 (#set! injection.language "markdown")
 (#set! injection.combined))"#,
    );
    highlight_fixture(&loader, "highlighter/rust_doc_comment.rs");
}

#[test]
fn injection_in_child() {
    let mut loader = TestLanguageLoader::new();
    // here doc_comment is a child of line_comment which has higher precedence
    // however since it doesn't include children the doc_comment injection is
    // still active here. This could probably use a more real world use case (maybe nix?)
    loader.shadow_injections(
        "rust",
        r#"
([(line_comment) (block_comment)] @injection.content
 (#set! injection.language "comment"))

([(line_comment (doc_comment) @injection.content) (block_comment (doc_comment) @injection.content)]
 (#set! injection.language "markdown")
 (#set! injection.combined))
"#,
    );
    highlight_fixture(&loader, "highlighter/rust_doc_comment.rs");
    injection_fixture(&loader, "injections/rust_doc_comment.rs");
}

#[test]
fn injection_precedence() {
    let mut loader = TestLanguageLoader::new();
    loader.shadow_injections(
        "rust",
        r#"
([(line_comment) (block_comment)] @injection.content
 (#set! injection.language "comment")
 (#set! injection.include-children))

([(line_comment (doc_comment) @injection.content) (block_comment (doc_comment) @injection.content)]
 (#set! injection.language "markdown")
 (#set! injection.combined))
 "#,
    );
    highlight_fixture(&loader, "highlighter/rust_doc_comment.rs");

    loader.shadow_injections(
        "rust",
        r#"
([(line_comment (doc_comment) @injection.content) (block_comment (doc_comment) @injection.content)]
 (#set! injection.language "markdown")
 (#set! injection.combined))

([(line_comment) (block_comment)] @injection.content
 (#set! injection.language "comment")
 (#set! injection.include-children))
 "#,
    );
    highlight_fixture(&loader, "highlighter/rust_no_doc_comment.rs");
    injection_fixture(&loader, "injections/rust_no_doc_comment.rs");

    loader.shadow_injections(
        "rust",
        r#"
((macro_invocation
   macro:
     [
       (scoped_identifier
         name: (_) @_macro_name)
       (identifier) @_macro_name
     ]
   (token_tree . (_) . (_) . (string_literal) @injection.content))
 (#any-of? @_macro_name
  ; std
  "some_macro")
  (#set! injection.language "rust")
  (#set! injection.include-children))
    "#,
    );
    injection_fixture(&loader, "injections/overlapping_injection.rs");
}

#[test]
fn rust_nested_vec() {
    let loader = TestLanguageLoader::new();
    highlight_fixture(&loader, "highlighter/nested_vec.rs");
    injection_fixture(&loader, "injections/nested_vec.rs");
}

#[test]
fn edit_remove_and_add_injection_layer() {
    let loader = TestLanguageLoader::new();
    // Add another backtick, causing the double old backtick to become a codefence and the second
    // HTML comment to become the codefence's body.
    // When we reuse the injection for the HTML comments, we need to be sure to re-parse the HTML
    // layer so that it recognizes that the second comment is no longer valid.
    let before_text = "<!---->\n``\n<!---->";
    let after_text = "<!---->\n```\n<!---->";
    let edit = InputEdit {
        start_byte: 10,
        old_end_byte: 10,
        new_end_byte: 11,
        start_point: Point::ZERO,
        old_end_point: Point::ZERO,
        new_end_point: Point::ZERO,
    };
    let mut syntax = Syntax::new(
        before_text.into(),
        loader.get("markdown"),
        PARSE_TIMEOUT,
        &loader,
    )
    .unwrap();
    // The test here is that `Syntax::update` can apply the edit `Ok(_)` without panicking.
    syntax
        .update(after_text.into(), PARSE_TIMEOUT, &[edit], &loader)
        .unwrap();

    // Now test the inverse. Start with the after text and edit it to be the before text. In this
    // case an injection is added for the HTML comment.
    let edit = InputEdit {
        start_byte: 10,
        old_end_byte: 11,
        new_end_byte: 10,
        start_point: Point::ZERO,
        old_end_point: Point::ZERO,
        new_end_point: Point::ZERO,
    };
    let mut syntax = Syntax::new(
        after_text.into(),
        loader.get("markdown"),
        PARSE_TIMEOUT,
        &loader,
    )
    .unwrap();
    // The test here is that `Syntax::update` can apply the edit `Ok(_)` without panicking.
    syntax
        .update(before_text.into(), PARSE_TIMEOUT, &[edit], &loader)
        .unwrap();
}

#[test]
fn markdown_bold_highlight() {
    let loader = TestLanguageLoader::new();
    // This is a very simple case to check that adjacent equivalent highlights are merged
    // properly: the `punctuation.bracket` highlight on the consecutive `*`s should be combined
    // into one span.
    highlight_fixture(&loader, "highlighter/markdown_bold.md");
}

#[test]
fn css_parent_child_highlight_precedence() {
    let mut loader = TestLanguageLoader::new();
    // NOTE: the pattern being tested here `((color_value) "#") @string.special` is odd and should
    // be rewritten to `(color_value "#" @string.special)` - that was probably the original intent
    // of the pattern. We overwrite the highlights to take the parts we need for this case so that
    // if/when we fix that pattern in the future it does not break this test case.
    loader.overwrite_highlights(
        "css",
        r##"
((property_name) @variable
 (#match? @variable "^--"))

"#" @punctuation
((color_value) "#") @string.special
(color_value) @string.special

[";" ":"] @punctuation.delimiter
"##
        .to_string(),
    );

    // In this case two patterns fight over the `#` character and both should actually highlight
    // it. Because of the odd way that the pattern `((color_value) "#") @string.special` is
    // written, the QueryIter yields the captures in the opposite order it would normally:
    // first the child pattern `{Node # 9..10}` and then `{Node color_value 9..13}`.
    //
    // This case checks the invariant that "`active_highlights` ends are sorted descending" is
    // preserved.
    highlight_fixture(&loader, "highlighter/parent_child_highlight_precedence.css");
}

#[test]
fn edoc_code_combined_injection() {
    let loader = TestLanguageLoader::new();

    highlight_fixture(&loader, "highlighter/edoc_code_combined_injection.erl");
    injection_fixture(&loader, "injections/edoc_code_combined_injection.erl");
}

#[test]
fn edoc_code_combined_injection_in_markdown() {
    let loader = TestLanguageLoader::new();
    // Same as the above but within markdown to add extra layers.
    highlight_fixture(
        &loader,
        "highlighter/edoc_code_combined_injection_in_markdown.md",
    );
}
