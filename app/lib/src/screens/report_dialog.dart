part of 'projects_screen.dart';

/// Brand colours offered for a report, the first being the report's own.
/// `''` means "the report's default", so a saved choice of it follows the
/// default if that ever changes.
const reportAccents = [
  ('', 'Indigo', Color(0xFF4F46E5)),
  ('#FF6A14', 'Ember', Color(0xFFFF6A14)),
  ('#2563EB', 'Ocean', Color(0xFF2563EB)),
  ('#0F9F8F', 'Teal', Color(0xFF0F9F8F)),
  ('#16A34A', 'Forest', Color(0xFF16A34A)),
  ('#E11D48', 'Rose', Color(0xFFE11D48)),
  ('#1F2937', 'Graphite', Color(0xFF1F2937)),
];

final _hex = RegExp(r'^#?([0-9a-fA-F]{6})$');

/// `#rrggbb` for a typed colour, with or without the `#`, or null.
String? normaliseHex(String text) {
  final match = _hex.firstMatch(text.trim());
  return match == null ? null : '#${match.group(1)!.toUpperCase()}';
}

Color? _colourOf(String hex) {
  final value = normaliseHex(hex);
  return value == null
      ? null
      : Color(0xFF000000 | int.parse(value.substring(1), radix: 16));
}

Future<void> showReportDialog(BuildContext context, ProjectInfo project) =>
    showDialog<void>(
      context: context,
      builder: (context) => ReportDialog(project: project),
    );

/// The button in a crawl's header, a circle like its neighbours so a
/// narrow window keeps room for the site's name.
class _ReportButton extends StatelessWidget {
  const _ReportButton({required this.project, required this.size});

  final ProjectInfo project;
  final double size;

  @override
  Widget build(BuildContext context) => SoftCircleButton(
    icon: AppIcon.report,
    size: size,
    iconSize: size * 0.42,
    tooltip: 'Export a client report',
    onPressed: () => showReportDialog(context, project),
  );
}

/// Export a client report: who it is from, in what colour, what goes in it,
/// and as what — beside a live sketch of the cover, so the choices show
/// rather than being described.
class ReportDialog extends ConsumerStatefulWidget {
  const ReportDialog({super.key, required this.project});

  final ProjectInfo project;

  @override
  ConsumerState<ReportDialog> createState() => _ReportDialogState();
}

class _ReportDialogState extends ConsumerState<ReportDialog> {
  late final TextEditingController _preparedBy;
  late final TextEditingController _customHex;
  late String _accent;
  late bool _pdf;
  bool _compare = true;
  bool _summary = true;
  bool _working = false;

  /// Why the last export failed, shown in the dialog rather than in a
  /// snackbar behind it.
  String? _error;

  @override
  void initState() {
    super.initState();
    final settings = ref.read(settingsProvider);
    _preparedBy = TextEditingController(text: settings.reportPreparedBy);
    _accent = normaliseHex(settings.reportAccent) ?? '';
    final preset = reportAccents.any((a) => a.$1 == _accent);
    _customHex = TextEditingController(text: preset ? '' : _accent);
    _pdf = ref.read(chromePathProvider) != null;
  }

  @override
  void dispose() {
    _preparedBy.dispose();
    _customHex.dispose();
    super.dispose();
  }

  Color get _colour => _colourOf(_accent) ?? reportAccents.first.$3;

  /// What the local model wrote on the Plan screen, which becomes the PDF's
  /// summary page.
  String? get _narrative {
    final text = ref.read(writtenReportsProvider)[widget.project.path];
    return (text ?? '').trim().isEmpty ? null : text;
  }

  Future<void> _export(ProjectInfo? previous) async {
    setState(() {
      _working = true;
      _error = null;
    });
    final preparedBy = _preparedBy.text.trim();
    ref
        .read(settingsProvider.notifier)
        .update(
          (s) =>
              s.copyWith(reportPreparedBy: preparedBy, reportAccent: _accent),
        );
    final project = widget.project;
    final standard = ref.read(accessibilityStandardProvider);
    final pdf = _pdf;
    final path = await exportFile(
      context,
      ref,
      name: 'report-${siteKey(project)}',
      extension: pdf ? 'pdf' : 'html',
      write: (file) async {
        try {
          await exportReport(
            dbPath: project.path,
            previousPath: _compare ? previous?.path : null,
            outPath: file,
            preparedBy: preparedBy.isEmpty ? null : preparedBy,
            accent: _accent.isEmpty ? null : _accent,
            standard: standard,
            narrative: _summary ? _narrative : null,
            pdf: pdf,
          );
        } on Object catch (error) {
          _error = '$error';
          rethrow;
        }
      },
      done: pdf ? 'Report saved as PDF' : 'Report saved as a web page',
    );
    if (!mounted) return;
    if (path == null) {
      setState(() => _working = false);
      return;
    }
    Navigator.of(context).pop();
    await openFile(path);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final project = widget.project;
    final isPro = ref.watch(isProProvider);
    final chrome = ref.watch(chromePathProvider);
    final previous = previousCrawl(project, ref.watch(recentProjectsProvider));
    final previousDate = previous == null ? null : crawlDate(previous);

    return Dialog(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 720),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(22, 20, 22, 4),
              child: Text('Export report', style: theme.textTheme.titleLarge),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 22),
              child: Text(
                'Health, a ranked action plan, accessibility and every issue, '
                'laid out for someone who will never open CrawlStat.',
                style: theme.textTheme.bodySmall,
              ),
            ),
            Flexible(
              child: SingleChildScrollView(
                padding: const EdgeInsets.fromLTRB(22, 18, 22, 8),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    _CoverSketch(
                      site: siteKey(project),
                      health: crawlHealth(project.stats),
                      accent: _colour,
                      preparedBy: _preparedBy.text.trim(),
                    ),
                    const SizedBox(width: 22),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          _Label('Prepared by'),
                          TextField(
                            controller: _preparedBy,
                            decoration: const InputDecoration(
                              hintText: 'Your name or agency (optional)',
                            ),
                            onChanged: (_) => setState(() {}),
                          ),
                          const SizedBox(height: 16),
                          _Label('Brand colour'),
                          Wrap(
                            spacing: 8,
                            runSpacing: 8,
                            crossAxisAlignment: WrapCrossAlignment.center,
                            children: [
                              for (final (value, name, colour) in reportAccents)
                                _Swatch(
                                  colour: colour,
                                  name: name,
                                  selected: _accent == value,
                                  onTap: () => setState(() {
                                    _accent = value;
                                    _customHex.clear();
                                  }),
                                ),
                              SizedBox(
                                width: 104,
                                child: TextField(
                                  controller: _customHex,
                                  style: CrawlStatTheme.mono(context),
                                  decoration: const InputDecoration(
                                    hintText: '#RRGGBB',
                                    isDense: true,
                                  ),
                                  onChanged: (text) {
                                    final hex = normaliseHex(text);
                                    if (hex != null) {
                                      setState(() => _accent = hex);
                                    }
                                  },
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(height: 16),
                          _Label('Format'),
                          SegmentedButton<bool>(
                            showSelectedIcon: false,
                            segments: [
                              ButtonSegment(
                                value: true,
                                label: const Text('PDF'),
                                enabled: chrome != null,
                                tooltip: chrome == null
                                    ? 'Needs Chrome, Edge or Brave installed'
                                    : null,
                              ),
                              const ButtonSegment(
                                value: false,
                                label: Text('Web page'),
                              ),
                            ],
                            selected: {_pdf},
                            onSelectionChanged: (value) =>
                                setState(() => _pdf = value.single),
                          ),
                          const SizedBox(height: 6),
                          Text(
                            _pdf
                                ? 'A4, ready to attach or print.'
                                : 'One self-contained file. Opens in any '
                                      'browser, and prints to A4 from there.',
                            style: theme.textTheme.bodySmall,
                          ),
                          if (ref
                                  .watch(writtenReportsProvider)[project.path]
                                  ?.trim()
                                  .isNotEmpty ??
                              false) ...[
                            const SizedBox(height: 14),
                            _Option(
                              title: 'Include the written report',
                              detail:
                                  'What the local model wrote, as a summary '
                                  'page after the cover.',
                              value: _summary,
                              onChanged: (value) =>
                                  setState(() => _summary = value),
                            ),
                          ],
                          if (previous != null) ...[
                            const SizedBox(height: 14),
                            _Option(
                              title: 'Include progress since the last crawl',
                              detail: !isPro
                                  ? 'Comparing crawls is a Pro feature.'
                                  : previousDate == null
                                  ? 'What was fixed and what is new since the '
                                        'previous crawl.'
                                  : 'What was fixed and what is new since the '
                                        'crawl ${relativeTime(previousDate)}.',
                              value: isPro && _compare,
                              onChanged: isPro
                                  ? (value) => setState(() => _compare = value)
                                  : null,
                            ),
                          ],
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
            if (_error case final error?)
              Padding(
                padding: const EdgeInsets.fromLTRB(22, 0, 22, 10),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Icon(
                      AppIcon.error,
                      size: 16,
                      color: theme.colorScheme.error,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: SelectableText(
                        error,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.error,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            Container(
              padding: const EdgeInsets.fromLTRB(22, 12, 22, 16),
              decoration: BoxDecoration(
                border: Border(top: BorderSide(color: theme.dividerColor)),
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      'Saved to ${ref.watch(settingsProvider).resolvedExportDirectory}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall,
                    ),
                  ),
                  OutlinedButton(
                    onPressed: _working
                        ? null
                        : () => Navigator.of(context).pop(),
                    child: const Text('Cancel'),
                  ),
                  const SizedBox(width: 8),
                  FilledButton.icon(
                    onPressed: _working ? null : () => _export(previous),
                    icon: _working
                        ? const SizedBox(
                            width: 13,
                            height: 13,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Icon(AppIcon.download, size: 16),
                    label: Text(_working ? 'Writing…' : 'Export'),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Label extends StatelessWidget {
  const _Label(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 6),
    child: Text(
      text.toUpperCase(),
      style: Theme.of(context).textTheme.labelMedium,
    ),
  );
}

class _Swatch extends StatelessWidget {
  const _Swatch({
    required this.colour,
    required this.name,
    required this.selected,
    required this.onTap,
  });

  final Color colour;
  final String name;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Tooltip(
    message: name,
    child: Semantics(
      button: true,
      selected: selected,
      label: name,
      child: GestureDetector(
        onTap: onTap,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 160),
          width: 28,
          height: 28,
          padding: const EdgeInsets.all(3),
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            border: Border.all(
              // Lifted in dark mode so the graphite swatch's ring still shows.
              color: !selected
                  ? Colors.transparent
                  : Theme.of(context).brightness == Brightness.dark
                  ? Color.lerp(colour, Colors.white, 0.35)!
                  : colour,
              width: 2,
            ),
          ),
          child: DecoratedBox(
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              gradient: LinearGradient(
                begin: Alignment.topLeft,
                end: Alignment.bottomRight,
                colors: [Color.lerp(colour, Colors.white, 0.2)!, colour],
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

/// A checkbox with a line of explanation under it.
class _Option extends StatelessWidget {
  const _Option({
    required this.title,
    required this.detail,
    required this.value,
    required this.onChanged,
  });

  final String title;
  final String detail;
  final bool value;

  /// Null disables it.
  final ValueChanged<bool>? onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final onChanged = this.onChanged;
    return Row(
      children: [
        Checkbox(
          value: value,
          onChanged: onChanged == null ? null : (v) => onChanged(v ?? false),
        ),
        const SizedBox(width: 4),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(title, style: theme.textTheme.bodyMedium),
              Text(detail, style: theme.textTheme.bodySmall),
            ],
          ),
        ),
      ],
    );
  }
}

/// A small drawing of the report's cover, in the chosen colour. Not the
/// real page — that needs a browser — but the same parts in the same places,
/// so a colour can be judged before anything is written.
class _CoverSketch extends StatelessWidget {
  const _CoverSketch({
    required this.site,
    required this.health,
    required this.accent,
    required this.preparedBy,
  });

  final String site;
  final double? health;
  final Color accent;
  final String preparedBy;

  static const _ink = Color(0xFF11131A);
  static const _muted = Color(0xFF6B7280);
  static const _line = Color(0xFFE6E8EE);

  @override
  Widget build(BuildContext context) {
    final tone = health == null
        ? _muted
        : health! >= 0.98
        ? const Color(0xFF16915A)
        : health! >= 0.9
        ? const Color(0xFFC47A0A)
        : const Color(0xFFD6403F);
    return Container(
      width: 190,
      height: 268,
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(10),
        boxShadow: const [
          BoxShadow(
            color: Color(0x2611131A),
            blurRadius: 24,
            offset: Offset(0, 10),
            spreadRadius: -8,
          ),
        ],
      ),
      child: Stack(
        children: [
          Positioned(
            right: -60,
            top: -60,
            child: Container(
              width: 170,
              height: 170,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                gradient: RadialGradient(
                  colors: [
                    accent.withValues(alpha: 0.18),
                    accent.withValues(alpha: 0),
                  ],
                ),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(14, 14, 14, 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Container(
                      width: 9,
                      height: 9,
                      decoration: BoxDecoration(
                        color: accent,
                        borderRadius: BorderRadius.circular(3),
                      ),
                    ),
                    const SizedBox(width: 5),
                    const Text(
                      'CrawlStat',
                      style: TextStyle(
                        fontSize: 7.5,
                        fontWeight: FontWeight.w700,
                        color: _ink,
                      ),
                    ),
                  ],
                ),
                const Spacer(),
                Text(
                  'SITE AUDIT',
                  style: TextStyle(
                    fontSize: 6,
                    letterSpacing: 1,
                    fontWeight: FontWeight.w700,
                    color: accent,
                  ),
                ),
                const SizedBox(height: 3),
                Text(
                  site,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 15,
                    height: 1.1,
                    fontWeight: FontWeight.w800,
                    letterSpacing: -0.4,
                    color: _ink,
                  ),
                ),
                const SizedBox(height: 14),
                Row(
                  children: [
                    SizedBox.square(
                      dimension: 46,
                      child: CustomPaint(
                        painter: _RingSketch(health ?? 0, tone),
                        child: Center(
                          child: Text(
                            health == null ? '—' : formatHealth(health!),
                            style: const TextStyle(
                              fontSize: 8.5,
                              fontWeight: FontWeight.w800,
                              color: _ink,
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 9),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          _bar(70, tone, 5),
                          const SizedBox(height: 5),
                          _bar(96, _line, 3),
                          const SizedBox(height: 3),
                          _bar(80, _line, 3),
                        ],
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 14),
                Row(
                  children: [
                    for (var i = 0; i < 4; i++) ...[
                      if (i > 0) const SizedBox(width: 4),
                      Expanded(
                        child: Container(
                          height: 30,
                          padding: const EdgeInsets.all(4),
                          decoration: BoxDecoration(
                            border: Border.all(color: _line),
                            borderRadius: BorderRadius.circular(4),
                          ),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            mainAxisAlignment: MainAxisAlignment.spaceBetween,
                            children: [
                              _bar(12, i == 0 ? accent : _line, 2),
                              _bar(18, _ink.withValues(alpha: 0.75), 4),
                            ],
                          ),
                        ),
                      ),
                    ],
                  ],
                ),
                const Spacer(),
                Container(height: 0.6, color: _line),
                const SizedBox(height: 5),
                Text(
                  preparedBy.isEmpty ? ' ' : 'Prepared by $preparedBy',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 6.5, color: _muted),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  static Widget _bar(double width, Color colour, double height) => Container(
    width: width,
    height: height,
    decoration: BoxDecoration(
      color: colour,
      borderRadius: BorderRadius.circular(height),
    ),
  );
}

class _RingSketch extends CustomPainter {
  _RingSketch(this.share, this.colour);

  final double share;
  final Color colour;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = (Offset.zero & size).deflate(3);
    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 4
      ..strokeCap = StrokeCap.round;
    canvas.drawArc(
      rect,
      0,
      math.pi * 2,
      false,
      paint..color = const Color(0xFFF1F2F5),
    );
    canvas.drawArc(
      rect,
      -math.pi / 2,
      math.pi * 2 * share.clamp(0, 1),
      false,
      paint..color = colour,
    );
  }

  @override
  bool shouldRepaint(_RingSketch old) =>
      old.share != share || old.colour != colour;
}
