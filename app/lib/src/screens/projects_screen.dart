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
      children: [
        ScreenHeader(
          title: 'Projects',
          subtitle: projects.isEmpty
              ? 'Every crawl is a single file on this machine.'
              : '${projects.length} crawl${projects.length == 1 ? '' : 's'} · '
                  '${defaultProjectsDirectory()}',
          actions: [
            ToolbarButton(
              icon: Icons.refresh,
              tooltip: 'Refresh',
              onPressed: () => ref.read(recentProjectsProvider.notifier).refresh(),
            ),
            const SizedBox(width: 6),
            FilledButton.icon(
              onPressed: () => showNewCrawlDialog(context),
              icon: const Icon(Icons.add, size: 15),
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
                    icon: const Icon(Icons.add, size: 15),
                    label: const Text('Start your first crawl'),
                  ),
                )
              : ListView.separated(
                  padding: const EdgeInsets.all(16),
                  itemCount: projects.length,
                  separatorBuilder: (_, _) => const SizedBox(height: 10),
                  itemBuilder: (context, index) {
                    final project = projects[index];
                    return _ProjectCard(
                      project: project,
                      isOpen: project.path == openPath,
                    );
                  },
                ),
        ),
      ],
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

    return Card(
      child: InkWell(
        onTap: () => _open(context, ref),
        borderRadius: BorderRadius.circular(8),
        child: Padding(
          padding: const EdgeInsets.all(14),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Flexible(
                              child: Text(
                                project.rootHost ?? _fileName(project.path),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: theme.textTheme.titleMedium,
                              ),
                            ),
                            const SizedBox(width: 8),
                            _StateBadge(state: project.state),
                            if (isOpen) ...[
                              const SizedBox(width: 6),
                              Text('open', style: theme.textTheme.bodySmall
                                  ?.copyWith(color: CrawlStatTheme.accent)),
                            ],
                          ],
                        ),
                        const SizedBox(height: 3),
                        Text(
                          project.startUrl ?? project.path,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: CrawlStatTheme.mono(
                            context,
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ),
                  ),
                  if (resumable)
                    OutlinedButton.icon(
                      onPressed: () => _resume(context, ref),
                      icon: const Icon(Icons.play_arrow, size: 14),
                      label: const Text('Resume'),
                    ),
                  const SizedBox(width: 6),
                  _ProjectMenu(project: project),
                ],
              ),
              const SizedBox(height: 12),
              Wrap(
                spacing: 22,
                runSpacing: 8,
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
                  _Metric(
                    label: '4xx',
                    value: formatCount(stats.status4Xx),
                    color: stats.status4Xx > 0 ? CrawlStatTheme.error : null,
                  ),
                  _Metric(
                    label: '5xx',
                    value: formatCount(stats.status5Xx),
                    color: stats.status5Xx > 0 ? CrawlStatTheme.error : null,
                  ),
                  if (stats.queued > 0)
                    _Metric(label: 'Queued', value: formatCount(stats.queued)),
                  _Metric(label: 'Size', value: formatBytes(project.sizeBytes)),
                  if (project.finishedAt != null)
                    _Metric(label: 'Finished', value: _shortDate(project.finishedAt!)),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  String _fileName(String path) => path.split(Platform.pathSeparator).last;

  /// `2026-02-24T14:32:10Z` as `24 Feb 14:32`, which is what a user scans for.
  String _shortDate(String iso) {
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

class _Metric extends StatelessWidget {
  const _Metric({required this.label, required this.value, this.color});

  final String label;
  final String value;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
        const SizedBox(height: 2),
        Text(
          value,
          style: CrawlStatTheme.mono(context, size: 14, color: color)
              .copyWith(fontWeight: FontWeight.w600),
        ),
      ],
    );
  }
}

class _StateBadge extends StatelessWidget {
  const _StateBadge({this.state});

  final String? state;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (label, color) = switch (state) {
      'finished' => ('Finished', CrawlStatTheme.success),
      'running' => ('Running', CrawlStatTheme.accent),
      'paused' => ('Paused', CrawlStatTheme.warning),
      'cancelled' => ('Stopped', CrawlStatTheme.warning),
      'failed' => ('Failed', CrawlStatTheme.error),
      _ => ('Unknown', CrawlStatTheme.notice),
    };

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.13),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        label,
        style: theme.textTheme.bodySmall?.copyWith(color: color, fontSize: 11),
      ),
    );
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
      icon: const Icon(Icons.more_horiz, size: 16),
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
