import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../theme/icons.dart';
import '../router.dart';
import '../rust/api/types.dart';
import '../state/crawl.dart';
import '../state/projects.dart';
import '../state/results.dart';
import '../state/settings.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/license_card.dart';

/// Opens the dialog, empty or prefilled.
///
/// [from] is an earlier crawl's settings — "recrawl with changes" — and fills
/// in the whole form. What the dialog does not show (concurrency, the user
/// agent, which resources to fetch…) is kept from it too rather than taken
/// from Settings, so a change to the URL limit does not quietly change
/// everything else about the crawl.
Future<void> showNewCrawlDialog(
  BuildContext context, {
  String? startUrl,
  CrawlRequest? from,
}) {
  return showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (context) => NewCrawlDialog(initialUrl: startUrl, from: from),
  );
}

/// The new-crawl dialog.
///
/// Three modes, one form. Defaults come from Settings — which in turn come from
/// the engine — so there is exactly one place that decides what CrawlStat does
/// when the user changes nothing.
class NewCrawlDialog extends ConsumerStatefulWidget {
  const NewCrawlDialog({super.key, this.initialUrl, this.from});

  final String? initialUrl;

  /// The earlier crawl being repeated, when there is one.
  final CrawlRequest? from;

  @override
  ConsumerState<NewCrawlDialog> createState() => _NewCrawlDialogState();
}

class _NewCrawlDialogState extends ConsumerState<NewCrawlDialog> {
  final _formKey = GlobalKey<FormState>();
  late final TextEditingController _url;
  final _listUrls = TextEditingController();
  final _maxUrls = TextEditingController();
  final _maxDepth = TextEditingController();
  final _include = TextEditingController();
  final _exclude = TextEditingController();

  String _mode = 'spider';
  bool _advanced = false;
  bool _renderJavascript = false;
  bool _starting = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _url = TextEditingController(text: widget.initialUrl ?? '');

    final from = widget.from;
    if (from != null) {
      _mode = from.mode;
      if (from.mode == 'list') {
        _listUrls.text = from.listUrls.join('\n');
      } else {
        _url.text = from.startUrl;
      }
      _maxUrls.text = from.maxUrls?.toString() ?? '';
      _maxDepth.text = from.maxDepth?.toString() ?? '';
      _include.text = from.includePatterns.join('\n');
      _exclude.text = from.excludePatterns.join('\n');
      _renderJavascript = from.renderJavascript;
      // Open the advanced section when it holds something, or the patterns
      // would carry over without the user ever seeing them.
      _advanced =
          from.includePatterns.isNotEmpty || from.excludePatterns.isNotEmpty;
    }
  }

  @override
  void dispose() {
    _url.dispose();
    _listUrls.dispose();
    _maxUrls.dispose();
    _maxDepth.dispose();
    _include.dispose();
    _exclude.dispose();
    super.dispose();
  }

  List<String> _splitLines(TextEditingController controller) {
    return controller.text
        .split('\n')
        .map((line) => line.trim())
        .where((line) => line.isNotEmpty)
        .toList();
  }

  Future<void> _start() async {
    if (!(_formKey.currentState?.validate() ?? false)) return;

    setState(() {
      _starting = true;
      _error = null;
    });

    try {
      final settings = ref.read(settingsProvider);
      final listUrls = _mode == 'list'
          ? _splitLines(_listUrls)
          : const <String>[];
      final startUrl = _mode == 'list'
          ? (listUrls.firstOrNull ?? '')
          : _url.text.trim();

      final maxUrls = int.tryParse(_maxUrls.text.trim());
      final maxDepth = int.tryParse(_maxDepth.text.trim());
      final from = widget.from;
      final request = from == null
          ? settings.toRequest(
              startUrl: startUrl,
              mode: _mode,
              listUrls: listUrls,
              maxUrls: maxUrls,
              maxDepth: maxDepth,
              includePatterns: _splitLines(_include),
              excludePatterns: _splitLines(_exclude),
              renderJavascript: _renderJavascript,
            )
          : CrawlRequest(
              startUrl: startUrl,
              mode: _mode,
              listUrls: listUrls,
              maxUrls: maxUrls,
              maxDepth: maxDepth,
              concurrency: from.concurrency,
              crawlDelayMs: from.crawlDelayMs,
              requestTimeoutMs: from.requestTimeoutMs,
              respectRobots: from.respectRobots,
              userAgent: from.userAgent,
              crawlSubdomains: from.crawlSubdomains,
              checkExternalLinks: from.checkExternalLinks,
              followNofollow: from.followNofollow,
              includePatterns: _splitLines(_include),
              excludePatterns: _splitLines(_exclude),
              crawlImages: from.crawlImages,
              crawlCss: from.crawlCss,
              crawlJavascript: from.crawlJavascript,
              crawlDocuments: from.crawlDocuments,
              storeHtml: from.storeHtml,
              renderJavascript: _renderJavascript,
              runAudits: from.runAudits,
              // Left out on purpose: the engine would use it verbatim and
              // ignore every field the user just edited.
            );

      final path = newProjectPath(settings.resolvedProjectsDirectory, startUrl);

      await ref.read(crawlControllerProvider.notifier).start(request, path);
      ref.read(openProjectProvider.notifier).open(path);
      ref.read(urlTableProvider.notifier).reset();

      if (!mounted) return;
      Navigator.of(context).pop();
      context.go(Routes.project);
    } on Object catch (error) {
      if (!mounted) return;
      setState(() {
        _starting = false;
        // The engine validates the config, so this is usually a bad regex or an
        // unusable URL — worth showing verbatim.
        _error = error.toString().replaceFirst('PanicException: ', '');
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final freeLimit = ref.watch(freeTierLimitProvider);

    return Dialog(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 560, maxHeight: 680),
        child: Form(
          key: _formKey,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 18, 20, 0),
                child: Text(
                  widget.from == null ? 'New crawl' : 'Recrawl with changes',
                  style: theme.textTheme.titleLarge,
                ),
              ),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: InlineMessage(
                    message: _error!,
                    severity: 'error',
                    onDismiss: () => setState(() => _error = null),
                  ),
                ),
              Flexible(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.fromLTRB(20, 16, 20, 8),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _ModeSelector(
                        mode: _mode,
                        onChanged: (mode) => setState(() => _mode = mode),
                      ),
                      const SizedBox(height: 16),
                      if (_mode == 'list')
                        _Field(
                          label: 'URLs to fetch',
                          hint: 'One per line. Nothing is followed.',
                          child: TextFormField(
                            controller: _listUrls,
                            maxLines: 7,
                            minLines: 5,
                            style: CrawlStatTheme.mono(context),
                            decoration: const InputDecoration(
                              hintText: 'https://example.com/page-one\nhttps://example.com/page-two',
                            ),
                            validator: (_) => _splitLines(_listUrls).isEmpty
                                ? 'Add at least one URL'
                                : null,
                          ),
                        )
                      else
                        _Field(
                          label: _mode == 'sitemap'
                              ? 'Sitemap URL'
                              : 'Start URL',
                          hint: _mode == 'sitemap'
                              ? 'A sitemap or sitemap index. .gz is handled.'
                              : 'Crawls outwards from here. The scheme may be omitted.',
                          child: TextFormField(
                            controller: _url,
                            autofocus: true,
                            style: CrawlStatTheme.mono(context),
                            decoration: InputDecoration(
                              hintText: _mode == 'sitemap'
                                  ? 'https://example.com/sitemap.xml'
                                  : 'example.com',
                            ),
                            onFieldSubmitted: (_) => _start(),
                            validator: (value) =>
                                (value == null || value.trim().isEmpty)
                                ? 'Enter a URL'
                                : null,
                          ),
                        ),
                      const SizedBox(height: 16),
                      Row(
                        children: [
                          Expanded(
                            child: _Field(
                              label: 'URL limit',
                              hint: 'Blank for no limit',
                              child: TextFormField(
                                controller: _maxUrls,
                                keyboardType: TextInputType.number,
                                decoration: InputDecoration(
                                  hintText: '$freeLimit',
                                ),
                                validator: _positiveOrBlank,
                              ),
                            ),
                          ),
                          const SizedBox(width: 12),
                          Expanded(
                            child: _Field(
                              label: 'Maximum depth',
                              hint: 'Clicks from the start URL',
                              child: TextFormField(
                                controller: _maxDepth,
                                keyboardType: TextInputType.number,
                                decoration: const InputDecoration(
                                  hintText: 'No limit',
                                ),
                                validator: _positiveOrBlank,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 14),
                      _RenderJavascript(
                        value: _renderJavascript,
                        chrome: ref.watch(chromePathProvider),
                        onChanged: (value) =>
                            setState(() => _renderJavascript = value),
                      ),
                      const SizedBox(height: 12),
                      FreeTierNote(
                        onUpgrade: () {
                          Navigator.of(context).pop();
                          context.go(Routes.settings);
                        },
                      ),
                      const SizedBox(height: 4),
                      Align(
                        alignment: Alignment.centerLeft,
                        child: TextButton.icon(
                          onPressed: () =>
                              setState(() => _advanced = !_advanced),
                          icon: Icon(
                            _advanced ? AppIcon.chevronUp : AppIcon.chevronDown,
                            size: 15,
                          ),
                          label: Text(
                            _advanced ? 'Fewer options' : 'More options',
                          ),
                        ),
                      ),
                      if (_advanced) ...[
                        const SizedBox(height: 4),
                        _Field(
                          label: 'Only crawl URLs matching',
                          hint: 'One regular expression per line. Blank crawls everything.',
                          child: TextFormField(
                            controller: _include,
                            maxLines: 3,
                            style: CrawlStatTheme.mono(context),
                            decoration: const InputDecoration(
                              hintText: r'^https://example\.com/blog',
                            ),
                          ),
                        ),
                        const SizedBox(height: 12),
                        _Field(
                          label: 'Never crawl URLs matching',
                          hint: 'Applied after the include rules.',
                          child: TextFormField(
                            controller: _exclude,
                            maxLines: 3,
                            style: CrawlStatTheme.mono(context),
                            decoration: const InputDecoration(
                              hintText: r'/cart|\?page=',
                            ),
                          ),
                        ),
                        const SizedBox(height: 14),
                        _SettingsSummary(from: widget.from),
                      ],
                    ],
                  ),
                ),
              ),
              Container(
                padding: const EdgeInsets.fromLTRB(20, 12, 20, 16),
                decoration: BoxDecoration(
                  border: Border(top: BorderSide(color: theme.dividerColor)),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: Text(
                        'Saved to ${ref.watch(settingsProvider).resolvedProjectsDirectory}',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.bodySmall,
                      ),
                    ),
                    OutlinedButton(
                      onPressed: _starting
                          ? null
                          : () => Navigator.of(context).pop(),
                      child: const Text('Cancel'),
                    ),
                    const SizedBox(width: 8),
                    FilledButton.icon(
                      onPressed: _starting ? null : _start,
                      icon: _starting
                          ? const SizedBox(
                              width: 13,
                              height: 13,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(AppIcon.play, size: 16),
                      label: Text(_starting ? 'Starting…' : 'Start crawl'),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  String? _positiveOrBlank(String? value) {
    if (value == null || value.trim().isEmpty) return null;
    final parsed = int.tryParse(value.trim());
    if (parsed == null || parsed <= 0) return 'Enter a positive number';
    return null;
  }
}

class _ModeSelector extends StatelessWidget {
  const _ModeSelector({required this.mode, required this.onChanged});

  final String mode;
  final void Function(String) onChanged;

  static const _modes = [
    ('spider', 'Spider', AppIcon.structure, 'Follow links from a start URL'),
    (
      'sitemap',
      'Sitemap',
      AppIcon.sitemap,
      'Crawl everything a sitemap declares',
    ),
    ('list', 'List', AppIcon.report, 'Fetch exactly these URLs'),
  ];

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: [
        for (final (id, label, icon, description) in _modes) ...[
          Expanded(
            child: Tooltip(
              message: description,
              child: InkWell(
                onTap: () => onChanged(id),
                borderRadius: BorderRadius.circular(7),
                child: Container(
                  padding: const EdgeInsets.symmetric(vertical: 11),
                  decoration: BoxDecoration(
                    color: mode == id
                        ? CrawlStatTheme.palette(context).accent
                              .withValues(alpha: 0.1)
                        : null,
                    border: Border.all(
                      color: mode == id
                          ? CrawlStatTheme.palette(context).accent
                          : theme.dividerColor,
                    ),
                    borderRadius: BorderRadius.circular(7),
                  ),
                  child: Column(
                    children: [
                      Icon(
                        icon,
                        size: 17,
                        color: mode == id
                            ? CrawlStatTheme.palette(context).accent
                            : theme.colorScheme.onSurfaceVariant,
                      ),
                      const SizedBox(height: 5),
                      Text(
                        label,
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: mode == id
                              ? CrawlStatTheme.palette(context).accent
                              : null,
                          fontWeight: mode == id ? FontWeight.w600 : null,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          if (id != _modes.last.$1) const SizedBox(width: 8),
        ],
      ],
    );
  }
}

/// Whether to run each page's scripts in Chrome before reading it.
///
/// Off by default and said plainly why: it is several times slower, and most
/// sites serve their content in the HTML anyway. Without a browser on the
/// machine the switch is shown, disabled, so the feature is discoverable.
class _RenderJavascript extends StatelessWidget {
  const _RenderJavascript({
    required this.value,
    required this.chrome,
    required this.onChanged,
  });

  final bool value;
  final String? chrome;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final available = chrome != null;
    final browser = chrome?.split('/').last.split(r'\').last;
    return Container(
      padding: const EdgeInsets.fromLTRB(12, 10, 8, 10),
      decoration: BoxDecoration(
        color: value
            ? CrawlStatTheme.palette(context).accent.withValues(alpha: 0.08)
            : theme.colorScheme.surfaceContainerLow,
        border: Border.all(
          color: value
              ? CrawlStatTheme.palette(context).accent.withValues(alpha: 0.5)
              : theme.dividerColor,
        ),
        borderRadius: BorderRadius.circular(7),
      ),
      child: Row(
        children: [
          Icon(
            AppIcon.code,
            size: 18,
            color: value
                ? CrawlStatTheme.palette(context).accent
                : theme.colorScheme.onSurfaceVariant,
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Render JavaScript', style: theme.textTheme.bodyMedium),
                const SizedBox(height: 2),
                Text(
                  available
                      ? 'Runs each page in $browser first, for sites built '
                            'with React, Vue and the like. Several times slower.'
                      : 'Needs Chrome, Chromium, Edge or Brave installed.',
                  style: theme.textTheme.bodySmall,
                ),
              ],
            ),
          ),
          Switch(value: value, onChanged: available ? onChanged : null),
        ],
      ),
    );
  }
}

/// A labelled field with a hint underneath.
class _Field extends StatelessWidget {
  const _Field({required this.label, required this.child, this.hint});

  final String label;
  final Widget child;
  final String? hint;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
        const SizedBox(height: 6),
        child,
        if (hint != null) ...[
          const SizedBox(height: 4),
          Text(hint!, style: theme.textTheme.bodySmall),
        ],
      ],
    );
  }
}

/// Reminds the user which defaults this crawl will use, with a way to change
/// them — rather than duplicating every engine option in this dialog.
///
/// For a recrawl it is the earlier crawl's settings instead, and Settings has
/// no say — so there is nothing to change there.
class _SettingsSummary extends ConsumerWidget {
  const _SettingsSummary({this.from});

  final CrawlRequest? from;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final request = from ?? ref.watch(settingsProvider).toRequest(startUrl: '');
    final theme = Theme.of(context);

    final parts = [
      '${request.concurrency} at a time',
      if (request.crawlDelayMs > 0) '${request.crawlDelayMs} ms apart',
      request.respectRobots ? 'robots.txt respected' : 'robots.txt ignored',
      if (request.crawlSubdomains) 'subdomains included',
      if (request.checkExternalLinks) 'external links checked',
      if (request.runAudits) 'audits on finish',
    ];

    return Container(
      padding: const EdgeInsets.all(11),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerLow,
        border: Border.all(color: theme.dividerColor),
        borderRadius: BorderRadius.circular(7),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              '${from == null ? '' : 'As before: '}${parts.join(' · ')}',
              style: theme.textTheme.bodySmall,
            ),
          ),
          if (from == null)
            TextButton(
              onPressed: () {
                Navigator.of(context).pop();
                context.go(Routes.settings);
              },
              child: const Text('Change'),
            ),
        ],
      ),
    );
  }
}
