pub mod config;

use std::{
    borrow::Cow,
    collections::HashMap,
    fmt, iter,
    ops::{self, RangeBounds},
    path::Path,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use arc_swap::{ArcSwap, Guard};
use config::{Configuration, FileType, LanguageConfiguration, LanguageServerConfiguration};
use foldhash::HashSet;
use helix_loader::grammar::get_language;
use helix_stdx::rope::RopeSliceExt as _;
use ropey::RopeSlice;
use std::sync::OnceLock;
use tree_house::{
    highlighter,
    query_iter::QueryIter,
    tree_sitter::{
        query::{InvalidPredicateError, UserPredicate},
        Capture, Grammar, InactiveQueryCursor, InputEdit, Node, Pattern, Query, RopeInput, Tree,
    },
    Error, InjectionLanguageMarker, LanguageConfig as SyntaxConfig, Layer,
};

use crate::{indent::IndentQuery, tree_sitter, ChangeSet, Language};

pub use tree_house::{
    highlighter::{Highlight, HighlightEvent},
    query_iter::{CapturedMatch, QueryIterEvent, QueryMatchIter, QueryMatchIterEvent},
    Error as HighlighterError, LanguageLoader, TreeCursor, TREE_SITTER_MATCH_LIMIT,
};

#[derive(Debug)]
pub struct LanguageData {
    config: Arc<LanguageConfiguration>,
    syntax: OnceLock<Option<SyntaxConfig>>,
    indent_query: OnceLock<Option<IndentQuery>>,
    textobject_query: OnceLock<Option<TextObjectQuery>>,
    tag_query: OnceLock<Option<TagQuery>>,
    rainbow_query: OnceLock<Option<RainbowQuery>>,
    // Cache for language string/comment type detection (FR-010 optimization)
    has_string_or_comment_types: OnceLock<bool>,
}

impl LanguageData {
    fn new(config: LanguageConfiguration) -> Self {
        Self {
            config: Arc::new(config),
            syntax: OnceLock::new(),
            indent_query: OnceLock::new(),
            textobject_query: OnceLock::new(),
            tag_query: OnceLock::new(),
            rainbow_query: OnceLock::new(),
            has_string_or_comment_types: OnceLock::new(),
        }
    }

    pub fn config(&self) -> &Arc<LanguageConfiguration> {
        &self.config
    }

    /// Loads the grammar and compiles the highlights, injections and locals for the language.
    /// This function should only be used by this module or the xtask crate.
    pub fn compile_syntax_config(
        config: &LanguageConfiguration,
        loader: &Loader,
    ) -> Result<Option<SyntaxConfig>> {
        let name = &config.language_id;
        let parser_name = config.grammar.as_deref().unwrap_or(name);
        let Some(grammar) = get_language(parser_name)? else {
            log::info!("Skipping syntax config for '{name}' because the parser's shared library does not exist");
            return Ok(None);
        };
        let highlight_query_text = read_query(name, "highlights.scm");
        let injection_query_text = read_query(name, "injections.scm");
        let local_query_text = read_query(name, "locals.scm");
        let config = SyntaxConfig::new(
            grammar,
            &highlight_query_text,
            &injection_query_text,
            &local_query_text,
        )
        .with_context(|| format!("Failed to compile highlights for '{name}'"))?;

        reconfigure_highlights(&config, &loader.scopes());

        Ok(Some(config))
    }

    fn syntax_config(&self, loader: &Loader) -> Option<&SyntaxConfig> {
        self.syntax
            .get_or_init(|| {
                Self::compile_syntax_config(&self.config, loader)
                    .map_err(|err| {
                        log::error!("{err:#}");
                    })
                    .ok()
                    .flatten()
            })
            .as_ref()
    }

    /// Compiles the indents.scm query for a language.
    /// This function should only be used by this module or the xtask crate.
    pub fn compile_indent_query(
        grammar: Grammar,
        config: &LanguageConfiguration,
    ) -> Result<Option<IndentQuery>> {
        let name = &config.language_id;
        let text = read_query(name, "indents.scm");
        if text.is_empty() {
            return Ok(None);
        }
        let indent_query = IndentQuery::new(grammar, &text)
            .with_context(|| format!("Failed to compile indents.scm query for '{name}'"))?;
        Ok(Some(indent_query))
    }

    fn indent_query(&self, loader: &Loader) -> Option<&IndentQuery> {
        self.indent_query
            .get_or_init(|| {
                let grammar = self.syntax_config(loader)?.grammar;
                Self::compile_indent_query(grammar, &self.config)
                    .map_err(|err| {
                        log::error!("{err}");
                    })
                    .ok()
                    .flatten()
            })
            .as_ref()
    }

    /// Compiles the textobjects.scm query for a language.
    /// This function should only be used by this module or the xtask crate.
    pub fn compile_textobject_query(
        grammar: Grammar,
        config: &LanguageConfiguration,
    ) -> Result<Option<TextObjectQuery>> {
        let name = &config.language_id;
        let text = read_query(name, "textobjects.scm");
        if text.is_empty() {
            return Ok(None);
        }
        let query = Query::new(grammar, &text, |_, _| Ok(()))
            .with_context(|| format!("Failed to compile textobjects.scm queries for '{name}'"))?;
        Ok(Some(TextObjectQuery::new(query)))
    }

    fn textobject_query(&self, loader: &Loader) -> Option<&TextObjectQuery> {
        self.textobject_query
            .get_or_init(|| {
                let grammar = self.syntax_config(loader)?.grammar;
                Self::compile_textobject_query(grammar, &self.config)
                    .map_err(|err| {
                        log::error!("{err}");
                    })
                    .ok()
                    .flatten()
            })
            .as_ref()
    }

    /// Compiles the tags.scm query for a language.
    /// This function should only be used by this module or the xtask crate.
    pub fn compile_tag_query(
        grammar: Grammar,
        config: &LanguageConfiguration,
    ) -> Result<Option<TagQuery>> {
        let name = &config.language_id;
        let text = read_query(name, "tags.scm");
        if text.is_empty() {
            return Ok(None);
        }
        let query = Query::new(grammar, &text, |_pattern, predicate| match predicate {
            // TODO: these predicates are allowed in tags.scm queries but not yet used.
            UserPredicate::IsPropertySet { key: "local", .. } => Ok(()),
            UserPredicate::Other(pred) => match pred.name() {
                "strip!" | "select-adjacent!" => Ok(()),
                _ => Err(InvalidPredicateError::unknown(predicate)),
            },
            _ => Err(InvalidPredicateError::unknown(predicate)),
        })
        .with_context(|| format!("Failed to compile tags.scm query for '{name}'"))?;
        Ok(Some(TagQuery { query }))
    }

    fn tag_query(&self, loader: &Loader) -> Option<&TagQuery> {
        self.tag_query
            .get_or_init(|| {
                let grammar = self.syntax_config(loader)?.grammar;
                Self::compile_tag_query(grammar, &self.config)
                    .map_err(|err| {
                        log::error!("{err}");
                    })
                    .ok()
                    .flatten()
            })
            .as_ref()
    }

    /// Compiles the rainbows.scm query for a language.
    /// This function should only be used by this module or the xtask crate.
    pub fn compile_rainbow_query(
        grammar: Grammar,
        config: &LanguageConfiguration,
    ) -> Result<Option<RainbowQuery>> {
        let name = &config.language_id;
        let text = read_query(name, "rainbows.scm");
        if text.is_empty() {
            return Ok(None);
        }
        let rainbow_query = RainbowQuery::new(grammar, &text)
            .with_context(|| format!("Failed to compile rainbows.scm query for '{name}'"))?;
        Ok(Some(rainbow_query))
    }

    fn rainbow_query(&self, loader: &Loader) -> Option<&RainbowQuery> {
        self.rainbow_query
            .get_or_init(|| {
                let grammar = self.syntax_config(loader)?.grammar;
                Self::compile_rainbow_query(grammar, &self.config)
                    .map_err(|err| {
                        log::error!("{err}");
                    })
                    .ok()
                    .flatten()
            })
            .as_ref()
    }

    fn reconfigure(&self, scopes: &[String]) {
        if let Some(Some(config)) = self.syntax.get() {
            reconfigure_highlights(config, scopes);
        }
    }
}

fn reconfigure_highlights(config: &SyntaxConfig, recognized_names: &[String]) {
    config.configure(move |capture_name| {
        let capture_parts: Vec<_> = capture_name.split('.').collect();

        let mut best_index = None;
        let mut best_match_len = 0;
        for (i, recognized_name) in recognized_names.iter().enumerate() {
            let mut len = 0;
            let mut matches = true;
            for (i, part) in recognized_name.split('.').enumerate() {
                match capture_parts.get(i) {
                    Some(capture_part) if *capture_part == part => len += 1,
                    _ => {
                        matches = false;
                        break;
                    }
                }
            }
            if matches && len > best_match_len {
                best_index = Some(i);
                best_match_len = len;
            }
        }
        best_index.map(|idx| Highlight::new(idx as u32))
    });
}

pub fn read_query(lang: &str, query_filename: &str) -> String {
    tree_house::read_query(lang, |language| {
        helix_loader::grammar::load_runtime_file(language, query_filename).unwrap_or_default()
    })
}

#[derive(Debug, Default)]
pub struct Loader {
    languages: Vec<LanguageData>,
    languages_by_extension: HashMap<String, Language>,
    languages_by_shebang: HashMap<String, Language>,
    languages_glob_matcher: FileTypeGlobMatcher,
    language_server_configs: HashMap<String, LanguageServerConfiguration>,
    scopes: ArcSwap<Vec<String>>,
}

pub type LoaderError = globset::Error;

impl Loader {
    pub fn new(config: Configuration) -> Result<Self, LoaderError> {
        let mut languages = Vec::with_capacity(config.language.len());
        let mut languages_by_extension = HashMap::new();
        let mut languages_by_shebang = HashMap::new();
        let mut file_type_globs = Vec::new();

        for mut config in config.language {
            let language = Language(languages.len() as u32);
            config.language = Some(language);

            for file_type in &config.file_types {
                match file_type {
                    FileType::Extension(extension) => {
                        languages_by_extension.insert(extension.clone(), language);
                    }
                    FileType::Glob(glob) => {
                        file_type_globs.push(FileTypeGlob::new(glob.to_owned(), language));
                    }
                };
            }
            for shebang in &config.shebangs {
                languages_by_shebang.insert(shebang.clone(), language);
            }

            languages.push(LanguageData::new(config));
        }

        Ok(Self {
            languages,
            languages_by_extension,
            languages_by_shebang,
            languages_glob_matcher: FileTypeGlobMatcher::new(file_type_globs)?,
            language_server_configs: config.language_server,
            scopes: ArcSwap::from_pointee(Vec::new()),
        })
    }

    pub fn languages(&self) -> impl ExactSizeIterator<Item = (Language, &LanguageData)> {
        self.languages
            .iter()
            .enumerate()
            .map(|(idx, data)| (Language(idx as u32), data))
    }

    pub fn language_configs(&self) -> impl ExactSizeIterator<Item = &LanguageConfiguration> {
        self.languages.iter().map(|language| &*language.config)
    }

    pub fn language(&self, lang: Language) -> &LanguageData {
        &self.languages[lang.idx()]
    }

    pub fn language_for_name(&self, name: impl PartialEq<String>) -> Option<Language> {
        self.languages.iter().enumerate().find_map(|(idx, config)| {
            (name == config.config.language_id).then_some(Language(idx as u32))
        })
    }

    pub fn language_for_scope(&self, scope: &str) -> Option<Language> {
        self.languages.iter().enumerate().find_map(|(idx, config)| {
            (scope == config.config.scope).then_some(Language(idx as u32))
        })
    }

    pub fn language_for_match(&self, text: RopeSlice) -> Option<Language> {
        // PERF: If the name matches up with the id, then this saves the need to do expensive regex.
        let shortcircuit = self.language_for_name(text);
        if shortcircuit.is_some() {
            return shortcircuit;
        }

        // If the name did not match up with a known id, then match on injection regex.

        let mut best_match_length = 0;
        let mut best_match_position = None;
        for (idx, data) in self.languages.iter().enumerate() {
            if let Some(injection_regex) = &data.config.injection_regex {
                if let Some(mat) = injection_regex.find(text.regex_input()) {
                    let length = mat.end() - mat.start();
                    if length > best_match_length {
                        best_match_position = Some(idx);
                        best_match_length = length;
                    }
                }
            }
        }

        best_match_position.map(|i| Language(i as u32))
    }

    pub fn language_for_filename(&self, path: &Path) -> Option<Language> {
        // Find all the language configurations that match this file name
        // or a suffix of the file name.

        // TODO: content_regex handling conflict resolution
        self.languages_glob_matcher
            .language_for_path(path)
            .or_else(|| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .and_then(|extension| self.languages_by_extension.get(extension).copied())
            })
    }

    pub fn language_for_shebang(&self, text: RopeSlice) -> Option<Language> {
        // NOTE: this is slightly different than the one for injection markers in tree-house. It
        // is anchored at the beginning.
        use helix_stdx::rope::Regex;
        use std::sync::LazyLock;
        const SHEBANG: &str = r"^#!\s*(?:\S*[/\\](?:env\s+(?:\-\S+\s+)*)?)?([^\s\.\d]+)";
        static SHEBANG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(SHEBANG).unwrap());

        let marker = SHEBANG_REGEX
            .captures_iter(regex_cursor::Input::new(text))
            .map(|cap| text.byte_slice(cap.get_group(1).unwrap().range()))
            .next()?;
        self.language_for_shebang_marker(marker)
    }

    fn language_for_shebang_marker(&self, marker: RopeSlice) -> Option<Language> {
        let shebang: Cow<str> = marker.into();
        self.languages_by_shebang.get(shebang.as_ref()).copied()
    }

    pub fn indent_query(&self, lang: Language) -> Option<&IndentQuery> {
        self.language(lang).indent_query(self)
    }

    pub fn textobject_query(&self, lang: Language) -> Option<&TextObjectQuery> {
        self.language(lang).textobject_query(self)
    }

    pub fn tag_query(&self, lang: Language) -> Option<&TagQuery> {
        self.language(lang).tag_query(self)
    }

    fn rainbow_query(&self, lang: Language) -> Option<&RainbowQuery> {
        self.language(lang).rainbow_query(self)
    }

    pub fn language_server_configs(&self) -> &HashMap<String, LanguageServerConfiguration> {
        &self.language_server_configs
    }

    pub fn scopes(&self) -> Guard<Arc<Vec<String>>> {
        self.scopes.load()
    }

    pub fn set_scopes(&self, scopes: Vec<String>) {
        self.scopes.store(Arc::new(scopes));

        // Reconfigure existing grammars
        for data in &self.languages {
            data.reconfigure(&self.scopes());
        }
    }
}

impl LanguageLoader for Loader {
    fn language_for_marker(&self, marker: InjectionLanguageMarker) -> Option<Language> {
        match marker {
            InjectionLanguageMarker::Name(name) => self.language_for_name(name),
            InjectionLanguageMarker::Match(text) => self.language_for_match(text),
            InjectionLanguageMarker::Filename(text) => {
                let path: Cow<str> = text.into();
                self.language_for_filename(Path::new(path.as_ref()))
            }
            InjectionLanguageMarker::Shebang(text) => self.language_for_shebang_marker(text),
        }
    }

    fn get_config(&self, lang: Language) -> Option<&SyntaxConfig> {
        self.languages[lang.idx()].syntax_config(self)
    }
}

#[derive(Debug)]
struct FileTypeGlob {
    glob: globset::Glob,
    language: Language,
}

impl FileTypeGlob {
    pub fn new(glob: globset::Glob, language: Language) -> Self {
        Self { glob, language }
    }
}

#[derive(Debug)]
struct FileTypeGlobMatcher {
    matcher: globset::GlobSet,
    file_types: Vec<FileTypeGlob>,
}

impl Default for FileTypeGlobMatcher {
    fn default() -> Self {
        Self {
            matcher: globset::GlobSet::empty(),
            file_types: Default::default(),
        }
    }
}

impl FileTypeGlobMatcher {
    fn new(file_types: Vec<FileTypeGlob>) -> Result<Self, globset::Error> {
        let mut builder = globset::GlobSetBuilder::new();
        for file_type in &file_types {
            builder.add(file_type.glob.clone());
        }

        Ok(Self {
            matcher: builder.build()?,
            file_types,
        })
    }

    fn language_for_path(&self, path: &Path) -> Option<Language> {
        self.matcher
            .matches(path)
            .iter()
            .filter_map(|idx| self.file_types.get(*idx))
            .max_by_key(|file_type| file_type.glob.glob().len())
            .map(|file_type| file_type.language)
    }
}

#[derive(Debug)]
pub struct Syntax {
    inner: tree_house::Syntax,
}

const PARSE_TIMEOUT: Duration = Duration::from_millis(500); // half a second is pretty generous

impl Syntax {
    pub fn new(source: RopeSlice, language: Language, loader: &Loader) -> Result<Self, Error> {
        let inner = tree_house::Syntax::new(source, language, PARSE_TIMEOUT, loader)?;
        Ok(Self { inner })
    }

    pub fn update(
        &mut self,
        old_source: RopeSlice,
        source: RopeSlice,
        changeset: &ChangeSet,
        loader: &Loader,
    ) -> Result<(), Error> {
        let edits = generate_edits(old_source, changeset);
        if edits.is_empty() {
            Ok(())
        } else {
            self.inner.update(source, PARSE_TIMEOUT, &edits, loader)
        }
    }

    pub fn layer(&self, layer: Layer) -> &tree_house::LayerData {
        self.inner.layer(layer)
    }

    pub fn root_layer(&self) -> Layer {
        self.inner.root()
    }

    /// Finds the smallest injection layer which fully includes the range `start..=end`.
    ///
    /// This is the same as using the last item in the `layers_for_byte_range` iterator.
    pub fn layer_for_byte_range(&self, start: u32, end: u32) -> Layer {
        self.inner.layer_for_byte_range(start, end)
    }

    /// Returns an iterator of layers which **fully include** the byte range `start..=end`.
    ///
    /// The iterator is non-empty and the root is always the first element. Other layers are
    /// returned in decreasing order based on the size of each layer. I.e. the last element is
    /// the smallest layer including the byte range.
    pub fn layers_for_byte_range(
        &self,
        start: u32,
        end: u32,
    ) -> impl Iterator<Item = Layer> + use<'_> {
        self.inner.layers_for_byte_range(start, end)
    }

    pub fn root_language(&self) -> Language {
        self.layer(self.root_layer()).language
    }

    pub fn tree(&self) -> &Tree {
        self.inner.tree()
    }

    pub fn tree_for_byte_range(&self, start: u32, end: u32) -> &Tree {
        self.inner.tree_for_byte_range(start, end)
    }

    pub fn named_descendant_for_byte_range(&self, start: u32, end: u32) -> Option<Node<'_>> {
        self.inner.named_descendant_for_byte_range(start, end)
    }

    pub fn descendant_for_byte_range(&self, start: u32, end: u32) -> Option<Node<'_>> {
        self.inner.descendant_for_byte_range(start, end)
    }

    pub fn walk(&self) -> TreeCursor<'_> {
        self.inner.walk()
    }

    pub fn highlighter<'a>(
        &'a self,
        source: RopeSlice<'a>,
        loader: &'a Loader,
        range: impl RangeBounds<u32>,
    ) -> Highlighter<'a> {
        Highlighter::new(&self.inner, source, loader, range)
    }

    pub fn query_iter<'a, QueryLoader, LayerState, Range>(
        &'a self,
        source: RopeSlice<'a>,
        loader: QueryLoader,
        range: Range,
    ) -> QueryIter<'a, 'a, QueryLoader, LayerState>
    where
        QueryLoader: FnMut(Language) -> Option<&'a Query> + 'a,
        LayerState: Default,
        Range: RangeBounds<u32>,
    {
        QueryIter::new(&self.inner, source, loader, range)
    }

    pub fn tags<'a>(
        &'a self,
        source: RopeSlice<'a>,
        loader: &'a Loader,
        range: impl RangeBounds<u32>,
    ) -> QueryMatchIter<'a, 'a, impl FnMut(Language) -> Option<&'a Query> + 'a, ()> {
        QueryMatchIter::new(
            &self.inner,
            source,
            |lang| loader.tag_query(lang).map(|q| &q.query),
            range,
        )
    }

    pub fn rainbow_highlights(
        &self,
        source: RopeSlice,
        rainbow_length: usize,
        loader: &Loader,
        range: impl RangeBounds<u32>,
    ) -> OverlayHighlights {
        struct RainbowScope<'tree> {
            end: u32,
            node: Option<Node<'tree>>,
            highlight: Highlight,
        }

        let mut scope_stack = Vec::<RainbowScope>::new();
        let mut highlights = Vec::new();
        let mut query_iter = self.query_iter::<_, (), _>(
            source,
            |lang| loader.rainbow_query(lang).map(|q| &q.query),
            range,
        );

        while let Some(event) = query_iter.next() {
            let QueryIterEvent::Match(mat) = event else {
                continue;
            };

            let rainbow_query = loader
                .rainbow_query(query_iter.current_language())
                .expect("language must have a rainbow query to emit matches");

            let byte_range = mat.node.byte_range();
            // Pop any scopes that end before this capture begins.
            while scope_stack
                .last()
                .is_some_and(|scope| byte_range.start >= scope.end)
            {
                scope_stack.pop();
            }

            let capture = Some(mat.capture);
            if capture == rainbow_query.scope_capture {
                scope_stack.push(RainbowScope {
                    end: byte_range.end,
                    node: if rainbow_query
                        .include_children_patterns
                        .contains(&mat.pattern)
                    {
                        None
                    } else {
                        Some(mat.node.clone())
                    },
                    highlight: Highlight::new((scope_stack.len() % rainbow_length) as u32),
                });
            } else if capture == rainbow_query.bracket_capture {
                if let Some(scope) = scope_stack.last() {
                    if !scope
                        .node
                        .as_ref()
                        .is_some_and(|node| mat.node.parent().as_ref() != Some(node))
                    {
                        let start = source
                            .byte_to_char(source.floor_char_boundary(byte_range.start as usize));
                        let end =
                            source.byte_to_char(source.ceil_char_boundary(byte_range.end as usize));
                        highlights.push((scope.highlight, start..end));
                    }
                }
            }
        }

        OverlayHighlights::Heterogenous { highlights }
    }
}

pub type Highlighter<'a> = highlighter::Highlighter<'a, 'a, Loader>;

fn generate_edits(old_text: RopeSlice, changeset: &ChangeSet) -> Vec<InputEdit> {
    use crate::Operation::*;
    use tree_sitter::Point;

    let mut old_pos = 0;

    let mut edits = Vec::new();

    if changeset.changes.is_empty() {
        return edits;
    }

    let mut iter = changeset.changes.iter().peekable();

    // TODO; this is a lot easier with Change instead of Operation.
    while let Some(change) = iter.next() {
        let len = match change {
            Delete(i) | Retain(i) => *i,
            Insert(_) => 0,
        };
        let mut old_end = old_pos + len;

        match change {
            Retain(_) => {}
            Delete(_) => {
                let start_byte = old_text.char_to_byte(old_pos) as u32;
                let old_end_byte = old_text.char_to_byte(old_end) as u32;

                // deletion
                edits.push(InputEdit {
                    start_byte,               // old_pos to byte
                    old_end_byte,             // old_end to byte
                    new_end_byte: start_byte, // old_pos to byte
                    start_point: Point::ZERO,
                    old_end_point: Point::ZERO,
                    new_end_point: Point::ZERO,
                });
            }
            Insert(s) => {
                let start_byte = old_text.char_to_byte(old_pos) as u32;

                // a subsequent delete means a replace, consume it
                if let Some(Delete(len)) = iter.peek() {
                    old_end = old_pos + len;
                    let old_end_byte = old_text.char_to_byte(old_end) as u32;

                    iter.next();

                    // replacement
                    edits.push(InputEdit {
                        start_byte,                                // old_pos to byte
                        old_end_byte,                              // old_end to byte
                        new_end_byte: start_byte + s.len() as u32, // old_pos to byte + s.len()
                        start_point: Point::ZERO,
                        old_end_point: Point::ZERO,
                        new_end_point: Point::ZERO,
                    });
                } else {
                    // insert
                    edits.push(InputEdit {
                        start_byte,                                // old_pos to byte
                        old_end_byte: start_byte,                  // same
                        new_end_byte: start_byte + s.len() as u32, // old_pos + s.len()
                        start_point: Point::ZERO,
                        old_end_point: Point::ZERO,
                        new_end_point: Point::ZERO,
                    });
                }
            }
        }
        old_pos = old_end;
    }
    edits
}

/// A set of "overlay" highlights and ranges they apply to.
///
/// As overlays, the styles for the given `Highlight`s are merged on top of the syntax highlights.
#[derive(Debug)]
pub enum OverlayHighlights {
    /// All highlights use a single `Highlight`.
    ///
    /// Note that, currently, all ranges are assumed to be non-overlapping. This could change in
    /// the future though.
    Homogeneous {
        highlight: Highlight,
        ranges: Vec<ops::Range<usize>>,
    },
    /// A collection of different highlights for given ranges.
    ///
    /// Note that the ranges **must be non-overlapping**.
    Heterogenous {
        highlights: Vec<(Highlight, ops::Range<usize>)>,
    },
}

impl OverlayHighlights {
    pub fn single(highlight: Highlight, range: ops::Range<usize>) -> Self {
        Self::Homogeneous {
            highlight,
            ranges: vec![range],
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Homogeneous { ranges, .. } => ranges.is_empty(),
            Self::Heterogenous { highlights } => highlights.is_empty(),
        }
    }
}

#[derive(Debug)]
struct Overlay {
    highlights: OverlayHighlights,
    /// The position of the highlighter into the Vec of ranges of the overlays.
    ///
    /// Used by the `OverlayHighlighter`.
    idx: usize,
    /// The currently active highlight (and the ending character index) for this overlay.
    ///
    /// Used by the `OverlayHighlighter`.
    active_highlight: Option<(Highlight, usize)>,
}

impl Overlay {
    fn new(highlights: OverlayHighlights) -> Option<Self> {
        (!highlights.is_empty()).then_some(Self {
            highlights,
            idx: 0,
            active_highlight: None,
        })
    }

    fn current(&self) -> Option<(Highlight, ops::Range<usize>)> {
        match &self.highlights {
            OverlayHighlights::Homogeneous { highlight, ranges } => ranges
                .get(self.idx)
                .map(|range| (*highlight, range.clone())),
            OverlayHighlights::Heterogenous { highlights } => highlights.get(self.idx).cloned(),
        }
    }

    fn start(&self) -> Option<usize> {
        match &self.highlights {
            OverlayHighlights::Homogeneous { ranges, .. } => {
                ranges.get(self.idx).map(|range| range.start)
            }
            OverlayHighlights::Heterogenous { highlights } => highlights
                .get(self.idx)
                .map(|(_highlight, range)| range.start),
        }
    }
}

/// A collection of highlights to apply when rendering which merge on top of syntax highlights.
#[derive(Debug)]
pub struct OverlayHighlighter {
    overlays: Vec<Overlay>,
    next_highlight_start: usize,
    next_highlight_end: usize,
}

impl OverlayHighlighter {
    pub fn new(overlays: impl IntoIterator<Item = OverlayHighlights>) -> Self {
        let overlays: Vec<_> = overlays.into_iter().filter_map(Overlay::new).collect();
        let next_highlight_start = overlays
            .iter()
            .filter_map(|overlay| overlay.start())
            .min()
            .unwrap_or(usize::MAX);

        Self {
            overlays,
            next_highlight_start,
            next_highlight_end: usize::MAX,
        }
    }

    /// The current position in the overlay highlights.
    ///
    /// This method is meant to be used when treating this type as a cursor over the overlay
    /// highlights.
    ///
    /// `usize::MAX` is returned when there are no more overlay highlights.
    pub fn next_event_offset(&self) -> usize {
        self.next_highlight_start.min(self.next_highlight_end)
    }

    pub fn advance(&mut self) -> (HighlightEvent, impl Iterator<Item = Highlight> + '_) {
        let mut refresh = false;
        let prev_stack_size = self
            .overlays
            .iter()
            .filter(|overlay| overlay.active_highlight.is_some())
            .count();
        let pos = self.next_event_offset();

        if self.next_highlight_end == pos {
            for overlay in self.overlays.iter_mut() {
                if overlay
                    .active_highlight
                    .is_some_and(|(_highlight, end)| end == pos)
                {
                    overlay.active_highlight.take();
                }
            }

            refresh = true;
        }

        while self.next_highlight_start == pos {
            let mut activated_idx = usize::MAX;
            for (idx, overlay) in self.overlays.iter_mut().enumerate() {
                let Some((highlight, range)) = overlay.current() else {
                    continue;
                };
                if range.start != self.next_highlight_start {
                    continue;
                }

                // If this overlay has a highlight at this start index, set its active highlight
                // and increment the cursor position within the overlay.
                overlay.active_highlight = Some((highlight, range.end));
                overlay.idx += 1;

                activated_idx = activated_idx.min(idx);
            }

            // If `self.next_highlight_start == pos` that means that some overlay was ready to
            // emit a highlight, so `activated_idx` must have been set to an existing index.
            assert!(
                (0..self.overlays.len()).contains(&activated_idx),
                "expected an overlay to highlight (at pos {pos}, there are {} overlays)",
                self.overlays.len()
            );

            // If any overlays are active after the (lowest) one which was just activated, the
            // highlights need to be refreshed.
            refresh |= self.overlays[activated_idx..]
                .iter()
                .any(|overlay| overlay.active_highlight.is_some());

            self.next_highlight_start = self
                .overlays
                .iter()
                .filter_map(|overlay| overlay.start())
                .min()
                .unwrap_or(usize::MAX);
        }

        self.next_highlight_end = self
            .overlays
            .iter()
            .filter_map(|overlay| Some(overlay.active_highlight?.1))
            .min()
            .unwrap_or(usize::MAX);

        let (event, start) = if refresh {
            (HighlightEvent::Refresh, 0)
        } else {
            (HighlightEvent::Push, prev_stack_size)
        };

        (
            event,
            self.overlays
                .iter()
                .flat_map(|overlay| overlay.active_highlight)
                .map(|(highlight, _end)| highlight)
                .skip(start),
        )
    }
}

#[derive(Debug)]
pub enum CapturedNode<'a> {
    Single(Node<'a>),
    /// Guaranteed to be not empty
    Grouped(Vec<Node<'a>>),
}

impl CapturedNode<'_> {
    pub fn start_byte(&self) -> usize {
        match self {
            Self::Single(n) => n.start_byte() as usize,
            Self::Grouped(ns) => ns[0].start_byte() as usize,
        }
    }

    pub fn end_byte(&self) -> usize {
        match self {
            Self::Single(n) => n.end_byte() as usize,
            Self::Grouped(ns) => ns.last().unwrap().end_byte() as usize,
        }
    }

    pub fn byte_range(&self) -> ops::Range<usize> {
        self.start_byte()..self.end_byte()
    }
}

#[derive(Debug)]
pub struct TextObjectQuery {
    query: Query,
}

impl TextObjectQuery {
    pub fn new(query: Query) -> Self {
        Self { query }
    }

    /// Run the query on the given node and return sub nodes which match given
    /// capture ("function.inside", "class.around", etc).
    ///
    /// Captures may contain multiple nodes by using quantifiers (+, *, etc),
    /// and support for this is partial and could use improvement.
    ///
    /// ```query
    /// (comment)+ @capture
    ///
    /// ; OR
    /// (
    ///   (comment)*
    ///   .
    ///   (function)
    /// ) @capture
    /// ```
    pub fn capture_nodes<'a>(
        &'a self,
        capture_name: &str,
        node: &Node<'a>,
        slice: RopeSlice<'a>,
    ) -> Option<impl Iterator<Item = CapturedNode<'a>>> {
        self.capture_nodes_any(&[capture_name], node, slice)
    }

    /// Find the first capture that exists out of all given `capture_names`
    /// and return sub nodes that match this capture.
    pub fn capture_nodes_any<'a>(
        &'a self,
        capture_names: &[&str],
        node: &Node<'a>,
        slice: RopeSlice<'a>,
    ) -> Option<impl Iterator<Item = CapturedNode<'a>>> {
        let capture = capture_names
            .iter()
            .find_map(|cap| self.query.get_capture(cap))?;

        let mut cursor = InactiveQueryCursor::new(0..u32::MAX, TREE_SITTER_MATCH_LIMIT)
            .execute_query(&self.query, node, RopeInput::new(slice));
        let capture_node = iter::from_fn(move || {
            let mat = cursor.next_match()?;
            Some(mat.nodes_for_capture(capture).cloned().collect())
        })
        .filter_map(move |nodes: Vec<_>| {
            if nodes.len() > 1 {
                Some(CapturedNode::Grouped(nodes))
            } else {
                nodes.into_iter().map(CapturedNode::Single).next()
            }
        });
        Some(capture_node)
    }
}

#[derive(Debug)]
pub struct TagQuery {
    pub query: Query,
}

pub fn pretty_print_tree<W: fmt::Write>(fmt: &mut W, node: Node) -> fmt::Result {
    if node.child_count() == 0 {
        if node_is_visible(&node) {
            write!(fmt, "({})", node.kind())
        } else {
            write!(fmt, "\"{}\"", format_anonymous_node_kind(node.kind()))
        }
    } else {
        pretty_print_tree_impl(fmt, &mut node.walk(), 0)
    }
}

fn node_is_visible(node: &Node) -> bool {
    node.is_named() && node.grammar().node_kind_is_visible(node.kind_id())
}

fn format_anonymous_node_kind(kind: &str) -> Cow<'_, str> {
    if kind.contains('"') || kind.contains('\\') {
        Cow::Owned(kind.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        Cow::Borrowed(kind)
    }
}

fn pretty_print_tree_impl<W: fmt::Write>(
    fmt: &mut W,
    cursor: &mut tree_sitter::TreeCursor,
    depth: usize,
) -> fmt::Result {
    let node = cursor.node();
    let visible = node_is_visible(&node);

    if visible {
        let indentation_columns = depth * 2;
        write!(fmt, "{:indentation_columns$}", "")?;

        if let Some(field_name) = cursor.field_name() {
            write!(fmt, "{}: ", field_name)?;
        }

        write!(fmt, "({}", node.kind())?;
    } else {
        write!(fmt, " \"{}\"", format_anonymous_node_kind(node.kind()))?;
    }

    // Handle children.
    if cursor.goto_first_child() {
        loop {
            if node_is_visible(&cursor.node()) {
                fmt.write_char('\n')?;
            }

            pretty_print_tree_impl(fmt, cursor, depth + 1)?;

            if !cursor.goto_next_sibling() {
                break;
            }
        }

        let moved = cursor.goto_parent();
        // The parent of the first child must exist, and must be `node`.
        debug_assert!(moved);
        debug_assert!(cursor.node() == node);
    }

    if visible {
        fmt.write_char(')')?;
    }

    Ok(())
}

/// Finds the child of `node` which contains the given byte range.
pub fn child_for_byte_range<'a>(node: &Node<'a>, range: ops::Range<u32>) -> Option<Node<'a>> {
    for child in node.children() {
        let child_range = child.byte_range();

        if range.start >= child_range.start && range.end <= child_range.end {
            return Some(child);
        }
    }

    None
}

#[derive(Debug)]
pub struct RainbowQuery {
    query: Query,
    include_children_patterns: HashSet<Pattern>,
    scope_capture: Option<Capture>,
    bracket_capture: Option<Capture>,
}

impl RainbowQuery {
    fn new(grammar: Grammar, source: &str) -> Result<Self, tree_sitter::query::ParseError> {
        let mut include_children_patterns = HashSet::default();

        let query = Query::new(grammar, source, |pattern, predicate| match predicate {
            UserPredicate::SetProperty {
                key: "rainbow.include-children",
                val,
            } => {
                if val.is_some() {
                    return Err(
                        "property 'rainbow.include-children' does not take an argument".into(),
                    );
                }
                include_children_patterns.insert(pattern);
                Ok(())
            }
            _ => Err(InvalidPredicateError::unknown(predicate)),
        })?;

        Ok(Self {
            include_children_patterns,
            scope_capture: query.get_capture("rainbow.scope"),
            bracket_capture: query.get_capture("rainbow.bracket"),
            query,
        })
    }
}

#[cfg(test)]
mod test {
    use std::sync::LazyLock;

    use super::*;
    use crate::{Rope, Transaction};

    static LOADER: LazyLock<Loader> = LazyLock::new(crate::config::default_lang_loader);

    #[test]
    fn test_textobject_queries() {
        let query_str = r#"
        (line_comment)+ @quantified_nodes
        ((line_comment)+) @quantified_nodes_grouped
        ((line_comment) (line_comment)) @multiple_nodes_grouped
        "#;
        let source = Rope::from_str(
            r#"
/// a comment on
/// multiple lines
        "#,
        );

        let language = LOADER.language_for_name("rust").unwrap();
        let grammar = LOADER.get_config(language).unwrap().grammar;
        let query = Query::new(grammar, query_str, |_, _| Ok(())).unwrap();
        let textobject = TextObjectQuery::new(query);
        let syntax = Syntax::new(source.slice(..), language, &LOADER).unwrap();

        let root = syntax.tree().root_node();
        let test = |capture, range| {
            let matches: Vec<_> = textobject
                .capture_nodes(capture, &root, source.slice(..))
                .unwrap()
                .collect();

            assert_eq!(
                matches[0].byte_range(),
                range,
                "@{} expected {:?}",
                capture,
                range
            )
        };

        test("quantified_nodes", 1..37);
        test("quantified_nodes_grouped", 1..37);
        // TODO: the query for this works instead as
        // ```
        // ((line_comment) @capture (line_comment) @capture)
        // ```
        // The query used in this test case only captures the first line_comment node.
        // Determine if this behavior is intentional in tree-sitter.
        // test("multiple_nodes_grouped", 1..37);
    }

    #[test]
    fn test_input_edits() {
        use tree_sitter::{InputEdit, Point};

        let doc = Rope::from("hello world!\ntest 123");
        let transaction = Transaction::change(
            &doc,
            vec![(6, 11, Some("test".into())), (12, 17, None)].into_iter(),
        );
        let edits = generate_edits(doc.slice(..), transaction.changes());
        // transaction.apply(&mut state);

        assert_eq!(
            edits,
            &[
                InputEdit {
                    start_byte: 6,
                    old_end_byte: 11,
                    new_end_byte: 10,
                    start_point: Point::ZERO,
                    old_end_point: Point::ZERO,
                    new_end_point: Point::ZERO
                },
                InputEdit {
                    start_byte: 12,
                    old_end_byte: 17,
                    new_end_byte: 12,
                    start_point: Point::ZERO,
                    old_end_point: Point::ZERO,
                    new_end_point: Point::ZERO
                }
            ]
        );

        // Testing with the official example from tree-sitter
        let mut doc = Rope::from("fn test() {}");
        let transaction =
            Transaction::change(&doc, vec![(8, 8, Some("a: u32".into()))].into_iter());
        let edits = generate_edits(doc.slice(..), transaction.changes());
        transaction.apply(&mut doc);

        assert_eq!(doc, "fn test(a: u32) {}");
        assert_eq!(
            edits,
            &[InputEdit {
                start_byte: 8,
                old_end_byte: 8,
                new_end_byte: 14,
                start_point: Point::ZERO,
                old_end_point: Point::ZERO,
                new_end_point: Point::ZERO
            }]
        );
    }

    #[track_caller]
    fn assert_pretty_print(
        language_name: &str,
        source: &str,
        expected: &str,
        start: usize,
        end: usize,
    ) {
        let source = Rope::from_str(source);
        let language = LOADER.language_for_name(language_name).unwrap();
        let syntax = Syntax::new(source.slice(..), language, &LOADER).unwrap();

        let root = syntax
            .tree()
            .root_node()
            .descendant_for_byte_range(start as u32, end as u32)
            .unwrap();

        let mut output = String::new();
        pretty_print_tree(&mut output, root).unwrap();

        assert_eq!(expected, output);
    }

    #[test]
    fn test_pretty_print() {
        let source = r#"// Hello"#;
        assert_pretty_print("rust", source, "(line_comment \"//\")", 0, source.len());

        // A large tree should be indented with fields:
        let source = r#"fn main() {
            println!("Hello, World!");
        }"#;
        assert_pretty_print(
            "rust",
            source,
            concat!(
                "(function_item \"fn\"\n",
                "  name: (identifier)\n",
                "  parameters: (parameters \"(\" \")\")\n",
                "  body: (block \"{\"\n",
                "    (expression_statement\n",
                "      (macro_invocation\n",
                "        macro: (identifier) \"!\"\n",
                "        (token_tree \"(\"\n",
                "          (string_literal \"\\\"\"\n",
                "            (string_content) \"\\\"\") \")\")) \";\") \"}\"))",
            ),
            0,
            source.len(),
        );

        // Selecting a token should print just that token:
        let source = r#"fn main() {}"#;
        assert_pretty_print("rust", source, r#""fn""#, 0, 1);

        // Error nodes are printed as errors:
        let source = r#"}{"#;
        assert_pretty_print("rust", source, "(ERROR \"}\" \"{\")", 0, source.len());

        // Fields broken under unnamed nodes are determined correctly.
        // In the following source, `object` belongs to the `singleton_method`
        // rule but `name` and `body` belong to an unnamed helper `_method_rest`.
        // This can cause a bug with a pretty-printing implementation that
        // uses `Node::field_name_for_child` to determine field names but is
        // fixed when using `tree_sitter::TreeCursor::field_name`.
        let source = "def self.method_name
          true
        end";
        assert_pretty_print(
            "ruby",
            source,
            concat!(
                "(singleton_method \"def\"\n",
                "  object: (self) \".\"\n",
                "  name: (identifier)\n",
                "  body: (body_statement\n",
                "    (true)) \"end\")"
            ),
            0,
            source.len(),
        );
    }

    /// Asserts the IME region detected at every byte position of `source`:
    /// positions within `sensitive` must yield `expected`, all others `Code`.
    #[track_caller]
    fn assert_ime_span(
        source: &str,
        sensitive: std::ops::RangeInclusive<usize>,
        expected: ImeSensitiveRegion,
    ) {
        let source = Rope::from_str(source);
        let language = LOADER.language_for_name("rust").unwrap();
        let syntax = Syntax::new(source.slice(..), language, &LOADER).unwrap();

        for pos in 0..=source.len_bytes() {
            let region =
                detect_ime_sensitive_region(Some(&syntax), source.slice(..), &LOADER, pos).region;
            let want = if sensitive.contains(&pos) {
                expected
            } else {
                ImeSensitiveRegion::Code
            };
            assert_eq!(region, want, "unexpected IME region at byte position {pos}");
        }
    }

    #[test]
    fn test_ime_region_line_comment_boundaries() {
        // `//` is a complete marker: only positions from right after the second
        // `/` — including the end-of-line cursor — are comment content.
        // Positions 0 and 1 sit inside the marker and must stay Code.
        let line = "// Heavily based on https://github.com/codemirror/closebrackets/";
        assert_ime_span(
            &format!("{line}\nfn f() {{}}\n"),
            2..=line.len(),
            ImeSensitiveRegion::CommentContent,
        );
    }

    #[test]
    fn test_ime_region_inner_doc_comment_boundaries() {
        // Everything after the leading `//` of `//!` is content, including the
        // position right after the `!`.
        let line =
            "//! this module provides the functionality to insert the paired closing character.";
        assert_ime_span(
            &format!("{line}\nfn f() {{}}\n"),
            2..=line.len(),
            ImeSensitiveRegion::CommentContent,
        );
    }

    #[test]
    fn test_ime_region_outer_doc_comment_boundaries() {
        let line = "/// outer doc comment";
        assert_ime_span(
            &format!("{line}\nfn f() {{}}\n"),
            2..=line.len(),
            ImeSensitiveRegion::CommentContent,
        );
    }

    #[test]
    fn test_ime_region_comment_with_multibyte_content() {
        let line = "// 你好";
        assert_ime_span(
            &format!("{line}\nfn f() {{}}\n"),
            2..=line.len(),
            ImeSensitiveRegion::CommentContent,
        );
    }

    #[test]
    fn test_ime_region_empty_line_comment() {
        // The end-of-line cursor of a bare `//` is still comment content.
        let line = "//";
        assert_ime_span(
            &format!("{line}\nfn f() {{}}\n"),
            2..=line.len(),
            ImeSensitiveRegion::CommentContent,
        );
    }

    #[test]
    fn test_ime_region_block_comment_boundaries() {
        // The `/*` marker is skipped; the closing `*/` stays outside the span
        // (positions inside the closing token remain content per FR-007, but
        // the position after `*/` is Code again).
        let line = "/* block */";
        assert_ime_span(
            &format!("{line}\nfn f() {{}}\n"),
            2..=line.len() - 1,
            ImeSensitiveRegion::CommentContent,
        );
    }

    #[test]
    fn test_ime_region_multiline_block_comment() {
        let line = "/* first\nsecond */";
        assert_ime_span(
            &format!("{line}\nfn f() {{}}\n"),
            2..=line.len() - 1,
            ImeSensitiveRegion::CommentContent,
        );
    }

    #[test]
    fn test_ime_region_string_boundaries() {
        // Quotes are excluded (FR-004); the position right before the trailing
        // quote is content (FR-006).
        let line = r#""string""#;
        assert_ime_span(
            &format!("{line}\nfn f() {{}}\n"),
            1..=line.len() - 1,
            ImeSensitiveRegion::StringContent,
        );
    }

    #[test]
    fn test_ime_region_span_extends_to_eol() {
        // The span cached for cursor-move handling must keep the end-of-line
        // cursor inside a line comment: for `// c` the span is [2, 5),
        // covering positions 2..=4 where position 4 is the newline.
        let source = Rope::from_str("// c\nfn f() {}\n");
        let language = LOADER.language_for_name("rust").unwrap();
        let syntax = Syntax::new(source.slice(..), language, &LOADER).unwrap();
        let detection = detect_ime_sensitive_region(Some(&syntax), source.slice(..), &LOADER, 2);
        assert_eq!(detection.region, ImeSensitiveRegion::CommentContent);
        assert_eq!(detection.node_range, Some((2, 5)));
    }
}

/// IME (Input Method Editor) sensitive region types.
///
/// This enum identifies the type of region the cursor is in, which determines
/// whether IME should be enabled or disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeSensitiveRegion {
    /// String content region (excluding leading quote).
    /// IME should be enabled in this region.
    StringContent,
    /// Comment content region (excluding header symbols like `//`, `/*`).
    /// IME should be enabled in this region.
    CommentContent,
    /// Code region (non-sensitive).
    /// IME should be disabled in this region.
    Code,
    /// Entire file (when syntax parsing is unavailable, failed, or still in progress).
    /// IME should be enabled in this region.
    EntireFile,
}

/// Result of IME region detection, with optional byte span used for caching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImeRegionDetection {
    pub region: ImeSensitiveRegion,
    pub node_range: Option<(usize, usize)>,
}

impl ImeRegionDetection {
    const fn new(region: ImeSensitiveRegion, node_range: Option<(usize, usize)>) -> Self {
        Self { region, node_range }
    }

    const fn code() -> Self {
        Self::new(ImeSensitiveRegion::Code, None)
    }

    fn entire_file(len_bytes: usize) -> Self {
        Self::new(ImeSensitiveRegion::EntireFile, Some((0, len_bytes)))
    }
}

/// Check if the language has string or comment types defined in its grammar.
///
/// This function checks the language configuration and highlights query to determine
/// if the language supports string or comment node types. Used to implement FR-010:
/// if a language doesn't have these types, the entire file is treated as IME sensitive.
///
/// # Arguments
/// * `loader` - The syntax loader to access language configuration
/// * `language` - The language identifier
///
/// # Returns
/// `true` if the language has string or comment types, `false` otherwise.
///
/// # Performance
/// This function uses a OnceCell cache to avoid repeated file I/O operations.
/// The result is computed once per language and cached for the lifetime of the Loader.
fn language_has_string_or_comment_types(loader: &Loader, language: Language) -> bool {
    let language_data = loader.language(language);

    // Use cached result if available, otherwise compute and cache
    *language_data.has_string_or_comment_types.get_or_init(|| {
        let config = language_data.config();

        // Check if language has comment tokens configured (indicates comment support)
        let has_comment = config.comment_tokens.is_some() || config.block_comment_tokens.is_some();

        // Check if language has string types by examining highlights query
        // Most languages with string support will have @string captures in highlights.scm
        let has_string = {
            let highlight_query = read_query(&config.language_id, "highlights.scm");
            // Check if the query contains @string capture
            highlight_query.contains("@string") || highlight_query.contains("string")
        };

        has_comment || has_string
    })
}

/// Detect IME sensitive region at the given cursor position.
///
/// This function queries the syntax tree to determine if the cursor is in a
/// string content region, comment content region, or code region.
///
/// # Arguments
/// * `syntax` - The syntax tree for the document (None if parsing unavailable/failed/in progress)
/// * `source` - The document text
/// * `loader` - The syntax loader for query compilation
/// * `cursor_pos` - The cursor position in bytes
///
/// # Returns
/// The detected IME sensitive region type.
///
/// # Behavior
/// - Returns `EntireFile` if syntax parsing is unavailable, failed, or still in progress (FR-008, FR-009, FR-021)
/// - Returns `EntireFile` if language doesn't have string/comment types (FR-010)
/// - Detects comment and string regions by checking node types (simplified approach)
/// - Excludes leading quote symbols (FR-004) and the full comment marker prefix
///   (`//`, `//!`, `/*`, ...) by matching the language's configured comment tokens (FR-005)
/// - Includes trailing quote first character (FR-006) and comment tail first character (FR-007);
///   line comments also include the end-of-line cursor position, where typed text
///   still continues the comment
#[inline]
fn contains_ignore_ascii_case(haystack: &str, needle: &[u8]) -> bool {
    let h = haystack.as_bytes();
    if needle.is_empty() {
        return true;
    }
    let n = needle.len();
    if n > h.len() {
        return false;
    }
    h.windows(n).any(|w| w.eq_ignore_ascii_case(needle))
}

pub fn detect_ime_sensitive_region(
    syntax: Option<&Syntax>,
    source: RopeSlice, // Used for full file fallback caching
    loader: &Loader,   // Now used to check language configuration
    cursor_pos: usize,
) -> ImeRegionDetection {
    // If syntax parsing is unavailable, failed, or still in progress, treat entire file as sensitive
    let Some(syntax) = syntax else {
        log::trace!("IME region detection: syntax unavailable, returning EntireFile");
        return ImeRegionDetection::entire_file(source.len_bytes());
    };

    let language = syntax.root_language();
    if !language_has_string_or_comment_types(loader, language) {
        log::trace!(
            "IME region detection: language has no string/comment types, returning EntireFile (FR-010)"
        );
        return ImeRegionDetection::entire_file(source.len_bytes());
    }

    // Check if the current node or any of its parents is a comment or string node
    // We need to check parents because the cursor might be in a child node
    // Also check for code blocks in document languages (like Markdown)
    let language_config = loader.language(language).config();
    let is_document_language = matches!(
        language_config.language_id.as_str(),
        "markdown" | "markdown-rustdoc" | "markdown.inline"
    );

    // The cursor position is probed directly first. Tree-sitter can resolve a
    // position exactly at a node's end byte to the node *following* it, so a
    // second pass probes the preceding position while still testing the
    // original cursor against each candidate span. This keeps the cursor at
    // the end of a line comment inside the region: anything typed there
    // continues the comment.
    let mut is_in_code_block = false;
    for (probe_index, probe_pos) in [Some(cursor_pos), cursor_pos.checked_sub(1)]
        .into_iter()
        .flatten()
        .enumerate()
    {
        // Get the node at probe position (including unnamed nodes)
        let Some(node) = syntax
            .tree()
            .root_node()
            .descendant_for_byte_range(probe_pos as u32, probe_pos as u32)
        else {
            continue;
        };

        let mut current_check = Some(node);
        while let Some(n) = current_check {
            let node_kind = n.kind();
            let node_start = n.start_byte() as usize;
            let node_end = n.end_byte() as usize;

            log::trace!(
                "IME region detection: checking node_kind={}, node_range=[{}, {})",
                node_kind,
                node_start,
                node_end
            );

            // For document languages, check if we're in a code block. Only the
            // direct probe counts: the preceding-position probe may reach into
            // the previous line's tree and must not leak a code block across
            // the line boundary.
            if probe_index == 0 && is_document_language && node_kind.contains("code_block") {
                is_in_code_block = true;
            }

            if let Some(detection) = node_ime_detection(
                node_kind,
                node_start,
                node_end,
                cursor_pos,
                source,
                language_config,
            ) {
                return detection;
            }

            current_check = n.parent();
        }
    }

    // If we didn't find a comment or string node at the cursor position,
    // check if this is a document language (like Markdown) where the entire file
    // should be treated as IME sensitive except for code blocks.
    if is_document_language {
        if is_in_code_block {
            // Cursor is in a code block, treat as non-sensitive (Code)
            log::trace!(
                "IME region detection: found code_block node in document language, returning Code"
            );
            return ImeRegionDetection::code();
        }
        // Not in a code block, treat entire file as sensitive
        log::trace!(
            "IME region detection: document language (not in code block), returning EntireFile"
        );
        return ImeRegionDetection::entire_file(source.len_bytes());
    }

    // For other languages, default to code region (non-sensitive)
    log::trace!("IME region detection: no comment/string node found, returning Code");
    ImeRegionDetection::code()
}

/// Check a single node against the cursor position for IME region detection.
///
/// Returns `Some` only when the cursor lies within the node's *content* span.
/// Positions inside the leading marker — or past the end — return `None` so
/// the caller keeps walking the parent nodes: a `//!` doc comment, for
/// example, resolves through its `doc_comment` child to the outer
/// `line_comment`, whose span starts right after the `//` marker.
fn node_ime_detection(
    node_kind: &str,
    node_start: usize,
    node_end: usize,
    cursor_pos: usize,
    source: RopeSlice,
    language_config: &LanguageConfiguration,
) -> Option<ImeRegionDetection> {
    // Check if node type contains "comment" (case-insensitive check for robustness)
    if contains_ignore_ascii_case(node_kind, b"comment") {
        let (start, end) = if node_kind == "comment_content" {
            // Content child nodes already exclude the comment marker
            (node_start, node_end)
        } else {
            comment_content_span(source, language_config, node_start, node_end)
        };
        if cursor_pos >= start && cursor_pos < end {
            log::trace!(
                "IME region detection: comment content span [{}, {}) contains cursor at {}, returning CommentContent",
                start,
                end,
                cursor_pos
            );
            return Some(ImeRegionDetection::new(
                ImeSensitiveRegion::CommentContent,
                Some((start, end)),
            ));
        }
    }

    // Check if node type contains "string" (excluding string_start and string_end)
    if node_kind.contains("string")
        && !node_kind.contains("string_start")
        && !node_kind.contains("string_end")
    {
        // Exclude leading quote symbols (FR-004), include the position right
        // before the trailing quote (FR-006)
        let (start, end) = if node_kind == "string_content" {
            // Content child nodes already exclude the quotes
            (node_start, node_end)
        } else {
            (node_start.saturating_add(1), node_end)
        };
        if cursor_pos >= start && cursor_pos < end {
            log::trace!(
                "IME region detection: string content span [{}, {}) contains cursor at {}, returning StringContent",
                start,
                end,
                cursor_pos
            );
            return Some(ImeRegionDetection::new(
                ImeSensitiveRegion::StringContent,
                Some((start, end)),
            ));
        }
    }

    None
}

/// Compute the content span `[start, end)` of a comment node for IME detection.
///
/// The leading marker is skipped by matching the node's text against the
/// language's configured comment tokens (FR-005):
///
/// - Line comments match the *shortest* `comment_tokens` entry, so `//!` and
///   `///` content begins right after the base `//` marker. The span extends
///   one byte past the node end: the cursor at the end of the line is still
///   inside the comment, since anything typed there continues it (FR-007).
/// - Block comments match the shortest `block_comment_tokens` start token;
///   their span keeps the node end exclusive so the position right after the
///   closing token is code again.
/// - When no configured token matches (exotic grammars), fall back to
///   skipping a single byte, preserving the historical behavior.
fn comment_content_span(
    source: RopeSlice,
    language_config: &LanguageConfiguration,
    node_start: usize,
    node_end: usize,
) -> (usize, usize) {
    let line_marker_len = language_config
        .comment_tokens
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|token| source_starts_with(source, node_start, node_end, token))
        .map(|token| token.len())
        .min();
    if let Some(len) = line_marker_len {
        // Grammars disagree on whether a line comment node swallows the
        // trailing newline (Rust doc comments like `//!` do, plain `//`
        // comments do not). Either way the span must include the cursor
        // sitting on the line terminator — typing there continues the
        // comment — but not the first position of the next line.
        let end = if node_end > node_start && source.get_byte(node_end - 1) == Some(b'\n') {
            node_end
        } else {
            node_end + 1
        };
        return (node_start + len, end);
    }

    let block_marker_len = language_config
        .block_comment_tokens
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|token| source_starts_with(source, node_start, node_end, &token.start))
        .map(|token| token.start.len())
        .min();
    if let Some(len) = block_marker_len {
        return (node_start + len, node_end);
    }

    (node_start.saturating_add(1), node_end)
}

/// Whether the rope text starting at `start` begins with `token`, bounded by `end`.
fn source_starts_with(source: RopeSlice, start: usize, end: usize, token: &str) -> bool {
    start + token.len() <= end
        && token
            .bytes()
            .enumerate()
            .all(|(i, byte)| source.get_byte(start + i) == Some(byte))
}
