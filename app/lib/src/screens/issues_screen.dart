import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../theme/icons.dart';
import '../router.dart';
import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';
import '../state/issues.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../widgets/export.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'shell.dart';
import '../theme/soft.dart';

/// The audit results: what is wrong, how bad it is, and how to fix it.
///
/// A list on the left, most urgent first, and the selected issue on the
/// right — its explanation, why it ranks where it does, and the pages it
/// touches. The fix text is the same text the documentation site publishes,
/// so what the app tells you and what the docs tell you can never drift
/// apart.
class IssuesScreen extends ConsumerStatefulWidget {
  const IssuesScreen({super.key, this.initialIssue});

  /// The issue to open on, from `?issue=` — the Plan and the Overview link
  /// straight to one.
  final String? initialIssue;

  @override
  ConsumerState<IssuesScreen> createState() => _IssuesScreenState();
}

class _IssuesScreenState extends ConsumerState<IssuesScreen> {
  late String? _selectedId = widget.initialIssue;
  bool _auditing = false;

  @override
  void didUpdateWidget(IssuesScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    // The route is reused when only the query changes, so a second link
    // arrives here rather than in a new state.
    if (widget.initialIssue != oldWidget.initialIssue &&
        widget.initialIssue != null) {
      _selectedId = widget.initialIssue;
    }
  }

  Future<void> _reaudit(String path) async {
    setState(() => _auditing = true);
    try {
      await rust.runAudit(dbPath: path, thresholdsJson: null);
      ref.invalidate(auditReportProvider(path));
      ref.invalidate(actionPlanProvider(path));
    } on Object catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('Audit failed: $error')));
      }
    } finally {
      if (mounted) setState(() => _auditing = false);
    }
  }

  Future<void> _exportIssues(String path) => exportCsv(
    context,
    ref,
    name: 'issues',
    write: (file) => rust.exportIssuesCsv(dbPath: path, outPath: file),
    done: (rows) => 'Exported ${formatCount(rows)} issue types',
  );

  @override
  Widget build(BuildContext context) {
    final path = ref.watch(openProjectProvider);
    if (path == null) {
      return const Column(
        children: [
          ScreenHeader(title: 'Issues'),
          Expanded(child: NoProjectState()),
        ],
      );
    }

    final report = ref.watch(auditReportProvider(path));
    // Optional: without it the list still sorts by severity and size.
    final plan = ref.watch(actionPlanProvider(path)).value;

    return Column(
      children: [
        ScreenHeader(
          title: 'Issues',
          subtitle: report.maybeWhen(
            data: (data) => data.issues.isEmpty
                ? 'Nothing found'
                : '${data.errors} error${data.errors == 1 ? '' : 's'}, '
                      '${data.warnings} warning${data.warnings == 1 ? '' : 's'}, '
                      '${data.notices} notice${data.notices == 1 ? '' : 's'}',
            orElse: () => null,
          ),
          actions: [
            OutlinedButton.icon(
              onPressed: _auditing ? null : () => _reaudit(path),
              icon: _auditing
                  ? const SizedBox(
                      width: 12,
                      height: 12,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(AppIcon.refresh, size: 16),
              label: Text(_auditing ? 'Auditing…' : 'Re-run audits'),
            ),
            OutlinedButton.icon(
              onPressed: () => _exportIssues(path),
              icon: const Icon(AppIcon.download, size: 16),
              label: const Text('Export CSV'),
            ),
          ],
        ),
        Expanded(
          child: report.when(
            loading: () => const Center(
              child: SizedBox(
                width: 20,
                height: 20,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            ),
            error: (error, _) => EmptyState(
              icon: AppIcon.error,
              title: 'Could not read the audits',
              message: '$error',
              action: OutlinedButton(
                onPressed: () => _reaudit(path),
                child: const Text('Run the audits'),
              ),
            ),
            data: (data) {
              if (data.issues.isEmpty) {
                return EmptyState(
                  icon: AppIcon.indexable,
                  title: 'No issues found',
                  message:
                      'Either this site is in good shape, or the audits have not '
                      'run yet for this crawl.',
                  action: OutlinedButton(
                    onPressed: () => _reaudit(path),
                    child: const Text('Run the audits'),
                  ),
                );
              }

              final ranked = rankIssues(
                data.issues,
                plan: plan,
                totalPages: data.indexablePages + data.nonIndexablePages,
              );
              final selected = ranked.firstWhere(
                (issue) => issue.id == _selectedId,
                orElse: () => ranked.first,
              );

              return Padding(
                padding: const EdgeInsets.fromLTRB(
                  CrawlStatTheme.gutter,
                  0,
                  CrawlStatTheme.gutter,
                  CrawlStatTheme.gutter,
                ),
                child: LayoutBuilder(
                  builder: (context, constraints) => Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      SizedBox(
                        width: constraints.maxWidth < 860 ? 280 : 380,
                        child: SoftPanel(
                          child: _IssueList(
                            issues: ranked,
                            selectedId: selected.id,
                            onSelect: (id) => setState(() => _selectedId = id),
                          ),
                        ),
                      ),
                      const SizedBox(width: 14),
                      Expanded(
                        child: SoftPanel(
                          child: _IssueDetail(
                            // A fresh scroll position for each issue.
                            key: ValueKey(selected.id),
                            ranked: selected,
                            path: path,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              );
            },
          ),
        ),
      ],
    );
  }
}

class _IssueList extends StatelessWidget {
  const _IssueList({
    required this.issues,
    required this.selectedId,
    required this.onSelect,
  });

  /// Already in order: severity, then impact.
  final List<RankedIssue> issues;
  final String selectedId;
  final void Function(String) onSelect;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final groups = <String, List<RankedIssue>>{};
    for (final issue in issues) {
      groups.putIfAbsent(issue.severity, () => []).add(issue);
    }
    final scored = issues.any((issue) => issue.impact != null);

    return ListView(
      padding: const EdgeInsets.fromLTRB(8, 4, 8, 12),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(10, 12, 10, 0),
          child: Text(
            scored
                ? 'Most urgent first: by severity, then by what fixing it '
                      'is worth'
                : 'Most urgent first: by severity, then by pages affected',
            style: theme.textTheme.bodySmall,
          ),
        ),
        for (final MapEntry(key: severity, value: group) in groups.entries) ...[
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 16, 10, 6),
            child: Row(
              children: [
                SeverityDot(severity),
                const SizedBox(width: 8),
                Text(
                  '${severity.toUpperCase()}S',
                  style: theme.textTheme.labelMedium,
                ),
                const Spacer(),
                TonePill(
                  '${group.length}',
                  color: CrawlStatTheme.severityColor(context, severity),
                ),
              ],
            ),
          ),
          for (final issue in group)
            _IssueRow(
              ranked: issue,
              selected: issue.id == selectedId,
              onTap: () => onSelect(issue.id),
            ),
        ],
      ],
    );
  }
}

class _IssueRow extends StatelessWidget {
  const _IssueRow({
    required this.ranked,
    required this.selected,
    required this.onTap,
  });

  final RankedIssue ranked;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final radius = BorderRadius.circular(CrawlStatTheme.controlRadius);
    final tone = CrawlStatTheme.severityColor(context, ranked.severity);
    final share = ranked.share;

    return Semantics(
      button: true,
      selected: selected,
      child: InkWell(
        onTap: onTap,
        borderRadius: radius,
        child: Container(
          constraints: const BoxConstraints(minHeight: 38),
          padding: const EdgeInsets.fromLTRB(10, 9, 10, 10),
          decoration: BoxDecoration(
            color: selected
                ? CrawlStatTheme.palette(context).accent.withValues(alpha: 0.14)
                : null,
            borderRadius: radius,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: Text(
                      ranked.issue.title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        fontWeight: selected ? FontWeight.w600 : null,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Text(
                    formatCount(ranked.pages),
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: tone,
                      fontWeight: FontWeight.w600,
                      fontFeatures: const [FontFeature.tabularFigures()],
                    ),
                  ),
                ],
              ),
              if (share != null || ranked.isTemplateFix) ...[
                const SizedBox(height: 7),
                Row(
                  children: [
                    if (share != null) ...[
                      ShareBar(share: share, color: tone, width: 56),
                      const SizedBox(width: 8),
                      Flexible(
                        child: Text(
                          '${formatShare(share)} of pages',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall,
                        ),
                      ),
                    ],
                    if (ranked.isTemplateFix) ...[
                      const SizedBox(width: 8),
                      const _TemplateBadge(),
                    ],
                  ],
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// How much of the site an issue covers, as a thin bar.
class ShareBar extends StatelessWidget {
  const ShareBar({
    super.key,
    required this.share,
    required this.color,
    this.width = 64,
  });

  /// 0..1.
  final double share;
  final Color color;
  final double width;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: width,
      height: 4,
      child: ClipRRect(
        borderRadius: BorderRadius.circular(2),
        child: Stack(
          fit: StackFit.expand,
          children: [
            ColoredBox(color: color.withValues(alpha: 0.16)),
            FractionallySizedBox(
              alignment: Alignment.centerLeft,
              // Something that is there should never draw as nothing.
              widthFactor: share <= 0 ? 0 : share.clamp(0.04, 1.0),
              child: ColoredBox(color: color),
            ),
          ],
        ),
      ),
    );
  }
}

/// One change fixes every page — the cheapest kind of win.
class _TemplateBadge extends StatelessWidget {
  const _TemplateBadge();

  @override
  Widget build(BuildContext context) {
    final accent = CrawlStatTheme.palette(context).accent;
    return Tooltip(
      message: 'The same fix repeats across pages — likely one template change',
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
        decoration: BoxDecoration(
          color: accent.withValues(alpha: 0.14),
          borderRadius: BorderRadius.circular(5),
        ),
        child: Text(
          'Template',
          style: Theme.of(context).textTheme.labelSmall
              ?.copyWith(color: accent, fontWeight: FontWeight.w600),
        ),
      ),
    );
  }
}

class _IssueDetail extends ConsumerWidget {
  const _IssueDetail({super.key, required this.ranked, required this.path});

  final RankedIssue ranked;
  final String path;

  Issue get issue => ranked.issue;

  Future<void> _exportAffected(BuildContext context, WidgetRef ref) =>
      exportCsv(
        context,
        ref,
        name: issue.id,
        write: (file) => rust.exportIssueUrlsCsv(
          dbPath: path,
          issueId: issue.id,
          outPath: file,
        ),
        done: (rows) => 'Exported ${formatCount(rows)} affected URLs',
      );

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);

    return ListView(
      padding: const EdgeInsets.fromLTRB(28, 24, 28, 30),
      children: [
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            TonePill(
              issue.severity[0].toUpperCase() + issue.severity.substring(1),
              color: CrawlStatTheme.severityColor(context, issue.severity),
              dot: true,
            ),
            TonePill(
              '${formatCount(ranked.pages)} URL${ranked.pages == 1 ? '' : 's'} '
              'affected',
              color: palette.notice,
            ),
            if (ranked.isTemplateFix)
              TonePill('Template fix', color: palette.accent),
          ],
        ),
        const SizedBox(height: 14),
        Text(issue.title, style: theme.textTheme.titleLarge),
        const SizedBox(height: 14),
        Text(
          issue.description,
          style: theme.textTheme.bodyMedium?.copyWith(height: 1.5),
        ),
        const SizedBox(height: 20),
        _WhyItRanks(ranked: ranked),
        const SizedBox(height: 22),
        Text('HOW TO FIX', style: theme.textTheme.labelMedium),
        const SizedBox(height: 8),
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: palette.accent.withValues(alpha: 0.07),
            border: Border.all(color: palette.accent.withValues(alpha: 0.25)),
            borderRadius: BorderRadius.circular(
              CrawlStatTheme.controlRadius + 2,
            ),
          ),
          child: Text(
            issue.howToFix,
            style: theme.textTheme.bodyMedium?.copyWith(height: 1.55),
          ),
        ),
        const SizedBox(height: 20),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            FilledButton.icon(
              onPressed: () => _showInPages(context, ref, issue.id),
              icon: const Icon(AppIcon.filter, size: 15),
              label: Text('Show all ${formatCount(ranked.pages)} in Pages'),
            ),
            OutlinedButton.icon(
              onPressed: () => _exportAffected(context, ref),
              icon: const Icon(AppIcon.download, size: 14),
              label: const Text('Export affected'),
            ),
            OutlinedButton.icon(
              onPressed: () => Clipboard.setData(
                ClipboardData(
                  text: 'https://crawlstat.com/docs/audits/${issue.docsSlug}',
                ),
              ),
              icon: const Icon(AppIcon.link, size: 14),
              label: const Text('Copy docs link'),
            ),
          ],
        ),
        const SizedBox(height: 28),
        Row(
          crossAxisAlignment: CrossAxisAlignment.baseline,
          textBaseline: TextBaseline.alphabetic,
          children: [
            Text('AFFECTED PAGES', style: theme.textTheme.labelMedium),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                'Best-linked first — those matter most',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.bodySmall,
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        _AffectedPages(ranked: ranked, path: path),
      ],
    );
  }
}

/// Filter the Pages table to one issue's URLs, optionally opening one.
void _showInPages(
  BuildContext context,
  WidgetRef ref,
  String issueId, {
  int? urlId,
}) {
  ref.read(urlTableProvider.notifier).setIssue(issueId);
  if (urlId != null) ref.read(selectedUrlProvider.notifier).select(urlId);
  context.go('${Routes.results}?tab=internal');
}

/// The plan's numbers behind the ranking, in words.
class _WhyItRanks extends StatelessWidget {
  const _WhyItRanks({required this.ranked});

  final RankedIssue ranked;

  @override
  Widget build(BuildContext context) {
    final tone = CrawlStatTheme.severityColor(context, ranked.severity);
    final share = ranked.share;
    final impact = ranked.impact;
    final plan = ranked.plan;

    return Wrap(
      spacing: 12,
      runSpacing: 12,
      children: [
        if (impact != null)
          _Fact(
            label: 'Impact',
            value: '${impact.round()}',
            unit: '/ 100',
            hint: 'What fixing it is worth, against everything else here',
            bar: ShareBar(share: impact / 100, color: tone, width: 120),
          ),
        if (share != null)
          _Fact(
            label: 'Reach',
            value: formatShare(share),
            unit: 'of pages',
            hint: plan == null
                ? '${formatCount(ranked.pages)} of '
                      '${formatCount(ranked.totalPages ?? 0)} pages'
                : '${formatCount(ranked.pages)} pages, '
                      '${formatShare(plan.reach)} of internal link value',
            bar: ShareBar(share: share, color: tone, width: 120),
          ),
        if (plan != null)
          _Fact(
            label: 'Effort',
            value: formatCount(plan.distinctFixes),
            unit: plan.distinctFixes == 1 ? 'change' : 'changes',
            hint: effortLine(plan),
          ),
      ],
    );
  }
}

/// What the fix takes, as a sentence.
String effortLine(rust.PlanItem plan) {
  final pages = plan.pages.toInt();
  final fixes = plan.distinctFixes.toInt();
  if (pages == 0) return 'No pages affected';
  if (fixes <= 1) {
    return pages == 1 ? 'One fix on one page' : 'One change fixes all $pages';
  }
  if (plan.leverage >= 2) {
    return 'About ${plan.leverage.toStringAsFixed(0)} pages per change';
  }
  return 'Each page needs its own fix';
}

class _Fact extends StatelessWidget {
  const _Fact({
    required this.label,
    required this.value,
    required this.unit,
    required this.hint,
    this.bar,
  });

  final String label;
  final String value;
  final String unit;
  final String hint;
  final Widget? bar;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      width: 200,
      child: SoftWell(
        radius: CrawlStatTheme.controlRadius,
        depth: 0.6,
        padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(label.toUpperCase(), style: theme.textTheme.labelSmall),
            const SizedBox(height: 6),
            Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: value,
                    style: theme.textTheme.titleLarge?.copyWith(
                      fontFeatures: const [FontFeature.tabularFigures()],
                    ),
                  ),
                  TextSpan(text: ' $unit', style: theme.textTheme.bodySmall),
                ],
              ),
            ),
            if (bar != null) ...[const SizedBox(height: 8), bar!],
            const SizedBox(height: 8),
            Text(hint, maxLines: 2, style: theme.textTheme.bodySmall),
          ],
        ),
      ),
    );
  }
}

/// The first pages an issue touches, each with why it was flagged.
class _AffectedPages extends ConsumerWidget {
  const _AffectedPages({required this.ranked, required this.path});

  final RankedIssue ranked;
  final String path;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final pages = ref.watch(
      issueUrlsProvider((path: path, issueId: ranked.id)),
    );

    return pages.when(
      loading: () => const Padding(
        padding: EdgeInsets.all(20),
        child: Center(
          child: SizedBox(
            width: 18,
            height: 18,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ),
      ),
      error: (error, _) => Text(
        'Could not list the pages: $error',
        style: theme.textTheme.bodySmall,
      ),
      data: (pages) {
        if (pages.isEmpty) {
          return Text(
            'No pages recorded for this issue.',
            style: theme.textTheme.bodySmall,
          );
        }
        final rest = ranked.pages - pages.length;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            for (final page in pages)
              _AffectedRow(page: page, issueId: ranked.id),
            if (rest > 0)
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton(
                  onPressed: () => _showInPages(context, ref, ranked.id),
                  child: Text(
                    'And ${formatCount(rest)} more — show all in Pages',
                  ),
                ),
              ),
          ],
        );
      },
    );
  }
}

class _AffectedRow extends ConsumerWidget {
  const _AffectedRow({required this.page, required this.issueId});

  final rust.AffectedPage page;
  final String issueId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final detail = page.detail;
    return InkWell(
      borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius),
      onTap: () =>
          _showInPages(context, ref, issueId, urlId: page.urlId.toInt()),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              width: 44,
              child: Align(
                alignment: Alignment.centerLeft,
                child: StatusChip(page.status?.toInt()),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    page.url,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: CrawlStatTheme.mono(context, size: 12.5),
                  ),
                  if (detail != null) ...[
                    const SizedBox(height: 3),
                    Text(
                      detail,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall,
                    ),
                  ],
                ],
              ),
            ),
            const SizedBox(width: 6),
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
