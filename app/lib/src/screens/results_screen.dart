import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/project.dart' as rust;
import '../theme/icons.dart';
import '../rust/api/types.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../widgets/export.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/data_grid.dart';
import 'issues_screen.dart' show ShareBar;
import 'shell.dart';
import 'url_detail_panel.dart';
import '../theme/soft.dart';

/// A Results tab: a filter on the data plus the columns worth seeing with it.
///
/// Tabs are column presets over one table rather than separate queries, which
/// is both how the database is shaped and how an SEO actually works — "show me
/// every page's title" is the same rows with different columns.
class ResultsTab {
  const ResultsTab({
    required this.id,
    required this.label,
    required this.filterTab,
    required this.columns,
    this.sortBy = 'url',
  });

  final String id;
  final String label;

  /// The `tab` value the bridge filters on.
  final String filterTab;

  /// Column ids to show, in order.
  final List<String> columns;
  final String sortBy;

  static const all = <ResultsTab>[
    ResultsTab(
      id: 'internal',
      label: 'Internal',
      filterTab: 'internal',
      columns: [
        'status',
        'issues',
        'url',
        'indexable',
        'title',
        'word_count',
        'depth',
        'inlinks',
        'link_score',
        'size',
        'time',
      ],
    ),
    ResultsTab(
      id: 'external',
      label: 'External',
      filterTab: 'external',
      columns: ['status', 'url', 'content_type', 'inlinks', 'time'],
    ),
    ResultsTab(
      id: 'codes',
      label: 'Response codes',
      filterTab: 'internal',
      columns: [
        'status',
        'final_status',
        'url',
        'redirects',
        'content_type',
        'time',
        'error',
      ],
      sortBy: 'status_code',
    ),
    ResultsTab(
      id: 'titles',
      label: 'Titles',
      filterTab: 'html',
      columns: ['status', 'url', 'title', 'title_length', 'indexable'],
    ),
    ResultsTab(
      id: 'meta',
      label: 'Meta description',
      filterTab: 'html',
      columns: [
        'status',
        'url',
        'description',
        'description_length',
        'indexable',
      ],
    ),
    ResultsTab(
      id: 'h1',
      label: 'H1',
      filterTab: 'html',
      columns: ['status', 'url', 'h1', 'h1_count', 'word_count'],
    ),
    ResultsTab(
      id: 'canonicals',
      label: 'Canonicals',
      filterTab: 'html',
      columns: ['status', 'url', 'canonical', 'self_canonical', 'indexable'],
    ),
    ResultsTab(
      id: 'hreflang',
      label: 'Hreflang',
      filterTab: 'html',
      columns: ['status', 'url', 'lang', 'canonical', 'indexable'],
    ),
    ResultsTab(
      id: 'structured',
      label: 'Structured data',
      filterTab: 'html',
      columns: ['status', 'url', 'structured_data', 'indexable'],
    ),
    ResultsTab(
      id: 'images',
      label: 'Images',
      filterTab: 'images',
      columns: ['status', 'url', 'size', 'content_type', 'inlinks'],
      sortBy: 'size',
    ),
    ResultsTab(
      id: 'documents',
      label: 'Documents',
      filterTab: 'documents',
      columns: ['status', 'url', 'size', 'content_type', 'inlinks'],
    ),
  ];
}

class ResultsScreen extends ConsumerStatefulWidget {
  const ResultsScreen({super.key, this.initialTab});

  final String? initialTab;

  @override
  ConsumerState<ResultsScreen> createState() => _ResultsScreenState();
}

class _ResultsScreenState extends ConsumerState<ResultsScreen> {
  final _search = TextEditingController();
  final _searchFocus = FocusNode();
  String _tabId = 'internal';

  /// The last lens counts that arrived, shown while the next ones load so
  /// the strip does not blink on every keystroke in the filter.
  PageLenses? _lenses;

  /// Column overrides per tab, so turning a column on does not leak into the
  /// other tabs' presets.
  final Map<String, Set<String>> _columnOverrides = {};

  @override
  void initState() {
    super.initState();
    final requested = widget.initialTab;
    if (requested != null && ResultsTab.all.any((tab) => tab.id == requested)) {
      _tabId = requested;
    }
    // Apply the tab's filter after the first frame, when providers are ready.
    WidgetsBinding.instance.addPostFrameCallback((_) => _applyTab());
  }

  @override
  void dispose() {
    _search.dispose();
    _searchFocus.dispose();
    super.dispose();
  }

  ResultsTab get _tab => ResultsTab.all.firstWhere((tab) => tab.id == _tabId);

  void _applyTab() {
    final table = ref.read(urlTableProvider.notifier);
    final current = ref.read(urlTableProvider).query;
    table.setQuery(
      current.copyWith(
        tab: _tab.filterTab,
        sortBy: _tab.sortBy,
        descending: false,
      ),
    );
  }

  Set<String> get _visibleColumns =>
      _columnOverrides[_tabId] ?? _tab.columns.toSet();

  Future<void> _export() async {
    final path = ref.read(openProjectProvider);
    if (path == null) return;
    final query = ref.read(urlTableProvider).query;

    await exportCsv(
      context,
      ref,
      name: _tabId,
      // Limit and offset are ignored by the export: it streams everything
      // the filter matches.
      write: (file) => rust.exportUrlsCsv(
        dbPath: path,
        query: query.toRequest(offset: 0, limit: 1),
        outPath: file,
      ),
      done: (rows) => 'Exported ${formatCount(rows)} rows',
    );
  }

  @override
  Widget build(BuildContext context) {
    final path = ref.watch(openProjectProvider);
    if (path == null) {
      return const Column(
        children: [
          ScreenHeader(title: 'Pages'),
          Expanded(child: NoProjectState()),
        ],
      );
    }

    final table = ref.watch(urlTableProvider);
    final selected = ref.watch(selectedUrlProvider);
    final lenses =
        ref
            .watch(pageLensesProvider((path: path, base: table.query.lensBase)))
            .value ??
        _lenses;
    _lenses = lenses;
    final columns = buildUrlColumns(context, limits: lenses);

    return CallbackShortcuts(
      bindings: {
        CrawlStatShortcuts.focusFilter: _searchFocus.requestFocus,
        CrawlStatShortcuts.exportCsv: _export,
        CrawlStatShortcuts.closePanel: ref
            .read(selectedUrlProvider.notifier)
            .clear,
      },
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ScreenHeader(
            title: 'Pages',
            subtitle:
                '${formatCount(table.total)} '
                '${table.total == 1 ? 'row' : 'rows'}'
                '${table.query.hasFilters ? ' matching the filter' : ''}',
            actions: [
              ColumnChooser(
                columns: [
                  for (final column in columns) (column.id, column.label),
                ],
                visible: _visibleColumns,
                onChanged: (next) =>
                    setState(() => _columnOverrides[_tabId] = next),
              ),
              OutlinedButton.icon(
                onPressed: _export,
                icon: const Icon(AppIcon.download, size: 16),
                label: const Text('Export CSV'),
              ),
            ],
          ),
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: CrawlStatTheme.gutter,
            ),
            child: _TabBar(
              tabs: ResultsTab.all,
              selected: _tabId,
              onSelect: (id) {
                setState(() => _tabId = id);
                _applyTab();
              },
            ),
          ),
          const SizedBox(height: 8),
          _LensStrip(
            lenses: lenses,
            selected: table.query.view,
            onSelect: ref.read(urlTableProvider.notifier).setView,
          ),
          const SizedBox(height: 8),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(
                CrawlStatTheme.gutter,
                0,
                CrawlStatTheme.gutter,
                CrawlStatTheme.gutter,
              ),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Expanded(
                    child: SoftPanel(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          _FilterBar(
                            search: _search,
                            focus: _searchFocus,
                            query: table.query,
                          ),
                          if (table.error != null)
                            InlineMessage(
                              message: 'Could not read results: ${table.error}',
                              severity: 'error',
                              margin: const EdgeInsets.all(12),
                            ),
                          Expanded(
                            child: DataGrid<UrlRow>(
                              columns: columns,
                              visibleColumnIds: _visibleColumns,
                              total: table.total,
                              loading: table.pending.isNotEmpty,
                              rowAt: table.rowAt,
                              onNeedRow: ref
                                  .read(urlTableProvider.notifier)
                                  .ensureIndexLoaded,
                              sortBy: table.query.sortBy,
                              descending: table.query.descending,
                              onSort: ref
                                  .read(urlTableProvider.notifier)
                                  .sortBy,
                              selectedKey: selected,
                              rowKey: (row) => row.id,
                              onSelect: (row) => ref
                                  .read(selectedUrlProvider.notifier)
                                  .select(row.id),
                              emptyState: EmptyState(
                                icon: AppIcon.clearFilter,
                                title: table.query.hasFilters
                                    ? 'Nothing matches'
                                    : 'No rows',
                                message: table.query.hasFilters
                                    ? 'Try widening the filter, or clear it to see everything.'
                                    : 'This tab has nothing in it for this crawl.',
                                action: table.query.hasFilters
                                    ? OutlinedButton(
                                        onPressed: () {
                                          _search.clear();
                                          ref
                                              .read(urlTableProvider.notifier)
                                              .clearFilters();
                                        },
                                        child: const Text('Clear filter'),
                                      )
                                    : null,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                  if (selected != null) ...[
                    const SizedBox(width: 14),
                    SoftPanel(
                      child: UrlDetailPanel(path: path, urlId: selected),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// The questions you can ask of the pages, each with its answer already
/// counted: "which are slow?" shows how many before you click.
///
/// Every tile is the same width, so the counts and bars line up across the
/// strip, and a lens with nothing in it steps back rather than disappearing —
/// zero broken pages is worth seeing.
class _LensStrip extends StatelessWidget {
  const _LensStrip({
    required this.lenses,
    required this.selected,
    required this.onSelect,
  });

  /// `null` until the first counts arrive.
  final PageLenses? lenses;
  final String? selected;
  final void Function(String?) onSelect;

  static const double tileWidth = 138;
  static const double tileHeight = 74;

  static Color tone(BuildContext context, String id) {
    final palette = CrawlStatTheme.palette(context);
    return switch (id) {
      'broken' => palette.error,
      'issues' || 'slow' => palette.warning,
      'titles' || 'descriptions' || 'thin' => palette.accent,
      'all' => palette.success,
      _ => palette.notice,
    };
  }

  @override
  Widget build(BuildContext context) {
    final lenses = this.lenses;
    final total = lenses?.total.toInt();
    final tiles = <(String?, String, int?)>[
      (null, 'All rows', total),
      for (final lens in lenses?.lenses ?? const <PageLens>[])
        (lens.id, lens.label, lens.count.toInt()),
    ];
    return SizedBox(
      height: tileHeight + 16,
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(
          horizontal: CrawlStatTheme.gutter,
          vertical: 8,
        ),
        clipBehavior: Clip.none,
        itemCount: lenses == null ? 1 : tiles.length,
        separatorBuilder: (_, _) => const SizedBox(width: 10),
        itemBuilder: (context, index) {
          final (id, label, count) = tiles[index];
          return _LensTile(
            label: label,
            count: count,
            total: total,
            tone: tone(context, id ?? 'all'),
            selected: selected == id,
            onTap: () => onSelect(id),
          );
        },
      ),
    );
  }
}

class _LensTile extends StatelessWidget {
  const _LensTile({
    required this.label,
    required this.count,
    required this.total,
    required this.tone,
    required this.selected,
    required this.onTap,
  });

  final String label;

  /// `null` while counting.
  final int? count;
  final int? total;
  final Color tone;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final count = this.count;
    final total = this.total;
    final empty = count == 0;
    final share = count == null || total == null || total == 0
        ? 0.0
        : count / total;
    final muted = theme.colorScheme.onSurfaceVariant;
    return SizedBox(
      width: _LensStrip.tileWidth,
      height: _LensStrip.tileHeight,
      child: Opacity(
        opacity: empty && !selected ? 0.55 : 1,
        child: SoftPanel(
          pressed: selected,
          onTap: onTap,
          padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Container(
                    width: 6,
                    height: 6,
                    decoration: BoxDecoration(
                      color: empty ? muted : tone,
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 7),
                  Expanded(
                    child: Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall?.copyWith(
                        fontWeight: selected
                            ? FontWeight.w600
                            : FontWeight.w500,
                        color: selected ? theme.colorScheme.onSurface : null,
                      ),
                    ),
                  ),
                ],
              ),
              const Spacer(),
              Text(
                count == null ? '…' : formatCount(count),
                style: theme.textTheme.titleMedium?.copyWith(
                  height: 1.1,
                  color: selected ? tone : theme.colorScheme.onSurface,
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
              const SizedBox(height: 6),
              ShareBar(
                share: share,
                color: empty ? muted : tone,
                width: _LensStrip.tileWidth - 24,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A length against the range it should fall in: the number, then a short
/// track with the good range drawn on it and a mark where this one lands.
class LengthGauge extends StatelessWidget {
  const LengthGauge({
    super.key,
    required this.length,
    required this.min,
    required this.max,
  });

  /// `null` or 0 when missing.
  final int? length;
  final int min;
  final int max;

  static const double trackWidth = 40;

  @override
  Widget build(BuildContext context) {
    final palette = CrawlStatTheme.palette(context);
    final length = this.length ?? 0;
    // The track runs a little past the maximum, so too long has somewhere
    // to land.
    final scale = max * 1.35;
    final good = length >= min && length <= max;
    final tone = length == 0
        ? palette.error
        : good
        ? palette.success
        : palette.warning;
    double at(num value) => (value / scale).clamp(0.0, 1.0) * trackWidth;
    return Tooltip(
      message: length == 0
          ? 'Missing — aim for $min–$max characters'
          : '$length characters — aim for $min–$max',
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 26,
            child: Text(
              length == 0 ? '—' : '$length',
              textAlign: TextAlign.right,
              style: CrawlStatTheme.mono(
                context,
                size: 11.5,
                color: good ? null : tone,
              ),
            ),
          ),
          const SizedBox(width: 6),
          SizedBox(
            width: trackWidth,
            height: 8,
            child: Stack(
              alignment: Alignment.centerLeft,
              children: [
                Container(
                  height: 3,
                  decoration: BoxDecoration(
                    color: tone.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(2),
                  ),
                ),
                Positioned(
                  left: at(min),
                  width: at(max) - at(min),
                  child: Container(
                    height: 3,
                    decoration: BoxDecoration(
                      color: palette.success.withValues(alpha: 0.35),
                      borderRadius: BorderRadius.circular(2),
                    ),
                  ),
                ),
                if (length > 0)
                  Positioned(
                    left: (at(length) - 1.5).clamp(0, trackWidth - 3),
                    child: Container(
                      width: 3,
                      height: 8,
                      decoration: BoxDecoration(
                        color: tone,
                        borderRadius: BorderRadius.circular(1.5),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// The result tabs as a segmented strip, left-aligned with the cards below.
class _TabBar extends StatelessWidget {
  const _TabBar({
    required this.tabs,
    required this.selected,
    required this.onSelect,
  });

  final List<ResultsTab> tabs;
  final String selected;
  final void Function(String) onSelect;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Align(
      alignment: Alignment.centerLeft,
      child: Container(
        height: 40,
        padding: const EdgeInsets.all(4),
        decoration: BoxDecoration(
          color: theme.colorScheme.surfaceContainerLow,
          borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 2),
          border: Border.all(color: theme.dividerColor.withValues(alpha: 0.7)),
        ),
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final tab in tabs) _tab(context, tab, selected == tab.id),
            ],
          ),
        ),
      ),
    );
  }

  Widget _tab(BuildContext context, ResultsTab tab, bool active) {
    final theme = Theme.of(context);
    return InkWell(
      onTap: () => onSelect(tab.id),
      borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius - 1),
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 150),
        padding: const EdgeInsets.symmetric(horizontal: 13),
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: active ? theme.colorScheme.surfaceContainer : null,
          borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius - 1),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (active) ...[
              Container(
                width: 6,
                height: 6,
                decoration: BoxDecoration(
                  color: CrawlStatTheme.palette(context).accent,
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 7),
            ],
            Text(
              tab.label,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: active
                    ? theme.colorScheme.onSurface
                    : theme.colorScheme.onSurfaceVariant,
                fontWeight: active ? FontWeight.w600 : FontWeight.w500,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _FilterBar extends ConsumerWidget {
  const _FilterBar({
    required this.search,
    required this.focus,
    required this.query,
  });

  final TextEditingController search;
  final FocusNode focus;
  final UrlQuerySpec query;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final table = ref.read(urlTableProvider.notifier);

    return Toolbar(
      children: [
        SizedBox(
          width: 300,
          child: TextField(
            controller: search,
            focusNode: focus,
            decoration: InputDecoration(
              hintText: 'Filter by URL…',
              prefixIcon: const Icon(AppIcon.search, size: 17),
              prefixIconConstraints: const BoxConstraints(minWidth: 36),
              suffixIcon: search.text.isEmpty
                  ? null
                  : IconButton(
                      icon: const Icon(AppIcon.close, size: 14),
                      onPressed: () {
                        search.clear();
                        table.setSearch(null);
                      },
                    ),
            ),
            onChanged: (value) => table.setSearch(value.isEmpty ? null : value),
          ),
        ),
        const SizedBox(width: 12),
        _StatusClassFilter(selected: query.statusClass),
        const Spacer(),
        if (query.issueId != null) ...[
          _ActiveFilterChip(
            label: 'Issue: ${query.issueId}',
            onClear: () => table.setIssue(null),
          ),
          const SizedBox(width: 6),
        ],
        if (query.hasFilters)
          TextButton(
            onPressed: () {
              search.clear();
              table.clearFilters();
            },
            child: const Text('Clear all'),
          ),
      ],
    );
  }
}

class _StatusClassFilter extends ConsumerWidget {
  const _StatusClassFilter({this.selected});

  final int? selected;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final table = ref.read(urlTableProvider.notifier);
    final theme = Theme.of(context);

    return Row(
      children: [
        for (final (value, label) in const [
          (2, '2xx'),
          (3, '3xx'),
          (4, '4xx'),
          (5, '5xx'),
        ])
          Padding(
            padding: const EdgeInsets.only(right: 6),
            child: InkWell(
              onTap: () {
                final query = ref.read(urlTableProvider).query;
                table.setQuery(
                  selected == value
                      ? query.copyWith(clearStatusClass: true)
                      : query.copyWith(statusClass: value),
                );
              },
              borderRadius: BorderRadius.circular(
                CrawlStatTheme.controlRadius - 1,
              ),
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 10,
                  vertical: 8,
                ),
                decoration: BoxDecoration(
                  color: selected == value
                      ? CrawlStatTheme.statusColor(
                          context,
                          value * 100,
                        ).withValues(alpha: 0.16)
                      : null,
                  border: Border.all(
                    color: selected == value
                        ? CrawlStatTheme.statusColor(context, value * 100)
                        : theme.dividerColor,
                  ),
                  borderRadius: BorderRadius.circular(
                    CrawlStatTheme.controlRadius - 1,
                  ),
                ),
                child: Text(
                  label,
                  style: CrawlStatTheme.mono(
                    context,
                    size: 11.5,
                    color: selected == value
                        ? CrawlStatTheme.statusColor(context, value * 100)
                        : theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

class _ActiveFilterChip extends StatelessWidget {
  const _ActiveFilterChip({required this.label, required this.onClear});

  final String label;
  final VoidCallback onClear;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.only(left: 8),
      decoration: BoxDecoration(
        color: CrawlStatTheme.palette(context).accent.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(5),
      ),
      child: Row(
        children: [
          Text(
            label,
            style: theme.textTheme.bodySmall?.copyWith(
              color: CrawlStatTheme.palette(context).accent,
            ),
          ),
          IconButton(
            icon: const Icon(AppIcon.close, size: 13),
            onPressed: onClear,
            visualDensity: VisualDensity.compact,
            constraints: const BoxConstraints(minWidth: 26, minHeight: 26),
            padding: EdgeInsets.zero,
            color: CrawlStatTheme.palette(context).accent,
          ),
        ],
      ),
    );
  }
}

/// Every column the results table can show.
///
/// One list for every tab: the tabs pick from it, so a column behaves
/// identically wherever it appears.
///
/// [limits] are the crawl's own audit thresholds, which title and
/// description lengths and response times are drawn against.
List<GridColumn<UrlRow>> buildUrlColumns(
  BuildContext context, {
  PageLenses? limits,
}) {
  final titleMin = limits?.titleMin.toInt() ?? 30;
  final titleMax = limits?.titleMax.toInt() ?? 60;
  final descriptionMin = limits?.descriptionMin.toInt() ?? 70;
  final descriptionMax = limits?.descriptionMax.toInt() ?? 155;
  final slow = limits?.slowMs.toInt() ?? 1000;
  final palette = CrawlStatTheme.palette(context);
  bool isPage(UrlRow row) =>
      row.kind == 'html' &&
      (row.statusCode ?? 0) >= 200 &&
      row.statusCode! < 300;
  return [
    GridColumn<UrlRow>(
      id: 'status',
      label: 'Code',
      width: 72,
      sortKey: 'status_code',
      value: (row) => row.statusCode?.toString() ?? '',
      cell: (context, row) => StatusChip(row.statusCode),
    ),
    GridColumn<UrlRow>(
      id: 'final_status',
      label: 'Final',
      width: 72,
      tooltip: 'Status at the end of the redirect chain',
      value: (row) => row.finalStatusCode?.toString() ?? '',
      cell: (context, row) => row.finalStatusCode == null
          ? const SizedBox.shrink()
          : StatusChip(row.finalStatusCode),
      defaultVisible: false,
    ),
    GridColumn<UrlRow>(
      id: 'issues',
      label: 'Issues',
      width: 72,
      sortKey: 'issues',
      align: TextAlign.right,
      tooltip: 'Audit issues on this URL, coloured by the worst',
      value: (row) => row.issueCount == 0 ? '' : row.issueCount.toString(),
      cell: (context, row) => row.issueCount == 0
          ? const SizedBox.shrink()
          : Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                SeverityDot(row.worstSeverity ?? 'notice', size: 7),
                const SizedBox(width: 7),
                SizedBox(
                  width: 24,
                  child: Text(
                    formatCount(row.issueCount),
                    textAlign: TextAlign.right,
                    style: CrawlStatTheme.mono(context, size: 11.5),
                  ),
                ),
              ],
            ),
    ),
    GridColumn<UrlRow>(
      id: 'url',
      label: 'URL',
      width: 460,
      sortKey: 'url',
      mono: true,
      grow: true,
      value: (row) => row.url,
    ),
    GridColumn<UrlRow>(
      id: 'indexable',
      label: 'Indexability',
      width: 150,
      value: (row) =>
          row.indexabilityReason ?? (row.indexable == true ? 'Indexable' : ''),
      cell: (context, row) => IndexablePill(
        indexable: row.indexable,
        reason: row.indexabilityReason,
      ),
    ),
    GridColumn<UrlRow>(
      id: 'title',
      label: 'Title',
      width: 300,
      sortKey: 'title',
      value: (row) => row.title ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'title_length',
      label: 'Length',
      width: 104,
      sortKey: 'title_length',
      align: TextAlign.right,
      mono: true,
      tooltip: 'Title length in characters, against $titleMin–$titleMax',
      value: (row) => row.titleLength?.toString() ?? '',
      cell: (context, row) => !isPage(row)
          ? const SizedBox.shrink()
          : Align(
              alignment: Alignment.centerRight,
              child: LengthGauge(
                length: row.titleLength,
                min: titleMin,
                max: titleMax,
              ),
            ),
    ),
    GridColumn<UrlRow>(
      id: 'description',
      label: 'Meta description',
      width: 380,
      value: (row) => row.metaDescription ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'description_length',
      label: 'Length',
      width: 104,
      sortKey: 'meta_description_length',
      align: TextAlign.right,
      mono: true,
      tooltip:
          'Description length in characters, against '
          '$descriptionMin–$descriptionMax',
      value: (row) => row.metaDescriptionLength?.toString() ?? '',
      cell: (context, row) => !isPage(row)
          ? const SizedBox.shrink()
          : Align(
              alignment: Alignment.centerRight,
              child: LengthGauge(
                length: row.metaDescriptionLength,
                min: descriptionMin,
                max: descriptionMax,
              ),
            ),
    ),
    GridColumn<UrlRow>(
      id: 'h1',
      label: 'H1',
      width: 280,
      value: (row) => row.h1 ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'h1_count',
      label: 'H1s',
      width: 48,
      align: TextAlign.right,
      mono: true,
      value: (row) => row.h1Count?.toString() ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'canonical',
      label: 'Canonical',
      width: 380,
      mono: true,
      value: (row) => row.canonical ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'self_canonical',
      label: 'Self',
      width: 52,
      tooltip: 'Whether the canonical points at this URL',
      value: (row) => switch (row.canonicalIsSelf) {
        true => 'yes',
        false => 'no',
        null => '',
      },
      color: (row) => row.canonicalIsSelf == false
          ? CrawlStatTheme.palette(context).warning
          : null,
    ),
    GridColumn<UrlRow>(
      id: 'lang',
      label: 'Lang',
      width: 64,
      mono: true,
      tooltip: 'The html lang attribute',
      value: (row) => row.htmlLang ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'structured_data',
      label: 'Structured data',
      width: 260,
      value: (row) => row.structuredDataTypes ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'word_count',
      label: 'Words',
      width: 64,
      sortKey: 'word_count',
      align: TextAlign.right,
      mono: true,
      value: (row) => row.wordCount == null ? '' : formatCount(row.wordCount!),
    ),
    GridColumn<UrlRow>(
      id: 'depth',
      label: 'Depth',
      width: 56,
      sortKey: 'depth',
      align: TextAlign.right,
      mono: true,
      tooltip: 'Clicks from the start URL',
      value: (row) => row.depth.toString(),
    ),
    GridColumn<UrlRow>(
      id: 'inlinks',
      label: 'Inlinks',
      width: 64,
      sortKey: 'inlinks',
      align: TextAlign.right,
      mono: true,
      tooltip: 'References from other pages on the site',
      value: (row) => formatCount(row.inlinks),
    ),
    GridColumn<UrlRow>(
      id: 'outlinks',
      label: 'Outlinks',
      width: 68,
      align: TextAlign.right,
      mono: true,
      value: (row) => row.internalLinks?.toString() ?? '',
      defaultVisible: false,
    ),
    GridColumn<UrlRow>(
      id: 'link_score',
      label: 'Score',
      width: 58,
      sortKey: 'link_score',
      align: TextAlign.right,
      mono: true,
      tooltip: 'Internal link score, 0–100',
      value: (row) => row.linkScore?.toStringAsFixed(0) ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'redirects',
      label: 'Hops',
      width: 52,
      align: TextAlign.right,
      mono: true,
      tooltip: 'Redirects traversed',
      value: (row) =>
          row.redirectCount == 0 ? '' : row.redirectCount.toString(),
      color: (row) => row.redirectCount > 2
          ? CrawlStatTheme.palette(context).warning
          : null,
    ),
    GridColumn<UrlRow>(
      id: 'content_type',
      label: 'Type',
      width: 150,
      value: (row) => row.contentType?.split(';').first ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'size',
      label: 'Size',
      width: 74,
      sortKey: 'size',
      align: TextAlign.right,
      mono: true,
      value: (row) => formatBytes(row.sizeBytes),
    ),
    GridColumn<UrlRow>(
      id: 'time',
      label: 'Time',
      width: 70,
      sortKey: 'response_time',
      align: TextAlign.right,
      mono: true,
      tooltip: 'Server response time; slow from ${formatMillis(slow)}',
      value: (row) => formatMillis(row.responseTimeMs),
      color: (row) => switch (row.responseTimeMs ?? 0) {
        final ms when ms >= slow => palette.error,
        final ms when ms >= slow ~/ 2 => palette.warning,
        _ => null,
      },
    ),
    GridColumn<UrlRow>(
      id: 'error',
      label: 'Error',
      width: 180,
      value: (row) => row.errorKind ?? '',
      color: (_) => CrawlStatTheme.palette(context).error,
    ),
  ];
}
