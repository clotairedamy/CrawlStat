import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../router.dart';
import '../rust/api/autopilot.dart' as ap;
import '../state/license.dart';
import '../state/projects.dart';
import '../state/settings.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/export.dart';
import 'shell.dart';
import 'ux_journeys.dart';

typedef _Json = Map<String, dynamic>;

/// UX Testing: inspect a page, test it, prove what was found, rank it,
/// recommend a fix, retest, compare.
///
/// The work is split the way a tester moves through it — set up, read the
/// overview, work the findings, look at every check, follow the journeys,
/// retest, export — so no one screen tries to say everything. The numbers
/// across the top are deliberately several: a check that could not run is
/// never folded into a pass rate.
class UxScreen extends ConsumerStatefulWidget {
  const UxScreen({super.key});

  @override
  ConsumerState<UxScreen> createState() => _UxScreenState();
}

enum _Segment {
  setup('Setup', Icons.tune),
  overview('Overview', Icons.dashboard_outlined),
  findings('Findings', Icons.flag_outlined),
  checks('Checks', Icons.checklist),
  journeys('Journeys', Icons.route_outlined),
  retest('Retest', Icons.compare_arrows),
  export('Export', Icons.ios_share);

  const _Segment(this.label, this.icon);
  final String label;
  final IconData icon;
}

class _UxScreenState extends ConsumerState<UxScreen> {
  late final _Json _catalog = jsonDecode(ap.autopilotCatalog()) as _Json;
  _Segment _segment = _Segment.setup;
  _Json? _view;
  List<_Json> _audits = const [];
  bool _busy = false;
  String? _error;
  String? _loadedFor;

  _Json? get _audit => _view?['audit'] as _Json?;

  Future<void> _guard(Future<void> Function() action) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } on Object catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _loadList(String path) async {
    final list = jsonDecode(await ap.autopilotList(dbPath: path)) as List;
    if (!mounted) return;
    setState(() => _audits = list.cast<_Json>());
  }

  void _show(String json) {
    setState(() => _view = jsonDecode(json) as _Json);
  }

  Future<void> _run(String path, _Json setup) => _guard(() async {
    _show(await ap.autopilotRun(dbPath: path, setupJson: jsonEncode(setup)));
    await _loadList(path);
    if (mounted) setState(() => _segment = _Segment.overview);
  });

  Future<void> _open(String path, int id) => _guard(() async {
    _show(await ap.autopilotOpen(dbPath: path, id: id));
  });

  Future<void> _retest(String path) => _guard(() async {
    final id = _audit!['id'] as int;
    _show(await ap.autopilotRetest(dbPath: path, id: id));
    await _loadList(path);
    if (mounted) setState(() => _segment = _Segment.retest);
  });

  Future<void> _saveState(String path, _Json state) => _guard(() async {
    final id = _audit!['id'] as int;
    _show(
      await ap.autopilotSetState(
        dbPath: path,
        auditId: id,
        stateJson: jsonEncode(state),
      ),
    );
  });

  Future<void> _delete(String path) => _guard(() async {
    await ap.autopilotDelete(dbPath: path, id: _audit!['id'] as int);
    setState(() {
      _view = null;
      _segment = _Segment.setup;
    });
    await _loadList(path);
  });

  Future<void> _export(String path, {required bool csv}) async {
    final audit = _audit;
    if (audit == null) return;
    final id = audit['id'] as int;
    final host = Uri.tryParse(audit['setup']['url'] as String)?.host ?? 'page';
    final contents = csv
        ? await ap.autopilotDeveloperCsv(dbPath: path, id: id)
        : await ap.autopilotClientReport(dbPath: path, id: id);
    if (!mounted) return;
    await exportText(
      context,
      ref,
      name: 'ux-audit-${host.replaceAll('.', '-')}${csv ? '-findings' : ''}',
      extension: csv ? 'csv' : 'md',
      contents: contents,
      done: csv ? 'Findings saved as CSV' : 'Client report saved as Markdown',
    );
  }

  @override
  Widget build(BuildContext context) {
    if (!ref.watch(isProProvider)) return const _ProGate();
    final path = ref.watch(openProjectProvider);
    if (path == null) return const NoProjectState();
    if (_loadedFor != path) {
      _loadedFor = path;
      _view = null;
      _audits = const [];
      Future.microtask(() => _loadList(path));
    }
    final audit = _audit;
    final needsAudit = {
      _Segment.overview,
      _Segment.findings,
      _Segment.checks,
      _Segment.retest,
      _Segment.export,
    };

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ScreenHeader(
          title: 'UX Testing',
          subtitle: audit == null
              ? 'Test a page against its goal, prove each finding, fix, retest'
              : '${audit['setup']['url']}  ·  ${_label('page_types', audit['page_type'])}'
                    '  ·  ${_date(audit['tested_at'] as int)}',
          actions: [
            if (_audits.isNotEmpty)
              _AuditPicker(
                audits: _audits,
                selected: audit?['id'] as int?,
                onOpen: (id) => _open(path, id),
              ),
            if (audit != null) ...[
              ToolbarButton(
                icon: Icons.refresh,
                tooltip: 'Retest this page',
                onPressed: _busy ? null : () => _retest(path),
              ),
              ToolbarButton(
                icon: Icons.delete_outline,
                tooltip: 'Delete this audit',
                onPressed: _busy ? null : () => _delete(path),
              ),
            ],
          ],
        ),
        Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: CrawlStatTheme.gutter,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_view != null) ...[
                _SummaryStrip(view: _view!),
                const SizedBox(height: 12),
              ],
              SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: SegmentedButton<_Segment>(
                  showSelectedIcon: false,
                  style: const ButtonStyle(
                    visualDensity: VisualDensity.compact,
                  ),
                  segments: [
                    for (final s in _Segment.values)
                      ButtonSegment(
                        value: s,
                        label: Text(
                          s == _Segment.findings && _view != null
                              ? 'Findings (${_findings(_view!).length})'
                              : s.label,
                        ),
                        icon: Icon(s.icon, size: 16),
                        enabled: audit != null || !needsAudit.contains(s),
                      ),
                  ],
                  selected: {_segment},
                  onSelectionChanged: (s) => setState(() => _segment = s.first),
                ),
              ),
              if (_busy) ...[
                const SizedBox(height: 10),
                const LinearProgressIndicator(minHeight: 2),
              ],
              if (_error != null) ...[
                const SizedBox(height: 10),
                InlineMessage(
                  message: _error!,
                  severity: 'error',
                  onDismiss: () => setState(() => _error = null),
                ),
              ],
              const SizedBox(height: 12),
            ],
          ),
        ),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: CrawlStatTheme.gutter,
            ),
            child: switch (_segment) {
              _Segment.setup => _SetupForm(
                catalog: _catalog,
                busy: _busy,
                initialUrl: audit?['setup']['url'] as String?,
                onRun: (setup) => _run(path, setup),
              ),
              _Segment.journeys => const JourneysPanel(),
              _ when audit == null => const EmptyState(
                icon: Icons.science_outlined,
                title: 'No audit yet',
                message: 'Run one from Setup, or open a saved one above.',
              ),
              _Segment.overview => _Overview(
                view: _view!,
                catalog: _catalog,
                onFindings: () => setState(() => _segment = _Segment.findings),
              ),
              _Segment.findings => _Findings(
                view: _view!,
                catalog: _catalog,
                busy: _busy,
                onSave: (state) => _saveState(path, state),
              ),
              _Segment.checks => _Checks(view: _view!, catalog: _catalog),
              _Segment.retest => _Retest(
                view: _view!,
                busy: _busy,
                onRetest: () => _retest(path),
              ),
              _Segment.export => _Export(
                onReport: () => _export(path, csv: false),
                onCsv: () => _export(path, csv: true),
              ),
            },
          ),
        ),
      ],
    );
  }

  String _label(String vocabulary, Object? id) => _labelIn(_catalog, vocabulary, id);
}

/// What a free install sees: what the tab does, and where to unlock it. The
/// bridge refuses to run audits without Pro too, so this is the explanation,
/// not the lock.
class _ProGate extends StatelessWidget {
  const _ProGate();

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const ScreenHeader(
          title: 'UX Testing',
          subtitle: 'Test a page against its goal, prove each finding, fix, retest',
        ),
        Expanded(
          child: EmptyState(
            icon: Icons.workspace_premium_outlined,
            title: 'UX Testing is part of CrawlStat Pro',
            message:
                'Audit any page against what it is for: around 60 checks chosen '
                'for its type, findings ranked by impact with evidence and a '
                'recommended fix, an owner-and-due-date workflow, retests that '
                'show what you fixed, journeys, and a client report and '
                'developer CSV.',
            action: FilledButton.icon(
              onPressed: () => context.go(Routes.settings),
              icon: const Icon(Icons.key_outlined, size: 18),
              label: const Text('Upgrade or enter a licence key'),
            ),
          ),
        ),
      ],
    );
  }
}

// ── Helpers ────────────────────────────────────────────────────────────────

String _labelIn(_Json catalog, String vocabulary, Object? id) {
  for (final option in (catalog[vocabulary] as List).cast<_Json>()) {
    if (option['id'] == id) return option['label'] as String;
  }
  return '${id ?? ''}'.replaceAll('_', ' ');
}

String _date(int seconds) {
  final d = DateTime.fromMillisecondsSinceEpoch(seconds * 1000);
  String two(int n) => n.toString().padLeft(2, '0');
  return '${d.year}-${two(d.month)}-${two(d.day)} ${two(d.hour)}:${two(d.minute)}';
}

String _pct(num? value) => value == null ? '—' : '${(value * 100).round()}%';

/// Checks that carry a finding, highest priority first.
List<_Json> _findings(_Json view) {
  final checks = (view['audit']['checks'] as List).cast<_Json>();
  final found = checks.where((c) => c['finding'] != null).toList()
    ..sort(
      (a, b) => (b['finding']['priority'] as num).compareTo(
        a['finding']['priority'] as num,
      ),
    );
  return found;
}

_Json _stateFor(_Json view, String checkId) {
  for (final s in (view['states'] as List).cast<_Json>()) {
    if (s['check_id'] == checkId) return Map.of(s);
  }
  return {
    'check_id': checkId,
    'work_status': 'open',
    'owner': '',
    'due': '',
    'note': '',
    'flagged': false,
    'flag_reason': '',
    'severity_override': null,
    'override_reason': '',
    'retest': null,
    'updated_at': 0,
  };
}

Color _severityColor(BuildContext context, String? severity) {
  final p = CrawlStatTheme.palette(context);
  return switch (severity) {
    'critical' => p.error,
    'high' => p.accent,
    'medium' => p.warning,
    _ => p.notice,
  };
}

Color _statusColor(BuildContext context, String? status) {
  final p = CrawlStatTheme.palette(context);
  return switch (status) {
    'pass' || 'resolved' || 'verified' => p.success,
    'fail' => p.error,
    'needs_review' || 'partial' || 'ready_to_retest' => p.warning,
    'in_progress' || 'planned' => p.accent,
    _ => p.notice,
  };
}

// ── Summary strip ──────────────────────────────────────────────────────────

class _SummaryStrip extends StatelessWidget {
  const _SummaryStrip({required this.view});

  final _Json view;

  @override
  Widget build(BuildContext context) {
    final s = view['summary'] as _Json;
    final p = CrawlStatTheme.palette(context);
    final rate = (view['comparison'] as _Json?)?['retest_pass_rate'] as num?;
    final tiles = [
      StatTile(
        label: 'Critical',
        value: '${s['critical']}',
        color: (s['critical'] as int) > 0 ? p.error : null,
      ),
      StatTile(
        label: 'High',
        value: '${s['high']}',
        color: (s['high'] as int) > 0 ? p.accent : null,
      ),
      StatTile(
        label: 'Passed',
        value: '${s['pass']} / ${s['selected']}',
        color: p.success,
      ),
      StatTile(
        label: 'Not tested',
        value: '${(s['not_tested'] as int) + (s['blocked'] as int)}',
        hint: '${s['not_tested']} not tested, ${s['blocked']} blocked — never counted as passes',
      ),
      StatTile(
        label: 'Coverage',
        value: _pct(s['coverage'] as num),
        hint: 'Share of selected checks that actually ran',
      ),
      StatTile(
        label: 'Confidence',
        value: _pct(s['confidence'] as num),
        hint: 'Share of checks that ran on verified evidence',
      ),
      StatTile(
        label: 'Retest pass rate',
        value: _pct(rate),
        hint: rate == null ? 'Retest the page to measure fixes' : null,
      ),
    ];
    return Row(
      children: [
        for (final (i, tile) in tiles.indexed) ...[
          if (i > 0) const SizedBox(width: 10),
          Expanded(child: tile),
        ],
      ],
    );
  }
}

class _AuditPicker extends StatelessWidget {
  const _AuditPicker({
    required this.audits,
    required this.selected,
    required this.onOpen,
  });

  final List<_Json> audits;
  final int? selected;
  final ValueChanged<int> onOpen;

  @override
  Widget build(BuildContext context) {
    return PopupMenuButton<int>(
      tooltip: 'Saved audits',
      onSelected: onOpen,
      itemBuilder: (_) => [
        for (final a in audits)
          PopupMenuItem(
            value: a['id'] as int,
            child: Row(
              children: [
                Icon(
                  a['id'] == selected ? Icons.check : Icons.history,
                  size: 16,
                ),
                const SizedBox(width: 8),
                Flexible(
                  child: Text(
                    '${a['kind'] == 'retest' ? '↻ ' : ''}${a['url']}',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 12),
                Text(
                  '${_date(a['tested_at'] as int)} · ${a['critical']}C ${a['high']}H',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
      ],
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.history, size: 18),
            const SizedBox(width: 6),
            Text('${audits.length} saved'),
            const Icon(Icons.arrow_drop_down, size: 18),
          ],
        ),
      ),
    );
  }
}

// ── Setup ──────────────────────────────────────────────────────────────────

class _SetupForm extends ConsumerStatefulWidget {
  const _SetupForm({
    required this.catalog,
    required this.busy,
    required this.onRun,
    this.initialUrl,
  });

  final _Json catalog;
  final bool busy;
  final String? initialUrl;
  final ValueChanged<_Json> onRun;

  @override
  ConsumerState<_SetupForm> createState() => _SetupFormState();
}

class _SetupFormState extends ConsumerState<_SetupForm> {
  late final _url = TextEditingController(text: widget.initialUrl ?? '');
  final _role = TextEditingController();
  final _needs = TextEditingController();
  final _constraints = TextEditingController();
  final _context = TextEditingController();
  String _pageType = '';
  String _goal = '';
  String _device = 'desktop';
  String _depth = 'standard';
  String _familiarity = 'new';
  String _traffic = 'unknown';
  bool _allowFetch = true;

  @override
  void dispose() {
    for (final c in [_url, _role, _needs, _constraints, _context]) {
      c.dispose();
    }
    super.dispose();
  }

  void _submit() {
    widget.onRun({
      'url': _url.text.trim(),
      'page_type': _pageType.isEmpty ? null : _pageType,
      'goal': _goal.isEmpty ? null : _goal,
      'device': _device,
      'depth': _depth,
      'constraints': _constraints.text.trim(),
      'context': _context.text.trim(),
      'persona': ref.read(settingsProvider).defaultPersona,
      'allow_fetch': _allowFetch,
      'audience': {
        'role': _role.text.trim(),
        'familiarity': _familiarity,
        'needs': _needs.text.trim(),
        'traffic_source': _traffic,
      },
    });
  }

  Widget _dropdown(
    String label,
    String value,
    List<(String, String)> options,
    ValueChanged<String> onChanged,
  ) {
    return DropdownButtonFormField<String>(
      initialValue: value,
      isExpanded: true,
      decoration: InputDecoration(labelText: label, isDense: true),
      items: [
        for (final (id, text) in options)
          DropdownMenuItem(value: id, child: Text(text)),
      ],
      onChanged: (v) => setState(() => onChanged(v ?? value)),
    );
  }

  List<(String, String)> _options(String vocabulary, {String? auto}) => [
    if (auto != null) ('', auto),
    for (final o in (widget.catalog[vocabulary] as List).cast<_Json>())
      (o['id'] as String, o['label'] as String),
  ];

  @override
  Widget build(BuildContext context) {
    final questions = _pageType.isEmpty
        ? const <String>[]
        : ((widget.catalog['questions'] as _Json)[_pageType] as List)
              .cast<String>();
    final library = (widget.catalog['library'] as List).cast<_Json>();
    final planned = library.where((d) {
      const order = ['quick', 'standard', 'deep'];
      final types = (d['page_types'] as List).cast<String>();
      return order.indexOf(d['tier'] as String) <= order.indexOf(_depth) &&
          (types.isEmpty || types.contains(_pageType));
    }).length;
    Widget row(List<Widget> children) => Padding(
      padding: const EdgeInsets.only(bottom: 14),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final (i, c) in children.indexed) ...[
            if (i > 0) const SizedBox(width: 14),
            Expanded(child: c),
          ],
        ],
      ),
    );

    return ListView(
      padding: const EdgeInsets.only(bottom: CrawlStatTheme.gutter),
      children: [
        SectionCard(
          title: 'What to test',
          subtitle:
              'The page is read from the open crawl when it has the HTML, '
              'otherwise fetched once (after checking robots.txt).',
          child: Column(
            children: [
              row([
                TextField(
                  controller: _url,
                  decoration: const InputDecoration(
                    labelText: 'Page URL',
                    hintText: 'https://example.com/pricing',
                    isDense: true,
                  ),
                  onSubmitted: (_) => _submit(),
                ),
              ]),
              row([
                _dropdown(
                  'Page type',
                  _pageType,
                  _options('page_types', auto: 'Detect automatically'),
                  (v) => _pageType = v,
                ),
                _dropdown(
                  'Primary goal',
                  _goal,
                  _options('goals', auto: 'Infer from page type'),
                  (v) => _goal = v,
                ),
              ]),
              row([
                _dropdown(
                  'Device',
                  _device,
                  _options('devices'),
                  (v) => _device = v,
                ),
                _dropdown(
                  'Depth',
                  _depth,
                  _options('depths'),
                  (v) => _depth = v,
                ),
              ]),
            ],
          ),
        ),
        const SizedBox(height: 14),
        SectionCard(
          title: 'Who it is for',
          subtitle: 'Used to phrase findings and recommendations.',
          child: Column(
            children: [
              row([
                TextField(
                  controller: _role,
                  decoration: const InputDecoration(
                    labelText: 'Audience',
                    hintText: 'e.g. finance managers at small firms',
                    isDense: true,
                  ),
                ),
                _dropdown('Familiarity', _familiarity, const [
                  ('new', 'First visit'),
                  ('returning', 'Returning'),
                  ('expert', 'Expert'),
                ], (v) => _familiarity = v),
                _dropdown('Arrives from', _traffic, const [
                  ('unknown', 'Unknown'),
                  ('search', 'Search'),
                  ('ads', 'Ads'),
                  ('social', 'Social'),
                  ('email', 'Email'),
                  ('referral', 'Referral'),
                  ('direct', 'Direct'),
                ], (v) => _traffic = v),
              ]),
              row([
                TextField(
                  controller: _needs,
                  decoration: const InputDecoration(
                    labelText: 'What they need (optional)',
                    isDense: true,
                  ),
                ),
              ]),
              row([
                TextField(
                  controller: _constraints,
                  decoration: const InputDecoration(
                    labelText: 'Constraints (optional)',
                    hintText: 'e.g. brand colours fixed, no new pages',
                    isDense: true,
                  ),
                ),
                TextField(
                  controller: _context,
                  decoration: const InputDecoration(
                    labelText: 'Context (optional)',
                    hintText: 'e.g. bounce rate rose after redesign',
                    isDense: true,
                  ),
                ),
              ]),
            ],
          ),
        ),
        const SizedBox(height: 14),
        SectionCard(
          title: 'Plan',
          subtitle: _pageType.isEmpty
              ? 'Shared baseline plus the checks for the detected page type.'
              : '$planned checks selected for this page type and depth.',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (questions.isNotEmpty) ...[
                Text(
                  'Questions this page type must answer',
                  style: Theme.of(context).textTheme.titleSmall,
                ),
                const SizedBox(height: 6),
                for (final q in questions)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: Text('•  $q'),
                  ),
                const SizedBox(height: 10),
              ],
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                value: _allowFetch,
                onChanged: (v) => setState(() => _allowFetch = v),
                title: const Text('Allow one live fetch'),
                subtitle: const Text(
                  'If the crawl did not keep this page, fetch it once. '
                  'robots.txt is always respected; forms are never submitted.',
                ),
              ),
              const SizedBox(height: 10),
              Align(
                alignment: Alignment.centerLeft,
                child: FilledButton.icon(
                  onPressed: widget.busy ? null : _submit,
                  icon: const Icon(Icons.play_arrow, size: 18),
                  label: Text(widget.busy ? 'Testing…' : 'Run audit'),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

// ── Overview ───────────────────────────────────────────────────────────────

class _Overview extends StatelessWidget {
  const _Overview({
    required this.view,
    required this.catalog,
    required this.onFindings,
  });

  final _Json view;
  final _Json catalog;
  final VoidCallback onFindings;

  @override
  Widget build(BuildContext context) {
    final audit = view['audit'] as _Json;
    final u = audit['understanding'] as _Json;
    final c = audit['classification'] as _Json;
    final checks = (audit['checks'] as List).cast<_Json>();
    final top = _findings(view).take(5).toList();
    final text = Theme.of(context).textTheme;

    Widget fact(String label, Object? value) {
      final shown = switch (value) {
        null => '—',
        List l when l.isEmpty => '—',
        List l => l.join(' · '),
        _ => '$value',
      };
      return Padding(
        padding: const EdgeInsets.only(bottom: 8),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              width: 130,
              child: Text(label, style: text.bodySmall),
            ),
            Expanded(child: SelectableText(shown)),
          ],
        ),
      );
    }

    final categories = <String, Map<String, int>>{};
    for (final check in checks) {
      final counts = categories.putIfAbsent(
        check['category'] as String,
        () => {},
      );
      counts.update(check['status'] as String, (n) => n + 1, ifAbsent: () => 1);
    }

    return ListView(
      padding: const EdgeInsets.only(bottom: CrawlStatTheme.gutter),
      children: [
        SectionCard(
          title: 'What this page is',
          subtitle: 'Read from the markup. Page type and intent are estimates.',
          trailing: TonePill(
            '${_labelIn(catalog, 'page_types', c['used'])}'
            '${c['overridden'] == true ? ' (your choice)' : ' · ${_pct(c['confidence'] as num)} sure'}',
            color: CrawlStatTheme.palette(context).accent,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              fact('Intent', u['intent']),
              fact(
                'Goal',
                '${_labelIn(catalog, 'goals', audit['goal'])}'
                    '${audit['goal_inferred'] == true ? ' (inferred)' : ''}',
              ),
              fact('Title', u['title']),
              fact('Main heading', u['h1']),
              fact('Offer', u['offer']),
              fact('Main action', u['main_cta']),
              fact('Other actions', u['other_ctas']),
              fact('Forms', u['forms']),
              fact('Navigation', u['nav']),
              fact('Trust signals', u['trust']),
              fact('Words', u['word_count']),
              fact('Why this type', c['reasons']),
              fact('Evidence', '${audit['source']} HTML · no browser'),
            ],
          ),
        ),
        const SizedBox(height: 14),
        SectionCard(
          title: 'Fix these first',
          subtitle: 'Priority = impact × reach × confidence ÷ effort. Critical escalates.',
          trailing: TextButton(
            onPressed: onFindings,
            child: const Text('All findings'),
          ),
          child: top.isEmpty
              ? const Text('No findings.')
              : Column(
                  children: [
                    for (final check in top)
                      _FindingLine(check: check, catalog: catalog),
                  ],
                ),
        ),
        const SizedBox(height: 14),
        SectionCard(
          title: 'By area',
          subtitle: 'Each status counted separately.',
          child: Column(
            children: [
              for (final entry in categories.entries)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Row(
                    children: [
                      SizedBox(
                        width: 130,
                        child: Text(_labelIn(catalog, 'categories', entry.key)),
                      ),
                      Expanded(
                        child: Wrap(
                          spacing: 6,
                          runSpacing: 6,
                          children: [
                            for (final s in [
                              'fail',
                              'needs_review',
                              'pass',
                              'not_tested',
                              'blocked',
                            ])
                              if ((entry.value[s] ?? 0) > 0)
                                TonePill(
                                  '${entry.value[s]} ${_labelIn(catalog, 'statuses', s).toLowerCase()}',
                                  color: _statusColor(context, s),
                                ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 14),
        SectionCard(
          title: 'Limitations',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              for (final l in (audit['limitations'] as List).cast<String>())
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Text('•  $l', style: text.bodySmall),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

class _FindingLine extends StatelessWidget {
  const _FindingLine({required this.check, required this.catalog});

  final _Json check;
  final _Json catalog;

  @override
  Widget build(BuildContext context) {
    final f = check['finding'] as _Json;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(
        children: [
          SizedBox(
            width: 92,
            child: TonePill(
              _labelIn(catalog, 'severities', f['severity']),
              color: _severityColor(context, f['severity'] as String),
              dot: true,
            ),
          ),
          const SizedBox(width: 10),
          Expanded(child: Text(f['title'] as String)),
          Text(
            (f['priority'] as num).toStringAsFixed(0),
            style: CrawlStatTheme.mono(context, size: 12),
          ),
        ],
      ),
    );
  }
}

// ── Findings ───────────────────────────────────────────────────────────────

class _Findings extends StatefulWidget {
  const _Findings({
    required this.view,
    required this.catalog,
    required this.busy,
    required this.onSave,
  });

  final _Json view;
  final _Json catalog;
  final bool busy;
  final ValueChanged<_Json> onSave;

  @override
  State<_Findings> createState() => _FindingsState();
}

class _FindingsState extends State<_Findings> {
  final Set<String> _severities = {};
  String _evidence = '';
  String _category = '';
  String _work = '';
  String _device = '';
  String _region = '';
  bool _showFlagged = false;

  @override
  Widget build(BuildContext context) {
    final all = _findings(widget.view);
    final shown = all.where((check) {
      final f = check['finding'] as _Json;
      final state = _stateFor(widget.view, check['id'] as String);
      final severity = (state['severity_override'] ?? f['severity']) as String;
      if (_severities.isNotEmpty && !_severities.contains(severity)) {
        return false;
      }
      if (_evidence.isNotEmpty && check['evidence']['status'] != _evidence) {
        return false;
      }
      if (_category.isNotEmpty && check['category'] != _category) return false;
      if (_work.isNotEmpty && state['work_status'] != _work) return false;
      if (_device.isNotEmpty && f['devices'] != _device) return false;
      if (_region.isNotEmpty && f['region'] != _region) return false;
      if (!_showFlagged && state['flagged'] == true) return false;
      return true;
    }).toList();
    final regions = {for (final c in all) c['finding']['region'] as String};
    final devices = {for (final c in all) c['finding']['devices'] as String};

    Widget menu(
      String hint,
      String value,
      List<(String, String)> options,
      ValueChanged<String> onChanged,
    ) => DropdownButton<String>(
      value: value,
      isDense: true,
      underline: const SizedBox.shrink(),
      items: [
        DropdownMenuItem(value: '', child: Text(hint)),
        for (final (id, label) in options)
          DropdownMenuItem(value: id, child: Text(label)),
      ],
      onChanged: (v) => setState(() => onChanged(v ?? '')),
    );
    List<(String, String)> vocab(String name) => [
      for (final o in (widget.catalog[name] as List).cast<_Json>())
        (o['id'] as String, o['label'] as String),
    ];

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Wrap(
          spacing: 6,
          runSpacing: 6,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            for (final (id, label) in vocab('severities'))
              FilterChip(
                label: Text(label),
                selected: _severities.contains(id),
                onSelected: (on) => setState(
                  () => on ? _severities.add(id) : _severities.remove(id),
                ),
              ),
            const SizedBox(width: 8),
            menu('Any evidence', _evidence, vocab('evidence'), (v) => _evidence = v),
            menu('Any area', _category, vocab('categories'), (v) => _category = v),
            menu('Any region', _region, [for (final r in regions) (r, r)], (v) => _region = v),
            menu('Any device', _device, [for (final d in devices) (d, d)], (v) => _device = v),
            menu('Any status', _work, vocab('work'), (v) => _work = v),
            FilterChip(
              label: const Text('Show flagged'),
              selected: _showFlagged,
              onSelected: (v) => setState(() => _showFlagged = v),
            ),
          ],
        ),
        const SizedBox(height: 10),
        Expanded(
          child: shown.isEmpty
              ? const EmptyState(
                  icon: Icons.filter_alt_off_outlined,
                  title: 'Nothing matches',
                  message: 'Loosen the filters to see more findings.',
                )
              : ListView.separated(
                  padding: const EdgeInsets.only(bottom: CrawlStatTheme.gutter),
                  itemCount: shown.length,
                  separatorBuilder: (_, _) => const SizedBox(height: 10),
                  itemBuilder: (_, i) => _FindingCard(
                    key: ValueKey(shown[i]['id']),
                    check: shown[i],
                    state: _stateFor(widget.view, shown[i]['id'] as String),
                    activity: (widget.view['activity'] as List)
                        .cast<_Json>()
                        .where((a) => a['check_id'] == shown[i]['id'])
                        .toList(),
                    catalog: widget.catalog,
                    busy: widget.busy,
                    onSave: widget.onSave,
                  ),
                ),
        ),
      ],
    );
  }
}

class _FindingCard extends StatefulWidget {
  const _FindingCard({
    super.key,
    required this.check,
    required this.state,
    required this.activity,
    required this.catalog,
    required this.busy,
    required this.onSave,
  });

  final _Json check;
  final _Json state;
  final List<_Json> activity;
  final _Json catalog;
  final bool busy;
  final ValueChanged<_Json> onSave;

  @override
  State<_FindingCard> createState() => _FindingCardState();
}

class _FindingCardState extends State<_FindingCard> {
  bool _open = false;
  late _Json _draft = Map.of(widget.state);
  late final _owner = TextEditingController(text: widget.state['owner'] as String);
  late final _due = TextEditingController(text: widget.state['due'] as String);
  late final _note = TextEditingController(text: widget.state['note'] as String);
  late final _flagReason = TextEditingController(
    text: widget.state['flag_reason'] as String,
  );
  late final _overrideReason = TextEditingController(
    text: widget.state['override_reason'] as String,
  );

  @override
  void didUpdateWidget(_FindingCard old) {
    super.didUpdateWidget(old);
    if (old.state['updated_at'] != widget.state['updated_at']) {
      _draft = Map.of(widget.state);
    }
  }

  @override
  void dispose() {
    for (final c in [_owner, _due, _note, _flagReason, _overrideReason]) {
      c.dispose();
    }
    super.dispose();
  }

  void _save() {
    widget.onSave({
      ..._draft,
      'owner': _owner.text.trim(),
      'due': _due.text.trim(),
      'note': _note.text.trim(),
      'flag_reason': _flagReason.text.trim(),
      'override_reason': _overrideReason.text.trim(),
    });
  }

  @override
  Widget build(BuildContext context) {
    final check = widget.check;
    final f = check['finding'] as _Json;
    final r = f['recommendation'] as _Json;
    final e = check['evidence'] as _Json;
    final text = Theme.of(context).textTheme;
    final state = widget.state;
    final override = state['severity_override'] as String?;
    final severity = override ?? f['severity'] as String;
    String label(String vocabulary, Object? id) =>
        _labelIn(widget.catalog, vocabulary, id);

    Widget heading(String title) => Padding(
      padding: const EdgeInsets.only(top: 14, bottom: 6),
      child: Text(title, style: text.titleSmall),
    );
    Widget code(String body) => Container(
      width: double.infinity,
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(8),
      ),
      child: SelectableText(body, style: CrawlStatTheme.mono(context, size: 12)),
    );

    return SectionCard(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            onTap: () => setState(() => _open = !_open),
            child: Row(
              children: [
                TonePill(
                  label('severities', severity) + (override != null ? ' *' : ''),
                  color: _severityColor(context, severity),
                  dot: true,
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    f['title'] as String,
                    style: text.titleMedium?.copyWith(
                      decoration: state['flagged'] == true
                          ? TextDecoration.lineThrough
                          : null,
                    ),
                  ),
                ),
                TonePill(
                  label('statuses', check['status']),
                  color: _statusColor(context, check['status'] as String),
                ),
                const SizedBox(width: 6),
                TonePill(
                  '${label('evidence', e['status'])} · ${check['method']}',
                  color: _statusColor(context, e['status'] as String),
                ),
                const SizedBox(width: 6),
                TonePill(
                  label('work', state['work_status']),
                  color: _statusColor(context, state['work_status'] as String),
                ),
                const SizedBox(width: 10),
                Tooltip(
                  message:
                      'Impact ${f['impact']} × reach ${f['reach']} × confidence '
                      '${f['confidence']} ÷ effort ${f['effort']}'
                      '${f['severity'] == 'critical' ? ' — critical, escalated' : ''}',
                  child: Text(
                    'P ${(f['priority'] as num).toStringAsFixed(0)}',
                    style: CrawlStatTheme.mono(context, size: 12),
                  ),
                ),
                Icon(_open ? Icons.expand_less : Icons.expand_more),
              ],
            ),
          ),
          const SizedBox(height: 6),
          Text(f['explanation'] as String),
          if (_open) ...[
            heading('Why it matters'),
            Text(
              '${f['consequence']}  User goal: ${f['user_goal']}.  '
              'Region: ${f['region']} · Devices: ${f['devices']}',
            ),
            heading('Evidence'),
            Text(
              '${label('evidence', e['status'])} — ${e['limitation']}',
              style: text.bodySmall,
            ),
            const SizedBox(height: 6),
            for (final o in (e['observed'] as List).cast<String>())
              Padding(
                padding: const EdgeInsets.only(bottom: 3),
                child: SelectableText('•  $o'),
              ),
            if (e['dom_clue'] != null) ...[
              const SizedBox(height: 4),
              Text('Where to look: ${e['dom_clue']}', style: text.bodySmall),
            ],
            if ((e['steps'] as List).isNotEmpty) ...[
              const SizedBox(height: 4),
              Text(
                'Reproduce: ${(e['steps'] as List).join(' → ')}',
                style: text.bodySmall,
              ),
            ],
            Text(
              '${e['url']} · ${e['viewport']} · ${e['browser']} · ${_date(e['tested_at'] as int)}',
              style: text.bodySmall,
            ),
            heading('Recommendation'),
            Text(r['change'] as String),
            const SizedBox(height: 4),
            Text('Why: ${r['why']}', style: text.bodySmall),
            if (r['before'] != null) ...[
              heading('Before'),
              code(r['before'] as String),
            ],
            if (r['after'] != null) ...[
              heading(r['before'] != null ? 'After (example)' : 'Example'),
              code(r['after'] as String),
            ],
            if ((r['notes'] as String).isNotEmpty) ...[
              const SizedBox(height: 6),
              Text('Note: ${r['notes']}', style: text.bodySmall),
            ],
            heading('Done when'),
            for (final a in (r['acceptance'] as List).cast<String>())
              Text('☐  $a'),
            const SizedBox(height: 4),
            Text('Validate: ${r['validation']}', style: text.bodySmall),
            heading('Workflow'),
            Wrap(
              spacing: 12,
              runSpacing: 10,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                SizedBox(
                  width: 190,
                  child: DropdownButtonFormField<String>(
                    initialValue: _draft['work_status'] as String,
                    isDense: true,
                    decoration: const InputDecoration(labelText: 'Status'),
                    items: [
                      for (final o in (widget.catalog['work'] as List).cast<_Json>())
                        DropdownMenuItem(
                          value: o['id'] as String,
                          child: Text(o['label'] as String),
                        ),
                    ],
                    onChanged: (v) => setState(() => _draft['work_status'] = v),
                  ),
                ),
                SizedBox(
                  width: 170,
                  child: TextField(
                    controller: _owner,
                    decoration: const InputDecoration(
                      labelText: 'Owner',
                      isDense: true,
                    ),
                  ),
                ),
                SizedBox(
                  width: 150,
                  child: TextField(
                    controller: _due,
                    decoration: const InputDecoration(
                      labelText: 'Due (YYYY-MM-DD)',
                      isDense: true,
                    ),
                    inputFormatters: [
                      FilteringTextInputFormatter.allow(RegExp(r'[0-9-]')),
                    ],
                  ),
                ),
                SizedBox(
                  width: 170,
                  child: DropdownButtonFormField<String>(
                    initialValue: _draft['severity_override'] as String? ?? '',
                    isDense: true,
                    decoration: const InputDecoration(
                      labelText: 'Override severity',
                    ),
                    items: [
                      const DropdownMenuItem(value: '', child: Text('No override')),
                      for (final o in (widget.catalog['severities'] as List).cast<_Json>())
                        DropdownMenuItem(
                          value: o['id'] as String,
                          child: Text(o['label'] as String),
                        ),
                    ],
                    onChanged: (v) => setState(
                      () => _draft['severity_override'] =
                          (v == null || v.isEmpty) ? null : v,
                    ),
                  ),
                ),
                if (_draft['severity_override'] != null)
                  SizedBox(
                    width: 260,
                    child: TextField(
                      controller: _overrideReason,
                      decoration: const InputDecoration(
                        labelText: 'Reason for override',
                        isDense: true,
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _note,
              minLines: 1,
              maxLines: 3,
              decoration: const InputDecoration(labelText: 'Note', isDense: true),
            ),
            const SizedBox(height: 6),
            Row(
              children: [
                Checkbox(
                  value: _draft['flagged'] == true,
                  onChanged: (v) => setState(() => _draft['flagged'] = v ?? false),
                ),
                const Text('This finding is wrong'),
                const SizedBox(width: 10),
                if (_draft['flagged'] == true)
                  Expanded(
                    child: TextField(
                      controller: _flagReason,
                      decoration: const InputDecoration(
                        labelText: 'Why? (kept out of the client report)',
                        isDense: true,
                      ),
                    ),
                  )
                else
                  const Spacer(),
                const SizedBox(width: 10),
                FilledButton(
                  onPressed: widget.busy ? null : _save,
                  child: const Text('Save'),
                ),
              ],
            ),
            if (state['retest'] != null)
              Padding(
                padding: const EdgeInsets.only(top: 6),
                child: Row(
                  children: [
                    const Text('Last retest: '),
                    TonePill(
                      label('statuses', state['retest']),
                      color: _statusColor(context, state['retest'] as String),
                    ),
                  ],
                ),
              ),
            if (widget.activity.isNotEmpty) ...[
              heading('Activity'),
              for (final a in widget.activity)
                Text(
                  '${_date(a['at'] as int)}  ${a['what']}',
                  style: text.bodySmall,
                ),
            ],
          ],
        ],
      ),
    );
  }
}

// ── Checks ─────────────────────────────────────────────────────────────────

class _Checks extends StatefulWidget {
  const _Checks({required this.view, required this.catalog});

  final _Json view;
  final _Json catalog;

  @override
  State<_Checks> createState() => _ChecksState();
}

class _ChecksState extends State<_Checks> {
  final Set<String> _statuses = {};

  @override
  Widget build(BuildContext context) {
    final checks = (widget.view['audit']['checks'] as List)
        .cast<_Json>()
        .where((c) => _statuses.isEmpty || _statuses.contains(c['status']))
        .toList();
    final byCategory = <String, List<_Json>>{};
    for (final c in checks) {
      byCategory.putIfAbsent(c['category'] as String, () => []).add(c);
    }
    final text = Theme.of(context).textTheme;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Wrap(
          spacing: 6,
          children: [
            for (final o in (widget.catalog['statuses'] as List).cast<_Json>())
              FilterChip(
                label: Text(o['label'] as String),
                selected: _statuses.contains(o['id']),
                onSelected: (on) => setState(
                  () => on
                      ? _statuses.add(o['id'] as String)
                      : _statuses.remove(o['id']),
                ),
              ),
          ],
        ),
        const SizedBox(height: 10),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.only(bottom: CrawlStatTheme.gutter),
            children: [
              for (final entry in byCategory.entries) ...[
                SectionCard(
                  title: _labelIn(widget.catalog, 'categories', entry.key),
                  padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
                  child: Column(
                    children: [
                      for (final c in entry.value)
                        Theme(
                          data: Theme.of(context).copyWith(
                            dividerColor: Colors.transparent,
                          ),
                          child: ExpansionTile(
                            tilePadding: EdgeInsets.zero,
                            dense: true,
                            leading: SizedBox(
                              width: 104,
                              child: TonePill(
                                _labelIn(widget.catalog, 'statuses', c['status']),
                                color: _statusColor(context, c['status'] as String),
                              ),
                            ),
                            title: Text(c['title'] as String),
                            subtitle: Text(
                              c['summary'] as String,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                            ),
                            trailing: Text(
                              '${c['method']} · ${c['baseline'] == true ? 'baseline' : 'page type'}',
                              style: text.bodySmall,
                            ),
                            expandedCrossAxisAlignment: CrossAxisAlignment.start,
                            childrenPadding: const EdgeInsets.only(
                              left: 116,
                              bottom: 10,
                            ),
                            children: [
                              Text('Question: ${c['question']}'),
                              const SizedBox(height: 4),
                              SelectableText(c['summary'] as String),
                              for (final o in (c['evidence']['observed'] as List).cast<String>())
                                SelectableText('•  $o', style: text.bodySmall),
                              const SizedBox(height: 4),
                              Text(
                                'Evidence: ${_labelIn(widget.catalog, 'evidence', c['evidence']['status'])}'
                                ' — ${c['evidence']['limitation']}',
                                style: text.bodySmall,
                              ),
                            ],
                          ),
                        ),
                    ],
                  ),
                ),
                const SizedBox(height: 12),
              ],
            ],
          ),
        ),
      ],
    );
  }
}

// ── Retest ─────────────────────────────────────────────────────────────────

class _Retest extends StatelessWidget {
  const _Retest({
    required this.view,
    required this.busy,
    required this.onRetest,
  });

  final _Json view;
  final bool busy;
  final VoidCallback onRetest;

  @override
  Widget build(BuildContext context) {
    final c = view['comparison'] as _Json?;
    final p = CrawlStatTheme.palette(context);
    final button = FilledButton.icon(
      onPressed: busy ? null : onRetest,
      icon: const Icon(Icons.refresh, size: 18),
      label: const Text('Retest now'),
    );
    if (c == null) {
      return EmptyState(
        icon: Icons.compare_arrows,
        title: 'Not retested yet',
        message:
            'After shipping fixes, re-crawl (or allow a live fetch) and retest. '
            'Each finding gets the new result; fixed ones are resolved.',
        action: button,
      );
    }
    final rows = (c['rows'] as List).cast<_Json>();
    final changed = rows.where((r) => r['change'] != 'unchanged').toList();
    Color tone(String change) => switch (change) {
      'fixed' => p.success,
      'regressed' => p.error,
      'still_failing' => p.warning,
      _ => p.notice,
    };
    String status(Object? s) => s == null ? '—' : '$s'.replaceAll('_', ' ');

    return ListView(
      padding: const EdgeInsets.only(bottom: CrawlStatTheme.gutter),
      children: [
        Row(
          children: [
            Expanded(
              child: StatTile(label: 'Fixed', value: '${c['fixed']}', color: p.success),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: StatTile(
                label: 'Still failing',
                value: '${c['still_failing']}',
                color: p.warning,
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: StatTile(label: 'Regressed', value: '${c['regressed']}', color: p.error),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: StatTile(
                label: 'Retest pass rate',
                value: _pct(c['retest_pass_rate'] as num?),
              ),
            ),
          ],
        ),
        const SizedBox(height: 14),
        SectionCard(
          title: 'Before and after',
          subtitle:
              'Audit #${c['before_id']} → #${c['after_id']}. Checks that could '
              'not run on either side are not comparable, not passed.',
          trailing: button,
          child: changed.isEmpty
              ? const Text('Nothing changed between the two runs.')
              : Column(
                  children: [
                    for (final r in changed)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 6),
                        child: Row(
                          children: [
                            Expanded(child: Text(r['title'] as String)),
                            SizedBox(width: 110, child: Text(status(r['before']))),
                            const Icon(Icons.arrow_forward, size: 14),
                            const SizedBox(width: 8),
                            SizedBox(width: 110, child: Text(status(r['after']))),
                            SizedBox(
                              width: 120,
                              child: TonePill(
                                status(r['change']),
                                color: tone(r['change'] as String),
                              ),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
        ),
      ],
    );
  }
}

// ── Export ─────────────────────────────────────────────────────────────────

class _Export extends StatelessWidget {
  const _Export({required this.onReport, required this.onCsv});

  final VoidCallback onReport;
  final VoidCallback onCsv;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.only(bottom: CrawlStatTheme.gutter),
      children: [
        SectionCard(
          title: 'Client report',
          subtitle:
              'Markdown: summary, findings in priority order with evidence and '
              'recommendations, journeys, accessibility, before/after, and what '
              'was not tested. Findings flagged as wrong are left out; overrides '
              'are shown as overrides.',
          trailing: FilledButton.icon(
            onPressed: onReport,
            icon: const Icon(Icons.description_outlined, size: 18),
            label: const Text('Save report'),
          ),
          child: const SizedBox.shrink(),
        ),
        const SizedBox(height: 14),
        SectionCard(
          title: 'Developer CSV',
          subtitle:
              'One row per finding: severity, priority factors, evidence, DOM '
              'clue, change, acceptance criteria, owner, due date and retest '
              'result — ready for a tracker.',
          trailing: FilledButton.icon(
            onPressed: onCsv,
            icon: const Icon(Icons.table_chart_outlined, size: 18),
            label: const Text('Save CSV'),
          ),
          child: const SizedBox.shrink(),
        ),
        const SizedBox(height: 14),
        const InlineMessage(
          severity: 'info',
          message:
              'Emails and tokens in URLs are masked in both exports. Automated '
              'checks are not a full accessibility audit and do not establish '
              'legal compliance.',
        ),
      ],
    );
  }
}
