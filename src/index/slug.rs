use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Slug(String);

impl Slug {
    pub fn from_base(base: &str) -> Self {
        let mut s = String::with_capacity(base.len());
        for ch in base.chars() {
            let c = ch.to_ascii_lowercase();
            match c {
                ' ' | '_' => s.push('-'),
                _ => s.push(c),
            }
        }
        Slug(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases() {
        assert_eq!(Slug::from_base("Carbonara").as_str(), "carbonara");
    }

    #[test]
    fn replaces_spaces_and_underscores() {
        assert_eq!(
            Slug::from_base("Spicy Tomato_Sauce").as_str(),
            "spicy-tomato-sauce"
        );
    }
}
