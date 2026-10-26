import 'package:crawlstat/src/router.dart';
import 'package:crawlstat/src/rust/api/project.dart';
import 'package:crawlstat/src/rust/api/types.dart';
import 'package:crawlstat/src/screens/issues_screen.dart';
import 'package:crawlstat/src/state/issues.dart';
import 'package:crawlstat/src/state/projects.dart';
import 'package:crawlstat/src/state/results.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

const _path = '/c/shop';

Issue _issue(String id, String severity, int urls) => Issue(
  id: id,
  title: 'Issue $id',
  severity: severity,
  description: 'About $id.',
  howToFix: 'Fix $id.',
  docsSlug: id,
  urlCount: urls,
);

PlanItem _item(
  String id,
  String severity, {
  required double impact,
  int pages = 10,
  int fixes = 10,
  String effort = 'per_page',
}) => PlanItem(
  issueId: id,
  title: 'Issue $id',
  severity: severity,
  howToFix: '',
  docsSlug: id,
  pages: pages,
  distinctFixes: fixes,
  leverage: fixes == 0 ? 0 : pages / fixes,
  reach: 0.2,
  impact: impact,
  effort: effort,
  examples: const [],
);

final _issues = [
  _issue('missing-h1', 'warning', 40),
  _issue('broken', 'error', 8),
  _issue('dup-title', 'warning', 200),
  _issue('alt', 'notice', 90),
];

final _plan = ActionPlan(
  items: [
    _item('broken', 'error', impact: 30, pages: 8, fixes: 8),
    // Fewer pages than dup-title but worth more: it should come first.
    _item(
      'missing-h1',
      'warning',
      impact: 80,
      pages: 40,
      fixes: 1,
      effort: 'template',
    ),
    _item('dup-title', 'warning', impact: 20, pages: 200, fixes: 200),
    _item('alt', 'notice', impact: 90, pages: 90, fixes: 90),
  ],
  affectedPages: 250,
  totalPages: 400,
  valueAtRisk: 0.1,
);

final _report = AuditReport(
  issues: _issues,
  errors: 1,
  warnings: 2,
  notices: 1,
  depthDistribution: const [],
  indexablePages: 300,
  nonIndexablePages: 100,
  elapsedMs: 10,
);

class _Open extends OpenProject {
  @override
  String? build() => _path;
}

Future<void> _pump(
  WidgetTester tester, {
  String location = Routes.issues,
}) async {
  tester.view.physicalSize = const Size(1300, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final router = GoRouter(
    initialLocation: location,
    routes: [
      GoRoute(
        path: Routes.issues,
        builder: (context, state) => Scaffold(
          body: IssuesScreen(initialIssue: state.uri.queryParameters['issue']),
        ),
      ),
      GoRoute(
        path: Routes.results,
        builder: (_, _) => const Scaffold(body: Text('pages screen')),
      ),
    ],
  );
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        openProjectProvider.overrideWith(_Open.new),
        auditReportProvider.overrideWith((ref, path) async => _report),
        actionPlanProvider.overrideWith((ref, path) async => _plan),
        issueUrlsProvider.overrideWith(
          (ref, args) async => [
            for (var i = 0; i < 3; i++)
              AffectedPage(
                urlId: i,
                url: 'https://shop.example/${args.issueId}/$i',
                status: 200,
                detail: i == 0 ? 'Reason for ${args.issueId}' : null,
              ),
          ],
        ),
      ],
      child: MaterialApp.router(
        theme: CrawlStatTheme.dark(),
        routerConfig: router,
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  group('rankIssues', () {
    test('severity first, then impact within each severity', () {
      final ranked = rankIssues(_issues, plan: _plan);
      expect(ranked.map((issue) => issue.id), [
        'broken',
        'missing-h1',
        'dup-title',
        'alt',
      ]);
    });

    test('without a plan, falls back to the number of pages', () {
      final ranked = rankIssues(_issues);
      expect(ranked.map((issue) => issue.id), [
        'broken',
        'dup-title',
        'missing-h1',
        'alt',
      ]);
      expect(ranked.first.share, isNull);
    });

    test('knows the share of the site and the template fixes', () {
      final ranked = {
        for (final issue in rankIssues(_issues, plan: _plan)) issue.id: issue,
      };
      expect(ranked['dup-title']!.share, closeTo(0.5, 1e-9));
      expect(ranked['missing-h1']!.isTemplateFix, isTrue);
      expect(ranked['dup-title']!.isTemplateFix, isFalse);
    });

    test('sizes the site from the report until the plan loads', () {
      final ranked = rankIssues(_issues, totalPages: 400);
      expect(ranked.first.share, closeTo(8 / 400, 1e-9));
    });
  });

  group('formatShare', () {
    test('never rounds something real down to nothing', () {
      expect(formatShare(0.001), '<1%');
      expect(formatShare(0.125), '13%');
      expect(formatShare(0.999), '99%');
      expect(formatShare(1), '100%');
    });
  });

  group('IssuesScreen', () {
    testWidgets('lists the most urgent first', (tester) async {
      await _pump(tester);
      double top(String id) =>
          tester.getTopLeft(find.text('Issue $id').first).dy;
      expect(top('broken'), lessThan(top('missing-h1')));
      expect(top('missing-h1'), lessThan(top('dup-title')));
      expect(top('dup-title'), lessThan(top('alt')));
      // The worst one is open to begin with.
      expect(find.text('About broken.'), findsOneWidget);
    });

    testWidgets('opens on the issue a link asks for', (tester) async {
      await _pump(tester, location: Routes.issue('dup-title'));
      expect(find.text('About dup-title.'), findsOneWidget);
      expect(find.text('Show all 200 in Pages'), findsOneWidget);
    });

    testWidgets('shows reach, effort and the template badge', (tester) async {
      await _pump(tester, location: Routes.issue('missing-h1'));
      // 40 of 400 pages, in the list and in the detail.
      expect(find.text('10% of pages'), findsNWidgets(2));
      expect(find.text('Template'), findsOneWidget);
      expect(find.text('Template fix'), findsOneWidget);
      expect(find.text('One change fixes all 40'), findsOneWidget);
    });

    testWidgets('lists the affected pages with their reasons', (tester) async {
      await _pump(tester, location: Routes.issue('broken'));
      await tester.scrollUntilVisible(
        find.text('https://shop.example/broken/2'),
        200,
        scrollable: find.byType(Scrollable).last,
      );
      expect(find.text('Reason for broken'), findsOneWidget);
      expect(find.text('And 5 more — show all in Pages'), findsOneWidget);
    });

    testWidgets('a page opens the Pages tab filtered to the issue', (
      tester,
    ) async {
      await _pump(tester, location: Routes.issue('broken'));
      final row = find.text('https://shop.example/broken/0');
      await tester.scrollUntilVisible(
        row,
        200,
        scrollable: find.byType(Scrollable).last,
      );
      await tester.tap(row);
      await tester.pumpAndSettle();
      expect(find.text('pages screen'), findsOneWidget);
    });
  });
}
