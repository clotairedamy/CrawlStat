import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../state/settings.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/data_grid.dart';
import 'shell.dart';
import 'url_detail_panel.dart';

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
      columns: ['status', 'final_status', 'url', 'redirects', 'content_type', 'time', 'error'],
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
      columns: ['status', 'url', 'description', 'description_length', 'indexable'],
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
    table.setQuery(current.copyWith(tab: _tab.filterTab, sortBy: _tab.sortBy, descending: false));
  }

  Set<String> get _visibleColumns => _columnOverrides[_tabId] ?? _tab.columns.toSet();

  Future<void> _export() async {
    final path = ref.read(openProjectProvider);
    if (path == null) return;
    final settings = ref.read(settingsProvider);
    final query = ref.read(urlTableProvider).query;

    final directory = Directory(settings.resolvedExportDirectory);
    if (!directory.existsSync()) directory.createSync(recursive: true);
    final file =
        '${directory.path}${Platform.pathSeparator}'
        'crawlstat-$_tabId-${DateTime.now().millisecondsSinceEpoch}.csv';

    try {
      final rows = await rust.exportUrlsCsv(
        dbPath: path,
        // Limit and offset are ignored by the export: it streams everything
        // the filter matches.
        query: query.toRequest(offset: 0, limit: 1),
        outPath: file,
      );
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Exported ${formatCount(rows)} rows'),
          action: SnackBarAction(
            label: 'Copy path',
            onPressed: () => Clipboard.setData(ClipboardData(text: file)),
          ),
        ),
      );
    } on Object catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('Export failed: $error')));
    }
  }

  @override
  Widget build(BuildContext context) {
    final path = ref.watch(openProjectProvider);
    if (path == null) {
      return const Column(
        children: [
          ScreenHeader(title: 'Results'),
          Expanded(child: NoProjectState()),
        ],
      );
    }

    final table = ref.watch(urlTableProvider);
    final selected = ref.watch(selectedUrlProvider);
    final columns = buildUrlColumns(context);

    return CallbackShortcuts(
      bindings: {
        CrawlStatShortcuts.focusFilter: _searchFocus.requestFocus,
        CrawlStatShortcuts.exportCsv: _export,
        CrawlStatShortcuts.closePanel: ref.read(selectedUrlProvider.notifier).clear,
      },
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ScreenHeader(
            title: 'Results',
            subtitle:
                '${formatCount(table.total)} '
                '${table.total == 1 ? 'row' : 'rows'}'
                '${table.query.hasFilters ? ' matching the filter' : ''}',
            actions: [
              ColumnChooser(
                columns: [for (final column in columns) (column.id, column.label)],
                visible: _visibleColumns,
                onChanged: (next) => setState(() => _columnOverrides[_tabId] = next),
              ),
              OutlinedButton.icon(
                onPressed: _export,
                icon: const Icon(Icons.file_download_outlined, size: 16),
                label: const Text('Export CSV'),
              ),
            ],
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: CrawlStatTheme.gutter),
            child: _TabBar(
              tabs: ResultsTab.all,
              selected: _tabId,
              onSelect: (id) {
                setState(() => _tabId = id);
                _applyTab();
              },
            ),
          ),
          const SizedBox(height: 14),
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
                    child: Card(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          _FilterBar(search: _search, focus: _searchFocus, query: table.query),
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
                              onNeedRow: ref.read(urlTableProvider.notifier).ensureIndexLoaded,
                              sortBy: table.query.sortBy,
                              descending: table.query.descending,
                              onSort: ref.read(urlTableProvider.notifier).sortBy,
                              selectedKey: selected,
                              rowKey: (row) => row.id,
                              onSelect: (row) =>
                                  ref.read(selectedUrlProvider.notifier).select(row.id),
                              emptyState: EmptyState(
                                icon: Icons.filter_alt_off_outlined,
                                title: table.query.hasFilters ? 'Nothing matches' : 'No rows',
                                message: table.query.hasFilters
                                    ? 'Try widening the filter, or clear it to see everything.'
                                    : 'This tab has nothing in it for this crawl.',
                                action: table.query.hasFilters
                                    ? OutlinedButton(
                                        onPressed: () {
                                          _search.clear();
                                          ref.read(urlTableProvider.notifier).clearFilters();
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
                    Card(
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

/// The result tabs as a segmented strip, left-aligned with the cards below.
class _TabBar extends StatelessWidget {
  const _TabBar({required this.tabs, required this.selected, required this.onSelect});

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
            children: [for (final tab in tabs) _tab(context, tab, selected == tab.id)],
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
                decoration: const BoxDecoration(
                  color: CrawlStatTheme.accent,
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 7),
            ],
            Text(
              tab.label,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: active ? theme.colorScheme.onSurface : theme.colorScheme.onSurfaceVariant,
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
  const _FilterBar({required this.search, required this.focus, required this.query});

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
              prefixIcon: const Icon(Icons.search_rounded, size: 17),
              prefixIconConstraints: const BoxConstraints(minWidth: 36),
              suffixIcon: search.text.isEmpty
                  ? null
                  : IconButton(
                      icon: const Icon(Icons.close, size: 14),
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
          _ActiveFilterChip(label: 'Issue: ${query.issueId}', onClear: () => table.setIssue(null)),
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
        for (final (value, label) in const [(2, '2xx'), (3, '3xx'), (4, '4xx'), (5, '5xx')])
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
              borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius - 1),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                decoration: BoxDecoration(
                  color: selected == value
                      ? CrawlStatTheme.statusColor(value * 100).withValues(alpha: 0.16)
                      : null,
                  border: Border.all(
                    color: selected == value
                        ? CrawlStatTheme.statusColor(value * 100)
                        : theme.dividerColor,
                  ),
                  borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius - 1),
                ),
                child: Text(
                  label,
                  style: CrawlStatTheme.mono(
                    context,
                    size: 11.5,
                    color: selected == value
                        ? CrawlStatTheme.statusColor(value * 100)
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
        color: CrawlStatTheme.accent.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(5),
      ),
      child: Row(
        children: [
          Text(label, style: theme.textTheme.bodySmall?.copyWith(color: CrawlStatTheme.accent)),
          IconButton(
            icon: const Icon(Icons.close, size: 13),
            onPressed: onClear,
            visualDensity: VisualDensity.compact,
            constraints: const BoxConstraints(minWidth: 26, minHeight: 26),
            padding: EdgeInsets.zero,
            color: CrawlStatTheme.accent,
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
List<GridColumn<UrlRow>> buildUrlColumns(BuildContext context) {
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
      cell: (context, row) =>
          row.finalStatusCode == null ? const SizedBox.shrink() : StatusChip(row.finalStatusCode),
      defaultVisible: false,
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
      value: (row) => row.indexabilityReason ?? (row.indexable == true ? 'Indexable' : ''),
      cell: (context, row) =>
          IndexablePill(indexable: row.indexable, reason: row.indexabilityReason),
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
      label: 'Len',
      width: 52,
      sortKey: 'title_length',
      align: TextAlign.right,
      mono: true,
      tooltip: 'Title length in characters',
      value: (row) => row.titleLength?.toString() ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'description',
      label: 'Meta description',
      width: 380,
      value: (row) => row.metaDescription ?? '',
    ),
    GridColumn<UrlRow>(
      id: 'description_length',
      label: 'Len',
      width: 52,
      sortKey: 'meta_description_length',
      align: TextAlign.right,
      mono: true,
      value: (row) => row.metaDescriptionLength?.toString() ?? '',
    ),
    GridColumn<UrlRow>(id: 'h1', label: 'H1', width: 280, value: (row) => row.h1 ?? ''),
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
      color: (row) => row.canonicalIsSelf == false ? CrawlStatTheme.warning : null,
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
      value: (row) => row.redirectCount == 0 ? '' : row.redirectCount.toString(),
      color: (row) => row.redirectCount > 2 ? CrawlStatTheme.warning : null,
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
      tooltip: 'Server response time',
      value: (row) => formatMillis(row.responseTimeMs),
      color: (row) => (row.responseTimeMs ?? 0) > 1000 ? CrawlStatTheme.warning : null,
    ),
    GridColumn<UrlRow>(
      id: 'error',
      label: 'Error',
      width: 180,
      value: (row) => row.errorKind ?? '',
      color: (_) => CrawlStatTheme.error,
    ),
  ];
}
