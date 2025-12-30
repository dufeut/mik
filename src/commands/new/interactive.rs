//! Interactive prompts for project creation.
//!
//! Uses simple stdin/stdout without additional dependencies.

use super::templates::{Language, Template};
use anyhow::Result;
use std::io::{self, Write};

/// Prompt user for language and template selection.
pub fn prompt_options(
    default_lang: Language,
    default_template: Template,
) -> Result<(Language, Template)> {
    let lang = prompt_language(default_lang)?;
    let template = prompt_template(lang, default_template)?;
    Ok((lang, template))
}

/// Prompt user to select a language.
fn prompt_language(default: Language) -> Result<Language> {
    println!();
    println!("Select language:");
    println!("  [1] rust       - Recommended, smallest output (~100KB)");
    println!("  [2] typescript - Requires jco + bundler (~12MB)");
    print!("\nEnter choice [1]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let lang = match input.trim().to_lowercase().as_str() {
        "" | "1" | "rust" | "rs" => Language::Rust,
        "2" | "typescript" | "ts" => Language::TypeScript,
        _ => {
            println!("Invalid choice, using {}", default.display_name());
            default
        },
    };

    Ok(lang)
}

/// Prompt user to select a template for the given language.
fn prompt_template(lang: Language, default: Template) -> Result<Template> {
    let templates = lang.available_templates();

    // If only one template available, skip prompt
    if templates.len() == 1 {
        return Ok(templates[0]);
    }

    println!();
    println!("Select template:");
    for (i, t) in templates.iter().enumerate() {
        println!("  [{}] {} - {}", i + 1, t.display_name(), t.description());
    }
    print!("\nEnter choice [1]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let template = match input.trim().to_lowercase().as_str() {
        "" | "1" => templates[0],
        "2" if templates.len() > 1 => templates[1],
        "basic" => Template::Basic,
        "rest-api" | "restapi" | "rest_api" => {
            if templates.contains(&Template::RestApi) {
                Template::RestApi
            } else {
                println!(
                    "Template not available for {}, using basic",
                    lang.display_name()
                );
                Template::Basic
            }
        },
        _ => {
            println!("Invalid choice, using {}", default.display_name());
            default
        },
    };

    Ok(template)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_display_name() {
        assert_eq!(Language::Rust.display_name(), "Rust");
        assert_eq!(Language::TypeScript.display_name(), "TypeScript");
    }

    #[test]
    fn test_template_description() {
        assert_eq!(Template::Basic.description(), "Simple hello world handler");
        assert_eq!(
            Template::RestApi.description(),
            "CRUD REST API with typed inputs"
        );
    }
}
