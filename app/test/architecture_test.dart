import 'dart:typed_data';

import 'package:crawlstat/src/rust/api/architecture.dart';
import 'package:crawlstat/src/screens/visualise_screen.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

ArchSection _section(
  String path,
  String label,
  int level, {
  required int pages,
  bool hasChildren = false,
  int? hub = 200,
  int deep = 0,
  int weak = 0,
  double pageShare = 0,
  double valueShare = 0,
}) => ArchSection(
  path: path,
  label: label,
  level: level,
  hasChildren: hasChildren,
  pages: pages,
  ownPages: pages,
  hubStatus: hub,
  avgDepth: 2,
  maxDepth: 3,
  deep: deep,
  weak: weak,
  errors: 0,
  redirects: 0,
  pageShare: pageShare,
  valueShare: valueShare,
);

final _architecture = SiteArchitecture(
  overview: const ArchOverview(
    pages: 120,
    avgDepth: 2.4,
    maxDepth: 6,
    withinReach: 0.82,
    topSections: 2,
    hasLinkScore: true,
  ),
  findings: const [
    ArchFinding(
      id: 'deep_pages',
      severity: 'warning',
      title: 'Pages buried more than 3 clicks deep',
      summary: '14 pages take 4 clicks or more to reach.',
      fix: 'Link the buried pages from their section landing page.',
      count: 14,
      examples: [
        ArchExample(
          url: 'https://example.com/blog/deep-post',
          detail: '6 clicks',
        ),
      ],
    ),
    ArchFinding(
      id: 'missing_hubs',
      severity: 'warning',
      title: 'Folders without a landing page',
      summary: '/shop holds 40 pages but has no page of its own.',
      fix: 'Publish a landing page at the folder URL.',
      count: 1,
      examples: [
        ArchExample(url: 'https://example.com/shop/', detail: '40 pages'),
      ],
    ),
  ],
  sections: [
    _section('/', 'example.com', 0, pages: 120, hasChildren: true),
    _section(
      '/blog',
      'blog',
      1,
      pages: 70,
      hasChildren: true,
      deep: 14,
      pageShare: 0.58,
      valueShare: 0.2,
    ),
    _section('/blog/archive', 'archive', 2, pages: 12),
    _section(
      '/shop',
      'shop',
      1,
      pages: 40,
      hub: null,
      weak: 6,
      pageShare: 0.33,
      valueShare: 0.6,
    ),
  ],
  map: ArchMap(
    sections: const ['/', '/blog', '/shop'],
    pages: Int32List.fromList([10, 70, 40]),
    flows: const [
      ArchFlow(from: 0, to: 1, links: 3),
      ArchFlow(from: 0, to: 2, links: 9),
      ArchFlow(from: 1, to: 1, links: 300),
    ],
  ),
);

Future<void> _pump(WidgetTester tester, {ValueChanged<String>? onShow}) async {
  tester.view.physicalSize = const Size(1400, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      child: MaterialApp(
        theme: CrawlStatTheme.dark(),
        home: Scaffold(
          body: ArchitectureOverview(
            architecture: _architecture,
            onShowSection: onShow ?? (_) {},
          ),
        ),
      ),
    ),
  );
}

void main() {
  testWidgets('leads with the headline numbers and what to fix', (
    tester,
  ) async {
    await _pump(tester);

    expect(find.text('82%'), findsOneWidget);
    expect(find.text('What to fix'), findsOneWidget);
    expect(find.text('Pages buried more than 3 clicks deep'), findsOneWidget);
    expect(find.text('14 pages'), findsOneWidget);
    expect(find.text('1 folder'), findsOneWidget);
    // The fix stays folded until asked for.
    expect(find.text('How to fix'), findsNothing);
  });

  testWidgets('expanding a finding shows the fix and examples', (tester) async {
    await _pump(tester);

    await tester.tap(find.text('Pages buried more than 3 clicks deep'));
    await tester.pumpAndSettle();

    expect(find.text('How to fix'), findsOneWidget);
    expect(
      find.text('Link the buried pages from their section landing page.'),
      findsOneWidget,
    );
    expect(find.text('/blog/deep-post'), findsOneWidget);
  });

  testWidgets('the tree opens one folder at a time', (tester) async {
    await _pump(tester);

    expect(find.text('example.com'), findsOneWidget);
    expect(find.text('blog/'), findsOneWidget);
    expect(find.text('shop/'), findsOneWidget);
    expect(find.text('No landing page'), findsOneWidget);
    expect(find.text('14 deep'), findsOneWidget);
    expect(find.text('archive/'), findsNothing);

    await tester.tap(find.text('blog/'));
    await tester.pumpAndSettle();
    expect(find.text('archive/'), findsOneWidget);
  });

  testWidgets('a section can be shown in the link graph', (tester) async {
    String? shown;
    await _pump(tester, onShow: (path) => shown = path);

    final buttons = find.byTooltip('Highlight in the link graph');
    expect(buttons, findsNWidgets(3));
    await tester.tap(buttons.at(2));
    expect(shown, '/shop');
  });

  testWidgets('the link map lists every pair of sections', (tester) async {
    await _pump(tester);

    expect(find.text('How sections link'), findsOneWidget);
    expect(find.byTooltip('/ → /shop\n9 linking page pairs'), findsOneWidget);
    expect(
      find.byTooltip('/blog → itself\n300 links between its own pages'),
      findsOneWidget,
    );
  });
}
