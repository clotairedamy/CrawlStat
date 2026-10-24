import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../router.dart';
import '../rust/api/types.dart';
import '../state/crawl.dart';
import '../state/results.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/data_grid.dart';
import 'new_crawl_dialog.dart';
import 'shell.dart';

/// The live crawl: counters, a rate graph, and the URLs as they come in.
class CrawlScreen extends ConsumerWidget {
  const CrawlScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final crawl = ref.watch(crawlControllerProvider);

    if (crawl.phase == CrawlPhase.idle) {
      return Column(
        children: [
          const ScreenHeader(title: 'Crawl'),
          Expanded(
            child: EmptyState(
              icon: Icons.radar_outlined,
              title: 'Nothing crawling',
              message: 'Start a crawl and this screen shows it happening, '
                  'URL by URL, with the counters updating live.',
              action: FilledButton.icon(
                onPressed: () => showNewCrawlDialog(context),
                icon: const Icon(Icons.add, size: 15),
                label: const Text('New crawl'),
              ),
            ),
          ),
        ],
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _CrawlHeader(crawl: crawl),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(
              CrawlStatTheme.gutter,
              0,
              CrawlStatTheme.gutter,
              CrawlStatTheme.gutter,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (crawl.error != null)
                  InlineMessage(
                    message: crawl.error!,
                    severity: 'error',
                    margin: const EdgeInsets.only(bottom: 16),
                  ),
                _Counters(crawl: crawl),
                const SizedBox(height: 16),
                _ProgressCard(crawl: crawl),
                if (crawl.warnings.isNotEmpty) ...[
                  const SizedBox(height: 16),
                  _Warnings(warnings: crawl.warnings),
                ],
                const SizedBox(height: 16),
                Expanded(
                  child: Card(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        _TableHeading(count: crawl.rows.length),
                        Expanded(child: _LiveTable(rows: crawl.rows, phase: crawl.phase)),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

/// The phase as a word and a colour, shared by the header and the progress.
(String, Color) _phaseLook(CrawlPhase phase) => switch (phase) {
      CrawlPhase.starting => ('Starting', CrawlStatTheme.accent),
      CrawlPhase.running => ('Crawling', CrawlStatTheme.success),
      CrawlPhase.paused => ('Paused', CrawlStatTheme.warning),
      CrawlPhase.auditing => ('Auditing', CrawlStatTheme.accent),
      CrawlPhase.finished => ('Finished', CrawlStatTheme.success),
      CrawlPhase.cancelled => ('Stopped', CrawlStatTheme.warning),
      CrawlPhase.failed => ('Failed', CrawlStatTheme.error),
      CrawlPhase.idle => ('Idle', CrawlStatTheme.notice),
    };

class _CrawlHeader extends ConsumerWidget {
  const _CrawlHeader({required this.crawl});

  final LiveCrawl crawl;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = ref.read(crawlControllerProvider.notifier);
    final (label, color) = _phaseLook(crawl.phase);
    final host = Uri.tryParse(crawl.startUrl ?? '')?.host;
    final live = crawl.isRunning || crawl.phase == CrawlPhase.auditing;

    final details = [
      ?crawl.startUrl,
      if (crawl.resumed) 'resumed',
      if (crawl.seeded > 0) '${formatCount(crawl.seeded)} seeded',
    ];

    return ScreenHeader(
      title: host == null || host.isEmpty ? (crawl.startUrl ?? 'Crawl') : host,
      leading: TonePill(label, color: color, dot: live),
      subtitle: details.isEmpty ? null : details.join('  ·  '),
      actions: [
        if (crawl.phase == CrawlPhase.running)
          OutlinedButton.icon(
            onPressed: controller.pause,
            icon: const Icon(Icons.pause_rounded, size: 16),
            label: const Text('Pause'),
          ),
        if (crawl.phase == CrawlPhase.paused)
          FilledButton.icon(
            onPressed: controller.resume,
            icon: const Icon(Icons.play_arrow_rounded, size: 16),
            label: const Text('Resume'),
          ),
        if (crawl.isRunning)
          OutlinedButton.icon(
            onPressed: controller.cancel,
            style: OutlinedButton.styleFrom(foregroundColor: CrawlStatTheme.error),
            icon: const Icon(Icons.stop_rounded, size: 16),
            label: const Text('Stop'),
          ),
        if (crawl.isFinished) ...[
          OutlinedButton(
            onPressed: controller.clear,
            child: const Text('Clear'),
          ),
          FilledButton.icon(
            onPressed: () {
              // The crawl is over, so the results are complete: send the
              // table back to the database for the full set.
              ref.read(urlTableProvider.notifier).reset();
              context.go(Routes.results);
            },
            icon: const Icon(Icons.table_chart_outlined, size: 16),
            label: const Text('See results'),
          ),
        ],
      ],
    );
  }
}

class _Counters extends ConsumerWidget {
  const _Counters({required this.crawl});

  final LiveCrawl crawl;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final snapshot = crawl.snapshot;
    final summary = crawl.summary;

    final crawled = summary?.crawled ?? snapshot?.crawled ?? 0;
    final discovered = summary?.discovered ?? snapshot?.discovered ?? 0;
    final errors = summary?.errors ?? snapshot?.errors ?? 0;
    final notFound = summary?.status4Xx ?? snapshot?.status4Xx ?? 0;
    final serverErrors = summary?.status5Xx ?? snapshot?.status5Xx ?? 0;
    final audit = crawl.audit;

    final tiles = [
      StatTile(
        label: 'Crawled',
        icon: Icons.check_circle_outline_rounded,
        color: CrawlStatTheme.success,
        value: formatCount(crawled),
        hint: 'Fetched and stored',
      ),
      StatTile(
        label: 'Queued',
        icon: Icons.schedule_rounded,
        value: formatCount(snapshot?.queued ?? 0),
        hint: 'Waiting to be fetched',
      ),
      StatTile(
        label: 'Discovered',
        icon: Icons.travel_explore_rounded,
        value: formatCount(discovered),
        hint: 'Known to the crawl',
      ),
      StatTile(
        label: 'Client errors',
        icon: Icons.link_off_rounded,
        color: notFound > 0 ? CrawlStatTheme.error : CrawlStatTheme.notice,
        value: formatCount(notFound),
        hint: '4xx responses',
      ),
      StatTile(
        label: 'Server errors',
        icon: Icons.dns_outlined,
        color: serverErrors > 0 ? CrawlStatTheme.error : CrawlStatTheme.notice,
        value: formatCount(serverErrors),
        hint: '5xx responses',
      ),
      if (audit != null)
        StatTile(
          label: 'Issues',
          icon: Icons.fact_check_outlined,
          color: audit.errors > 0 ? CrawlStatTheme.error : CrawlStatTheme.warning,
          value: formatCount(audit.errors + audit.warnings),
          hint: '${audit.errors} errors, ${audit.warnings} warnings',
          onTap: () => context.go(Routes.issues),
        )
      else
        StatTile(
          label: 'Failed',
          icon: Icons.error_outline_rounded,
          color: errors > 0 ? CrawlStatTheme.error : CrawlStatTheme.notice,
          value: formatCount(errors),
          hint: 'Could not be fetched',
        ),
    ];

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final (index, tile) in tiles.indexed) ...[
          if (index > 0) const SizedBox(width: 14),
          Expanded(child: tile),
        ],
      ],
    );
  }
}

/// How far along the crawl is, and how fast it is going.
class _ProgressCard extends StatelessWidget {
  const _ProgressCard({required this.crawl});

  final LiveCrawl crawl;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final snapshot = crawl.snapshot;
    final summary = crawl.summary;
    final (_, color) = _phaseLook(crawl.phase);

    final crawled = summary?.crawled ?? snapshot?.crawled ?? 0;
    final discovered = summary?.discovered ?? snapshot?.discovered ?? 0;
    final rate = summary?.urlsPerSecond ?? snapshot?.urlsPerSecond ?? 0;
    final elapsed = summary?.elapsedMs ?? snapshot?.elapsedMs ?? 0;
    final fraction = crawl.isFinished ? 1.0 : crawl.fraction;
    final live = crawl.isRunning || crawl.phase == CrawlPhase.auditing;

    Widget metric(String label, String value) => Padding(
          padding: const EdgeInsets.only(left: 28),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Text(label, style: theme.textTheme.bodySmall),
              const SizedBox(height: 2),
              Text(
                value,
                style: theme.textTheme.titleSmall?.copyWith(
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
            ],
          ),
        );

    return Card(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 14, 18, 16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Progress', style: theme.textTheme.bodySmall),
                      const SizedBox(height: 2),
                      Text.rich(
                        TextSpan(
                          children: [
                            TextSpan(
                              text: fraction == null ? '—' : '${(fraction * 100).round()}%',
                              style: theme.textTheme.titleLarge?.copyWith(fontSize: 22),
                            ),
                            TextSpan(
                              text: '   ${formatCount(crawled)} of ${formatCount(discovered)} URLs',
                              style: theme.textTheme.bodySmall,
                            ),
                          ],
                        ),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ],
                  ),
                ),
                metric('Speed', '${rate.toStringAsFixed(rate >= 10 ? 0 : 1)} URLs/s'),
                if (live) metric('In flight', formatCount(snapshot?.inFlight ?? 0)),
                metric('Elapsed', formatMillis(elapsed)),
              ],
            ),
            const SizedBox(height: 14),
            ClipRRect(
              borderRadius: BorderRadius.circular(6),
              child: LinearProgressIndicator(
                value: fraction ?? (live ? null : 0),
                minHeight: 8,
                color: color,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _TableHeading extends StatelessWidget {
  const _TableHeading({required this.count});

  final int count;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.fromLTRB(18, 14, 18, 12),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      child: Row(
        children: [
          Text('Live URLs', style: theme.textTheme.titleSmall),
          const SizedBox(width: 10),
          Text(
            count >= liveRowLimit ? 'newest $liveRowLimit' : 'newest first',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

/// The warnings the engine raised, newest first and collapsed by default.
class _Warnings extends StatefulWidget {
  const _Warnings({required this.warnings});

  final List<CrawlWarning> warnings;

  @override
  State<_Warnings> createState() => _WarningsState();
}

class _WarningsState extends State<_Warnings> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final latest = widget.warnings.first;

    return Material(
      color: CrawlStatTheme.warning.withValues(alpha: 0.09),
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 1),
        side: BorderSide(color: CrawlStatTheme.warning.withValues(alpha: 0.25)),
      ),
      child: Column(
        children: [
          InkWell(
            onTap: () => setState(() => _expanded = !_expanded),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              child: Row(
                children: [
                  const Icon(
                    Icons.warning_amber_rounded,
                    size: 16,
                    color: CrawlStatTheme.warning,
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text(
                      _expanded
                          ? '${widget.warnings.length} warning'
                              '${widget.warnings.length == 1 ? '' : 's'}'
                          : latest.message,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall
                          ?.copyWith(color: CrawlStatTheme.warning),
                    ),
                  ),
                  if (widget.warnings.length > 1 && !_expanded)
                    Text(
                      '+${widget.warnings.length - 1}',
                      style: theme.textTheme.bodySmall
                          ?.copyWith(color: CrawlStatTheme.warning),
                    ),
                  Icon(
                    _expanded ? Icons.expand_less : Icons.expand_more,
                    size: 15,
                    color: CrawlStatTheme.warning,
                  ),
                ],
              ),
            ),
          ),
          if (_expanded)
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 150),
              child: ListView.builder(
                padding: const EdgeInsets.fromLTRB(40, 0, 14, 10),
                itemCount: widget.warnings.length,
                itemBuilder: (context, index) {
                  final warning = widget.warnings[index];
                  return Padding(
                    padding: const EdgeInsets.symmetric(vertical: 2),
                    child: Text(
                      warning.url == null
                          ? warning.message
                          : '${warning.message}  —  ${warning.url}',
                      style: theme.textTheme.bodySmall,
                    ),
                  );
                },
              ),
            ),
        ],
      ),
    );
  }
}

/// URLs as they arrive, newest at the top.
///
/// Bounded to the last few hundred: the full results come from the database the
/// moment the crawl ends, so there is no reason to accumulate them twice.
class _LiveTable extends StatelessWidget {
  const _LiveTable({required this.rows, required this.phase});

  final List<CrawledUrlEvent> rows;
  final CrawlPhase phase;

  @override
  Widget build(BuildContext context) {
    if (rows.isEmpty) {
      return EmptyState(
        icon: phase == CrawlPhase.starting ? Icons.hourglass_empty : Icons.link_off,
        title: phase == CrawlPhase.starting ? 'Starting up' : 'No URLs yet',
        message: phase == CrawlPhase.starting
            ? 'Reading robots.txt and seeding the frontier.'
            : 'The first results will appear here.',
      );
    }

    final columns = <GridColumn<CrawledUrlEvent>>[
      GridColumn(
        id: 'status',
        label: 'Code',
        width: 72,
        value: (row) => row.statusCode?.toString() ?? '—',
        cell: (context, row) => StatusChip(row.statusCode),
      ),
      GridColumn(
        id: 'url',
        label: 'URL',
        width: 460,
        mono: true,
        grow: true,
        value: (row) => row.url,
      ),
      GridColumn(
        id: 'title',
        label: 'Title',
        width: 280,
        value: (row) => row.title ?? '',
      ),
      GridColumn(
        id: 'depth',
        label: 'Depth',
        width: 58,
        align: TextAlign.right,
        mono: true,
        value: (row) => row.depth.toString(),
      ),
      GridColumn(
        id: 'time',
        label: 'Time',
        width: 76,
        align: TextAlign.right,
        mono: true,
        value: (row) => formatMillis(row.responseTimeMs),
      ),
      GridColumn(
        id: 'error',
        label: 'Error',
        width: 220,
        value: (row) => row.error ?? '',
        color: (_) => CrawlStatTheme.error,
      ),
    ];

    return DataGrid<CrawledUrlEvent>(
      columns: columns,
      total: rows.length,
      rowAt: (index) => index < rows.length ? rows[index] : null,
      // Everything is already in memory here, so nothing is ever fetched.
      onNeedRow: (_) {},
      rowKey: (row) => row.urlId,
    );
  }
}
