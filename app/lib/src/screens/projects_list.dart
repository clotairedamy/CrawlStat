part of 'projects_screen.dart';

// ─── the model ───────────────────────────────────────────────────────────

/// Every crawl of one site, newest first.
///
/// The list view is organised by site rather than by crawl because that is
/// the question people bring to it: not "what did I run on Tuesday" but "is
/// this site getting better". Seventeen cards for seven sites hide that
/// answer; seven rows with their history beside them show it.
class SiteHistory {
  const SiteHistory(this.key, this.crawls);

  final String key;
  final List<ProjectInfo> crawls;

  ProjectInfo get latest => crawls.first;
  ProjectInfo? get previous => crawls.length > 1 ? crawls[1] : null;
}

/// The site a crawl belongs to. `www.` is dropped, so `www.example.com` and
/// `example.com` count as one site — they nearly always are.
String siteKey(ProjectInfo project) {
  final host =
      (project.rootHost ?? Uri.tryParse(project.startUrl ?? '')?.host ?? '')
          .toLowerCase();
  if (host.isEmpty) return project.path;
  return host.startsWith('www.') ? host.substring(4) : host;
}

/// When a crawl started. The recents list is ordered by when a crawl was last
/// *opened*, so it cannot be trusted for "which came first".
DateTime? crawlDate(ProjectInfo project) =>
    DateTime.tryParse(project.createdAt ?? project.finishedAt ?? '');

/// The newest crawl of the same site that started before [project], if any.
///
/// What a recrawl is compared against, and the one the Compare menu marks.
ProjectInfo? previousCrawl(ProjectInfo project, List<ProjectInfo> all) {
  final mine = crawlDate(project);
  if (mine == null) return null;
  final site = siteKey(project);
  return _newestFirst([
    for (final crawl in all)
      if (crawl.path != project.path && siteKey(crawl) == site) crawl,
  ]).where((crawl) => crawlDate(crawl)?.isBefore(mine) ?? false).firstOrNull;
}

/// Crawls grouped by site, each group newest first, groups in the order their
/// newest crawl appears.
List<SiteHistory> groupBySite(List<ProjectInfo> projects) {
  final groups = <String, List<ProjectInfo>>{};
  for (final project in projects) {
    groups.putIfAbsent(siteKey(project), () => []).add(project);
  }
  return [
    for (final MapEntry(:key, :value) in groups.entries)
      SiteHistory(key, _newestFirst(value)),
  ];
}

List<ProjectInfo> _newestFirst(List<ProjectInfo> crawls) {
  final indexed = crawls.indexed.toList()
    ..sort((a, b) {
      final (ai, ap) = a;
      final (bi, bp) = b;
      final ad = crawlDate(ap);
      final bd = crawlDate(bp);
      if (ad != null && bd != null && ad != bd) return bd.compareTo(ad);
      if (ad == null && bd != null) return 1;
      if (bd == null && ad != null) return -1;
      // Undated, or started the same second: keep the order they came in.
      return ai.compareTo(bi);
    });
  return [for (final (_, project) in indexed) project];
}

/// The share of answered URLs that did not fail: 2xx and 3xx over everything
/// that got a response or an error. `null` when nothing has been fetched yet.
///
/// A share, not a count, so a crawl stopped at 300 URLs and one that ran to
/// 3,000 can be put side by side — which is the whole point of the history.
double? crawlHealth(ProjectStats stats) {
  final fine = stats.status2Xx + stats.status3Xx;
  final answered = fine + stats.status4Xx + stats.status5Xx + stats.failed;
  if (answered == 0) return null;
  return fine / answered;
}

/// Health as a percentage, rounded *down*: a site with one broken link in
/// three thousand is 99.9%, never a reassuring 100%.
String formatHealth(double health) {
  final tenths = (health * 1000).floor() / 10;
  return tenths == tenths.roundToDouble()
      ? '${tenths.toInt()}%'
      : '${tenths.toStringAsFixed(1)}%';
}

/// "3 h ago" for this week, a date after that. Recency is what the column is
/// for; a timestamp makes the reader do the subtraction.
String relativeTime(DateTime then, {DateTime? now}) {
  final current = now ?? DateTime.now();
  final elapsed = current.difference(then);
  if (elapsed.inMinutes < 1) return 'Just now';
  if (elapsed.inMinutes < 60) return '${elapsed.inMinutes} min ago';
  if (elapsed.inHours < 24) return '${elapsed.inHours} h ago';
  if (elapsed.inDays < 2) return 'Yesterday';
  if (elapsed.inDays < 7) return '${elapsed.inDays} days ago';
  const months = [
    'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', //
    'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
  ];
  final local = then.toLocal();
  final date = '${local.day} ${months[local.month - 1]}';
  return local.year == current.year ? date : '$date ${local.year}';
}

enum SiteSort { recent, site, health, crawled, issues }

/// Sites in the order a column asks for. Each column's natural direction is
/// the useful one — newest, A to Z, *least* healthy, biggest, most issues —
/// and [reversed] flips it.
List<SiteHistory> sortSites(
  List<SiteHistory> sites,
  SiteSort by, {
  bool reversed = false,
}) {
  int compare(SiteHistory a, SiteHistory b) {
    switch (by) {
      case SiteSort.recent:
        final ad = crawlDate(a.latest);
        final bd = crawlDate(b.latest);
        if (ad == null || bd == null) return 0;
        return bd.compareTo(ad);
      case SiteSort.site:
        return a.key.compareTo(b.key);
      case SiteSort.health:
        final ah = crawlHealth(a.latest.stats) ?? 2;
        final bh = crawlHealth(b.latest.stats) ?? 2;
        return ah.compareTo(bh);
      case SiteSort.crawled:
        return b.latest.stats.crawled.compareTo(a.latest.stats.crawled);
      case SiteSort.issues:
        return (b.latest.errors + b.latest.warnings).compareTo(
          a.latest.errors + a.latest.warnings,
        );
    }
  }

  // Indexed so ties keep their incoming order: List.sort is not stable.
  final indexed = sites.indexed.toList()
    ..sort((a, b) {
      final order = compare(a.$2, b.$2);
      return order != 0 ? (reversed ? -order : order) : a.$1.compareTo(b.$1);
    });
  return [for (final (_, site) in indexed) site];
}

Color _healthTone(CrawlStatPalette palette, double? health) {
  if (health == null) return palette.notice;
  if (health >= 0.98) return palette.success;
  if (health >= 0.9) return palette.warning;
  return palette.error;
}

// ─── the list ────────────────────────────────────────────────────────────

/// One row per site: how healthy it is now, how that has moved, and every
/// earlier crawl a click away.
class ProjectList extends StatefulWidget {
  const ProjectList({
    super.key,
    required this.projects,
    required this.openPath,
  });

  final List<ProjectInfo> projects;
  final String? openPath;

  @override
  State<ProjectList> createState() => _ProjectListState();
}

class _ProjectListState extends State<ProjectList> {
  SiteSort _sort = SiteSort.recent;
  bool _reversed = false;
  final _expanded = <String>{};

  void _sortBy(SiteSort sort) => setState(() {
    if (_sort == sort) {
      _reversed = !_reversed;
    } else {
      _sort = sort;
      _reversed = false;
    }
  });

  @override
  Widget build(BuildContext context) {
    final sites = sortSites(
      groupBySite(widget.projects),
      _sort,
      reversed: _reversed,
    );

    // A sliver, so rows are built — and arrive — as they scroll into view.
    return SliverLayoutBuilder(
      builder: (context, constraints) {
        final columns = _Columns(constraints.crossAxisExtent);
        return SliverMainAxisGroup(
          slivers: [
            SliverToBoxAdapter(
              child: _ListHeader(
                columns: columns,
                sort: _sort,
                reversed: _reversed,
                onSort: _sortBy,
              ),
            ),
            const SliverToBoxAdapter(child: SizedBox(height: 8)),
            SliverList.builder(
              itemCount: sites.length,
              itemBuilder: (context, index) {
                final site = sites[index];
                return Padding(
                  key: ValueKey(site.key),
                  padding: const EdgeInsets.only(bottom: 12),
                  child: _Arrive(
                    index: index,
                    child: _SiteRow(
                      site: site,
                      columns: columns,
                      openPath: widget.openPath,
                      expanded: _expanded.contains(site.key),
                      onToggle: () => setState(() {
                        if (!_expanded.remove(site.key)) {
                          _expanded.add(site.key);
                        }
                      }),
                    ),
                  ),
                );
              },
            ),
          ],
        );
      },
    );
  }
}

/// Which columns fit, and one place that lays a row out, so the header and
/// every row line up cell for cell.
class _Columns {
  const _Columns(this.width);

  final double width;

  /// Columns drop out right to left in order of how much they add: the mix
  /// and history are the richest but the widest, and the site, its health
  /// and the actions always stay.
  bool get showMix => width >= 1160;
  bool get showHistory => width >= 1020;
  bool get showWhen => width >= 840;
  bool get showCounts => width >= 680;

  static const double health = 118;
  static const double history = 140;
  static const double crawled = 92;
  static const double issues = 136;
  static const double when = 104;
  static const double actions = 164;

  /// The padding inside a row's panel; the header uses it too.
  static const EdgeInsets inset = EdgeInsets.fromLTRB(16, 12, 8, 12);

  Widget layout({
    required Widget site,
    required Widget health,
    required Widget mix,
    required Widget history,
    required Widget crawled,
    required Widget issues,
    required Widget when,
    required Widget actions,
  }) {
    return Row(
      children: [
        Expanded(flex: 5, child: site),
        const SizedBox(width: 12),
        SizedBox(width: _Columns.health, child: health),
        if (showMix)
          Expanded(
            flex: 3,
            child: Padding(
              padding: const EdgeInsets.only(right: 24),
              child: mix,
            ),
          ),
        if (showHistory) SizedBox(width: _Columns.history, child: history),
        if (showCounts) ...[
          SizedBox(width: _Columns.crawled, child: crawled),
          SizedBox(width: _Columns.issues, child: issues),
        ],
        if (showWhen) SizedBox(width: _Columns.when, child: when),
        SizedBox(width: _Columns.actions, child: actions),
      ],
    );
  }
}

class _ListHeader extends StatelessWidget {
  const _ListHeader({
    required this.columns,
    required this.sort,
    required this.reversed,
    required this.onSort,
  });

  final _Columns columns;
  final SiteSort sort;
  final bool reversed;
  final ValueChanged<SiteSort> onSort;

  @override
  Widget build(BuildContext context) {
    Widget sortable(String label, SiteSort by) => _SortLabel(
      label: label,
      active: sort == by,
      reversed: reversed,
      onTap: () => onSort(by),
    );
    final style = Theme.of(context).textTheme.labelMedium;
    Widget plain(String label) => Text(label.toUpperCase(), style: style);

    return Padding(
      // One extra pixel for the panel's rim, so labels sit over the content.
      padding:
          _Columns.inset.copyWith(top: 0, bottom: 0) +
          const EdgeInsets.symmetric(horizontal: 1),
      child: columns.layout(
        site: sortable('Site', SiteSort.site),
        health: sortable('Health', SiteSort.health),
        mix: plain('Responses'),
        history: plain('History'),
        crawled: sortable('Crawled', SiteSort.crawled),
        issues: sortable('Issues', SiteSort.issues),
        when: sortable('Last crawl', SiteSort.recent),
        actions: const SizedBox.shrink(),
      ),
    );
  }
}

class _SortLabel extends StatefulWidget {
  const _SortLabel({
    required this.label,
    required this.active,
    required this.reversed,
    required this.onTap,
  });

  final String label;
  final bool active;
  final bool reversed;
  final VoidCallback onTap;

  @override
  State<_SortLabel> createState() => _SortLabelState();
}

class _SortLabelState extends State<_SortLabel> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colour = widget.active || _hovered
        ? theme.colorScheme.onSurface
        : theme.colorScheme.onSurfaceVariant;
    return Align(
      alignment: Alignment.centerLeft,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: GestureDetector(
          onTap: widget.onTap,
          behavior: HitTestBehavior.opaque,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: Text(
                    widget.label.toUpperCase(),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.labelMedium?.copyWith(color: colour),
                  ),
                ),
                const SizedBox(width: 4),
                AnimatedOpacity(
                  duration: const Duration(milliseconds: 140),
                  opacity: widget.active ? 1 : (_hovered ? 0.4 : 0),
                  child: AnimatedRotation(
                    duration: const Duration(milliseconds: 180),
                    turns: widget.active && widget.reversed ? 0.5 : 0,
                    child: Icon(
                      AppIcon.sortDescending,
                      size: 12,
                      color: colour,
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

/// Fades and lifts a row or card in as it is first built.
///
/// The first screenful is staggered so the opening paint settles in
/// sequence. Past that, an item is built because the reader scrolled to it,
/// and a delay would read as lag.
class _Arrive extends StatelessWidget {
  const _Arrive({super.key, required this.index, required this.child});

  final int index;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final delay = index < 9 ? 45 * index : 0;
    final total = 340 + delay;
    return TweenAnimationBuilder<double>(
      tween: Tween(begin: 0, end: 1),
      duration: Duration(milliseconds: total),
      curve: Interval(delay / total, 1, curve: Curves.easeOutCubic),
      builder: (context, t, child) => Opacity(
        opacity: t,
        child: Transform.translate(
          offset: Offset(0, (1 - t) * 14),
          child: child,
        ),
      ),
      child: child,
    );
  }
}

// ─── a row ───────────────────────────────────────────────────────────────

class _SiteRow extends ConsumerWidget {
  const _SiteRow({
    required this.site,
    required this.columns,
    required this.openPath,
    required this.expanded,
    required this.onToggle,
  });

  final SiteHistory site;
  final _Columns columns;
  final String? openPath;
  final bool expanded;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final latest = site.latest;
    final previous = site.previous;
    final stats = latest.stats;
    final health = crawlHealth(stats);
    final earlier = site.crawls.skip(1).toList();
    final isOpen = site.crawls.any((crawl) => crawl.path == openPath);
    final host = latest.rootHost ?? site.key;

    final main = columns.layout(
      site: Row(
        children: [
          _HostAvatar(host: host, size: 38),
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
                          fontSize: 14.5,
                        ),
                      ),
                    ),
                    if (isOpen) ...[
                      const SizedBox(width: 8),
                      TonePill('Open', color: palette.accent),
                    ],
                  ],
                ),
                const SizedBox(height: 4),
                Row(
                  children: [
                    _StateBadge(state: latest.state),
                    const SizedBox(width: 8),
                    Flexible(
                      child: Text(
                        site.crawls.length == 1
                            ? 'One crawl'
                            : '${site.crawls.length} crawls',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.bodySmall,
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
      health: _HealthCell(
        health: health,
        previous: previous == null ? null : crawlHealth(previous.stats),
      ),
      mix: _ResponseMix(stats: stats),
      history: _History(
        crawls: site.crawls,
        onOpen: (path) => _openProject(context, ref, path),
      ),
      crawled: _CrawledCell(stats: stats),
      issues: _IssuesCell(project: latest),
      when: _WhenCell(project: latest),
      actions: Row(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [
          if (_isResumable(latest))
            SoftCircleButton(
              icon: AppIcon.play,
              size: 30,
              iconSize: 13,
              tooltip: 'Resume this crawl',
              onPressed: () => _resumeProject(context, ref, latest.path),
            )
          else
            _RecrawlButton(project: latest, size: 30),
          const SizedBox(width: 6),
          _CompareButton(project: latest, size: 30),
          if (earlier.isNotEmpty) ...[
            const SizedBox(width: 4),
            IconButton(
              tooltip: expanded ? 'Hide earlier crawls' : 'Show earlier crawls',
              visualDensity: VisualDensity.compact,
              onPressed: onToggle,
              icon: AnimatedRotation(
                duration: const Duration(milliseconds: 200),
                curve: Curves.easeOutCubic,
                turns: expanded ? 0.5 : 0,
                child: const Icon(AppIcon.chevronDown, size: 17),
              ),
            ),
          ],
          _ProjectMenu(project: latest),
        ],
      ),
    );

    return SoftPanel(
      radius: 18,
      depth: 0.8,
      pressed: isOpen,
      glow: isOpen ? palette.accent : null,
      onTap: () => _openProject(context, ref, latest.path),
      padding: _Columns.inset,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          main,
          AnimatedSize(
            duration: const Duration(milliseconds: 240),
            curve: Curves.easeOutCubic,
            alignment: Alignment.topCenter,
            child: !expanded
                ? const SizedBox(width: double.infinity)
                : Padding(
                    padding: const EdgeInsets.only(top: 10),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        for (final (index, crawl) in earlier.indexed)
                          _EarlierCrawl(
                            project: crawl,
                            // Against the crawl before it, so each step of
                            // the history says which way it went.
                            before: index + 1 < earlier.length
                                ? earlier[index + 1]
                                : null,
                            columns: columns,
                            isOpen: crawl.path == openPath,
                            isLast: index == earlier.length - 1,
                          ),
                      ],
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

/// An earlier crawl of the same site, hung off a timeline under its row.
class _EarlierCrawl extends ConsumerStatefulWidget {
  const _EarlierCrawl({
    required this.project,
    required this.before,
    required this.columns,
    required this.isOpen,
    required this.isLast,
  });

  final ProjectInfo project;
  final ProjectInfo? before;
  final _Columns columns;
  final bool isOpen;
  final bool isLast;

  @override
  ConsumerState<_EarlierCrawl> createState() => _EarlierCrawlState();
}

class _EarlierCrawlState extends ConsumerState<_EarlierCrawl> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final project = widget.project;
    final health = crawlHealth(project.stats);
    final rail = theme.colorScheme.onSurfaceVariant.withValues(alpha: 0.25);

    final row = widget.columns.layout(
      site: Row(
        children: [
          // The timeline: a rail from the avatar above, a node per crawl,
          // tinted by how healthy that crawl was.
          SizedBox(
            width: 38,
            height: 44,
            child: CustomPaint(
              painter: _TimelinePainter(
                rail: rail,
                node: _healthTone(palette, health),
                last: widget.isLast,
              ),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Row(
              children: [
                Flexible(
                  child: Text(
                    _shortDate(project.createdAt ?? project.finishedAt),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodyMedium?.copyWith(
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                _StateBadge(state: project.state),
                if (widget.isOpen) ...[
                  const SizedBox(width: 6),
                  TonePill('Open', color: palette.accent),
                ],
              ],
            ),
          ),
        ],
      ),
      health: _HealthCell(
        health: health,
        previous: widget.before == null
            ? null
            : crawlHealth(widget.before!.stats),
        ring: 22,
      ),
      mix: _ResponseMix(stats: project.stats, caption: false),
      history: const SizedBox.shrink(),
      crawled: _CrawledCell(stats: project.stats),
      issues: _IssuesCell(project: project),
      when: _WhenCell(project: project),
      actions: Row(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [_ProjectMenu(project: project)],
      ),
    );

    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () => _openProject(context, ref, project.path),
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 140),
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius),
            color: _hovered
                ? theme.colorScheme.onSurface.withValues(alpha: 0.04)
                : Colors.transparent,
          ),
          child: row,
        ),
      ),
    );
  }

  String _shortDate(String? iso) {
    final parsed = iso == null ? null : DateTime.tryParse(iso);
    if (parsed == null) return 'Undated';
    final local = parsed.toLocal();
    final time =
        '${local.hour.toString().padLeft(2, '0')}:'
        '${local.minute.toString().padLeft(2, '0')}';
    return '${relativeTime(parsed)}, $time';
  }
}

class _TimelinePainter extends CustomPainter {
  const _TimelinePainter({
    required this.rail,
    required this.node,
    required this.last,
  });

  final Color rail;
  final Color node;
  final bool last;

  @override
  void paint(Canvas canvas, Size size) {
    final x = size.width / 2;
    final y = size.height / 2;
    final line = Paint()
      ..color = rail
      ..strokeWidth = 1.5
      ..strokeCap = StrokeCap.round;
    // Up through the gap above, so the first node joins the avatar.
    canvas.drawLine(Offset(x, -12), Offset(x, last ? y : size.height), line);
    canvas.drawCircle(
      Offset(x, y),
      6,
      Paint()..color = node.withValues(alpha: 0.22),
    );
    canvas.drawCircle(Offset(x, y), 3.5, Paint()..color = node);
  }

  @override
  bool shouldRepaint(_TimelinePainter old) =>
      old.rail != rail || old.node != node || old.last != last;
}

// ─── cells ───────────────────────────────────────────────────────────────

/// A ring that fills to the crawl's health, with the change since the crawl
/// before it underneath the figure.
class _HealthCell extends StatelessWidget {
  const _HealthCell({required this.health, this.previous, this.ring = 30});

  final double? health;
  final double? previous;
  final double ring;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final health = this.health;
    final tone = _healthTone(palette, health);

    return Row(
      children: [
        TweenAnimationBuilder<double>(
          tween: Tween(begin: 0, end: health ?? 0),
          duration: const Duration(milliseconds: 900),
          curve: Curves.easeOutCubic,
          builder: (context, value, _) => CustomPaint(
            size: Size.square(ring),
            painter: _RingPainter(
              value: value,
              track: theme.colorScheme.onSurfaceVariant.withValues(alpha: 0.16),
              colour: tone,
              stroke: ring > 24 ? 4 : 3,
            ),
          ),
        ),
        const SizedBox(width: 9),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                health == null ? '—' : formatHealth(health),
                maxLines: 1,
                style: theme.textTheme.titleSmall?.copyWith(
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
              if (health != null && previous != null)
                _HealthDelta(change: (health - previous!) * 100),
            ],
          ),
        ),
      ],
    );
  }
}

class _HealthDelta extends StatelessWidget {
  const _HealthDelta({required this.change});

  /// In percentage points.
  final double change;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    if (change.abs() < 0.05) {
      return Text('Steady', style: theme.textTheme.bodySmall);
    }
    final better = change > 0;
    final colour = better ? palette.success : palette.error;
    final points = change.abs().toStringAsFixed(1);
    return Tooltip(
      message: '${better ? 'Up' : 'Down'} $points points on the crawl before',
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            better ? AppIcon.trendUp : AppIcon.trendDown,
            size: 12,
            color: colour,
          ),
          const SizedBox(width: 3),
          Text(
            '${better ? '+' : '−'}$points',
            style: theme.textTheme.bodySmall?.copyWith(
              color: colour,
              fontWeight: FontWeight.w600,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
        ],
      ),
    );
  }
}

class _RingPainter extends CustomPainter {
  const _RingPainter({
    required this.value,
    required this.track,
    required this.colour,
    required this.stroke,
  });

  final double value;
  final Color track;
  final Color colour;
  final double stroke;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = (Offset.zero & size).deflate(stroke / 2);
    canvas.drawArc(
      rect,
      0,
      2 * math.pi,
      false,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = stroke
        ..color = track,
    );
    if (value <= 0) return;
    canvas.drawArc(
      rect,
      -math.pi / 2,
      2 * math.pi * value.clamp(0, 1),
      false,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = stroke
        ..strokeCap = StrokeCap.round
        ..shader = SweepGradient(
          startAngle: -math.pi / 2,
          endAngle: 3 * math.pi / 2,
          transform: const GradientRotation(-math.pi / 2),
          colors: [Color.lerp(colour, Colors.black, 0.2)!, colour],
        ).createShader(rect),
    );
  }

  @override
  bool shouldRepaint(_RingPainter old) =>
      old.value != value || old.colour != colour || old.track != track;
}

/// The status bar from the cards, with a plain-words caption in place of the
/// legend: "27 redirects · 4 broken" reads faster than five coloured dots.
class _ResponseMix extends StatelessWidget {
  const _ResponseMix({required this.stats, this.caption = true});

  final ProjectStats stats;
  final bool caption;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final broken = stats.status4Xx + stats.status5Xx;
    final parts = [
      if (stats.status3Xx > 0)
        '${formatCount(stats.status3Xx)} redirect'
            '${stats.status3Xx == 1 ? '' : 's'}',
      if (broken > 0) '${formatCount(broken)} broken',
      if (stats.failed > 0) '${formatCount(stats.failed)} failed',
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        _StatusBar(stats: stats, height: 6),
        if (caption) ...[
          const SizedBox(height: 6),
          Text(
            stats.crawled == 0
                ? 'Nothing fetched yet'
                : parts.isEmpty
                ? 'Every page answered'
                : parts.join('  ·  '),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall,
          ),
        ],
      ],
    );
  }
}

/// Every crawl of the site as a bar: height for how many URLs it reached,
/// tone for how healthy they were. Newest on the right, and brightest.
class _History extends StatelessWidget {
  const _History({required this.crawls, required this.onOpen});

  /// Newest first.
  final List<ProjectInfo> crawls;
  final ValueChanged<String> onOpen;

  static const int _most = 8;
  static const double _height = 30;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    final shown = crawls.take(_most).toList().reversed.toList();
    final peak = shown.fold<int>(
      1,
      (most, crawl) => math.max(most, crawl.stats.crawled),
    );

    return Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        SizedBox(
          height: _height,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              for (final (index, crawl) in shown.indexed)
                _HistoryBar(
                  crawl: crawl,
                  fraction: crawl.stats.crawled / peak,
                  tone: _healthTone(palette, crawlHealth(crawl.stats)),
                  newest: index == shown.length - 1,
                  delay: index,
                  onTap: () => onOpen(crawl.path),
                ),
            ],
          ),
        ),
        if (crawls.length > _most)
          Padding(
            padding: const EdgeInsets.only(left: 2, bottom: 1),
            child: Text(
              '+${crawls.length - _most}',
              style: theme.textTheme.bodySmall?.copyWith(fontSize: 10.5),
            ),
          ),
      ],
    );
  }
}

class _HistoryBar extends StatefulWidget {
  const _HistoryBar({
    required this.crawl,
    required this.fraction,
    required this.tone,
    required this.newest,
    required this.delay,
    required this.onTap,
  });

  final ProjectInfo crawl;
  final double fraction;
  final Color tone;
  final bool newest;
  final int delay;
  final VoidCallback onTap;

  @override
  State<_HistoryBar> createState() => _HistoryBarState();
}

class _HistoryBarState extends State<_HistoryBar> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final crawl = widget.crawl;
    final health = crawlHealth(crawl.stats);
    final date = crawlDate(crawl);
    final lit = widget.newest || _hovered;
    // Short crawls still get a visible stub, so "it ran" is never invisible.
    final target = 5 + (_History._height - 5) * widget.fraction.clamp(0, 1);
    final total = 500 + 60 * widget.delay;

    return Tooltip(
      message: [
        if (date != null) relativeTime(date),
        '${formatCount(crawl.stats.crawled)} URLs',
        if (health != null) '${formatHealth(health)} healthy',
      ].join('  ·  '),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: GestureDetector(
          onTap: widget.onTap,
          behavior: HitTestBehavior.opaque,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 2),
            // Grows up from the baseline on first paint, left to right, so
            // the history reads as something that happened in order.
            child: TweenAnimationBuilder<double>(
              tween: Tween(begin: 0, end: target),
              duration: Duration(milliseconds: total),
              curve: Interval(
                60.0 * widget.delay / total,
                1,
                curve: Curves.easeOutCubic,
              ),
              builder: (context, height, _) => AnimatedContainer(
                duration: const Duration(milliseconds: 140),
                width: 10,
                height: height,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(3),
                  gradient: glossOf(
                    widget.tone.withValues(alpha: lit ? 1 : 0.45),
                  ),
                  boxShadow: widget.newest
                      ? [
                          BoxShadow(
                            color: widget.tone.withValues(alpha: 0.35),
                            blurRadius: 8,
                          ),
                        ]
                      : null,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _CrawledCell extends StatelessWidget {
  const _CrawledCell({required this.stats});

  final ProjectStats stats;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          formatCount(stats.crawled),
          maxLines: 1,
          style: theme.textTheme.titleSmall?.copyWith(
            fontFeatures: const [FontFeature.tabularFigures()],
          ),
        ),
        if (stats.queued > 0)
          Text(
            '+${formatCount(stats.queued)} queued',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall,
          ),
      ],
    );
  }
}

class _IssuesCell extends StatelessWidget {
  const _IssuesCell({required this.project});

  final ProjectInfo project;

  @override
  Widget build(BuildContext context) {
    final palette = CrawlStatTheme.palette(context);
    return Row(
      children: [
        Flexible(
          child: _Count(
            icon: AppIcon.error,
            count: project.errors,
            colour: palette.error,
            tooltip: 'Errors',
          ),
        ),
        const SizedBox(width: 12),
        Flexible(
          child: _Count(
            icon: AppIcon.warning,
            count: project.warnings,
            colour: palette.warning,
            tooltip: 'Warnings',
          ),
        ),
      ],
    );
  }
}

class _Count extends StatelessWidget {
  const _Count({
    required this.icon,
    required this.count,
    required this.colour,
    required this.tooltip,
  });

  final IconData icon;
  final int count;
  final Color colour;
  final String tooltip;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    // A zero is not news, so it steps back rather than wearing the colour.
    final tone = count > 0 ? colour : theme.colorScheme.onSurfaceVariant;
    return Tooltip(
      message: tooltip,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 14, color: tone.withValues(alpha: 0.85)),
          const SizedBox(width: 5),
          Flexible(
            child: Text(
              formatCount(count),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.titleSmall?.copyWith(
                color: count > 0 ? tone : theme.colorScheme.onSurfaceVariant,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _WhenCell extends StatelessWidget {
  const _WhenCell({required this.project});

  final ProjectInfo project;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final date = crawlDate(project);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          date == null ? '—' : relativeTime(date),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: theme.textTheme.bodyMedium,
        ),
        Text(
          formatBytes(project.sizeBytes),
          maxLines: 1,
          style: theme.textTheme.bodySmall,
        ),
      ],
    );
  }
}
