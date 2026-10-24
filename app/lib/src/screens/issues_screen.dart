import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../theme/icons.dart';
import '../router.dart';
import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../widgets/export.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'shell.dart';
import '../theme/soft.dart';

/// The audit results: what is wrong, how bad it is, and how to fix it.
///
/// A list on the left, the selected issue's explanation on the right. The fix
/// text is the same text the documentation site publishes, so what the app
/// tells you and what the docs tell you can never drift apart.
class IssuesScreen extends ConsumerStatefulWidget {
  const IssuesScreen({super.key});

  @override
  ConsumerState<IssuesScreen> createState() => _IssuesScreenState();
}

class _IssuesScreenState extends ConsumerState<IssuesScreen> {
  String? _selectedId;
  bool _auditing = false;

  Future<void> _reaudit(String path) async {
    setState(() => _auditing = true);
    try {
      await rust.runAudit(dbPath: path, thresholdsJson: null);
      ref.invalidate(auditReportProvider(path));
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

              final selected = data.issues.firstWhere(
                (issue) => issue.id == _selectedId,
                orElse: () => data.issues.first,
              );

              return Padding(
                padding: const EdgeInsets.fromLTRB(
                  CrawlStatTheme.gutter,
                  0,
                  CrawlStatTheme.gutter,
                  CrawlStatTheme.gutter,
                ),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    SizedBox(
                      width: 360,
                      child: SoftPanel(
                        child: _IssueList(
                          report: data,
                          selectedId: selected.id,
                          onSelect: (id) => setState(() => _selectedId = id),
                        ),
                      ),
                    ),
                    const SizedBox(width: 14),
                    Expanded(
                      child: SoftPanel(
                        child: _IssueDetail(issue: selected, path: path),
                      ),
                    ),
                  ],
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
    required this.report,
    required this.selectedId,
    required this.onSelect,
  });

  final AuditReport report;
  final String selectedId;
  final void Function(String) onSelect;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    // Grouped by severity, because that is the order to fix things in.
    final groups = <String, List<Issue>>{
      'error': report.issues
          .where((issue) => issue.severity == 'error')
          .toList(),
      'warning': report.issues
          .where((issue) => issue.severity == 'warning')
          .toList(),
      'notice': report.issues
          .where((issue) => issue.severity == 'notice')
          .toList(),
    };

    return ListView(
      padding: const EdgeInsets.fromLTRB(8, 4, 8, 12),
      children: [
        for (final entry in groups.entries)
          if (entry.value.isNotEmpty) ...[
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 14, 10, 6),
              child: Row(
                children: [
                  SeverityDot(entry.key),
                  const SizedBox(width: 8),
                  Text(
                    '${entry.key.toUpperCase()}S',
                    style: theme.textTheme.labelMedium,
                  ),
                  const Spacer(),
                  TonePill(
                    '${entry.value.length}',
                    color: CrawlStatTheme.severityColor(entry.key),
                  ),
                ],
              ),
            ),
            for (final issue in entry.value)
              _IssueRow(
                issue: issue,
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
    required this.issue,
    required this.selected,
    required this.onTap,
  });

  final Issue issue;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final radius = BorderRadius.circular(CrawlStatTheme.controlRadius);
    return InkWell(
      onTap: onTap,
      borderRadius: radius,
      child: Container(
        constraints: const BoxConstraints(minHeight: 38),
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
        decoration: BoxDecoration(
          color: selected
              ? CrawlStatTheme.accent.withValues(alpha: 0.14)
              : null,
          borderRadius: radius,
        ),
        child: Row(
          children: [
            Expanded(
              child: Text(
                issue.title,
                maxLines: 2,
                style: theme.textTheme.bodyMedium?.copyWith(
                  fontWeight: selected ? FontWeight.w600 : null,
                ),
              ),
            ),
            const SizedBox(width: 8),
            Text(
              formatCount(issue.urlCount),
              style: theme.textTheme.bodySmall?.copyWith(
                color: CrawlStatTheme.severityColor(issue.severity),
                fontWeight: FontWeight.w600,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _IssueDetail extends ConsumerWidget {
  const _IssueDetail({required this.issue, required this.path});

  final Issue issue;
  final String path;

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

    return ListView(
      padding: const EdgeInsets.fromLTRB(28, 24, 28, 30),
      children: [
        Row(
          children: [
            TonePill(
              issue.severity[0].toUpperCase() + issue.severity.substring(1),
              color: CrawlStatTheme.severityColor(issue.severity),
              dot: true,
            ),
            const SizedBox(width: 8),
            TonePill(
              '${formatCount(issue.urlCount)} URLs affected',
              color: CrawlStatTheme.notice,
            ),
          ],
        ),
        const SizedBox(height: 14),
        Text(issue.title, style: theme.textTheme.titleLarge),
        const SizedBox(height: 14),
        Text(
          issue.description,
          style: theme.textTheme.bodyMedium?.copyWith(height: 1.5),
        ),
        const SizedBox(height: 22),
        Text('HOW TO FIX', style: theme.textTheme.labelMedium),
        const SizedBox(height: 8),
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: CrawlStatTheme.accent.withValues(alpha: 0.07),
            border: Border.all(
              color: CrawlStatTheme.accent.withValues(alpha: 0.25),
            ),
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
              onPressed: () {
                // Filter the results table to exactly these URLs — the whole
                // point of the Issues screen is getting to the rows.
                ref.read(urlTableProvider.notifier).setIssue(issue.id);
                context.go('${Routes.results}?tab=internal');
              },
              icon: const Icon(AppIcon.filter, size: 15),
              label: Text('Show ${formatCount(issue.urlCount)} URLs'),
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
      ],
    );
  }
}
