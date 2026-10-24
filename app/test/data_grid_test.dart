import 'package:crawlstat/src/theme/icons.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:crawlstat/src/widgets/data_grid.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// A row shaped like the real one, with just what the grid reads.
class _Row {
  const _Row(this.id, this.url, this.status);
  final int id;
  final String url;
  final int status;
}

final _columns = <GridColumn<_Row>>[
  GridColumn(
    id: 'status',
    label: 'Code',
    width: 70,
    sortKey: 'status_code',
    value: (row) => row.status.toString(),
  ),
  GridColumn(
    id: 'url',
    label: 'URL',
    width: 300,
    sortKey: 'url',
    value: (row) => row.url,
  ),
  GridColumn(
    id: 'hidden',
    label: 'Hidden',
    width: 90,
    value: (row) => 'nope',
    defaultVisible: false,
  ),
];

/// Pump a grid over `total` rows, with `loaded` of them available.
Future<_Harness> _pumpGrid(
  WidgetTester tester, {
  int total = 500,
  int loaded = 500,
  Set<String>? visible,
  String? sortBy,
  bool descending = false,
}) async {
  final harness = _Harness(total: total, loaded: loaded);

  await tester.pumpWidget(
    MaterialApp(
      theme: CrawlStatTheme.light(),
      home: Scaffold(
        body: SizedBox(
          width: 800,
          height: 400,
          child: DataGrid<_Row>(
            columns: _columns,
            visibleColumnIds: visible,
            total: total,
            rowAt: harness.rowAt,
            onNeedRow: harness.requested.add,
            sortBy: sortBy,
            descending: descending,
            onSort: harness.sorted.add,
            onSelect: harness.selected.add,
            onActivate: harness.activated.add,
            rowKey: (row) => row.id,
            emptyState: const Text('Nothing here'),
          ),
        ),
      ),
    ),
  );
  await tester.pump();
  return harness;
}

class _Harness {
  _Harness({required this.total, required this.loaded});

  final int total;
  final int loaded;
  final requested = <int>[];
  final sorted = <String>[];
  final selected = <_Row>[];
  final activated = <_Row>[];

  _Row? rowAt(int index) {
    if (index >= loaded) return null;
    return _Row(index, 'https://a.test/page-$index', index.isEven ? 200 : 404);
  }
}

void main() {
  testWidgets('renders only the rows on screen, whatever the total', (
    tester,
  ) async {
    final harness = await _pumpGrid(tester, total: 100000);

    // A hundred thousand rows, a few dozen widgets.
    expect(find.text('https://a.test/page-0'), findsOneWidget);
    final rendered = tester
        .widgetList(find.textContaining('https://a.test/page-'))
        .length;
    expect(
      rendered,
      lessThan(40),
      reason: 'rendered $rendered rows for 100,000',
    );
    expect(
      harness.requested,
      isEmpty,
      reason: 'nothing to fetch when rows are loaded',
    );
  });

  testWidgets('asks for rows whose page has not arrived', (tester) async {
    // Nothing is loaded, so every visible row has to be requested.
    final harness = await _pumpGrid(tester, total: 500, loaded: 0);
    await tester.pump();

    expect(harness.requested, isNotEmpty);
    expect(harness.requested.first, 0);
    // And it only asks for what is on screen, not for all 500.
    expect(harness.requested.toSet().length, lessThan(40));
  });

  testWidgets('shows the empty state when there is nothing to show', (
    tester,
  ) async {
    await _pumpGrid(tester, total: 0);
    expect(find.text('Nothing here'), findsOneWidget);
  });

  testWidgets('tapping a header sorts by that column', (tester) async {
    final harness = await _pumpGrid(tester);

    await tester.tap(find.text('Code'));
    await tester.pump();
    expect(harness.sorted, ['status_code']);

    await tester.tap(find.text('URL'));
    await tester.pump();
    expect(harness.sorted, ['status_code', 'url']);
  });

  testWidgets('a column with no sort key is not tappable', (tester) async {
    final harness = await _pumpGrid(
      tester,
      visible: {'status', 'url', 'hidden'},
    );

    await tester.tap(find.text('Hidden'));
    await tester.pump();
    expect(harness.sorted, isEmpty);
  });

  testWidgets('the sorted column is marked with its direction', (tester) async {
    await _pumpGrid(tester, sortBy: 'url', descending: true);
    expect(find.byIcon(AppIcon.sortDescending), findsOneWidget);
    expect(find.byIcon(AppIcon.sortAscending), findsNothing);

    await _pumpGrid(tester, sortBy: 'url');
    expect(find.byIcon(AppIcon.sortAscending), findsOneWidget);
  });

  testWidgets('only the chosen columns are shown', (tester) async {
    await _pumpGrid(tester, visible: {'url'});
    expect(find.text('URL'), findsOneWidget);
    expect(find.text('Code'), findsNothing);
    expect(find.text('Hidden'), findsNothing);

    await _pumpGrid(tester, visible: {'url', 'hidden'});
    expect(find.text('Hidden'), findsOneWidget);
  });

  testWidgets('hidden-by-default columns stay hidden when nothing is chosen', (
    tester,
  ) async {
    await _pumpGrid(tester);
    expect(find.text('Hidden'), findsNothing);
    expect(find.text('URL'), findsOneWidget);
  });

  testWidgets('clicking a row selects it, double-clicking opens it', (
    tester,
  ) async {
    final harness = await _pumpGrid(tester);

    await tester.tap(find.text('https://a.test/page-2'));
    await tester.pump();
    expect(harness.selected.single.id, 2);

    // A double tap also fires the single-tap handler, which is the behaviour a
    // list wants: select, then open.
    await tester.tap(find.text('https://a.test/page-3'));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tap(find.text('https://a.test/page-3'));
    await tester.pump();
    expect(harness.activated.single.id, 3);
  });

  testWidgets('arrow keys move the selection and Enter opens it', (
    tester,
  ) async {
    final harness = await _pumpGrid(tester);

    // Click once to take focus, then drive it from the keyboard.
    await tester.tap(find.text('https://a.test/page-0'));
    await tester.pump();
    harness.selected.clear();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(harness.selected.last.id, 1);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(harness.selected.last.id, 2);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(harness.selected.last.id, 1);

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(harness.activated.last.id, 1);
  });

  testWidgets('the cursor cannot run off either end', (tester) async {
    final harness = await _pumpGrid(tester, total: 3);
    await tester.tap(find.text('https://a.test/page-0'));
    await tester.pump();

    // Up from the first row stays put.
    harness.selected.clear();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(harness.selected, isEmpty);

    // End jumps to the last row, and past it is still the last row.
    await tester.sendKeyEvent(LogicalKeyboardKey.end);
    await tester.pump();
    expect(harness.selected.last.id, 2);

    harness.selected.clear();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(harness.selected, isEmpty);
  });

  testWidgets('Page Down moves a screenful at a time', (tester) async {
    final harness = await _pumpGrid(tester, total: 500);
    await tester.tap(find.text('https://a.test/page-0'));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.pageDown);
    await tester.pump();
    expect(harness.selected.last.id, 20);
  });

  testWidgets('a loading bar appears without disturbing the rows', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: CrawlStatTheme.light(),
        home: Scaffold(
          body: SizedBox(
            width: 800,
            height: 400,
            child: DataGrid<_Row>(
              columns: _columns,
              total: 10,
              loading: true,
              rowAt: (index) => _Row(index, 'https://a.test/page-$index', 200),
              onNeedRow: (_) {},
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    expect(find.text('https://a.test/page-0'), findsOneWidget);
  });

  group('column resizing', () {
    /// The body cell holding `text`. Measured from the rows rather than the
    /// header, so every check here also proves the two stay aligned.
    Rect cell(WidgetTester tester, String text) => tester.getRect(
      find
          .ancestor(of: find.text(text).first, matching: find.byType(SizedBox))
          .first,
    );

    /// A point on the grab area at the right edge of `label`'s column.
    Offset handle(WidgetTester tester, String label, String bodyText) => Offset(
      cell(tester, bodyText).right - 4,
      tester.getCenter(find.text(label)).dy,
    );

    testWidgets('dragging a header edge resizes the column', (tester) async {
      final harness = await _pumpGrid(tester);
      expect(cell(tester, '200').width, 70);
      final urlWidth = cell(tester, 'https://a.test/page-0').width;

      await tester.dragFrom(
        handle(tester, 'Code', '200'),
        const Offset(50, 0),
        kind: PointerDeviceKind.mouse,
      );
      await tester.pumpAndSettle();

      expect(cell(tester, '200').width, 120);
      // The URL column was taking the spare width, so it gives some back and
      // the table still exactly fills the viewport.
      expect(cell(tester, 'https://a.test/page-0').width, urlWidth - 50);
      expect(harness.sorted, isEmpty, reason: 'grabbing an edge is not a sort');
    });

    testWidgets('a column cannot be dragged narrower than the minimum', (
      tester,
    ) async {
      await _pumpGrid(tester);
      await tester.dragFrom(
        handle(tester, 'Code', '200'),
        const Offset(-200, 0),
        kind: PointerDeviceKind.mouse,
      );
      await tester.pumpAndSettle();
      expect(cell(tester, '200').width, 48);
    });

    testWidgets('a table wider than its viewport scrolls instead of '
        'overflowing', (tester) async {
      await _pumpGrid(tester);
      await tester.dragFrom(
        handle(tester, 'Code', '200'),
        const Offset(900, 0),
        kind: PointerDeviceKind.mouse,
      );
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      expect(cell(tester, '200').width, 970);
      // The URL column is back to its own width, off the right of the screen.
      expect(cell(tester, 'https://a.test/page-0').width, 300);
    });

    testWidgets('double-clicking the edge restores the default width', (
      tester,
    ) async {
      await _pumpGrid(tester);
      final urlWidth = cell(tester, 'https://a.test/page-0').width;

      await tester.dragFrom(
        handle(tester, 'Code', '200'),
        const Offset(80, 0),
        kind: PointerDeviceKind.mouse,
      );
      await tester.pumpAndSettle();
      expect(cell(tester, '200').width, 150);

      final edge = handle(tester, 'Code', '200');
      await tester.tapAt(edge, kind: PointerDeviceKind.mouse);
      await tester.pump(const Duration(milliseconds: 50));
      await tester.tapAt(edge, kind: PointerDeviceKind.mouse);
      await tester.pumpAndSettle();

      expect(cell(tester, '200').width, 70);
      expect(cell(tester, 'https://a.test/page-0').width, urlWidth);
    });
  });
}
