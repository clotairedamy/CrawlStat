import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../router.dart';
import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart' show ProjectInfo;
import '../state/issues.dart';
import '../state/projects.dart';
import '../state/settings.dart';
import '../theme/icons.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/reveal_list.dart';
import '../widgets/export.dart';
import '../widgets/report_view.dart';
import 'projects_screen.dart' show showReportDialog;
import 'shell.dart';

/// What to fix, in order.
///
/// The Issues screen answers "what is wrong with this site". This one answers
/// the question people actually have, which is "what should I do on Monday
/// morning" — and they are not the same list. An issue affecting nine hundred
/// pages through three template changes belongs above an error on one orphan,
/// and no severity-sorted table will ever put it there.
class PlanScreen extends ConsumerWidget {
  const PlanScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final path = ref.watch(openProjectProvider);
    if (path == null) return const NoProjectState();

    final plan = ref.watch(actionPlanProvider(path));

    return plan.when(
      loading: () => const Column(
        children: [
          ScreenHeader(title: 'Plan', subtitle: 'Ranking what to fix…'),
          Expanded(child: Center(child: CircularProgressIndicator())),
        ],
      ),
      error: (error, _) => Column(
        children: [
          const ScreenHeader(title: 'Plan'),
          Expanded(
            child: EmptyState(
              icon: AppIcon.report,
              title: 'Could not build the plan',
              message: '$error',
            ),
          ),
        ],
      ),
      data: (data) => _Body(plan: data, path: path),
    );
  }
}

class _Body extends ConsumerWidget {
  const _Body({required this.plan, required this.path});

  final rust.ActionPlan plan;
  final String path;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (plan.items.isEmpty) {
      return Column(
        children: [
          const ScreenHeader(title: 'Plan'),
          Expanded(
            child: EmptyState(
              icon: AppIcon.ok,
              title: 'Nothing to do',
              message:
                  'This crawl turned up no issues worth ranking. '
                  'Run an audit if you have not yet.',
            ),
          ),
        ],
      );
    }

    final templateWins = plan.items
        .where((item) => item.effort == 'template')
        .length;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ScreenHeader(
          title: 'Plan',
          subtitle:
              '${plan.items.length} job${plan.items.length == 1 ? '' : 's'} '
              '· ${formatCount(plan.affectedPages)} of '
              '${formatCount(plan.totalPages)} pages affected'
              '${templateWins > 0 ? ' · $templateWins can be fixed in a template' : ''}',
        ),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(
              CrawlStatTheme.gutter,
              0,
              CrawlStatTheme.gutter,
              CrawlStatTheme.gutter,
            ),
            child: RevealList(
              spacing: 12,
              children: [
                _Narrative(plan: plan, path: path),
                for (final (index, item) in plan.items.indexed)
                  _JobCard(item: item, rank: index + 1),
                _Caveat(),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

/// One job.
class _JobCard extends ConsumerStatefulWidget {
  const _JobCard({required this.item, required this.rank});

  final rust.PlanItem item;
  final int rank;

  @override
  ConsumerState<_JobCard> createState() => _JobCardState();
}

class _JobCardState extends ConsumerState<_JobCard> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final item = widget.item;
    final tone = CrawlStatTheme.severityColor(context, item.severity);

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 16),
      onTap: () => setState(() => _open = !_open),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _ImpactDial(impact: item.impact, rank: widget.rank, tone: tone),
              const SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(item.title, style: theme.textTheme.titleSmall),
                    const SizedBox(height: 6),
                    Text(
                      _headline(item),
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurface,
                      ),
                    ),
                    const SizedBox(height: 10),
                    Wrap(
                      spacing: 8,
                      runSpacing: 6,
                      children: [
                        TonePill(_effortLabel(item.effort), color: tone),
                        _Stat(label: 'pages', value: formatCount(item.pages)),
                        _Stat(
                          label: 'changes',
                          value: formatCount(item.distinctFixes),
                        ),
                        _Stat(
                          label: 'of link value',
                          value:
                              '${(item.reach * 100).toStringAsFixed(item.reach < 0.1 ? 1 : 0)}%',
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
            SoftWell(
              radius: CrawlStatTheme.controlRadius,
              depth: 0.6,
              padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('How to fix', style: theme.textTheme.labelMedium),
                  const SizedBox(height: 6),
                  Text(item.howToFix, style: theme.textTheme.bodyMedium),
                  if (item.examples.isNotEmpty) ...[
                    const SizedBox(height: 14),
                    Text('Worst affected', style: theme.textTheme.labelMedium),
                    const SizedBox(height: 6),
                    for (final example in item.examples)
                      _ExampleRow(example: example),
                  ],
                ],
              ),
            ),
            const SizedBox(height: 12),
            Align(
              alignment: Alignment.centerLeft,
              child: SoftButton(
                icon: AppIcon.issues,
                onPressed: () => context.go(Routes.issue(item.issueId)),
                child: const Text('See every page'),
              ),
            ),
          ],
        ],
      ),
    );
  }

  /// The one line that says what this job actually is.
  ///
  /// The whole argument for this screen lives here: "968 pages, 3 changes"
  /// is a completely different job from "968 pages, 968 changes", and a
  /// severity-sorted issue list shows them identically.
  String _headline(rust.PlanItem item) {
    if (item.pages == 0) return 'No pages affected.';
    final pages =
        '${formatCount(item.pages)} page'
        '${item.pages == 1 ? '' : 's'}';
    if (item.distinctFixes <= 1) {
      return 'One change fixes all $pages.';
    }
    if (item.leverage >= 2) {
      return '$pages, but only ${formatCount(item.distinctFixes)} distinct '
          'changes — about ${item.leverage.toStringAsFixed(0)} pages per fix.';
    }
    return '$pages, each needing its own change.';
  }
}

/// The impact score, as a dial.
class _ImpactDial extends StatelessWidget {
  const _ImpactDial({
    required this.impact,
    required this.rank,
    required this.tone,
  });

  final double impact;
  final int rank;
  final Color tone;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      width: 56,
      child: Column(
        children: [
          Container(
            width: 50,
            height: 50,
            padding: const EdgeInsets.all(1),
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              gradient: Soft.rim(context),
              boxShadow: Soft.raised(context, depth: 0.5),
            ),
            child: DecoratedBox(
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: theme.colorScheme.surface,
                gradient: Soft.sheen(context),
              ),
              child: Center(
                child: Text(
                  impact.round().toString(),
                  style: CrawlStatTheme.mono(
                    context,
                    size: 17,
                    color: tone,
                  ).copyWith(fontWeight: FontWeight.w700),
                ),
              ),
            ),
          ),
          const SizedBox(height: 5),
          Text('#$rank', style: theme.textTheme.labelMedium),
        ],
      ),
    );
  }
}

class _ExampleRow extends StatelessWidget {
  const _ExampleRow({required this.example});

  final rust.PlanExample example;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Row(
        children: [
          Expanded(
            child: Text(
              example.url,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: CrawlStatTheme.mono(context, size: 11.5),
            ),
          ),
          if (example.detail != null) ...[
            const SizedBox(width: 10),
            Text(
              example.detail!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}

class _Stat extends StatelessWidget {
  const _Stat({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          value,
          style: CrawlStatTheme.mono(
            context,
            size: 12.5,
            color: theme.colorScheme.onSurface,
          ).copyWith(fontWeight: FontWeight.w600),
        ),
        const SizedBox(width: 4),
        Text(label, style: theme.textTheme.bodySmall),
      ],
    );
  }
}

/// The local model's write-up, when the user asks for one.
///
/// Streams in as the model writes it: a local model takes a minute or more
/// to finish, and watching the report form is a far better minute than a
/// spinner.
class _Narrative extends ConsumerStatefulWidget {
  const _Narrative({required this.plan, required this.path});

  final rust.ActionPlan plan;
  final String path;

  @override
  ConsumerState<_Narrative> createState() => _NarrativeState();
}

class _NarrativeState extends ConsumerState<_Narrative> {
  StreamSubscription<rust.ReportEvent>? _subscription;
  String? _report;
  String? _error;

  /// The model writing the report, once Ollama has said which it is.
  String? _model;

  /// The model the user had chosen, when it was not installed and another
  /// one stood in for it.
  String? _replaced;

  DateTime? _startedAt;
  Duration? _took;

  bool get _running => _subscription != null;

  @override
  void initState() {
    super.initState();
    // The last report written for this crawl, so leaving the screen and
    // coming back does not lose it.
    _report = ref.read(writtenReportsProvider)[widget.path];
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  void _write() {
    final settings = ref.read(settingsProvider);
    setState(() {
      _report = null;
      _error = null;
      _model = null;
      _replaced = null;
      _took = null;
      _startedAt = DateTime.now();
    });
    ref.read(writtenReportsProvider.notifier).clear(widget.path);
    _subscription = rust
        .ollamaWriteReport(
          dbPath: widget.path,
          endpoint: settings.ollamaEndpoint,
          model: settings.ollamaModel,
          site: _siteOf(widget.path),
        )
        .listen(_onEvent, onError: _fail, onDone: _finish);
  }

  void _onEvent(rust.ReportEvent event) {
    if (!mounted) return;
    switch (event) {
      case rust.ReportEvent_Model(:final name):
        final wanted = ref.read(settingsProvider).ollamaModel;
        if (name != wanted) {
          // Remember the stand-in, so the next report does not have to
          // discover all over again that the default is missing.
          ref
              .read(settingsProvider.notifier)
              .update((s) => s.copyWith(ollamaModel: name));
        }
        setState(() {
          _model = name;
          _replaced = name == wanted ? null : wanted;
        });
      case rust.ReportEvent_Progress(:final text):
        setState(() => _report = text);
      case rust.ReportEvent_Done(:final text):
        ref.read(writtenReportsProvider.notifier).put(widget.path, text);
        setState(() {
          _report = text;
          if (_startedAt != null) {
            _took = DateTime.now().difference(_startedAt!);
          }
        });
      case rust.ReportEvent_Failed(:final message):
        _fail(message);
    }
  }

  void _fail(Object error) {
    if (mounted) setState(() => _error = '$error');
  }

  void _finish() {
    if (mounted) setState(() => _subscription = null);
  }

  void _stop() {
    _subscription?.cancel();
    setState(() => _subscription = null);
  }

  Future<void> _copy() async {
    await Clipboard.setData(ClipboardData(text: _report!));
    if (!mounted) return;
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(const SnackBar(content: Text('Report copied as Markdown')));
  }

  Future<void> _save() => exportText(
    context,
    ref,
    name: 'report-${_siteOf(widget.path).replaceAll('.', '-')}',
    extension: 'md',
    contents: _report!,
    done: 'Report saved as Markdown',
  );

  /// The designed client report, with what the model wrote as its summary.
  void _exportPdf(ProjectInfo project) => showReportDialog(context, project);

  Future<void> _showBrief() async {
    final text = await rust.ollamaBrief(
      dbPath: widget.path,
      site: _siteOf(widget.path),
    );
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Exactly what is sent to the model'),
        content: SizedBox(
          width: 620,
          child: SingleChildScrollView(
            child: SelectableText(
              text,
              style: CrawlStatTheme.mono(context, size: 12),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Clipboard.setData(ClipboardData(text: text)),
            child: const Text('Copy'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  String get _status {
    final model = _model ?? ref.read(settingsProvider).ollamaModel;
    if (_running && (_report ?? '').isEmpty) return 'Loading $model…';
    if (_running) return 'Writing with $model…';
    if (_took != null) {
      return 'Written by $model on this machine in ${_duration(_took!)}';
    }
    if (_report != null) return 'Stopped part-way · $model';
    return 'Written on this machine by a local model';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final finished = !_running && (_report ?? '').isNotEmpty;
    final project = ref
        .watch(recentProjectsProvider)
        .where((p) => p.path == widget.path)
        .firstOrNull;

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(22, 18, 22, 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              IconBadge(
                AppIcon.pro,
                color: CrawlStatTheme.palette(context).accent,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Report', style: theme.textTheme.titleSmall),
                    const SizedBox(height: 2),
                    Text(
                      _status,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall,
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  SoftButton(
                    icon: AppIcon.report,
                    onPressed: _showBrief,
                    child: const Text('What is sent'),
                  ),
                  if (finished) ...[
                    SoftButton(
                      icon: AppIcon.copy,
                      onPressed: _copy,
                      child: const Text('Copy'),
                    ),
                    SoftButton(
                      icon: AppIcon.save,
                      onPressed: _save,
                      child: const Text('Markdown'),
                    ),
                  ],
                  if (project != null && !_running)
                    SoftButton(
                      icon: AppIcon.download,
                      gradient: finished ? Soft.accentGradient(context) : null,
                      onPressed: () => _exportPdf(project),
                      child: const Text('Export PDF'),
                    ),
                  if (_running)
                    SoftButton(
                      icon: AppIcon.stop,
                      onPressed: _stop,
                      child: const Text('Stop'),
                    )
                  else
                    SoftButton(
                      icon: AppIcon.pro,
                      gradient: _report == null
                          ? Soft.accentGradient(context)
                          : null,
                      onPressed: _write,
                      child: Text(
                        _report == null ? 'Write the report' : 'Rewrite',
                      ),
                    ),
                ],
              ),
            ],
          ),
          if (_running) ...[
            const SizedBox(height: 14),
            const SoftProgressBar(value: 0, height: 5, indeterminate: true),
          ],
          if (_replaced != null) ...[
            const SizedBox(height: 14),
            InlineMessage(
              severity: 'notice',
              message:
                  '$_replaced is not installed, so $_model wrote this. '
                  'It is now the model in Settings.',
            ),
          ],
          if (_error != null) ...[
            const SizedBox(height: 14),
            InlineMessage(message: _error!, severity: 'warning'),
          ],
          const SizedBox(height: 18),
          if ((_report ?? '').isNotEmpty)
            SelectionArea(
              child: ReportView(
                markdown: _report!,
                figures: _Figures(plan: widget.plan),
              ),
            )
          else ...[
            _Figures(plan: widget.plan),
            const SizedBox(height: 14),
            Text(
              'A language model running on this machine can turn the ranking '
              'below into a client-ready report: a verdict, what to fix first, '
              'the plan in order, and what can wait. Nothing is uploaded — the '
              'model is local, and you can read the exact brief before it is '
              'sent.',
              style: theme.textTheme.bodySmall,
            ),
          ],
          if (finished) ...[
            const SizedBox(height: 18),
            Text(
              'The figures come from this crawl; the wording is the model\'s. '
              'Read it through before it goes to a client.',
              style: theme.textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}

/// The crawl's own headline numbers, shown in the report beside the model's
/// prose so the figures that matter never depend on what the model wrote.
class _Figures extends StatelessWidget {
  const _Figures({required this.plan});

  final rust.ActionPlan plan;

  @override
  Widget build(BuildContext context) {
    final affected = plan.totalPages == 0
        ? 0.0
        : plan.affectedPages / plan.totalPages;
    final figures = [
      ('Pages crawled', formatCount(plan.totalPages), null),
      (
        'Pages with a problem',
        formatCount(plan.affectedPages),
        '${(affected * 100).toStringAsFixed(0)}% of the site',
      ),
      (
        'Link value at risk',
        '${(plan.valueAtRisk * 100).toStringAsFixed(0)}%',
        'on pages with an error',
      ),
      ('Jobs ranked', formatCount(plan.items.length), null),
    ];

    return SoftWell(
      radius: CrawlStatTheme.controlRadius + 1,
      depth: 0.6,
      padding: const EdgeInsets.symmetric(vertical: 14),
      child: IntrinsicHeight(
        child: Row(
          children: [
            for (final (i, (label, value, hint)) in figures.indexed) ...[
              if (i > 0)
                VerticalDivider(
                  width: 1,
                  color: Theme.of(context).dividerColor,
                ),
              Expanded(
                child: _Figure(label: label, value: value, hint: hint),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _Figure extends StatelessWidget {
  const _Figure({required this.label, required this.value, this.hint});

  final String label;
  final String value;
  final String? hint;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 6),
          Text(
            value,
            style: theme.textTheme.titleLarge?.copyWith(
              fontSize: 22,
              height: 1.1,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
          if (hint != null) ...[
            const SizedBox(height: 4),
            Text(
              hint!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}

String _duration(Duration duration) {
  final seconds = duration.inSeconds;
  if (seconds < 60) return '$seconds s';
  return '${seconds ~/ 60} min ${seconds % 60} s';
}

/// What the score is and, more importantly, what it is not.
class _Caveat extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 4),
      child: Text(
        'Impact is relative to this crawl: it ranks the jobs against each '
        'other, highest at 100. It is built from severity, the share of the '
        "site's internal link value on the affected pages, and how many pages "
        'one change fixes. It is not a traffic forecast — CrawlStat has not '
        'seen your analytics, and a number pretending otherwise would be a '
        'guess in a lab coat.',
        style: theme.textTheme.bodySmall,
      ),
    );
  }
}

String _effortLabel(String effort) => switch (effort) {
  'template' => 'One template',
  'bulk' => 'Batch',
  _ => 'Page by page',
};

/// The host a crawl file is about, for the model's brief.
String _siteOf(String path) {
  final name = path.split(Platform.pathSeparator).last;
  return name
      .replaceFirst(RegExp(r'-\d{8}-\d{4}\.db$'), '')
      .replaceAll('-', '.');
}
