# CrawlStat

A desktop SEO crawler: faster, lighter and better designed than the incumbents.

Everything runs on the user's machine. The crawl engine is a Rust library, the
app is Flutter, and the only hosted parts are the marketing site and a handful
of tiny serverless routes. **No server is ever required to crawl.**

## Layout

```
crawlstat/
  engine/        Rust crate: crawler core + `crawlstat-cli` binary
  app/           Flutter desktop app (macOS, Windows, Linux)
  app/rust/      flutter_rust_bridge glue crate
  site/          Nuxt 3 site for crawlstat.com, deployed to Vercel — Phase 6
  .github/       CI: tests, builds, signed installers, releases   — Phase 7
```

## Phases

| Phase | Scope | State |
|------:|-------|-------|
| 1 | Crawl engine + CLI | **done** |
| 2 | Audits — 35 checks, link score, duplicate detection | **done** |
| 3 | Flutter desktop app | **done** |
| 4 | Licensing and free tier | **done** |
| 5 | BYOK integrations | local AI **done**; PageSpeed/GSC key storage only |
| 6 | crawlstat.com (pages; the API routes already exist) | **done** |
| 7 | Distribution | installers **done**, code signing pending |

## Getting started

The engine needs a Rust toolchain (see `rust-toolchain.toml`).

```bash
cargo test                                  # 228 unit + integration tests
cargo run -p crawlstat-engine --bin crawlstat-cli -- \
    crawl https://example.com --max-urls 500 --out crawl.db
cargo run -p crawlstat-engine --bin crawlstat-cli -- issues crawl.db
cargo run -p crawlstat-engine --bin crawlstat-cli -- plan crawl.db
```

## The plan, in one paragraph

Every crawler ends at the same place: ten thousand rows sorted by severity, and
no answer to the only question anyone has — what do I fix first? CrawlStat
ranks the findings instead. Each job is scored on severity, on the share of the
site's internal link value sitting on the affected pages, and on how many pages
a single change fixes, which it reads from how many *distinct* findings are
behind them. On a real 1,929-page crawl the top job was an accessibility error
on 823 pages that turned out to be **two template changes**, and near-duplicate
content across 371 pages sank to eighth because it is 278 separate pieces of
work. `crawlstat-cli plan --fail-above 60` exits non-zero, which is the seed of
the CI tier. A local model (Ollama, on loopback) can write the ranking up in
plain English; nothing is uploaded, and the exact brief is shown on request.
The score is relative to one crawl, and deliberately not a traffic forecast —
see [`engine/src/plan.rs`](engine/src/plan.rs).

For the app:

```bash
cd app && flutter run -d macos
```

See [`engine/README.md`](engine/README.md) for the engine's architecture, the
database schema and the full CLI, and [`app/README.md`](app/README.md) for how
the UI and the `flutter_rust_bridge` layer fit together.

## Directional UX intelligence

```bash
cargo run --bin crawlstat-cli -- ux https://example.com --persona task_oriented
```

Predicts the order a reader attends to things in, how attention is shared, and
where the page loses them: actions competing for the same decision, a primary
action buried past the point anyone is still reading, a page with no onward
link, links whose text says nothing. Four personas — task-oriented, explorer,
sceptical, mobile-rushed — weigh the same markup differently, which is the
point: a page that works for one can fail another.

**It has not seen the page.** There is no browser in CrawlStat, so there is no
layout, no pixel positions, no contrast and no screenshot. Everything comes out
of the markup. What keeps that honest is that the base ordering is not invented:
document order is exactly what a screen reader speaks and what Tab follows, so
the predicted path is *literally correct* for keyboard and assistive-technology
users and a reasonable prior for everyone else. A page whose source order fights
its visual order will be described wrongly. Treat the output as a hypothesis to
test, never as a measurement, and never as a substitute for watching a real
person use the thing. See [`engine/src/ux/mod.rs`](engine/src/ux/mod.rs).

## Shipping a build

```bash
git tag v1.0.0 && git push origin v1.0.0
```

That runs [`release.yml`](.github/workflows/release.yml), which builds on all
three platforms and publishes the installers to GitHub Releases: a `.dmg` for
macOS, an Inno Setup `setup.exe` for Windows, and an AppImage plus a `.deb` for
Linux, each with a SHA-256 file. Use the workflow's **Run workflow** button to
produce a test build without tagging.

The packaging scripts in [`packaging/`](packaging) run locally too, which is
how to check a change without pushing:

```bash
./packaging/macos/make-dmg.sh 1.0.0      # macOS, needs Xcode
./packaging/linux/make-packages.sh 1.0.0 # Linux, needs ninja + libgtk-3-dev
```

**Signing is optional and absent by default.** Unsigned builds install and run
on all three platforms, but macOS and Windows each show a one-time warning the
user has to click past. Adding the repository secrets listed in
`release.yml` makes those warnings go away; nothing else changes.

## Licensing in one paragraph

The payment provider is the source of truth for who has paid. A few Vercel
functions proxy it and sign a short-lived entitlement token with an Ed25519 key;
the app embeds the public key and verifies tokens offline. So there is no
database to run, the app works on a plane, and ten thousand customers cost a few
hundred thousand function invocations a month. Limits are enforced by the Rust
engine reading its own licence state, never by the UI hiding a button. See
[`site/README.md`](site/README.md) and the licensing section of
[`engine/README.md`](engine/README.md).

## Principles

- **Local first.** Crawl data never leaves the machine, except through BYOK
  integrations the user triggers themselves.
- **Polite by default.** `robots.txt` is respected, requests are paced per host,
  and the crawler identifies itself as
  `CrawlStatBot/1.0 (+https://crawlstat.com/bot)`.
- **Flat memory.** Crawl state lives in SQLite, not in RAM, so crawl size is a
  question of time and disk rather than of memory.
- **Simple over clever.** Plain, documented code; the engine stays reusable so
  "CrawlStat CI" can ship as another tier.
