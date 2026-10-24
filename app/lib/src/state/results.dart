import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';
import 'projects.dart';

/// Rows fetched per request.
///
/// Big enough that scrolling rarely waits, small enough that the first page
/// paints immediately on a crawl of any size. The grid never holds more than a
/// handful of these.
const int chunkSize = 200;

/// Loaded chunks kept in memory, oldest evicted first.
///
/// 20 × 200 rows is 4,000 at most, whatever the size of the crawl — which is
/// what lets the table scroll through a 500,000-row result without the app
/// growing. The database is the data; this is a window onto it.
const int maxCachedChunks = 20;

/// Which rows a table is showing. Immutable, and compared by value so a rebuild
/// with an identical query does not refetch.
class UrlQuerySpec {
  const UrlQuerySpec({
    this.tab = 'internal',
    this.sortBy = 'url',
    this.descending = false,
    this.search,
    this.statusCode,
    this.statusClass,
    this.issueId,
    this.maxDepth,
  });

  final String tab;
  final String sortBy;
  final bool descending;
  final String? search;
  final int? statusCode;
  final int? statusClass;
  final String? issueId;
  final int? maxDepth;

  UrlQuerySpec copyWith({
    String? tab,
    String? sortBy,
    bool? descending,
    String? search,
    bool clearSearch = false,
    int? statusCode,
    bool clearStatusCode = false,
    int? statusClass,
    bool clearStatusClass = false,
    String? issueId,
    bool clearIssue = false,
    int? maxDepth,
    bool clearMaxDepth = false,
  }) {
    return UrlQuerySpec(
      tab: tab ?? this.tab,
      sortBy: sortBy ?? this.sortBy,
      descending: descending ?? this.descending,
      search: clearSearch ? null : (search ?? this.search),
      statusCode: clearStatusCode ? null : (statusCode ?? this.statusCode),
      statusClass: clearStatusClass ? null : (statusClass ?? this.statusClass),
      issueId: clearIssue ? null : (issueId ?? this.issueId),
      maxDepth: clearMaxDepth ? null : (maxDepth ?? this.maxDepth),
    );
  }

  /// True when anything beyond the tab itself is narrowing the result.
  bool get hasFilters =>
      (search != null && search!.isNotEmpty) ||
      statusCode != null ||
      statusClass != null ||
      issueId != null ||
      maxDepth != null;

  UrlQuerySpec cleared() =>
      UrlQuerySpec(tab: tab, sortBy: sortBy, descending: descending);

  /// Convert to the bridge's request for one window of rows.
  UrlQuery toRequest({required int offset, required int limit}) {
    return UrlQuery(
      tab: tab,
      statusCode: statusCode,
      statusClass: statusClass,
      urlContains: (search == null || search!.isEmpty) ? null : search,
      issueId: issueId,
      maxDepth: maxDepth,
      sortBy: sortBy,
      descending: descending,
      limit: limit,
      offset: offset,
    );
  }

  @override
  bool operator ==(Object other) {
    return other is UrlQuerySpec &&
        other.tab == tab &&
        other.sortBy == sortBy &&
        other.descending == descending &&
        other.search == search &&
        other.statusCode == statusCode &&
        other.statusClass == statusClass &&
        other.issueId == issueId &&
        other.maxDepth == maxDepth;
  }

  @override
  int get hashCode => Object.hash(
    tab,
    sortBy,
    descending,
    search,
    statusCode,
    statusClass,
    issueId,
    maxDepth,
  );
}

/// A window onto the results table.
class UrlTableState {
  const UrlTableState({
    this.query = const UrlQuerySpec(),
    this.total = 0,
    this.chunks = const {},
    this.pending = const {},
    this.loadedOnce = false,
    this.error,
  });

  final UrlQuerySpec query;

  /// How many rows match the query, from the database's own count.
  final int total;

  /// Chunk index to its rows.
  final Map<int, List<UrlRow>> chunks;

  /// Chunks currently being fetched, so a fast scroll does not ask twice.
  final Set<int> pending;

  final bool loadedOnce;
  final Object? error;

  /// The row at `index`, or `null` if its chunk is not loaded yet.
  UrlRow? rowAt(int index) {
    final chunk = chunks[index ~/ chunkSize];
    if (chunk == null) return null;
    final offset = index % chunkSize;
    return offset < chunk.length ? chunk[offset] : null;
  }

  UrlTableState copyWith({
    UrlQuerySpec? query,
    int? total,
    Map<int, List<UrlRow>>? chunks,
    Set<int>? pending,
    bool? loadedOnce,
    Object? error,
    bool clearError = false,
  }) {
    return UrlTableState(
      query: query ?? this.query,
      total: total ?? this.total,
      chunks: chunks ?? this.chunks,
      pending: pending ?? this.pending,
      loadedOnce: loadedOnce ?? this.loadedOnce,
      error: clearError ? null : (error ?? this.error),
    );
  }
}

/// Pages the results table from SQLite.
///
/// The grid asks for a row by index; if its chunk is missing, a fetch starts
/// and the row renders as a placeholder until it lands. Nothing is ever loaded
/// that the user has not scrolled to.
class UrlTable extends Notifier<UrlTableState> {
  /// Chunk indexes in the order they were loaded, for eviction.
  final List<int> _order = [];

  @override
  UrlTableState build() {
    // Re-read whenever the open project changes.
    ref.listen(openProjectProvider, (previous, next) {
      if (previous != next) {
        reset();
      }
    });
    return const UrlTableState();
  }

  String? get _path => ref.read(openProjectProvider);

  /// Apply a new query, discarding what was loaded for the old one.
  void setQuery(UrlQuerySpec query) {
    if (query == state.query && state.loadedOnce) return;
    _order.clear();
    state = UrlTableState(query: query);
    _loadChunk(0);
  }

  void setTab(String tab) => setQuery(state.query.copyWith(tab: tab));

  void setSearch(String? search) => setQuery(
    state.query.copyWith(search: search, clearSearch: search == null),
  );

  void setIssue(String? issueId) => setQuery(
    state.query.copyWith(issueId: issueId, clearIssue: issueId == null),
  );

  void clearFilters() => setQuery(state.query.cleared());

  /// Sort by a column, flipping the direction when it is already sorted.
  void sortBy(String column) {
    final descending = state.query.sortBy == column
        ? !state.query.descending
        : false;
    setQuery(state.query.copyWith(sortBy: column, descending: descending));
  }

  /// Reload from scratch, keeping the query. Called when a crawl finishes.
  void reset() {
    _order.clear();
    state = UrlTableState(query: state.query);
    _loadChunk(0);
  }

  /// Make sure the chunk containing `index` is loaded or loading.
  void ensureIndexLoaded(int index) {
    if (index < 0) return;
    final chunk = index ~/ chunkSize;
    if (state.chunks.containsKey(chunk) || state.pending.contains(chunk)) {
      return;
    }
    _loadChunk(chunk);
  }

  Future<void> _loadChunk(int chunk) async {
    final path = _path;
    if (path == null) return;

    state = state.copyWith(
      pending: {...state.pending, chunk},
      clearError: true,
    );
    final query = state.query;

    try {
      final page = await rust.listUrls(
        dbPath: path,
        query: query.toRequest(offset: chunk * chunkSize, limit: chunkSize),
      );

      // The query changed while this was in flight: throw the result away
      // rather than mixing two filters in one table.
      if (query != state.query) return;

      final chunks = {...state.chunks, chunk: page.rows};
      _order.remove(chunk);
      _order.add(chunk);
      while (_order.length > maxCachedChunks) {
        chunks.remove(_order.removeAt(0));
      }

      state = state.copyWith(
        total: page.total.toInt(),
        chunks: chunks,
        pending: {...state.pending}..remove(chunk),
        loadedOnce: true,
      );
    } on Object catch (error) {
      if (query != state.query) return;
      state = state.copyWith(
        pending: {...state.pending}..remove(chunk),
        loadedOnce: true,
        error: error,
      );
    }
  }
}

final urlTableProvider = NotifierProvider<UrlTable, UrlTableState>(
  UrlTable.new,
);

/// The URL the detail panel is showing, or `null` when it is closed.
class SelectedUrl extends Notifier<int?> {
  @override
  int? build() => null;

  void select(int urlId) => state = urlId;
  void clear() => state = null;
}

final selectedUrlProvider = NotifierProvider<SelectedUrl, int?>(
  SelectedUrl.new,
);

/// Detail for the selected URL.
final urlDetailProvider =
    FutureProvider.family<UrlDetail, ({String path, int urlId})>(
      (ref, args) => rust.urlDetail(dbPath: args.path, urlId: args.urlId),
    );

/// The audit report for the open project.
final auditReportProvider = FutureProvider.family<AuditReport, String>(
  (ref, path) => rust.listIssues(dbPath: path),
);

/// Response-code counts, for the Response Codes tab.
final statusSummaryProvider =
    FutureProvider.family<List<rust.StatusCount>, String>(
      (ref, path) => rust.statusSummary(dbPath: path),
    );
