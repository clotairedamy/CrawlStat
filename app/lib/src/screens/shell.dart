import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../router.dart';
import '../state/crawl.dart';
import '../state/projects.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'new_crawl_dialog.dart';

/// The window chrome: a narrow rail on the left, the screen on the right.
///
/// A rail rather than a sidebar because the content is a wide table and every
/// pixel of horizontal space belongs to it. The rail shows a live badge while a
/// crawl runs, so the user can navigate away and still see progress.
class AppShell extends ConsumerWidget {
  const AppShell({super.key, required this.location, required this.child});

  final String location;
  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final crawl = ref.watch(crawlControllerProvider);
    final openProject = ref.watch(openProjectProvider);

    return CallbackShortcuts(
      bindings: {
        CrawlStatShortcuts.newCrawl: () => showNewCrawlDialog(context),
        CrawlStatShortcuts.pauseResume: () {
          final controller = ref.read(crawlControllerProvider.notifier);
          if (crawl.phase == CrawlPhase.paused) {
            controller.resume();
          } else if (crawl.phase == CrawlPhase.running) {
            controller.pause();
          }
        },
      },
      child: Focus(
        autofocus: true,
        child: Scaffold(
          body: Row(
            children: [
              _Rail(location: location, crawl: crawl, hasProject: openProject != null),
              Container(width: 1, color: theme.dividerColor),
              Expanded(child: child),
            ],
          ),
        ),
      ),
    );
  }
}

class _Rail extends ConsumerWidget {
  const _Rail({required this.location, required this.crawl, required this.hasProject});

  final String location;
  final LiveCrawl crawl;
  final bool hasProject;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);

    return Container(
      width: 62,
      color: theme.colorScheme.surface,
      child: Column(
        children: [
          const SizedBox(height: 14),
          Tooltip(
            message: 'CrawlStat',
            child: Container(
              width: 26,
              height: 26,
              decoration: BoxDecoration(
                color: CrawlStatTheme.accent,
                borderRadius: BorderRadius.circular(7),
              ),
              alignment: Alignment.center,
              child: const Text(
                'C',
                style: TextStyle(
                  color: Colors.white,
                  fontWeight: FontWeight.w700,
                  fontSize: 15,
                  height: 1,
                ),
              ),
            ),
          ),
          const SizedBox(height: 18),
          _RailItem(
            icon: Icons.folder_outlined,
            label: 'Projects',
            path: Routes.projects,
            location: location,
          ),
          _RailItem(
            icon: Icons.radar_outlined,
            label: 'Crawl',
            path: Routes.crawl,
            location: location,
            // A dot while the crawl runs, so progress is visible from any screen.
            badge: crawl.isRunning || crawl.phase == CrawlPhase.auditing,
          ),
          _RailItem(
            icon: Icons.table_rows_outlined,
            label: 'Results',
            path: Routes.results,
            location: location,
            enabled: hasProject,
          ),
          _RailItem(
            icon: Icons.rule_outlined,
            label: 'Issues',
            path: Routes.issues,
            location: location,
            enabled: hasProject,
          ),
          const Spacer(),
          _RailItem(
            icon: Icons.tune_outlined,
            label: 'Settings',
            path: Routes.settings,
            location: location,
          ),
          const SizedBox(height: 8),
          Tooltip(
            message: 'New crawl  (${_shortcutLabel()})',
            child: IconButton(
              onPressed: () => showNewCrawlDialog(context),
              icon: const Icon(Icons.add, size: 18),
              style: IconButton.styleFrom(
                backgroundColor: CrawlStatTheme.accent,
                foregroundColor: Colors.white,
                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(7)),
                minimumSize: const Size(30, 30),
              ),
            ),
          ),
          const SizedBox(height: 14),
        ],
      ),
    );
  }

  String _shortcutLabel() => defaultTargetPlatform == TargetPlatform.macOS ? '⌘N' : 'Ctrl+N';
}

class _RailItem extends StatelessWidget {
  const _RailItem({
    required this.icon,
    required this.label,
    required this.path,
    required this.location,
    this.enabled = true,
    this.badge = false,
  });

  final IconData icon;
  final String label;
  final String path;
  final String location;
  final bool enabled;
  final bool badge;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final selected = location == path;
    final color = !enabled
        ? theme.colorScheme.onSurfaceVariant.withValues(alpha: 0.4)
        : selected
            ? CrawlStatTheme.accent
            : theme.colorScheme.onSurfaceVariant;

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      child: Tooltip(
        message: enabled ? label : '$label — open a project first',
        child: InkWell(
          onTap: enabled ? () => context.go(path) : null,
          borderRadius: BorderRadius.circular(7),
          child: Container(
            height: 42,
            decoration: BoxDecoration(
              color: selected ? CrawlStatTheme.accent.withValues(alpha: 0.12) : null,
              borderRadius: BorderRadius.circular(7),
            ),
            child: Stack(
              alignment: Alignment.center,
              children: [
                Icon(icon, size: 18, color: color),
                if (badge)
                  Positioned(
                    top: 9,
                    right: 11,
                    child: Container(
                      width: 6,
                      height: 6,
                      decoration: const BoxDecoration(
                        color: CrawlStatTheme.success,
                        shape: BoxShape.circle,
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// A heading strip at the top of a screen.
class ScreenHeader extends StatelessWidget {
  const ScreenHeader({
    super.key,
    required this.title,
    this.subtitle,
    this.actions = const [],
  });

  final String title;
  final String? subtitle;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.fromLTRB(20, 16, 14, 14),
      decoration: BoxDecoration(
        color: theme.colorScheme.surface,
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(title, style: theme.textTheme.titleLarge),
                if (subtitle != null) ...[
                  const SizedBox(height: 2),
                  Text(
                    subtitle!,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ],
            ),
          ),
          ...actions,
        ],
      ),
    );
  }
}

/// Shown on Results and Issues when no crawl is open.
class NoProjectState extends StatelessWidget {
  const NoProjectState({super.key});

  @override
  Widget build(BuildContext context) {
    return EmptyState(
      icon: Icons.folder_open_outlined,
      title: 'No crawl open',
      message: 'Open a recent crawl from Projects, or start a new one.',
      action: FilledButton.icon(
        onPressed: () => showNewCrawlDialog(context),
        icon: const Icon(Icons.add, size: 15),
        label: const Text('New crawl'),
      ),
    );
  }
}
