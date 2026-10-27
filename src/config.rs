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

    /// The settings that a run uses, with every key of the tables that `constructo check` has set.
    pub fn effective(options: &CheckOptions, prelude: bool, verbose: bool) -> Config {
        Config {
            global: Global { verbose: Some(verbose) },
            experimental: ExperimentalSettings {
                some_query: Some(options.experimental.some_query),
            },
            check: CheckSettings {
                prelude: Some(prelude),
                type_check: Some(options.type_check),
                widening: Some(options.widening),
                continue_after_error: Some(options.continue_after_error),
                allow_refs: Some(options.allow_refs),
                infer_limit: Some(options.infer_limit),
                default_limit: Some(options.default_limit),
            },
        }
    }

    /// The settings as a TOML document that `Config::parse` reads back. Every key is written, with its setting after
    /// the defaults have been applied, and a comment says what it does. So the document is a template for a file.
    pub fn to_toml(&self) -> String {
        let options = self.check_options();
        let sections: [(&str, Vec<Key>); 3] =
            [
                (
                    "global",
                    vec![Key::new(
                        "verbose",
                        "Report each input that was checked.",
                        self.verbose(),
                    )],
                ),
                (
                    "experimental",
                    vec![Key::new(
                        "some-query",
                        "A postfix `?` on a value, as in `0?`, is shorthand for `Some(0)`.",
                        options.experimental.some_query,
                    )],
                ),
                (
                    "check",
                    vec![
                    Key::new("prelude", "Load the standard prelude, with Bool and Option.", self.prelude()),
                    Key::new(
                        "type-check",
                        "Check that data-values match the data-types. If false, only check that they are well-formed.",
                        options.type_check,
                    ),
                    Key::new(
                        "widening",
                        "Let a Nat fit where an Int or Float is expected, and an Int where a Float is expected.",
                        options.widening,
                    ),
                    Key::new(
                        "continue",
                        "After a type error, carry on with the next value to report more errors.",
                        options.continue_after_error,
                    ),
                    Key::new(
                        "allow-refs",
                        "Let the definition of a const refer to other consts.",
                        options.allow_refs,
                    ),
                    Key::new(
                        "infer-limit",
                        "The most nodes that the type inferred for a const may have.",
                        options.infer_limit,
                    ),
                    Key::new(
                        "default-limit",
                        "The most nodes that the default value of an omitted argument may have.",
                        options.default_limit,
                    ),
                ],
                ),
            ];
        let tables: Vec<String> = sections
            .iter()
            .map(|(table, keys)| {
                let lines: Vec<String> = keys
                    .iter()
                    .map(|key| format!("# {}\n{} = {}\n", key.comment, key.name, key.value))
                    .collect();
                format!("[{}]\n{}", table, lines.join("\n"))
            })
            .collect();
        tables.join("\n")
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

/// A key of the configuration file as `Config::to_toml` writes it.
struct Key {
    name: &'static str,
    comment: &'static str,
    /// The value in TOML syntax.
    value: String,
}

impl Key {
    fn new(name: &'static str, comment: &'static str, value: impl std::fmt::Display) -> Key {
        Key {
            name,
            comment,
            value: value.to_string(),
        }
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
    fn the_effective_settings_are_written_so_that_they_can_be_read_back() {
        let options = CheckOptions {
            widening: false,
            infer_limit: 7,
            experimental: Experimental::all(),
            ..CheckOptions::default()
        };
        let config = Config::effective(&options, false, true);
        let text = config.to_toml();
        assert!(
            text.contains("[check]\n") && text.contains("infer-limit = 7\n") && text.contains("continue = false\n")
        );
        assert!(text.contains("# Load the standard prelude"), "{}", text);
        assert_eq!(Config::parse(&text, "c.toml").unwrap(), config);
        assert_eq!(config.check_options(), options);
        assert!(!config.prelude() && config.verbose());
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
