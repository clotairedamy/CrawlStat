import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../theme/theme.dart';

final _thousands = NumberFormat.decimalPattern();

/// Format a count with thousands separators: `128,431`.
String formatCount(num value) => _thousands.format(value);

/// Format a byte count the way a developer reads it.
String formatBytes(int? bytes) {
  if (bytes == null) return '';
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(0)} KB';
  return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
}

/// Format a duration in milliseconds, compactly.
String formatMillis(int? ms) {
  if (ms == null) return '';
  if (ms < 1000) return '$ms ms';
  if (ms < 60000) return '${(ms / 1000).toStringAsFixed(1)} s';
  final minutes = ms ~/ 60000;
  final seconds = (ms % 60000) ~/ 1000;
  return '${minutes}m ${seconds.toString().padLeft(2, '0')}s';
}

/// An HTTP status code, coloured by class.
class StatusChip extends StatelessWidget {
  const StatusChip(this.status, {super.key, this.label});

  final int? status;
  final String? label;

  @override
  Widget build(BuildContext context) {
    final color = CrawlStatTheme.statusColor(status);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.13),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        label ?? (status?.toString() ?? '—'),
        style: CrawlStatTheme.mono(context, color: color, size: 11.5),
      ),
    );
  }
}

/// A small coloured dot for an issue severity.
class SeverityDot extends StatelessWidget {
  const SeverityDot(this.severity, {super.key, this.size = 8});

  final String severity;
  final double size;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: CrawlStatTheme.severityColor(severity),
        shape: BoxShape.circle,
      ),
    );
  }
}

/// `Indexable` / the reason it is not.
class IndexablePill extends StatelessWidget {
  const IndexablePill({super.key, required this.indexable, this.reason});

  final bool? indexable;
  final String? reason;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (indexable == null) {
      return Text('—', style: theme.textTheme.bodySmall);
    }
    final color = indexable! ? CrawlStatTheme.success : CrawlStatTheme.notice;
    return Tooltip(
      message: reason ?? (indexable! ? 'Can be indexed' : 'Not indexable'),
      child: Text(
        indexable! ? 'Indexable' : (reason ?? 'Not indexable'),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: theme.textTheme.bodySmall?.copyWith(color: color),
      ),
    );
  }
}

/// A headline number with a label, for the crawl screen and the project cards.
class StatTile extends StatelessWidget {
  const StatTile({
    super.key,
    required this.label,
    required this.value,
    this.color,
    this.hint,
    this.onTap,
  });

  final String label;
  final String value;
  final Color? color;
  final String? hint;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final content = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
        const SizedBox(height: 4),
        Text(
          value,
          style: CrawlStatTheme.mono(
            context,
            size: 21,
            color: color ?? theme.colorScheme.onSurface,
          ).copyWith(fontWeight: FontWeight.w600),
        ),
        if (hint != null) ...[
          const SizedBox(height: 2),
          Text(hint!, style: theme.textTheme.bodySmall),
        ],
      ],
    );

    return Tooltip(
      message: hint ?? '',
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(6),
          child: Padding(padding: const EdgeInsets.all(10), child: content),
        ),
      ),
    );
  }
}

/// A titled panel. Used instead of elevation to group things.
class SectionCard extends StatelessWidget {
  const SectionCard({
    super.key,
    required this.child,
    this.title,
    this.trailing,
    this.padding = const EdgeInsets.all(14),
  });

  final Widget child;
  final String? title;
  final Widget? trailing;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (title != null)
            Container(
              padding: const EdgeInsets.fromLTRB(14, 10, 10, 10),
              decoration: BoxDecoration(
                border: Border(bottom: BorderSide(color: theme.dividerColor)),
              ),
              child: Row(
                children: [
                  Expanded(child: Text(title!, style: theme.textTheme.labelMedium)),
                  ?trailing,
                ],
              ),
            ),
          Padding(padding: padding, child: child),
        ],
      ),
    );
  }
}

/// What a screen shows when there is nothing to show.
///
/// Always says what to do next, never just "no data": an empty state is a
/// moment where the user needs a hint, not a shrug.
class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.icon,
    required this.title,
    required this.message,
    this.action,
  });

  final IconData icon;
  final String title;
  final String message;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 380),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 28, color: theme.colorScheme.onSurfaceVariant),
            const SizedBox(height: 12),
            Text(title, style: theme.textTheme.titleMedium, textAlign: TextAlign.center),
            const SizedBox(height: 6),
            Text(
              message,
              style: theme.textTheme.bodySmall,
              textAlign: TextAlign.center,
            ),
            if (action != null) ...[const SizedBox(height: 16), action!],
          ],
        ),
      ),
    );
  }
}

/// A horizontal strip of controls above a table.
class Toolbar extends StatelessWidget {
  const Toolbar({super.key, required this.children, this.padding});

  final List<Widget> children;
  final EdgeInsets? padding;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: padding ?? const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: theme.colorScheme.surface,
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      child: Row(children: children),
    );
  }
}

/// A compact icon button for toolbars.
class ToolbarButton extends StatelessWidget {
  const ToolbarButton({
    super.key,
    required this.icon,
    required this.tooltip,
    this.onPressed,
    this.active = false,
  });

  final IconData icon;
  final String tooltip;
  final VoidCallback? onPressed;
  final bool active;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Tooltip(
      message: tooltip,
      child: IconButton(
        onPressed: onPressed,
        icon: Icon(icon, size: 16),
        iconSize: 16,
        visualDensity: VisualDensity.compact,
        constraints: const BoxConstraints(minWidth: 30, minHeight: 30),
        padding: EdgeInsets.zero,
        style: IconButton.styleFrom(
          foregroundColor: active ? CrawlStatTheme.accent : theme.colorScheme.onSurfaceVariant,
          backgroundColor:
              active ? CrawlStatTheme.accent.withValues(alpha: 0.12) : Colors.transparent,
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(5)),
        ),
      ),
    );
  }
}

/// A short, dismissible banner for a non-fatal problem.
class InlineMessage extends StatelessWidget {
  const InlineMessage({
    super.key,
    required this.message,
    this.severity = 'warning',
    this.onDismiss,
    this.action,
  });

  final String message;
  final String severity;
  final VoidCallback? onDismiss;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final color = CrawlStatTheme.severityColor(severity);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.1),
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      child: Row(
        children: [
          Icon(
            severity == 'error' ? Icons.error_outline : Icons.warning_amber_outlined,
            size: 15,
            color: color,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(message, style: theme.textTheme.bodySmall?.copyWith(color: color)),
          ),
          ?action,
          if (onDismiss != null)
            ToolbarButton(icon: Icons.close, tooltip: 'Dismiss', onPressed: onDismiss),
        ],
      ),
    );
  }
}

/// Lets the user choose which grid columns to show.
class ColumnChooser extends StatelessWidget {
  const ColumnChooser({
    super.key,
    required this.columns,
    required this.visible,
    required this.onChanged,
  });

  /// `(id, label)` for every available column.
  final List<(String, String)> columns;
  final Set<String> visible;
  final void Function(Set<String>) onChanged;

  @override
  Widget build(BuildContext context) {
    return PopupMenuButton<String>(
      tooltip: 'Choose columns',
      position: PopupMenuPosition.under,
      icon: const Icon(Icons.view_column_outlined, size: 16),
      iconSize: 16,
      constraints: const BoxConstraints(maxHeight: 420, minWidth: 220),
      // Closing on every tick would make choosing several columns tedious.
      onSelected: (id) {
        final next = {...visible};
        if (!next.remove(id)) next.add(id);
        onChanged(next);
      },
      itemBuilder: (context) => [
        for (final (id, label) in columns)
          CheckedPopupMenuItem<String>(
            value: id,
            checked: visible.contains(id),
            height: 34,
            child: Text(label, style: Theme.of(context).textTheme.bodyMedium),
          ),
      ],
    );
  }
}
