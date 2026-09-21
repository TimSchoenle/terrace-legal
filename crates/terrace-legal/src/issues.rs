//! The complaints a refused configuration reports.

use std::fmt;

/// One thing wrong with a configuration, at one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    path: Vec<String>,
    message: String,
}

impl ConfigIssue {
    /// Creates an issue at `path`, given as the key segments from the section root.
    pub fn new<I, S>(path: I, message: impl Into<String>) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            path: path.into_iter().map(Into::into).collect(),
            message: message.into(),
        }
    }

    /// Returns the issue with `prefix` prepended to its key.
    pub(crate) fn under<'a>(mut self, prefix: impl IntoIterator<Item = &'a str>) -> Self {
        let mut path: Vec<String> = prefix.into_iter().map(str::to_owned).collect();
        path.append(&mut self.path);
        self.path = path;
        self
    }

    /// Returns the key segments, for example `["documents", "terms", "body", "EN"]`.
    #[must_use]
    pub fn path(&self) -> &[String] {
        &self.path
    }

    /// Returns what is wrong, without the key.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the key segments joined with `.`.
    #[must_use]
    pub fn key(&self) -> String {
        self.path.join(".")
    }
}

impl fmt::Display for ConfigIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.key(), self.message)
        }
    }
}

/// Every issue found in one configuration, so an operator can fix the whole file in one pass.
///
/// The library cannot know which section a host nests [`crate::LegalConfig`] under. Call
/// [`Self::with_prefix`] with that section to show full key paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssues {
    issues: Vec<ConfigIssue>,
}

impl ConfigIssues {
    pub(crate) fn from_vec(issues: Vec<ConfigIssue>) -> Option<Self> {
        if issues.is_empty() {
            None
        } else {
            Some(Self { issues })
        }
    }

    /// Prepends `prefix` to the key of every issue. The prefix may hold several segments
    /// separated by `.`.
    #[must_use]
    pub fn with_prefix(mut self, prefix: &str) -> Self {
        let head: Vec<String> = prefix
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(str::to_owned)
            .collect();
        for issue in &mut self.issues {
            let mut path = head.clone();
            path.append(&mut issue.path);
            issue.path = path;
        }
        self
    }

    /// Iterates the issues in the order they were found.
    pub fn iter(&self) -> std::slice::Iter<'_, ConfigIssue> {
        self.issues.iter()
    }

    /// Returns how many issues were found. It is never zero.
    #[must_use]
    pub fn len(&self) -> usize {
        self.issues.len()
    }

    /// Returns whether there are no issues, which never holds for a returned error.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }
}

impl<'a> IntoIterator for &'a ConfigIssues {
    type Item = &'a ConfigIssue;
    type IntoIter = std::slice::Iter<'a, ConfigIssue>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl fmt::Display for ConfigIssues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the legal configuration has {} problem",
            self.issues.len()
        )?;
        if self.issues.len() != 1 {
            f.write_str("s")?;
        }
        for issue in &self.issues {
            write!(f, "\n  - {issue}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigIssues {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_is_prepended_to_every_key() {
        let issues = ConfigIssues::from_vec(vec![
            ConfigIssue::new(["documents", "terms"], "bad"),
            ConfigIssue::new(Vec::<String>::new(), "root"),
        ])
        .expect("not empty")
        .with_prefix("app.legal");
        let keys: Vec<_> = issues.iter().map(ConfigIssue::key).collect();
        assert_eq!(keys, ["app.legal.documents.terms", "app.legal"]);
    }

    #[test]
    fn display_lists_every_issue() {
        let issues = ConfigIssues::from_vec(vec![
            ConfigIssue::new(["a"], "one"),
            ConfigIssue::new(["b"], "two"),
        ])
        .expect("not empty");
        assert_eq!(
            issues.to_string(),
            "the legal configuration has 2 problems\n  - a: one\n  - b: two"
        );
    }

    #[test]
    fn no_issues_is_not_an_error() {
        assert!(ConfigIssues::from_vec(Vec::new()).is_none());
    }
}
