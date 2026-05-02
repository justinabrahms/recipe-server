use cooklang::Recipe;
use std::sync::Arc;
use std::sync::OnceLock;

/// Shared parser; cheaper to reuse than `cooklang::parse` per call.
fn parser() -> &'static cooklang::CooklangParser {
    static P: OnceLock<cooklang::CooklangParser> = OnceLock::new();
    P.get_or_init(cooklang::CooklangParser::extended)
}

/// Borrow the shared parser's `Converter`. Needed to call `Recipe::group_ingredients`.
pub fn converter() -> &'static cooklang::convert::Converter {
    parser().converter()
}

#[derive(Debug, Clone)]
pub struct ParseOutcome {
    pub recipe: Arc<Recipe>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cooklang parse failed:\n{0}")]
    Cooklang(String),
}

pub fn parse_str(source: &str) -> Result<ParseOutcome, ParseError> {
    let result = parser().parse(source);
    match result.into_result() {
        Ok((recipe, report)) => {
            let warnings = report
                .warnings()
                .map(|w| w.message.to_string())
                .filter(|m| !is_noise_warning(m))
                .collect();
            Ok(ParseOutcome {
                recipe: Arc::new(recipe),
                warnings,
            })
        }
        Err(report) => {
            let msg = report
                .errors()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            Err(ParseError::Cooklang(if msg.is_empty() {
                "unknown parse error".to_string()
            } else {
                msg
            }))
        }
    }
}

pub fn parse_file(path: &std::path::Path) -> Result<ParseOutcome, ParseError> {
    let source = std::fs::read_to_string(path)?;
    parse_str(&source)
}

/// Drop warnings that are generated for cooklang shapes we deliberately
/// support per PLAN.md (e.g. the `>>` metadata shorthand) or that are
/// false-positive (e.g. servings parsed as Number is "unsupported").
fn is_noise_warning(msg: &str) -> bool {
    let msg = msg.trim();
    msg.contains("'>>' syntax for metadata is deprecated")
        || msg.starts_with("Unsupported value for key")
}
