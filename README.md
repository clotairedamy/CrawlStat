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
| 5 | BYOK integrations | |
| 6 | crawlstat.com (pages; the API routes already exist) | |
| 7 | Distribution | |

## Getting started

The engine needs a Rust toolchain (see `rust-toolchain.toml`).

```bash
cargo test                                  # 228 unit + integration tests
cargo run -p crawlstat-engine --bin crawlstat-cli -- \
    crawl https://example.com --max-urls 500 --out crawl.db
cargo run -p crawlstat-engine --bin crawlstat-cli -- issues crawl.db
```

For the app:

```bash
cd app && flutter run -d macos
```

See [`engine/README.md`](engine/README.md) for the engine's architecture, the
database schema and the full CLI, and [`app/README.md`](app/README.md) for how
the UI and the `flutter_rust_bridge` layer fit together.

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
