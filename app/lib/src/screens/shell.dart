import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../router.dart';
import '../state/crawl.dart';
import '../state/license.dart';
import '../state/projects.dart';
import '../state/settings.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'new_crawl_dialog.dart';

/// Below this window width the sidebar starts collapsed, so the table keeps
/// its room. The user's own choice, once made, wins.
const double _autoCollapseWidth = 1100;

const double _sidebarExpanded = 236;
const double _sidebarCollapsed = 76;

/// Labels show only once the sidebar is wide enough to hold them, which keeps
/// the collapse animation free of half-squeezed text.
const double _labelThreshold = 180;

/// The window chrome: a collapsible sidebar on the left, the screen on the
/// right.
///
/// The sidebar shows where you are, a live card while a crawl runs (so the
/// user can navigate away and still see progress), and folds down to an icon
/// rail when the content needs the width.
class AppShell extends ConsumerWidget {
  const AppShell({super.key, required this.location, required this.child});

  final String location;
  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final crawl = ref.watch(crawlControllerProvider);
    final openProject = ref.watch(openProjectProvider);
    final preference = ref.watch(settingsProvider.select((s) => s.sidebarCollapsed));

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
          body: LayoutBuilder(
            builder: (context, constraints) {
              final collapsed = preference ?? constraints.maxWidth < _autoCollapseWidth;
              return Row(
                children: [
                  _Sidebar(
                    location: location,
                    crawl: crawl,
                    hasProject: openProject != null,
                    collapsed: collapsed,
                    onToggle: () => ref
                        .read(settingsProvider.notifier)
                        .setSidebarCollapsed(!collapsed),
                  ),
                  Expanded(child: child),
                ],
              );
            },
          ),
        ),
      ),
    );
  }
}

class _Sidebar extends ConsumerWidget {
  const _Sidebar({
    required this.location,
    required this.crawl,
    required this.hasProject,
    required this.collapsed,
    required this.onToggle,
  });

  final String location;
  final LiveCrawl crawl;
  final bool hasProject;
  final bool collapsed;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final live = crawl.isRunning ||
        crawl.phase == CrawlPhase.starting ||
        crawl.phase == CrawlPhase.auditing;
    final isPro = ref.watch(isProProvider);

    return AnimatedContainer(
      duration: const Duration(milliseconds: 220),
      curve: Curves.easeOutCubic,
      width: collapsed ? _sidebarCollapsed : _sidebarExpanded,
      decoration: BoxDecoration(
        color: theme.colorScheme.surface,
        border: Border(
          right: BorderSide(color: theme.dividerColor.withValues(alpha: 0.6)),
        ),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final wide = constraints.maxWidth >= _labelThreshold;
          return Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const SizedBox(height: 18),
                _Brand(wide: wide),
                const SizedBox(height: 22),
                _NewCrawlButton(wide: wide),
                const SizedBox(height: 22),
                _SectionLabel('Menu', wide: wide),
                _NavItem(
                  icon: Icons.space_dashboard_outlined,
                  selectedIcon: Icons.space_dashboard,
                  label: 'Projects',
                  path: Routes.projects,
                  location: location,
                  wide: wide,
                ),
                _NavItem(
                  icon: Icons.radar_outlined,
                  selectedIcon: Icons.radar,
                  label: 'Crawl',
                  path: Routes.crawl,
                  location: location,
                  wide: wide,
                  // Visible from any screen, so progress is never out of sight.
                  live: live,
                ),
                _NavItem(
                  icon: Icons.table_chart_outlined,
                  selectedIcon: Icons.table_chart,
                  label: 'Results',
                  path: Routes.results,
                  location: location,
                  wide: wide,
                  enabled: hasProject,
                ),
                _NavItem(
                  icon: Icons.fact_check_outlined,
                  selectedIcon: Icons.fact_check,
                  label: 'Issues',
                  path: Routes.issues,
                  location: location,
                  wide: wide,
                  enabled: hasProject,
                ),
                const Spacer(),
                if (wide && live) ...[
                  _LiveCrawlCard(crawl: crawl),
                  const SizedBox(height: 12),
                ] else if (wide && !isPro) ...[
                  const _UpgradeCard(),
                  const SizedBox(height: 12),
                ],
                _NavItem(
                  icon: Icons.settings_outlined,
                  selectedIcon: Icons.settings,
                  label: 'Settings',
                  path: Routes.settings,
                  location: location,
                  wide: wide,
                ),
                _SidebarButton(
                  icon: collapsed
                      ? Icons.keyboard_double_arrow_right_rounded
                      : Icons.keyboard_double_arrow_left_rounded,
                  label: 'Collapse',
                  tooltip: collapsed ? 'Expand sidebar' : 'Collapse sidebar',
                  wide: wide,
                  onTap: onToggle,
                ),
                const SizedBox(height: 14),
              ],
            ),
          );
        },
      ),
    );
  }
}

/// The square every sidebar row puts its icon in. Its width equals the
/// collapsed sidebar's content width, so icons do not move when it collapses.
const double _leading = _sidebarCollapsed - 24;

class _Brand extends StatelessWidget {
  const _Brand({required this.wide});

  final bool wide;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final logo = Container(
      width: 34,
      height: 34,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(10),
        gradient: const LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [Color(0xFF818CF8), CrawlStatTheme.accent, Color(0xFF4338CA)],
        ),
        boxShadow: [
          BoxShadow(
            color: CrawlStatTheme.accent.withValues(alpha: 0.35),
            blurRadius: 12,
            offset: const Offset(0, 4),
          ),
        ],
      ),
      alignment: Alignment.center,
      child: const Icon(Icons.radar, size: 19, color: Colors.white),
    );

    return SizedBox(
      height: 40,
      child: Row(
        children: [
          SizedBox(width: _leading, child: Center(child: logo)),
          if (wide) ...[
            const SizedBox(width: 6),
            Expanded(
              child: Text.rich(
                TextSpan(
                  children: [
                    const TextSpan(text: 'Crawl'),
                    TextSpan(
                      text: 'Stat',
                      style: TextStyle(color: theme.colorScheme.onSurfaceVariant),
                    ),
                  ],
                ),
                maxLines: 1,
                overflow: TextOverflow.clip,
                softWrap: false,
                style: theme.textTheme.titleMedium?.copyWith(
                  fontSize: 18,
                  fontWeight: FontWeight.w700,
                  letterSpacing: -0.3,
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _NewCrawlButton extends StatelessWidget {
  const _NewCrawlButton({required this.wide});

  final bool wide;

  @override
  Widget build(BuildContext context) {
    final shortcut = defaultTargetPlatform == TargetPlatform.macOS ? '⌘N' : 'Ctrl+N';
    final button = Material(
      color: CrawlStatTheme.accent,
      borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 1),
      child: InkWell(
        onTap: () => showNewCrawlDialog(context),
        borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 1),
        hoverColor: Colors.white.withValues(alpha: 0.08),
        child: SizedBox(
          height: 42,
          child: wide
              ? Row(
                  children: [
                    const SizedBox(
                      width: _leading,
                      child: Icon(Icons.add_rounded, size: 20, color: Colors.white),
                    ),
                    const Expanded(
                      child: Text(
                        'New crawl',
                        maxLines: 1,
                        softWrap: false,
                        overflow: TextOverflow.clip,
                        style: TextStyle(
                          color: Colors.white,
                          fontWeight: FontWeight.w600,
                          fontSize: 13.5,
                        ),
                      ),
                    ),
                    Padding(
                      padding: const EdgeInsets.only(right: 12),
                      child: Text(
                        shortcut,
                        style: TextStyle(
                          color: Colors.white.withValues(alpha: 0.65),
                          fontSize: 12,
                        ),
                      ),
                    ),
                  ],
                )
              : const Center(
                  child: Icon(Icons.add_rounded, size: 20, color: Colors.white),
                ),
        ),
      ),
    );

    return wide ? button : Tooltip(message: 'New crawl  ($shortcut)', child: button);
  }
}

class _SectionLabel extends StatelessWidget {
  const _SectionLabel(this.text, {required this.wide});

  final String text;
  final bool wide;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      height: 28,
      child: wide
          ? Padding(
              padding: const EdgeInsets.only(left: 14),
              child: Align(
                alignment: Alignment.centerLeft,
                child: Text(text.toUpperCase(), style: theme.textTheme.labelMedium),
              ),
            )
          : Center(child: Container(width: 18, height: 1, color: theme.dividerColor)),
    );
  }
}

class _NavItem extends StatelessWidget {
  const _NavItem({
    required this.icon,
    required this.selectedIcon,
    required this.label,
    required this.path,
    required this.location,
    required this.wide,
    this.enabled = true,
    this.live = false,
  });

  final IconData icon;
  final IconData selectedIcon;
  final String label;
  final String path;
  final String location;
  final bool wide;
  final bool enabled;
  final bool live;

  @override
  Widget build(BuildContext context) {
    return _SidebarButton(
      icon: location == path ? selectedIcon : icon,
      label: label,
      tooltip: enabled ? label : '$label — open a project first',
      wide: wide,
      selected: location == path,
      enabled: enabled,
      live: live,
      onTap: () => context.go(path),
    );
  }
}

/// One row of the sidebar: an icon in a fixed square, and a label when there
/// is room for it.
class _SidebarButton extends StatelessWidget {
  const _SidebarButton({
    required this.icon,
    required this.label,
    required this.tooltip,
    required this.wide,
    required this.onTap,
    this.selected = false,
    this.enabled = true,
    this.live = false,
  });

  final IconData icon;
  final String label;
  final String tooltip;
  final bool wide;
  final bool selected;
  final bool enabled;
  final bool live;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final subdued = theme.colorScheme.onSurfaceVariant;
    final color = !enabled
        ? subdued.withValues(alpha: 0.4)
        : selected
            ? theme.colorScheme.onSurface
            : subdued;

    final iconWidget = Stack(
      clipBehavior: Clip.none,
      children: [
        Icon(icon, size: 19, color: selected ? CrawlStatTheme.accent : color),
        if (live && !wide)
          const Positioned(top: -2, right: -3, child: _LiveDot()),
      ],
    );

    final row = AnimatedContainer(
      duration: const Duration(milliseconds: 150),
      height: 42,
      decoration: BoxDecoration(
        color: selected ? CrawlStatTheme.accent.withValues(alpha: 0.14) : null,
        borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 1),
      ),
      child: Row(
        children: [
          SizedBox(width: _leading, child: Center(child: iconWidget)),
          if (wide) ...[
            Expanded(
              child: Text(
                label,
                maxLines: 1,
                softWrap: false,
                overflow: TextOverflow.clip,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: color,
                  fontSize: 13.5,
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                ),
              ),
            ),
            if (live)
              const Padding(
                padding: EdgeInsets.only(right: 10),
                child: _LivePill(),
              ),
          ],
        ],
      ),
    );

    final button = Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Material(
        type: MaterialType.transparency,
        child: InkWell(
          onTap: enabled ? onTap : null,
          borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 1),
          child: row,
        ),
      ),
    );

    // Expanded, the label says it all; collapsed, the tooltip stands in for
    // it. A disabled item always explains itself.
    if (wide && enabled) return button;
    return Tooltip(message: tooltip, preferBelow: false, child: button);
  }
}

class _LiveDot extends StatelessWidget {
  const _LiveDot();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 8,
      height: 8,
      decoration: BoxDecoration(
        color: CrawlStatTheme.success,
        shape: BoxShape.circle,
        border: Border.all(color: Theme.of(context).colorScheme.surface, width: 1.5),
      ),
    );
  }
}

class _LivePill extends StatelessWidget {
  const _LivePill();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
      decoration: BoxDecoration(
        color: CrawlStatTheme.success.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(20),
      ),
      child: const Text(
        'LIVE',
        style: TextStyle(
          color: CrawlStatTheme.success,
          fontSize: 10,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.6,
        ),
      ),
    );
  }
}

/// Progress of the running crawl, visible from every screen.
class _LiveCrawlCard extends StatelessWidget {
  const _LiveCrawlCard({required this.crawl});

  final LiveCrawl crawl;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final snapshot = crawl.snapshot;
    final fraction = crawl.fraction;
    final host = Uri.tryParse(crawl.startUrl ?? '')?.host;
    final (label, color) = switch (crawl.phase) {
      CrawlPhase.paused => ('Paused', CrawlStatTheme.warning),
      CrawlPhase.auditing => ('Auditing', CrawlStatTheme.accent),
      CrawlPhase.starting => ('Starting', CrawlStatTheme.accent),
      _ => ('Crawling', CrawlStatTheme.success),
    };

    return Material(
      color: theme.colorScheme.surfaceContainer,
      borderRadius: BorderRadius.circular(CrawlStatTheme.radius - 2),
      child: InkWell(
        onTap: () => context.go(Routes.crawl),
        borderRadius: BorderRadius.circular(CrawlStatTheme.radius - 2),
        child: Padding(
          padding: const EdgeInsets.all(14),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Container(
                    width: 7,
                    height: 7,
                    decoration: BoxDecoration(color: color, shape: BoxShape.circle),
                  ),
                  const SizedBox(width: 7),
                  Expanded(
                    child: Text(
                      label,
                      maxLines: 1,
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: color,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  if (fraction != null)
                    Text(
                      '${(fraction * 100).round()}%',
                      style: CrawlStatTheme.mono(context, size: 11.5),
                    ),
                ],
              ),
              const SizedBox(height: 8),
              Text(
                host == null || host.isEmpty ? (crawl.startUrl ?? 'New crawl') : host,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.titleSmall,
              ),
              const SizedBox(height: 10),
              ClipRRect(
                borderRadius: BorderRadius.circular(4),
                child: LinearProgressIndicator(
                  value: fraction ?? (crawl.phase == CrawlPhase.paused ? 0 : null),
                  minHeight: 5,
                  color: color,
                ),
              ),
              if (snapshot != null) ...[
                const SizedBox(height: 10),
                Text(
                  '${formatCount(snapshot.crawled)} URLs · '
                  '${snapshot.urlsPerSecond.toStringAsFixed(1)}/s',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodySmall,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// The free plan's one nudge, kept to the sidebar's quiet corner.
class _UpgradeCard extends StatelessWidget {
  const _UpgradeCard();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(CrawlStatTheme.radius - 2),
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            CrawlStatTheme.accent.withValues(alpha: 0.28),
            CrawlStatTheme.accent.withValues(alpha: 0.08),
          ],
        ),
        border: Border.all(color: CrawlStatTheme.accent.withValues(alpha: 0.25)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(Icons.auto_awesome_rounded, size: 18, color: CrawlStatTheme.accent),
          const SizedBox(height: 8),
          Text('CrawlStat Pro', style: theme.textTheme.titleSmall),
          const SizedBox(height: 3),
          Text(
            'Unlimited URLs, crawl comparison and custom extraction.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 10),
          SizedBox(
            width: double.infinity,
            child: FilledButton(
              onPressed: () => context.go(Routes.settings),
              style: FilledButton.styleFrom(
                minimumSize: const Size(0, 32),
                padding: const EdgeInsets.symmetric(horizontal: 10),
              ),
              child: const Text('Upgrade'),
            ),
          ),
        ],
      ),
    );
  }
}

/// The heading at the top of a screen: a large title on the canvas, with the
/// screen's actions on the right.
class ScreenHeader extends StatelessWidget {
  const ScreenHeader({
    super.key,
    required this.title,
    this.subtitle,
    this.leading,
    this.actions = const [],
  });

  final String title;
  final String? subtitle;

  /// Sits after the title, on its baseline: a status pill, a count.
  final Widget? leading;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        CrawlStatTheme.gutter,
        22,
        CrawlStatTheme.gutter,
        18,
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    Flexible(
                      child: Text(
                        title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.titleLarge,
                      ),
                    ),
                    if (leading != null) ...[const SizedBox(width: 12), leading!],
                  ],
                ),
                if (subtitle != null) ...[
                  const SizedBox(height: 4),
                  Text(
                    subtitle!,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall?.copyWith(fontSize: 12.5),
                  ),
                ],
              ],
            ),
          ),
          for (final (index, action) in actions.indexed) ...[
            if (index > 0) const SizedBox(width: 8),
            action,
          ],
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
