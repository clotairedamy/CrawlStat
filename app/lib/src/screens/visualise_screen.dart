import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../theme/icons.dart';
import '../router.dart';
import '../rust/api/project.dart' as rust;
import '../state/projects.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';
import '../widgets/charts.dart';
import '../widgets/common.dart';
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

class VisualiseScreen extends ConsumerStatefulWidget {
  const VisualiseScreen({super.key});

  @override
  ConsumerState<VisualiseScreen> createState() => _VisualiseScreenState();
}

class _VisualiseScreenState extends ConsumerState<VisualiseScreen> {
  rust.SiteNode? _selected;

  @override
  Widget build(BuildContext context) {
    final path = ref.watch(openProjectProvider);
    if (path == null) return const NoProjectState();

    final structure = ref.watch(siteStructureProvider(path));

    return structure.when(
      loading: () => const Column(
        children: [
          ScreenHeader(title: 'Visualise', subtitle: 'Reading the link graph…'),
          Expanded(child: Center(child: CircularProgressIndicator())),
        ],
      ),
      error: (error, _) => Column(
        children: [
          const ScreenHeader(title: 'Visualise'),
          Expanded(
            child: EmptyState(
              icon: AppIcon.visualise,
              title: 'Could not read the site structure',
              message: '$error',
            ),
          ),
        ],
      ),
      data: (data) => _Body(
        data: data,
        selected: _selected,
        onSelected: (node) => setState(() => _selected = node),
      ),
    );
  }
}

class _Body extends ConsumerWidget {
  const _Body({
    required this.data,
    required this.selected,
    required this.onSelected,
  });

  final rust.SiteStructure data;
  final rust.SiteNode? selected;
  final ValueChanged<rust.SiteNode?> onSelected;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final capped = data.totalPages > data.nodes.length;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ScreenHeader(
          title: 'Visualise',
          subtitle: capped
              ? '${_count(data.nodes.length)} of ${_count(data.totalPages)} pages '
                    '· the best-connected ones, shallowest first'
              : '${_count(data.nodes.length)} pages · '
                    '${_count(data.edges.length)} internal links',
        ),
        Expanded(
          child: Padding(
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
                  selected: selected,
                  onSelected: onSelected,
                );
                final details = _SidePanel(data: data, selected: selected);

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
                    SliverToBoxAdapter(
                      child: SizedBox(height: 420, child: graph),
                    ),
                    const SliverToBoxAdapter(child: SizedBox(height: 16)),
                    SliverToBoxAdapter(child: details),
                  ],
                );
              },
            ),
          ),
        ),
      ],
    );
  }
}

class _GraphPanel extends StatelessWidget {
  const _GraphPanel({
    required this.data,
    required this.selected,
    required this.onSelected,
  });

  final rust.SiteStructure data;
  final rust.SiteNode? selected;
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
                  'rings are clicks from the home page · size is inlinks',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodySmall,
                ),
              ),
              const _GraphLegend(),
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
                  nodes: [
                    for (final node in data.nodes)
                      GraphNodeData(
                        id: node.id,
                        url: node.url,
                        label: node.label,
                        depth: node.depth,
                        inlinks: node.inlinks,
                        status: node.status,
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
  const _GraphLegend();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    // A healthy page is neutral, not green. With a few hundred nodes, colouring
    // every working page would leave nothing for the broken ones to stand out
    // against — the point of the colour here is to find trouble, not to
    // celebrate the 95% of pages that are fine.
    const entries = [
      ('OK', CrawlStatTheme.notice),
      ('Redirect', CrawlStatTheme.warning),
      ('Error', CrawlStatTheme.error),
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

/// The column beside the diagram: what you clicked, and the charts that say
/// what the diagram cannot.
class _SidePanel extends ConsumerWidget {
  const _SidePanel({required this.data, required this.selected});

  final rust.SiteStructure data;
  final rust.SiteNode? selected;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return SingleChildScrollView(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (selected != null) ...[
            _SelectedPage(node: selected!),
            const SizedBox(height: 16),
          ],
          _DepthCard(depths: data.depths),
          const SizedBox(height: 16),
          _StatusCard(statuses: data.statuses),
          const SizedBox(height: 16),
          _SectionsCard(sections: data.sections),
        ],
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
              _Figure(
                label: 'Status',
                value: node.status?.toString() ?? '—',
                color: nodeColour(node.status, theme.colorScheme.onSurface),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Align(
            alignment: Alignment.centerLeft,
            child: SoftButton(
              icon: AppIcon.results,
              onPressed: () => context.go(Routes.results),
              child: const Text('Find in results'),
            ),
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
          const SizedBox(height: 14),
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
                    const [
                      CrawlStatTheme.success,
                      CrawlStatTheme.accent,
                      CrawlStatTheme.error,
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
                    color: CrawlStatTheme.statusColor(entry.key * 100),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// The site's top-level sections, sized by how many pages they hold.
///
/// A treemap rather than another bar chart: the question here is "how is the
/// site divided up", and relative *area* answers that at a glance in a way a
/// row of bars does not.
class _SectionsCard extends StatelessWidget {
  const _SectionsCard({required this.sections});

  final List<rust.SiteSection> sections;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (sections.isEmpty) return const SizedBox.shrink();
    final total = sections.fold<int>(0, (sum, s) => sum + s.pages);

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text('Sections', style: theme.textTheme.titleSmall),
          const SizedBox(height: 2),
          Text(
            'Top-level paths, by how many pages they hold',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 12),
          for (final section in sections) ...[
            _SectionRow(section: section, total: total),
            const SizedBox(height: 8),
          ],
        ],
      ),
    );
  }
}

class _SectionRow extends StatelessWidget {
  const _SectionRow({required this.section, required this.total});

  final rust.SiteSection section;
  final int total;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final share = total == 0 ? 0.0 : section.pages / total;
    final troubled = section.issues > 0;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                section.path,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: CrawlStatTheme.mono(context, size: 12),
              ),
            ),
            const SizedBox(width: 8),
            Text(
              '${section.pages}',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurface,
                fontWeight: FontWeight.w600,
              ),
            ),
            if (troubled) ...[
              const SizedBox(width: 6),
              Text(
                '${section.issues} issue${section.issues == 1 ? '' : 's'}',
                style: theme.textTheme.bodySmall?.copyWith(
                  color: CrawlStatTheme.warning,
                ),
              ),
            ],
          ],
        ),
        const SizedBox(height: 5),
        SoftWell(
          radius: 4,
          depth: 0.5,
          padding: const EdgeInsets.all(1),
          child: SizedBox(
            height: 6,
            child: FractionallySizedBox(
              alignment: Alignment.centerLeft,
              widthFactor: share.clamp(0.02, 1.0),
              child: DecoratedBox(
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(4),
                  gradient: glossOf(
                    troubled ? CrawlStatTheme.warning : CrawlStatTheme.accent,
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

String _count(int value) {
  if (value < 1000) return '$value';
  if (value < 1000000) return '${(value / 1000).toStringAsFixed(1)}k';
  return '${(value / 1000000).toStringAsFixed(1)}M';
}
