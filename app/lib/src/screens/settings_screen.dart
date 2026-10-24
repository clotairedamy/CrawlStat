import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../theme/icons.dart';
import '../state/projects.dart';
import '../state/settings.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import '../widgets/license_card.dart';
import 'shell.dart';

/// Crawl defaults, appearance, and what this build is.
///
/// Everything here is a *default* for new crawls, not a change to crawls
/// already on disk: each crawl stores the configuration it ran with, which is
/// what makes a resumed crawl behave like the original.
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(settingsProvider);
    final controller = ref.read(settingsProvider.notifier);
    final theme = Theme.of(context);

    return Column(
      children: [
        const ScreenHeader(
          title: 'Settings',
          subtitle: 'Defaults for new crawls. Existing crawls keep the settings they ran with.',
        ),
        Expanded(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final cards = <Widget>[
                SectionCard(
                  title: 'Appearance',
                  child: Row(
                    children: [
                      Expanded(
                        child: Text('Theme', style: theme.textTheme.bodyMedium),
                      ),
                      SegmentedButton<ThemeMode>(
                        segments: const [
                          ButtonSegment(
                            value: ThemeMode.system,
                            label: Text('System'),
                          ),
                          ButtonSegment(
                            value: ThemeMode.light,
                            label: Text('Light'),
                          ),
                          ButtonSegment(
                            value: ThemeMode.dark,
                            label: Text('Dark'),
                          ),
                        ],
                        selected: {settings.themeMode},
                        showSelectedIcon: false,
                        style: const ButtonStyle(
                          visualDensity: VisualDensity.compact,
                        ),
                        onSelectionChanged: (selection) =>
                            controller.setThemeMode(selection.first),
                      ),
                    ],
                  ),
                ),
                SectionCard(
                  title: 'Politeness',
                  child: Column(
                    children: [
                      _SwitchRow(
                        label: 'Respect robots.txt',
                        description:
                            'Honour Disallow rules and Crawl-delay. Turning this off is for '
                            'sites you own; it is not a licence to hammer someone else’s.',
                        value: settings.respectRobots,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(respectRobots: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _NumberRow(
                        label: 'Simultaneous requests',
                        description:
                            'How many URLs are in flight at once, per host.',
                        value: settings.concurrency,
                        min: 1,
                        max: 50,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(concurrency: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _NumberRow(
                        label: 'Delay between requests',
                        description:
                            'Milliseconds to wait between two requests to one host. '
                            'Zero means as fast as the server answers.',
                        value: settings.crawlDelayMs,
                        min: 0,
                        max: 10000,
                        step: 50,
                        suffix: 'ms',
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(crawlDelayMs: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _NumberRow(
                        label: 'Request timeout',
                        description:
                            'How long to wait for a response before giving up.',
                        value: settings.requestTimeoutMs,
                        min: 1000,
                        max: 120000,
                        step: 1000,
                        suffix: 'ms',
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(requestTimeoutMs: value),
                        ),
                      ),
                    ],
                  ),
                ),
                SectionCard(
                  title: 'User agent',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(
                        'Sent with every request, and matched against robots.txt. '
                        'Identifying the crawler honestly is what keeps it welcome.',
                        style: theme.textTheme.bodySmall,
                      ),
                      const SizedBox(height: 8),
                      _UserAgentField(
                        value: settings.userAgent,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(userAgent: value),
                        ),
                        onReset: controller.resetUserAgent,
                      ),
                    ],
                  ),
                ),
                SectionCard(
                  title: 'Scope',
                  child: Column(
                    children: [
                      _SwitchRow(
                        label: 'Crawl subdomains',
                        description:
                            'Treat blog.example.com as part of example.com.',
                        value: settings.crawlSubdomains,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(crawlSubdomains: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _SwitchRow(
                        label: 'Check external links',
                        description:
                            'Fetch off-site links once to see whether they are broken. '
                            'They are never followed further.',
                        value: settings.checkExternalLinks,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(checkExternalLinks: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _SwitchRow(
                        label: 'Follow nofollow links',
                        description: 'Off by default, which is what a search engine does.',
                        value: settings.followNofollow,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(followNofollow: value),
                        ),
                      ),
                    ],
                  ),
                ),
                SectionCard(
                  title: 'Resources to crawl',
                  child: Column(
                    children: [
                      _SwitchRow(
                        label: 'Images',
                        description: 'Needed for the oversized-image audit to have real sizes.',
                        value: settings.crawlImages,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(crawlImages: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _SwitchRow(
                        label: 'Documents',
                        description:
                            'PDFs and office files linked from the site.',
                        value: settings.crawlDocuments,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(crawlDocuments: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _SwitchRow(
                        label: 'Stylesheets',
                        description: 'Checks that CSS files load. Adds URLs to every crawl.',
                        value: settings.crawlCss,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(crawlCss: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _SwitchRow(
                        label: 'JavaScript',
                        description: 'Checks that scripts load. They are never executed.',
                        value: settings.crawlJavascript,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(crawlJavascript: value),
                        ),
                      ),
                    ],
                  ),
                ),
                SectionCard(
                  title: 'Storage',
                  child: Column(
                    children: [
                      _SwitchRow(
                        label: 'Run audits when a crawl finishes',
                        description:
                            'Takes a moment on a large crawl. They can be re-run '
                            'at any time from the Issues screen.',
                        value: settings.runAudits,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(runAudits: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _SwitchRow(
                        label: 'Keep raw HTML',
                        description:
                            'Stores every page’s source in the crawl file. '
                            'Useful for custom extraction; costly in disk space.',
                        value: settings.storeHtml,
                        onChanged: (value) => controller.update(
                          (s) => s.copyWith(storeHtml: value),
                        ),
                      ),
                      const Divider(height: 28),
                      _PathRow(
                        label: 'Crawls',
                        path: settings.resolvedProjectsDirectory,
                      ),
                      const SizedBox(height: 8),
                      _PathRow(
                        label: 'Exports',
                        path: settings.resolvedExportDirectory,
                      ),
                    ],
                  ),
                ),
                SectionCard(
                  title: 'Keyboard',
                  child: Column(
                    children: const [
                      _ShortcutRow(action: 'New crawl', keys: '⌘N'),
                      _ShortcutRow(action: 'Pause or resume', keys: '⌘Space'),
                      _ShortcutRow(action: 'Focus the filter', keys: '⌘F'),
                      _ShortcutRow(
                        action: 'Export the current table',
                        keys: '⌘E',
                      ),
                      _ShortcutRow(
                        action: 'Close the detail panel',
                        keys: 'Esc',
                      ),
                      _ShortcutRow(
                        action: 'Move through rows',
                        keys: '↑ ↓ PgUp PgDn Home End',
                      ),
                      _ShortcutRow(
                        action: 'Open the selected row',
                        keys: 'Enter',
                      ),
                    ],
                  ),
                ),
                SectionCard(
                  title: 'About',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _AboutRow(
                        label: 'Engine',
                        value: ref.watch(engineVersionProvider),
                      ),
                      _AboutRow(
                        label: 'Free tier limit',
                        value:
                            '${formatCount(ref.watch(freeTierLimitProvider))} URLs per crawl',
                      ),
                      _AboutRow(
                        label: 'Crawl data',
                        value: appSupportDirectory(),
                      ),
                      const SizedBox(height: 10),
                      Text(
                        'Crawls run entirely on this machine. Nothing is sent anywhere.',
                        style: theme.textTheme.bodySmall,
                      ),
                    ],
                  ),
                ),
              ];

              const padding = EdgeInsets.fromLTRB(
                CrawlStatTheme.gutter,
                0,
                CrawlStatTheme.gutter,
                CrawlStatTheme.gutter,
              );

              // A single form stretched across a wide window would put each
              // switch a long way from its label, so wide windows get two
              // columns instead: crawl behaviour on the left, what is kept
              // and how the app is driven on the right.
              if (constraints.maxWidth < 1040) {
                return ListView(
                  padding: padding,
                  children: [
                    const LicenseCard(),
                    for (final card in cards) ...[
                      const SizedBox(height: 16),
                      card,
                    ],
                  ],
                );
              }
              Widget column(Iterable<Widget> children) => Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (final (index, card) in children.indexed) ...[
                      if (index > 0) const SizedBox(height: 16),
                      card,
                    ],
                  ],
                ),
              );
              return SingleChildScrollView(
                padding: padding,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    const LicenseCard(),
                    const SizedBox(height: 16),
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        column(cards.take(4)),
                        const SizedBox(width: 16),
                        column(cards.skip(4)),
                      ],
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

class _SwitchRow extends StatelessWidget {
  const _SwitchRow({
    required this.label,
    required this.description,
    required this.value,
    required this.onChanged,
  });

  final String label;
  final String description;
  final bool value;
  final void Function(bool) onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                label,
                style: theme.textTheme.bodyMedium?.copyWith(
                  fontWeight: FontWeight.w500,
                ),
              ),
              const SizedBox(height: 3),
              Text(description, style: theme.textTheme.bodySmall),
            ],
          ),
        ),
        const SizedBox(width: 24),
        // The Material switch is sized for touch; scaled to sit with the
        // desktop-sized steppers and buttons around it.
        SizedBox(
          height: 26,
          child: FittedBox(
            child: Switch(value: value, onChanged: onChanged),
          ),
        ),
      ],
    );
  }
}

class _NumberRow extends StatelessWidget {
  const _NumberRow({
    required this.label,
    required this.description,
    required this.value,
    required this.min,
    required this.max,
    required this.onChanged,
    this.step = 1,
    this.suffix,
  });

  final String label;
  final String description;
  final int value;
  final int min;
  final int max;
  final int step;
  final String? suffix;
  final void Function(int) onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                label,
                style: theme.textTheme.bodyMedium?.copyWith(
                  fontWeight: FontWeight.w500,
                ),
              ),
              const SizedBox(height: 3),
              Text(description, style: theme.textTheme.bodySmall),
            ],
          ),
        ),
        const SizedBox(width: 24),
        // A stepper in a single rounded field, sized for the widest value
        // ("120000 ms") so the buttons never shift as the number changes.
        Container(
          padding: const EdgeInsets.all(3),
          decoration: BoxDecoration(
            color: theme.colorScheme.surfaceContainer,
            border: Border.all(color: theme.dividerColor),
            borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              ToolbarButton(
                icon: AppIcon.remove,
                tooltip: 'Less',
                onPressed: value <= min
                    ? null
                    : () => onChanged((value - step).clamp(min, max)),
              ),
              SizedBox(
                width: 96,
                child: Text(
                  suffix == null ? '$value' : '$value $suffix',
                  textAlign: TextAlign.center,
                  style: CrawlStatTheme.mono(context),
                ),
              ),
              ToolbarButton(
                icon: AppIcon.add,
                tooltip: 'More',
                onPressed: value >= max
                    ? null
                    : () => onChanged((value + step).clamp(min, max)),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _UserAgentField extends StatefulWidget {
  const _UserAgentField({
    required this.value,
    required this.onChanged,
    required this.onReset,
  });

  final String value;
  final void Function(String) onChanged;
  final VoidCallback onReset;

  @override
  State<_UserAgentField> createState() => _UserAgentFieldState();
}

class _UserAgentFieldState extends State<_UserAgentField> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value);
  }

  @override
  void didUpdateWidget(_UserAgentField old) {
    super.didUpdateWidget(old);
    // Reflect a reset that came from the button, without fighting the user's
    // cursor while they type.
    if (widget.value != old.value && widget.value != _controller.text) {
      _controller.text = widget.value;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: TextField(
            controller: _controller,
            style: CrawlStatTheme.mono(context),
            onChanged: widget.onChanged,
          ),
        ),
        const SizedBox(width: 8),
        OutlinedButton(onPressed: widget.onReset, child: const Text('Reset')),
      ],
    );
  }
}

class _PathRow extends StatelessWidget {
  const _PathRow({required this.label, required this.path});

  final String label;
  final String path;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: [
        SizedBox(
          width: 120,
          child: Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
        ),
        Expanded(
          child: SelectableText(
            path,
            maxLines: 1,
            style: CrawlStatTheme.mono(context, size: 11.5),
          ),
        ),
      ],
    );
  }
}

class _ShortcutRow extends StatelessWidget {
  const _ShortcutRow({required this.action, required this.keys});

  final String action;
  final String keys;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        children: [
          Expanded(child: Text(action, style: theme.textTheme.bodyMedium)),
          Text(keys, style: CrawlStatTheme.mono(context, size: 11.5)),
        ],
      ),
    );
  }
}

class _AboutRow extends StatelessWidget {
  const _AboutRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 120,
            child: Text(
              label.toUpperCase(),
              style: theme.textTheme.labelMedium,
            ),
          ),
          Expanded(
            child: SelectableText(
              value,
              style: CrawlStatTheme.mono(context, size: 11.5),
            ),
          ),
        ],
      ),
    );
  }
}
