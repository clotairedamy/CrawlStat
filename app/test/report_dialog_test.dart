import 'package:crawlstat/src/rust/api/types.dart';
import 'package:crawlstat/src/screens/projects_screen.dart';
import 'package:crawlstat/src/state/crawl.dart';
import 'package:crawlstat/src/state/license.dart';
import 'package:crawlstat/src/state/projects.dart';
import 'package:crawlstat/src/state/settings.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// Settings without the engine's default user agent, which needs Rust.
class _Settings extends SettingsController {
  _Settings(this.initial);

  final Settings initial;

  @override
  Settings build() => initial;
}

class _Recent extends RecentProjects {
  _Recent(this.crawls);

  final List<ProjectInfo> crawls;

  @override
  List<ProjectInfo> build() => crawls;
}

class _Written extends WrittenReports {
  _Written(this.reports);

  final Map<String, String> reports;

  @override
  Map<String, String> build() => reports;
}

ProjectInfo _crawl(String path, String createdAt) => ProjectInfo(
  path: path,
  rootHost: 'shop.example',
  startUrl: 'https://shop.example/',
  createdAt: createdAt,
  state: 'finished',
  stats: const ProjectStats(
    total: 100,
    queued: 0,
    crawled: 100,
    failed: 0,
    skipped: 0,
    blockedByRobots: 0,
    status2Xx: 96,
    status3Xx: 2,
    status4Xx: 2,
    status5Xx: 0,
  ),
  errors: 1,
  warnings: 4,
  sizeBytes: 0,
);

final _latest = _crawl('/tmp/latest.crawlstat', '2026-10-20T10:00:00Z');
final _earlier = _crawl('/tmp/earlier.crawlstat', '2026-10-01T10:00:00Z');

Future<void> _pump(
  WidgetTester tester, {
  String? chrome = '/Applications/Google Chrome.app/Contents/MacOS/Chrome',
  bool pro = true,
  List<ProjectInfo>? recents,
  Settings settings = const Settings(userAgent: 'test'),
  String? written,
}) async {
  tester.view.physicalSize = const Size(1200, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        settingsProvider.overrideWith(() => _Settings(settings)),
        chromePathProvider.overrideWithValue(chrome),
        isProProvider.overrideWithValue(pro),
        recentProjectsProvider.overrideWith(
          () => _Recent(recents ?? [_latest]),
        ),
        writtenReportsProvider.overrideWith(
          () => _Written({_latest.path: ?written}),
        ),
      ],
      child: MaterialApp(
        theme: CrawlStatTheme.dark(),
        home: Scaffold(body: ReportDialog(project: _latest)),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

SegmentedButton<bool> _format(WidgetTester tester) =>
    tester.widget(find.byType(SegmentedButton<bool>));

void main() {
  test('typed colours are accepted with or without the hash', () {
    expect(normaliseHex('0f9f8f'), '#0F9F8F');
    expect(normaliseHex(' #12abEF '), '#12ABEF');
    expect(normaliseHex('#12ab'), isNull);
    expect(normaliseHex('red'), isNull);
  });

  testWidgets('the cover sketch shows the site and who it is from', (
    tester,
  ) async {
    await _pump(tester);
    expect(find.text('Export report'), findsOneWidget);
    expect(find.text('shop.example'), findsOneWidget);
    expect(find.textContaining('Prepared by Ada'), findsNothing);

    await tester.enterText(find.byType(TextField).first, 'Ada');
    await tester.pump();
    expect(find.text('Prepared by Ada'), findsOneWidget);
  });

  testWidgets('remembers the last name and colour', (tester) async {
    await _pump(
      tester,
      settings: const Settings(
        userAgent: 'test',
        reportPreparedBy: 'Acme',
        reportAccent: '#ABCDEF',
      ),
    );
    final fields = tester.widgetList<TextField>(find.byType(TextField));
    expect(fields.first.controller!.text, 'Acme');
    // Not one of the swatches, so it lands in the hex field.
    expect(fields.last.controller!.text, '#ABCDEF');
  });

  testWidgets('PDF is the default when a browser can print it', (tester) async {
    await _pump(tester);
    expect(_format(tester).selected, {true});
    expect(find.text('A4, ready to attach or print.'), findsOneWidget);
  });

  testWidgets('without a browser only the web page is offered', (tester) async {
    await _pump(tester, chrome: null);
    final format = _format(tester);
    expect(format.selected, {false});
    expect(format.segments.first.enabled, isFalse);
  });

  testWidgets('progress is offered only when there is an earlier crawl', (
    tester,
  ) async {
    await _pump(tester);
    expect(find.text('Include progress since the last crawl'), findsNothing);
  });

  testWidgets('with an earlier crawl, progress is in by default', (
    tester,
  ) async {
    await _pump(tester, recents: [_latest, _earlier]);
    expect(find.text('Include progress since the last crawl'), findsOneWidget);
    expect(tester.widget<Checkbox>(find.byType(Checkbox)).value, isTrue);
  });

  testWidgets('progress needs Pro', (tester) async {
    await _pump(tester, pro: false, recents: [_latest, _earlier]);
    final box = tester.widget<Checkbox>(find.byType(Checkbox));
    expect(box.value, isFalse);
    expect(box.onChanged, isNull);
    expect(find.text('Comparing crawls is a Pro feature.'), findsOneWidget);
  });

  testWidgets('without a written report there is nothing to include', (
    tester,
  ) async {
    await _pump(tester);
    expect(find.text('Include the written report'), findsNothing);
  });

  testWidgets('with a written report, it is in by default', (tester) async {
    await _pump(tester, written: '# Healthy\n\nAll good.');
    expect(find.text('Include the written report'), findsOneWidget);
    expect(tester.widget<Checkbox>(find.byType(Checkbox)).value, isTrue);
  });
}
