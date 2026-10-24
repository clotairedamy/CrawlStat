# CrawlStat desktop app

The Flutter front end. It renders what the engine finds; it holds no crawl logic
of its own.

```
app/
  rust/            flutter_rust_bridge glue crate (crawlstat-bridge)
  lib/
    main.dart      loads the engine, then the app
    src/
      rust/        generated bindings — do not edit
      theme/       the design system
      state/       Riverpod providers
      screens/     one file per screen
      widgets/     the data grid and shared pieces
  test/            unit and widget tests
```

## Running it

```bash
flutter pub get
flutter run -d macos          # or -d windows, -d linux
flutter analyze
flutter test
```

After changing anything under `rust/src/api/`, regenerate the bindings:

```bash
flutter_rust_bridge_codegen generate
```

> **macOS note.** Building the desktop app needs full Xcode with its licence
> accepted (`sudo xcodebuild -license accept`). `flutter analyze` and
> `flutter test` only need the Command Line Tools; set
> `DEVELOPER_DIR=/Library/Developer/CommandLineTools` if `xcode-select` points
> at an Xcode whose licence is unaccepted.

## How it is put together

**The bridge is a data boundary, not a second brain.** `rust/src/api/` converts
engine types to plain structs the generator can carry, and does nothing else.
Scope decisions, limits and validation all live in the engine, so the CLI, the
app and the future CI tier behave identically. `CrawlRequest::to_config` is the
one place the UI's options become an engine config, and it validates there — a
bad regex or an unusable URL is reported to the dialog that collected it rather
than three frames later from a background task.

**The database is the data; the UI is a window.** The results table renders
`total` rows from SQLite's own count and fetches 200 at a time as you scroll,
keeping at most 20 pages in memory. A 500,000-row crawl scrolls in the same
memory as a small one. During a crawl the live table keeps only the last 300
URLs, because every one of them is already on disk.

**One crawl, one project, one runtime.** The bridge keeps a tokio runtime, the
running crawl's handle and the open project's connection as process state. That
matches the UI — you watch one crawl and read one crawl — and keeps Dart free of
object lifetimes across the FFI boundary.

**Navigating away does not cancel anything.** The crawl controller lives for the
app's lifetime, so you can look at the results of the last crawl while this one
runs. The rail shows a dot while it does.

### The design system

[`theme/theme.dart`](lib/src/theme/theme.dart) states the rules. In short:
borders rather than shadows, because a data grid wants crisp flat separation at
13px; monospaced numbers and URLs, so columns scan vertically; and a single
accent, because colour here carries meaning — severity and status class — and
spending it on decoration would make the meaningful uses invisible.

### Screens

| Screen | |
|---|---|
| **Projects** | Every crawl on this machine, with its counts and issue totals. Resume, open or delete. |
| **Crawl** | Live counters, URLs/sec, warnings, and the URLs as they arrive. Pause, resume, stop. |
| **Results** | Eleven tabs over one table — tabs are column presets, not separate queries — with a filter bar, a column chooser, sortable headers and CSV export. |
| **URL detail** | A side panel: overview, issues, inlinks, outlinks, images, headers. |
| **Issues** | The audit findings grouped by severity, with the fix text, and one click to the affected URLs. |
| **Settings** | Crawl defaults, user agent, resource policy, theme, shortcuts. |

### Keyboard

The results table is where a user spends an hour, so it is fully keyboard
driven: arrows and Page Up/Down move the cursor, Home/End jump to the ends,
Enter opens the detail panel, Esc closes it, ⌘F focuses the filter, ⌘E exports,
⌘N starts a crawl and ⌘Space pauses or resumes one.

## No native plugins

The app deliberately depends on no Flutter plugin with native code. Recent
projects, settings and export paths are handled with `dart:io` and the platform
directory conventions, which means the app builds with nothing but the Flutter
SDK — no CocoaPods, no per-platform plugin registration. The cost is that
"export" writes to a known directory and offers to copy the path instead of
opening a native save dialog; swapping in `file_selector` later is a contained
change.

## Not yet built

- **Site architecture graph** — the link graph is exported and queryable; the
  force-directed view is not drawn yet.
- **Crawl comparison** — a Pro feature, and the diff needs two open projects.
- **XLSX and the HTML audit report** — Pro exports; CSV is the free-tier one.
- **Custom extraction** (CSS/XPath/regex) and **scheduled crawls** — Pro, and
  both need engine work before any UI.
