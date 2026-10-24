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
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(5),
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

/// A headline number in its own card: label on top, the number, then a
/// one-line hint.
///
/// Every part has a fixed slot — the hint line is reserved even when empty —
/// so a row of tiles lines up label to label and number to number.
class StatTile extends StatelessWidget {
  const StatTile({
    super.key,
    required this.label,
    required this.value,
    this.color,
    this.hint,
    this.icon,
    this.onTap,
  });

  final String label;
  final String value;
  final Color? color;
  final String? hint;
  final IconData? icon;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final tone = color ?? CrawlStatTheme.accent;
    final content = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        SizedBox(
          height: 26,
          child: Row(
            children: [
              if (icon != null) ...[
                Container(
                  width: 26,
                  height: 26,
                  decoration: BoxDecoration(
                    color: tone.withValues(alpha: 0.14),
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(icon, size: 15, color: tone),
                ),
                const SizedBox(width: 10),
              ],
              Expanded(
                child: Text(
                  label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodySmall?.copyWith(fontWeight: FontWeight.w500),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 12),
        Text(
          value,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: theme.textTheme.titleLarge?.copyWith(
            fontSize: 24,
            height: 1.1,
            color: color ?? theme.colorScheme.onSurface,
            fontFeatures: const [FontFeature.tabularFigures()],
          ),
        ),
        const SizedBox(height: 6),
        Text(
          hint ?? '',
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: theme.textTheme.bodySmall,
        ),
      ],
    );

    return Card(
      child: InkWell(
        onTap: onTap,
        child: Padding(padding: const EdgeInsets.fromLTRB(16, 14, 16, 12), child: content),
      ),
    );
  }
}

/// A short coloured label: a crawl phase, a plan, a count.
class TonePill extends StatelessWidget {
  const TonePill(this.label, {super.key, required this.color, this.dot = false});

  final String label;
  final Color color;

  /// Lead with a dot, for states that are live.
  final bool dot;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 3),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(20),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (dot) ...[
            Container(
              width: 6,
              height: 6,
              decoration: BoxDecoration(color: color, shape: BoxShape.circle),
            ),
            const SizedBox(width: 6),
          ],
          Text(
            label,
            style: TextStyle(
              color: color,
              fontSize: 11.5,
              fontWeight: FontWeight.w600,
              height: 1.3,
            ),
          ),
        ],
      ),
    );
  }
}

/// A titled card. The title row and the body share one left edge.
class SectionCard extends StatelessWidget {
  const SectionCard({
    super.key,
    required this.child,
    this.title,
    this.subtitle,
    this.trailing,
    this.padding,
  });

  final Widget child;
  final String? title;
  final String? subtitle;
  final Widget? trailing;

  /// Defaults to the card's standard inset, tucked up under the title.
  final EdgeInsets? padding;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (title != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 16, 14, 10),
              child: Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(title!, style: theme.textTheme.titleSmall),
                        if (subtitle != null) ...[
                          const SizedBox(height: 2),
                          Text(subtitle!, style: theme.textTheme.bodySmall),
                        ],
                      ],
                    ),
                  ),
                  ?trailing,
                ],
              ),
            ),
          Padding(
            padding: padding ??
                (title == null
                    ? const EdgeInsets.all(18)
                    : const EdgeInsets.fromLTRB(18, 4, 18, 18)),
            child: child,
          ),
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
            Container(
              width: 56,
              height: 56,
              decoration: BoxDecoration(
                color: CrawlStatTheme.accent.withValues(alpha: 0.12),
                shape: BoxShape.circle,
              ),
              child: Icon(icon, size: 26, color: CrawlStatTheme.accent),
            ),
            const SizedBox(height: 16),
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
      padding: padding ?? const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      decoration: BoxDecoration(
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
        constraints: const BoxConstraints(minWidth: 32, minHeight: 32),
        padding: EdgeInsets.zero,
        style: IconButton.styleFrom(
          foregroundColor: active ? CrawlStatTheme.accent : theme.colorScheme.onSurfaceVariant,
          backgroundColor:
              active ? CrawlStatTheme.accent.withValues(alpha: 0.14) : Colors.transparent,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius - 1),
          ),
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
    this.margin = EdgeInsets.zero,
  });

  final String message;
  final String severity;
  final EdgeInsets margin;
  final VoidCallback? onDismiss;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final color = CrawlStatTheme.severityColor(severity);
    return Container(
      margin: margin,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.1),
        borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 1),
        border: Border.all(color: color.withValues(alpha: 0.3)),
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
