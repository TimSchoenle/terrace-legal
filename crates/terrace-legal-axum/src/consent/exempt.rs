//! The routes a consent requirement never blocks.

/// The mount points the layer must never block, which only the host knows.
///
/// Every field is required, so a host cannot build the exemption set without naming each one.
/// Each is a path prefix such as `/v1/me/export`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mounts {
    /// Where the legal documents are served. A reader has to be able to read what they are asked
    /// to accept.
    pub legal: String,
    /// Where the consent routes are mounted. A subject has to be able to accept.
    pub consent: String,
    /// The route that ends a session.
    pub sign_out: String,
    /// The route that exports the subject's data. Access to one's data cannot depend on accepting
    /// new terms.
    pub export: String,
    /// The route that erases the account. Deletion cannot depend on accepting new terms either.
    pub erasure: String,
}

/// Why a path prefix cannot be an exemption.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "`{prefix}` is not a usable exemption: it has to start with `/`, name at least one segment, \
     and contain no `.` or `..` segment, query or whitespace"
)]
pub struct ExemptionError {
    prefix: String,
}

/// The path prefixes that a consent requirement never blocks.
///
/// A host builds it from [`Mounts`], which names the five routes that must always be reachable,
/// and may [`extend`](Self::extend) it. It cannot remove one, and a prefix of `/` is refused
/// because it would exempt everything.
///
/// A prefix covers the path equal to it and every path below it, so `/v1/me/export` covers
/// `/v1/me/export/csv` and not `/v1/me/exported`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exemptions {
    prefixes: Vec<String>,
}

impl Exemptions {
    /// Builds the default set from the five required mounts.
    ///
    /// # Errors
    ///
    /// [`ExemptionError`] when a mount is not a usable prefix.
    pub fn new(mounts: Mounts) -> Result<Self, ExemptionError> {
        let mut exemptions = Self {
            prefixes: Vec::with_capacity(5),
        };
        for prefix in [
            mounts.legal,
            mounts.consent,
            mounts.sign_out,
            mounts.export,
            mounts.erasure,
        ] {
            exemptions.add(prefix)?;
        }
        Ok(exemptions)
    }

    /// Adds a prefix to the set.
    ///
    /// # Errors
    ///
    /// [`ExemptionError`] when `prefix` is not a usable prefix.
    pub fn extend(mut self, prefix: impl Into<String>) -> Result<Self, ExemptionError> {
        self.add(prefix.into())?;
        Ok(self)
    }

    /// Returns the prefixes, the five defaults first.
    #[must_use]
    pub fn prefixes(&self) -> &[String] {
        &self.prefixes
    }

    /// Returns whether `path` is covered.
    ///
    /// A path with a `.` or `..` segment, in any percent-encoding of the dots, is never covered:
    /// it could name a route outside the prefix it starts with.
    #[must_use]
    pub fn covers(&self, path: &str) -> bool {
        if path.split('/').any(is_dot_segment) {
            return false;
        }
        self.prefixes.iter().any(|prefix| {
            path.strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
    }

    fn add(&mut self, prefix: String) -> Result<(), ExemptionError> {
        let trimmed = prefix.trim_end_matches('/');
        let usable = trimmed.starts_with('/')
            && trimmed.len() > 1
            && !trimmed.contains(['?', '#'])
            && !trimmed.chars().any(|c| c.is_whitespace() || c.is_control())
            && !trimmed
                .split('/')
                .skip(1)
                .any(|segment| segment.is_empty() || is_dot_segment(segment));
        if !usable {
            return Err(ExemptionError { prefix });
        }
        let trimmed = trimmed.to_owned();
        if !self.prefixes.contains(&trimmed) {
            self.prefixes.push(trimmed);
        }
        Ok(())
    }
}

fn is_dot_segment(segment: &str) -> bool {
    let decoded = segment.to_ascii_lowercase().replace("%2e", ".");
    decoded == "." || decoded == ".."
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mounts() -> Mounts {
        Mounts {
            legal: "/v1/legal".into(),
            consent: "/v1/me/consent".into(),
            sign_out: "/v1/auth/logout".into(),
            export: "/v1/me/export".into(),
            erasure: "/v1/me/erasure/".into(),
        }
    }

    /// Access to one's data, its deletion, signing out, reading the terms and accepting them must
    /// never depend on having accepted the terms.
    #[test]
    fn the_default_set_is_the_five_required_routes() {
        let exemptions = Exemptions::new(mounts()).expect("valid");
        assert_eq!(
            exemptions.prefixes(),
            [
                "/v1/legal",
                "/v1/me/consent",
                "/v1/auth/logout",
                "/v1/me/export",
                "/v1/me/erasure"
            ]
        );
    }

    #[test]
    fn a_prefix_covers_itself_and_everything_below_it() {
        let exemptions = Exemptions::new(mounts()).expect("valid");
        for path in [
            "/v1/legal",
            "/v1/legal/",
            "/v1/legal/terms",
            "/v1/me/consent/withdraw",
            "/v1/auth/logout",
            "/v1/me/export/csv",
            "/v1/me/erasure",
        ] {
            assert!(exemptions.covers(path), "path {path}");
        }
    }

    #[test]
    fn a_prefix_does_not_cover_a_sibling_that_shares_its_text() {
        let exemptions = Exemptions::new(mounts()).expect("valid");
        for path in [
            "/",
            "/v1",
            "/v1/me",
            "/v1/me/exported",
            "/v1/legality",
            "/v1/me/profile",
            "/v1/LEGAL",
            "v1/legal",
            "",
        ] {
            assert!(!exemptions.covers(path), "path {path}");
        }
    }

    /// A path such as `/v1/legal/../me/profile` starts with an exempt prefix and could be
    /// normalised by a proxy into a route that is not exempt.
    #[test]
    fn a_path_with_a_dot_segment_is_never_covered() {
        let exemptions = Exemptions::new(mounts()).expect("valid");
        for path in [
            "/v1/legal/../me/profile",
            "/v1/legal/./terms",
            "/v1/legal/%2e%2e/me/profile",
            "/v1/legal/%2E%2E/me/profile",
            "/v1/legal/.%2e/me/profile",
            "/v1/legal/%2e/terms",
        ] {
            assert!(!exemptions.covers(path), "path {path}");
        }
        assert!(
            exemptions.covers("/v1/legal/terms.md"),
            "a dot inside a name is fine"
        );
        assert!(exemptions.covers("/v1/legal/..terms"));
    }

    #[test]
    fn a_host_can_extend_the_set_but_not_shrink_it() {
        let exemptions = Exemptions::new(mounts())
            .expect("valid")
            .extend("/v1/health/")
            .expect("valid")
            .extend("/v1/legal")
            .expect("a repeat is not an error");
        assert_eq!(exemptions.prefixes().len(), 6);
        assert!(exemptions.covers("/v1/health"));
        assert!(exemptions.covers("/v1/legal/terms"));
    }

    #[test]
    fn a_prefix_that_would_exempt_everything_or_nothing_sensible_is_refused() {
        for prefix in [
            "",
            "/",
            "//",
            "v1/legal",
            "/v1//legal",
            "/v1/../legal",
            "/v1/legal?x=1",
            "/v1/legal#x",
            "/v1/ legal",
            "/.",
            "/%2e%2e/x",
        ] {
            assert!(
                Exemptions::new(mounts())
                    .expect("valid")
                    .extend(prefix)
                    .is_err(),
                "prefix {prefix:?}"
            );
        }
        let bad = Mounts {
            export: "/".into(),
            ..mounts()
        };
        assert!(
            Exemptions::new(bad).is_err(),
            "a required mount cannot be `/` either"
        );
    }
}
