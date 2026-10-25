import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';
import '../state/projects.dart';
import '../theme/icons.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'shell.dart';

/// Did the fix work?
///
/// The other screens say what is wrong and what to do first. This one closes
/// the loop — without it every crawl is an island and there is no reason to
/// run the tool twice.
class CompareScreen extends ConsumerStatefulWidget {
  const CompareScreen({super.key});

  @override
  ConsumerState<CompareScreen> createState() => _CompareScreenState();
}

class _CompareScreenState extends ConsumerState<CompareScreen> {
  String? _before;
  String? _after;
  rust.CrawlComparison? _result;
  String? _error;
  bool _running = false;

  Future<void> _run() async {
    final before = _before;
    final after = _after;
    if (before == null || after == null) return;
    setState(() {
      _running = true;
      _error = null;
    });
    try {
      final result = await rust.compareCrawls(
        beforePath: before,
        afterPath: after,
      );
      if (mounted) setState(() => _result = result);
    } on Object catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _running = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final projects = ref.watch(recentProjectsProvider);

    if (projects.length < 2) {
      return const Column(
        children: [
          ScreenHeader(title: 'Compare'),
          Expanded(
            child: EmptyState(
              icon: AppIcon.report,
              title: 'Two crawls needed',
              message:
                  'Crawl the same site twice — before and after a change — '
                  'and this shows what actually moved.',
            ),
          ),
        ],
      );
    }

    // Newest as "after", the one before it as "before": the comparison people
    // want nine times out of ten, available without touching the pickers.
    _after ??= projects.first.path;
    _before ??= projects.length > 1 ? projects[1].path : null;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const ScreenHeader(
          title: 'Compare',
          subtitle: 'What a change actually moved',
        ),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(
              CrawlStatTheme.gutter,
              0,
              CrawlStatTheme.gutter,
              CrawlStatTheme.gutter,
            ),
            children: [
              _Pickers(
                projects: projects,
                before: _before,
                after: _after,
                running: _running,
                onBefore: (path) => setState(() {
                  _before = path;
                  _result = null;
                }),
                onAfter: (path) => setState(() {
                  _after = path;
                  _result = null;
                }),
                onRun: _run,
              ),
              const SizedBox(height: 16),
              if (_error != null)
                InlineMessage(message: _error!, severity: 'warning')
              else if (_result != null)
                _Result(result: _result!),
            ],
          ),
        ),
      ],
    );
  }
}

class _Pickers extends StatelessWidget {
  const _Pickers({
    required this.projects,
    required this.before,
    required this.after,
    required this.running,
    required this.onBefore,
    required this.onAfter,
    required this.onRun,
  });

  final List<ProjectInfo> projects;
  final String? before;
  final String? after;
  final bool running;
  final ValueChanged<String> onBefore;
  final ValueChanged<String> onAfter;
  final VoidCallback onRun;

  @override
  Widget build(BuildContext context) {
    final sameCrawl = before != null && before == after;
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _CrawlPicker(
            label: 'Before',
            projects: projects,
            value: before,
            onChanged: onBefore,
          ),
          const SizedBox(height: 14),
          _CrawlPicker(
            label: 'After',
            projects: projects,
            value: after,
            onChanged: onAfter,
          ),
          if (sameCrawl) ...[
            const SizedBox(height: 12),
            InlineMessage(
              severity: 'notice',
              message:
                  'Both pickers point at the same crawl, so nothing will '
                  'have changed.',
            ),
          ],
          const SizedBox(height: 18),
          Align(
            alignment: Alignment.centerLeft,
            child: SoftButton(
              icon: AppIcon.report,
              onPressed: running ? null : onRun,
              child: Text(running ? 'Comparing…' : 'Compare'),
            ),
          ),
        ],
      ),
    );
  }
}

class _CrawlPicker extends StatelessWidget {
  const _CrawlPicker({
    required this.label,
    required this.projects,
    required this.value,
    required this.onChanged,
  });

  final String label;
  final List<ProjectInfo> projects;
  final String? value;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
        const SizedBox(height: 6),
        SoftWell(
          radius: CrawlStatTheme.controlRadius,
          depth: 0.6,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: DropdownButtonHideUnderline(
            child: DropdownButton<String>(
              value: value,
              isExpanded: true,
              dropdownColor: theme.colorScheme.surfaceContainerLow,
              borderRadius: BorderRadius.circular(CrawlStatTheme.radius),
              items: [
                for (final project in projects)
                  DropdownMenuItem(
                    value: project.path,
                    child: Text(
                      '${project.rootHost ?? project.path.split('/').last}'
                      '  ·  ${_when(project)}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: CrawlStatTheme.mono(context, size: 12.5),
                    ),
                  ),
              ],
              onChanged: (path) {
                if (path != null) onChanged(path);
              },
            ),
          ),
        ),
      ],
    );
  }

  String _when(ProjectInfo project) {
    final iso = project.finishedAt ?? project.createdAt;
    if (iso == null) return 'unknown date';
    final parsed = DateTime.tryParse(iso);
    if (parsed == null) return iso;
    final local = parsed.toLocal();
    String two(int v) => v.toString().padLeft(2, '0');
    return '${two(local.day)}/${two(local.month)} ${two(local.hour)}:${two(local.minute)}';
  }
}

class _Result extends StatelessWidget {
  const _Result({required this.result});

  final rust.CrawlComparison result;

  @override
  Widget build(BuildContext context) {
    final improvements = result.issues
        .where((i) => i.after < i.before)
        .toList();
    final regressions = result.issues.where((i) => i.after > i.before).toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Headline(result: result),
        if (result.pagesBroken.isNotEmpty) ...[
          const SizedBox(height: 16),
          _BrokenPages(pages: result.pagesBroken),
        ],
        if (improvements.isNotEmpty) ...[
          const SizedBox(height: 16),
          _DeltaList(
            title: 'Fixed',
            subtitle: 'Pages that had the problem and no longer do.',
            tone: CrawlStatTheme.palette(context).success,
            issues: improvements,
          ),
        ],
        if (regressions.isNotEmpty) ...[
          const SizedBox(height: 16),
          _DeltaList(
            title: 'Got worse',
            subtitle: 'New since the earlier crawl.',
            tone: CrawlStatTheme.palette(context).error,
            issues: regressions,
          ),
        ],
        if (improvements.isEmpty && regressions.isEmpty) ...[
          const SizedBox(height: 16),
          SoftPanel(
            padding: const EdgeInsets.all(18),
            child: Text(
              'No issue changed between these two crawls.',
              style: Theme.of(context).textTheme.bodyMedium,
            ),
          ),
        ],
      ],
    );
  }
}

class _Headline extends StatelessWidget {
  const _Headline({required this.result});

  final rust.CrawlComparison result;

  @override
  Widget build(BuildContext context) {
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 18, 18, 18),
      child: Wrap(
        spacing: 28,
        runSpacing: 18,
        children: [
          _Figure(
            label: 'Fixed',
            value: '${result.totalFixed}',
            colour: result.totalFixed > 0
                ? CrawlStatTheme.palette(context).success
                : null,
          ),
          _Figure(
            label: 'Introduced',
            value: '${result.totalIntroduced}',
            colour: result.totalIntroduced > 0
                ? CrawlStatTheme.palette(context).error
                : null,
          ),
          _Figure(
            label: 'Pages with problems',
            value: '${result.affectedBefore} → ${result.affectedAfter}',
            colour: result.affectedAfter < result.affectedBefore
                ? CrawlStatTheme.palette(context).success
                : result.affectedAfter > result.affectedBefore
                ? CrawlStatTheme.palette(context).error
                : null,
          ),
          _Figure(
            label: 'Link value at risk',
            value:
                '${(result.valueAtRiskBefore * 100).round()}%'
                ' → ${(result.valueAtRiskAfter * 100).round()}%',
          ),
          _Figure(
            label: 'Pages crawled',
            value: '${result.pagesBefore} → ${result.pagesAfter}',
          ),
        ],
      ),
    );
  }
}

class _Figure extends StatelessWidget {
  const _Figure({required this.label, required this.value, this.colour});

  final String label;
  final String value;
  final Color? colour;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
        const SizedBox(height: 4),
        Text(
          value,
          style: CrawlStatTheme.mono(
            context,
            size: 19,
            color: colour ?? theme.colorScheme.onSurface,
          ).copyWith(fontWeight: FontWeight.w600),
        ),
      ],
    );
  }
}

class _DeltaList extends StatelessWidget {
  const _DeltaList({
    required this.title,
    required this.subtitle,
    required this.tone,
    required this.issues,
  });

  final String title;
  final String subtitle;
  final Color tone;
  final List<rust.IssueDelta> issues;

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
              Container(
                width: 8,
                height: 8,
                margin: const EdgeInsets.only(right: 10),
                decoration: BoxDecoration(
                  gradient: glossOf(tone),
                  shape: BoxShape.circle,
                ),
              ),
              Text(title, style: theme.textTheme.titleSmall),
            ],
          ),
          const SizedBox(height: 2),
          Padding(
            padding: const EdgeInsets.only(left: 18),
            child: Text(subtitle, style: theme.textTheme.bodySmall),
          ),
          const SizedBox(height: 14),
          for (final issue in issues) ...[
            _DeltaRow(issue: issue, tone: tone),
            const SizedBox(height: 10),
          ],
        ],
      ),
    );
  }
}

class _DeltaRow extends StatelessWidget {
  const _DeltaRow({required this.issue, required this.tone});

  final rust.IssueDelta issue;
  final Color tone;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final net = issue.after - issue.before;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 54,
          child: Text(
            net < 0 ? '$net' : '+$net',
            textAlign: TextAlign.right,
            style: CrawlStatTheme.mono(
              context,
              size: 14,
              color: tone,
            ).copyWith(fontWeight: FontWeight.w700),
          ),
        ),
        const SizedBox(width: 14),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(issue.title, style: theme.textTheme.bodyMedium),
              const SizedBox(height: 2),
              Text(
                '${issue.before} → ${issue.after} pages'
                '${issue.impactBefore > 0 || issue.impactAfter > 0 ? '  ·  impact '
                          '${issue.impactBefore.round()} → ${issue.impactAfter.round()}' : ''}',
                style: theme.textTheme.bodySmall,
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _BrokenPages extends StatelessWidget {
  const _BrokenPages({required this.pages});

  final List<rust.BrokenPage> pages;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      glow: CrawlStatTheme.palette(context).error,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Icon(
                AppIcon.error,
                size: 17,
                color: CrawlStatTheme.palette(context).error,
              ),
              const SizedBox(width: 8),
              Text(
                '${pages.length} page${pages.length == 1 ? '' : 's'} started failing',
                style: theme.textTheme.titleSmall,
              ),
            ],
          ),
          const SizedBox(height: 2),
          Text(
            'These worked in the earlier crawl. Whatever changed, this is the '
            'first thing to look at.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 12),
          for (final page in pages.take(20))
            Padding(
              padding: const EdgeInsets.only(bottom: 5),
              child: Row(
                children: [
                  SizedBox(
                    width: 86,
                    child: Text(
                      '${page.statusBefore ?? '—'} → ${page.statusAfter ?? 'failed'}',
                      style: CrawlStatTheme.mono(
                        context,
                        size: 12,
                        color: CrawlStatTheme.palette(context).error,
                      ),
                    ),
                  ),
                  Expanded(
                    child: Text(
                      page.url,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: CrawlStatTheme.mono(context, size: 12),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}
