//! Unit normalization for shopping-list aggregation.
//!
//! Per PLAN.md §7.1: every quantity normalizes to a canonical metric unit per
//! dimension (g, ml, or piece) before aggregation.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dimension {
    Mass,
    Volume,
    Count,
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Dimension::Mass => "mass",
            Dimension::Volume => "volume",
            Dimension::Count => "count",
        })
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum UnitError {
    #[error("unknown unit `{0}`; expected metric (g, ml, l, kg, dl, cl, pc, …) or none")]
    UnknownUnit(String),
}

/// `(canonical_amount, dimension)` for a quantity expressed in some unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Canonical {
    pub amount: f64,
    pub dimension: Dimension,
}

/// Convert a value+unit pair to canonical units. `None` for the unit means a
/// counted item (e.g. `@onion{2}` → 2 pieces).
pub fn canonicalize(value: f64, unit: Option<&str>) -> Result<Canonical, UnitError> {
    match unit {
        None => Ok(Canonical {
            amount: value,
            dimension: Dimension::Count,
        }),
        Some(u) => {
            let lower = u.trim().to_ascii_lowercase();
            if let Some(factor_g) = mass_factor_to_grams(&lower) {
                Ok(Canonical {
                    amount: value * factor_g,
                    dimension: Dimension::Mass,
                })
            } else if let Some(factor_ml) = volume_factor_to_ml(&lower) {
                Ok(Canonical {
                    amount: value * factor_ml,
                    dimension: Dimension::Volume,
                })
            } else if is_count_unit(&lower) {
                Ok(Canonical {
                    amount: value,
                    dimension: Dimension::Count,
                })
            } else {
                Err(UnitError::UnknownUnit(u.to_string()))
            }
        }
    }
}

fn mass_factor_to_grams(u: &str) -> Option<f64> {
    Some(match u {
        "g" | "gram" | "grams" => 1.0,
        "kg" | "kilogram" | "kilograms" => 1000.0,
        _ => return None,
    })
}

fn volume_factor_to_ml(u: &str) -> Option<f64> {
    Some(match u {
        "ml" | "millilitre" | "millilitres" | "milliliter" | "milliliters" => 1.0,
        "cl" | "centilitre" | "centilitres" => 10.0,
        "dl" | "decilitre" | "decilitres" => 100.0,
        "l" | "litre" | "litres" | "liter" | "liters" => 1000.0,
        _ => return None,
    })
}

fn is_count_unit(u: &str) -> bool {
    matches!(u, "pc" | "pcs" | "piece" | "pieces")
}

/// Format a canonical amount using the "best display unit" rule (§7.2):
///   mass    ≥ 1000 g  → kg with 2 dp
///   volume  ≥ 1000 ml → l with 2 dp
///   count   integer when whole, else 1 dp
pub fn format_display(c: Canonical) -> String {
    match c.dimension {
        Dimension::Mass => {
            if c.amount.abs() >= 1000.0 {
                format!("{:.2} kg", c.amount / 1000.0)
            } else {
                format!("{} g", trim_number(c.amount, 1))
            }
        }
        Dimension::Volume => {
            if c.amount.abs() >= 1000.0 {
                format!("{:.2} l", c.amount / 1000.0)
            } else {
                format!("{} ml", trim_number(c.amount, 1))
            }
        }
        Dimension::Count => {
            if (c.amount - c.amount.round()).abs() < 1e-9 {
                format!("{}", c.amount as i64)
            } else {
                format!("{:.1}", c.amount)
            }
        }
    }
}

fn trim_number(value: f64, max_decimals: usize) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        let s = format!("{value:.*}", max_decimals);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(value: f64, unit: Option<&str>) -> Canonical {
        canonicalize(value, unit).unwrap()
    }

    #[test]
    fn mass_to_grams() {
        assert_eq!(c(2.0, Some("kg")).amount, 2000.0);
        assert_eq!(c(500.0, Some("g")).amount, 500.0);
        assert_eq!(c(1.0, Some("Grams")).amount, 1.0); // case-insensitive
    }

    #[test]
    fn volume_to_ml() {
        assert_eq!(c(1.0, Some("l")).amount, 1000.0);
        assert_eq!(c(2.0, Some("dl")).amount, 200.0);
        assert_eq!(c(5.0, Some("cl")).amount, 50.0);
        assert_eq!(c(750.0, Some("ml")).amount, 750.0);
    }

    #[test]
    fn count_with_or_without_unit() {
        assert_eq!(c(2.0, None).dimension, Dimension::Count);
        assert_eq!(c(3.0, Some("pcs")).dimension, Dimension::Count);
    }

    #[test]
    fn unknown_unit_is_err() {
        assert!(matches!(
            canonicalize(1.0, Some("oz")),
            Err(UnitError::UnknownUnit(_))
        ));
        assert!(matches!(
            canonicalize(1.0, Some("tsp")),
            Err(UnitError::UnknownUnit(_))
        ));
    }

    #[test]
    fn display_mass_promotes_to_kg() {
        assert_eq!(
            format_display(Canonical {
                amount: 1500.0,
                dimension: Dimension::Mass
            }),
            "1.50 kg"
        );
        assert_eq!(
            format_display(Canonical {
                amount: 250.0,
                dimension: Dimension::Mass
            }),
            "250 g"
        );
    }

    #[test]
    fn display_volume_promotes_to_l() {
        assert_eq!(
            format_display(Canonical {
                amount: 1250.0,
                dimension: Dimension::Volume
            }),
            "1.25 l"
        );
        assert_eq!(
            format_display(Canonical {
                amount: 200.0,
                dimension: Dimension::Volume
            }),
            "200 ml"
        );
    }

    #[test]
    fn display_count_integer_or_decimal() {
        assert_eq!(
            format_display(Canonical {
                amount: 3.0,
                dimension: Dimension::Count
            }),
            "3"
        );
        assert_eq!(
            format_display(Canonical {
                amount: 1.5,
                dimension: Dimension::Count
            }),
            "1.5"
        );
    }
}
