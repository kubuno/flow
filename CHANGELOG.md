# Changelog

All notable changes to **kubuno-flow** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this
project adheres to [Semantic Versioning](https://semver.org/). Entries are added under
`[Unreleased]` **as the change is made**; `_tools/release.sh` stamps them under the version
number at release time, and CI publishes that section as the GitHub Release notes.

## [Unreleased]

### Changed

- **Flow now runs on PostgreSQL, MySQL/MariaDB or SQLite.** The module was tied
  to PostgreSQL; it now stores its workflows, jobs, executions, logs,
  credentials and trigger state on whichever database engine the administrator
  chooses, from the same build. The engine is read from configuration at
  start-up (`[database] engine = "postgres" | "mysql" | "sqlite"`), and the
  module creates and migrates its own namespace on first run. Existing
  PostgreSQL instances are migrated in place — a workflow's tags become a JSON
  array and an internal timestamp trigger is retired — with no change to stored
  data or behaviour. On a lightweight single-file (SQLite) or MySQL/MariaDB
  deployment, the event/form/chat triggers that rely on PostgreSQL's
  notification bus are inactive; time (cron), e-mail and SSE triggers, and every
  other feature, work on all three engines.

### Security

- **A webhook can no longer be used to rewrite a workflow's SQL.** A workflow
  that runs a query against an external database can write expressions like
  `{{ trigger.body.email }}` anywhere in its configuration, and those were
  expanded in the text of the SQL statement as well. On a workflow started by
  an incoming webhook — a public address, with no sign-in — the caller supplies
  that data, so anyone who knew the address could reshape the statement and run
  their own SQL against the database the workflow's owner had connected. The
  statement is now used exactly as it was written, and dynamic values go
  through the query's parameters, as the node's own help already advised. Only
  workflows that placed an expression inside the SQL text were exposed; those
  workflows will now run their statement literally and must move the value into
  the parameters list.
- **Concurrency library updated: the last advisory on this module is closed.**
  `crossbeam-epoch` moves to 0.9.21, fixing an invalid pointer dereference
  (RUSTSEC-2026-0204) reachable when a stale atomic pointer was formatted for a
  log line. This module now reports no known vulnerability at all.
- **Database driver updated past an unfixable advisory.** The previous line
  pulled in an RSA implementation vulnerable to a timing side-channel
  (RUSTSEC-2023-0071) for which no fix will ever exist. The new line does not
  depend on it at all, and it refuses any SQL string built at run time unless it
  has been audited — the queries here were checked and marked. The two that
  carry a workflow's own statement are marked because writing that statement is
  what the node is for; the fix above is what keeps a caller out of it.
- **Input validation library updated.** The version in use carried
  RUSTSEC-2024-0421 through its domain-name parser, which accepted Punycode
  labels that decode to plain ASCII — a mismatch an attacker can use to make two
  different names look like one.

## [0.1.8] - 2026-09-18

### Security

- **HTTP/2 layer updated to a patched release.** `h2` moves from 0.4.15 to
  0.4.19, closing a denial of service through unbounded empty DATA frames
  (RUSTSEC-2026-0258).
- **Error library updated to a patched release.** `anyhow` moves from 1.0.102
  to 1.0.104, closing an unsoundness in `Error::downcast_mut()`
  (RUSTSEC-2026-0190).
- **TLS library updated to a patched release.** The pinned `rustls` carried
  RUSTSEC-2026-0285 (medium). Every outbound HTTPS connection goes through it.

## [0.1.7] - 2026-09-18

### Changed



- **This module now installs as a Kubuno package (`.kbpkg`) only.** Its system
  packages (Debian/RPM and the Windows and macOS installers) are no longer
  built: the module is distributed as one `.kbpkg` per platform (Linux, Windows,
  macOS) that the Kubuno server installs itself — from the admin console, or
  offline with `kubuno modules:install <file>.kbpkg`.
- **Dates are formatted by the platform now, not by a library.** `date-fns` is
  gone from this module: the shared SDK exposes helpers built on `Intl`, which is
  localised for every language we ship and needs no locale bundle loaded. Call
  sites say what a date is FOR — `formatDate(d, 'date')` — and the platform
  decides how to write it, so a reader in Japanese no longer gets a French
  layout. Machine formats (keys, `<input type="date">` values) go through
  `toISODate` and friends, built from local calendar fields so the day cannot
  shift near midnight.
- **Class names are composed by `cn()` from `@ui`**, replacing `clsx`. One less
  dependency for a dozen lines of finished logic; call sites are unchanged.
- **The README now opens with the module's logo.** The public README on
  GitHub now shows the module's designer logo (the same PNG shown as the
  browser tab icon and in the applications menu) at the top of the page — the
  repository landing now matches the icon a signed-in user sees inside the
  platform. The image ships in-repo, under `.github/logo.png`, so it renders
  even when the repo is browsed offline.

- **New Flow logo** — a gold hexagon with an "F" and a small flowchart, used
  as the browser-tab icon and in the applications menu. It is now raster (PNG)
  designer artwork.




### Fixed


- **A withdrawn dependency is no longer used.** A crate deep in the tree
  (`spin` 0.9.8, pulled in through the HTTP stack) was yanked by its authors.
  No vulnerability was announced, but a withdrawn crate has no business in a
  release; the lockfile now takes the version that replaced it.
- **The package could not be built where `zip` is absent.** The Windows job of
  the continuous integration has no `zip`, so the Windows package was simply lost
  the first time it was attempted — a script failure, not a build failure. The
  builder now falls back to 7-Zip, then to PowerShell.
### Added

- **This module now ships a `.kbpkg`** — the single package format a Kubuno
  server installs by itself, the same file on Linux, Windows and macOS. It
  carries the same binary, interface and manifest as the system packages,
  arranged the way the server expects to find a module on disk, plus a
  `SHA256SUMS` so a copy carried offline can be checked without the catalogue.
  Nothing changes for existing installations: the `.deb`, `.rpm`, `.exe` and
  `.pkg` are still published, and a catalogue that sees both simply prefers the
  new one. It is also the only format the server can unpack without an external
  tool, which is what makes one-click installation possible away from
  Debian-like systems.
### Fixed

- **A built package could be thrown away instead of published.** The job that
  attaches a package to the release waited ten minutes for another workflow to
  create that release, then gave up with "release never appeared — build.yml
  likely failed". The diagnosis was wrong: on a repository whose `.deb` takes
  longer than ten minutes to build, the release simply did not exist yet, and a
  package that had built perfectly was discarded. Four modules reached v0.1.6
  with packages missing for some systems because of it. The job now creates the
  release itself when it is missing, so it no longer depends on another workflow
  finishing first.
### Added

- **Security policy and CI quality gate.** A `SECURITY.md` documents how to
  report vulnerabilities, and a CI workflow enforces `clippy -D warnings`, a
  dependency-vulnerability audit (`cargo audit`) and the frontend typecheck/tests.

### Security

- **Flow now authenticates proxied requests from a signed token instead of
  trusting plain headers.** Requests must carry a valid `X-Kubuno-Auth` token
  minted by the core with this module's internal secret (see `kubuno-modauth`),
  rather than reading `X-Kubuno-User-*` headers at face value — which any process
  reaching Flow's loopback port could otherwise forge to act as any user.

### Added

- **The workflow list can now show the trash** (`?trashed=true`), so tools auditing
  which Drive files still belong to a workflow can see the trashed ones instead of
  mistaking their files for leftovers.

### Fixed

- **Deleting a workflow now really deletes it.** A deleted workflow disappeared
  from the list but was only flagged as removed: its file stayed in Drive and
  still opened, bringing back something you believed gone. The workflow, its file
  and its execution history are now removed together.

## [0.1.6] - 2026-08-19

### Changed

- **Pill-shaped buttons are gone from the interface.** Filter chips, view
  segments, tab selectors and action buttons that were drawn as pills now use the
  same 4 px corner radius as every other button — the shape set them apart for no
  reason other than habit. Round buttons that hold a lone icon, avatars, status
  dots and non-clickable badges keep their shape: a circle around a single glyph
  is not a pill.

- Theme tokens: two colours for navigation labels (`--color-text-nav`,
  `--color-text-nav-active`). Every module carries the same token sheet, so the
  values must match across them — whichever bundle loads last would otherwise
  win. No visible change inside this module.

### Changed

- Default application background token aligned with the core (`--body-bg` `#f8fafd`). Only
  visible when the module runs standalone: inside the shell the active theme sets it.

[Unreleased]: https://github.com/kubuno/flow/compare/v0.1.8...HEAD
[0.1.8]: https://github.com/kubuno/flow/releases/tag/v0.1.8
[0.1.7]: https://github.com/kubuno/flow/releases/tag/v0.1.7
[0.1.6]: https://github.com/kubuno/flow/releases/tag/v0.1.6
