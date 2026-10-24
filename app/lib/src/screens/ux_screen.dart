import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/project.dart' as rust;
import '../state/projects.dart';
import '../theme/icons.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'shell.dart';

/// Pages worth analysing: the best-connected ones in the crawl.
final uxPagesProvider = FutureProvider.family<List<rust.SiteNode>, String>((
  ref,
  path,
) {
  return rust.uxCandidatePages(dbPath: path, limit: 60);
});

final uxPersonasProvider = FutureProvider<List<rust.UxPersona>>((ref) {
  return rust.uxPersonas();
});

/// Where a page leads a reader, and where it loses them.
///
/// Deliberately a button rather than something that runs on opening the
/// screen: when the crawl did not keep raw HTML the analysis re-fetches the
/// page, and a screen that quietly makes requests the moment you look at it is
/// not what a privacy-first crawler should do.
class UxScreen extends ConsumerStatefulWidget {
  const UxScreen({super.key});

  @override
  ConsumerState<UxScreen> createState() => _UxScreenState();
}

class _UxScreenState extends ConsumerState<UxScreen> {
  String _persona = 'task_oriented';
  rust.SiteNode? _page;
  rust.UxFlow? _flow;
  String? _error;
  bool _running = false;

  Future<void> _run(String dbPath) async {
    final page = _page;
    if (page == null) return;
    setState(() {
      _running = true;
      _error = null;
    });
    try {
      final flow = await rust.uxAnalyse(
        dbPath: dbPath,
        urlId: page.id,
        persona: _persona,
      );
      if (mounted) setState(() => _flow = flow);
    } on Object catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _running = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final path = ref.watch(openProjectProvider);
    if (path == null) return const NoProjectState();

    final pages = ref.watch(uxPagesProvider(path));
    final personas = ref.watch(uxPersonasProvider);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const ScreenHeader(
          title: 'Reader flow',
          subtitle: 'Where a page leads someone — and where it loses them',
        ),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(
              CrawlStatTheme.gutter,
              0,
              CrawlStatTheme.gutter,
              CrawlStatTheme.gutter,
            ),
            child: pages.when(
              loading: () => const Center(child: CircularProgressIndicator()),
              error: (error, _) => EmptyState(
                icon: AppIcon.report,
                title: 'Could not read the crawl',
                message: '$error',
              ),
              data: (candidates) {
                if (candidates.isEmpty) {
                  return const EmptyState(
                    icon: AppIcon.report,
                    title: 'No pages to read',
                    message: 'This crawl holds no fetched HTML pages yet.',
                  );
                }
                _page ??= candidates.first;
                return ListView(
                  children: [
                    _Controls(
                      pages: candidates,
                      selected: _page!,
                      persona: _persona,
                      personas: personas.value ?? const [],
                      running: _running,
                      onPage: (page) => setState(() {
                        _page = page;
                        _flow = null;
                      }),
                      onPersona: (persona) => setState(() {
                        _persona = persona;
                        _flow = null;
                      }),
                      onRun: () => _run(path),
                    ),
                    const SizedBox(height: 16),
                    if (_error != null)
                      InlineMessage(message: _error!, severity: 'error')
                    else if (_flow != null)
                      _Result(flow: _flow!)
                    else
                      const _Explainer(),
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

class _Controls extends StatelessWidget {
  const _Controls({
    required this.pages,
    required this.selected,
    required this.persona,
    required this.personas,
    required this.running,
    required this.onPage,
    required this.onPersona,
    required this.onRun,
  });

  final List<rust.SiteNode> pages;
  final rust.SiteNode selected;
  final String persona;
  final List<rust.UxPersona> personas;
  final bool running;
  final ValueChanged<rust.SiteNode> onPage;
  final ValueChanged<String> onPersona;
  final VoidCallback onRun;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final current = personas.where((p) => p.id == persona).firstOrNull;

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Page', style: theme.textTheme.labelMedium),
          const SizedBox(height: 6),
          SoftWell(
            radius: CrawlStatTheme.controlRadius,
            depth: 0.6,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: DropdownButtonHideUnderline(
              child: DropdownButton<int>(
                value: selected.id,
                isExpanded: true,
                dropdownColor: theme.colorScheme.surfaceContainerLow,
                borderRadius: BorderRadius.circular(CrawlStatTheme.radius),
                items: [
                  for (final page in pages)
                    DropdownMenuItem(
                      value: page.id,
                      child: Text(
                        page.url,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: CrawlStatTheme.mono(context, size: 12.5),
                      ),
                    ),
                ],
                onChanged: (id) {
                  final page = pages.where((p) => p.id == id).firstOrNull;
                  if (page != null) onPage(page);
                },
              ),
            ),
          ),
          const SizedBox(height: 16),
          Text('Reader', style: theme.textTheme.labelMedium),
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final option in personas)
                SoftPanel(
                  radius: 999,
                  depth: 0.5,
                  pressed: option.id == persona,
                  onTap: () => onPersona(option.id),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 10,
                  ),
                  child: Text(
                    option.label,
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: option.id == persona
                          ? CrawlStatTheme.accent
                          : theme.colorScheme.onSurfaceVariant,
                      fontWeight: option.id == persona
                          ? FontWeight.w600
                          : FontWeight.w500,
                    ),
                  ),
                ),
            ],
          ),
          if (current != null) ...[
            const SizedBox(height: 10),
            Text(current.describes, style: theme.textTheme.bodySmall),
          ],
          const SizedBox(height: 18),
          Align(
            alignment: Alignment.centerLeft,
            child: SoftButton(
              icon: AppIcon.visualise,
              onPressed: running ? null : onRun,
              child: Text(running ? 'Reading…' : 'Read this page'),
            ),
          ),
        ],
      ),
    );
  }
}

/// Shown before anything has been analysed: what this is, and what it is not.
class _Explainer extends StatelessWidget {
  const _Explainer();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('What this does', style: theme.textTheme.titleSmall),
          const SizedBox(height: 8),
          Text(
            'It predicts the order a reader attends to things in, how attention '
            'is shared out, and what goes wrong on the way: actions competing '
            'for one decision, a primary action buried past where anyone is '
            'still reading, a page with no way onward.',
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 14),
          Text('What it is not', style: theme.textTheme.titleSmall),
          const SizedBox(height: 8),
          Text(
            'It has not seen the page. CrawlStat has no browser, so there is no '
            'layout, no pixel positions, no contrast and no screenshot — this '
            'reads the markup. That is honest for keyboard and screen-reader '
            'users, whose order really is the document order, and a reasonable '
            'guess for everyone else. A page whose source order fights its '
            'visual order will be described wrongly.\n\n'
            'Treat it as a hypothesis worth testing, not a measurement, and '
            'never as a substitute for watching a real person use the thing.',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

class _Result extends StatelessWidget {
  const _Result({required this.flow});

  final rust.UxFlow flow;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final top = flow.attention.take(12).toList();
    final peak = top.isEmpty ? 1.0 : top.first.share;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Verdict(flow: flow),
        const SizedBox(height: 16),
        SoftPanel(
          padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Text(
                    'Where the attention goes',
                    style: theme.textTheme.titleSmall,
                  ),
                  const Spacer(),
                  Text(
                    '${flow.nodeCount} elements · markup '
                    '${flow.source == 'stored' ? 'from the crawl' : 'fetched now'}',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
              const SizedBox(height: 14),
              for (final item in top) ...[
                _AttentionRow(item: item, peak: peak),
                const SizedBox(height: 10),
              ],
            ],
          ),
        ),
        if (flow.conflicts.isNotEmpty) ...[
          const SizedBox(height: 16),
          _Conflicts(conflicts: flow.conflicts),
        ],
        if (flow.vagueLinks.isNotEmpty) ...[
          const SizedBox(height: 16),
          _VagueLinks(labels: flow.vagueLinks),
        ],
      ],
    );
  }
}

/// The findings, in a sentence each.
class _Verdict extends StatelessWidget {
  const _Verdict({required this.flow});

  final rust.UxFlow flow;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final reachPercent = flow.nodeCount == 0
        ? 0
        : (flow.reach * 100 / flow.nodeCount).round();

    final findings = <(String, String, Color)>[
      (
        'Reach',
        'Gets through $reachPercent% of the page before deciding.',
        CrawlStatTheme.notice,
      ),
      if (flow.stepsToAction == null)
        (
          'No clear action',
          'Nothing on this page reads as the thing it wants you to do.',
          CrawlStatTheme.warning,
        )
      else if (flow.actionOutOfReach)
        (
          'Action out of reach',
          'The primary action is element ${flow.stepsToAction}, past where '
              'this reader stops.',
          CrawlStatTheme.error,
        )
      else
        (
          'Action reachable',
          'The primary action is element ${flow.stepsToAction}, well inside '
              'their patience.',
          CrawlStatTheme.success,
        ),
      if (flow.deadEnd)
        (
          'Dead end',
          'No onward link in the content — navigation does not count.',
          CrawlStatTheme.error,
        ),
      (
        'Coherence ${(flow.coherence * 100).round()}%',
        flow.coherence > 0.35
            ? 'Attention lands somewhere in particular.'
            : 'Attention is spread thin — the page does not lead anywhere.',
        flow.coherence > 0.35 ? CrawlStatTheme.success : CrawlStatTheme.warning,
      ),
      if (flow.decisions.isNotEmpty)
        (
          '${flow.decisions.length} decision point'
              '${flow.decisions.length == 1 ? '' : 's'}',
          'Places where several options appear at once.',
          CrawlStatTheme.notice,
        ),
    ];

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final (index, finding) in findings.indexed) ...[
            if (index > 0) const SizedBox(height: 12),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Container(
                  width: 8,
                  height: 8,
                  margin: const EdgeInsets.only(top: 6, right: 12),
                  decoration: BoxDecoration(
                    gradient: glossOf(finding.$3),
                    shape: BoxShape.circle,
                  ),
                ),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(finding.$1, style: theme.textTheme.titleSmall),
                      const SizedBox(height: 2),
                      Text(finding.$2, style: theme.textTheme.bodySmall),
                    ],
                  ),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

class _AttentionRow extends StatelessWidget {
  const _AttentionRow({required this.item, required this.peak});

  final rust.UxAttention item;
  final double peak;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final fraction = peak <= 0 ? 0.0 : (item.share / peak).clamp(0.0, 1.0);
    final colour = switch (item.kind) {
      'action' => CrawlStatTheme.accent,
      final k when k.startsWith('h') => CrawlStatTheme.success,
      'link' => CrawlStatTheme.warning,
      _ => CrawlStatTheme.notice,
    };

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 52,
          child: Text(
            '${(item.share * 100).toStringAsFixed(1)}%',
            textAlign: TextAlign.right,
            style: CrawlStatTheme.mono(
              context,
              size: 12.5,
              color: theme.colorScheme.onSurface,
            ),
          ),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      item.label.isEmpty ? '(unlabelled)' : item.label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        fontStyle: item.label.isEmpty
                            ? FontStyle.italic
                            : FontStyle.normal,
                        color: item.label.isEmpty
                            ? theme.colorScheme.onSurfaceVariant
                            : null,
                      ),
                    ),
                  ),
                  const SizedBox(width: 10),
                  Text(
                    '${item.kind} · ${item.region}',
                    style: theme.textTheme.bodySmall,
                  ),
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
                    widthFactor: fraction.clamp(0.015, 1.0),
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(4),
                        gradient: glossOf(colour),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _Conflicts extends StatelessWidget {
  const _Conflicts({required this.conflicts});

  final List<rust.UxConflict> conflicts;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Icon(AppIcon.warning, size: 16, color: CrawlStatTheme.warning),
              const SizedBox(width: 8),
              Text('Competing actions', style: theme.textTheme.titleSmall),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            'Asked to choose between these at the same moment.',
            style: theme.textTheme.bodySmall,
          ),
          for (final conflict in conflicts) ...[
            const SizedBox(height: 12),
            SoftWell(
              radius: CrawlStatTheme.controlRadius,
              depth: 0.6,
              padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '${(conflict.share * 100).toStringAsFixed(0)}% of attention '
                    'split ${conflict.labels.length} ways',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: CrawlStatTheme.warning,
                    ),
                  ),
                  const SizedBox(height: 6),
                  for (final label in conflict.labels)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: Text(
                        label.isEmpty ? '(unlabelled)' : label,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: CrawlStatTheme.mono(context, size: 12),
                      ),
                    ),
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _VagueLinks extends StatelessWidget {
  const _VagueLinks({required this.labels});

  final List<String> labels;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Links that say nothing', style: theme.textTheme.titleSmall),
          const SizedBox(height: 4),
          Text(
            'Out of context — in a screen reader\'s link list, or scanned down '
            'a page — these give the reader nothing to go on.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final label in labels.take(12))
                SoftWell(
                  radius: 999,
                  depth: 0.5,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 7,
                  ),
                  child: Text(
                    '“$label”',
                    style: CrawlStatTheme.mono(context, size: 12),
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }
}
