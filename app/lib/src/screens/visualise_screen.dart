import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../theme/icons.dart';
import '../router.dart';
import '../rust/api/architecture.dart' as arch;
import '../rust/api/project.dart' as rust;
import '../state/projects.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';
import '../widgets/charts.dart';
import '../widgets/common.dart';
import '../widgets/export.dart' show openLink;
import '../widgets/gauge.dart' show rampColourAt;
import '../widgets/site_graph.dart';
import 'shell.dart';

/// How many pages the diagram draws.
///
/// A node-link diagram stops meaning anything long before it stops rendering.
/// Four hundred is about the point where a viewer can still pick out shape and
/// clusters; beyond that it is a texture. The screen says plainly when it is
/// showing a subset rather than quietly drawing part of the site.
const int graphNodeLimit = 400;

/// The site's shape, as a picture.
final siteStructureProvider = FutureProvider.family<rust.SiteStructure, String>(
  (ref, path) {
    return rust.siteStructure(dbPath: path, nodeLimit: graphNodeLimit);
  },
);

/// The site's information architecture: findings, section tree, link map.
final siteArchitectureProvider =
    FutureProvider.family<arch.SiteArchitecture, String>((ref, path) {
      return arch.siteArchitecture(dbPath: path);
    });

enum _Mode { structure, graph }

/// Information architecture: how the site is organised, and what to change.
///
/// The screen leads with answers — what to fix, how the folders are sized and
/// fed, which sections link to which — and keeps the link graph one click
/// away for exploring. A diagram of a few hundred bubbles is a picture; the
/// structure view is the list of things to do about it.
class VisualiseScreen extends ConsumerStatefulWidget {
  const VisualiseScreen({super.key});

  @override
  ConsumerState<VisualiseScreen> createState() => _VisualiseScreenState();
}

class _VisualiseScreenState extends ConsumerState<VisualiseScreen> {
  _Mode _mode = _Mode.structure;
  rust.SiteNode? _selected;

  /// The folder lit up in the graph, from a click in the section tree.
  String? _highlight;

  void _showInGraph(String path) => setState(() {
    _highlight = path == '/' ? null : path;
    _selected = null;
    _mode = _Mode.graph;
  });

  @override
  Widget build(BuildContext context) {
    final path = ref.watch(openProjectProvider);
    if (path == null) return const NoProjectState();

    final architecture = ref.watch(siteArchitectureProvider(path));
    final structure = ref.watch(siteStructureProvider(path));
    final overview = architecture.value?.overview;

    final picker = SegmentedButton<_Mode>(
      segments: const [
        ButtonSegment(
          value: _Mode.structure,
          icon: Icon(AppIcon.structure, size: 15),
          label: Text('Structure'),
        ),
        ButtonSegment(
          value: _Mode.graph,
          icon: Icon(AppIcon.visualise, size: 15),
          label: Text('Link graph'),
        ),
      ],
      selected: {_mode},
      showSelectedIcon: false,
      style: const ButtonStyle(visualDensity: VisualDensity.compact),
      onSelectionChanged: (selection) =>
          setState(() => _mode = selection.first),
    );

    final String subtitle;
    if (_mode == _Mode.graph && structure.value != null) {
      final data = structure.value!;
      subtitle = data.totalPages > data.nodes.length
          ? '${_count(data.nodes.length)} of ${_count(data.totalPages)} pages '
                '· the best-connected ones, shallowest first'
          : '${_count(data.nodes.length)} pages · '
                '${_count(data.edges.length)} internal links';
    } else if (overview != null) {
      subtitle =
          '${formatCount(overview.pages)} pages in '
          '${overview.topSections} top-level '
          'section${overview.topSections == 1 ? '' : 's'}';
    } else {
      subtitle = 'Reading the site structure…';
    }

    final Widget body;
    if (_mode == _Mode.structure) {
      body = architecture.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, _) => EmptyState(
          icon: AppIcon.structure,
          title: 'Could not read the site structure',
          message: '$error',
        ),
        data: (data) => ArchitectureOverview(
          architecture: data,
          depths: structure.value?.depths,
          onShowSection: _showInGraph,
        ),
      );
    } else {
      body = structure.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, _) => EmptyState(
          icon: AppIcon.visualise,
          title: 'Could not read the link graph',
          message: '$error',
        ),
        data: (data) => _GraphMode(
          data: data,
          sections: [
            for (final section
                in architecture.value?.sections ?? const <arch.ArchSection>[])
              if (section.level == 1) section,
          ],
          hasLinkScore: overview?.hasLinkScore ?? false,
          selected: _selected,
          highlight: _highlight,
          onSelected: (node) => setState(() => _selected = node),
          onHighlight: (path) => setState(() => _highlight = path),
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ScreenHeader(
          title: 'Site architecture',
          subtitle: subtitle,
          actions: [picker],
        ),
        Expanded(child: body),
      ],
    );
  }
}

// ─── structure ──────────────────────────────────────────────────────────

/// The structure view: headline numbers, what to fix, the section tree and
/// the section link map. Public so it can be tested with data built by hand.
class ArchitectureOverview extends StatelessWidget {
  const ArchitectureOverview({
    super.key,
    required this.architecture,
    required this.onShowSection,
    this.depths,
  });

  final arch.SiteArchitecture architecture;
  final List<rust.DepthCount>? depths;

  /// Open the link graph with this folder highlighted.
  final ValueChanged<String> onShowSection;

  @override
  Widget build(BuildContext context) {
    final data = architecture;
    if (data.overview.pages == 0) {
      return const EmptyState(
        icon: AppIcon.structure,
        title: 'No pages to map',
        message:
            'This crawl has no working HTML pages yet. Run or resume the '
            'crawl, then come back here.',
      );
    }

    final tree = _TreeCard(
      sections: data.sections,
      hasLinkScore: data.overview.hasLinkScore,
      onShowSection: onShowSection,
    );
    final map = _MapCard(map: data.map);
    final depth = depths == null || depths!.isEmpty
        ? null
        : _DepthCard(depths: depths!);

    return ListView(
      padding: const EdgeInsets.fromLTRB(
        CrawlStatTheme.gutter,
        0,
        CrawlStatTheme.gutter,
        CrawlStatTheme.gutter,
      ),
      children: [
        _Kpis(overview: data.overview, findings: data.findings),
        const SizedBox(height: 16),
        if (!data.overview.hasLinkScore) ...[
          const InlineMessage(
            severity: 'notice',
            message:
                'Link scores have not been computed for this crawl, so the '
                'tree cannot show where internal link value goes. Re-run the '
                'audit from the Issues screen to add them.',
          ),
          const SizedBox(height: 16),
        ],
        _FindingsCard(findings: data.findings),
        const SizedBox(height: 16),
        LayoutBuilder(
          builder: (context, constraints) {
            if (constraints.maxWidth >= 1150) {
              return Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(flex: 3, child: tree),
                  const SizedBox(width: 16),
                  Expanded(
                    flex: 2,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        map,
                        if (depth != null) ...[
                          const SizedBox(height: 16),
                          depth,
                        ],
                      ],
                    ),
                  ),
                ],
              );
            }
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                tree,
                const SizedBox(height: 16),
                map,
                if (depth != null) ...[const SizedBox(height: 16), depth],
              ],
            );
          },
        ),
      ],
    );
  }
}

class _Kpis extends StatelessWidget {
  const _Kpis({required this.overview, required this.findings});

  final arch.ArchOverview overview;
  final List<arch.ArchFinding> findings;

  @override
  Widget build(BuildContext context) {
    final reach = overview.withinReach;
    final warnings = findings.where((f) => f.severity == 'warning').length;
    final tiles = [
      StatTile(
        label: 'Pages',
        value: formatCount(overview.pages),
        hint: 'working HTML pages',
        icon: AppIcon.report,
      ),
      StatTile(
        label: 'Within 3 clicks',
        value: '${(reach * 100).round()}%',
        hint: 'of pages, from the start page',
        icon: AppIcon.structure,
        color: reach >= 0.9
            ? CrawlStatTheme.palette(context).success
            : reach >= 0.7
            ? CrawlStatTheme.palette(context).warning
            : CrawlStatTheme.palette(context).error,
      ),
      StatTile(
        label: 'Average depth',
        value: overview.avgDepth.toStringAsFixed(1),
        hint: 'clicks from the start page',
        icon: AppIcon.link,
      ),
      StatTile(
        label: 'Deepest page',
        value: '${overview.maxDepth}',
        hint: 'clicks',
        icon: AppIcon.discovered,
        color: overview.maxDepth > 3
            ? CrawlStatTheme.palette(context).warning
            : null,
      ),
      StatTile(
        label: 'To fix',
        value: '${findings.length}',
        hint: warnings == 0
            ? 'structural findings'
            : '$warnings warning${warnings == 1 ? '' : 's'}',
        icon: AppIcon.priority,
        color: warnings > 0
            ? CrawlStatTheme.palette(context).warning
            : findings.isEmpty
            ? CrawlStatTheme.palette(context).success
            : null,
      ),
    ];
    return LayoutBuilder(
      builder: (context, constraints) {
        const gap = 12.0;
        final columns = constraints.maxWidth >= 900
            ? 5
            : constraints.maxWidth >= 560
            ? 3
            : 2;
        final width =
            (constraints.maxWidth - gap * (columns - 1)) / columns - 0.01;
        return Wrap(
          spacing: gap,
          runSpacing: gap,
          children: [
            for (final tile in tiles) SizedBox(width: width, child: tile),
          ],
        );
      },
    );
  }
}

// ─── findings ───────────────────────────────────────────────────────────

class _FindingsCard extends StatelessWidget {
  const _FindingsCard({required this.findings});

  final List<arch.ArchFinding> findings;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SectionCard(
      title: 'What to fix',
      subtitle: 'Structural problems, most important first',
      child: findings.isEmpty
          ? Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Row(
                children: [
                  IconBadge(
                    AppIcon.ok,
                    color: CrawlStatTheme.palette(context).success,
                    size: 30,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      'No structural problems found: pages are within easy '
                      'reach, every section has a way in, and nothing hangs '
                      'off a single link.',
                      style: theme.textTheme.bodyMedium,
                    ),
                  ),
                ],
              ),
            )
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                for (final (index, finding) in findings.indexed) ...[
                  if (index > 0) const SizedBox(height: 10),
                  _FindingTile(finding: finding),
                ],
              ],
            ),
    );
  }
}

class _FindingTile extends StatefulWidget {
  const _FindingTile({required this.finding});

  final arch.ArchFinding finding;

  @override
  State<_FindingTile> createState() => _FindingTileState();
}

class _FindingTileState extends State<_FindingTile> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final finding = widget.finding;
    final colour = CrawlStatTheme.severityColor(context, finding.severity);
    final unit = switch (finding.id) {
      'missing_hubs' => finding.count == 1 ? 'folder' : 'folders',
      'underlinked_sections' ||
      'isolated_sections' => finding.count == 1 ? 'section' : 'sections',
      _ => finding.count == 1 ? 'page' : 'pages',
    };

    return SoftWell(
      radius: CrawlStatTheme.controlRadius,
      depth: 0.6,
      child: Material(
        type: MaterialType.transparency,
        child: InkWell(
          borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius),
          onTap: () => setState(() => _open = !_open),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    IconBadge(
                      finding.severity == 'warning'
                          ? AppIcon.warning
                          : AppIcon.error,
                      color: colour,
                      size: 30,
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Wrap(
                            spacing: 8,
                            runSpacing: 4,
                            crossAxisAlignment: WrapCrossAlignment.center,
                            children: [
                              Text(
                                finding.title,
                                style: theme.textTheme.titleSmall,
                              ),
                              TonePill(
                                '${formatCount(finding.count)} $unit',
                                color: colour,
                              ),
                            ],
                          ),
                          const SizedBox(height: 4),
                          Text(
                            finding.summary,
                            style: theme.textTheme.bodyMedium?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(width: 8),
                    Icon(
                      _open ? AppIcon.chevronUp : AppIcon.chevronDown,
                      size: 18,
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ],
                ),
                if (_open) ...[
                  const SizedBox(height: 14),
                  Padding(
                    padding: const EdgeInsets.only(left: 42),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Text('How to fix', style: theme.textTheme.labelLarge),
                        const SizedBox(height: 4),
                        Text(finding.fix, style: theme.textTheme.bodyMedium),
                        if (finding.examples.isNotEmpty) ...[
                          const SizedBox(height: 14),
                          Text(
                            finding.count > finding.examples.length
                                ? 'Examples (${finding.examples.length} of '
                                      '${formatCount(finding.count)})'
                                : 'Where',
                            style: theme.textTheme.labelLarge,
                          ),
                          const SizedBox(height: 6),
                          for (final example in finding.examples)
                            _ExampleRow(example: example),
                        ],
                      ],
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ExampleRow extends StatelessWidget {
  const _ExampleRow({required this.example});

  final arch.ArchExample example;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final url = example.url;
    final openable = url.startsWith('http');
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          Expanded(
            flex: 3,
            child: Text(
              openable ? _pathOf(url) : url,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: CrawlStatTheme.mono(context, size: 12),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            flex: 2,
            child: Text(
              example.detail,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.bodySmall,
            ),
          ),
          SizedBox(
            width: 30,
            child: openable
                ? IconButton(
                    tooltip: 'Open $url',
                    iconSize: 14,
                    visualDensity: VisualDensity.compact,
                    padding: EdgeInsets.zero,
                    icon: const Icon(AppIcon.open),
                    onPressed: () => _openPage(context, url),
                  )
                : null,
          ),
        ],
      ),
    );
  }
}

// ─── the section tree ───────────────────────────────────────────────────

class _TreeCard extends StatefulWidget {
  const _TreeCard({
    required this.sections,
    required this.hasLinkScore,
    required this.onShowSection,
  });

  /// Depth first, with `level` for indentation; the first is the whole site.
  final List<arch.ArchSection> sections;
  final bool hasLinkScore;
  final ValueChanged<String> onShowSection;

  @override
  State<_TreeCard> createState() => _TreeCardState();
}

class _TreeCardState extends State<_TreeCard> {
  /// Folders whose children are showing. The site root starts open.
  final Set<String> _expanded = {'/'};

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final sections = widget.sections;

    // A row shows when every folder above it is open.
    final visible = <arch.ArchSection>[];
    final openAt = <bool>[];
    for (final section in sections) {
      final level = section.level;
      openAt.length = math.min(openAt.length, level);
      if (openAt.every((open) => open)) visible.add(section);
      openAt.add(_expanded.contains(section.path));
    }

    // Bars are scaled to the biggest section rather than to 100%, so a site
    // of many small sections still has bars long enough to compare.
    var widest = 0.0;
    for (final section in sections) {
      if (section.level == 0) continue;
      widest = math.max(
        widest,
        math.max(section.pageShare, section.valueShare),
      );
    }

    return SectionCard(
      title: 'Section tree',
      subtitle: widget.hasLinkScore
          ? 'Folders from the URLs. Each pair of bars compares a section’s '
                'share of pages with its share of internal link value.'
          : 'Folders from the URLs, with how many pages each holds and how '
                'deep they sit.',
      trailing: widget.hasLinkScore ? const _ShareLegend() : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _TreeHeader(hasLinkScore: widget.hasLinkScore),
          Divider(height: 1, color: theme.dividerColor),
          for (final section in visible)
            _TreeRow(
              section: section,
              widest: widest,
              hasLinkScore: widget.hasLinkScore,
              expanded: _expanded.contains(section.path),
              onToggle: section.hasChildren
                  ? () => setState(() {
                      if (!_expanded.remove(section.path)) {
                        _expanded.add(section.path);
                      }
                    })
                  : null,
              onShow: () => widget.onShowSection(section.path),
            ),
        ],
      ),
    );
  }
}

const double _pagesColumn = 64;
const double _depthColumn = 64;
const double _barsColumn = 130;

class _TreeHeader extends StatelessWidget {
  const _TreeHeader({required this.hasLinkScore});

  final bool hasLinkScore;

  @override
  Widget build(BuildContext context) {
    final style = Theme.of(context).textTheme.labelMedium;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(
        children: [
          Expanded(flex: 3, child: Text('FOLDER', style: style)),
          SizedBox(
            width: _pagesColumn,
            child: Text('PAGES', style: style, textAlign: TextAlign.right),
          ),
          SizedBox(
            width: _depthColumn,
            child: Text('DEPTH', style: style, textAlign: TextAlign.right),
          ),
          if (hasLinkScore) ...[
            const SizedBox(width: 16),
            SizedBox(
              width: _barsColumn,
              child: Text('PAGES · VALUE', style: style),
            ),
          ],
          const SizedBox(width: 12),
          Expanded(flex: 2, child: Text('PROBLEMS', style: style)),
          const SizedBox(width: 32),
        ],
      ),
    );
  }
}

class _TreeRow extends StatelessWidget {
  const _TreeRow({
    required this.section,
    required this.widest,
    required this.hasLinkScore,
    required this.expanded,
    required this.onToggle,
    required this.onShow,
  });

  final arch.ArchSection section;
  final double widest;
  final bool hasLinkScore;
  final bool expanded;
  final VoidCallback? onToggle;
  final VoidCallback onShow;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final s = section;
    final hubOk =
        s.hubStatus != null && s.hubStatus! >= 200 && s.hubStatus! < 300;
    final pills = <Widget>[
      if (s.level > 0 && s.pages >= 5 && !hubOk)
        TonePill(
          'No landing page',
          color: CrawlStatTheme.palette(context).warning,
        ),
      if (s.errors > 0)
        TonePill(
          '${formatCount(s.errors)} broken',
          color: CrawlStatTheme.palette(context).error,
        ),
      if (s.deep > 0)
        TonePill(
          '${formatCount(s.deep)} deep',
          color: CrawlStatTheme.palette(context).warning,
        ),
      if (s.weak > 0)
        TonePill(
          '${formatCount(s.weak)} weak',
          color: CrawlStatTheme.palette(context).notice,
        ),
    ];

    final label = Row(
      children: [
        SizedBox(width: s.level * 18.0),
        SizedBox(
          width: 22,
          child: onToggle == null
              ? null
              : Icon(
                  expanded ? AppIcon.chevronDown : AppIcon.chevronRight,
                  size: 15,
                  color: theme.colorScheme.onSurfaceVariant,
                ),
        ),
        Icon(
          s.level == 0
              ? AppIcon.web
              : expanded && onToggle != null
              ? AppIcon.folderOpen
              : AppIcon.folder,
          size: 14,
          color: theme.colorScheme.onSurfaceVariant,
        ),
        const SizedBox(width: 8),
        Flexible(
          child: Tooltip(
            message: s.level == 0 ? 'The whole site' : s.path,
            child: Text(
              s.level == 0 ? s.label : '${s.label}/',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: CrawlStatTheme.mono(
                context,
                size: 12.5,
                color: theme.colorScheme.onSurface,
              ).copyWith(fontWeight: s.level <= 1 ? FontWeight.w600 : null),
            ),
          ),
        ),
      ],
    );

    return InkWell(
      onTap: onToggle,
      borderRadius: BorderRadius.circular(8),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 7),
        child: Row(
          children: [
            Expanded(flex: 3, child: label),
            SizedBox(
              width: _pagesColumn,
              child: Text(
                formatCount(s.pages),
                textAlign: TextAlign.right,
                style: CrawlStatTheme.mono(context, size: 12.5),
              ),
            ),
            SizedBox(
              width: _depthColumn,
              child: Tooltip(
                message:
                    'Average ${s.avgDepth.toStringAsFixed(1)} clicks · '
                    'deepest ${s.maxDepth}',
                child: Text(
                  s.avgDepth.toStringAsFixed(1),
                  textAlign: TextAlign.right,
                  style: CrawlStatTheme.mono(
                    context,
                    size: 12.5,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            ),
            if (hasLinkScore) ...[
              const SizedBox(width: 16),
              SizedBox(
                width: _barsColumn,
                child: s.level == 0
                    ? null
                    : _ShareBars(
                        pageShare: s.pageShare,
                        valueShare: s.valueShare,
                        widest: widest,
                      ),
              ),
            ],
            const SizedBox(width: 12),
            Expanded(
              flex: 2,
              child: Wrap(spacing: 4, runSpacing: 4, children: pills),
            ),
            SizedBox(
              width: 32,
              child: IconButton(
                tooltip: 'Highlight in the link graph',
                iconSize: 15,
                visualDensity: VisualDensity.compact,
                padding: EdgeInsets.zero,
                icon: const Icon(AppIcon.visualise),
                onPressed: onShow,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Share of pages (grey) over share of link value (accent), as two thin bars
/// on the same scale, so a section fed less than its size shows a short
/// orange bar under a long grey one.
class _ShareBars extends StatelessWidget {
  const _ShareBars({
    required this.pageShare,
    required this.valueShare,
    required this.widest,
  });

  final double pageShare;
  final double valueShare;
  final double widest;

  @override
  Widget build(BuildContext context) {
    String pct(double v) => v > 0 && v < 0.01
        ? '<1%'
        : '${(v * 100).toStringAsFixed(v < 0.1 ? 1 : 0)}%';
    final scale = widest <= 0 ? 1.0 : widest;
    return Tooltip(
      message:
          '${pct(pageShare)} of pages · ${pct(valueShare)} of internal link '
          'value',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          _Bar(
            share: pageShare / scale,
            color: CrawlStatTheme.palette(context).notice,
          ),
          const SizedBox(height: 3),
          _Bar(
            share: valueShare / scale,
            color: CrawlStatTheme.palette(context).accent,
          ),
        ],
      ),
    );
  }
}

class _Bar extends StatelessWidget {
  const _Bar({required this.share, required this.color});

  final double share;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 5,
      child: FractionallySizedBox(
        alignment: Alignment.centerLeft,
        // A sliver even at zero, so "none" reads as a value and not a gap.
        widthFactor: share.clamp(0.015, 1.0),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: color,
            borderRadius: BorderRadius.circular(3),
          ),
        ),
      ),
    );
  }
}

class _ShareLegend extends StatelessWidget {
  const _ShareLegend();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    Widget entry(String label, Color colour) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 12,
          height: 5,
          decoration: BoxDecoration(
            color: colour,
            borderRadius: BorderRadius.circular(3),
          ),
        ),
        const SizedBox(width: 5),
        Text(label, style: theme.textTheme.bodySmall),
      ],
    );
    return Wrap(
      spacing: 12,
      children: [
        entry('Pages', CrawlStatTheme.palette(context).notice),
        entry('Link value', CrawlStatTheme.palette(context).accent),
      ],
    );
  }
}

// ─── the section link map ───────────────────────────────────────────────

class _MapCard extends StatelessWidget {
  const _MapCard({required this.map});

  final arch.ArchMap map;

  @override
  Widget build(BuildContext context) {
    return SectionCard(
      title: 'How sections link',
      subtitle:
          'Pages in each row’s section linking to pages in each column’s. '
          'Darker is more. A pale column is a section the rest of the site '
          'rarely points to.',
      child: map.sections.length < 2
          ? Text(
              'The site has a single section, so there is nothing to compare.',
              style: Theme.of(context).textTheme.bodySmall,
            )
          : _LinkMatrix(map: map),
    );
  }
}

class _LinkMatrix extends StatelessWidget {
  const _LinkMatrix({required this.map});

  final arch.ArchMap map;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final n = map.sections.length;
    final links = List.generate(n, (_) => List<int>.filled(n, 0));
    for (final flow in map.flows) {
      if (flow.from < n && flow.to < n) links[flow.from][flow.to] = flow.links;
    }
    // Scaled to the busiest link *between* sections. Links within a section
    // are usually far more numerous and would wash every other cell out;
    // they are drawn at full strength past that.
    var busiest = 0;
    for (var i = 0; i < n; i++) {
      for (var j = 0; j < n; j++) {
        if (i != j) busiest = math.max(busiest, links[i][j]);
      }
    }
    if (busiest == 0) busiest = 1;

    return LayoutBuilder(
      builder: (context, constraints) {
        const labelWidth = 92.0;
        final cell = math.min(46.0, (constraints.maxWidth - labelWidth) / n);
        final headerStyle = CrawlStatTheme.mono(
          context,
          size: 11,
          color: theme.colorScheme.onSurfaceVariant,
        );

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                SizedBox(
                  width: labelWidth,
                  child: Text('from ↓  to →', style: theme.textTheme.bodySmall),
                ),
                for (final name in map.sections)
                  SizedBox(
                    width: cell,
                    height: 70,
                    child: Align(
                      alignment: Alignment.bottomCenter,
                      child: RotatedBox(
                        quarterTurns: 3,
                        child: Text(
                          name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: headerStyle,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 4),
            for (var i = 0; i < n; i++)
              Row(
                children: [
                  SizedBox(
                    width: labelWidth,
                    child: Text.rich(
                      TextSpan(
                        children: [
                          TextSpan(text: map.sections[i]),
                          TextSpan(
                            text: '  ${formatCount(map.pages[i])}',
                            style: theme.textTheme.bodySmall,
                          ),
                        ],
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: CrawlStatTheme.mono(context, size: 11.5),
                    ),
                  ),
                  for (var j = 0; j < n; j++)
                    _MatrixCell(
                      size: cell,
                      links: links[i][j],
                      strength: math.min(1, links[i][j] / busiest),
                      message: i == j
                          ? '${map.sections[i]} → itself\n'
                                '${formatCount(links[i][j])} links between '
                                'its own pages'
                          : '${map.sections[i]} → ${map.sections[j]}\n'
                                '${formatCount(links[i][j])} '
                                'linking page pair'
                                '${links[i][j] == 1 ? '' : 's'}',
                    ),
                ],
              ),
          ],
        );
      },
    );
  }
}

class _MatrixCell extends StatelessWidget {
  const _MatrixCell({
    required this.size,
    required this.links,
    required this.strength,
    required this.message,
  });

  final double size;
  final int links;
  final double strength;
  final String message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    // One hue, light to dark: square-rooted so the many small counts are
    // still told apart from none at all.
    final fill = links == 0
        ? theme.colorScheme.onSurface.withValues(alpha: 0.04)
        : CrawlStatTheme.palette(context).accent
              .withValues(alpha: 0.14 + 0.86 * math.sqrt(strength));
    final strong = links > 0 && strength > 0.45;
    return Tooltip(
      message: message,
      waitDuration: const Duration(milliseconds: 150),
      child: Container(
        width: size,
        height: math.min(size, 34),
        padding: const EdgeInsets.all(1),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: fill,
            borderRadius: BorderRadius.circular(4),
          ),
          child: Center(
            child: size < 30 || links == 0
                ? null
                : Text(
                    _count(links),
                    maxLines: 1,
                    style: CrawlStatTheme.mono(
                      context,
                      size: 10.5,
                      color: strong
                          ? Colors.white
                          : theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
          ),
        ),
      ),
    );
  }
}

// ─── the link graph ─────────────────────────────────────────────────────

class _GraphMode extends StatelessWidget {
  const _GraphMode({
    required this.data,
    required this.sections,
    required this.hasLinkScore,
    required this.selected,
    required this.highlight,
    required this.onSelected,
    required this.onHighlight,
  });

  final rust.SiteStructure data;
  final List<arch.ArchSection> sections;
  final bool hasLinkScore;
  final rust.SiteNode? selected;
  final String? highlight;
  final ValueChanged<rust.SiteNode?> onSelected;
  final ValueChanged<String?> onHighlight;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        CrawlStatTheme.gutter,
        0,
        CrawlStatTheme.gutter,
        CrawlStatTheme.gutter,
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          // The diagram needs room above all else. Below this the side
          // column is stacked underneath instead of squeezing it.
          final side = constraints.maxWidth >= 1000;
          final graph = _GraphPanel(
            data: data,
            hasLinkScore: hasLinkScore,
            selected: selected,
            highlight: highlight,
            onSelected: onSelected,
          );
          final details = _SidePanel(
            data: data,
            sections: sections,
            selected: selected,
            highlight: highlight,
            onHighlight: onHighlight,
          );

          if (side) {
            return Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(child: graph),
                const SizedBox(width: 16),
                SizedBox(width: 320, child: details),
              ],
            );
          }
          return CustomScrollView(
            slivers: [
              SliverToBoxAdapter(child: SizedBox(height: 460, child: graph)),
              const SliverToBoxAdapter(child: SizedBox(height: 16)),
              SliverToBoxAdapter(child: details),
            ],
          );
        },
      ),
    );
  }
}

class _GraphPanel extends StatelessWidget {
  const _GraphPanel({
    required this.data,
    required this.hasLinkScore,
    required this.selected,
    required this.highlight,
    required this.onSelected,
  });

  final rust.SiteStructure data;
  final bool hasLinkScore;
  final rust.SiteNode? selected;
  final String? highlight;
  final ValueChanged<rust.SiteNode?> onSelected;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final byId = {for (final node in data.nodes) node.id: node};

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Text('Link graph', style: theme.textTheme.titleSmall),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  'rings are clicks from home · slices are sections · '
                  'size is ${hasLinkScore ? 'link value' : 'inlinks'}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodySmall,
                ),
              ),
              _GraphLegend(highlight: highlight),
            ],
          ),
          const SizedBox(height: 12),
          Expanded(
            child: SoftWell(
              radius: CrawlStatTheme.radius - 4,
              depth: 0.8,
              child: ClipRRect(
                borderRadius: BorderRadius.circular(CrawlStatTheme.radius - 4),
                child: SiteGraphView(
                  selectedId: selected?.id,
                  highlight: highlight,
                  nodes: [
                    for (final node in data.nodes)
                      GraphNodeData(
                        id: node.id,
                        url: node.url,
                        label: node.label,
                        depth: node.depth,
                        inlinks: node.inlinks,
                        status: node.status,
                        linkScore: node.linkScore,
                      ),
                  ],
                  edges: [
                    for (final edge in data.edges)
                      GraphEdgeData(source: edge.source, target: edge.target),
                  ],
                  onSelected: (node) => onSelected(byId[node.id]),
                ),
              ),
            ),
          ),
          const SizedBox(height: 8),
          Text(
            'Drag to pan · scroll to zoom · double-click to reset · '
            'click a page for its details',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

class _GraphLegend extends StatelessWidget {
  const _GraphLegend({required this.highlight});

  final String? highlight;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    // A healthy page is neutral, not green. With a few hundred nodes, colouring
    // every working page would leave nothing for the broken ones to stand out
    // against — the point of the colour here is to find trouble, not to
    // celebrate the 95% of pages that are fine.
    final entries = [
      if (highlight != null)
        (highlight!, CrawlStatTheme.palette(context).accent),
      ('OK', CrawlStatTheme.palette(context).notice),
      ('Redirect', CrawlStatTheme.palette(context).warning),
      ('Error', CrawlStatTheme.palette(context).error),
    ];
    return Wrap(
      spacing: 10,
      children: [
        for (final (label, colour) in entries)
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 8,
                height: 8,
                decoration: BoxDecoration(
                  gradient: glossOf(colour),
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 4),
              Text(label, style: theme.textTheme.bodySmall),
            ],
          ),
      ],
    );
  }
}

/// The column beside the diagram: what you clicked, the sections to light
/// up, and the charts that say what the diagram cannot.
class _SidePanel extends StatelessWidget {
  const _SidePanel({
    required this.data,
    required this.sections,
    required this.selected,
    required this.highlight,
    required this.onHighlight,
  });

  final rust.SiteStructure data;
  final List<arch.ArchSection> sections;
  final rust.SiteNode? selected;
  final String? highlight;
  final ValueChanged<String?> onHighlight;

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (selected != null) ...[
            _SelectedPage(node: selected!),
            const SizedBox(height: 16),
          ],
          if (sections.isNotEmpty) ...[
            _SectionPicker(
              sections: sections,
              highlight: highlight,
              onHighlight: onHighlight,
            ),
            const SizedBox(height: 16),
          ],
          _DepthCard(depths: data.depths),
          const SizedBox(height: 16),
          _StatusCard(statuses: data.statuses),
        ],
      ),
    );
  }
}

/// The top-level sections, to light one up in the diagram.
class _SectionPicker extends StatelessWidget {
  const _SectionPicker({
    required this.sections,
    required this.highlight,
    required this.onHighlight,
  });

  final List<arch.ArchSection> sections;
  final String? highlight;
  final ValueChanged<String?> onHighlight;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final most = sections.fold<int>(0, (a, s) => math.max(a, s.pages));
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              Expanded(
                child: Text('Sections', style: theme.textTheme.titleSmall),
              ),
              if (highlight != null)
                TextButton(
                  onPressed: () => onHighlight(null),
                  child: const Text('Clear'),
                ),
            ],
          ),
          Text(
            'Click one to light it up in the graph',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 8),
          for (final section in sections)
            _PickerRow(
              section: section,
              most: most,
              lit: highlight == section.path,
              onTap: () =>
                  onHighlight(highlight == section.path ? null : section.path),
            ),
        ],
      ),
    );
  }
}

class _PickerRow extends StatelessWidget {
  const _PickerRow({
    required this.section,
    required this.most,
    required this.lit,
    required this.onTap,
  });

  final arch.ArchSection section;
  final int most;
  final bool lit;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      type: MaterialType.transparency,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(8),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 6),
          decoration: BoxDecoration(
            color: lit
                ? CrawlStatTheme.palette(context).accent.withValues(alpha: 0.14)
                : null,
            borderRadius: BorderRadius.circular(8),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      section.path,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: CrawlStatTheme.mono(
                        context,
                        size: 12,
                        color: theme.colorScheme.onSurface,
                      ).copyWith(fontWeight: lit ? FontWeight.w700 : null),
                    ),
                  ),
                  Text(
                    formatCount(section.pages),
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
              const SizedBox(height: 4),
              _Bar(
                share: most == 0 ? 0 : section.pages / most,
                color: lit
                    ? CrawlStatTheme.palette(context).accent
                    : CrawlStatTheme.palette(context).notice,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SelectedPage extends ConsumerWidget {
  const _SelectedPage({required this.node});

  final rust.SiteNode node;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(node.label, style: theme.textTheme.titleSmall),
          const SizedBox(height: 4),
          Text(
            node.url,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: CrawlStatTheme.mono(
              context,
              size: 11.5,
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 18,
            runSpacing: 8,
            children: [
              _Figure(label: 'Depth', value: '${node.depth}'),
              _Figure(label: 'Inlinks', value: '${node.inlinks}'),
              _Figure(label: 'Outlinks', value: '${node.outlinks}'),
              if (node.linkScore > 0)
                _Figure(
                  label: 'Link score',
                  value: node.linkScore.toStringAsFixed(0),
                ),
              _Figure(
                label: 'Status',
                value: node.status?.toString() ?? '—',
                color: nodeColour(
                  node.status,
                  theme.colorScheme.onSurface,
                  CrawlStatTheme.palette(context),
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              SoftButton(
                icon: AppIcon.results,
                onPressed: () => context.go(Routes.results),
                child: const Text('Find in results'),
              ),
              SoftButton(
                icon: AppIcon.open,
                onPressed: () => _openPage(context, node.url),
                child: const Text('Open'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _Figure extends StatelessWidget {
  const _Figure({required this.label, required this.value, this.color});

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
          style: CrawlStatTheme.mono(
            context,
            size: 17,
            color: color ?? theme.colorScheme.onSurface,
          ).copyWith(fontWeight: FontWeight.w600),
        ),
      ],
    );
  }
}

/// How far a visitor has to travel. The single most useful chart an SEO
/// crawler draws: pages past depth three or four are, in practice, pages
/// nobody reaches.
class _DepthCard extends StatelessWidget {
  const _DepthCard({required this.depths});

  final List<rust.DepthCount> depths;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final deepest = depths.isEmpty
        ? 0
        : depths.map((d) => d.depth).reduce(math.max);
    final buried = depths
        .where((d) => d.depth > 3)
        .fold<int>(0, (sum, d) => sum + d.count);

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text('Clicks from home', style: theme.textTheme.titleSmall),
          const SizedBox(height: 2),
          Text(
            buried > 0
                ? '$buried page${buried == 1 ? '' : 's'} more than 3 clicks deep'
                : 'Nothing is more than 3 clicks from the home page',
            style: theme.textTheme.bodySmall,
          ),
          SizedBox(height: 14),
          BarChart(
            height: 96,
            data: [
              for (final entry in depths)
                ChartDatum(
                  label: '${entry.depth}',
                  value: entry.count,
                  // Deeper is worse, so the bars warm up as they go right.
                  color: rampColourAt(
                    deepest == 0 ? 0 : entry.depth / deepest,
                    [
                      CrawlStatTheme.palette(context).success,
                      CrawlStatTheme.palette(context).accent,
                      CrawlStatTheme.palette(context).error,
                    ],
                  ),
                  hint: '${entry.count} pages at depth ${entry.depth}',
                ),
            ],
          ),
        ],
      ),
    );
  }
}

class _StatusCard extends StatelessWidget {
  const _StatusCard({required this.statuses});

  final List<rust.StatusCount> statuses;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final buckets = <int, int>{};
    for (final entry in statuses) {
      buckets.update(
        entry.status ~/ 100,
        (value) => value + entry.count,
        ifAbsent: () => entry.count,
      );
    }
    final total = buckets.values.fold<int>(0, (a, b) => a + b);

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text('Responses', style: theme.textTheme.titleSmall),
          const SizedBox(height: 14),
          Center(
            child: DonutChart(
              size: 148,
              centreValue: _count(total),
              centreLabel: 'URLs',
              data: [
                for (final entry
                    in buckets.entries.toList()
                      ..sort((a, b) => a.key.compareTo(b.key)))
                  ChartDatum(
                    label: '${entry.key}xx',
                    value: entry.value,
                    color: CrawlStatTheme.statusColor(context, entry.key * 100),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

Future<void> _openPage(BuildContext context, String url) async {
  final messenger = ScaffoldMessenger.of(context);
  if (!await openLink(url)) {
    messenger.showSnackBar(
      const SnackBar(content: Text('Link copied — paste it into a browser')),
    );
  }
}

/// `https://example.com/blog/post?x=1` → `/blog/post?x=1`.
String _pathOf(String url) {
  final uri = Uri.tryParse(url);
  if (uri == null || uri.host.isEmpty) return url;
  final path = uri.path.isEmpty ? '/' : uri.path;
  return uri.hasQuery ? '$path?${uri.query}' : path;
}

String _count(int value) {
  if (value < 1000) return '$value';
  if (value < 1000000) return '${(value / 1000).toStringAsFixed(1)}k';
  return '${(value / 1000000).toStringAsFixed(1)}M';
}
