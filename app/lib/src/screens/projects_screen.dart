import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../router.dart';
import '../rust/api/types.dart';
import '../state/crawl.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'new_crawl_dialog.dart';
import 'shell.dart';

/// The crawls the user has run, newest first.
class ProjectsScreen extends ConsumerWidget {
  const ProjectsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final projects = ref.watch(recentProjectsProvider);
    final openPath = ref.watch(openProjectProvider);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ScreenHeader(
          title: 'Projects',
          subtitle: projects.isEmpty
              ? 'Every crawl is a single file on this machine.'
              : '${projects.length} crawl${projects.length == 1 ? '' : 's'}  ·  '
                  '${defaultProjectsDirectory()}',
          actions: [
            ToolbarButton(
              icon: Icons.refresh_rounded,
              tooltip: 'Refresh',
              onPressed: () => ref.read(recentProjectsProvider.notifier).refresh(),
            ),
            FilledButton.icon(
              onPressed: () => showNewCrawlDialog(context),
              icon: const Icon(Icons.add_rounded, size: 17),
              label: const Text('New crawl'),
            ),
          ],
        ),
        Expanded(
          child: projects.isEmpty
              ? EmptyState(
                  icon: Icons.travel_explore_outlined,
                  title: 'No crawls yet',
                  message:
                      'Point CrawlStat at a site and it will fetch every page, store the '
                      'results locally, and tell you what is wrong with them.',
                  action: FilledButton.icon(
                    onPressed: () => showNewCrawlDialog(context),
                    icon: const Icon(Icons.add_rounded, size: 17),
                    label: const Text('Start your first crawl'),
                  ),
                )
              : ListView(
                  padding: const EdgeInsets.fromLTRB(
                    CrawlStatTheme.gutter,
                    0,
                    CrawlStatTheme.gutter,
                    CrawlStatTheme.gutter,
                  ),
                  children: [
                    _Overview(projects: projects),
                    const SizedBox(height: 28),
                    Text('Recent crawls', style: Theme.of(context).textTheme.titleMedium),
                    const SizedBox(height: 14),
                    _ProjectGrid(projects: projects, openPath: openPath),
                  ],
                ),
        ),
      ],
    );
  }
}

/// Totals across every crawl, as the dashboard's headline row.
class _Overview extends StatelessWidget {
  const _Overview({required this.projects});

  final List<ProjectInfo> projects;

  @override
  Widget build(BuildContext context) {
    final crawled = projects.fold<int>(0, (sum, p) => sum + p.stats.crawled);
    final errors = projects.fold<int>(0, (sum, p) => sum + p.errors);
    final warnings = projects.fold<int>(0, (sum, p) => sum + p.warnings);
    final broken = projects.fold<int>(0, (sum, p) => sum + p.stats.status4Xx + p.stats.status5Xx);

    final tiles = [
      StatTile(
        label: 'Crawls',
        icon: Icons.folder_outlined,
        value: formatCount(projects.length),
        hint: 'On this machine',
      ),
      StatTile(
        label: 'URLs crawled',
        icon: Icons.language_rounded,
        color: CrawlStatTheme.success,
        value: formatCount(crawled),
        hint: 'Across every crawl',
      ),
      StatTile(
        label: 'Broken URLs',
        icon: Icons.link_off_rounded,
        color: broken > 0 ? CrawlStatTheme.error : CrawlStatTheme.notice,
        value: formatCount(broken),
        hint: '4xx and 5xx responses',
      ),
      StatTile(
        label: 'Issues',
        icon: Icons.fact_check_outlined,
        color: errors > 0 ? CrawlStatTheme.warning : CrawlStatTheme.notice,
        value: formatCount(errors + warnings),
        hint: '${formatCount(errors)} errors, ${formatCount(warnings)} warnings',
      ),
    ];

    return Row(
      children: [
        for (final (index, tile) in tiles.indexed) ...[
          if (index > 0) const SizedBox(width: 14),
          Expanded(child: tile),
        ],
      ],
    );
  }
}

/// Project cards in as many equal columns as fit.
class _ProjectGrid extends StatelessWidget {
  const _ProjectGrid({required this.projects, required this.openPath});

  final List<ProjectInfo> projects;
  final String? openPath;

  static const double _minCardWidth = 340;
  static const double _gap = 14;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final columns = ((constraints.maxWidth + _gap) / (_minCardWidth + _gap)).floor().clamp(1, 4);
        final width = (constraints.maxWidth - _gap * (columns - 1)) / columns;
        return Wrap(
          spacing: _gap,
          runSpacing: _gap,
          children: [
            for (final project in projects)
              SizedBox(
                width: width,
                child: _ProjectCard(project: project, isOpen: project.path == openPath),
              ),
          ],
        );
      },
    );
  }
}

class _ProjectCard extends ConsumerWidget {
  const _ProjectCard({required this.project, required this.isOpen});

  final ProjectInfo project;
  final bool isOpen;

  void _open(BuildContext context, WidgetRef ref) {
    ref.read(openProjectProvider.notifier).open(project.path);
    ref.read(urlTableProvider.notifier).reset();
    context.go(Routes.results);
  }

  Future<void> _resume(BuildContext context, WidgetRef ref) async {
    ref.read(openProjectProvider.notifier).open(project.path);
    await ref.read(crawlControllerProvider.notifier).resumeProject(project.path);
    if (context.mounted) context.go(Routes.crawl);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final stats = project.stats;
    final resumable = project.state == 'cancelled' ||
        project.state == 'paused' ||
        project.state == 'running' ||
        stats.queued > 0;
    final host = project.rootHost ?? _fileName(project.path);

    return Card(
      shape: isOpen
          ? RoundedRectangleBorder(
              side: BorderSide(color: CrawlStatTheme.accent.withValues(alpha: 0.6), width: 1.2),
              borderRadius: BorderRadius.circular(CrawlStatTheme.radius),
            )
          : null,
      child: InkWell(
        onTap: () => _open(context, ref),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(18, 16, 10, 16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  _HostAvatar(host: host),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          host,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.titleSmall?.copyWith(fontSize: 15),
                        ),
                        const SizedBox(height: 2),
                        Text(
                          project.startUrl ?? project.path,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: CrawlStatTheme.mono(
                            context,
                            size: 11.5,
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ),
                  ),
                  _ProjectMenu(project: project),
                ],
              ),
              const SizedBox(height: 14),
              Padding(
                padding: const EdgeInsets.only(right: 8),
                child: Row(
                  children: [
                    _StateBadge(state: project.state),
                    if (isOpen) ...[
                      const SizedBox(width: 6),
                      const TonePill('Open', color: CrawlStatTheme.accent),
                    ],
                    const Spacer(),
                    Text(
                      _shortDate(project.finishedAt ?? project.createdAt),
                      style: theme.textTheme.bodySmall,
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 14),
              Padding(
                padding: const EdgeInsets.only(right: 8),
                child: _StatusBreakdown(stats: stats),
              ),
              const SizedBox(height: 16),
              Padding(
                padding: const EdgeInsets.only(right: 8),
                child: Row(
                  children: [
                    _Metric(label: 'Crawled', value: formatCount(stats.crawled)),
                    _Metric(
                      label: 'Errors',
                      value: formatCount(project.errors),
                      color: project.errors > 0 ? CrawlStatTheme.error : null,
                    ),
                    _Metric(
                      label: 'Warnings',
                      value: formatCount(project.warnings),
                      color: project.warnings > 0 ? CrawlStatTheme.warning : null,
                    ),
                    _Metric(label: 'Size', value: formatBytes(project.sizeBytes)),
                  ],
                ),
              ),
              if (resumable) ...[
                const SizedBox(height: 14),
                Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          stats.queued > 0
                              ? '${formatCount(stats.queued)} URLs still queued'
                              : 'Interrupted before it finished',
                          style: theme.textTheme.bodySmall,
                        ),
                      ),
                      OutlinedButton.icon(
                        onPressed: () => _resume(context, ref),
                        icon: const Icon(Icons.play_arrow_rounded, size: 16),
                        label: const Text('Resume'),
                      ),
                    ],
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }

  String _fileName(String path) => path.split(Platform.pathSeparator).last;

  /// `2026-02-24T14:32:10Z` as `24 Feb 14:32`, which is what a user scans for.
  String _shortDate(String? iso) {
    if (iso == null) return '';
    final parsed = DateTime.tryParse(iso);
    if (parsed == null) return iso;
    const months = [
      'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
      'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
    ];
    final local = parsed.toLocal();
    final time = '${local.hour.toString().padLeft(2, '0')}:'
        '${local.minute.toString().padLeft(2, '0')}';
    return '${local.day} ${months[local.month - 1]} $time';
  }
}

/// A coloured initial standing in for the site's favicon, which would need a
/// network request just to decorate a list.
class _HostAvatar extends StatelessWidget {
  const _HostAvatar({required this.host});

  final String host;

  static const _palette = [
    Color(0xFF6366F1),
    Color(0xFF22B573),
    Color(0xFFF2A12E),
    Color(0xFFEC4899),
    Color(0xFF0EA5E9),
    Color(0xFF8B5CF6),
  ];

  @override
  Widget build(BuildContext context) {
    final color = _palette[host.codeUnits.fold<int>(0, (a, b) => a + b) % _palette.length];
    return Container(
      width: 38,
      height: 38,
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.16),
        borderRadius: BorderRadius.circular(10),
      ),
      alignment: Alignment.center,
      child: Text(
        host.isEmpty ? '?' : host.characters.first.toUpperCase(),
        style: TextStyle(color: color, fontWeight: FontWeight.w700, fontSize: 16),
      ),
    );
  }
}

/// The share of each status class, as one segmented bar with a legend.
class _StatusBreakdown extends StatelessWidget {
  const _StatusBreakdown({required this.stats});

  final ProjectStats stats;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final segments = [
      ('2xx', stats.status2Xx, CrawlStatTheme.success),
      ('3xx', stats.status3Xx, CrawlStatTheme.warning),
      ('4xx', stats.status4Xx, CrawlStatTheme.error),
      ('5xx', stats.status5Xx, const Color(0xFFB91C4B)),
      ('Failed', stats.failed, CrawlStatTheme.notice),
    ];
    final total = segments.fold<int>(0, (sum, segment) => sum + segment.$2);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ClipRRect(
          borderRadius: BorderRadius.circular(4),
          child: SizedBox(
            height: 6,
            child: total == 0
                ? ColoredBox(color: theme.colorScheme.surfaceContainer)
                : Row(
                    children: [
                      for (final (_, count, color) in segments)
                        if (count > 0)
                          Expanded(
                            flex: (count * 1000 / total).ceil(),
                            child: Container(
                              color: color,
                              margin: const EdgeInsets.only(right: 2),
                            ),
                          ),
                    ],
                  ),
          ),
        ),
        const SizedBox(height: 8),
        Wrap(
          spacing: 14,
          runSpacing: 4,
          children: [
            for (final (label, count, color) in segments)
              if (count > 0 || label == '2xx')
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      width: 7,
                      height: 7,
                      decoration: BoxDecoration(color: color, shape: BoxShape.circle),
                    ),
                    const SizedBox(width: 5),
                    Text('$label ', style: theme.textTheme.bodySmall),
                    Text(
                      formatCount(count),
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurface,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ],
                ),
          ],
        ),
      ],
    );
  }
}

class _Metric extends StatelessWidget {
  const _Metric({required this.label, required this.value, this.color});

  final String label;
  final String value;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(label, style: theme.textTheme.bodySmall),
          const SizedBox(height: 3),
          Text(
            value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.titleSmall?.copyWith(
              color: color,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
        ],
      ),
    );
  }
}

class _StateBadge extends StatelessWidget {
  const _StateBadge({this.state});

  final String? state;

  @override
  Widget build(BuildContext context) {
    final (label, color) = switch (state) {
      'finished' => ('Finished', CrawlStatTheme.success),
      'running' => ('Running', CrawlStatTheme.accent),
      'paused' => ('Paused', CrawlStatTheme.warning),
      'cancelled' => ('Stopped', CrawlStatTheme.warning),
      'failed' => ('Failed', CrawlStatTheme.error),
      _ => ('Unknown', CrawlStatTheme.notice),
    };
    return TonePill(label, color: color, dot: true);
  }
}

class _ProjectMenu extends ConsumerWidget {
  const _ProjectMenu({required this.project});

  final ProjectInfo project;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return PopupMenuButton<String>(
      tooltip: 'More',
      position: PopupMenuPosition.under,
      icon: const Icon(Icons.more_vert_rounded, size: 18),
      iconSize: 16,
      onSelected: (action) async {
        final notifier = ref.read(recentProjectsProvider.notifier);
        switch (action) {
          case 'reveal':
            // No native file dialog is bundled, so the path goes to the
            // clipboard instead — which is what the user needs it for anyway.
            await Clipboard.setData(ClipboardData(text: project.path));
            if (context.mounted) {
              ScaffoldMessenger.of(context).showSnackBar(
                const SnackBar(content: Text('Path copied to the clipboard')),
              );
            }
          case 'forget':
            await notifier.forget(project.path);
          case 'delete':
            final confirmed = await _confirmDelete(context);
            if (confirmed) await notifier.forget(project.path, deleteFile: true);
        }
      },
      itemBuilder: (context) => const [
        PopupMenuItem(value: 'reveal', height: 34, child: Text('Copy file path')),
        PopupMenuItem(value: 'forget', height: 34, child: Text('Remove from list')),
        PopupMenuDivider(),
        PopupMenuItem(value: 'delete', height: 34, child: Text('Delete crawl file…')),
      ],
    );
  }

  Future<bool> _confirmDelete(BuildContext context) async {
    final result = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete this crawl?'),
        content: Text(
          'The file and everything in it will be removed from disk. '
          'This cannot be undone.\n\n${project.path}',
          style: Theme.of(context).textTheme.bodySmall,
        ),
        actions: [
          OutlinedButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Keep'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            style: FilledButton.styleFrom(backgroundColor: CrawlStatTheme.error),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    return result ?? false;
  }
}
