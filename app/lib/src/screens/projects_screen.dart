import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../theme/glyphs.dart';
import '../theme/icons.dart';
import '../router.dart';
import '../rust/api/accessibility.dart' show A11yReport;
import '../rust/api/project.dart'
    show ActionPlan, exportReport, storedCrawlRequest;
import '../rust/api/types.dart';
import '../state/accessibility.dart';
import '../state/crawl.dart';
import '../state/issues.dart';
import '../state/license.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../state/settings.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/export.dart' show exportFile, openFile;
import 'crawl_screen.dart';
import 'issues_screen.dart' show ShareBar;
import 'new_crawl_dialog.dart';
import 'shell.dart';
import '../theme/soft.dart';

part 'project_page.dart';
part 'projects_list.dart';
part 'report_dialog.dart';

/// The crawls the user has run, as cards or as one row per site.
class ProjectsScreen extends ConsumerStatefulWidget {
  const ProjectsScreen({super.key});

  @override
  ConsumerState<ProjectsScreen> createState() => _ProjectsScreenState();
}

class _ProjectsScreenState extends ConsumerState<ProjectsScreen> {
  final _filter = TextEditingController();

  @override
  void initState() {
    super.initState();
    _filter.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _filter.dispose();
    super.dispose();
  }

  bool _matches(ProjectInfo project, String query) {
    if (query.isEmpty) return true;
    return [
      project.rootHost,
      project.startUrl,
      project.path,
    ].any((field) => field?.toLowerCase().contains(query) ?? false);
  }

  @override
  Widget build(BuildContext context) {
    final projects = ref.watch(recentProjectsProvider);
    final openPath = ref.watch(openProjectProvider);
    final asList = ref.watch(
      settingsProvider.select((s) => s.projectsView == 'list'),
    );
    final query = _filter.text.trim().toLowerCase();
    final shown = [
      for (final project in projects)
        if (_matches(project, query)) project,
    ];
    final theme = Theme.of(context);

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
              icon: AppIcon.refresh,
              tooltip: 'Refresh',
              onPressed: () =>
                  ref.read(recentProjectsProvider.notifier).refresh(),
            ),
            FilledButton.icon(
              onPressed: () => showNewCrawlDialog(context),
              icon: const Icon(AppIcon.add, size: 17),
              label: const Text('New crawl'),
            ),
          ],
        ),
        Expanded(
          child: projects.isEmpty
              ? EmptyState(
                  icon: AppIcon.discovered,
                  title: 'No crawls yet',
                  message:
                      'Point CrawlStat at a site and it will fetch every page, store the '
                      'results locally, and tell you what is wrong with them.',
                  action: FilledButton.icon(
                    onPressed: () => showNewCrawlDialog(context),
                    icon: const Icon(AppIcon.add, size: 17),
                    label: const Text('Start your first crawl'),
                  ),
                )
              // Slivers rather than one long column, so cards and rows are
              // only built as they scroll into view — and animate in as they
              // arrive, which is what makes the page feel endless rather
              // than a document with a scrollbar.
              : CustomScrollView(
                  slivers: [
                    SliverPadding(
                      padding: const EdgeInsets.fromLTRB(
                        CrawlStatTheme.gutter,
                        4,
                        CrawlStatTheme.gutter,
                        CrawlStatTheme.gutter,
                      ),
                      sliver: SliverMainAxisGroup(
                        slivers: [
                          SliverToBoxAdapter(
                            child: _Overview(projects: projects),
                          ),
                          const SliverToBoxAdapter(child: SizedBox(height: 32)),
                          SliverToBoxAdapter(
                            child: _SectionBar(
                              asList: asList,
                              sites: groupBySite(shown).length,
                              crawls: shown.length,
                              filter: _filter,
                            ),
                          ),
                          const SliverToBoxAdapter(child: SizedBox(height: 18)),
                          if (shown.isEmpty)
                            SliverToBoxAdapter(
                              child: Padding(
                                padding: const EdgeInsets.symmetric(
                                  vertical: 48,
                                ),
                                child: Text(
                                  'No crawl matches “${_filter.text.trim()}”.',
                                  textAlign: TextAlign.center,
                                  style: theme.textTheme.bodyMedium?.copyWith(
                                    color: theme.colorScheme.onSurfaceVariant,
                                  ),
                                ),
                              ),
                            )
                          else ...[
                            if (asList)
                              ProjectList(projects: shown, openPath: openPath)
                            else
                              ProjectGrid(projects: shown, openPath: openPath),
                            SliverToBoxAdapter(
                              child: _EndOfList(
                                label: asList
                                    ? '${formatCount(groupBySite(shown).length)} '
                                          'sites'
                                    : '${formatCount(shown.length)} crawls',
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                  ],
                ),
        ),
      ],
    );
  }
}

/// A quiet full stop under the last card: with no scrollbar to show the end,
/// the page says so itself.
class _EndOfList extends StatelessWidget {
  const _EndOfList({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final rule = Expanded(
      child: Container(
        height: 1,
        color: theme.colorScheme.onSurfaceVariant.withValues(alpha: 0.14),
      ),
    );
    return Padding(
      padding: const EdgeInsets.fromLTRB(80, 36, 80, 8),
      child: Row(
        children: [
          rule,
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 14),
            child: Text(
              'All $label shown',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant.withValues(
                  alpha: 0.7,
                ),
              ),
            ),
          ),
          rule,
        ],
      ),
    );
  }
}

/// The title over the crawls, a filter, and the grid/list switch.
class _SectionBar extends ConsumerWidget {
  const _SectionBar({
    required this.asList,
    required this.sites,
    required this.crawls,
    required this.filter,
  });

  final bool asList;
  final int sites;
  final int crawls;
  final TextEditingController filter;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Text(
          asList ? 'Sites' : 'Recent crawls',
          style: theme.textTheme.titleMedium,
        ),
        const SizedBox(width: 10),
        Flexible(
          child: Text(
            asList
                ? '${formatCount(sites)} site${sites == 1 ? '' : 's'}  ·  '
                      '${formatCount(crawls)} crawl${crawls == 1 ? '' : 's'}'
                : '${formatCount(crawls)} crawl${crawls == 1 ? '' : 's'}',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall,
          ),
        ),
        const Spacer(),
        SizedBox(
          width: 220,
          height: 36,
          child: TextField(
            controller: filter,
            style: theme.textTheme.bodyMedium,
            decoration: InputDecoration(
              isDense: true,
              hintText: 'Filter by site…',
              prefixIcon: const Icon(AppIcon.search, size: 16),
              prefixIconConstraints: const BoxConstraints(minWidth: 36),
              suffixIcon: filter.text.isEmpty
                  ? null
                  : IconButton(
                      tooltip: 'Clear',
                      iconSize: 14,
                      onPressed: filter.clear,
                      icon: const Icon(AppIcon.close),
                    ),
            ),
          ),
        ),
        const SizedBox(width: 10),
        SegmentedButton<String>(
          showSelectedIcon: false,
          style: const ButtonStyle(visualDensity: VisualDensity.compact),
          segments: const [
            ButtonSegment(
              value: 'grid',
              tooltip: 'Cards',
              icon: Icon(AppIcon.gridView, size: 16),
            ),
            ButtonSegment(
              value: 'list',
              tooltip: 'One row per site',
              icon: Icon(AppIcon.listView, size: 16),
            ),
          ],
          selected: {asList ? 'list' : 'grid'},
          onSelectionChanged: (selection) => ref
              .read(settingsProvider.notifier)
              .update((s) => s.copyWith(projectsView: selection.first)),
        ),
      ],
    );
  }
}

/// Opens a crawl on its own page. Shared by the cards and the list.
void _openProject(BuildContext context, WidgetRef ref, String path) {
  ref.read(openProjectProvider.notifier).open(path);
  ref.read(urlTableProvider.notifier).reset();
  context.go(Routes.project);
}

Future<void> _resumeProject(
  BuildContext context,
  WidgetRef ref,
  String path,
) async {
  ref.read(openProjectProvider.notifier).open(path);
  await ref.read(crawlControllerProvider.notifier).resumeProject(path);
  if (context.mounted) context.go(Routes.project);
}

/// Whether a crawl stopped short and can pick up where it left off.
bool _isResumable(ProjectInfo project) =>
    project.state == 'cancelled' ||
    project.state == 'paused' ||
    project.state == 'running' ||
    project.stats.queued > 0;

/// Runs a crawl again with exactly the settings it was started with.
///
/// The recrawl goes to a new file rather than over the old one: the point of
/// running it again is nearly always to compare the two.
Future<void> _recrawlProject(
  BuildContext context,
  WidgetRef ref,
  ProjectInfo project,
) async {
  final controller = ref.read(crawlControllerProvider.notifier);
  if (ref.read(crawlControllerProvider).isBusy) return;
  final messenger = ScaffoldMessenger.of(context);
  try {
    final request = await storedCrawlRequest(dbPath: project.path);
    final path = newProjectPath(
      ref.read(settingsProvider).resolvedProjectsDirectory,
      request.startUrl,
    );
    await controller.start(request, path);
    ref.read(openProjectProvider.notifier).open(path);
    ref.read(urlTableProvider.notifier).reset();
    if (context.mounted) context.go(Routes.project);
  } on Object catch (error) {
    messenger.showSnackBar(
      SnackBar(content: Text('Could not recrawl: ${_reason(error)}')),
    );
  }
}

/// The New crawl dialog, filled in with the settings [project] was run with.
Future<void> _recrawlWithChanges(
  BuildContext context,
  ProjectInfo project,
) async {
  final messenger = ScaffoldMessenger.of(context);
  try {
    final request = await storedCrawlRequest(dbPath: project.path);
    if (context.mounted) await showNewCrawlDialog(context, from: request);
  } on Object catch (error) {
    messenger.showSnackBar(
      SnackBar(content: Text('Could not read its settings: ${_reason(error)}')),
    );
  }
}

String _reason(Object error) =>
    error.toString().replaceFirst('PanicException: ', '');

/// Recrawl with the same settings, or why not right now.
///
/// The engine runs one crawl at a time, so while one is going the button
/// stays where it is but cannot be pressed — and says so.
class _RecrawlButton extends ConsumerWidget {
  const _RecrawlButton({required this.project, this.size = 34});

  final ProjectInfo project;
  final double size;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final busy = ref.watch(crawlControllerProvider.select((c) => c.isBusy));
    return SoftCircleButton(
      icon: AppIcon.recrawl,
      size: size,
      iconSize: size * 0.42,
      tooltip: busy
          ? 'Another crawl is running — recrawl when it finishes'
          : 'Recrawl with the same settings',
      onPressed: busy ? null : () => _recrawlProject(context, ref, project),
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
    final broken = projects.fold<int>(
      0,
      (sum, p) => sum + p.stats.status4Xx + p.stats.status5Xx,
    );

    final tiles = [
      StatTile(
        label: 'Crawls',
        icon: AppIcon.folder,
        value: formatCount(projects.length),
        hint: 'On this machine',
      ),
      StatTile(
        label: 'URLs crawled',
        icon: AppIcon.web,
        color: CrawlStatTheme.palette(context).success,
        value: formatCount(crawled),
        hint: 'Across every crawl',
      ),
      StatTile(
        label: 'Broken URLs',
        icon: AppIcon.brokenLink,
        color: broken > 0
            ? CrawlStatTheme.palette(context).error
            : CrawlStatTheme.palette(context).notice,
        value: formatCount(broken),
        hint: '4xx and 5xx responses',
      ),
      StatTile(
        label: 'Issues',
        icon: AppIcon.issues,
        color: errors > 0
            ? CrawlStatTheme.palette(context).warning
            : CrawlStatTheme.palette(context).notice,
        value: formatCount(errors + warnings),
        hint:
            '${formatCount(errors)} errors, ${formatCount(warnings)} warnings',
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

/// Project cards, at most three to a row, all the same height.
///
/// A sliver, so a card is only built as it scrolls into view — and since
/// [_Arrive] animates on first build, each one rises in as it arrives.
class ProjectGrid extends StatelessWidget {
  const ProjectGrid({
    super.key,
    required this.projects,
    required this.openPath,
  });

  final List<ProjectInfo> projects;
  final String? openPath;

  static const double _minCardWidth = 320;
  static const double _gap = 24;

  /// Fixed rather than measured: every card has the same sections, with the
  /// footer always present (holding Resume or Open), so one height fits all
  /// and a row never goes ragged because one crawl was interrupted.
  static const double cardHeight = 304;

  @override
  Widget build(BuildContext context) {
    // The crawl before each one, from the same site, for the trend arrow.
    final previous = <String, ProjectInfo>{};
    for (final site in groupBySite(projects)) {
      for (var i = 0; i + 1 < site.crawls.length; i++) {
        previous[site.crawls[i].path] = site.crawls[i + 1];
      }
    }

    return SliverLayoutBuilder(
      builder: (context, constraints) {
        final columns =
            ((constraints.crossAxisExtent + _gap) / (_minCardWidth + _gap))
                .floor()
                .clamp(1, 3);
        return SliverGrid.builder(
          gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
            crossAxisCount: columns,
            mainAxisSpacing: _gap,
            crossAxisSpacing: _gap,
            mainAxisExtent: cardHeight,
          ),
          itemCount: projects.length,
          itemBuilder: (context, index) {
            final project = projects[index];
            return _Arrive(
              key: ValueKey(project.path),
              index: index,
              child: _ProjectCard(
                project: project,
                previous: previous[project.path],
                isOpen: project.path == openPath,
              ),
            );
          },
        );
      },
    );
  }
}

class _ProjectCard extends ConsumerWidget {
  const _ProjectCard({
    required this.project,
    required this.previous,
    required this.isOpen,
  });

  final ProjectInfo project;

  /// The crawl of the same site before this one, if there was one.
  final ProjectInfo? previous;
  final bool isOpen;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final stats = project.stats;
    final health = crawlHealth(stats);
    final before = previous == null ? null : crawlHealth(previous!.stats);
    final resumable = _isResumable(project);
    final host =
        project.rootHost ?? project.path.split(Platform.pathSeparator).last;
    final date = crawlDate(project);

    final verdict = switch (health) {
      null => 'Not started',
      >= 0.98 => 'Healthy',
      >= 0.9 => 'Some problems',
      _ => 'Needs work',
    };

    return SoftPanel(
      // The crawl you have open is pressed into the page rather than outlined:
      // soft UI has no borders to thicken.
      pressed: isOpen,
      glow: isOpen ? palette.accent : null,
      radius: 24,
      // The tap belongs to the panel, not to an InkWell inside it: an InkWell
      // paints its hover as a square, which showed as a grey box poking out
      // of the card's rounded corners.
      onTap: () => _openProject(context, ref, project.path),
      padding: const EdgeInsets.fromLTRB(20, 18, 12, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              _HostAvatar(host: host, size: 40),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Flexible(
                          child: Text(
                            host,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: theme.textTheme.titleSmall?.copyWith(
                              fontSize: 15,
                            ),
                          ),
                        ),
                        if (isOpen) ...[
                          const SizedBox(width: 8),
                          TonePill('Open', color: palette.accent),
                        ],
                      ],
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
          const SizedBox(height: 18),
          // The hero: one number for how the crawl went, big enough to
          // compare across a row of cards without reading any of them.
          Padding(
            padding: const EdgeInsets.only(right: 8),
            child: Row(
              children: [
                _HealthRing(health: health, size: 76),
                const SizedBox(width: 18),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Row(
                        children: [
                          Flexible(
                            child: Text(
                              verdict,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: theme.textTheme.titleSmall,
                            ),
                          ),
                          if (health != null && before != null) ...[
                            const SizedBox(width: 8),
                            _HealthDelta(change: (health - before) * 100),
                          ],
                        ],
                      ),
                      const SizedBox(height: 10),
                      _ResponseMix(stats: stats),
                    ],
                  ),
                ),
              ],
            ),
          ),
          const Spacer(),
          Padding(
            padding: const EdgeInsets.only(right: 8),
            child: SoftWell(
              radius: 16,
              depth: 0.5,
              padding: const EdgeInsets.symmetric(vertical: 10),
              child: Row(
                children: [
                  _Figure(label: 'Crawled', value: formatCount(stats.crawled)),
                  const _FigureRule(),
                  _Figure(
                    label: 'Errors',
                    value: formatCount(project.errors),
                    color: project.errors > 0 ? palette.error : null,
                  ),
                  const _FigureRule(),
                  _Figure(
                    label: 'Warnings',
                    value: formatCount(project.warnings),
                    color: project.warnings > 0 ? palette.warning : null,
                  ),
                  const _FigureRule(),
                  _Figure(label: 'Size', value: formatBytes(project.sizeBytes)),
                ],
              ),
            ),
          ),
          const SizedBox(height: 14),
          // Always there, whatever the crawl's state, so every card is the
          // same shape and the actions sit in the same place on each.
          SizedBox(
            height: 38,
            child: Padding(
              padding: const EdgeInsets.only(right: 8),
              child: Row(
                children: [
                  _StateBadge(state: project.state),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      resumable && stats.queued > 0
                          ? '${formatCount(stats.queued)} queued'
                          : date == null
                          ? ''
                          : relativeTime(date),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall,
                    ),
                  ),
                  _CompareButton(project: project, size: 34),
                  const SizedBox(width: 8),
                  if (resumable)
                    SoftButton(
                      icon: AppIcon.play,
                      gradient: Soft.accentGradient(context),
                      padding: const EdgeInsets.symmetric(
                        horizontal: 14,
                        vertical: 9,
                      ),
                      onPressed: () =>
                          _resumeProject(context, ref, project.path),
                      child: const Text('Resume'),
                    )
                  else
                    // The card itself opens the results, so the button that
                    // used to repeat that does something new instead.
                    _RecrawlButton(project: project, size: 34),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Health as a ring that fills to the figure, with the figure inside it.
class _HealthRing extends StatelessWidget {
  const _HealthRing({required this.health, required this.size});

  final double? health;
  final double size;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final health = this.health;
    return SizedBox.square(
      dimension: size,
      child: TweenAnimationBuilder<double>(
        tween: Tween(begin: 0, end: health ?? 0),
        duration: const Duration(milliseconds: 1000),
        curve: Curves.easeOutCubic,
        builder: (context, value, _) => CustomPaint(
          painter: _RingPainter(
            value: value,
            track: theme.colorScheme.onSurfaceVariant.withValues(alpha: 0.14),
            colour: _healthTone(CrawlStatTheme.palette(context), health),
            stroke: 7,
          ),
          child: Center(
            child: Text(
              health == null ? '—' : formatHealth(health),
              style: theme.textTheme.titleMedium?.copyWith(
                fontSize: size * 0.21,
                fontWeight: FontWeight.w700,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// One figure in a card's recessed strip.
class _Figure extends StatelessWidget {
  const _Figure({required this.label, required this.value, this.color});

  final String label;
  final String value;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Expanded(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.titleSmall?.copyWith(
              color: color,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
          const SizedBox(height: 2),
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall?.copyWith(fontSize: 11),
          ),
        ],
      ),
    );
  }
}

class _FigureRule extends StatelessWidget {
  const _FigureRule();

  @override
  Widget build(BuildContext context) => Container(
    width: 1,
    height: 26,
    color: Theme.of(context).colorScheme.onSurfaceVariant
        .withValues(alpha: 0.15),
  );
}

/// Compares this crawl with another, picked from a menu under the button.
///
/// Crawls of the same site come first — that is nearly always the question,
/// "did my fix work" — with the one just before this crawl marked. Whichever
/// is older goes on the left.
class _CompareButton extends ConsumerWidget {
  const _CompareButton({required this.project, this.size = 34});

  final ProjectInfo project;
  final double size;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final all = ref.watch(recentProjectsProvider);
    final others = [
      for (final crawl in all)
        if (crawl.path != project.path) crawl,
    ];
    final site = siteKey(project);
    final sameSite = groupBySite(others)
        .where((group) => group.key == site)
        .expand((group) => group.crawls);
    final otherSites = others.where((crawl) => siteKey(crawl) != site);

    final previous = previousCrawl(project, all);

    return Builder(
      builder: (context) => SoftCircleButton(
        icon: AppIcon.compare,
        size: size,
        iconSize: size * 0.42,
        tooltip: others.isEmpty
            ? 'Run another crawl to compare against'
            : 'Compare with another crawl',
        onPressed: others.isEmpty
            ? null
            : () => _pick(context, [
                for (final crawl in sameSite)
                  _option(
                    context,
                    crawl,
                    note: crawl == previous ? 'Previous crawl' : null,
                  ),
                if (sameSite.isNotEmpty && otherSites.isNotEmpty)
                  const PopupMenuDivider(),
                for (final crawl in otherSites.take(12))
                  _option(context, crawl),
              ]),
      ),
    );
  }

  PopupMenuItem<ProjectInfo> _option(
    BuildContext context,
    ProjectInfo crawl, {
    String? note,
  }) {
    final theme = Theme.of(context);
    final date = crawlDate(crawl);
    final health = crawlHealth(crawl.stats);
    final details = [
      ?note,
      if (date != null) relativeTime(date),
      if (health != null) '${formatHealth(health)} healthy',
    ].join('  ·  ');
    return PopupMenuItem(
      value: crawl,
      height: 48,
      child: Row(
        children: [
          Container(
            width: 8,
            height: 8,
            margin: const EdgeInsets.only(right: 12),
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              gradient: glossOf(
                _healthTone(CrawlStatTheme.palette(context), health),
              ),
            ),
          ),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  crawl.rootHost ?? siteKey(crawl),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodyMedium,
                ),
                if (details.isNotEmpty)
                  Text(
                    details,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall,
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _pick(
    BuildContext context,
    List<PopupMenuEntry<ProjectInfo>> items,
  ) async {
    final button = context.findRenderObject()! as RenderBox;
    final overlay =
        Overlay.of(context).context.findRenderObject()! as RenderBox;
    final topLeft = button.localToGlobal(
      Offset(0, button.size.height + 6),
      ancestor: overlay,
    );
    final chosen = await showMenu<ProjectInfo>(
      context: context,
      position: RelativeRect.fromRect(
        topLeft & button.size,
        Offset.zero & overlay.size,
      ),
      constraints: const BoxConstraints(minWidth: 300, maxWidth: 380),
      items: items,
    );
    if (chosen == null || !context.mounted) return;

    final mine = crawlDate(project);
    final theirs = crawlDate(chosen);
    final chosenFirst = mine != null && theirs != null && theirs.isBefore(mine);
    context.go(
      Routes.comparing(
        before: chosenFirst ? chosen.path : project.path,
        after: chosenFirst ? project.path : chosen.path,
      ),
    );
  }
}

/// A coloured initial standing in for the site's favicon, which would need a
/// network request just to decorate a list.
class _HostAvatar extends StatelessWidget {
  const _HostAvatar({required this.host, this.size = 42});

  final String host;
  final double size;

  static const _palette = [
    Color(0xFF6366F1),
    Color(0xFF22B573),
    Color(0xFFF2A12E),
    Color(0xFFEC4899),
    Color(0xFF0EA5E9),
    Color(0xFF8B5CF6),
  ];

  /// The same six, as greys of different lightness, for the monochrome
  /// theme. Hosts still tell apart at a glance, by tone instead of hue.
  static const _greys = [
    Color(0xFF9A9A9A),
    Color(0xFFBDBDBD),
    Color(0xFF7A7A7A),
    Color(0xFFD6D6D6),
    Color(0xFF8C8C8C),
    Color(0xFFADADAD),
  ];

  @override
  Widget build(BuildContext context) {
    final swatches = CrawlStatTheme.palette(context).monochrome
        ? _greys
        : _palette;
    // `www.gtlaw.com` is G, not W: half the web would otherwise share a letter.
    final name = host.toLowerCase().startsWith('www.')
        ? host.substring(4)
        : host;
    final color =
        swatches[name.codeUnits.fold<int>(0, (a, b) => a + b) %
            swatches.length];
    // A soft disc rather than a tinted square: the hero elements on each card
    // carry the style, and a flat rounded rectangle here undoes the panel it
    // is sitting on.
    return Container(
      width: size,
      height: size,
      padding: const EdgeInsets.all(1),
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: Soft.rim(context),
        boxShadow: Soft.raised(context, depth: 0.45),
      ),
      foregroundDecoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [
            color.withValues(alpha: 0.26),
            color.withValues(alpha: 0.10),
          ],
        ),
      ),
      alignment: Alignment.center,
      child: Text(
        name.isEmpty ? '?' : name.characters.first.toUpperCase(),
        style: TextStyle(
          // Lifted towards white on a dark surface and pushed towards black
          // on a light one: a pale letter on the light theme's pale disc
          // (and the pale greys of monochrome) all but vanished.
          color: Theme.of(context).brightness == Brightness.dark
              ? Color.lerp(color, Colors.white, 0.45)
              : Color.lerp(color, Colors.black, 0.45),
          fontWeight: FontWeight.w700,
          fontSize: size * 0.4,
        ),
      ),
    );
  }
}

/// Status classes in the order the bar lays them out.
List<(String, int, Color)> _statusSegments(
  BuildContext context,
  ProjectStats stats,
) {
  final palette = CrawlStatTheme.palette(context);
  return [
    ('2xx', stats.status2Xx, palette.success),
    ('3xx', stats.status3Xx, palette.warning),
    ('4xx', stats.status4Xx, palette.error),
    ('5xx', stats.status5Xx, palette.error),
    ('Failed', stats.failed, palette.notice),
  ];
}

/// The share of each status class as one segmented bar.
class _StatusBar extends StatelessWidget {
  const _StatusBar({required this.stats, this.height = 7});

  final ProjectStats stats;
  final double height;

  @override
  Widget build(BuildContext context) {
    final segments = _statusSegments(context, stats);
    final total = segments.fold<int>(0, (sum, segment) => sum + segment.$2);
    // A recessed track with the segments lying in it, rather than a flat
    // strip painted on the card. Each segment keeps its own colour but gets
    // the same top-lit gloss as everything else.
    return SoftWell(
      radius: 5,
      depth: 0.6,
      padding: const EdgeInsets.all(1.5),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(4),
        child: SizedBox(
          height: height,
          child: total == 0
              ? const SizedBox.shrink()
              : Row(
                  children: [
                    for (final (_, count, color) in segments)
                      if (count > 0)
                        Expanded(
                          flex: (count * 1000 / total).ceil(),
                          child: Container(
                            margin: const EdgeInsets.only(right: 2),
                            decoration: BoxDecoration(
                              borderRadius: BorderRadius.circular(4),
                              gradient: glossOf(color),
                            ),
                          ),
                        ),
                  ],
                ),
        ),
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
      'finished' => ('Finished', CrawlStatTheme.palette(context).success),
      'running' => ('Running', CrawlStatTheme.palette(context).accent),
      'paused' => ('Paused', CrawlStatTheme.palette(context).warning),
      'cancelled' => ('Stopped', CrawlStatTheme.palette(context).warning),
      'failed' => ('Failed', CrawlStatTheme.palette(context).error),
      _ => ('Unknown', CrawlStatTheme.palette(context).notice),
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
      icon: const Icon(AppIcon.more, size: 18),
      iconSize: 16,
      onSelected: (action) async {
        final notifier = ref.read(recentProjectsProvider.notifier);
        switch (action) {
          case 'recrawl':
            await _recrawlProject(context, ref, project);
          case 'recrawl-with-changes':
            await _recrawlWithChanges(context, project);
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
            if (confirmed) {
              await notifier.forget(project.path, deleteFile: true);
            }
        }
      },
      itemBuilder: (context) => [
        // Read when the menu opens rather than watched — a menu is only
        // open for a moment.
        PopupMenuItem(
          value: 'recrawl',
          height: 34,
          enabled: !ref.read(crawlControllerProvider).isBusy,
          child: const Text('Recrawl'),
        ),
        PopupMenuItem(
          value: 'recrawl-with-changes',
          height: 34,
          enabled: !ref.read(crawlControllerProvider).isBusy,
          child: const Text('Recrawl with changes…'),
        ),
        const PopupMenuDivider(),
        const PopupMenuItem(
          value: 'reveal',
          height: 34,
          child: Text('Copy file path'),
        ),
        const PopupMenuItem(
          value: 'forget',
          height: 34,
          child: Text('Remove from list'),
        ),
        const PopupMenuDivider(),
        const PopupMenuItem(
          value: 'delete',
          height: 34,
          child: Text('Delete crawl file…'),
        ),
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
            style: FilledButton.styleFrom(
              backgroundColor: CrawlStatTheme.palette(context).error,
            ),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    return result ?? false;
  }
}
