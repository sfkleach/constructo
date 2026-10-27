//! The configuration file, which gives the settings that the command-line options would.
//!
//! The file is in TOML. Every key is optional, and a key that is left out keeps its default. Every problem with the
//! file is an error, so that a misspelt key cannot silently leave a setting at its default.

use crate::diagnostics::{Diagnostic, Pos};
use crate::parser::Experimental;
use crate::CheckOptions;
use serde::Deserialize;

/// The settings in a configuration file. One file serves every subcommand: a subcommand reads the shared tables and
/// its own and ignores the others, but a table that no subcommand has is an error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Settings that every subcommand has.
    pub global: Global,
    /// Which experimental features are switched on, by the name that they have on the command line.
    pub experimental: ExperimentalSettings,
    /// Settings of `constructo check`.
    pub check: CheckSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Global {
    pub verbose: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ExperimentalSettings {
    pub some_query: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct CheckSettings {
    /// Whether to load the standard prelude.
    pub prelude: Option<bool>,
    pub type_check: Option<bool>,
    pub widening: Option<bool>,
    #[serde(rename = "continue")]
    pub continue_after_error: Option<bool>,
    pub allow_refs: Option<bool>,
    pub infer_limit: Option<usize>,
    pub default_limit: Option<usize>,
}

impl Config {
    /// Reads the settings from the text of a configuration file. `name` is the name of the file in diagnostics.
    pub fn parse(text: &str, name: &str) -> Result<Config, Diagnostic> {
        toml::from_str(text).map_err(|e| {
            let offset = e.span().map_or(0, |span| span.start);
            Diagnostic::new(name, position(text, offset), e.message())
        })
    }

    /// The options for checking: the defaults, changed by what the file sets.
    pub fn check_options(&self) -> CheckOptions {
        let defaults = CheckOptions::default();
        let check = &self.check;
        CheckOptions {
            type_check: check.type_check.unwrap_or(defaults.type_check),
            widening: check.widening.unwrap_or(defaults.widening),
            continue_after_error: check.continue_after_error.unwrap_or(defaults.continue_after_error),
            allow_refs: check.allow_refs.unwrap_or(defaults.allow_refs),
            infer_limit: check.infer_limit.unwrap_or(defaults.infer_limit),
            default_limit: check.default_limit.unwrap_or(defaults.default_limit),
            experimental: Experimental {
                some_query: self.experimental.some_query.unwrap_or(defaults.experimental.some_query),
            },
        }
    }

    /// Whether the standard prelude is loaded, which is not one of the `CheckOptions`.
    pub fn prelude(&self) -> bool {
        self.check.prelude.unwrap_or(true)
    }

    /// Whether to report each input that was checked, which is not one of the `CheckOptions` either.
    pub fn verbose(&self) -> bool {
        self.global.verbose.unwrap_or(false)
    }
}

/// The line and column of a byte offset in a text, counting characters.
fn position(text: &str, offset: usize) -> Pos {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() as u32 + 1;
    let col = before.rsplit('\n').next().map_or(0, |last| last.chars().count()) as u32 + 1;
    Pos { line, col }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(text: &str) -> (u32, u32, String) {
        let d = Config::parse(text, "c.toml").unwrap_err();
        (d.pos.line, d.pos.col, d.message)
    }

    #[test]
    fn an_empty_file_sets_nothing() {
        assert_eq!(Config::parse("", "c.toml").unwrap(), Config::default());
        assert_eq!(
            Config::parse("# Only a comment.\n", "c.toml").unwrap().check_options(),
            CheckOptions::default()
        );
        assert!(Config::default().prelude());
        assert!(!Config::default().verbose());
    }

    #[test]
    fn reads_every_setting() {
        let text = "[global]\nverbose = true\n[check]\nprelude = false\ntype-check = false\nwidening = false\n\
                    continue = true\nallow-refs = true\ninfer-limit = 5\ndefault-limit = 6\n\
                    [experimental]\nsome-query = true\n";
        let config = Config::parse(text, "c.toml").unwrap();
        assert!(!config.prelude());
        assert!(config.verbose());
        assert_eq!(
            config.check_options(),
            CheckOptions {
                type_check: false,
                widening: false,
                continue_after_error: true,
                allow_refs: true,
                infer_limit: 5,
                default_limit: 6,
                experimental: Experimental::all(),
            }
        );
    }

    #[test]
    fn a_key_that_is_left_out_keeps_its_default() {
        let config = Config::parse("[check]\nwidening = false\n", "c.toml").unwrap();
        let expected = CheckOptions {
            widening: false,
            ..CheckOptions::default()
        };
        assert_eq!(config.check_options(), expected);
    }

    #[test]
    fn an_unknown_key_is_an_error_that_lists_the_known_ones() {
        let (line, col, message) = error("[check]\nalow-refs = true\n");
        assert_eq!((line, col), (2, 1));
        assert!(
            message.contains("unknown field `alow-refs`") && message.contains("`allow-refs`"),
            "{}",
            message
        );
        let (_, _, message) = error("[experimental]\nsome_query = true\n");
        assert!(
            message.contains("unknown field `some_query`") && message.contains("`some-query`"),
            "{}",
            message
        );
    }

    #[test]
    fn an_unknown_section_is_an_error() {
        let (line, col, message) = error("[global]\nverbose = true\n\n[lint]\nfoo = 1\n");
        assert_eq!((line, col), (4, 2));
        assert!(
            message.contains("unknown field `lint`") && message.contains("`check`"),
            "{}",
            message
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_is_an_error() {
        let (line, col, message) = error("[global]\nverbose = \"yes\"\n");
        assert_eq!((line, col), (2, 11));
        assert!(message.contains("expected a boolean"), "{}", message);
        assert_eq!(error("[check]\ninfer-limit = -1\n").0, 2);
        assert_eq!(error("[check]\ninfer-limit = 1.5\n").0, 2);
        assert_eq!(error("[check]\ninfer-limit = \"10\"\n").0, 2);
    }

    #[test]
    fn a_syntax_error_in_the_toml_is_an_error() {
        assert_eq!(error("[check\n").0, 1);
        assert_eq!(error("[check]\nprelude = \n").0, 2);
        assert_eq!(error("[check]\nprelude = true\nprelude = false\n").0, 3);
    }

    #[test]
    fn the_column_counts_characters() {
        let (line, col, _) = error("# é\n[check]\nbogus = 1\n");
        assert_eq!((line, col), (3, 1));
        // The text before the error on the same line has a character of two bytes.
        let (line, col, _) = error("[global]\nverbose = \"é\" x\n");
        assert_eq!((line, col), (2, 15));
    }

    #[test]
    fn the_diagnostic_is_in_the_usual_form() {
        let d = Config::parse("[check]\nbogus = 1\n", "c.toml").unwrap_err();
        assert!(d.to_string().starts_with("ERROR: unknown field `bogus`"), "{}", d);
        assert!(d.to_string().contains("(c.toml, line 2, col 1)"), "{}", d);
    }
}
