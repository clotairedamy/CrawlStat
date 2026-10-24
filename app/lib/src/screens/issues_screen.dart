import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../router.dart';
import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../state/settings.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'shell.dart';

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

  Future<void> _exportIssues(String path) async {
    final settings = ref.read(settingsProvider);
    final directory = Directory(settings.resolvedExportDirectory);
    if (!directory.existsSync()) directory.createSync(recursive: true);
    final file =
        '${directory.path}${Platform.pathSeparator}crawlstat-issues-'
        '${DateTime.now().millisecondsSinceEpoch}.csv';

    try {
      await rust.exportIssuesCsv(dbPath: path, outPath: file);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: const Text('Issue summary exported'),
          action: SnackBarAction(
            label: 'Copy path',
            onPressed: () => Clipboard.setData(ClipboardData(text: file)),
          ),
        ),
      );
    } on Object catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('Export failed: $error')));
    }
  }

  @override
  Widget build(BuildContext context) {
    final path = ref.watch(openProjectProvider);
    if (path == null) {
      return const Column(
        children: [ScreenHeader(title: 'Issues'), Expanded(child: NoProjectState())],
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
                  : const Icon(Icons.refresh, size: 14),
              label: Text(_auditing ? 'Auditing…' : 'Re-run audits'),
            ),
            const SizedBox(width: 6),
            OutlinedButton.icon(
              onPressed: () => _exportIssues(path),
              icon: const Icon(Icons.file_download_outlined, size: 14),
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
              icon: Icons.error_outline,
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
                  icon: Icons.verified_outlined,
                  title: 'No issues found',
                  message: 'Either this site is in good shape, or the audits have not '
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

              return Row(
                children: [
                  SizedBox(
                    width: 380,
                    child: _IssueList(
                      report: data,
                      selectedId: selected.id,
                      onSelect: (id) => setState(() => _selectedId = id),
                    ),
                  ),
                  Container(width: 1, color: Theme.of(context).dividerColor),
                  Expanded(child: _IssueDetail(issue: selected, path: path)),
                ],
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
      'error': report.issues.where((issue) => issue.severity == 'error').toList(),
      'warning': report.issues.where((issue) => issue.severity == 'warning').toList(),
      'notice': report.issues.where((issue) => issue.severity == 'notice').toList(),
    };

    return Container(
      color: theme.colorScheme.surface,
      child: ListView(
        padding: const EdgeInsets.only(bottom: 20),
        children: [
          for (final entry in groups.entries)
            if (entry.value.isNotEmpty) ...[
              Container(
                padding: const EdgeInsets.fromLTRB(14, 14, 14, 6),
                child: Row(
                  children: [
                    SeverityDot(entry.key),
                    const SizedBox(width: 7),
                    Text(
                      '${entry.key.toUpperCase()}S',
                      style: theme.textTheme.labelMedium,
                    ),
                    const Spacer(),
                    Text('${entry.value.length}', style: theme.textTheme.bodySmall),
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
      ),
    );
  }
}

class _IssueRow extends StatelessWidget {
  const _IssueRow({required this.issue, required this.selected, required this.onTap});

  final Issue issue;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return InkWell(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
        decoration: BoxDecoration(
          color: selected ? CrawlStatTheme.accent.withValues(alpha: 0.12) : null,
          border: Border(
            left: BorderSide(
              color: selected ? CrawlStatTheme.accent : Colors.transparent,
              width: 2,
            ),
          ),
        ),
        child: Row(
          children: [
            Expanded(
              child: Text(
                issue.title,
                maxLines: 2,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: selected ? CrawlStatTheme.accent : null,
                ),
              ),
            ),
            const SizedBox(width: 8),
            Text(
              formatCount(issue.urlCount),
              style: CrawlStatTheme.mono(
                context,
                color: CrawlStatTheme.severityColor(issue.severity),
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

  Future<void> _exportAffected(BuildContext context, WidgetRef ref) async {
    final settings = ref.read(settingsProvider);
    final directory = Directory(settings.resolvedExportDirectory);
    if (!directory.existsSync()) directory.createSync(recursive: true);
    final file = '${directory.path}${Platform.pathSeparator}'
        'crawlstat-${issue.id}-${DateTime.now().millisecondsSinceEpoch}.csv';

    try {
      final rows = await rust.exportIssueUrlsCsv(
        dbPath: path,
        issueId: issue.id,
        outPath: file,
      );
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Exported ${formatCount(rows)} affected URLs'),
          action: SnackBarAction(
            label: 'Copy path',
            onPressed: () => Clipboard.setData(ClipboardData(text: file)),
          ),
        ),
      );
    } on Object catch (error) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('Export failed: $error')));
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);

    return ListView(
      padding: const EdgeInsets.fromLTRB(22, 20, 22, 30),
      children: [
        Row(
          children: [
            SeverityDot(issue.severity, size: 9),
            const SizedBox(width: 8),
            Text(
              issue.severity.toUpperCase(),
              style: theme.textTheme.labelMedium?.copyWith(
                color: CrawlStatTheme.severityColor(issue.severity),
              ),
            ),
            const Spacer(),
            Text(
              '${formatCount(issue.urlCount)} affected',
              style: theme.textTheme.bodySmall,
            ),
          ],
        ),
        const SizedBox(height: 8),
        Text(issue.title, style: theme.textTheme.titleLarge),
        const SizedBox(height: 14),
        Text(issue.description, style: theme.textTheme.bodyMedium?.copyWith(height: 1.5)),
        const SizedBox(height: 22),
        Text('HOW TO FIX', style: theme.textTheme.labelMedium),
        const SizedBox(height: 8),
        Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(
            color: theme.colorScheme.surfaceContainerLow,
            border: Border.all(color: theme.dividerColor),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Text(
            issue.howToFix,
            style: theme.textTheme.bodyMedium?.copyWith(height: 1.55),
          ),
        ),
        const SizedBox(height: 20),
        Row(
          children: [
            FilledButton.icon(
              onPressed: () {
                // Filter the results table to exactly these URLs — the whole
                // point of the Issues screen is getting to the rows.
                ref.read(urlTableProvider.notifier).setIssue(issue.id);
                context.go('${Routes.results}?tab=internal');
              },
              icon: const Icon(Icons.filter_alt_outlined, size: 15),
              label: Text('Show ${formatCount(issue.urlCount)} URLs'),
            ),
            const SizedBox(width: 8),
            OutlinedButton.icon(
              onPressed: () => _exportAffected(context, ref),
              icon: const Icon(Icons.file_download_outlined, size: 14),
              label: const Text('Export affected'),
            ),
            const SizedBox(width: 8),
            OutlinedButton.icon(
              onPressed: () => Clipboard.setData(
                ClipboardData(
                  text: 'https://crawlstat.com/docs/audits/${issue.docsSlug}',
                ),
              ),
              icon: const Icon(Icons.link, size: 14),
              label: const Text('Copy docs link'),
            ),
          ],
        ),
      ],
    );
  }
}
