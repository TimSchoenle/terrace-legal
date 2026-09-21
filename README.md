<!--
Generated from .github/templates/README.md.hbs — edit that file, not this one.

CI renders it on every pull request and commits the result back to the branch. A push to `main`
whose README.md does not match its template fails the `readme` job in
.github/workflows/docs.yml, which is a required check.

The payload is collected by TimSchoenle/actions/actions/common/readme-variables, which reads
Cargo.toml, merged over the output of one command:

    bash .github/scripts/readme-variables.sh

Every number this page quotes about itself — the tag its install snippets pin, the MSRV its badge
shows, the licence its last section names — comes from that payload, so the release pull request
is the commit that corrects them.

Nothing in this comment may contain a mustache that is not a real reference.
-->

# terrace-legal

Operator-configured legal documents: configuration, validation, HTTP serving and a Dioxus frontend.

[![Release](https://img.shields.io/github/v/release/TimSchoenle/terrace-legal?sort=semver)](https://github.com/TimSchoenle/terrace-legal/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/TimSchoenle/terrace-legal/ci.yml?branch=main&label=ci)](https://github.com/TimSchoenle/terrace-legal/actions/workflows/ci.yml)
[![Licence](https://img.shields.io/github/license/TimSchoenle/terrace-legal)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.94-blue)](Cargo.toml)

## What this is

An operator publishes a deployment's legal documents, such as terms of service, a privacy notice
and an imprint, by configuring them. The library owns the configuration types, validates them
into something that can be served, negotiates the language, answers conditional HTTP requests,
and renders the Markdown in a Dioxus frontend without an HTML string.

The text of each document is a configuration value. It arrives through `_FILE` indirection or
inline TOML, so [terrace-config](https://github.com/TimSchoenle/terrace-config) reads it, watches
the mounted file, and rebuilds the runtime when it changes. The library reads no files itself.

No legal text ships with the library. Each deployment brings its own.

## Quick start

```toml
[dependencies]
terrace-legal = { git = "https://github.com/TimSchoenle/terrace-legal", tag = "v0.2.0" }
```

Nest the configuration in your own, build a catalog wherever you load configuration, and refuse
the configuration when that fails:

```rust
use terrace_legal::{Catalog, Legal, LegalConfig};

#[derive(Deserialize, Describe)]
struct Config {
    #[serde(default)]
    #[config(nested)]
    legal: LegalConfig,
}

let catalog = Catalog::build(&config.legal).map_err(|issues| issues.with_prefix("legal"))?;
let legal = Legal::new(catalog);
```

## Table of contents

- [Crates](#crates)
- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [Configuration](#configuration)
- [Compatibility](#compatibility)
- [Contributing](#contributing)
- [Security](#security)
- [Licence](#licence)

## Crates

Every crate is versioned together and installed from one tag.

| Crate | What it holds | Builds for `wasm32` |
| --- | --- | --- |
| `terrace-legal-model` | Wire types, locale negotiation, `Accept-Language` parsing, consent evaluation | Yes |
| `terrace-legal` | `LegalConfig`, validation into a `Catalog`, digests and entity tags, the `Legal` handle, the consent store contract | No |
| `terrace-legal-axum` | `ETag`, `304`, `Vary` and `Cache-Control` responders, an optional router, consent routes and a layer that enforces them | No |
| `terrace-legal-dioxus` | A provider, hooks, link and page components, an acceptance sentence, a consent gate | Yes |
| `terrace-legal-markdown` | Markdown to Dioxus elements with a link allowlist and no HTML string | Yes |

Every edge of the dependency graph points at `terrace-legal-model`. `terrace-legal-dioxus` does not
depend on `terrace-legal`, so the configuration loader never reaches a browser bundle. CI checks
that with `cargo tree`.

## Features

- **Configuration errors arrive together.** `Catalog::build` reports every problem in one pass,
  and `ConfigIssues::with_prefix` turns `documents.terms.body.EN` into the key the operator wrote.
- Locale keys normalise, so `de_at`, `DE-at` and `de-AT` are one locale, and `de-AT` can be written
  as an environment variable at all.
- One function negotiates the body and the title, so a French title is never shown over a German
  text. A configured default locale decides before the first published one does.
- Responses carry a strong `ETag` over the whole representation, honour `If-None-Match` including
  weak and list forms, and send `Vary: Accept-Language`.
- The Markdown renderer produces elements, never a string. Raw HTML is shown as text, a link with
  a scheme outside `http`, `https`, `mailto` and `tel` is shown as its text, and an image is shown
  as its alt text.
- Consent is implemented and tested end to end behind a `consent` feature that is off by default:
  evaluation, a storage contract with a conformance suite, HTTP routes, a layer that blocks after
  a grace period, and a Dioxus gate.
- Four fuzz oracles run on every push, and a replay of every committed seed runs on a plain
  `cargo test`.

## Installation

```toml
[dependencies]
terrace-legal = { git = "https://github.com/TimSchoenle/terrace-legal", tag = "v0.2.0" }
terrace-legal-axum = { git = "https://github.com/TimSchoenle/terrace-legal", tag = "v0.2.0", features = ["router"] }
```

Cargo finds every package in the repository from one git URL. Pin the tag, not a branch, so that
every bump is a manifest edit that shows up in review.

The Dioxus pair goes in the frontend's own manifest:

```toml
[dependencies]
terrace-legal-dioxus = { git = "https://github.com/TimSchoenle/terrace-legal", tag = "v0.2.0" }
terrace-legal-markdown = { git = "https://github.com/TimSchoenle/terrace-legal", tag = "v0.2.0" }
```

| Feature | Crate | Effect |
| --- | --- | --- |
| `consent` | `terrace-legal-model`, `terrace-legal`, `terrace-legal-axum`, `terrace-legal-dioxus` | The consent keys, records, routes and gate |
| `utoipa` | `terrace-legal-model`, `terrace-legal-axum` | `ToSchema` and `IntoParams` on the wire types |
| `router` | `terrace-legal-axum` | `router::<S>()` for hosts without an `OpenAPI` document |
| `testing` | `terrace-legal` | `LegalFixture` and the consent conformance suite, for dev-dependencies |

Cargo features are additive. If any crate in a dependency graph turns on `consent`, the wire
types gain fields and a generated `OpenAPI` document changes. A host that must not change its API
should diff that document in CI.

## Usage

### Serving with axum

A host with an `OpenAPI` document keeps its two annotated handlers and calls the responders, since
an operation's tag and identifier are constants a generic handler cannot carry:

```rust
async fn legal_index(
    State(legal): State<Legal>,
    Query(params): Query<LegalParams>,
    headers: HeaderMap,
) -> Response {
    respond_index(&legal, &headers, params.lang).await
}
```

A host without one mounts `terrace_legal_axum::router::<S>()`. Both routes are public: a reader
needs the terms before they have an account, so classify them that way in your access rules.

### Showing documents in Dioxus

Implement four traits and mount `LegalProvider` once:

| Trait | Supplies |
| --- | --- |
| `LegalTransport` | Fetching the index and a document, over your own client |
| `LegalText` | Your words: known titles, headings, the "last updated" line |
| `LegalRouting` | A link to a hosted document's page, through your router |
| `LegalSkin` | The class for each part. No component writes a class name |

`LegalLinks`, `LegalDocumentPage` and `AcceptanceSentence` then read from the provider.

### Reloading

A text edit is a configuration change. A host that rebuilds its runtime on reload builds a fresh
`Legal` per generation. A host that keeps its state calls `Legal::replace`, which swaps the catalog
atomically. A configuration that fails validation leaves the previous one serving.

## Configuration

```toml
[legal]
default_locale = "en"

[legal.documents.terms]
updated = "2026-08-04"
title = { en = "Terms of Service", de = "Nutzungsbedingungen" }
order = 10
# The bodies arrive as mounted files:
#   APP_LEGAL__DOCUMENTS__TERMS__BODY__EN_FILE=/etc/app/legal/terms.en.md
#   APP_LEGAL__DOCUMENTS__TERMS__BODY__DE_FILE=/etc/app/legal/terms.de.md

[legal.documents.privacy]
order = 20
# A short document can be inline:
body.en = '''
# Privacy
…
'''

[legal.documents.imprint]
url = "https://example.org/impressum"
order = 30
```

| Rule | Why |
| --- | --- |
| A slug matches `^[a-z0-9][a-z0-9_-]{0,63}$` | It becomes a URL path segment, and `_` is there because a variable name cannot contain `-` |
| A document has exactly one of `body` and `url` | It is either served or linked |
| A body is not blank and is at most 1 MiB | A mounted file of the wrong kind is refused by key |
| A `url` is absolute `http` or `https`, with a host and no credentials | A link is published, so a password in it would be too |

`LegalConfig` and `LegalDocument` refuse a key they do not declare, so a misspelt `updatd` and a
`consent` key in a build without the `consent` feature both fail at boot, by name.

## Compatibility

| Library | axum | Dioxus | utoipa | terrace-config |
| --- | --- | --- | --- | --- |
| `v0.2.0` | 0.8 | 0.7 | 5 | v0.12.0 |

| | Supported |
| --- | --- |
| Rust | 1.94, checked by the `msrv` job |
| Platforms | Linux and Windows, both in the test matrix |
| Browsers | `wasm32-unknown-unknown`, checked for the three crates that ship there |

A major bump of axum, Dioxus, utoipa or terrace-config is a breaking release of this library.

## Contributing

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org): the type
decides both the changelog section and the version bump release-please proposes.

`README.md` is generated. Edit `.github/templates/README.md.hbs` instead. CI renders it on every
pull request and commits the result back to the branch, and a push to `main` whose `README.md`
does not match its template fails.

The gates a pull request has to pass are in [`.github/workflows/ci.yml`](.github/workflows/ci.yml).
Each runs locally:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets
cargo test --workspace --all-features
cargo test --workspace --no-default-features --features testing
cargo deny check
cd fuzz && cargo test          # replays every committed seed and corpus entry
```

## Security

[SECURITY.md](SECURITY.md) has the reporting instructions. Do not open a public issue for a
vulnerability.

## Licence

MIT. [LICENSE](LICENSE) has the terms.
