import 'package:crawlstat/src/rust/api/types.dart';
import 'package:crawlstat/src/screens/new_crawl_dialog.dart';
import 'package:crawlstat/src/state/crawl.dart';
import 'package:crawlstat/src/state/license.dart';
import 'package:crawlstat/src/state/settings.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// Settings without the engine's default user agent, which needs Rust.
class _Settings extends SettingsController {
  @override
  Settings build() => const Settings(userAgent: 'test');
}

CrawlRequest _earlier({
  String mode = 'spider',
  List<String> listUrls = const [],
  List<String> include = const [],
}) => CrawlRequest(
  startUrl: 'https://example.com/',
  mode: mode,
  listUrls: listUrls,
  maxUrls: 250,
  maxDepth: 3,
  concurrency: 9,
  crawlDelayMs: 0,
  requestTimeoutMs: 10000,
  respectRobots: true,
  userAgent: 'old agent',
  crawlSubdomains: false,
  checkExternalLinks: false,
  followNofollow: false,
  includePatterns: include,
  excludePatterns: const [],
  crawlImages: true,
  crawlCss: true,
  crawlJavascript: true,
  crawlDocuments: true,
  storeHtml: false,
  renderJavascript: true,
  runAudits: true,
  configJson: '{}',
);

Future<void> _pump(
  WidgetTester tester,
  CrawlRequest? from, {
  String? chrome =
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
}) async {
  tester.view.physicalSize = const Size(1200, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        settingsProvider.overrideWith(_Settings.new),
        freeTierLimitProvider.overrideWithValue(500),
        urlLimitProvider.overrideWithValue(null),
        chromePathProvider.overrideWithValue(chrome),
      ],
      child: MaterialApp(
        theme: CrawlStatTheme.dark(),
        home: Scaffold(body: NewCrawlDialog(from: from)),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

String _field(WidgetTester tester, int index) =>
    tester.widget<TextField>(find.byType(TextField).at(index)).controller!.text;

void main() {
  testWidgets('a blank dialog is a new crawl', (tester) async {
    await _pump(tester, null);
    expect(find.text('New crawl'), findsOneWidget);
    expect(_field(tester, 0), isEmpty);
  });

  testWidgets('recrawl with changes starts from the earlier settings', (
    tester,
  ) async {
    await _pump(tester, _earlier(include: const ['^https://example.com/a']));
    expect(find.text('Recrawl with changes'), findsOneWidget);
    expect(_field(tester, 0), 'https://example.com/');
    expect(_field(tester, 1), '250');
    expect(_field(tester, 2), '3');
    // The patterns would carry over unseen were the section left closed.
    expect(find.text('Fewer options'), findsOneWidget);
    expect(_field(tester, 3), '^https://example.com/a');
    // What the form does not show comes from the earlier crawl too.
    expect(find.textContaining('As before: 9 at a time'), findsOneWidget);
    expect(find.text('Change'), findsNothing);
  });

  testWidgets('a list crawl comes back as its list', (tester) async {
    await _pump(
      tester,
      _earlier(
        mode: 'list',
        listUrls: const ['https://a.test/', 'https://b.test/'],
      ),
    );
    expect(_field(tester, 0), 'https://a.test/\nhttps://b.test/');
    expect(find.text('More options'), findsOneWidget);
  });

  testWidgets(
    'rendering JavaScript is off for a new crawl and names the browser',
    (tester) async {
      await _pump(tester, null);
      expect(find.text('Render JavaScript'), findsOneWidget);
      expect(find.textContaining('in Google Chrome first'), findsOneWidget);
      final toggle = tester.widget<Switch>(find.byType(Switch));
      expect(toggle.value, isFalse);
      expect(toggle.onChanged, isNotNull);

      await tester.tap(find.byType(Switch));
      await tester.pumpAndSettle();
      expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
    },
  );

  testWidgets('a recrawl keeps rendering JavaScript', (tester) async {
    await _pump(tester, _earlier());
    expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
  });

  testWidgets('without a browser the switch says why and cannot be turned on', (
    tester,
  ) async {
    await _pump(tester, null, chrome: null);
    expect(find.textContaining('Needs Chrome'), findsOneWidget);
    expect(tester.widget<Switch>(find.byType(Switch)).onChanged, isNull);
  });
}
