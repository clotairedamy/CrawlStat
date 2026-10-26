import 'package:crawlstat/src/rust/api/types.dart';
import 'package:crawlstat/src/screens/projects_screen.dart';
import 'package:crawlstat/src/state/crawl.dart';
import 'package:crawlstat/src/state/projects.dart';
import 'package:crawlstat/src/theme/soft.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

ProjectStats _stats({
  int ok = 0,
  int redirects = 0,
  int missing = 0,
  int server = 0,
  int failed = 0,
  int queued = 0,
}) => ProjectStats(
  total: ok + redirects + missing + server + failed + queued,
  queued: queued,
  crawled: ok + redirects + missing + server + failed,
  failed: failed,
  skipped: 0,
  blockedByRobots: 0,
  status2Xx: ok,
  status3Xx: redirects,
  status4Xx: missing,
  status5Xx: server,
);

ProjectInfo _crawl(
  String path, {
  String? host,
  String? startUrl,
  String? createdAt,
  ProjectStats? stats,
  int errors = 0,
  int warnings = 0,
}) => ProjectInfo(
  path: path,
  rootHost: host,
  startUrl: startUrl,
  createdAt: createdAt,
  stats: stats ?? _stats(ok: 10),
  errors: errors,
  warnings: warnings,
  sizeBytes: 0,
);

/// The real list loads from disk through Rust; the compare button only
/// needs to see these.
class _Recent extends RecentProjects {
  _Recent(this.crawls);

  final List<ProjectInfo> crawls;

  @override
  List<ProjectInfo> build() => crawls;
}

/// A crawl controller stuck in one phase, so nothing reaches the engine.
class _Crawl extends CrawlController {
  _Crawl(this.phase);

  final CrawlPhase phase;

  @override
  LiveCrawl build() => LiveCrawl(phase: phase);
}

/// [sliver] in a scrolling page, as the Projects screen hosts it.
Future<void> _pumpSliver(
  WidgetTester tester,
  List<ProjectInfo> crawls,
  Widget sliver, {
  required double width,
  ThemeData? theme,
  CrawlPhase crawlPhase = CrawlPhase.idle,
}) async {
  tester.view.physicalSize = Size(width, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        recentProjectsProvider.overrideWith(() => _Recent(crawls)),
        crawlControllerProvider.overrideWith(() => _Crawl(crawlPhase)),
      ],
      child: MaterialApp(
        theme: theme ?? CrawlStatTheme.dark(),
        home: Scaffold(body: CustomScrollView(slivers: [sliver])),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  group('siteKey', () {
    test('treats www and the bare domain as one site', () {
      expect(siteKey(_crawl('a', host: 'www.Example.com')), 'example.com');
      expect(siteKey(_crawl('b', host: 'example.com')), 'example.com');
    });

    test('falls back to the start URL, then the file', () {
      expect(
        siteKey(_crawl('a', startUrl: 'https://www.shop.io/start')),
        'shop.io',
      );
      expect(siteKey(_crawl('/tmp/x.crawl')), '/tmp/x.crawl');
    });
  });

  group('groupBySite', () {
    test(
      'puts each site’s crawls newest first, whatever order they came in',
      () {
        final sites = groupBySite([
          _crawl('old', host: 'a.com', createdAt: '2026-01-01T10:00:00Z'),
          _crawl('other', host: 'b.com', createdAt: '2026-03-01T10:00:00Z'),
          _crawl('new', host: 'www.a.com', createdAt: '2026-02-01T10:00:00Z'),
        ]);

        expect(sites.map((s) => s.key), ['a.com', 'b.com']);
        final a = sites.first;
        expect(a.crawls.map((c) => c.path), ['new', 'old']);
        expect(a.latest.path, 'new');
        expect(a.previous?.path, 'old');
        expect(sites.last.previous, isNull);
      },
    );

    test('keeps undated crawls, after the dated ones', () {
      final site = groupBySite([
        _crawl('undated', host: 'a.com'),
        _crawl('dated', host: 'a.com', createdAt: '2026-01-01T10:00:00Z'),
      ]).single;
      expect(site.crawls.map((c) => c.path), ['dated', 'undated']);
    });
  });

  group('previousCrawl', () {
    final crawls = [
      _crawl('new', host: 'a.com', createdAt: '2026-03-01T10:00:00Z'),
      _crawl('other', host: 'b.com', createdAt: '2026-02-15T10:00:00Z'),
      _crawl('mid', host: 'www.a.com', createdAt: '2026-02-01T10:00:00Z'),
      _crawl('old', host: 'a.com', createdAt: '2026-01-01T10:00:00Z'),
    ];

    test('is the newest same-site crawl before this one', () {
      expect(previousCrawl(crawls.first, crawls)?.path, 'mid');
      expect(previousCrawl(crawls[2], crawls)?.path, 'old');
    });

    test('is nothing for the first crawl of a site, or an undated one', () {
      expect(previousCrawl(crawls.last, crawls), isNull);
      expect(previousCrawl(crawls[1], crawls), isNull);
      expect(previousCrawl(_crawl('x', host: 'a.com'), crawls), isNull);
    });
  });

  group('crawlHealth', () {
    test('counts redirects as healthy and failures as not', () {
      expect(
        crawlHealth(_stats(ok: 70, redirects: 20, missing: 5, failed: 5)),
        closeTo(0.9, 1e-9),
      );
    });

    test('ignores what is still queued', () {
      expect(crawlHealth(_stats(ok: 10, queued: 990)), 1.0);
    });

    test('is unknown until something has been fetched', () {
      expect(crawlHealth(_stats(queued: 5)), isNull);
    });
  });

  group('formatHealth', () {
    test('rounds down, so one broken page never reads as 100%', () {
      expect(formatHealth(2999 / 3000), '99.9%');
      expect(formatHealth(1), '100%');
      expect(formatHealth(0.5), '50%');
    });
  });

  group('relativeTime', () {
    final now = DateTime(2026, 10, 25, 12);

    test('reads like a person would say it', () {
      expect(
        relativeTime(now.subtract(const Duration(seconds: 20)), now: now),
        'Just now',
      );
      expect(
        relativeTime(now.subtract(const Duration(minutes: 5)), now: now),
        '5 min ago',
      );
      expect(
        relativeTime(now.subtract(const Duration(hours: 3)), now: now),
        '3 h ago',
      );
      expect(
        relativeTime(now.subtract(const Duration(hours: 30)), now: now),
        'Yesterday',
      );
      expect(
        relativeTime(now.subtract(const Duration(days: 4)), now: now),
        '4 days ago',
      );
    });

    test(
      'turns into a date after a week, with the year only when it differs',
      () {
        expect(relativeTime(DateTime(2026, 9, 3, 9), now: now), '3 Sep');
        expect(relativeTime(DateTime(2025, 9, 3, 9), now: now), '3 Sep 2025');
      },
    );
  });

  group('sortSites', () {
    final sites = groupBySite([
      _crawl(
        'a',
        host: 'alpha.com',
        createdAt: '2026-01-01T00:00:00Z',
        stats: _stats(ok: 50, missing: 50),
        errors: 1,
      ),
      _crawl(
        'b',
        host: 'beta.com',
        createdAt: '2026-03-01T00:00:00Z',
        stats: _stats(ok: 100),
        warnings: 9,
      ),
      _crawl(
        'c',
        host: 'gamma.com',
        createdAt: '2026-02-01T00:00:00Z',
        stats: _stats(ok: 300, missing: 3),
      ),
    ]);
    List<String> keys(List<SiteHistory> s) => [for (final x in s) x.key];

    test('most recent first by default', () {
      expect(keys(sortSites(sites, SiteSort.recent)), [
        'beta.com',
        'gamma.com',
        'alpha.com',
      ]);
    });

    test('least healthy first, so problems come to the top', () {
      expect(keys(sortSites(sites, SiteSort.health)), [
        'alpha.com',
        'gamma.com',
        'beta.com',
      ]);
    });

    test('reversing flips the order', () {
      expect(keys(sortSites(sites, SiteSort.site, reversed: true)), [
        'gamma.com',
        'beta.com',
        'alpha.com',
      ]);
    });

    test('by size and by issues, biggest first', () {
      expect(keys(sortSites(sites, SiteSort.crawled)).first, 'gamma.com');
      expect(keys(sortSites(sites, SiteSort.issues)).first, 'beta.com');
    });
  });

  group('ProjectList', () {
    final crawls = [
      _crawl(
        '/c/a2',
        host: 'www.alpha-very-long-hostname-example.com',
        startUrl: 'https://www.alpha-very-long-hostname-example.com/',
        createdAt: '2026-03-01T00:00:00Z',
        stats: _stats(ok: 900, redirects: 40, missing: 12, queued: 300),
        errors: 4,
        warnings: 1200,
      ),
      _crawl(
        '/c/a1',
        host: 'alpha-very-long-hostname-example.com',
        createdAt: '2026-02-01T00:00:00Z',
        stats: _stats(ok: 800, missing: 80),
      ),
      _crawl('/c/b', host: 'beta.com', createdAt: '2026-01-01T00:00:00Z'),
      _crawl('/c/empty', host: 'gamma.com'),
    ];

    Future<void> pump(
      WidgetTester tester,
      double width, {
      CrawlPhase crawlPhase = CrawlPhase.idle,
    }) => _pumpSliver(
      tester,
      crawls,
      ProjectList(projects: crawls, openPath: '/c/a1'),
      width: width,
      crawlPhase: crawlPhase,
    );

    // Overflow shows up as a thrown FlutterError, which fails the test.
    for (final width in [1400.0, 900.0, 560.0]) {
      testWidgets('lays out without overflowing at ${width.toInt()}px', (
        tester,
      ) async {
        await pump(tester, width);
        expect(find.text('beta.com'), findsOneWidget);
        expect(find.text('gamma.com'), findsOneWidget);
      });
    }

    testWidgets('one row per site, with earlier crawls folded away', (
      tester,
    ) async {
      await pump(tester, 1400);
      expect(find.text('2 crawls'), findsOneWidget);
      // The older alpha crawl is open, so its row is marked — but its own
      // line stays hidden until the row is expanded.
      expect(find.text('Open'), findsOneWidget);

      await tester.tap(find.byTooltip('Show earlier crawls'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Hide earlier crawls'), findsOneWidget);
      expect(find.text('Open'), findsNWidgets(2));
    });

    testWidgets('sorting by health brings the worst site to the top', (
      tester,
    ) async {
      await pump(tester, 1400);
      await tester.tap(find.text('HEALTH'));
      await tester.pumpAndSettle();

      final alpha = tester.getTopLeft(
        find.text('www.alpha-very-long-hostname-example.com'),
      );
      final beta = tester.getTopLeft(find.text('beta.com'));
      expect(alpha.dy, lessThan(beta.dy));
    });

    testWidgets('a finished site offers recrawl where Resume would be', (
      tester,
    ) async {
      await pump(tester, 1400);
      // Alpha's latest crawl is interrupted; beta and gamma are not.
      expect(find.byTooltip('Resume this crawl'), findsOneWidget);
      expect(
        find.byTooltip('Recrawl with the same settings'),
        findsNWidgets(2),
      );
    });
  });

  group('ProjectGrid', () {
    final crawls = [
      // Interrupted, so its card carries Resume rather than Open.
      _crawl(
        '/c/a2',
        host: 'www.alpha-very-long-hostname-example.com',
        startUrl: 'https://www.alpha-very-long-hostname-example.com/',
        createdAt: '2026-03-01T00:00:00Z',
        stats: _stats(ok: 900, redirects: 40, missing: 12, queued: 300),
        errors: 4,
        warnings: 1200,
      ),
      _crawl(
        '/c/a1',
        host: 'alpha-very-long-hostname-example.com',
        createdAt: '2026-02-01T00:00:00Z',
        stats: _stats(ok: 800, missing: 80),
      ),
      _crawl('/c/b', host: 'beta.com', createdAt: '2026-01-01T00:00:00Z'),
      _crawl('/c/empty', host: 'gamma.com', stats: _stats()),
    ];

    Future<void> pump(
      WidgetTester tester,
      double width, {
      ThemeData? theme,
      CrawlPhase crawlPhase = CrawlPhase.idle,
    }) => _pumpSliver(
      tester,
      crawls,
      ProjectGrid(projects: crawls, openPath: '/c/b'),
      width: width,
      theme: theme,
      crawlPhase: crawlPhase,
    );

    SoftCircleButton recrawlButton(WidgetTester tester, String tooltip) =>
        tester.widget<SoftCircleButton>(
          find
              .ancestor(
                of: find.byTooltip(tooltip).first,
                matching: find.byType(SoftCircleButton),
              )
              .first,
        );

    for (final width in [1600.0, 1000.0, 700.0, 340.0]) {
      testWidgets('lays out without overflowing at ${width.toInt()}px', (
        tester,
      ) async {
        await pump(tester, width);
        expect(find.text('beta.com'), findsOneWidget);
      });
    }

    testWidgets('in the light theme too', (tester) async {
      await pump(tester, 1200, theme: CrawlStatTheme.light());
      expect(find.text('Resume'), findsOneWidget);
    });

    testWidgets('three to a row at most, every card the same height', (
      tester,
    ) async {
      await pump(tester, 1600);
      final resume = find.ancestor(
        of: find.text('Resume'),
        matching: find.byType(SoftPanel),
      );
      final open = find.ancestor(
        of: find.text('beta.com'),
        matching: find.byType(SoftPanel),
      );
      expect(
        tester.getSize(resume.first).height,
        tester.getSize(open.first).height,
      );

      // Three across the first row, the fourth card wrapping under them.
      final tops = [
        for (final host in [
          'www.alpha-very-long-hostname-example.com',
          'alpha-very-long-hostname-example.com',
          'beta.com',
          'gamma.com',
        ])
          tester.getTopLeft(find.text(host)).dy,
      ];
      expect(tops.sublist(0, 3).toSet(), hasLength(1));
      expect(tops[3], greaterThan(tops[0]));
    });

    testWidgets('compare lists the same site first, previous crawl marked', (
      tester,
    ) async {
      await pump(tester, 1600);
      await tester.tap(find.byTooltip('Compare with another crawl').first);
      await tester.pumpAndSettle();

      final menu = find.byType(PopupMenuItem<ProjectInfo>);
      expect(menu, findsNWidgets(3));
      expect(
        find.descendant(
          of: menu.first,
          matching: find.text('alpha-very-long-hostname-example.com'),
        ),
        findsOneWidget,
      );
      expect(find.textContaining('Previous crawl'), findsOneWidget);
    });

    testWidgets('every card but the resumable one has a recrawl button', (
      tester,
    ) async {
      await pump(tester, 1600);
      expect(
        find.byTooltip('Recrawl with the same settings'),
        findsNWidgets(3),
      );
      expect(
        recrawlButton(tester, 'Recrawl with the same settings').onPressed,
        isNotNull,
      );
    });

    for (final phase in [
      CrawlPhase.starting,
      CrawlPhase.running,
      CrawlPhase.paused,
      CrawlPhase.auditing,
    ]) {
      testWidgets('recrawl waits while a crawl is ${phase.name}', (
        tester,
      ) async {
        await pump(tester, 1600, crawlPhase: phase);
        const busy = 'Another crawl is running — recrawl when it finishes';
        expect(find.byTooltip(busy), findsNWidgets(3));
        expect(recrawlButton(tester, busy).onPressed, isNull);
      });
    }

    testWidgets('the menu offers both kinds of recrawl', (tester) async {
      await pump(tester, 1600);
      await tester.tap(find.byTooltip('More').first);
      await tester.pumpAndSettle();
      expect(find.text('Recrawl'), findsOneWidget);
      expect(find.text('Recrawl with changes…'), findsOneWidget);
    });

    testWidgets('the menu greys them out while a crawl is running', (
      tester,
    ) async {
      await pump(tester, 1600, crawlPhase: CrawlPhase.running);
      await tester.tap(find.byTooltip('More').first);
      await tester.pumpAndSettle();
      final items = tester.widgetList<PopupMenuItem<String>>(
        find.byType(PopupMenuItem<String>),
      );
      final recrawls = items.where(
        (item) => item.value?.startsWith('recrawl') ?? false,
      );
      expect(recrawls, hasLength(2));
      expect(recrawls.every((item) => !item.enabled), isTrue);
    });
  });
}
