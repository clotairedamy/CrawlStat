import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../theme/icons.dart';
import '../router.dart';
import '../rust/api/types.dart';
import '../state/crawl.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/gauge.dart';
import '../widgets/data_grid.dart';
import 'new_crawl_dialog.dart';
import 'projects_screen.dart' show previousCrawl;
import 'shell.dart';
import '../theme/soft.dart';

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
              icon: AppIcon.crawl,
              title: 'Nothing crawling',
              message:
                  'Start a crawl and this screen shows it happening, '
                  'URL by URL, with the counters updating live.',
              action: FilledButton.icon(
                onPressed: () => showNewCrawlDialog(context),
                icon: const Icon(AppIcon.add, size: 15),
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
            // Slivers rather than a Column with an Expanded tail.
            //
            // The tiles above the table wrap to more rows as the window
            // narrows, and a plain Column has nowhere to put that extra
            // height: Expanded collapses to zero and the fixed children
            // overflow, which is the yellow-and-black bar. SliverFillRemaining
            // gives the table the rest of the viewport when there is room and
            // lets the whole screen scroll when there is not.
            child: CustomScrollView(
              slivers: [
                SliverToBoxAdapter(
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
                    ],
                  ),
                ),
                SliverFillRemaining(
                  hasScrollBody: true,
                  child: ConstrainedBox(
                    // Below this the table shows nothing useful, so it keeps
                    // its own height and the page scrolls instead.
                    constraints: const BoxConstraints(minHeight: 220),
                    child: SoftPanel(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          _TableHeading(count: crawl.rows.length),
                          Expanded(
                            child: _LiveTable(
                              rows: crawl.rows,
                              phase: crawl.phase,
                            ),
                          ),
                        ],
                      ),
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
(String, Color) _phaseLook(BuildContext context, CrawlPhase phase) {
  final palette = CrawlStatTheme.palette(context);
  return switch (phase) {
    CrawlPhase.starting => ('Starting', palette.accent),
    CrawlPhase.running => ('Crawling', palette.success),
    CrawlPhase.paused => ('Paused', palette.warning),
    CrawlPhase.auditing => ('Auditing', palette.accent),
    CrawlPhase.finished => ('Finished', palette.success),
    CrawlPhase.cancelled => ('Stopped', palette.warning),
    CrawlPhase.failed => ('Failed', palette.error),
    CrawlPhase.idle => ('Idle', palette.notice),
  };
}

class _CrawlHeader extends ConsumerWidget {
  const _CrawlHeader({required this.crawl});

  final LiveCrawl crawl;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = ref.read(crawlControllerProvider.notifier);
    final (label, color) = _phaseLook(context, crawl.phase);
    final host = Uri.tryParse(crawl.startUrl ?? '')?.host;
    final live = crawl.isRunning || crawl.phase == CrawlPhase.auditing;

    // A finished crawl of a site crawled before is usually a recrawl, and the
    // question after a recrawl is what changed.
    final recents = ref.watch(recentProjectsProvider);
    final current = crawl.isFinished
        ? recents.where((p) => p.path == crawl.dbPath).firstOrNull
        : null;
    final previous = current == null ? null : previousCrawl(current, recents);

    final details = [
      ?crawl.startUrl,
      if (crawl.resumed) 'resumed',
      if (crawl.seeded > 0) '${formatCount(crawl.seeded)} seeded',
    ];

    // Inside a crawl's page, which already says which site it is.
    final title = ProjectScope.isInside(context)
        ? 'Progress'
        : host == null || host.isEmpty
        ? (crawl.startUrl ?? 'Crawl')
        : host;

    return ScreenHeader(
      title: title,
      leading: TonePill(label, color: color, dot: live),
      subtitle: details.isEmpty ? null : details.join('  ·  '),
      actions: [
        if (crawl.phase == CrawlPhase.running)
          OutlinedButton.icon(
            onPressed: controller.pause,
            icon: const Icon(AppIcon.pause, size: 16),
            label: const Text('Pause'),
          ),
        if (crawl.phase == CrawlPhase.paused)
          FilledButton.icon(
            onPressed: controller.resume,
            icon: const Icon(AppIcon.play, size: 16),
            label: const Text('Resume'),
          ),
        if (crawl.isRunning)
          OutlinedButton.icon(
            onPressed: controller.cancel,
            style: OutlinedButton.styleFrom(
              foregroundColor: CrawlStatTheme.palette(context).error,
            ),
            icon: const Icon(AppIcon.stop, size: 16),
            label: const Text('Stop'),
          ),
        if (crawl.isFinished) ...[
          OutlinedButton(
            onPressed: controller.clear,
            child: const Text('Clear'),
          ),
          if (previous != null)
            OutlinedButton.icon(
              onPressed: () => context.go(
                Routes.comparing(before: previous.path, after: current!.path),
              ),
              icon: const Icon(AppIcon.compare, size: 16),
              label: const Text('Compare with previous crawl'),
            ),
          FilledButton.icon(
            onPressed: () {
              // The crawl is over, so the results are complete: send the
              // table back to the database for the full set.
              ref.read(urlTableProvider.notifier).reset();
              context.go(Routes.results);
            },
            icon: const Icon(AppIcon.results, size: 16),
            label: const Text('See pages'),
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
        icon: AppIcon.ok,
        color: CrawlStatTheme.palette(context).success,
        value: formatCount(crawled),
        hint: 'Fetched and stored',
      ),
      StatTile(
        label: 'Queued',
        icon: AppIcon.queued,
        value: formatCount(snapshot?.queued ?? 0),
        hint: 'Waiting to be fetched',
      ),
      StatTile(
        label: 'Discovered',
        icon: AppIcon.discovered,
        value: formatCount(discovered),
        hint: 'Known to the crawl',
      ),
      StatTile(
        label: 'Client errors',
        icon: AppIcon.brokenLink,
        color: notFound > 0
            ? CrawlStatTheme.palette(context).error
            : CrawlStatTheme.palette(context).notice,
        value: formatCount(notFound),
        hint: '4xx responses',
      ),
      StatTile(
        label: 'Server errors',
        icon: AppIcon.server,
        color: serverErrors > 0
            ? CrawlStatTheme.palette(context).error
            : CrawlStatTheme.palette(context).notice,
        value: formatCount(serverErrors),
        hint: '5xx responses',
      ),
      if (audit != null)
        StatTile(
          label: 'Issues',
          icon: AppIcon.issues,
          color: audit.errors > 0
              ? CrawlStatTheme.palette(context).error
              : CrawlStatTheme.palette(context).warning,
          value: formatCount(audit.errors + audit.warnings),
          hint: '${audit.errors} errors, ${audit.warnings} warnings',
          onTap: () => context.go(Routes.issues),
        )
      else
        StatTile(
          label: 'Failed',
          icon: AppIcon.error,
          color: errors > 0
              ? CrawlStatTheme.palette(context).error
              : CrawlStatTheme.palette(context).notice,
          value: formatCount(errors),
          hint: 'Could not be fetched',
        ),
    ];

    // Tiles wrap to a second line rather than overflowing. A Row of Expanded
    // children looks right at full width and then paints Flutter's
    // yellow-and-black overflow stripes the moment the window is narrowed,
    // because an Expanded still cannot shrink below the tile's intrinsic
    // width.
    const spacing = 14.0;
    const minTileWidth = 168.0;
    return LayoutBuilder(
      builder: (context, constraints) {
        final available = constraints.maxWidth;
        var perRow = ((available + spacing) / (minTileWidth + spacing)).floor();
        perRow = perRow.clamp(1, tiles.length);
        final width = (available - spacing * (perRow - 1)) / perRow;
        return Wrap(
          spacing: spacing,
          runSpacing: spacing,
          children: [
            for (final tile in tiles) SizedBox(width: width, child: tile),
          ],
        );
      },
    );
  }
}

/// The hero of the crawl screen: a dial, a scrubber and the running totals.
///
/// A crawl has a speed and a completion, and both move every second. A dial
/// reads a changing rate far better than digits do — you can see at a glance
/// whether the crawl is moving or has stalled, without reading anything.
class _ProgressCard extends StatefulWidget {
  const _ProgressCard({required this.crawl});

  final LiveCrawl crawl;

  @override
  State<_ProgressCard> createState() => _ProgressCardState();
}

class _ProgressCardState extends State<_ProgressCard> {
  /// The fastest rate seen so far.
  ///
  /// The dial is scaled to this rather than to the current reading, and only
  /// ever grows. A scale that tracked the live value would pin the needle in
  /// the same place whatever the rate, which tells you nothing.
  double _peakRate = 0;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final crawl = widget.crawl;
    final snapshot = crawl.snapshot;
    final summary = crawl.summary;

    final crawled = summary?.crawled ?? snapshot?.crawled ?? 0;
    final discovered = summary?.discovered ?? snapshot?.discovered ?? 0;
    final queued = snapshot?.queued ?? 0;
    final rate = summary?.urlsPerSecond ?? snapshot?.urlsPerSecond ?? 0;
    final elapsed = summary?.elapsedMs ?? snapshot?.elapsedMs ?? 0;
    final fraction = crawl.isFinished ? 1.0 : crawl.fraction;
    final live = crawl.isRunning || crawl.phase == CrawlPhase.auditing;

    if (rate > _peakRate) {
      _peakRate = rate.toDouble();
    }

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(22, 20, 22, 20),
      child: LayoutBuilder(
        builder: (context, constraints) {
          // Side by side when there is room for both; stacked when the dial
          // would leave the readouts nothing to live in.
          final side = constraints.maxWidth >= 640;
          final gauge = CrawlGauge(
            value: rate.toDouble(),
            max: gaugeCeiling(_peakRate),
            label: 'URL/s',
            size: 232,
            progress: fraction,
            secondaryValue: formatCount(crawled),
            secondaryLabel: 'crawled',
            live: live,
          );

          final details = Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              // A spider crawl does not know its total until it has found
              // it, so the bar sweeps rather than lying about a percentage.
              SoftProgressBar(
                value: fraction ?? 0,
                indeterminate: fraction == null && live,
                height: 9,
              ),
              const SizedBox(height: 8),
              // This used to be printed inside the dial, where it ran over
              // the tick ring. It belongs beside the bar it describes.
              Text(
                fraction == null
                    ? 'finding the size of the site'
                    : '${(fraction * 100).round()}% of what is known',
                style: theme.textTheme.bodySmall,
              ),
              const SizedBox(height: 18),
              Wrap(
                spacing: 34,
                runSpacing: 16,
                children: [
                  _Readout(label: 'Discovered', value: formatCount(discovered)),
                  _Readout(label: 'Queued', value: formatCount(queued)),
                  _Readout(
                    label: 'In flight',
                    value: formatCount(snapshot?.inFlight ?? 0),
                  ),
                  _Readout(label: 'Elapsed', value: formatMillis(elapsed)),
                  if (_peakRate > 0)
                    _Readout(
                      label: 'Peak',
                      value:
                          '${_peakRate.toStringAsFixed(_peakRate >= 10 ? 0 : 1)}/s',
                    ),
                ],
              ),
              if (crawl.startUrl != null) ...[
                const SizedBox(height: 18),
                Text(
                  crawl.startUrl!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: CrawlStatTheme.mono(
                    context,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ],
            ],
          );

          if (!side) {
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: [
                Center(child: gauge),
                const SizedBox(height: 22),
                details,
              ],
            );
          }
          return Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              gauge,
              const SizedBox(width: 26),
              Expanded(child: details),
            ],
          );
        },
      ),
    );
  }
}

/// A label over a figure, for the block beside the dial.
class _Readout extends StatelessWidget {
  const _Readout({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
        const SizedBox(height: 3),
        Text(
          value,
          style: CrawlStatTheme.mono(
            context,
            size: 19,
          ).copyWith(fontWeight: FontWeight.w600),
        ),
      ],
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
      color: CrawlStatTheme.palette(context).warning.withValues(alpha: 0.09),
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 1),
        side: BorderSide(
          color: CrawlStatTheme.palette(context).warning
              .withValues(alpha: 0.25),
        ),
      ),
      child: Column(
        children: [
          InkWell(
            onTap: () => setState(() => _expanded = !_expanded),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              child: Row(
                children: [
                  Icon(
                    AppIcon.warning,
                    size: 16,
                    color: CrawlStatTheme.palette(context).warning,
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
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: CrawlStatTheme.palette(context).warning,
                      ),
                    ),
                  ),
                  if (widget.warnings.length > 1 && !_expanded)
                    Text(
                      '+${widget.warnings.length - 1}',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: CrawlStatTheme.palette(context).warning,
                      ),
                    ),
                  Icon(
                    _expanded ? AppIcon.chevronUp : AppIcon.chevronDown,
                    size: 15,
                    color: CrawlStatTheme.palette(context).warning,
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
        icon: phase == CrawlPhase.starting
            ? AppIcon.waiting
            : AppIcon.brokenLink,
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
        color: (_) => CrawlStatTheme.palette(context).error,
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
