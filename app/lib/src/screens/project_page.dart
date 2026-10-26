part of 'projects_screen.dart';

/// The tabs of a crawl's page, in the order you would read them: how it
/// went, what is wrong, the pages themselves, then the deeper tools.
const _projectTabs = [
  (Glyph.crawl, 'Overview', Routes.project),
  (Glyph.issues, 'Issues', Routes.issues),
  (Glyph.results, 'Pages', Routes.results),
  (Glyph.plan, 'Plan', Routes.plan),
  (Glyph.visualise, 'Visualise', Routes.visualise),
  (Glyph.readerFlow, 'UX Testing', Routes.ux),
  (Glyph.accessibility, 'Accessibility', Routes.accessibility),
];

/// One crawl's page: which crawl it is, what you can do with it, and its
/// tabs, around whichever tab is showing.
///
/// Everything about a crawl used to be a destination in the sidebar, each
/// one quietly showing "the open crawl". Putting them under the crawl's own
/// header says which crawl you are looking at, and leaves the sidebar for
/// the things that are not about one crawl.
class ProjectShell extends ConsumerWidget {
  const ProjectShell({super.key, required this.location, required this.child});

  final String location;
  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final path = ref.watch(openProjectProvider);
    if (path == null) {
      return Column(
        children: [
          ScreenHeader(title: 'No crawl open', actions: [_BackToProjects()]),
          const Expanded(child: NoProjectState()),
        ],
      );
    }

    // The recents list is already loaded and refreshes when a crawl ends;
    // the file is only asked directly for a crawl that is not on it.
    final recents = ref.watch(recentProjectsProvider);
    final project =
        recents.where((p) => p.path == path).firstOrNull ??
        ref.watch(projectInfoProvider(path)).value;

    // The accessibility check starts the moment a crawl opens, in the
    // background, so its verdict is waiting on the Overview and the
    // Accessibility tab opens on a finished report. Not while this crawl is
    // still running: it would only be checking half the site.
    final crawl = ref.watch(crawlControllerProvider);
    final crawling = crawl.dbPath == path && crawl.isBusy;
    final standard = ref.watch(accessibilityStandardProvider);
    if (!crawling) {
      ref.listen(accessibilityReportProvider((path, standard)), (_, _) {});
    }
    ref.listen(crawlControllerProvider.select((c) => c.isFinished), (
      was,
      finished,
    ) {
      if (finished && !(was ?? false) && crawl.dbPath == path) {
        ref.invalidate(accessibilityReportProvider);
      }
    });

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _ProjectHeader(path: path, project: project),
        _ProjectTabs(location: location),
        Expanded(child: ProjectScope(child: child)),
      ],
    );
  }
}

class _BackToProjects extends StatelessWidget {
  @override
  Widget build(BuildContext context) => SoftCircleButton(
    icon: AppIcon.back,
    size: 38,
    tooltip: 'All projects',
    onPressed: () => context.go(Routes.projects),
  );
}

class _ProjectHeader extends ConsumerWidget {
  const _ProjectHeader({required this.path, required this.project});

  final String path;

  /// `null` for the moment it takes to read a crawl that is not in recents.
  final ProjectInfo? project;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final crawl = ref.watch(crawlControllerProvider);
    final project = this.project;
    final live = crawl.dbPath == path && (crawl.isBusy);

    final host =
        project?.rootHost ??
        Uri.tryParse(crawl.dbPath == path ? crawl.startUrl ?? '' : '')?.host ??
        path.split(Platform.pathSeparator).last;
    final date = project == null ? null : crawlDate(project);
    final health = project == null ? null : crawlHealth(project.stats);

    // While it crawls, the file's figures are from when it started; the
    // live counters are the ones worth reading.
    final crawled = live ? crawl.snapshot?.crawled : project?.stats.crawled;
    final details = [
      if (date != null) relativeTime(date),
      if (crawled != null) '${formatCount(crawled)} URLs',
      if (!live && health != null) '${formatHealth(health)} healthy',
      if (!live && project != null) formatBytes(project.sizeBytes),
    ];

    return Padding(
      padding: const EdgeInsets.fromLTRB(
        CrawlStatTheme.gutter,
        22,
        CrawlStatTheme.gutter,
        16,
      ),
      child: LayoutBuilder(
        builder: (context, constraints) => Row(
          children: [
            _BackToProjects(),
            const SizedBox(width: 14),
            _HostAvatar(host: host.isEmpty ? path : host, size: 44),
            const SizedBox(width: 14),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          host.isEmpty ? path : host,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.titleLarge,
                        ),
                      ),
                      const SizedBox(width: 10),
                      if (live)
                        TonePill(
                          crawl.phase == CrawlPhase.paused ? 'Paused' : 'Live',
                          color: crawl.phase == CrawlPhase.paused
                              ? palette.warning
                              : palette.success,
                          dot: true,
                        )
                      else if (project != null)
                        _StateBadge(state: project.state),
                    ],
                  ),
                  const SizedBox(height: 3),
                  Text(
                    [?project?.startUrl, ...details].join('  ·  '),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall?.copyWith(fontSize: 12.5),
                  ),
                ],
              ),
            ),
            if (project != null) ...[
              const SizedBox(width: 16),
              ConstrainedBox(
                constraints: BoxConstraints(
                  maxWidth: constraints.maxWidth * 0.45,
                ),
                child: Wrap(
                  alignment: WrapAlignment.end,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    if (!live) _ReportButton(project: project, size: 38),
                    _CompareButton(project: project, size: 38),
                    if (_isResumable(project) && !crawl.isBusy)
                      SoftButton(
                        icon: AppIcon.play,
                        gradient: Soft.accentGradient(context),
                        padding: const EdgeInsets.symmetric(
                          horizontal: 14,
                          vertical: 10,
                        ),
                        onPressed: () =>
                            _resumeProject(context, ref, project.path),
                        child: const Text('Resume'),
                      )
                    else if (!live)
                      _RecrawlButton(project: project, size: 38),
                  ],
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// The crawl's tabs, as a pressed-in track with the current one raised out
/// of it.
class _ProjectTabs extends ConsumerWidget {
  const _ProjectTabs({required this.location});

  final String location;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final isPro = ref.watch(isProProvider);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: CrawlStatTheme.gutter),
      child: Align(
        alignment: Alignment.centerLeft,
        child: SoftWell(
          radius: 18,
          depth: 0.6,
          padding: const EdgeInsets.all(4),
          child: SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final (glyph, label, path) in _projectTabs)
                  _ProjectTab(
                    glyph: glyph,
                    label: label,
                    selected: location == path,
                    tooltip: path == Routes.ux && !isPro
                        ? 'UX Testing — a Pro feature'
                        : null,
                    onTap: () => context.go(path),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ProjectTab extends StatefulWidget {
  const _ProjectTab({
    required this.glyph,
    required this.label,
    required this.selected,
    required this.onTap,
    this.tooltip,
  });

  final Glyph glyph;
  final String label;
  final bool selected;
  final VoidCallback onTap;
  final String? tooltip;

  @override
  State<_ProjectTab> createState() => _ProjectTabState();
}

class _ProjectTabState extends State<_ProjectTab> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final highlight = CrawlStatTheme.palette(context).highlight;
    final selected = widget.selected;
    // Lit the way the sidebar lights its current tab, so the two read as
    // one navigation.
    final stroke = selected
        ? Color.lerp(highlight, Colors.white, 0.06)!
        : _hovered
        ? theme.colorScheme.onSurface
        : theme.colorScheme.onSurfaceVariant;
    final fill = LinearGradient(
      begin: Alignment.topCenter,
      end: Alignment.bottomCenter,
      colors: selected
          ? [
              Color.lerp(highlight, Colors.white, 0.24)!,
              Color.lerp(highlight, Colors.black, 0.12)!,
            ]
          : [stroke.withValues(alpha: 0.38), stroke.withValues(alpha: 0.14)],
    );

    final content = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        GlyphIcon(widget.glyph, stroke: stroke, fill: fill, size: 18),
        const SizedBox(width: 8),
        Text(
          widget.label,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: selected
                ? theme.colorScheme.onSurface
                : _hovered
                ? theme.colorScheme.onSurface
                : theme.colorScheme.onSurfaceVariant,
            fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
          ),
        ),
      ],
    );
    const padding = EdgeInsets.symmetric(horizontal: 14, vertical: 9);

    Widget tab = selected
        ? SoftPanel(radius: 14, depth: 0.7, padding: padding, child: content)
        : MouseRegion(
            cursor: SystemMouseCursors.click,
            onEnter: (_) => setState(() => _hovered = true),
            onExit: (_) => setState(() => _hovered = false),
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: widget.onTap,
              child: Padding(padding: padding, child: content),
            ),
          );
    if (widget.tooltip != null) {
      tab = Tooltip(message: widget.tooltip, child: tab);
    }
    return Semantics(button: true, selected: selected, child: tab);
  }
}

/// The Overview tab: the crawl as it happens, then a summary once it is done.
class ProjectOverview extends ConsumerWidget {
  const ProjectOverview({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final path = ref.watch(openProjectProvider);
    if (path == null) return const NoProjectState();
    final crawl = ref.watch(crawlControllerProvider);
    final mine = crawl.dbPath == path;

    // A finished crawl has new figures and a new audit; anything read while
    // it ran is out of date.
    ref.listen(crawlControllerProvider.select((c) => c.isFinished), (
      was,
      finished,
    ) {
      if (finished && !(was ?? false) && mine) {
        ref.invalidate(auditReportProvider(path));
        ref.invalidate(projectInfoProvider(path));
      }
    });

    // A failed crawl stays on the live view, which is where its error is.
    if (mine && (crawl.isBusy || crawl.phase == CrawlPhase.failed)) {
      return const CrawlScreen();
    }
    return _ProjectSummary(path: path);
  }
}

class _ProjectSummary extends ConsumerWidget {
  const _ProjectSummary({required this.path});

  final String path;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final recents = ref.watch(recentProjectsProvider);
    final project =
        recents.where((p) => p.path == path).firstOrNull ??
        ref.watch(projectInfoProvider(path)).value;
    if (project == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final report = ref.watch(auditReportProvider(path)).value;
    final plan = ref.watch(actionPlanProvider(path)).value;
    final previous = previousCrawl(project, recents);
    final accessibility = ref.watch(
      accessibilityReportProvider((
        path,
        ref.watch(accessibilityStandardProvider),
      )),
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ScreenHeader(
          title: 'Overview',
          subtitle: _verdict(crawlHealth(project.stats)),
        ),
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(
              CrawlStatTheme.gutter,
              0,
              CrawlStatTheme.gutter,
              CrawlStatTheme.gutter,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (_isResumable(project) && project.stats.queued > 0)
                  InlineMessage(
                    message:
                        'This crawl stopped with '
                        '${formatCount(project.stats.queued)} URLs still '
                        'queued. Resume it to finish.',
                    severity: 'warning',
                    margin: const EdgeInsets.only(bottom: 16),
                  ),
                _SummaryTiles(
                  project: project,
                  previous: previous,
                  accessibility: accessibility,
                ),
                const SizedBox(height: 16),
                _TopIssues(report: report, plan: plan),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

String _verdict(double? health) => switch (health) {
  null => 'Nothing fetched yet',
  >= 0.98 => 'Healthy',
  >= 0.9 => 'Some problems',
  _ => 'Needs work',
};

class _SummaryTiles extends StatelessWidget {
  const _SummaryTiles({
    required this.project,
    required this.previous,
    required this.accessibility,
  });

  final ProjectInfo project;
  final ProjectInfo? previous;
  final AsyncValue<A11yReport> accessibility;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final stats = project.stats;
    final health = crawlHealth(stats);
    final before = previous == null ? null : crawlHealth(previous!.stats);
    final broken = stats.status4Xx + stats.status5Xx;

    // Spread top to bottom, so beside the tiles it is exactly as tall as
    // they are, with the accessibility strip on the same line as their base.
    final healthCard = SoftPanel(
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Row(
            children: [
              _HealthRing(health: health, size: 92),
              const SizedBox(width: 20),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Row(
                      children: [
                        Flexible(
                          child: Text(
                            _verdict(health),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: theme.textTheme.titleMedium,
                          ),
                        ),
                        if (health != null && before != null) ...[
                          const SizedBox(width: 8),
                          _HealthDelta(change: (health - before) * 100),
                        ],
                      ],
                    ),
                    const SizedBox(height: 12),
                    _ResponseMix(stats: stats),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: 18),
          _AccessibilityStrip(report: accessibility),
        ],
      ),
    );

    final tiles = [
      StatTile(
        label: 'Crawled',
        icon: AppIcon.ok,
        color: palette.success,
        value: formatCount(stats.crawled),
        hint: stats.queued > 0
            ? '${formatCount(stats.queued)} still queued'
            : 'Fetched and stored',
        onTap: () => context.go(Routes.results),
      ),
      StatTile(
        label: 'Broken',
        icon: AppIcon.brokenLink,
        color: broken > 0 ? palette.error : palette.notice,
        value: formatCount(broken),
        hint: '4xx and 5xx responses',
        onTap: () => context.go('${Routes.results}?tab=codes'),
      ),
      StatTile(
        label: 'Errors',
        icon: AppIcon.error,
        color: project.errors > 0 ? palette.error : palette.notice,
        value: formatCount(project.errors),
        hint: 'Fix these first',
        onTap: () => context.go(Routes.issues),
      ),
      StatTile(
        label: 'Warnings',
        icon: AppIcon.warning,
        color: project.warnings > 0 ? palette.warning : palette.notice,
        value: formatCount(project.warnings),
        hint: 'Worth a look',
        onTap: () => context.go(Routes.issues),
      ),
    ];

    const spacing = 14.0;
    const gap = SizedBox.square(dimension: spacing);
    return LayoutBuilder(
      builder: (context, constraints) {
        // Side by side when there is room for both; stacked when not.
        if (constraints.maxWidth < 900) {
          final perRow = constraints.maxWidth >= 420 ? 2 : 1;
          final width =
              (constraints.maxWidth - spacing * (perRow - 1)) / perRow;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              healthCard,
              gap,
              Wrap(
                spacing: spacing,
                runSpacing: spacing,
                children: [
                  for (final tile in tiles) SizedBox(width: width, child: tile),
                ],
              ),
            ],
          );
        }
        // One block with straight edges: every tile in a row as tall as its
        // neighbour, and the health card as tall as both rows together.
        Widget pair(Widget a, Widget b) => Expanded(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(child: a),
              gap,
              Expanded(child: b),
            ],
          ),
        );
        return IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(flex: 5, child: healthCard),
              gap,
              Expanded(
                flex: 6,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    pair(tiles[0], tiles[1]),
                    gap,
                    pair(tiles[2], tiles[3]),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// The accessibility check, which runs on its own as soon as a crawl opens:
/// "checking" while it works, then its verdict, one tap from the full report.
class _AccessibilityStrip extends StatelessWidget {
  const _AccessibilityStrip({required this.report});

  final AsyncValue<A11yReport> report;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final report = this.report.value;
    final (tone, verdict, detail) = switch (report) {
      null when this.report.hasError => (
        palette.notice,
        'Not checked',
        'The check could not run on this crawl',
      ),
      null => (palette.accent, 'Checking…', 'Running in the background'),
      _ => (
        switch (report.verdict) {
          'does_not_conform' => palette.error,
          'incomplete' => palette.warning,
          'conforms' => palette.success,
          _ => palette.notice,
        },
        switch (report.verdict) {
          'does_not_conform' => 'Not compliant',
          'incomplete' => 'Needs review',
          'conforms' => 'Conforms',
          _ => 'Not checked',
        },
        _accessibilityDetail(report),
      ),
    };
    final checking = report == null && !this.report.hasError;

    return SoftWell(
      padding: const EdgeInsets.fromLTRB(12, 10, 10, 10),
      child: InkWell(
        borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius),
        onTap: () => context.go(Routes.accessibility),
        child: Row(
          children: [
            SizedBox.square(
              dimension: 28,
              child: checking
                  ? Padding(
                      padding: const EdgeInsets.all(6),
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: tone,
                      ),
                    )
                  : IconBadge(AppIcon.accessibility, color: tone, size: 28),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    report == null
                        ? 'Accessibility'
                        : 'Accessibility · ${report.standard.label}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall?.copyWith(
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    detail,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
            const SizedBox(width: 10),
            TonePill(verdict, color: tone, dot: checking),
            const SizedBox(width: 4),
            Icon(
              AppIcon.chevronRight,
              size: 15,
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ],
        ),
      ),
    );
  }
}

String _accessibilityDetail(A11yReport report) {
  final fail = report.counts.fail.toInt();
  final review = report.counts.review.toInt();
  final pages = report.coverage.pagesChecked.toInt();
  final parts = [
    if (fail > 0) '$fail criteri${fail == 1 ? 'on' : 'a'} failing',
    if (review > 0) '$review to review',
    '${formatCount(pages)} page${pages == 1 ? '' : 's'} checked',
  ];
  return parts.join(' · ');
}

/// The worst few issues, so the Overview answers "what should I look at".
class _TopIssues extends StatelessWidget {
  const _TopIssues({required this.report, this.plan});

  /// `null` while the audit loads.
  final AuditReport? report;

  /// Ranks the issues within each severity, once it loads.
  final ActionPlan? plan;

  static const _shown = 6;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final report = this.report;
    final issues = report == null
        ? const <RankedIssue>[]
        : rankIssues(
            report.issues,
            plan: plan,
            totalPages: report.indexablePages + report.nonIndexablePages,
          );

    return SectionCard(
      title: 'Top issues',
      subtitle: report == null
          ? 'Reading the audit…'
          : issues.isEmpty
          ? 'Nothing found'
          : 'The worst first: by severity, then by what fixing it is worth',
      trailing: issues.isEmpty
          ? null
          : TextButton(
              onPressed: () => context.go(Routes.issues),
              child: Text('All ${formatCount(issues.length)} issues'),
            ),
      child: report == null
          ? const Padding(
              padding: EdgeInsets.all(20),
              child: Center(child: CircularProgressIndicator()),
            )
          : issues.isEmpty
          ? Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Text(
                'The audit found nothing to fix.',
                style: theme.textTheme.bodySmall,
              ),
            )
          : LayoutBuilder(
              builder: (context, constraints) {
                // Narrow, the badge and the bar step aside so the title keeps
                // its room; the columns that stay still line up.
                final compact = constraints.maxWidth < 600;
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    _TopIssueHeader(compact: compact),
                    for (final (index, issue) in issues.take(_shown).indexed)
                      _TopIssueRow(
                        ranked: issue,
                        rank: index + 1,
                        compact: compact,
                      ),
                  ],
                );
              },
            ),
    );
  }
}

class _TopIssueRow extends StatelessWidget {
  const _TopIssueRow({
    required this.ranked,
    required this.rank,
    this.compact = false,
  });

  final RankedIssue ranked;

  /// Without the badge and bar columns, for narrow windows.
  final bool compact;

  /// 1 for the issue to fix first.
  final int rank;

  // Every column after the title has a fixed width, so badges, counts, bars
  // and percentages sit on the same vertical lines down the whole list.
  static const double badgeWidth = 96;
  static const double countWidth = 76;
  static const double barWidth = 88;
  static const double shareWidth = 40;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final issue = ranked.issue;
    final count = ranked.pages;
    final share = ranked.share;
    final tone = switch (issue.severity) {
      'error' => palette.error,
      'warning' => palette.warning,
      _ => palette.notice,
    };
    final figures = theme.textTheme.bodySmall?.copyWith(
      fontFeatures: const [FontFeature.tabularFigures()],
    );
    return InkWell(
      borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius),
      onTap: () => context.go(Routes.issue(issue.id)),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 10),
        child: Row(
          children: [
            SizedBox(
              width: 22,
              child: Text(
                '$rank',
                textAlign: TextAlign.right,
                style: figures?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant.withValues(
                    alpha: 0.6,
                  ),
                ),
              ),
            ),
            const SizedBox(width: 12),
            SeverityDot(issue.severity),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                issue.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.bodyMedium,
              ),
            ),
            const SizedBox(width: 12),
            if (!compact)
              SizedBox(
                width: badgeWidth,
                child: Align(
                  alignment: Alignment.centerRight,
                  child: ranked.isTemplateFix
                      ? TonePill('Template fix', color: palette.accent)
                      : null,
                ),
              ),
            SizedBox(
              width: countWidth,
              child: Text(
                '${formatCount(count)} URL${count == 1 ? '' : 's'}',
                textAlign: TextAlign.right,
                maxLines: 1,
                style: figures,
              ),
            ),
            const SizedBox(width: 16),
            if (!compact)
              SizedBox(
                width: barWidth,
                child: share == null
                    ? null
                    : ShareBar(share: share, color: tone, width: barWidth),
              ),
            SizedBox(
              width: shareWidth + 8,
              child: Text(
                share == null ? '' : formatShare(share),
                textAlign: TextAlign.right,
                maxLines: 1,
                style: figures,
              ),
            ),
            const SizedBox(width: 8),
            Icon(
              AppIcon.chevronRight,
              size: 15,
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ],
        ),
      ),
    );
  }
}

/// Labels over the Top issues columns, on the same lines as the rows.
class _TopIssueHeader extends StatelessWidget {
  const _TopIssueHeader({required this.compact});

  final bool compact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final style = theme.textTheme.labelSmall?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
      letterSpacing: 0.6,
    );
    return Padding(
      padding: const EdgeInsets.fromLTRB(8, 0, 8, 6),
      child: Row(
        children: [
          SizedBox(
            width: 22,
            child: Text('#', textAlign: TextAlign.right, style: style),
          ),
          const SizedBox(width: 12 + 8 + 12),
          Expanded(child: Text('ISSUE', style: style)),
          SizedBox(width: compact ? 12 : 12 + _TopIssueRow.badgeWidth),
          SizedBox(
            width: _TopIssueRow.countWidth,
            child: Text('PAGES', textAlign: TextAlign.right, style: style),
          ),
          const SizedBox(width: 16),
          SizedBox(
            width: compact
                ? _TopIssueRow.shareWidth + 8
                : _TopIssueRow.barWidth + _TopIssueRow.shareWidth + 8,
            child: Text(
              compact ? 'SHARE' : 'SHARE OF SITE',
              textAlign: TextAlign.right,
              style: style,
            ),
          ),
          const SizedBox(width: 8 + 15),
        ],
      ),
    );
  }
}
