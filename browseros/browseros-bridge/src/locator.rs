use std::fmt;

/// Declarative strategy for finding elements in the DOM.
///
/// Each variant describes *what* to find, not *how* to find it.
/// The implementation (CDP, WebDriver, etc.) resolves the strategy.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocatorStrategy {
    /// CSS selector string.
    Css(String),
    /// XPath expression.
    XPath(String),
    /// Match by visible text content.
    Text { text: String, exact: bool },
    /// Match by ARIA role and optional accessible name.
    Role { role: String, name: Option<String> },
    /// Match by `data-testid` attribute (or equivalent).
    TestId(String),
    /// Match by `placeholder` attribute.
    Placeholder(String),
    /// Match by associated `<label>` text.
    Label(String),
    /// Match by `alt` attribute (images).
    AltText(String),
    /// Match by `title` attribute.
    Title(String),
    /// Parent strategy scoping a child strategy.
    Nested(Box<LocatorStrategy>, Box<LocatorStrategy>),
    /// All strategies must match (logical AND).
    And(Vec<LocatorStrategy>),
    /// Any strategy must match (logical OR).
    Or(Vec<LocatorStrategy>),
}

impl fmt::Display for LocatorStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Css(s) => write!(f, "css={s}"),
            Self::XPath(s) => write!(f, "xpath={s}"),
            Self::Text { text, exact } => {
                if *exact {
                    write!(f, "text=\"{text}\"")
                } else {
                    write!(f, "text~=\"{text}\"")
                }
            }
            Self::Role {
                role,
                name: Some(n),
            } => write!(f, "role={role}[name=\"{n}\"]"),
            Self::Role { role, name: None } => write!(f, "role={role}"),
            Self::TestId(s) => write!(f, "testid={s}"),
            Self::Placeholder(s) => write!(f, "placeholder={s}"),
            Self::Label(s) => write!(f, "label={s}"),
            Self::AltText(s) => write!(f, "alt={s}"),
            Self::Title(s) => write!(f, "title={s}"),
            Self::Nested(p, c) => write!(f, "{p} >> {c}"),
            Self::And(v) => {
                let strs: Vec<String> = v.iter().map(|s| s.to_string()).collect();
                write!(f, "({})", strs.join(" && "))
            }
            Self::Or(v) => {
                let strs: Vec<String> = v.iter().map(|s| s.to_string()).collect();
                write!(f, "({})", strs.join(" || "))
            }
        }
    }
}
