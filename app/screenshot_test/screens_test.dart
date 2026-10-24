// Renders every screen of the real app, backed by the real engine and a real
// crawl file, to PNGs for design review.
//
// Not part of `flutter test`: it needs the host build of the Rust library and a
// crawl to point at.
//
//   cargo build -p crawlstat_bridge
//   cargo run -p crawlstat-engine --bin crawlstat-cli -- \
//       crawl https://example.com --max-urls 80 --out /tmp/cs/demo.db
//   flutter test screenshot_test --update-goldens
//
// The images land in screenshot_test/goldens/.
import 'dart:io';

import 'package:crawlstat/src/app.dart';
import 'package:crawlstat/src/router.dart';
import 'package:crawlstat/src/rust/api/project.dart' as project_api;
import 'package:crawlstat/src/rust/api/types.dart';
import 'package:crawlstat/src/rust/frb_generated.dart';
import 'package:crawlstat/src/screens/shell.dart';
import 'package:crawlstat/src/state/crawl.dart';
import 'package:crawlstat/src/state/projects.dart';
import 'package:crawlstat/src/state/settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

final demoPath = Platform.environment['CRAWLSTAT_DEMO_DB'] ?? '/tmp/cs/demo.db';
late ProjectInfo demoInfo;
late List<UrlRow> demoRows;

Future<void> _loadFont(String family, List<String> files) async {
  final loader = FontLoader(family);
  for (final file in files) {
    final bytes = File(file).readAsBytesSync();
    loader.addFont(Future.value(ByteData.view(bytes.buffer)));
  }
  await loader.load();
}

Future<void> _loadFonts() async {
  final sdk = Platform.environment['FLUTTER_ROOT']!;
  final material = '$sdk/bin/cache/artifacts/material_fonts';
  final roboto = [
    for (final weight in ['Regular', 'Medium', 'Bold', 'Light', 'Black'])
      '$material/Roboto-$weight.ttf',
  ];
  // Stand-ins for the macOS system font, which a test cannot load by name.
  for (final family in ['Roboto', '.AppleSystemUIFont', 'CupertinoSystemText', 'CupertinoSystemDisplay']) {
    await _loadFont(family, roboto);
  }
  await _loadFont('SF Mono', ['/System/Library/Fonts/SFNSMono.ttf']);
  await _loadFont('MaterialIcons', ['$material/MaterialIcons-Regular.otf']);
}

class _FakeRecents extends RecentProjects {
  @override
  List<ProjectInfo> build() => [demoInfo];

  @override
  Future<void> refresh() async {}
}

class _FakeOpen extends OpenProject {
  @override
  String? build() => demoPath;
}

class _FakeSettings extends SettingsController {
  _FakeSettings(this.mode);
  final ThemeMode mode;

  @override
  Settings build() => Settings(userAgent: 'CrawlStatBot/1.0 (+https://crawlstat.com/bot)', themeMode: mode);
}

class _FakeCrawl extends CrawlController {
  @override
  LiveCrawl build() {
    return LiveCrawl(
      phase: CrawlPhase.running,
      dbPath: demoPath,
      startUrl: demoInfo.startUrl,
      seeded: 1,
      snapshot: const ProgressSnapshot(
        state: 'running',
        crawled: 1284,
        queued: 3211,
        inFlight: 8,
        errors: 3,
        status4Xx: 17,
        status5Xx: 0,
        discovered: 4503,
        urlsPerSecond: 42.6,
        elapsedMs: 30500,
      ),
      rows: [
        for (final row in demoRows)
          CrawledUrlEvent(
            urlId: row.id,
            url: row.url,
            statusCode: row.statusCode,
            contentType: row.contentType,
            depth: row.depth,
            responseTimeMs: row.responseTimeMs,
            title: row.title,
          ),
      ],
      warnings: [
        CrawlWarning('robots.txt disallows /wp-admin/', null, DateTime(2026)),
        CrawlWarning('Slow response (2.4 s)', demoInfo.startUrl, DateTime(2026)),
      ],
    );
  }
}

Future<void> _shoot(
  WidgetTester tester, {
  required String name,
  required String route,
  required ThemeMode mode,
}) async {
  await tester.binding.setSurfaceSize(const Size(1440, 900));
  tester.view.devicePixelRatio = 1;

  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        recentProjectsProvider.overrideWith(_FakeRecents.new),
        openProjectProvider.overrideWith(_FakeOpen.new),
        settingsProvider.overrideWith(() => _FakeSettings(mode)),
        crawlControllerProvider.overrideWith(_FakeCrawl.new),
      ],
      child: const CrawlStatApp(),
    ),
  );
  await tester.pump();
  GoRouter.of(tester.element(find.byType(AppShell))).go(route);

  // Let the engine answer the screen's queries, then settle the frames.
  for (var i = 0; i < 6; i++) {
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 150)));
    await tester.pump(const Duration(milliseconds: 50));
  }

  await expectLater(
    find.byType(MaterialApp),
    matchesGoldenFile('goldens/$name-${mode.name}.png'),
  );
}

void main() {
  setUpAll(() async {
    TestWidgetsFlutterBinding.ensureInitialized();
    await RustLib.init(
      externalLibrary: ExternalLibrary.open('../target/debug/libcrawlstat_bridge.dylib'),
    );
    await _loadFonts();
    demoInfo = await project_api.projectInfo(dbPath: demoPath);
    final page = await project_api.listUrls(
      dbPath: demoPath,
      query: const UrlQuery(tab: 'internal', sortBy: 'url', descending: false, offset: 0, limit: 40),
    );
    demoRows = page.rows;
  });

  const screens = {
    'projects': Routes.projects,
    'crawl': Routes.crawl,
    'results': Routes.results,
    'issues': Routes.issues,
    'settings': Routes.settings,
  };

  for (final mode in [ThemeMode.dark, ThemeMode.light]) {
    for (final entry in screens.entries) {
      testWidgets('${entry.key} ${mode.name}', (tester) async {
        await _shoot(tester, name: entry.key, route: entry.value, mode: mode);
      }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));
    }
  }
}
