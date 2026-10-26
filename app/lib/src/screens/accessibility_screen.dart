import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/accessibility.dart' as rust;
import '../state/accessibility.dart';
import '../state/projects.dart';
import '../theme/icons.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/export.dart';
import 'shell.dart';

/// Web accessibility compliance: where the site stands against WCAG, what to
/// fix first, and the checklist a person works through to finish the audit.
///
/// The crawler can prove some failures from markup alone. It cannot prove a
/// pass — contrast, keyboard use and screen reader behaviour need a person —
/// so every criterion it cannot settle stays visibly open until someone tests
/// it and records the result here.
class AccessibilityScreen extends ConsumerWidget {
  const AccessibilityScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final path = ref.watch(openProjectProvider);
    if (path == null) return const NoProjectState();

    final standard = ref.watch(accessibilityStandardProvider);
    final report = ref.watch(accessibilityReportProvider((path, standard)));
    final data = report.value;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ScreenHeader(
          title: 'Accessibility',
          subtitle: data == null
              ? 'WCAG conformance for ADA, Section 508 and EAA compliance'
              : '${data.standard.label} · '
                    '${formatCount(data.coverage.pagesChecked)} pages checked',
          actions: [
            _StandardPicker(
              value: standard,
              onChanged: ref
                  .read(accessibilityStandardProvider.notifier)
                  .select,
            ),
            SoftButton(
              icon: AppIcon.save,
              onPressed: data == null
                  ? null
                  : () => _export(context, ref, path, standard),
              child: const Text('Export report'),
            ),
          ],
        ),
        Expanded(
          child: report.when(
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (error, _) => EmptyState(
              icon: AppIcon.report,
              title: 'Could not build the report',
              message: '$error',
            ),
            data: (data) => AccessibilityReportView(report: data, path: path),
          ),
        ),
      ],
    );
  }

  Future<void> _export(
    BuildContext context,
    WidgetRef ref,
    String path,
    String standard,
  ) async {
    final site = siteOfCrawl(path);
    final contents = await rust.accessibilityReportMarkdown(
      dbPath: path,
      standard: standard,
      site: site,
    );
    if (!context.mounted) return;
    await exportText(
      context,
      ref,
      name: 'accessibility-${site.replaceAll('.', '-')}',
      extension: 'md',
      contents: contents,
      done: 'Conformance report saved as Markdown',
    );
  }
}

class _StandardPicker extends StatelessWidget {
  const _StandardPicker({required this.value, required this.onChanged});

  final String value;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message:
          'WCAG 2.1 AA: ADA, EU Accessibility Act. '
          'WCAG 2.0 AA: Section 508. WCAG 2.2 AA: the current W3C standard.',
      child: SegmentedButton<String>(
        segments: const [
          ButtonSegment(value: 'wcag20aa', label: Text('2.0 AA')),
          ButtonSegment(value: 'wcag21aa', label: Text('2.1 AA')),
          ButtonSegment(value: 'wcag22aa', label: Text('2.2 AA')),
        ],
        selected: {value},
        showSelectedIcon: false,
        style: const ButtonStyle(visualDensity: VisualDensity.compact),
        onSelectionChanged: (selection) => onChanged(selection.first),
      ),
    );
  }
}

/// Everything below the header. Public so it can be tested with a report
/// built by hand.
class AccessibilityReportView extends ConsumerStatefulWidget {
  const AccessibilityReportView({
    super.key,
    required this.report,
    required this.path,
  });

  final rust.A11yReport report;
  final String path;

  @override
  ConsumerState<AccessibilityReportView> createState() =>
      _AccessibilityReportViewState();
}

class _AccessibilityReportViewState
    extends ConsumerState<AccessibilityReportView> {
  /// Which criteria the checklist shows: `open`, `fail`, `done` or `all`.
  String _filter = 'open';
  bool _allRules = false;

  static const _ruleLimit = 6;

  bool _matches(rust.A11yCriterion criterion) => switch (_filter) {
    'fail' => criterion.status == 'fail',
    'done' => criterion.status == 'pass' || criterion.status == 'na',
    'open' => criterion.status != 'pass' && criterion.status != 'na',
    _ => true,
  };

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final report = widget.report;
    final rules = _allRules
        ? report.rules
        : report.rules.take(_ruleLimit).toList();
    final shown = report.criteria.where(_matches).toList();
    final settled = report.counts.pass + report.counts.notApplicable;

    return ListView(
      padding: const EdgeInsets.fromLTRB(
        CrawlStatTheme.gutter,
        0,
        CrawlStatTheme.gutter,
        CrawlStatTheme.gutter,
      ),
      children: [
        _VerdictPanel(report: report),
        ..._coverageMessages(report.coverage),
        const SizedBox(height: 16),
        const _CheckUrlPanel(),
        const SizedBox(height: 28),
        if (report.coverage.pagesChecked > 0) ...[
          _SectionTitle(
            title: 'Fix first',
            subtitle: report.rules.isEmpty
                ? 'The crawler found no barriers it can detect from markup.'
                : 'Ranked by how badly each barrier blocks people, the WCAG '
                      'level, and how many pages it is on.',
          ),
          const SizedBox(height: 12),
          if (report.rules.isEmpty)
            SoftPanel(
              padding: EdgeInsets.all(18),
              child: Row(
                children: [
                  IconBadge(
                    AppIcon.ok,
                    color: CrawlStatTheme.palette(context).success,
                  ),
                  SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      'No automated failures. That covers only part of WCAG — '
                      'work through the checklist below to finish the audit.',
                    ),
                  ),
                ],
              ),
            ),
          for (final (index, rule) in rules.indexed) ...[
            _RuleCard(
              key: ValueKey(rule.info.id),
              rule: rule,
              rank: index + 1,
              path: widget.path,
              pagesChecked: report.coverage.pagesChecked,
            ),
            const SizedBox(height: 12),
          ],
          if (report.rules.length > _ruleLimit)
            Align(
              alignment: Alignment.centerLeft,
              child: SoftButton(
                icon: _allRules ? AppIcon.chevronUp : AppIcon.chevronDown,
                onPressed: () => setState(() => _allRules = !_allRules),
                child: Text(
                  _allRules
                      ? 'Show the top $_ruleLimit'
                      : 'Show all ${report.rules.length} issues',
                ),
              ),
            ),
          const SizedBox(height: 28),
        ],
        _SectionTitle(
          title: '${report.standard.label} checklist',
          subtitle:
              '$settled of ${report.criteria.length} criteria signed off. '
              'Test each open one by hand and record the result — reviews are '
              'saved with this crawl and go into the exported report.',
          trailing: SegmentedButton<String>(
            segments: [
              ButtonSegment(
                value: 'open',
                label: Text('Open ${report.criteria.length - settled}'),
              ),
              ButtonSegment(
                value: 'fail',
                label: Text('Failing ${report.counts.fail}'),
              ),
              ButtonSegment(value: 'done', label: Text('Done $settled')),
              ButtonSegment(
                value: 'all',
                label: Text('All ${report.criteria.length}'),
              ),
            ],
            selected: {_filter},
            showSelectedIcon: false,
            style: const ButtonStyle(visualDensity: VisualDensity.compact),
            onSelectionChanged: (selection) =>
                setState(() => _filter = selection.first),
          ),
        ),
        const SizedBox(height: 12),
        if (shown.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 24),
            child: Text(
              _filter == 'fail'
                  ? 'Nothing is failing.'
                  : _filter == 'done'
                  ? 'Nothing signed off yet.'
                  : 'Every criterion is signed off.',
              textAlign: TextAlign.center,
              style: theme.textTheme.bodySmall,
            ),
          ),
        for (final principle in _principles)
          if (shown.any((c) => c.principle == principle)) ...[
            Padding(
              padding: const EdgeInsets.fromLTRB(4, 10, 4, 8),
              child: Row(
                children: [
                  Text(
                    principle.toUpperCase(),
                    style: theme.textTheme.labelMedium,
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text(
                      _principleSummary[principle] ?? '',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall,
                    ),
                  ),
                ],
              ),
            ),
            for (final criterion in shown.where(
              (c) => c.principle == principle,
            )) ...[
              _CriterionTile(
                key: ValueKey(criterion.id),
                criterion: criterion,
                report: report,
                path: widget.path,
              ),
              const SizedBox(height: 8),
            ],
          ],
        const SizedBox(height: 28),
        _LawPanel(standard: report.standard),
      ],
    );
  }

  List<Widget> _coverageMessages(rust.A11yCoverage coverage) => [
    if (coverage.pagesUnchecked > 0) ...[
      const SizedBox(height: 12),
      InlineMessage(
        severity: 'notice',
        message:
            '${formatCount(coverage.pagesUnchecked)} pages were crawled before '
            'the accessibility checks existed. Re-crawl to include them.',
      ),
    ],
    if (coverage.thinPages > 0) ...[
      const SizedBox(height: 12),
      InlineMessage(
        severity: 'warning',
        message:
            '${formatCount(coverage.thinPages)} pages have almost no text, so '
            'they are probably built by JavaScript and the crawler saw only the '
            'loading shell${coverage.thinExamples.isEmpty ? '' : ' (e.g. ${coverage.thinExamples.first})'}. '
            'Test those in a browser — the results for them are incomplete.',
      ),
    ],
  ];
}

const _principles = ['Perceivable', 'Operable', 'Understandable', 'Robust'];

const _principleSummary = {
  'Perceivable': 'People can see or hear the content, whatever their senses.',
  'Operable': 'Every control works, whatever people use to control it.',
  'Understandable': 'The content and the interface make sense.',
  'Robust': 'Assistive technology can read the page reliably.',
};

// ─── the verdict ─────────────────────────────────────────────────────────

class _VerdictPanel extends StatelessWidget {
  const _VerdictPanel({required this.report});

  final rust.A11yReport report;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final counts = report.counts;
    final tone = switch (report.verdict) {
      'does_not_conform' => CrawlStatTheme.palette(context).error,
      'incomplete' => CrawlStatTheme.palette(context).warning,
      'conforms' => CrawlStatTheme.palette(context).success,
      _ => CrawlStatTheme.palette(context).notice,
    };
    final pill = switch (report.verdict) {
      'does_not_conform' => 'Not compliant',
      'incomplete' => 'Audit in progress',
      'conforms' => 'Conforms',
      _ => 'Not checked',
    };
    final segments = [
      ('fail', counts.fail),
      ('review', counts.review),
      ('checks_passed', counts.checksPassed),
      ('untested', counts.untested),
      ('pass', counts.pass),
      ('na', counts.notApplicable),
    ];

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(22, 18, 22, 20),
      glow: report.verdict == 'does_not_conform' ? tone : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              IconBadge(
                report.verdict == 'conforms' ? AppIcon.ok : AppIcon.warning,
                color: tone,
                size: 40,
              ),
              const SizedBox(width: 14),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Wrap(
                      spacing: 10,
                      runSpacing: 6,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        Text(
                          report.headline,
                          style: theme.textTheme.titleMedium,
                        ),
                        TonePill(pill, color: tone, dot: true),
                      ],
                    ),
                    const SizedBox(height: 6),
                    Text(report.detail, style: theme.textTheme.bodyMedium),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: 18),
          ClipRRect(
            borderRadius: BorderRadius.circular(6),
            child: SizedBox(
              height: 10,
              child: Row(
                children: [
                  for (final (status, count) in segments)
                    if (count > 0)
                      Expanded(
                        flex: count,
                        child: Tooltip(
                          message: '${_statusLabel(status)}: $count',
                          child: ColoredBox(
                            color: _statusColour(context, status),
                          ),
                        ),
                      ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 16,
            runSpacing: 6,
            children: [
              for (final (status, count) in segments)
                if (count > 0)
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        width: 8,
                        height: 8,
                        decoration: BoxDecoration(
                          color: _statusColour(context, status),
                          shape: BoxShape.circle,
                        ),
                      ),
                      const SizedBox(width: 6),
                      Text(
                        '$count ${_statusLabel(status).toLowerCase()}',
                        style: theme.textTheme.bodySmall,
                      ),
                    ],
                  ),
            ],
          ),
        ],
      ),
    );
  }
}

class _SectionTitle extends StatelessWidget {
  const _SectionTitle({
    required this.title,
    required this.subtitle,
    this.trailing,
  });

  final String title;
  final String subtitle;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 4),
      child: Wrap(
        spacing: 16,
        runSpacing: 10,
        crossAxisAlignment: WrapCrossAlignment.end,
        alignment: WrapAlignment.spaceBetween,
        children: [
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 620),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: theme.textTheme.titleMedium),
                const SizedBox(height: 4),
                Text(subtitle, style: theme.textTheme.bodySmall),
              ],
            ),
          ),
          ?trailing,
        ],
      ),
    );
  }
}

// ─── fix first ───────────────────────────────────────────────────────────

class _RuleCard extends ConsumerStatefulWidget {
  const _RuleCard({
    super.key,
    required this.rule,
    required this.rank,
    required this.path,
    required this.pagesChecked,
  });

  final rust.A11yRule rule;
  final int rank;
  final String path;
  final int pagesChecked;

  @override
  ConsumerState<_RuleCard> createState() => _RuleCardState();
}

class _RuleCardState extends ConsumerState<_RuleCard> {
  bool _open = false;
  bool _showPages = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final rule = widget.rule;
    final info = rule.info;
    final tone = _impactColour(context, info.impact);

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 16),
      onTap: () => setState(() => _open = !_open),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _RankBadge(rank: widget.rank, tone: tone),
              const SizedBox(width: 14),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(info.title, style: theme.textTheme.titleSmall),
                    const SizedBox(height: 6),
                    Text(
                      _ruleHeadline(rule, widget.pagesChecked),
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurface,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      'Affects: ${info.affects}',
                      style: theme.textTheme.bodySmall,
                    ),
                    const SizedBox(height: 10),
                    Wrap(
                      spacing: 8,
                      runSpacing: 6,
                      children: [
                        TonePill(_capitalise(info.impact), color: tone),
                        _CertaintyPill(certainty: info.certainty),
                        if (rule.criteria.isEmpty)
                          TonePill(
                            'Best practice',
                            color: CrawlStatTheme.palette(context).notice,
                          )
                        else
                          TonePill(
                            'WCAG ${rule.criteria.join(', ')} · ${rule.level}',
                            color: CrawlStatTheme.palette(context).notice,
                          ),
                        if (rule.templateFix)
                          TonePill(
                            'One template',
                            color: CrawlStatTheme.palette(context).success,
                          ),
                      ],
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
            const SizedBox(height: 16),
            _RuleExplainer(info: info),
            if (rule.groups.isNotEmpty) ...[
              const SizedBox(height: 16),
              Text(
                rule.groups.length == 1
                    ? 'Where it is'
                    : 'Where it is — grouped by the markup at fault',
                style: theme.textTheme.labelMedium,
              ),
              const SizedBox(height: 8),
              for (final group in rule.groups) _GroupRow(group: group),
            ],
            const SizedBox(height: 8),
            Align(
              alignment: Alignment.centerLeft,
              child: SoftButton(
                icon: _showPages ? AppIcon.chevronUp : AppIcon.link,
                onPressed: () => setState(() => _showPages = !_showPages),
                child: Text(
                  _showPages
                      ? 'Hide pages'
                      : 'All ${formatCount(rule.pages)} affected pages',
                ),
              ),
            ),
            if (_showPages) ...[
              const SizedBox(height: 10),
              _RulePages(path: widget.path, rule: info.id),
            ],
          ],
        ],
      ),
    );
  }
}

String _ruleHeadline(rust.A11yRule rule, int pagesChecked) {
  final pages = '${formatCount(rule.pages)} page${rule.pages == 1 ? '' : 's'}';
  final share = pagesChecked == 0
      ? ''
      : ' (${(rule.share * 100).toStringAsFixed(rule.share < 0.1 ? 1 : 0)}% of the site)';
  if (rule.templateFix && rule.groups.isNotEmpty) {
    final group = rule.groups.first;
    return 'The same markup is on ${formatCount(group.pages)} of $pages$share — '
        'fix it once in the shared template.';
  }
  return '$pages$share, ${formatCount(rule.instances)} '
      'instance${rule.instances == 1 ? '' : 's'}.';
}

class _RankBadge extends StatelessWidget {
  const _RankBadge({required this.rank, required this.tone});

  final int rank;
  final Color tone;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 40,
      height: 40,
      padding: const EdgeInsets.all(1),
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: Soft.rim(context),
        boxShadow: Soft.raised(context, depth: 0.5),
      ),
      child: DecoratedBox(
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: Theme.of(context).colorScheme.surface,
          gradient: Soft.sheen(context),
        ),
        child: Center(
          child: Text(
            '$rank',
            style: CrawlStatTheme.mono(
              context,
              size: 15,
              color: tone,
            ).copyWith(fontWeight: FontWeight.w700),
          ),
        ),
      ),
    );
  }
}

class _CertaintyPill extends StatelessWidget {
  const _CertaintyPill({required this.certainty});

  final String certainty;

  @override
  Widget build(BuildContext context) {
    final fail = certainty == 'fail';
    return Tooltip(
      message: fail
          ? 'The markup alone proves this fails WCAG.'
          : 'Likely a failure, but there are legitimate exceptions. '
                'Confirm by looking at the page.',
      child: TonePill(
        fail ? 'Failure' : 'Needs a look',
        color: fail
            ? CrawlStatTheme.palette(context).error
            : CrawlStatTheme.palette(context).warning,
      ),
    );
  }
}

/// Why it matters, how to fix it, and a before/after.
class _RuleExplainer extends StatelessWidget {
  const _RuleExplainer({required this.info});

  final rust.A11yRuleInfo info;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SoftWell(
      radius: CrawlStatTheme.controlRadius,
      depth: 0.6,
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Why it matters', style: theme.textTheme.labelMedium),
          const SizedBox(height: 4),
          Text(info.why, style: theme.textTheme.bodyMedium),
          const SizedBox(height: 12),
          Text('How to fix', style: theme.textTheme.labelMedium),
          const SizedBox(height: 4),
          Text(info.fix, style: theme.textTheme.bodyMedium),
          const SizedBox(height: 12),
          LayoutBuilder(
            builder: (context, constraints) {
              final bad = _CodeSample(
                label: 'Before',
                code: info.bad,
                tone: CrawlStatTheme.palette(context).error,
              );
              final good = _CodeSample(
                label: 'After',
                code: info.good,
                tone: CrawlStatTheme.palette(context).success,
              );
              if (constraints.maxWidth < 640) {
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [bad, const SizedBox(height: 8), good],
                );
              }
              return Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(child: bad),
                  const SizedBox(width: 10),
                  Expanded(child: good),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

class _CodeSample extends StatelessWidget {
  const _CodeSample({
    required this.label,
    required this.code,
    required this.tone,
  });

  final String label;
  final String code;
  final Color tone;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.fromLTRB(12, 8, 8, 10),
      decoration: BoxDecoration(
        color: tone.withValues(alpha: 0.07),
        borderRadius: BorderRadius.circular(10),
        border: Border(left: BorderSide(color: tone, width: 3)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  label,
                  style: TextStyle(
                    color: tone,
                    fontSize: 11.5,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              _CopyButton(text: code),
            ],
          ),
          const SizedBox(height: 4),
          SelectableText(code, style: CrawlStatTheme.mono(context, size: 12)),
        ],
      ),
    );
  }
}

class _CopyButton extends StatelessWidget {
  const _CopyButton({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return IconButton(
      tooltip: 'Copy',
      visualDensity: VisualDensity.compact,
      iconSize: 14,
      icon: const Icon(AppIcon.copy),
      onPressed: () async {
        await Clipboard.setData(ClipboardData(text: text));
        if (!context.mounted) return;
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text('Copied')));
      },
    );
  }
}

class _GroupRow extends StatelessWidget {
  const _GroupRow({required this.group});

  final rust.A11yGroup group;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: SelectableText(
                  group.sample,
                  maxLines: 2,
                  style: CrawlStatTheme.mono(context, size: 12),
                ),
              ),
              const SizedBox(width: 10),
              Text(
                'on ${formatCount(group.pages)} page${group.pages == 1 ? '' : 's'}',
                style: theme.textTheme.bodySmall,
              ),
            ],
          ),
          for (final url in group.exampleUrls)
            Padding(
              padding: const EdgeInsets.only(left: 12, top: 2),
              child: _UrlLine(url: url),
            ),
        ],
      ),
    );
  }
}

class _UrlLine extends StatelessWidget {
  const _UrlLine({required this.url, this.trailing});

  final String url;
  final String? trailing;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: [
        Expanded(
          child: Text(
            url,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: CrawlStatTheme.mono(
              context,
              size: 11.5,
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        if (trailing != null) ...[
          const SizedBox(width: 8),
          Text(trailing!, style: theme.textTheme.bodySmall),
        ],
        IconButton(
          tooltip: 'Open in browser',
          visualDensity: VisualDensity.compact,
          iconSize: 13,
          icon: const Icon(AppIcon.open),
          onPressed: () => _openPage(context, url),
        ),
      ],
    );
  }
}

class _RulePages extends ConsumerWidget {
  const _RulePages({required this.path, required this.rule});

  final String path;
  final String rule;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pages = ref.watch(accessibilityRulePagesProvider((path, rule)));
    return pages.when(
      loading: () => const Padding(
        padding: EdgeInsets.all(12),
        child: SoftProgressBar(value: 0, height: 4, indeterminate: true),
      ),
      error: (error, _) => InlineMessage(message: '$error'),
      data: (pages) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final page in pages)
            _UrlLine(url: page.url, trailing: '${page.count}×'),
          if (pages.length >= 200)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                'Showing the first 200. Export the report for the full list '
                'of templates.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
        ],
      ),
    );
  }
}

// ─── the checklist ───────────────────────────────────────────────────────

class _CriterionTile extends ConsumerStatefulWidget {
  const _CriterionTile({
    super.key,
    required this.criterion,
    required this.report,
    required this.path,
  });

  final rust.A11yCriterion criterion;
  final rust.A11yReport report;
  final String path;

  @override
  ConsumerState<_CriterionTile> createState() => _CriterionTileState();
}

class _CriterionTileState extends ConsumerState<_CriterionTile> {
  bool _open = false;
  bool _saving = false;
  late final TextEditingController _note = TextEditingController(
    text: widget.criterion.reviewNote ?? '',
  );

  @override
  void dispose() {
    _note.dispose();
    super.dispose();
  }

  Future<void> _review(String status) async {
    setState(() => _saving = true);
    try {
      await rust.accessibilitySetReview(
        dbPath: widget.path,
        criterion: widget.criterion.id,
        status: status,
        note: _note.text,
      );
      ref.invalidate(
        accessibilityReportProvider((
          widget.path,
          ref.read(accessibilityStandardProvider),
        )),
      );
    } on Object catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('Could not save: $error')));
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final c = widget.criterion;
    final tone = _statusColour(context, c.status);
    final hits = widget.report.rules
        .where((rule) => c.rulesHit.contains(rule.info.id))
        .toList();

    return SoftPanel(
      depth: 0.7,
      padding: const EdgeInsets.fromLTRB(16, 12, 14, 12),
      onTap: () => setState(() => _open = !_open),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              ConstrainedBox(
                constraints: const BoxConstraints(minWidth: 118),
                child: Align(
                  alignment: Alignment.centerLeft,
                  widthFactor: 1,
                  child: TonePill(
                    _statusLabel(c.status),
                    color: tone,
                    dot: true,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              SizedBox(
                width: 52,
                child: Text(
                  c.id,
                  style: CrawlStatTheme.mono(
                    context,
                    size: 12.5,
                    color: theme.colorScheme.onSurface,
                  ),
                ),
              ),
              Expanded(
                child: Text(
                  c.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodyMedium,
                ),
              ),
              if (c.conflict)
                Padding(
                  padding: EdgeInsets.only(right: 8),
                  child: Tooltip(
                    message:
                        'Marked as passing, but the crawler still finds '
                        'failures.',
                    child: Icon(
                      AppIcon.warning,
                      size: 15,
                      color: CrawlStatTheme.palette(context).error,
                    ),
                  ),
                ),
              if (c.since != '2.0')
                Padding(
                  padding: const EdgeInsets.only(right: 6),
                  child: TonePill(
                    'New in ${c.since}',
                    color: CrawlStatTheme.palette(context).notice,
                  ),
                ),
              TonePill(c.level, color: CrawlStatTheme.palette(context).notice),
              const SizedBox(width: 6),
              Icon(
                _open ? AppIcon.chevronUp : AppIcon.chevronDown,
                size: 16,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ],
          ),
          if (_open) ...[
            const SizedBox(height: 12),
            Text(c.summary, style: theme.textTheme.bodyMedium),
            if (c.conflict) ...[
              const SizedBox(height: 10),
              InlineMessage(
                severity: 'error',
                message:
                    'Marked as ${c.reviewStatus == 'na' ? 'not applicable' : 'passing'}, '
                    'but the crawler still finds failures on '
                    '${formatCount(c.failingPages)} pages. Fix those, or '
                    'change the review.',
              ),
            ],
            if (c.notApplicableHint != null) ...[
              const SizedBox(height: 10),
              InlineMessage(
                severity: 'notice',
                message: c.notApplicableHint!,
                action: TextButton(
                  onPressed: _saving ? null : () => _review('na'),
                  child: const Text('Mark N/A'),
                ),
              ),
            ],
            const SizedBox(height: 12),
            _Evidence(criterion: c, hits: hits),
            const SizedBox(height: 12),
            SoftWell(
              radius: CrawlStatTheme.controlRadius,
              depth: 0.6,
              padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('How to test it', style: theme.textTheme.labelMedium),
                  const SizedBox(height: 4),
                  Text(c.howToTest, style: theme.textTheme.bodyMedium),
                ],
              ),
            ),
            const SizedBox(height: 12),
            Wrap(
              spacing: 10,
              runSpacing: 10,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Text('Your result', style: theme.textTheme.labelMedium),
                SegmentedButton<String>(
                  segments: const [
                    ButtonSegment(value: 'pass', label: Text('Passes')),
                    ButtonSegment(value: 'fail', label: Text('Fails')),
                    ButtonSegment(value: 'na', label: Text('Not applicable')),
                  ],
                  selected: {?c.reviewStatus},
                  emptySelectionAllowed: true,
                  showSelectedIcon: false,
                  style: const ButtonStyle(
                    visualDensity: VisualDensity.compact,
                  ),
                  onSelectionChanged: _saving
                      ? null
                      : (selection) =>
                            _review(selection.isEmpty ? '' : selection.first),
                ),
                if (c.reviewedAt != null)
                  Text(
                    'Recorded ${_date(c.reviewedAt!)}',
                    style: theme.textTheme.bodySmall,
                  ),
                TextButton.icon(
                  onPressed: () => _openPage(context, c.url),
                  icon: const Icon(AppIcon.open, size: 14),
                  label: const Text('W3C guidance'),
                ),
              ],
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _note,
              minLines: 1,
              maxLines: 3,
              decoration: InputDecoration(
                isDense: true,
                hintText:
                    'Evidence or notes — tool used, pages tested, what failed',
                suffixIcon: c.reviewStatus == null
                    ? null
                    : IconButton(
                        tooltip: 'Save note',
                        icon: const Icon(AppIcon.save, size: 16),
                        onPressed: _saving
                            ? null
                            : () => _review(c.reviewStatus!),
                      ),
              ),
              onSubmitted: c.reviewStatus == null
                  ? null
                  : (_) => _review(c.reviewStatus!),
            ),
          ],
        ],
      ),
    );
  }
}

/// What the crawler contributed to a criterion.
class _Evidence extends StatelessWidget {
  const _Evidence({required this.criterion, required this.hits});

  final rust.A11yCriterion criterion;
  final List<rust.A11yRule> hits;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final c = criterion;
    if (hits.isNotEmpty) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Found by the crawler', style: theme.textTheme.labelMedium),
          const SizedBox(height: 6),
          for (final rule in hits)
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Row(
                children: [
                  _CertaintyPill(certainty: rule.info.certainty),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      rule.info.title,
                      style: theme.textTheme.bodyMedium,
                    ),
                  ),
                  Text(
                    '${formatCount(rule.pages)} pages',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
          Text(
            'Details and fixes are under Fix first.',
            style: theme.textTheme.bodySmall,
          ),
        ],
      );
    }
    final text = c.rulesRun.isEmpty
        ? 'The crawler cannot test this from markup. Only a person can.'
        : '${c.rulesRun.length} automated check'
              '${c.rulesRun.length == 1 ? '' : 's'} ran and found nothing. '
              'That is not a pass: they catch the common failures, not all '
              'of them.';
    return Row(
      children: [
        Icon(
          c.rulesRun.isEmpty ? AppIcon.priority : AppIcon.ok,
          size: 15,
          color: c.rulesRun.isEmpty
              ? CrawlStatTheme.palette(context).notice
              : CrawlStatTheme.palette(context).success,
        ),
        const SizedBox(width: 8),
        Expanded(child: Text(text, style: theme.textTheme.bodySmall)),
      ],
    );
  }
}

// ─── test one page ───────────────────────────────────────────────────────

/// Check any live URL right now: a staging page, a fix, a page the crawl
/// did not reach.
class _CheckUrlPanel extends StatefulWidget {
  const _CheckUrlPanel();

  @override
  State<_CheckUrlPanel> createState() => _CheckUrlPanelState();
}

class _CheckUrlPanelState extends State<_CheckUrlPanel> {
  final _url = TextEditingController();
  bool _running = false;
  rust.A11yPageCheck? _result;
  String? _error;

  @override
  void dispose() {
    _url.dispose();
    super.dispose();
  }

  Future<void> _check() async {
    final url = _url.text.trim();
    if (url.isEmpty) return;
    setState(() {
      _running = true;
      _error = null;
    });
    try {
      final result = await rust.accessibilityCheckUrl(url: url);
      if (mounted) setState(() => _result = result);
    } on Object catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _running = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final result = _result;

    return SectionCard(
      title: 'Test a page now',
      subtitle:
          'Check any URL against the automated rules without a crawl — a '
          'staging build, a fix you just shipped, a page behind a link.',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _url,
                  style: CrawlStatTheme.mono(context),
                  decoration: const InputDecoration(
                    isDense: true,
                    hintText: 'https://staging.example.com/checkout',
                    prefixIcon: Icon(AppIcon.web, size: 16),
                  ),
                  onSubmitted: (_) => _check(),
                ),
              ),
              const SizedBox(width: 10),
              SoftButton(
                icon: AppIcon.search,
                gradient: Soft.accentGradient(context),
                onPressed: _running ? null : _check,
                child: const Text('Check page'),
              ),
            ],
          ),
          if (_running) ...[
            const SizedBox(height: 12),
            const SoftProgressBar(value: 0, height: 4, indeterminate: true),
          ],
          if (_error != null) ...[
            const SizedBox(height: 12),
            InlineMessage(message: _error!),
          ],
          if (result != null && !_running) ...[
            const SizedBox(height: 16),
            _PageCheckResult(result: result),
          ],
          if (result == null && !_running && _error == null) ...[
            const SizedBox(height: 10),
            Text(
              'Pages behind a login, or built entirely in JavaScript, will '
              'show only what the server sends before scripts run.',
              style: theme.textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}

class _PageCheckResult extends StatelessWidget {
  const _PageCheckResult({required this.result});

  final rust.A11yPageCheck result;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final failures = result.findings
        .where((hit) => hit.info.certainty == 'fail')
        .length;
    final looks = result.findings.length - failures;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          result.title?.isNotEmpty == true ? result.title! : result.url,
          style: theme.textTheme.titleSmall,
        ),
        const SizedBox(height: 6),
        Wrap(
          spacing: 8,
          runSpacing: 6,
          children: [
            TonePill(
              '$failures failure${failures == 1 ? '' : 's'}',
              color: failures == 0
                  ? CrawlStatTheme.palette(context).success
                  : CrawlStatTheme.palette(context).error,
            ),
            TonePill(
              '$looks to look at',
              color: looks == 0
                  ? CrawlStatTheme.palette(context).notice
                  : CrawlStatTheme.palette(context).warning,
            ),
            TonePill(
              '${result.passed.length} checks passed',
              color: CrawlStatTheme.palette(context).success,
            ),
          ],
        ),
        if (result.likelyScriptRendered) ...[
          const SizedBox(height: 10),
          InlineMessage(
            message:
                'This page has only ${result.wordCount} words before scripts '
                'run, so it is probably built by JavaScript. These results '
                'describe its loading shell, not what people see.',
          ),
        ],
        const SizedBox(height: 10),
        for (final hit in result.findings) _HitTile(hit: hit),
      ],
    );
  }
}

class _HitTile extends StatefulWidget {
  const _HitTile({required this.hit});

  final rust.A11yHit hit;

  @override
  State<_HitTile> createState() => _HitTileState();
}

class _HitTileState extends State<_HitTile> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final hit = widget.hit;
    final info = hit.info;
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            borderRadius: BorderRadius.circular(8),
            onTap: () => setState(() => _open = !_open),
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 6, horizontal: 4),
              child: Row(
                children: [
                  Container(
                    width: 8,
                    height: 8,
                    decoration: BoxDecoration(
                      color: _impactColour(context, info.impact),
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text(info.title, style: theme.textTheme.bodyMedium),
                  ),
                  Text(
                    '${hit.count}× · ${info.criteria.join(', ')}',
                    style: theme.textTheme.bodySmall,
                  ),
                  const SizedBox(width: 8),
                  _CertaintyPill(certainty: info.certainty),
                  Icon(
                    _open ? AppIcon.chevronUp : AppIcon.chevronDown,
                    size: 15,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ],
              ),
            ),
          ),
          if (_open) ...[
            for (final sample in hit.samples)
              Padding(
                padding: const EdgeInsets.fromLTRB(22, 2, 4, 2),
                child: SelectableText(
                  sample,
                  style: CrawlStatTheme.mono(context, size: 12),
                ),
              ),
            const SizedBox(height: 8),
            _RuleExplainer(info: info),
            const SizedBox(height: 8),
          ],
        ],
      ),
    );
  }
}

// ─── the law ─────────────────────────────────────────────────────────────

class _LawPanel extends StatelessWidget {
  const _LawPanel({required this.standard});

  final rust.A11yStandard standard;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SectionCard(
      title: 'Why ${standard.label}',
      subtitle: 'The laws and policies that point at this standard',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final law in standard.laws)
            Padding(
              padding: const EdgeInsets.only(bottom: 6),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Padding(
                    padding: EdgeInsets.only(top: 3),
                    child: Icon(
                      AppIcon.indexable,
                      size: 14,
                      color: CrawlStatTheme.palette(context).accent,
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(child: Text(law, style: theme.textTheme.bodyMedium)),
                ],
              ),
            ),
          const SizedBox(height: 10),
          Text('To finish an audit', style: theme.textTheme.labelMedium),
          const SizedBox(height: 4),
          Text(
            '1. Fix everything under Fix first, then re-crawl.\n'
            '2. Work through the open criteria: keyboard only, a screen '
            'reader (VoiceOver, NVDA), a contrast checker, 200% zoom and a '
            '320px-wide window.\n'
            '3. Record each result here, then export the report as your '
            'conformance evidence.\n'
            '4. Publish an accessibility statement with a way for people to '
            'report barriers — and test with disabled users when you can.',
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 12),
          Text(
            'Automated checks find only part of WCAG failures, so a clean '
            'crawl is not compliance. This tool is an engineering aid, not '
            'legal advice or a certification.',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

// ─── helpers ─────────────────────────────────────────────────────────────

const _checkedBlue = Color(0xFF5B9BD5);

Color _statusColour(BuildContext context, String status) {
  final palette = CrawlStatTheme.palette(context);
  return switch (status) {
    'fail' => palette.error,
    'review' => palette.warning,
    // Blue only where severity is a hue at all; on a grey ramp it sits
    // between "check" and "passes".
    'checks_passed' =>
      palette.name == 'ember'
          ? _checkedBlue
          : Color.lerp(palette.notice, palette.success, 0.5)!,
    'pass' => palette.success,
    _ => palette.notice,
  };
}

String _statusLabel(String status) => switch (status) {
  'fail' => 'Fails',
  'review' => 'Check',
  'checks_passed' => 'Nothing found',
  'untested' => 'Manual test',
  'pass' => 'Passes',
  'na' => 'N/A',
  _ => status,
};

Color _impactColour(BuildContext context, String impact) {
  final palette = CrawlStatTheme.palette(context);
  return switch (impact) {
    'critical' => palette.error,
    // Between critical and moderate. Ember has an orange for that; the grey
    // ramps take the step between their error and warning tones, since their
    // accent is reserved for the call to action.
    'serious' =>
      palette.name == 'ember'
          ? palette.accent
          : Color.lerp(palette.error, palette.warning, 0.5)!,
    'moderate' => palette.warning,
    _ => palette.notice,
  };
}

String _capitalise(String text) =>
    text.isEmpty ? text : text[0].toUpperCase() + text.substring(1);

String _date(int unixSeconds) {
  final date = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  return '${date.year}-${date.month.toString().padLeft(2, '0')}-'
      '${date.day.toString().padLeft(2, '0')}';
}

Future<void> _openPage(BuildContext context, String url) async {
  final messenger = ScaffoldMessenger.of(context);
  if (!await openLink(url)) {
    messenger.showSnackBar(
      const SnackBar(content: Text('Link copied — paste it into a browser')),
    );
  }
}

/// The host a crawl file is about: `example-com-20261024-0930.db` →
/// `example.com`.
String siteOfCrawl(String path) {
  final name = path.split(Platform.pathSeparator).last;
  return name
      .replaceFirst(RegExp(r'-\d{8}-\d{4}\.db$'), '')
      .replaceAll('-', '.');
}
