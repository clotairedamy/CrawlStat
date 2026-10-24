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
      children: [
        _CrawlHeader(crawl: crawl),
        if (crawl.error != null)
          InlineMessage(message: crawl.error!, severity: 'error'),
        _Counters(crawl: crawl),
        if (crawl.warnings.isNotEmpty) _Warnings(warnings: crawl.warnings),
        Expanded(child: _LiveTable(rows: crawl.rows, phase: crawl.phase)),
      ],
    );
  }
}

class _CrawlHeader extends ConsumerWidget {
  const _CrawlHeader({required this.crawl});

  final LiveCrawl crawl;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = ref.read(crawlControllerProvider.notifier);
    final (label, color) = switch (crawl.phase) {
      CrawlPhase.starting => ('Starting', CrawlStatTheme.accent),
      CrawlPhase.running => ('Crawling', CrawlStatTheme.accent),
      CrawlPhase.paused => ('Paused', CrawlStatTheme.warning),
      CrawlPhase.auditing => ('Auditing', CrawlStatTheme.accent),
      CrawlPhase.finished => ('Finished', CrawlStatTheme.success),
      CrawlPhase.cancelled => ('Stopped', CrawlStatTheme.warning),
      CrawlPhase.failed => ('Failed', CrawlStatTheme.error),
      CrawlPhase.idle => ('Idle', CrawlStatTheme.notice),
    };

    return Column(
      children: [
        ScreenHeader(
          title: crawl.startUrl ?? 'Crawl',
          subtitle: '$label${crawl.resumed ? ' · resumed' : ''}'
              '${crawl.seeded > 0 ? ' · ${formatCount(crawl.seeded)} seeded' : ''}',
          actions: [
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
              decoration: BoxDecoration(
                color: color.withValues(alpha: 0.13),
                borderRadius: BorderRadius.circular(5),
              ),
              child: Row(
                children: [
                  if (crawl.isRunning || crawl.phase == CrawlPhase.auditing)
                    const Padding(
                      padding: EdgeInsets.only(right: 6),
                      child: SizedBox(
                        width: 10,
                        height: 10,
                        child: CircularProgressIndicator(strokeWidth: 1.8),
                      ),
                    ),
                  Text(
                    label,
                    style: Theme.of(context)
                        .textTheme
                        .bodySmall
                        ?.copyWith(color: color, fontWeight: FontWeight.w600),
                  ),
                ],
              ),
            ),
            const SizedBox(width: 10),
            if (crawl.phase == CrawlPhase.running)
              OutlinedButton.icon(
                onPressed: controller.pause,
                icon: const Icon(Icons.pause, size: 14),
                label: const Text('Pause'),
              ),
            if (crawl.phase == CrawlPhase.paused)
              FilledButton.icon(
                onPressed: controller.resume,
                icon: const Icon(Icons.play_arrow, size: 14),
                label: const Text('Resume'),
              ),
            if (crawl.isRunning) ...[
              const SizedBox(width: 6),
              OutlinedButton.icon(
                onPressed: controller.cancel,
                icon: const Icon(Icons.stop, size: 14),
                label: const Text('Stop'),
              ),
            ],
            if (crawl.isFinished) ...[
              FilledButton.icon(
                onPressed: () {
                  // The crawl is over, so the results are complete: send the
                  // table back to the database for the full set.
                  ref.read(urlTableProvider.notifier).reset();
                  context.go(Routes.results);
                },
                icon: const Icon(Icons.table_rows_outlined, size: 14),
                label: const Text('See results'),
              ),
              const SizedBox(width: 6),
              OutlinedButton(
                onPressed: controller.clear,
                child: const Text('Clear'),
              ),
            ],
          ],
        ),
        if (crawl.isRunning || crawl.phase == CrawlPhase.auditing)
          LinearProgressIndicator(
            value: crawl.fraction,
            minHeight: 2,
            backgroundColor: Theme.of(context).colorScheme.surface,
          ),
      ],
    );
  }
}

class _Counters extends ConsumerWidget {
  const _Counters({required this.crawl});

  final LiveCrawl crawl;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final snapshot = crawl.snapshot;
    final summary = crawl.summary;

    final crawled = summary?.crawled ?? snapshot?.crawled ?? 0;
    final discovered = summary?.discovered ?? snapshot?.discovered ?? 0;
    final errors = summary?.errors ?? snapshot?.errors ?? 0;
    final notFound = summary?.status4Xx ?? snapshot?.status4Xx ?? 0;
    final serverErrors = summary?.status5Xx ?? snapshot?.status5Xx ?? 0;
    final rate = summary?.urlsPerSecond ?? snapshot?.urlsPerSecond ?? 0;
    final elapsed = summary?.elapsedMs ?? snapshot?.elapsedMs ?? 0;

    return Container(
      decoration: BoxDecoration(
        color: theme.colorScheme.surface,
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
      child: Row(
        children: [
          StatTile(label: 'Crawled', value: formatCount(crawled)),
          StatTile(
            label: 'Queued',
            value: formatCount(snapshot?.queued ?? 0),
            hint: 'Discovered but not yet fetched',
          ),
          StatTile(
            label: 'In flight',
            value: formatCount(snapshot?.inFlight ?? 0),
            hint: 'Requests open right now',
          ),
          StatTile(
            label: 'Discovered',
            value: formatCount(discovered),
            hint: 'Every URL the crawl knows about',
          ),
          StatTile(
            label: '4xx',
            value: formatCount(notFound),
            color: notFound > 0 ? CrawlStatTheme.error : null,
          ),
          StatTile(
            label: '5xx',
            value: formatCount(serverErrors),
            color: serverErrors > 0 ? CrawlStatTheme.error : null,
          ),
          StatTile(
            label: 'Failed',
            value: formatCount(errors),
            color: errors > 0 ? CrawlStatTheme.error : null,
            hint: 'Could not be fetched at all',
          ),
          const Spacer(),
          StatTile(
            label: 'URLs/sec',
            value: rate.toStringAsFixed(rate >= 10 ? 0 : 1),
            hint: 'Over the last few seconds',
          ),
          StatTile(label: 'Elapsed', value: formatMillis(elapsed)),
          if (crawl.audit != null)
            StatTile(
              label: 'Issues',
              value: formatCount(crawl.audit!.errors + crawl.audit!.warnings),
              color: crawl.audit!.errors > 0 ? CrawlStatTheme.error : CrawlStatTheme.warning,
              hint: '${crawl.audit!.errors} errors, ${crawl.audit!.warnings} warnings',
              onTap: () => context.go(Routes.issues),
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

    return Container(
      decoration: BoxDecoration(
        color: CrawlStatTheme.warning.withValues(alpha: 0.08),
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      child: Column(
        children: [
          InkWell(
            onTap: () => setState(() => _expanded = !_expanded),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
              child: Row(
                children: [
                  const Icon(
                    Icons.warning_amber_outlined,
                    size: 14,
                    color: CrawlStatTheme.warning,
                  ),
                  const SizedBox(width: 8),
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
                padding: const EdgeInsets.fromLTRB(34, 0, 12, 8),
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
        width: 62,
        align: TextAlign.right,
        value: (row) => row.statusCode?.toString() ?? '—',
        cell: (context, row) => StatusChip(row.statusCode),
      ),
      GridColumn(
        id: 'url',
        label: 'URL',
        width: 540,
        mono: true,
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
