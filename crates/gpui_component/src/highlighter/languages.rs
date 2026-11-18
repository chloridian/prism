use gpui::SharedString;

use crate::highlighter::GrammarConfig;

const LANGUAGES: &[Language] = &[
    Language::Json,
    Language::Plain,
    #[cfg(feature = "tree-sitter-html")]
    Language::Html,
    #[cfg(feature = "tree-sitter-lua")]
    Language::Lua,
    #[cfg(feature = "tree-sitter-markdown")]
    Language::Markdown,
    #[cfg(feature = "tree-sitter-markdown")]
    Language::MarkdownInline,
    #[cfg(feature = "tree-sitter-toml")]
    Language::Toml,
    #[cfg(feature = "tree-sitter-yaml")]
    Language::Yaml,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Json,
    Plain,
    #[cfg(feature = "tree-sitter-html")]
    Html,
    #[cfg(feature = "tree-sitter-lua")]
    Lua,
    #[cfg(feature = "tree-sitter-markdown")]
    Markdown,
    #[cfg(feature = "tree-sitter-markdown")]
    MarkdownInline,
    #[cfg(feature = "tree-sitter-toml")]
    Toml,
    #[cfg(feature = "tree-sitter-yaml")]
    Yaml,
}

impl From<Language> for SharedString {
    fn from(language: Language) -> Self {
        language.name().into()
    }
}

impl Language {
    pub fn all() -> impl Iterator<Item = Self> {
        LANGUAGES.iter().copied()
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Plain => "text",
            #[cfg(feature = "tree-sitter-html")]
            Self::Html => "html",
            #[cfg(feature = "tree-sitter-lua")]
            Self::Lua => "lua",
            #[cfg(feature = "tree-sitter-markdown")]
            Self::Markdown => "markdown",
            #[cfg(feature = "tree-sitter-markdown")]
            Self::MarkdownInline => "markdown_inline",
            #[cfg(feature = "tree-sitter-toml")]
            Self::Toml => "toml",
            #[cfg(feature = "tree-sitter-yaml")]
            Self::Yaml => "yaml",
        }
    }

    #[allow(unused)]
    pub fn from_str(s: &str) -> Self {
        Self::from_name(s).unwrap_or(Self::Plain)
    }

    pub(crate) fn from_name(s: &str) -> Option<Self> {
        match s {
            "text" | "plain" | "plaintext" => Some(Self::Plain),
            "json" | "jsonc" => Some(Self::Json),
            #[cfg(feature = "tree-sitter-html")]
            "html" => Some(Self::Html),
            #[cfg(feature = "tree-sitter-lua")]
            "lua" => Some(Self::Lua),
            #[cfg(feature = "tree-sitter-markdown")]
            "markdown" | "md" | "mdx" => Some(Self::Markdown),
            #[cfg(feature = "tree-sitter-markdown")]
            "markdown_inline" | "markdown-inline" => Some(Self::MarkdownInline),
            #[cfg(feature = "tree-sitter-toml")]
            "toml" => Some(Self::Toml),
            #[cfg(feature = "tree-sitter-yaml")]
            "yaml" | "yml" => Some(Self::Yaml),
            _ => None,
        }
    }

    #[allow(unused)]
    pub(super) fn injection_languages(&self) -> Vec<SharedString> {
        let mut languages: Vec<&'static str> = Vec::new();

        match self {
            #[cfg(feature = "tree-sitter-markdown")]
            Self::Markdown => {
                languages.push("markdown_inline");
                #[cfg(feature = "tree-sitter-html")]
                languages.push("html");
                #[cfg(feature = "tree-sitter-toml")]
                languages.push("toml");
                #[cfg(feature = "tree-sitter-yaml")]
                languages.push("yaml");
            }
            _ => {}
        }

        languages.into_iter().map(SharedString::from).collect()
    }

    /// Return the language info for the language.
    ///
    /// (language, query, injection, locals)
    pub(super) fn config(&self) -> GrammarConfig {
        let (language, query, injection, locals) = match self {
            Self::Plain => return GrammarConfig::plain(self.name()),
            Self::Json => (
                tree_sitter_json::LANGUAGE,
                include_str!("languages/json/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-markdown")]
            Self::Markdown => (
                tree_sitter_md::LANGUAGE,
                include_str!("languages/markdown/highlights.scm"),
                include_str!("languages/markdown/injections.scm"),
                "",
            ),
            #[cfg(feature = "tree-sitter-markdown")]
            Self::MarkdownInline => (
                tree_sitter_md::INLINE_LANGUAGE,
                include_str!("languages/markdown_inline/highlights.scm"),
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-toml")]
            Self::Toml => (
                tree_sitter_toml_ng::LANGUAGE,
                tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-yaml")]
            Self::Yaml => (
                tree_sitter_yaml::LANGUAGE,
                tree_sitter_yaml::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
            #[cfg(feature = "tree-sitter-html")]
            Self::Html => (
                tree_sitter_html::LANGUAGE,
                include_str!("languages/html/highlights.scm"),
                include_str!("languages/html/injections.scm"),
                "",
            ),
            #[cfg(feature = "tree-sitter-lua")]
            Self::Lua => (
                tree_sitter_lua::LANGUAGE,
                include_str!("languages/lua/highlights.scm"),
                tree_sitter_lua::INJECTIONS_QUERY,
                tree_sitter_lua::LOCALS_QUERY,
            ),
        };

        let language = tree_sitter::Language::new(language);

        GrammarConfig::new(
            self.name(),
            language,
            self.injection_languages(),
            query,
            injection,
            locals,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_name() {
        assert_eq!(Language::Plain.name(), "text");
        assert_eq!(Language::Json.name(), "json");

        #[cfg(feature = "tree-sitter-markdown")]
        {
            assert_eq!(Language::MarkdownInline.name(), "markdown_inline");
            assert_eq!(Language::Markdown.name(), "markdown");
        }

        #[cfg(feature = "tree-sitter-yaml")]
        assert_eq!(Language::Yaml.name(), "yaml");
        #[cfg(feature = "tree-sitter-toml")]
        assert_eq!(Language::Toml.name(), "toml");
        #[cfg(feature = "tree-sitter-html")]
        assert_eq!(Language::Html.name(), "html");
        #[cfg(feature = "tree-sitter-lua")]
        assert_eq!(Language::Lua.name(), "lua");
    }

    #[test]
    fn test_language_aliases_only_resolve_enabled_features() {
        assert_eq!(Language::from_name("text"), Some(Language::Plain));
        assert_eq!(Language::from_name("jsonc"), Some(Language::Json));
        assert_eq!(Language::from_name("unknown"), None);
        assert_eq!(Language::from_name("rs"), None);

        #[cfg(feature = "tree-sitter-markdown")]
        assert_eq!(Language::from_name("md"), Some(Language::Markdown));
        #[cfg(not(feature = "tree-sitter-markdown"))]
        assert_eq!(Language::from_name("md"), None);

        assert_eq!(Language::from_str("unknown"), Language::Plain);
    }

    #[test]
    fn test_every_language_query_compiles() {
        let failures: Vec<String> = Language::all()
            .filter_map(|language| {
                let config = language.config();
                let grammar = config.language?;
                let source = format!(
                    "{}{}{}",
                    config.injections, config.locals, config.highlights
                );
                let err = tree_sitter::Query::new(&grammar, &source).err()?;
                Some(format!("{}: {err}", language.name()))
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
