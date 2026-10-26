import 'package:crawlstat/src/rust/api/types.dart';
import 'package:crawlstat/src/screens/results_screen.dart';
import 'package:crawlstat/src/state/projects.dart';
import 'package:crawlstat/src/state/results.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

const _path = '/c/shop';

UrlRow _row(
  int id, {
  int? titleLength,
  int issues = 0,
  String? worst,
  int? ms,
}) => UrlRow(
  id: id,
  url: 'https://shop.example/$id',
  state: 'done',
  kind: 'html',
  isInternal: true,
  depth: 1,
  statusCode: 200,
  title: 'Page $id',
  titleLength: titleLength,
  responseTimeMs: ms,
  inlinks: 0,
  redirectCount: 0,
  issueCount: issues,
  worstSeverity: worst,
);

final _rows = [
  _row(1, titleLength: 45, ms: 120),
  _row(2, titleLength: 12, issues: 3, worst: 'error', ms: 2400),
];

final _lenses = PageLenses(
  lenses: [
    for (final (id, label, count) in const [
      ('issues', 'Has issues', 1),
      ('broken', 'Broken', 0),
      ('redirects', 'Redirects', 0),
      ('slow', 'Slow', 1),
      ('titles', 'Title problems', 1),
      ('descriptions', 'Description problems', 2),
      ('thin', 'Thin content', 1),
      ('non_indexable', 'Not indexable', 0),
    ])
      PageLens(id: id, label: label, count: count),
  ],
  total: 2,
  titleMin: 30,
  titleMax: 60,
  descriptionMin: 70,
  descriptionMax: 155,
  slowMs: 1000,
  thinWords: 200,
);

class _Open extends OpenProject {
  @override
  String? build() => _path;
}

/// Serves two rows and remembers the query, without the engine.
class _Table extends UrlTable {
  @override
  UrlTableState build() =>
      UrlTableState(total: _rows.length, chunks: {0: _rows}, loadedOnce: true);

  @override
  void setQuery(UrlQuerySpec query) => state = state.copyWith(query: query);
}

Future<ProviderContainer> _pump(WidgetTester tester, {String? tab}) async {
  tester.view.physicalSize = const Size(1500, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        openProjectProvider.overrideWith(_Open.new),
        urlTableProvider.overrideWith(_Table.new),
        pageLensesProvider.overrideWith((ref, args) async => _lenses),
      ],
      child: MaterialApp(
        theme: CrawlStatTheme.dark(),
        home: Scaffold(body: ResultsScreen(initialTab: tab)),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return ProviderScope.containerOf(tester.element(find.byType(ResultsScreen)));
}

void main() {
  testWidgets('every lens shows its count before it is opened', (tester) async {
    await _pump(tester);
    for (final label in ['All rows', 'Has issues', 'Slow', 'Not indexable']) {
      expect(find.text(label), findsOneWidget);
    }
    // Every tile is the same width, so the counts line up.
    final widths = {
      for (final label in ['All rows', 'Slow', 'Description problems'])
        tester
            .getSize(
              find
                  .ancestor(
                    of: find.text(label),
                    matching: find.byType(SizedBox),
                  )
                  .last,
            )
            .width,
    };
    expect(widths, hasLength(1));
  });

  testWidgets('a lens filters the table, and a second tap lifts it', (
    tester,
  ) async {
    final container = await _pump(tester);
    await tester.tap(find.text('Slow'));
    await tester.pumpAndSettle();
    expect(container.read(urlTableProvider).query.view, 'slow');

    await tester.tap(find.text('Slow'));
    await tester.pumpAndSettle();
    expect(container.read(urlTableProvider).query.view, isNull);

    await tester.tap(find.text('Broken'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('All rows'));
    await tester.pumpAndSettle();
    expect(container.read(urlTableProvider).query.view, isNull);
  });

  testWidgets('pages show their issues, worst first in colour', (tester) async {
    await _pump(tester);
    expect(find.text('Issues'), findsOneWidget);
    expect(find.text('3'), findsOneWidget);
  });

  testWidgets('title lengths are drawn against the crawl’s range', (
    tester,
  ) async {
    await _pump(tester, tab: 'titles');
    expect(find.byType(LengthGauge), findsNWidgets(2));
    final short = tester.widget<LengthGauge>(find.byType(LengthGauge).last);
    expect((short.length, short.min, short.max), (12, 30, 60));
    expect(find.byTooltip('12 characters — aim for 30–60'), findsOneWidget);
  });
}
