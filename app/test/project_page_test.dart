import 'dart:async';

import 'package:crawlstat/src/router.dart';
import 'package:crawlstat/src/rust/api/accessibility.dart';
import 'package:crawlstat/src/rust/api/types.dart';
import 'package:crawlstat/src/screens/projects_screen.dart';
import 'package:crawlstat/src/screens/shell.dart';
import 'package:crawlstat/src/state/accessibility.dart';
import 'package:crawlstat/src/state/crawl.dart';
import 'package:crawlstat/src/state/license.dart';
import 'package:crawlstat/src/state/projects.dart';
import 'package:crawlstat/src/state/results.dart';
import 'package:crawlstat/src/state/settings.dart';
import 'package:crawlstat/src/theme/soft.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

const _path = '/c/shop';

final _shop = ProjectInfo(
  path: _path,
  rootHost: 'shop.example',
  startUrl: 'https://shop.example/',
  createdAt: '2026-10-20T10:00:00Z',
  state: 'finished',
  stats: const ProjectStats(
    total: 120,
    queued: 0,
    crawled: 120,
    failed: 0,
    skipped: 0,
    blockedByRobots: 0,
    status2Xx: 100,
    status3Xx: 12,
    status4Xx: 8,
    status5Xx: 0,
  ),
  errors: 3,
  warnings: 14,
  sizeBytes: 2048,
);

Issue _issue(String id, String severity, int urls) => Issue(
  id: id,
  title: 'Issue $id',
  severity: severity,
  description: '',
  howToFix: '',
  docsSlug: id,
  urlCount: urls,
);

final _report = AuditReport(
  issues: [
    _issue('broken', 'error', 8),
    _issue('titles', 'warning', 40),
    _issue('alt', 'notice', 90),
  ],
  errors: 1,
  warnings: 1,
  notices: 1,
  depthDistribution: const [],
  indexablePages: 100,
  nonIndexablePages: 20,
  elapsedMs: 10,
);

A11yReport _a11y({String verdict = 'does_not_conform', int fail = 3}) =>
    A11yReport(
      standard: const A11yStandard(
        id: 'wcag21aa',
        label: 'WCAG 2.1 AA',
        laws: [],
      ),
      coverage: const A11yCoverage(
        pagesChecked: 120,
        pagesUnchecked: 0,
        thinPages: 0,
        thinExamples: [],
        videoPages: 0,
        audioPages: 0,
        formPages: 0,
        passwordPages: 0,
      ),
      verdict: verdict,
      headline: '',
      detail: '',
      rules: const [],
      criteria: const [],
      counts: A11yCounts(
        fail: fail,
        review: 0,
        checksPassed: 0,
        untested: 0,
        pass: 0,
        notApplicable: 0,
      ),
    );

class _Recent extends RecentProjects {
  @override
  List<ProjectInfo> build() => [_shop];
}

class _Open extends OpenProject {
  _Open(this.path);

  final String? path;

  @override
  String? build() => path;
}

class _Crawl extends CrawlController {
  _Crawl(this.crawl);

  final LiveCrawl crawl;

  @override
  LiveCrawl build() => crawl;
}

class _Settings extends SettingsController {
  @override
  Settings build() =>
      const Settings(userAgent: 'test', sidebarCollapsed: false);
}

/// The app's two shells, with stand-ins for every tab but the Overview —
/// the others need the engine.
GoRouter _router(String location) {
  Widget stub(String name) => Center(child: Text('$name screen'));
  return GoRouter(
    initialLocation: location,
    routes: [
      ShellRoute(
        builder: (context, state, child) =>
            AppShell(location: state.uri.path, child: child),
        routes: [
          GoRoute(path: Routes.projects, builder: (_, _) => stub('projects')),
          GoRoute(path: Routes.settings, builder: (_, _) => stub('settings')),
          ShellRoute(
            builder: (context, state, child) =>
                ProjectShell(location: state.uri.path, child: child),
            routes: [
              GoRoute(
                path: Routes.project,
                builder: (_, _) => const ProjectOverview(),
              ),
              for (final path in Routes.inProject.difference({Routes.project}))
                GoRoute(path: path, builder: (_, _) => stub(path)),
            ],
          ),
        ],
      ),
    ],
  );
}

Future<void> _pump(
  WidgetTester tester, {
  String location = Routes.project,
  String? open = _path,
  LiveCrawl crawl = const LiveCrawl(),
  double width = 1400,
  Future<A11yReport> Function()? accessibility,
}) async {
  tester.view.physicalSize = Size(width, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        recentProjectsProvider.overrideWith(_Recent.new),
        openProjectProvider.overrideWith(() => _Open(open)),
        crawlControllerProvider.overrideWith(() => _Crawl(crawl)),
        settingsProvider.overrideWith(_Settings.new),
        isProProvider.overrideWithValue(true),
        auditReportProvider.overrideWith((ref, path) async => _report),
        accessibilityReportProvider.overrideWith(
          (ref, key) => (accessibility ?? () async => _a11y())(),
        ),
      ],
      child: MaterialApp.router(
        theme: CrawlStatTheme.dark(),
        routerConfig: _router(location),
      ),
    ),
  );
  // The live tile's ring turns for as long as the crawl runs, so this
  // cannot wait for everything to settle.
  for (var i = 0; i < 10; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

void main() {
  testWidgets('a crawl opens on its own page, its tabs under its name', (
    tester,
  ) async {
    await _pump(tester);
    expect(find.text('shop.example'), findsOneWidget);
    for (final tab in ['Overview', 'Issues', 'Pages', 'Plan', 'Visualise']) {
      expect(find.text(tab), findsWidgets);
    }
    // The summary, not the live view: nothing is crawling.
    expect(find.text('Top issues'), findsOneWidget);
    expect(find.text('Progress'), findsNothing);
  });

  testWidgets('the Overview puts the worst issue first', (tester) async {
    await _pump(tester);
    final error = tester.getTopLeft(find.text('Issue broken'));
    final warning = tester.getTopLeft(find.text('Issue titles'));
    final notice = tester.getTopLeft(find.text('Issue alt'));
    expect(error.dy, lessThan(warning.dy));
    expect(warning.dy, lessThan(notice.dy));
  });

  testWidgets('a tab swaps the content but keeps the crawl’s header', (
    tester,
  ) async {
    await _pump(tester);
    await tester.tap(find.text('Pages'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('${Routes.results} screen'), findsOneWidget);
    expect(find.text('shop.example'), findsOneWidget);
  });

  testWidgets('the live crawl’s page shows it crawling', (tester) async {
    await _pump(
      tester,
      crawl: const LiveCrawl(
        phase: CrawlPhase.running,
        dbPath: _path,
        startUrl: 'https://shop.example/',
      ),
    );
    expect(find.text('Progress'), findsOneWidget);
    expect(find.text('Top issues'), findsNothing);
    // In the header and on the sidebar's live tile.
    expect(find.text('Live'), findsNWidgets(2));
  });

  testWidgets('another crawl running leaves this one’s summary alone', (
    tester,
  ) async {
    await _pump(
      tester,
      crawl: const LiveCrawl(
        phase: CrawlPhase.running,
        dbPath: '/c/other',
        startUrl: 'https://other.example/',
      ),
    );
    expect(find.text('Top issues'), findsOneWidget);
    // Only the sidebar's tile says so.
    expect(find.text('Live'), findsOneWidget);
    expect(find.text('other.example'), findsOneWidget);
  });

  testWidgets('with no crawl open it says so and offers a way back', (
    tester,
  ) async {
    await _pump(tester, open: null);
    expect(find.text('No crawl open'), findsWidgets);
    expect(find.byTooltip('All projects'), findsOneWidget);
  });

  testWidgets('the sidebar lists the tools, not a crawl’s other tabs', (
    tester,
  ) async {
    await _pump(tester, location: Routes.projects);
    for (final shown in [
      'Projects',
      'Accessibility',
      'UX Testing',
      'Settings',
    ]) {
      expect(find.text(shown), findsOneWidget, reason: shown);
    }
    for (final gone in ['Crawl', 'Results', 'Issues', 'Pages']) {
      expect(find.text(gone), findsNothing, reason: gone);
    }
  });

  testWidgets('a sidebar tool opens on the crawl’s page, lit in both places', (
    tester,
  ) async {
    await _pump(tester);
    await tester.tap(find.text('Accessibility').first);
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.text('${Routes.accessibility} screen'), findsOneWidget);
    expect(find.text('shop.example'), findsOneWidget);
    // The rail's tile and the page's tab.
    final lit = tester.widgetList<Semantics>(
      find.ancestor(
        of: find.text('Accessibility'),
        matching: find.byWidgetPredicate(
          (widget) => widget is Semantics && widget.properties.selected == true,
        ),
      ),
    );
    expect(lit, isNotEmpty);
  });

  testWidgets('with nothing open, a sidebar tool opens the last crawl used', (
    tester,
  ) async {
    await _pump(tester, location: Routes.projects, open: null);
    await tester.tap(find.text('UX Testing'));
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.text('${Routes.ux} screen'), findsOneWidget);
    expect(find.text('shop.example'), findsOneWidget);
    expect(find.text('No crawl open'), findsNothing);
  });

  testWidgets('the accessibility check runs on its own, then reports', (
    tester,
  ) async {
    final check = Completer<A11yReport>();
    await _pump(tester, accessibility: () => check.future);
    expect(find.text('Checking…'), findsOneWidget);
    expect(find.text('Running in the background'), findsOneWidget);

    check.complete(_a11y());
    await tester.pump();
    await tester.pump();
    expect(find.text('Not compliant'), findsOneWidget);
    expect(find.text('Accessibility · WCAG 2.1 AA'), findsOneWidget);
    expect(find.text('3 criteria failing · 120 pages checked'), findsOneWidget);

    await tester.tap(find.text('Not compliant'));
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.text('${Routes.accessibility} screen'), findsOneWidget);
  });

  testWidgets('the check starts before the Accessibility tab is opened', (
    tester,
  ) async {
    var runs = 0;
    await _pump(
      tester,
      location: Routes.issues,
      accessibility: () async {
        runs++;
        return _a11y();
      },
    );
    expect(runs, 1);
  });

  testWidgets('the check waits while this crawl is still running', (
    tester,
  ) async {
    var runs = 0;
    await _pump(
      tester,
      location: Routes.issues,
      crawl: const LiveCrawl(
        phase: CrawlPhase.running,
        dbPath: _path,
        startUrl: 'https://shop.example/',
      ),
      accessibility: () async {
        runs++;
        return _a11y();
      },
    );
    expect(runs, 0);
  });

  testWidgets('the health card is exactly as tall as the tiles beside it', (
    tester,
  ) async {
    await _pump(tester);
    Rect panel(Finder inside) => tester.getRect(
      find.ancestor(of: inside, matching: find.byType(SoftPanel)).first,
    );
    final health = panel(find.text('Accessibility · WCAG 2.1 AA'));
    final first = panel(find.text('Crawled'));
    final last = panel(find.text('Warnings'));
    expect(health.top, closeTo(first.top, 0.5));
    expect(health.bottom, closeTo(last.bottom, 0.5));
    // And the tiles in a row match each other.
    expect(panel(find.text('Broken')).height, closeTo(first.height, 0.5));
  });

  testWidgets('top issue counts and shares line up down the list', (
    tester,
  ) async {
    await _pump(tester);
    final counts = ['8 URLs', '40 URLs', '90 URLs'];
    final rights = {
      for (final text in counts) tester.getTopRight(find.text(text)).dx,
    };
    expect(rights, hasLength(1));
  });

  for (final width in [1400.0, 900.0, 700.0]) {
    testWidgets('lays out without overflowing at ${width.toInt()}px', (
      tester,
    ) async {
      await _pump(tester, width: width);
      expect(find.text('Top issues'), findsOneWidget);
    });
  }
}
