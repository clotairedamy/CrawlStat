import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../theme/icons.dart';
import '../rust/api/types.dart';
import '../state/results.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';

/// Everything known about one URL, in a side panel.
///
/// Tabs rather than one long scroll: a user opens this to answer a specific
/// question — why is this a 404, who links here, what does the chain look
/// like — and should not have to scroll past four other answers to find it.
class UrlDetailPanel extends ConsumerStatefulWidget {
  const UrlDetailPanel({super.key, required this.path, required this.urlId});

  final String path;
  final int urlId;

  @override
  ConsumerState<UrlDetailPanel> createState() => _UrlDetailPanelState();
}

class _UrlDetailPanelState extends ConsumerState<UrlDetailPanel> {
  int _tab = 0;

  static const _tabs = [
    'Overview',
    'Issues',
    'Inlinks',
    'Outlinks',
    'Images',
    'Headers',
  ];

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final detail = ref.watch(
      urlDetailProvider((path: widget.path, urlId: widget.urlId)),
    );

    return SizedBox(
      width: 420,
      child: detail.when(
        loading: () => const Center(
          child: SizedBox(
            width: 18,
            height: 18,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ),
        error: (error, _) => Padding(
          padding: const EdgeInsets.all(14),
          child: Text(
            'Could not load this URL: $error',
            style: theme.textTheme.bodySmall,
          ),
        ),
        data: (data) => Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _Header(row: data.row),
            _PanelTabs(
              tabs: _tabs,
              counts: {
                1: data.issues.length,
                2: data.inlinks.length,
                3: data.outlinks.length,
                4: data.images.length,
              },
              selected: _tab,
              onSelect: (index) => setState(() => _tab = index),
            ),
            Expanded(
              child: switch (_tab) {
                1 => _IssuesTab(issues: data.issues),
                2 => _LinksTab(links: data.inlinks, showSource: true),
                3 => _LinksTab(links: data.outlinks, showSource: false),
                4 => _ImagesTab(images: data.images),
                5 => _HeadersTab(headers: data.headers),
                _ => _OverviewTab(detail: data),
              },
            ),
          ],
        ),
      ),
    );
  }
}

class _Header extends ConsumerWidget {
  const _Header({required this.row});

  final UrlRow row;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.fromLTRB(18, 14, 10, 14),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              StatusChip(row.statusCode),
              const SizedBox(width: 8),
              Expanded(
                child: IndexablePill(
                  indexable: row.indexable,
                  reason: row.indexabilityReason,
                ),
              ),
              ToolbarButton(
                icon: AppIcon.copy,
                tooltip: 'Copy URL',
                onPressed: () =>
                    Clipboard.setData(ClipboardData(text: row.url)),
              ),
              ToolbarButton(
                icon: AppIcon.close,
                tooltip: 'Close  (Esc)',
                onPressed: ref.read(selectedUrlProvider.notifier).clear,
              ),
            ],
          ),
          const SizedBox(height: 8),
          SelectableText(
            row.url,
            style: CrawlStatTheme.mono(context, size: 12.5),
            maxLines: 3,
          ),
        ],
      ),
    );
  }
}

class _PanelTabs extends StatelessWidget {
  const _PanelTabs({
    required this.tabs,
    required this.selected,
    required this.onSelect,
    this.counts = const {},
  });

  final List<String> tabs;
  final int selected;
  final void Function(int) onSelect;
  final Map<int, int> counts;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      height: 32,
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(
          children: [
            const SizedBox(width: 6),
            for (var index = 0; index < tabs.length; index++)
              InkWell(
                onTap: () => onSelect(index),
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 9,
                    vertical: 7,
                  ),
                  decoration: BoxDecoration(
                    border: Border(
                      bottom: BorderSide(
                        color: selected == index
                            ? CrawlStatTheme.palette(context).accent
                            : Colors.transparent,
                        width: 2,
                      ),
                    ),
                  ),
                  child: Text(
                    counts[index] != null && counts[index]! > 0
                        ? '${tabs[index]} ${counts[index]}'
                        : tabs[index],
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: selected == index
                          ? CrawlStatTheme.palette(context).accent
                          : null,
                      fontWeight: selected == index ? FontWeight.w600 : null,
                    ),
                  ),
                ),
              ),
            const SizedBox(width: 6),
          ],
        ),
      ),
    );
  }
}

class _OverviewTab extends StatelessWidget {
  const _OverviewTab({required this.detail});

  final UrlDetail detail;

  @override
  Widget build(BuildContext context) {
    final row = detail.row;
    return ListView(
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 20),
      children: [
        if (detail.redirectChain.isNotEmpty) ...[
          _Field(label: 'Redirect chain'),
          for (final hop in detail.redirectChain)
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  StatusChip(hop.status),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      hop.toUrl,
                      style: CrawlStatTheme.mono(context, size: 11.5),
                    ),
                  ),
                ],
              ),
            ),
          const SizedBox(height: 14),
        ],
        _Pair(label: 'Title', value: row.title, suffix: _len(row.titleLength)),
        _Pair(
          label: 'Meta description',
          value: row.metaDescription,
          suffix: _len(row.metaDescriptionLength),
        ),
        _Pair(
          label: 'H1',
          value: row.h1,
          suffix: row.h1Count == null ? null : '${row.h1Count}',
        ),
        _Pair(label: 'Canonical', value: row.canonical, mono: true),
        _Pair(label: 'Meta robots', value: row.metaRobots),
        _Pair(label: 'Language', value: row.htmlLang),
        _Pair(label: 'Structured data', value: row.structuredDataTypes),
        const Divider(height: 22),
        _Pair(label: 'Content type', value: row.contentType),
        _Pair(label: 'Size', value: formatBytes(row.sizeBytes)),
        _Pair(label: 'Response time', value: formatMillis(row.responseTimeMs)),
        _Pair(
          label: 'Word count',
          value: row.wordCount == null ? null : formatCount(row.wordCount!),
        ),
        const Divider(height: 22),
        _Pair(label: 'Crawl depth', value: row.depth.toString()),
        _Pair(label: 'Inlinks', value: formatCount(row.inlinks)),
        _Pair(
          label: 'Outlinks',
          value: row.internalLinks == null
              ? null
              : '${row.internalLinks} internal, ${row.externalLinks ?? 0} external',
        ),
        _Pair(
          label: 'Link score',
          value: row.linkScore?.toStringAsFixed(1),
          suffix: 'of 100',
        ),
        if (row.errorKind != null) ...[
          const Divider(height: 22),
          _Pair(
            label: 'Error',
            value: row.errorKind,
            color: CrawlStatTheme.palette(context).error,
          ),
        ],
      ],
    );
  }

  String? _len(int? value) => value == null ? null : '$value chars';
}

class _IssuesTab extends StatelessWidget {
  const _IssuesTab({required this.issues});

  final List<UrlIssue> issues;

  @override
  Widget build(BuildContext context) {
    if (issues.isEmpty) {
      return const EmptyState(
        icon: AppIcon.ok,
        title: 'No issues',
        message: 'Nothing the audits look for applies to this URL.',
      );
    }

    final theme = Theme.of(context);
    return ListView.separated(
      padding: const EdgeInsets.all(14),
      itemCount: issues.length,
      separatorBuilder: (_, _) => const SizedBox(height: 10),
      itemBuilder: (context, index) {
        final issue = issues[index];
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 5, right: 8),
              child: SeverityDot(issue.severity),
            ),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(issue.title, style: theme.textTheme.bodyMedium),
                  if (issue.detail != null)
                    Text(issue.detail!, style: theme.textTheme.bodySmall),
                ],
              ),
            ),
          ],
        );
      },
    );
  }
}

class _LinksTab extends StatelessWidget {
  const _LinksTab({required this.links, required this.showSource});

  final List<LinkRow> links;
  final bool showSource;

  @override
  Widget build(BuildContext context) {
    if (links.isEmpty) {
      return EmptyState(
        icon: AppIcon.brokenLink,
        title: showSource ? 'No inlinks' : 'No outlinks',
        message: showSource
            ? 'Nothing on this site references this URL — it is an orphan.'
            : 'This URL links to nothing, or it was never parsed.',
      );
    }

    final theme = Theme.of(context);
    return ListView.separated(
      padding: const EdgeInsets.all(14),
      itemCount: links.length,
      separatorBuilder: (_, _) =>
          Divider(height: 14, color: theme.dividerColor),
      itemBuilder: (context, index) {
        final link = links[index];
        final url = showSource
            ? link.sourceUrl
            : (link.targetUrl ?? link.anchorText);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                if (link.statusCode != null) ...[
                  StatusChip(link.statusCode),
                  const SizedBox(width: 6),
                ],
                Expanded(
                  child: Text(
                    url,
                    style: CrawlStatTheme.mono(context, size: 11.5),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 3),
            Wrap(
              spacing: 8,
              children: [
                if (link.anchorText.isNotEmpty)
                  Text(
                    '“${link.anchorText}”',
                    style: theme.textTheme.bodySmall,
                  ),
                Text(link.linkType, style: theme.textTheme.bodySmall),
                if (link.position != 'unknown')
                  Text(link.position, style: theme.textTheme.bodySmall),
                if (link.nofollow)
                  Text(
                    'nofollow',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: CrawlStatTheme.palette(context).warning,
                    ),
                  ),
              ],
            ),
          ],
        );
      },
    );
  }
}

class _ImagesTab extends StatelessWidget {
  const _ImagesTab({required this.images});

  final List<ImageRow> images;

  @override
  Widget build(BuildContext context) {
    if (images.isEmpty) {
      return const EmptyState(
        icon: AppIcon.noImage,
        title: 'No images',
        message: 'This page has no <img> elements.',
      );
    }

    final theme = Theme.of(context);
    return ListView.separated(
      padding: const EdgeInsets.all(14),
      itemCount: images.length,
      separatorBuilder: (_, _) =>
          Divider(height: 14, color: theme.dividerColor),
      itemBuilder: (context, index) {
        final image = images[index];
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              image.src ?? image.rawSrc,
              style: CrawlStatTheme.mono(context, size: 11.5),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: 3),
            Wrap(
              spacing: 10,
              children: [
                if (image.altMissing)
                  Text(
                    'no alt attribute',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: CrawlStatTheme.palette(context).warning,
                    ),
                  )
                else if ((image.alt ?? '').isEmpty)
                  Text(
                    'decorative (empty alt)',
                    style: theme.textTheme.bodySmall,
                  )
                else
                  Text('“${image.alt}”', style: theme.textTheme.bodySmall),
                if (image.sizeBytes != null)
                  Text(
                    formatBytes(image.sizeBytes),
                    style: theme.textTheme.bodySmall,
                  ),
                if (image.declaredWidth != null && image.declaredHeight != null)
                  Text(
                    '${image.declaredWidth}×${image.declaredHeight}',
                    style: theme.textTheme.bodySmall,
                  ),
              ],
            ),
          ],
        );
      },
    );
  }
}

class _HeadersTab extends StatelessWidget {
  const _HeadersTab({required this.headers});

  final List<HeaderPair> headers;

  @override
  Widget build(BuildContext context) {
    if (headers.isEmpty) {
      return const EmptyState(
        icon: AppIcon.web,
        title: 'No headers stored',
        message: 'This URL was never fetched, or the headers were not kept.',
      );
    }

    return ListView.builder(
      padding: const EdgeInsets.all(14),
      itemCount: headers.length,
      itemBuilder: (context, index) {
        final header = headers[index];
        return Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(header.name, style: Theme.of(context).textTheme.labelMedium),
              SelectableText(
                header.value,
                style: CrawlStatTheme.mono(context, size: 11.5),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _Field extends StatelessWidget {
  const _Field({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Text(
        label.toUpperCase(),
        style: Theme.of(context).textTheme.labelMedium,
      ),
    );
  }
}

/// A label and its value, or nothing at all when the value is absent.
class _Pair extends StatelessWidget {
  const _Pair({
    required this.label,
    required this.value,
    this.suffix,
    this.mono = false,
    this.color,
  });

  final String label;
  final String? value;
  final String? suffix;
  final bool mono;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    if (value == null || value!.isEmpty) {
      // An empty row is noise; absence is shown by the field not being there.
      return const SizedBox.shrink();
    }
    final theme = Theme.of(context);

    return Padding(
      padding: const EdgeInsets.only(bottom: 11),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text(label.toUpperCase(), style: theme.textTheme.labelMedium),
              if (suffix != null) ...[
                const SizedBox(width: 6),
                Text(suffix!, style: theme.textTheme.bodySmall),
              ],
            ],
          ),
          const SizedBox(height: 2),
          SelectableText(
            value!,
            style: mono
                ? CrawlStatTheme.mono(context, color: color)
                : theme.textTheme.bodyMedium?.copyWith(color: color),
          ),
        ],
      ),
    );
  }
}
