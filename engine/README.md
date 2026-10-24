# crawlstat-engine

The CrawlStat crawl engine: a Rust library plus a `crawlstat-cli` binary. It
fetches a site, parses every page, and writes everything into one SQLite file.
No server, no network service, no shared state.

The same library backs the desktop app (through `flutter_rust_bridge`, Phase 3)
and the planned CrawlStat CI tier, so the CLI is not a toy — it is the engine
with a different front end.

## Quick start

```bash
# Crawl a site
cargo run --bin crawlstat-cli -- crawl https://example.com --max-urls 500 --out crawl.db

# Summarise it
cargo run --bin crawlstat-cli -- report crawl.db

# List rows, with filters
cargo run --bin crawlstat-cli -- urls crawl.db --status 404
cargo run --bin crawlstat-cli -- urls crawl.db --contains /blog --limit 100

# Other seeds
cargo run --bin crawlstat-cli -- sitemap https://example.com/sitemap.xml --out crawl.db
cargo run --bin crawlstat-cli -- list urls.txt --out crawl.db

# Continue an interrupted crawl (Ctrl-C is safe: it finishes in flight and stops)
cargo run --bin crawlstat-cli -- resume crawl.db
```

`--json` prints one progress event per line, which is what CI integrations and
scripts should use. `RUST_LOG=crawlstat_engine=debug` turns on engine tracing.

## Architecture

```
            ┌──────────────┐  claim   ┌───────────┐  spawn   ┌───────────┐
            │  crawl loop  │─────────▶│  frontier │          │  visit ×N │
  control ─▶│  (engine)    │◀─────────│  (SQLite) │◀─────────│  (tokio)  │
            └──────┬───────┘  batch   └───────────┘  results └─────┬─────┘
                   │ progress events                              │ fetch
                   ▼                                              ▼
            CLI / Flutter StreamSink                       Fetcher (trait)
```

| Module | What it does |
|---|---|
| `config` | `CrawlConfig`: plain, serialisable data. Stored in the crawl file so a crawl can be resumed or inspected. |
| `url_norm` | Two normalisations: the URL we fetch (safe only) and the dedup key (policy-driven). |
| `fetch` | `Fetcher` trait, a `reqwest`/rustls implementation, and a mock. Redirects are followed by hand so chains and loops are recorded. |
| `robots` | Per-origin `robots.txt` cache, `Crawl-delay`, `Sitemap:` discovery. |
| `parse` | One pass over the HTML for every field we record. Infallible by design. |
| `sitemap` | `<urlset>`, `<sitemapindex>`, gzip, bounded. |
| `store` | SQLite: schema, batched writes on one blocking thread, paginated reads. |
| `engine` | The crawl loop, per-host politeness, seeding, per-URL visits. |
| `progress` | The event stream the CLI and the UI both consume. |
| `hash` | BLAKE3 content hashing and a 64-bit simhash for near-duplicates. |

### Three decisions worth knowing

**The dedup key is not the URL.** `normalize` applies only semantically safe
changes to the URL it will request (lowercased host, no fragment, no default
port, dot segments resolved). Policy — trailing slashes, tracking parameters,
`index.html` — only affects the *key* the frontier dedups on. So the first form
of a URL that is seen is the form that is fetched and shown, while later
variants of the same page are skipped.

**Redirects are followed manually.** `reqwest` can follow them, but it will not
tell you what it passed through. For an SEO crawler the chain *is* the finding,
so the client is configured with no redirect policy and the fetcher walks the
chain itself, recording each hop and detecting loops. A URL row's
`status_code` is therefore the 3xx the URL itself returned;
`final_status_code` is what the end of the chain returned.

**The database is the frontier.** There is no in-memory queue or seen-set: the
`urls` table is both, with a unique index on the dedup key doing the
deduplication. That is what makes a crawl resumable after a crash, and what
keeps memory flat (see below).

## The crawl database

One SQLite file per crawl, WAL mode. `urls` is one wide row per URL — its
frontier state and, once fetched, its response and page fields — so the UI's
main table is a single indexed query with no joins. Everything repeating lives
in a child table keyed by `url_id`: `links`, `headers`, `images`, `headings`,
`hreflang`, `social_tags`, `structured_data`, `redirects`. Plus `sitemap_urls`
(what the site declared) and `crawl_log` (notices).

`links.target_key` holds the *dedup key* of the target rather than a row id, so
a page's links can be written in one batch without first looking up ids for
targets that may not exist yet. Reads join on `urls.dedup_key`, which is a
unique index.

Indexes needed during a crawl are created up front; the ones only used for
analysis are built by `finalize()`, so they do not slow down several hundred
thousand inserts.

## Audits

After a crawl the engine turns the data into a prioritised issue list. The pass
is driven by a catalogue of **35 audits** in
[`src/audit/catalog.rs`](src/audit/catalog.rs), each with a stable id, a
severity, a description, how-to-fix text and a `docs_slug`. That text is written
to be shown verbatim in the app *and* to become the body of
`crawlstat.com/docs/audits/<slug>`, so each audit doubles as an SEO landing page.

```bash
cargo run --bin crawlstat-cli -- issues crawl.db
cargo run --bin crawlstat-cli -- issue crawl.db redirect_chain
cargo run --bin crawlstat-cli -- urls crawl.db --issue title_duplicate
cargo run --bin crawlstat-cli -- audit crawl.db --title-max 70 --slow-ms 500
```

Audits run automatically when a crawl finishes (`--no-audit` to skip) and can be
re-run at any time — the checks and their explanations live in the binary, not
in the crawl file, so upgrading improves what an old crawl reports.

| | |
|---|---|
| **Errors** | 5xx, 4xx, fetch failures, pages linking to broken URLs, redirect loops, canonicals pointing at non-200 URLs, mixed content |
| **Warnings** | redirect chains, internal links to redirects, missing/duplicate titles and descriptions, missing H1, `noindex` pages receiving internal links, robots-blocked pages that are linked, hreflang return links and self-references, duplicate content, images without alt text, orphan pages, HTTP URLs on an HTTPS site |
| **Notices** | title and description lengths, multiple H1s, missing canonicals, canonicalised and `noindex` pages, near-duplicate content, oversized images, slow responses, buried pages |

Results go into two tables: `issues` (one row per issue type with a count) and
`issue_urls` (the affected URLs with a short per-URL detail). `UrlFilter`'s
`issue_id` is how clicking an issue filters the URL table.

### Three things worth knowing

**Indexability is computed once.** Half the audits only make sense for pages a
search engine could actually index, so `urls.indexable` and
`indexability_reason` are derived in one pass rather than re-stated as four
conditions in every query. There is no point nagging about the title of a
`noindex` page.

**Link score is PageRank in SQL.** Followed `<a href>` links between internal
200 HTML pages, deduplicated per pair, damping 0.85, rescaled so the strongest
page on the site is 100. It runs as indexed passes over a materialised edge
table rather than an in-memory graph: the edge list grows with the crawl, and
holding it would break the flat-memory property.

**Near-duplicates use simhash banding.** Each 64-bit hash is split into four
16-bit bands, and only pages matching on a band are compared — two hashes
within the threshold must agree on at least one band, so nothing is missed.
Pages shorter than 200 words are left alone, because simhash distance gets
unreliable on thin content; those are caught by exact text hash instead. A page
that is an exact duplicate is reported as that and not also as a near one, but
it can still be another page's match.

## Licensing

```bash
cargo run --bin crawlstat-cli -- license status
cargo run --bin crawlstat-cli -- license activate CS-XXXX-XXXX
cargo run --bin crawlstat-cli -- license refresh
```

[`src/license.rs`](src/license.rs) holds the whole client side: token
verification, the grace period, and enforcement. Three things are worth knowing.

**Enforcement is in the engine, not the UI.** `Crawler::assemble` clamps
`max_urls` by reading the process-wide licence state, so the app, the CLI, a
hand-built `CrawlConfig` and a future CI runner all get the same limit. The
clamped value is what gets stored with the crawl, so resuming one stays limited
too. The default state is the free tier, which means a build that forgets to
load a licence is correctly limited rather than accidentally unlimited — the
direction a mistake here should fail in.

**A network failure never interrupts work.** A token is valid for a fortnight
and refreshed about once a day; failures retry with an exponential backoff
capped at a day. A paying customer on a plane keeps their features, and a crawl
already running is never re-checked. Only after fourteen days of failed
refreshes does the install fall back to the free tier — and "falls back" means
exactly that: it keeps working, with the free limit.

**Features, not the plan name, decide what is allowed.** A token grants a list
of capabilities; `max_urls_per_crawl` asks for `unlimited_urls` rather than
asking whether the plan is called Pro. A partially refunded or downgraded
licence therefore cannot grant more than it should, and the server can grant a
capability a shipped build has never heard of without breaking it.

### The secret never reaches Dart

The licence key and the signed token live in the OS keychain
([`src/license/storage.rs`](src/license/storage.rs)) and are read only by Rust.
The app asks "am I licensed?" and gets a status. The brief suggested
`flutter_secure_storage`, which would work; doing it here means a UI bug cannot
leak either secret, neither appears in a Flutter crash dump, and the app needs
no native plugin at all.

### The contract with the API

Tokens are signed by `site/server/utils/token.ts` and verified here — two
languages, two base64 implementations, two JSON serialisers. If any of them
disagrees, every activation fails in production.
[`tests/license_contract.rs`](tests/license_contract.rs) verifies a committed
token that the real Node code actually signed, so a format drift fails the build
instead of the customer.

## Testing

```bash
cargo test                     # 258 unit + integration tests
cargo clippy --all-targets     # clean, and CI treats warnings as errors
cargo fmt --all --check
```

Integration tests run against a local `axum` fixture site
(`tests/fixture/mod.rs`) with redirect chains, a redirect loop, 404s and 500s,
a slow route, canonicals, `noindex`, `X-Robots-Tag`, a robots-disallowed path,
a sitemap index, duplicate and near-duplicate pages, an oversized body, images,
CSS, a PDF and a three-page hreflang group with a missing return link, a missing
self-reference and an invalid region code. `/page/{n}` generates a binary tree of
pages for volume work.

[`tests/audit_fixture_site.rs`](tests/audit_fixture_site.rs) crawls that site and
audits the result, so the audits are proven against real HTTP responses parsed
by the real parser — not just against hand-inserted rows.

## Performance

```bash
cargo test --release --test perf_memory     -- --ignored --nocapture
cargo test --release --test perf_throughput -- --ignored --nocapture
cargo test --release --test perf_flatness   -- --ignored --nocapture

# One crawl in a fresh process, with a memory curve over time
cargo run --release --example memory_profile -- 50000 20
```

Each measurement needs its own process: `ru_maxrss` is a per-process
high-water mark, so two crawls in one process cannot be compared.

Measured on an Apple Silicon laptop against the loopback fixture site, so these
are engine-bound rather than network-bound numbers:

| Crawl | Concurrency | Throughput | Peak RSS | Database |
|---|---|---|---|---|
| 2,000 | 1 | 3,900 URLs/sec | 21 MiB | 4.6 MiB |
| 2,000 | 5 | 8,200 URLs/sec | 22 MiB | 4.6 MiB |
| 2,000 | 20 | 11,000 URLs/sec | 25 MiB | 4.7 MiB |
| 30,000 | 10 | 9,600 URLs/sec | 111 MiB | 69 MiB |
| **50,000** | **20** | **11,700 URLs/sec** | **128 MiB** | **100 MiB** |
| 60,000 | 10 | 10,600 URLs/sec | 131 MiB | 114 MiB |

The acceptance bar was 50,000 URLs under ~500 MB: it uses **128 MiB**, and
doubling the crawl from 30,000 to 60,000 URLs costs **1.19×** the peak memory
(717 bytes of RAM per additional URL). Memory is dominated by SQLite's page
cache, which is a fixed 64 MiB, so it plateaus rather than growing — crawl size
is a question of time and disk, about 2 KB per URL on disk.

On a real site, throughput is bounded by the network and by politeness settings,
not by the engine. With the default 5 concurrent requests and no crawl delay,
expect tens of URLs per second against a typical server.

## Not in the engine

- **JavaScript rendering.** Out of scope for v1, but `Fetcher` is the seam: a
  `chromiumoxide` implementation drops in without the engine changing.
- **Mixed content end to end.** The `mixed_content` and `http_url_on_https_site`
  audits are unit-tested only: the fixture site is loopback HTTP, so proving
  them against a real server would mean running TLS in the test harness.
