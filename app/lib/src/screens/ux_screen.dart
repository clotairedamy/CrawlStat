import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';
import '../state/projects.dart';
import '../state/settings.dart';
import '../theme/icons.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/reveal_list.dart';
import 'shell.dart';

final _personasProvider = FutureProvider<List<rust.UxPersona>>((ref) {
  return rust.uxPersonas();
});

/// Can a visitor actually find the thing they came for?
///
/// The earlier version of this screen scored one page's elements and left the
/// reader to work out what that meant. This one answers a question somebody
/// actually asks out loud — *can a first-time visitor get from the front door
/// to the pricing page, and how many clicks does it cost?* — by walking the
/// crawled site the way a visitor would and showing the path it took.
class UxScreen extends ConsumerStatefulWidget {
  const UxScreen({super.key});

  @override
  ConsumerState<UxScreen> createState() => _UxScreenState();
}

class _UxScreenState extends ConsumerState<UxScreen> {
  String? _personaOverride;
  List<rust.JourneyResult>? _results;
  rust.JourneyResult? _custom;
  rust.IaReport? _ia;
  String? _rivalPath;
  List<rust.TaskBenchmark>? _benchmark;
  String? _error;
  bool _running = false;
  final TextEditingController _task = TextEditingController();

  /// The chosen visitor, falling back to the one set in Settings.
  String get _persona =>
      _personaOverride ?? ref.read(settingsProvider).defaultPersona;

  @override
  void dispose() {
    _task.dispose();
    super.dispose();
  }

  /// Run one task written in plain language — the thing a real study is
  /// made of, as opposed to the seven presets.
  Future<void> _runTask(String dbPath) async {
    final task = _task.text.trim();
    if (task.isEmpty) return;
    setState(() {
      _running = true;
      _error = null;
    });
    try {
      final result = await rust.runCustomJourney(
        dbPath: dbPath,
        persona: _persona,
        task: task,
      );
      if (mounted) setState(() => _custom = result);
    } on Object catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _running = false);
    }
  }

  /// The comparison a stakeholder actually responds to.
  ///
  /// "Our pricing is four clicks deep" is an opinion about a number. "Ours is
  /// four and theirs is one" is a decision — and it needs no lab, no
  /// recruiting and no sessions, only two crawls.
  Future<void> _compare(String dbPath) async {
    final rival = _rivalPath;
    if (rival == null) return;
    setState(() {
      _running = true;
      _error = null;
    });
    try {
      final results = await rust.benchmarkAgainst(
        ourPath: dbPath,
        theirPath: rival,
        persona: _persona,
      );
      if (mounted) setState(() => _benchmark = results);
    } on Object catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _running = false);
    }
  }

  Future<void> _loadArchitecture(String dbPath) async {
    try {
      final settings = ref.read(settingsProvider);
      final report = await rust.architectureReport(
        dbPath: dbPath,
        deepThreshold: settings.deepThreshold,
        crowdedThreshold: settings.crowdedThreshold,
      );
      if (mounted) setState(() => _ia = report);
    } on Object catch (error) {
      if (mounted) setState(() => _error = '$error');
    }
  }

  Future<void> _run(String dbPath) async {
    setState(() {
      _running = true;
      _error = null;
    });
    try {
      final results = await rust.runJourneys(dbPath: dbPath, persona: _persona);
      if (mounted) setState(() => _results = results);
      await _loadArchitecture(dbPath);
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
    final personas = ref.watch(_personasProvider).value ?? const [];

    final found = _results?.where((r) => r.outcome == 'reached').length ?? 0;
    final missing =
        _results
            ?.where((r) => r.outcome != 'reached' && r.targetExists)
            .length ??
        0;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ScreenHeader(
          title: 'Journeys',
          subtitle: _results == null
              ? 'Can a visitor find what they came for?'
              : '$found of ${_results!.length} goals reachable'
                    '${missing > 0 ? '  ·  $missing page${missing == 1 ? '' : 's'} '
                              'exist but could not be found' : ''}',
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
              spacing: 16,
              children: [
                _Controls(
                  personas: personas,
                  persona: _persona,
                  running: _running,
                  onPersona: (id) => setState(() {
                    _personaOverride = id;
                    _results = null;
                  }),
                  onRun: () => _run(path),
                ),
                _TaskBox(
                  controller: _task,
                  running: _running,
                  result: _custom,
                  onRun: () => _runTask(path),
                ),
                _BenchmarkCard(
                  projects: ref.watch(recentProjectsProvider),
                  ourPath: path,
                  rivalPath: _rivalPath,
                  results: _benchmark,
                  running: _running,
                  onRival: (value) => setState(() {
                    _rivalPath = value;
                    _benchmark = null;
                  }),
                  onRun: () => _compare(path),
                ),
                Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (_error != null)
                      InlineMessage(message: _error!, severity: 'error')
                    else if (_results == null)
                      const _Explainer()
                    else ...[
                      if (_results!.any((r) => r.unfetchedLinks > 20))
                        Padding(
                          padding: const EdgeInsets.only(bottom: 16),
                          child: InlineMessage(
                            severity: 'warning',
                            message:
                                'This crawl did not fetch every page it found, so '
                                'some routes are missing from the map. Finish the '
                                'crawl for a reliable answer.',
                          ),
                        ),
                      for (final result in _results!) ...[
                        _JourneyCard(result: result),
                        const SizedBox(height: 12),
                      ],
                      if (_ia != null) ...[
                        const SizedBox(height: 8),
                        _Architecture(
                          report: _ia!,
                          path: path,
                          deepThreshold: ref
                              .read(settingsProvider)
                              .deepThreshold,
                          crowdedThreshold: ref
                              .read(settingsProvider)
                              .crowdedThreshold,
                        ),
                        const SizedBox(height: 12),
                      ],
                      const SizedBox(height: 4),
                      const _Caveat(),
                    ],
                  ],
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _Controls extends StatelessWidget {
  const _Controls({
    required this.personas,
    required this.persona,
    required this.running,
    required this.onPersona,
    required this.onRun,
  });

  final List<rust.UxPersona> personas;
  final String persona;
  final bool running;
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
          Text('Who is visiting', style: theme.textTheme.labelMedium),
          const SizedBox(height: 10),
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
                          ? CrawlStatTheme.palette(context).accent
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
              child: Text(running ? 'Walking the site…' : 'Run the journeys'),
            ),
          ),
        ],
      ),
    );
  }
}

/// One goal, its outcome, and the path taken.
class _JourneyCard extends StatefulWidget {
  const _JourneyCard({required this.result});

  final rust.JourneyResult result;

  @override
  State<_JourneyCard> createState() => _JourneyCardState();
}

class _JourneyCardState extends State<_JourneyCard> {
  bool _open = false;

  /// The three outcomes a designer needs to tell apart, because each has a
  /// completely different fix.
  (String, String, Color, IconData) _verdict() {
    final result = widget.result;
    if (result.outcome == 'reached') {
      return (
        'Found in ${result.clicks} click${result.clicks == 1 ? '' : 's'}',
        'The path is signposted the whole way.',
        CrawlStatTheme.palette(context).success,
        AppIcon.ok,
      );
    }
    if (result.targetExists) {
      return (
        'Page exists but was not found',
        'The page is in the crawl'
            '${result.shortestKnown != null ? ' at depth ${result.shortestKnown}' : ''}. '
            'Nothing on the way signposts it, so a visitor hunting for this '
            'would not get there.',
        CrawlStatTheme.palette(context).error,
        AppIcon.warning,
      );
    }
    return (
      'No such page',
      'Nothing on this site looks like it. That is only a problem if it '
          'should exist.',
      CrawlStatTheme.palette(context).notice,
      AppIcon.remove,
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final result = widget.result;
    final (headline, detail, tone, icon) = _verdict();
    final expandable = result.steps.isNotEmpty || result.targetUrl != null;

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 16),
      onTap: expandable ? () => setState(() => _open = !_open) : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Container(
                width: 34,
                height: 34,
                margin: const EdgeInsets.only(right: 14),
                padding: const EdgeInsets.all(1),
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  gradient: Soft.rim(context),
                  boxShadow: Soft.raised(context, depth: 0.4),
                ),
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: theme.colorScheme.surface,
                    gradient: Soft.sheen(context),
                  ),
                  child: Icon(icon, size: 16, color: tone),
                ),
              ),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(result.goalLabel, style: theme.textTheme.titleSmall),
                    const SizedBox(height: 3),
                    Text(
                      headline,
                      style: theme.textTheme.bodyMedium?.copyWith(color: tone),
                    ),
                    const SizedBox(height: 3),
                    Text(detail, style: theme.textTheme.bodySmall),
                  ],
                ),
              ),
              if (expandable)
                Icon(
                  _open ? AppIcon.chevronUp : AppIcon.chevronDown,
                  size: 18,
                  color: theme.colorScheme.onSurfaceVariant,
                ),
            ],
          ),
          if (_open) ...[
            const SizedBox(height: 14),
            SoftWell(
              radius: CrawlStatTheme.controlRadius,
              depth: 0.6,
              padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (result.steps.isEmpty)
                    Text(
                      'The visitor did not click anything — nothing on the '
                      'home page pointed this way.',
                      style: theme.textTheme.bodySmall,
                    )
                  else ...[
                    Text('The path taken', style: theme.textTheme.labelMedium),
                    const SizedBox(height: 8),
                    for (final (index, step) in result.steps.indexed)
                      _StepRow(step: step, index: index + 1),
                  ],
                  if (result.targetUrl != null &&
                      result.outcome != 'reached') ...[
                    const SizedBox(height: 12),
                    Text(
                      'Where it actually is',
                      style: theme.textTheme.labelMedium,
                    ),
                    const SizedBox(height: 4),
                    SelectableText(
                      result.targetUrl!,
                      style: CrawlStatTheme.mono(context, size: 12),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _StepRow extends StatelessWidget {
  const _StepRow({required this.step, required this.index});

  final rust.JourneyStep step;
  final int index;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 22,
            child: Text(
              '$index',
              style: CrawlStatTheme.mono(
                context,
                size: 12,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text.rich(
                  TextSpan(
                    children: [
                      const TextSpan(text: 'clicked '),
                      TextSpan(
                        text: '“${step.anchor}”',
                        style: TextStyle(
                          color: CrawlStatTheme.palette(context).accent,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ],
                  ),
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 2),
                Text(
                  step.toUrl,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: CrawlStatTheme.mono(
                    context,
                    size: 11.5,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
                if (step.alternatives.isNotEmpty) ...[
                  const SizedBox(height: 3),
                  Text(
                    'also on offer: ${step.alternatives.join(' · ')}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

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
            'It starts at your home page and walks the crawled site the way a '
            'visitor would: reads what is on offer, clicks whatever looks most '
            'like what it came for, and repeats until it finds the page, runs '
            'out of patience, or goes in a circle.\n\n'
            'Each result separates the three cases that need different fixes: '
            'the page was found, the page exists but nothing signposts it, or '
            'there is no such page at all.',
            style: theme.textTheme.bodyMedium,
          ),
        ],
      ),
    );
  }
}

class _Caveat extends StatelessWidget {
  const _Caveat();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 4),
      child: Text(
        'The simulated visitor reads link text, not pixels, and never uses '
        'search or the back button. A journey that succeeds is good evidence '
        'the path exists. A journey that fails is a hypothesis worth testing, '
        'not proof that nobody can get there.',
        style: Theme.of(context).textTheme.bodySmall,
      ),
    );
  }
}

/// Run one task of your own, in the words the study uses.
class _TaskBox extends StatelessWidget {
  const _TaskBox({
    required this.controller,
    required this.running,
    required this.result,
    required this.onRun,
  });

  final TextEditingController controller;
  final bool running;
  final rust.JourneyResult? result;
  final VoidCallback onRun;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Try a task of your own', style: theme.textTheme.titleSmall),
          const SizedBox(height: 4),
          Text(
            'The seven above are the usual suspects. A real study has its own: '
            '“find the returns policy”, “book a demo”.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 14),
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: controller,
                  decoration: const InputDecoration(
                    hintText: 'find the returns policy',
                  ),
                  onSubmitted: (_) => onRun(),
                ),
              ),
              const SizedBox(width: 10),
              SoftButton(
                icon: AppIcon.search,
                onPressed: running ? null : onRun,
                child: const Text('Try it'),
              ),
            ],
          ),
          if (result != null) ...[
            const SizedBox(height: 14),
            _JourneyCard(result: result!),
          ],
        ],
      ),
    );
  }
}

/// What the shape of the site does to a visitor.
class _Architecture extends StatelessWidget {
  const _Architecture({
    required this.report,
    required this.path,
    required this.deepThreshold,
    required this.crowdedThreshold,
  });

  final rust.IaReport report;
  final String path;
  final int deepThreshold;
  final int crowdedThreshold;

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
              Expanded(
                child: Text(
                  'How the site is put together',
                  style: theme.textTheme.titleSmall,
                ),
              ),
              SoftButton(
                icon: AppIcon.download,
                onPressed: () => _export(context),
                child: const Text('Export'),
              ),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            '${report.pages} pages · deepest ${report.maxDepth} clicks from home '
            '· ${report.medianLinks} links on a typical page',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 14),
          if (report.findings.isEmpty)
            Text(
              'Nothing stood out. Labels are consistent, nothing is orphaned '
              'and no page is drowning in links.',
              style: theme.textTheme.bodyMedium,
            )
          else
            for (final finding in report.findings) ...[
              _Finding(finding: finding),
              const SizedBox(height: 10),
            ],
        ],
      ),
    );
  }

  Future<void> _export(BuildContext context) async {
    final target =
        '${defaultExportDirectory()}${Platform.pathSeparator}ux-findings.md';
    try {
      await rust.exportUxReport(
        dbPath: path,
        outPath: target,
        deepThreshold: deepThreshold,
        crowdedThreshold: crowdedThreshold,
      );
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('Written to $target')));
      }
    } on Object catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('Export failed: $error')));
      }
    }
  }
}

/// One finding, with the evidence folded away until asked for.
class _Finding extends StatefulWidget {
  const _Finding({required this.finding});

  final rust.IaFinding finding;

  @override
  State<_Finding> createState() => _FindingState();
}

class _FindingState extends State<_Finding> {
  bool _open = false;

  /// Why this matters, in one line. A count on its own is not a finding.
  String get _why => switch (widget.finding.kind) {
    'ambiguous_label' =>
      'Nobody can predict where it goes, and a screen reader\u2019s link list '
          'becomes a row of identical entries.',
    'inconsistent_label' =>
      'A visitor cannot tell they have already been there.',
    'orphan' =>
      'Reachable only from a sitemap, a search result or a bookmark \u2014 '
          'never by browsing.',
    'too_deep' => 'Past about three clicks, browsing stops finding things.',
    _ => 'When everything is offered at once, choosing becomes the task.',
  };

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SoftWell(
      radius: CrawlStatTheme.controlRadius,
      depth: 0.6,
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            onTap: () => setState(() => _open = !_open),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        widget.finding.summary,
                        style: theme.textTheme.bodyMedium,
                      ),
                      const SizedBox(height: 2),
                      Text(_why, style: theme.textTheme.bodySmall),
                    ],
                  ),
                ),
                Icon(
                  _open ? AppIcon.chevronUp : AppIcon.chevronDown,
                  size: 16,
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ],
            ),
          ),
          if (_open) ...[
            const SizedBox(height: 10),
            for (final line in widget.finding.evidence)
              Padding(
                padding: const EdgeInsets.only(bottom: 3),
                child: SelectableText(
                  line,
                  style: CrawlStatTheme.mono(context, size: 11.5),
                ),
              ),
          ],
        ],
      ),
    );
  }
}

/// How findable things are here, against how findable they are somewhere else.
class _BenchmarkCard extends StatelessWidget {
  const _BenchmarkCard({
    required this.projects,
    required this.ourPath,
    required this.rivalPath,
    required this.results,
    required this.running,
    required this.onRival,
    required this.onRun,
  });

  final List<ProjectInfo> projects;
  final String ourPath;
  final String? rivalPath;
  final List<rust.TaskBenchmark>? results;
  final bool running;
  final ValueChanged<String> onRival;
  final VoidCallback onRun;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final others = projects.where((p) => p.path != ourPath).toList();

    return SoftPanel(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Against someone else', style: theme.textTheme.titleSmall),
          const SizedBox(height: 4),
          Text(
            'Crawl a competitor, then run the same tasks on both. “Our pricing '
            'is four clicks” is an opinion. “Ours is four and theirs is one” '
            'is a decision.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 14),
          if (others.isEmpty)
            Text(
              'Crawl another site first — anyone\u2019s. Then pick it here.',
              style: theme.textTheme.bodyMedium,
            )
          else ...[
            Row(
              children: [
                Expanded(
                  child: SoftWell(
                    radius: CrawlStatTheme.controlRadius,
                    depth: 0.6,
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    child: DropdownButtonHideUnderline(
                      child: DropdownButton<String>(
                        value: rivalPath,
                        isExpanded: true,
                        hint: const Text('Pick a crawl to compare against'),
                        dropdownColor: theme.colorScheme.surfaceContainerLow,
                        borderRadius: BorderRadius.circular(
                          CrawlStatTheme.radius,
                        ),
                        items: [
                          for (final project in others)
                            DropdownMenuItem(
                              value: project.path,
                              child: Text(
                                project.rootHost ??
                                    project.path
                                        .split(Platform.pathSeparator)
                                        .last,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: CrawlStatTheme.mono(context, size: 12.5),
                              ),
                            ),
                        ],
                        onChanged: (value) {
                          if (value != null) onRival(value);
                        },
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 10),
                SoftButton(
                  icon: AppIcon.report,
                  onPressed: running || rivalPath == null ? null : onRun,
                  child: Text(running ? 'Running…' : 'Compare'),
                ),
              ],
            ),
            if (results != null) ...[
              const SizedBox(height: 16),
              for (final row in results!) ...[
                _BenchmarkRow(row: row),
                const SizedBox(height: 8),
              ],
            ],
          ],
        ],
      ),
    );
  }
}

class _BenchmarkRow extends StatelessWidget {
  const _BenchmarkRow({required this.row});

  final rust.TaskBenchmark row;

  /// What each side cost, in the words a report uses.
  String _cost(String outcome, int clicks, bool exists) {
    if (outcome == 'reached') return '$clicks';
    return exists ? 'lost' : '—';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final palette = CrawlStatTheme.palette(context);
    // Only one verdict is bad news, and it is the one worth colouring.
    final losing =
        row.verdict == 'they have it, we do not' ||
        row.verdict == 'slower than theirs';
    final tone = losing ? palette.error : palette.notice;

    return SoftWell(
      radius: CrawlStatTheme.controlRadius,
      depth: 0.6,
      padding: const EdgeInsets.fromLTRB(14, 11, 14, 11),
      child: Row(
        children: [
          Expanded(
            child: Text(row.goalLabel, style: theme.textTheme.bodyMedium),
          ),
          const SizedBox(width: 12),
          _Cell(
            label: 'ours',
            value: _cost(row.ourOutcome, row.ourClicks, row.ourExists),
          ),
          _Cell(
            label: 'theirs',
            value: _cost(row.theirOutcome, row.theirClicks, row.theirExists),
          ),
          const SizedBox(width: 14),
          SizedBox(
            width: 168,
            child: Text(
              row.verdict,
              textAlign: TextAlign.right,
              style: theme.textTheme.bodySmall?.copyWith(
                color: tone,
                fontWeight: losing ? FontWeight.w600 : FontWeight.w400,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Cell extends StatelessWidget {
  const _Cell({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      width: 54,
      child: Column(
        children: [
          Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
          const SizedBox(height: 2),
          Text(
            value,
            style: CrawlStatTheme.mono(
              context,
              size: 15,
              color: theme.colorScheme.onSurface,
            ).copyWith(fontWeight: FontWeight.w600),
          ),
        ],
      ),
    );
  }
}
