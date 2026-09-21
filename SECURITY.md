# Security policy

## Reporting a vulnerability

Do not open a public issue. Use GitHub's private vulnerability reporting on this repository,
under the Security tab, which opens an advisory only the maintainers can read:

<https://github.com/TimSchoenle/terrace-legal/security/advisories/new>

Include what you did, what happened, and which crates and features were compiled. A report against
`--all-features` and one against a build without `consent` are different reports, because the
wire types and the configuration surface differ.

## Supported versions

The crates are pre-1.0 and distributed as git dependencies, so there is no maintenance branch to
backport to. A fix lands on `main` and goes out in the next tag, and the remedy is to move the
`tag = "…"` in your manifest. Only the newest tag is supported.

## What a report is about

The library publishes text an operator wrote, to every visitor, and records that a person accepted
some of it. The failures that matter are:

- An operator's Markdown producing a script, an event handler, a remote fetch, or an `href` outside
  the allowlist in the rendered output.
- A slug or a locale reaching the filesystem. Files are named only by configuration, so any
  request that reads one is a defect.
- A consent recorded for a version or a digest that was not the text served.
- A way to bypass a `RequireConsent` exemption, or to make one apply to a path it does not cover,
  such as through a `..` segment.
- A cached response served to a reader who asked for another language.

Panics on hostile input are fuzzed rather than reported: `fuzz/` carries an oracle for header
parsing, locales, configuration and Markdown rendering, and a reproducing input is more useful as a
seed than as an advisory. Open a normal issue with the input if a committed seed does not already
cover it.
