import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state/license.dart';
import '../theme/theme.dart';
import 'common.dart';

/// The licence section of Settings: what you have, and how to change it.
class LicenseCard extends ConsumerStatefulWidget {
  const LicenseCard({super.key});

  @override
  ConsumerState<LicenseCard> createState() => _LicenseCardState();
}

class _LicenseCardState extends ConsumerState<LicenseCard> {
  final _key = TextEditingController();
  bool _busy = false;
  String? _error;
  String? _notice;

  @override
  void dispose() {
    _key.dispose();
    super.dispose();
  }

  Future<void> _run(Future<void> Function() action, {String? success}) async {
    setState(() {
      _busy = true;
      _error = null;
      _notice = null;
    });
    try {
      await action();
      if (mounted) setState(() => _notice = success);
    } on Object catch (error) {
      // The service's own wording, not a generic message: "this licence is on
      // three machines already" is actionable, "activation failed" is not.
      if (mounted) setState(() => _error = _clean(error));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Strip the FFI wrapper Dart puts around a Rust error string.
  String _clean(Object error) =>
      error.toString().replaceFirst(RegExp(r'^\w*(Exception|Error):\s*'), '');

  @override
  Widget build(BuildContext context) {
    final info = ref.watch(licenseProvider);
    final theme = Theme.of(context);
    final isPro = info.plan == 'pro';

    return SectionCard(
      title: 'Licence',
      trailing: TonePill(
        isPro ? 'Pro' : 'Free',
        color: isPro ? CrawlStatTheme.success : CrawlStatTheme.notice,
        dot: isPro,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(describeLicense(info), style: theme.textTheme.bodyMedium),
          if (info.keyFingerprint != null) ...[
            const SizedBox(height: 4),
            Text(
              'Key ending ${info.keyFingerprint}',
              style: CrawlStatTheme.mono(context, size: 11.5),
            ),
          ],
          if (info.status == 'grace') ...[
            const SizedBox(height: 10),
            _Note(
              'CrawlStat has not been able to reach the licence service recently. '
              'Everything keeps working; connect within '
              '${info.daysRemaining} day${info.daysRemaining == 1 ? '' : 's'} '
              'and it will sort itself out.',
              color: CrawlStatTheme.warning,
            ),
          ],
          if (info.status == 'expired') ...[
            const SizedBox(height: 10),
            _Note(
              'Your cached licence has lapsed after a fortnight without a check. '
              'Pro features are paused until it is re-checked — nothing has been lost.',
              color: CrawlStatTheme.warning,
            ),
          ],
          if (_error != null) ...[
            const SizedBox(height: 10),
            _Note(_error!, color: CrawlStatTheme.error),
          ],
          if (_notice != null) ...[
            const SizedBox(height: 10),
            _Note(_notice!, color: CrawlStatTheme.success),
          ],
          const SizedBox(height: 14),
          if (isPro) _proActions(context) else _activation(context),
        ],
      ),
    );
  }

  Widget _activation(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Pro removes the URL limit and adds crawl comparison, custom extraction, '
          'scheduled crawls, XLSX and HTML reports, and the BYOK integrations.',
          style: theme.textTheme.bodySmall,
        ),
        const SizedBox(height: 10),
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: _key,
                enabled: !_busy,
                style: CrawlStatTheme.mono(context),
                decoration: const InputDecoration(hintText: 'Paste your licence key'),
                onSubmitted: (_) => _activate(),
              ),
            ),
            const SizedBox(width: 8),
            FilledButton(
              onPressed: _busy ? null : _activate,
              child: _busy
                  ? const SizedBox(
                      width: 13,
                      height: 13,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Text('Activate'),
            ),
          ],
        ),
        const SizedBox(height: 8),
        Text(
          'Activating contacts crawlstat.com once. After that CrawlStat works '
          'offline for a fortnight at a time — a crawl is never interrupted by '
          'a network problem.',
          style: theme.textTheme.bodySmall,
        ),
      ],
    );
  }

  Widget _proActions(BuildContext context) {
    return Row(
      children: [
        OutlinedButton.icon(
          onPressed: _busy
              ? null
              : () => _run(
                    ref.read(licenseProvider.notifier).refreshNow,
                    success: 'Licence re-checked.',
                  ),
          icon: const Icon(Icons.refresh, size: 14),
          label: const Text('Check now'),
        ),
        const SizedBox(width: 8),
        OutlinedButton.icon(
          onPressed: _busy ? null : () => _confirmDeactivate(context),
          icon: const Icon(Icons.logout, size: 14),
          label: const Text('Deactivate'),
        ),
      ],
    );
  }

  void _activate() {
    _run(
      () => ref.read(licenseProvider.notifier).activate(_key.text),
      success: 'Thank you — Pro is active on this machine.',
    ).then((_) {
      if (mounted && _error == null) _key.clear();
    });
  }

  Future<void> _confirmDeactivate(BuildContext context) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Deactivate this machine?'),
        content: Text(
          'The licence is removed from this computer and the seat is freed for '
          'another one. Your crawls and their results are untouched.',
          style: Theme.of(context).textTheme.bodySmall,
        ),
        actions: [
          OutlinedButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Keep it'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Deactivate'),
          ),
        ],
      ),
    );

    if (confirmed ?? false) {
      await _run(
        ref.read(licenseProvider.notifier).deactivate,
        success: 'Deactivated. CrawlStat is back on the free tier.',
      );
    }
  }
}

class _Note extends StatelessWidget {
  const _Note(this.message, {required this.color});

  final String message;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.1),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Text(
        message,
        style: Theme.of(context).textTheme.bodySmall?.copyWith(color: color),
      ),
    );
  }
}

/// A short line telling a free-tier user what the limit is.
///
/// Shown in the new-crawl dialog rather than as a modal: a limit is a fact
/// about the product, not an interruption.
class FreeTierNote extends ConsumerWidget {
  const FreeTierNote({super.key, required this.onUpgrade});

  final VoidCallback onUpgrade;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final limit = ref.watch(urlLimitProvider);
    if (limit == null) {
      // Pro: nothing to say.
      return const SizedBox.shrink();
    }

    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.fromLTRB(11, 9, 6, 9),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerLow,
        border: Border.all(color: theme.dividerColor),
        borderRadius: BorderRadius.circular(7),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              'The free tier crawls up to ${formatCount(limit)} URLs. Larger crawls '
              'stop at the limit and keep everything found up to that point.',
              style: theme.textTheme.bodySmall,
            ),
          ),
          TextButton(onPressed: onUpgrade, child: const Text('Upgrade')),
        ],
      ),
    );
  }
}
