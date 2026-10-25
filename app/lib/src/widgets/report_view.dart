import 'package:flutter/material.dart';

import '../theme/icons.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';

/// A report written by the local model, laid out as a document.
///
/// The model is asked for a small, fixed subset of Markdown — one `#`
/// verdict, `##` sections, numbered and bulleted lists, bold and italics —
/// and this draws exactly that. A full Markdown package would render tables,
/// images and links the prompt forbids, and would draw a half-streamed
/// document no better than this does.
///
/// Rendered as the text streams in, so every block must make sense when cut
/// off mid-sentence.
class ReportView extends StatelessWidget {
  const ReportView({super.key, required this.markdown, this.figures});

  final String markdown;

  /// Drawn under the verdict: the crawl's own numbers, not the model's.
  final Widget? figures;

  @override
  Widget build(BuildContext context) {
    final blocks = _parse(markdown);
    final children = <Widget>[];

    // Everything before the first `##` is the verdict and its overview.
    var index = 0;
    var placedFigures = false;
    while (index < blocks.length && blocks[index] is! _Heading) {
      final block = blocks[index];
      children.add(_buildBlock(context, block));
      if (block is _Title && figures != null) {
        children.add(figures!);
        placedFigures = true;
      }
      index++;
    }
    if (!placedFigures && figures != null) children.insert(0, figures!);

    while (index < blocks.length) {
      final heading = blocks[index] as _Heading;
      final body = <_Block>[];
      index++;
      while (index < blocks.length && blocks[index] is! _Heading) {
        body.add(blocks[index]);
        index++;
      }
      children.add(_Section(heading: heading.text, blocks: body));
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final (i, child) in children.indexed) ...[
          if (i > 0) const SizedBox(height: 14),
          child,
        ],
      ],
    );
  }
}

/// One `##` section, with its own look depending on what it is for.
class _Section extends StatelessWidget {
  const _Section({required this.heading, required this.blocks});

  final String heading;
  final List<_Block> blocks;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final kind = _SectionKind.of(heading);

    final content = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Icon(kind.icon, size: 15, color: kind.color),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                heading,
                style: theme.textTheme.titleSmall?.copyWith(
                  color: kind.callout ? kind.color : null,
                  letterSpacing: 0.1,
                ),
              ),
            ),
          ],
        ),
        for (final block in blocks) ...[
          const SizedBox(height: 10),
          _buildBlock(context, block, tone: kind.color),
        ],
      ],
    );

    if (!kind.callout) {
      return Padding(padding: const EdgeInsets.only(top: 6), child: content);
    }
    return Container(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(CrawlStatTheme.controlRadius + 1),
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            kind.color.withValues(alpha: 0.14),
            kind.color.withValues(alpha: 0.04),
          ],
        ),
        border: Border.all(color: kind.color.withValues(alpha: 0.28)),
      ),
      child: content,
    );
  }
}

/// How a section is drawn, guessed from its heading.
///
/// The prompt fixes the headings, but a model paraphrases, so this matches
/// on a keyword and falls back to a plain section rather than failing.
class _SectionKind {
  const _SectionKind(this.icon, this.color, {this.callout = false});

  final IconData icon;
  final Color color;
  final bool callout;

  static _SectionKind of(String heading) {
    final text = heading.toLowerCase();
    if (text.contains('first')) {
      return const _SectionKind(
        AppIcon.priority,
        CrawlStatTheme.accent,
        callout: true,
      );
    }
    if (text.contains('quick')) {
      return const _SectionKind(AppIcon.quickWin, CrawlStatTheme.success);
    }
    if (text.contains('wait') || text.contains('later')) {
      return const _SectionKind(AppIcon.queued, CrawlStatTheme.notice);
    }
    return const _SectionKind(AppIcon.issues, CrawlStatTheme.accent);
  }
}

Widget _buildBlock(BuildContext context, _Block block, {Color? tone}) {
  final theme = Theme.of(context);
  final body = theme.textTheme.bodyMedium?.copyWith(height: 1.55);

  switch (block) {
    case _Title(:final text):
      return Text.rich(
        _inline(context, text, theme.textTheme.headlineSmall),
        style: theme.textTheme.headlineSmall?.copyWith(
          fontWeight: FontWeight.w700,
          height: 1.25,
          letterSpacing: -0.2,
        ),
      );
    case _Heading(:final text):
      return Text(text, style: theme.textTheme.titleSmall);
    case _Paragraph(:final text):
      return Text.rich(_inline(context, text, body), style: body);
    case _ListBlock(:final ordered, :final items):
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final (i, item) in items.indexed)
            Padding(
              padding: EdgeInsets.only(top: i == 0 ? 0 : (ordered ? 10 : 6)),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  ordered
                      ? _NumberBadge(number: i + 1, color: tone)
                      : _Bullet(color: tone),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Padding(
                      padding: EdgeInsets.only(top: ordered ? 2 : 0),
                      child: Text.rich(
                        _inline(context, item, body),
                        style: body,
                      ),
                    ),
                  ),
                ],
              ),
            ),
        ],
      );
  }
}

class _NumberBadge extends StatelessWidget {
  const _NumberBadge({required this.number, this.color});

  final int number;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 24,
      height: 24,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: Soft.accentGradient(),
        boxShadow: Soft.emberGlow(depth: 0.5),
      ),
      child: Text(
        '$number',
        style: const TextStyle(
          color: Colors.white,
          fontSize: 12,
          fontWeight: FontWeight.w700,
          fontFeatures: [FontFeature.tabularFigures()],
        ),
      ),
    );
  }
}

class _Bullet extends StatelessWidget {
  const _Bullet({this.color});

  final Color? color;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 24,
      height: 22,
      child: Center(
        child: Container(
          width: 6,
          height: 6,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: color ?? CrawlStatTheme.palette(context).accent,
          ),
        ),
      ),
    );
  }
}

// ─── Parsing ─────────────────────────────────────────────────────────────

sealed class _Block {
  const _Block();
}

class _Title extends _Block {
  const _Title(this.text);
  final String text;
}

class _Heading extends _Block {
  const _Heading(this.text);
  final String text;
}

class _Paragraph extends _Block {
  const _Paragraph(this.text);
  final String text;
}

class _ListBlock extends _Block {
  _ListBlock({required this.ordered});
  final bool ordered;
  final List<String> items = [];
}

final _ordered = RegExp(r'^\s*\d+[.)]\s+');
final _bullet = RegExp(r'^\s*[-*•]\s+');
final _heading = RegExp(r'^(#{1,6})\s+');

List<_Block> _parse(String markdown) {
  final blocks = <_Block>[];
  final paragraph = <String>[];

  void flush() {
    if (paragraph.isEmpty) return;
    blocks.add(_Paragraph(paragraph.join(' ')));
    paragraph.clear();
  }

  for (final raw in markdown.split('\n')) {
    final line = raw.trimRight();
    if (line.trim().isEmpty) {
      flush();
      continue;
    }

    final heading = _heading.firstMatch(line);
    if (heading != null) {
      flush();
      final text = line.substring(heading.end).replaceAll('#', '').trim();
      // A heading with only `**` around it is still just a heading.
      final plain = text.replaceAll('**', '');
      blocks.add(
        heading.group(1)!.length == 1 ? _Title(plain) : _Heading(plain),
      );
      continue;
    }

    final ordered = _ordered.firstMatch(line);
    final bullet = ordered == null ? _bullet.firstMatch(line) : null;
    final item = ordered ?? bullet;
    if (item != null) {
      flush();
      final isOrdered = ordered != null;
      final last = blocks.isEmpty ? null : blocks.last;
      if (last is! _ListBlock || last.ordered != isOrdered) {
        blocks.add(_ListBlock(ordered: isOrdered));
      }
      (blocks.last as _ListBlock).items.add(line.substring(item.end).trim());
      continue;
    }

    // A wrapped line under a list item belongs to that item.
    final last = blocks.isEmpty ? null : blocks.last;
    if (paragraph.isEmpty && last is _ListBlock && raw.startsWith(' ')) {
      last.items.last += ' ${line.trim()}';
      continue;
    }
    paragraph.add(line.trim());
  }
  flush();
  return blocks;
}

/// Bold, italics and inline code, as spans.
///
/// An unclosed `**` — the usual state of a line mid-stream — is drawn bold
/// to the end of the line rather than as two literal asterisks.
TextSpan _inline(BuildContext context, String text, TextStyle? base) {
  final spans = <InlineSpan>[];
  final pattern = RegExp(
    r'\*\*(.+?)(\*\*|$)|`([^`]+)`|(?<![\w*])[*_]([^*_]+)[*_](?![\w*])',
  );
  var cursor = 0;
  for (final match in pattern.allMatches(text)) {
    if (match.start > cursor) {
      spans.add(TextSpan(text: text.substring(cursor, match.start)));
    }
    if (match.group(1) != null) {
      spans.add(
        TextSpan(
          text: match.group(1),
          style: TextStyle(
            fontWeight: FontWeight.w700,
            color: Theme.of(context).colorScheme.onSurface,
          ),
        ),
      );
    } else if (match.group(3) != null) {
      spans.add(
        TextSpan(
          text: match.group(3),
          style: CrawlStatTheme.mono(context, size: (base?.fontSize ?? 14) - 1),
        ),
      );
    } else {
      spans.add(
        TextSpan(
          text: match.group(4),
          style: const TextStyle(fontStyle: FontStyle.italic),
        ),
      );
    }
    cursor = match.end;
  }
  if (cursor < text.length) spans.add(TextSpan(text: text.substring(cursor)));
  return TextSpan(children: spans);
}
