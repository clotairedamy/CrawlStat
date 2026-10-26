import 'package:crawlstat/src/rust/api/types.dart';
import 'package:crawlstat/src/state/results.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('UrlQuerySpec', () {
    test('compares by value, so an identical query does not refetch', () {
      const a = UrlQuerySpec(tab: 'internal', search: 'blog');
      const b = UrlQuerySpec(tab: 'internal', search: 'blog');
      expect(a, b);
      expect(a.hashCode, b.hashCode);
      expect(a, isNot(const UrlQuerySpec(tab: 'external', search: 'blog')));
    });

    test('copyWith can clear a filter as well as set one', () {
      const query = UrlQuerySpec(
        search: 'blog',
        statusClass: 4,
        issueId: 'status_4xx',
      );
      expect(query.copyWith(clearSearch: true).search, isNull);
      expect(query.copyWith(clearStatusClass: true).statusClass, isNull);
      expect(query.copyWith(clearIssue: true).issueId, isNull);
      // Clearing one filter leaves the others alone.
      expect(query.copyWith(clearSearch: true).statusClass, 4);
    });

    test('a lens narrows the result, and the lenses count without it', () {
      const query = UrlQuerySpec(
        tab: 'html',
        search: 'blog',
        sortBy: 'title',
        descending: true,
        view: 'slow',
      );
      expect(query.hasFilters, isTrue);
      expect(query.toRequest(offset: 0, limit: 1).view, 'slow');
      expect(query.copyWith(clearView: true).view, isNull);
      // Same counts whichever lens is on and however the rows are sorted.
      expect(query.lensBase, const UrlQuerySpec(tab: 'html', search: 'blog'));
      expect(
        query.lensBase,
        query.copyWith(view: 'broken', sortBy: 'url').lensBase,
      );
    });

    test('knows whether anything is narrowing the result', () {
      expect(const UrlQuerySpec().hasFilters, isFalse);
      // The tab itself is not a filter: every tab shows something.
      expect(const UrlQuerySpec(tab: 'images').hasFilters, isFalse);
      expect(const UrlQuerySpec(search: 'x').hasFilters, isTrue);
      expect(const UrlQuerySpec(search: '').hasFilters, isFalse);
      expect(const UrlQuerySpec(issueId: 'title_missing').hasFilters, isTrue);
    });

    test('clearing keeps the tab and the sort the user chose', () {
      const query = UrlQuerySpec(
        tab: 'images',
        sortBy: 'size',
        descending: true,
        search: 'logo',
      );
      final cleared = query.cleared();
      expect(cleared.search, isNull);
      expect(cleared.tab, 'images');
      expect(cleared.sortBy, 'size');
      expect(cleared.descending, isTrue);
    });

    test('an empty search is sent as absent rather than as an empty match', () {
      final request = const UrlQuerySpec(search: '')
          .toRequest(offset: 0, limit: 50);
      expect(request.urlContains, isNull);

      final searching = const UrlQuerySpec(search: 'blog')
          .toRequest(offset: 0, limit: 50);
      expect(searching.urlContains, 'blog');
    });

    test('paging arithmetic reaches the bridge unchanged', () {
      final request = const UrlQuerySpec().toRequest(offset: 400, limit: 200);
      expect(request.offset, 400);
      expect(request.limit, 200);
    });
  });

  group('UrlTableState', () {
    test('maps a row index onto the chunk holding it', () {
      // Two chunks loaded with a gap: rows in the gap are not available yet.
      final state = UrlTableState(
        total: 1000,
        chunks: {
          0: List.generate(chunkSize, (i) => _row(i)),
          3: List.generate(chunkSize, (i) => _row(3 * chunkSize + i)),
        },
      );

      expect(state.rowAt(0)?.id, 0);
      expect(state.rowAt(chunkSize - 1)?.id, chunkSize - 1);
      expect(state.rowAt(3 * chunkSize)?.id, 3 * chunkSize);
      // Inside an unloaded chunk.
      expect(state.rowAt(chunkSize), isNull);
      expect(state.rowAt(999), isNull);
    });

    test('a short final chunk does not read past its end', () {
      final state = UrlTableState(
        total: 205,
        chunks: {
          0: List.generate(chunkSize, (i) => _row(i)),
          1: List.generate(5, (i) => _row(chunkSize + i)),
        },
      );
      expect(state.rowAt(204)?.id, 204);
      expect(state.rowAt(205), isNull);
    });

    test('starts empty rather than pretending to have rows', () {
      const state = UrlTableState();
      expect(state.total, 0);
      expect(state.loadedOnce, isFalse);
      expect(state.rowAt(0), isNull);
    });
  });
}

/// A row with only the fields these tests read.
UrlRow _row(int id) => UrlRow(
  id: id,
  url: 'https://a.test/$id',
  state: 'done',
  kind: 'html',
  isInternal: true,
  depth: 1,
  inlinks: 0,
  redirectCount: 0,
  issueCount: 0,
);
