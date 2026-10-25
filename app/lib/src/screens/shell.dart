import 'dart:math' as math;
import 'dart:ui' show lerpDouble;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../theme/glyphs.dart';
import '../theme/icons.dart';
import '../router.dart';
import '../state/crawl.dart';
import '../state/license.dart';
import '../state/projects.dart';
import '../state/settings.dart';
import '../theme/soft.dart';
import '../theme/theme.dart';
import '../widgets/common.dart';
import 'new_crawl_dialog.dart';

/// Height left clear at the top of the window for the macOS traffic lights,
/// which float over the app's own background rather than over a title bar.
const double _titleBarInset = 28;

/// The rail's two widths. Collapsed it is icons only: the traffic lights need
/// about 70pt, and the tiles want a margin either side for their shadows.
/// Expanded it adds a label beside each tile.
const double _railCollapsed = 88;
const double _railExpanded = 232;

/// Below this window width the rail starts collapsed, so the tables keep their
/// room. The user's own choice, once made, wins.
const double _autoCollapseWidth = 1100;

/// One navigation tile. Big enough to be a comfortable target and to carry a
/// visible bevel; the glyph inside is a little under half of it.
const double _tileSize = 48;
const double _tileRadius = 15;

/// Where every tile starts, whatever the rail's width — centred in the
/// collapsed rail, so collapsing slides nothing sideways.
const double _tileLeft = (_railCollapsed - _tileSize) / 2;

/// Where the labels start, a little past the tiles.
const double _labelLeft = _tileLeft + _tileSize + 14;

/// One destination's row. The spare height either side of the tile is the
/// gap between tiles, and the room the selected tab needs around its tile.
const double _rowHeight = 62;

/// The selected destination's tab: its rounded outer end, the concave curves
/// where it flows into the frame, and how far it reaches under the frame's
/// edge so the seam is covered.
const double _tabInset = 10;
const double _tabRadius = 20;
const double _fillet = 14;
const double _tabOverlap = 2;

/// The gap between the content frame and the window's edges.
const double _frameMargin = 12;
const double _frameRadius = 28;

/// The window chrome: a rail on the left, the screen on a raised frame to its
/// right.
///
/// The rail sits on the canvas, the deepest layer; the screen is moulded out
/// of it as one large panel, and the screen's own cards are moulded out of
/// that. Three depths, each stepping towards the user, which is what gives
/// the window its sense of order. The screen you are on is the one place the
/// rail and the frame meet: its tab is cut from the frame's own surface.
class AppShell extends ConsumerWidget {
  const AppShell({super.key, required this.location, required this.child});

  final String location;
  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final crawl = ref.watch(crawlControllerProvider);
    final openProject = ref.watch(openProjectProvider);
    final preference = ref.watch(
      settingsProvider.select((s) => s.sidebarCollapsed),
    );

    return CallbackShortcuts(
      bindings: {
        CrawlStatShortcuts.newCrawl: () => showNewCrawlDialog(context),
        CrawlStatShortcuts.pauseResume: () {
          final controller = ref.read(crawlControllerProvider.notifier);
          if (crawl.phase == CrawlPhase.paused) {
            controller.resume();
          } else if (crawl.phase == CrawlPhase.running) {
            controller.pause();
          }
        },
      },
      child: Focus(
        autofocus: true,
        child: Scaffold(
          body: LayoutBuilder(
            builder: (context, constraints) {
              final collapsed =
                  preference ?? constraints.maxWidth < _autoCollapseWidth;
              return TweenAnimationBuilder<double>(
                tween: Tween(end: collapsed ? _railCollapsed : _railExpanded),
                duration: const Duration(milliseconds: 240),
                curve: Curves.easeOutCubic,
                builder: (context, width, _) => Stack(
                  children: [
                    Positioned(
                      left: width,
                      top: _frameMargin,
                      right: _frameMargin,
                      bottom: _frameMargin,
                      child: _ContentFrame(child: child),
                    ),
                    // Over the frame, not beside it: the selected tab reaches
                    // a couple of points under the frame's edge to hide the
                    // seam between them.
                    Positioned(
                      left: 0,
                      top: 0,
                      bottom: 0,
                      width: width,
                      child: _Rail(
                        width: width,
                        collapsed: collapsed,
                        location: location,
                        crawl: crawl,
                        hasProject: openProject != null,
                        onToggle: () => ref
                            .read(settingsProvider.notifier)
                            .setSidebarCollapsed(!collapsed),
                      ),
                    ),
                  ],
                ),
              );
            },
          ),
        ),
      ),
    );
  }
}

/// The large panel every screen is drawn on.
class _ContentFrame extends StatelessWidget {
  const _ContentFrame({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final surface = Theme.of(context).colorScheme.surface;
    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(_frameRadius),
        gradient: Soft.rim(context, base: surface),
        boxShadow: Soft.raised(context, depth: 0.9),
      ),
      child: Padding(
        padding: const EdgeInsets.all(1),
        child: ClipRRect(
          borderRadius: BorderRadius.circular(_frameRadius - 1),
          // Solid, not the sheen the cards get: across a whole window even a
          // few percent of fall-off bands visibly.
          child: ColoredBox(color: surface, child: child),
        ),
      ),
    );
  }
}

class _Rail extends ConsumerWidget {
  const _Rail({
    required this.width,
    required this.collapsed,
    required this.location,
    required this.crawl,
    required this.hasProject,
    required this.onToggle,
  });

  /// The current width, mid-animation included.
  final double width;
  final bool collapsed;
  final String location;
  final LiveCrawl crawl;
  final bool hasProject;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final isPro = ref.watch(isProProvider);
    final live =
        crawl.isRunning ||
        crawl.phase == CrawlPhase.starting ||
        crawl.phase == CrawlPhase.auditing ||
        crawl.phase == CrawlPhase.paused;
    // 0 collapsed, 1 expanded. Everything that only exists in the wide rail
    // fades with this, so the animation never shows half-squeezed text.
    final expansion =
        ((width - _railCollapsed) / (_railExpanded - _railCollapsed)).clamp(
          0.0,
          1.0,
        );

    Widget destination(
      Glyph glyph,
      String label,
      String path, {
      bool needsProject = false,
      String? tooltip,
      LiveCrawl? liveCrawl,
      Widget? badge,
    }) {
      final enabled = !needsProject || hasProject;
      return _RailItem(
        glyph: glyph,
        label: label,
        tooltip: enabled ? tooltip : '$label — open a crawl first',
        width: width,
        expansion: expansion,
        selected: location == path,
        enabled: enabled,
        live: liveCrawl,
        badge: badge,
        onTap: () => context.go(path),
      );
    }

    return LayoutBuilder(
      builder: (context, constraints) => SingleChildScrollView(
        // Not clipped: the selected tab and its shadow reach past the rail's
        // edge into the frame.
        clipBehavior: Clip.none,
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: constraints.maxHeight),
          // A short window scrolls the rail rather than overflowing it.
          // IntrinsicHeight is what lets the Spacer push the footer down when
          // there is room.
          child: IntrinsicHeight(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const SizedBox(height: _titleBarInset + 16),
                _Brand(expansion: expansion),
                const SizedBox(height: 22),
                Padding(
                  padding: const EdgeInsets.only(left: _tileLeft),
                  child: _NewCrawlTile(
                    width: width - 2 * _tileLeft,
                    expansion: expansion,
                  ),
                ),
                const SizedBox(height: 22),
                Padding(
                  padding: EdgeInsets.only(
                    left: lerpDouble(30, _tileLeft, expansion)!,
                  ),
                  child: _Engraving(
                    width: lerpDouble(28, width - 2 * _tileLeft, expansion),
                    onCanvas: true,
                  ),
                ),
                const SizedBox(height: 22 - (_rowHeight - _tileSize) / 2),
                destination(Glyph.projects, 'Projects', Routes.projects),
                destination(
                  Glyph.crawl,
                  'Crawl',
                  Routes.crawl,
                  tooltip: live ? _crawlLabel(crawl) : null,
                  liveCrawl: live ? crawl : null,
                  badge: live
                      ? TonePill(
                          crawl.phase == CrawlPhase.paused ? 'Paused' : 'Live',
                          color: crawl.phase == CrawlPhase.paused
                              ? CrawlStatTheme.palette(context).warning
                              : CrawlStatTheme.palette(context).success,
                          dot: true,
                        )
                      : null,
                ),
                destination(
                  Glyph.results,
                  'Results',
                  Routes.results,
                  needsProject: true,
                ),
                destination(
                  Glyph.issues,
                  'Issues',
                  Routes.issues,
                  needsProject: true,
                ),
                destination(
                  Glyph.plan,
                  'Plan',
                  Routes.plan,
                  needsProject: true,
                ),
                destination(
                  Glyph.visualise,
                  'Visualise',
                  Routes.visualise,
                  needsProject: true,
                ),
                destination(
                  Glyph.readerFlow,
                  'Journeys',
                  Routes.ux,
                  needsProject: true,
                ),
                destination(
                  Glyph.accessibility,
                  'Accessibility',
                  Routes.accessibility,
                  needsProject: true,
                ),
                destination(Glyph.compare, 'Compare', Routes.compare),
                const Spacer(),
                const SizedBox(height: 16),
                if (!isPro)
                  _RailItem(
                    glyph: Glyph.pro,
                    label: 'Upgrade to Pro',
                    tooltip:
                        'CrawlStat Pro — unlimited URLs, crawl comparison '
                        'and custom extraction',
                    width: width,
                    expansion: expansion,
                    alwaysLit: true,
                    onTap: () => context.go(Routes.settings),
                  ),
                destination(Glyph.settings, 'Settings', Routes.settings),
                // Smaller than the destinations, and last: it changes the
                // window, not the screen. It also keeps the Settings tab
                // clear of the frame's rounded bottom corner.
                _RailItem(
                  icon: collapsed ? AppIcon.expand : AppIcon.collapse,
                  label: 'Collapse',
                  tooltip: collapsed ? 'Expand sidebar' : 'Collapse sidebar',
                  width: width,
                  expansion: expansion,
                  compact: true,
                  onTap: onToggle,
                ),
                const SizedBox(height: 10),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The Crawl tile's tooltip, which doubles as the live crawl's status line.
String _crawlLabel(LiveCrawl crawl) {
  final host = Uri.tryParse(crawl.startUrl ?? '')?.host;
  final site = host == null || host.isEmpty ? crawl.startUrl : host;
  final phase = switch (crawl.phase) {
    CrawlPhase.paused => 'Paused',
    CrawlPhase.auditing => 'Auditing',
    CrawlPhase.starting => 'Starting',
    _ => 'Crawling',
  };
  final parts = [
    '$phase${site == null ? '' : ' $site'}',
    if (crawl.fraction != null) '${(crawl.fraction! * 100).round()}%',
    if (crawl.snapshot != null) '${formatCount(crawl.snapshot!.crawled)} URLs',
  ];
  return parts.join(' · ');
}

/// Something that only the expanded rail shows, faded in as it opens.
///
/// Laid out at its full width the whole time and clipped to whatever room
/// there is, so a label never re-wraps or overflows while the rail animates.
class _Reveal extends StatelessWidget {
  const _Reveal({
    required this.expansion,
    required this.width,
    required this.child,
  });

  final double expansion;
  final double width;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    // The text waits until the rail is most of the way open.
    final opacity = Curves.easeIn.transform(
      ((expansion - 0.4) / 0.6).clamp(0.0, 1.0),
    );
    if (opacity == 0) return const SizedBox.shrink();
    return ClipRect(
      child: OverflowBox(
        alignment: Alignment.centerLeft,
        minWidth: width,
        maxWidth: width,
        child: Opacity(opacity: opacity, child: child),
      ),
    );
  }
}

/// The brand: the owl's head in the collapsed rail, the whole owl, wings
/// spread, once there is room for it. White line art, tinted to the theme's
/// text colour rather than shipped per theme.
class _Brand extends StatelessWidget {
  const _Brand({required this.expansion});

  final double expansion;

  @override
  Widget build(BuildContext context) {
    final ink = Theme.of(context).colorScheme.onSurface;
    const markWidth = 34.0;
    const markHeight = 36.0;
    const markLeft = (_railCollapsed - markWidth) / 2;
    const owlWidth = _railExpanded - 2 * _tileLeft;
    // The artwork's own proportions, 1197 × 579.
    //
    // This was 321 high, which is the wings-only crop: with the full owl in
    // the box the talons and the triangle below them were cut off, because
    // the box was barely half the height the artwork needs. The mark is a
    // whole owl, so the rail shows a whole owl.
    const owlHeight = owlWidth * 579 / 1197;
    // The head steps aside for the owl early, so the two never show at once
    // at full strength.
    final markOpacity = (1 - expansion * 2.5).clamp(0.0, 1.0);

    return Semantics(
      label: 'CrawlStat',
      image: true,
      child: SizedBox(
        width: _railExpanded,
        // Grows with the rail, so the rows below ease down rather than jump.
        height: lerpDouble(markHeight, owlHeight, expansion),
        child: Stack(
          children: [
            if (markOpacity > 0)
              Positioned(
                left: markLeft,
                top: 0,
                height: markHeight,
                child: Opacity(
                  opacity: markOpacity,
                  child: Image.asset(
                    'assets/brand/crawlstat_mark.png',
                    width: markWidth,
                    fit: BoxFit.contain,
                    color: ink,
                    colorBlendMode: BlendMode.srcIn,
                    filterQuality: FilterQuality.high,
                  ),
                ),
              ),
            Positioned(
              left: _tileLeft,
              right: 0,
              top: 0,
              bottom: 0,
              child: _Reveal(
                expansion: expansion,
                width: _railExpanded - 2 * _tileLeft,
                child: Center(
                  child: Image.asset(
                    'assets/brand/crawlstat_owl.png',
                    width: owlWidth,
                    fit: BoxFit.contain,
                    color: ink,
                    colorBlendMode: BlendMode.srcIn,
                    filterQuality: FilterQuality.high,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A rule cut into the surface: a shaded line with a lit one beneath it.
class _Engraving extends StatelessWidget {
  const _Engraving({this.width, this.onCanvas = false});

  final double? width;

  /// Cut into the canvas (the rail) rather than into a panel.
  final bool onCanvas;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final surface = onCanvas
        ? theme.scaffoldBackgroundColor
        : theme.colorScheme.surface;
    return SizedBox(
      width: width,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            height: 1,
            color: Color.lerp(surface, Colors.black, dark ? 0.42 : 0.1),
          ),
          Container(
            height: 1,
            color: Color.lerp(surface, Colors.white, dark ? 0.05 : 0.8),
          ),
        ],
      ),
    );
  }
}

/// The one filled tile: starting a crawl is the app's primary action. A
/// square in the collapsed rail, a full-width button in the open one.
class _NewCrawlTile extends StatefulWidget {
  const _NewCrawlTile({required this.width, required this.expansion});

  final double width;
  final double expansion;

  @override
  State<_NewCrawlTile> createState() => _NewCrawlTileState();
}

class _NewCrawlTileState extends State<_NewCrawlTile> {
  bool _hovered = false;
  bool _pressed = false;

  @override
  Widget build(BuildContext context) {
    final shortcut = defaultTargetPlatform == TargetPlatform.macOS
        ? '⌘N'
        : 'Ctrl+N';
    final corner = BorderRadius.circular(_tileRadius);
    const glyphSize = 22.0;
    const textLeft = _tileSize - 4;

    final tile = MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = _pressed = false),
      child: GestureDetector(
        onTapDown: (_) => setState(() => _pressed = true),
        onTapCancel: () => setState(() => _pressed = false),
        onTapUp: (_) => setState(() => _pressed = false),
        onTap: () => showNewCrawlDialog(context),
        child: AnimatedScale(
          duration: const Duration(milliseconds: 120),
          scale: _pressed ? 0.96 : 1,
          // Sized outside the animated box, so the width follows the rail
          // exactly instead of easing along behind it.
          child: SizedBox(
            width: math.max(_tileSize, widget.width),
            height: _tileSize,
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 160),
              decoration: BoxDecoration(
                borderRadius: corner,
                gradient: const LinearGradient(
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                  colors: Soft.emberFill,
                ),
                boxShadow: Soft.emberGlow(depth: _hovered ? 0.95 : 0.7),
              ),
              // A lit top edge, so the ember tile is moulded like the others
              // rather than a flat orange sticker.
              foregroundDecoration: BoxDecoration(
                borderRadius: corner,
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.center,
                  colors: [
                    Colors.white.withValues(alpha: _hovered ? 0.3 : 0.22),
                    Colors.white.withValues(alpha: 0),
                  ],
                ),
              ),
              child: Stack(
                children: [
                  const Positioned(
                    left: (_tileSize - glyphSize) / 2,
                    top: (_tileSize - glyphSize) / 2,
                    child: GlyphIcon(
                      Glyph.add,
                      size: glyphSize,
                      stroke: Colors.white,
                      fill: _none,
                      strokeWidth: 2.2,
                    ),
                  ),
                  Positioned(
                    left: textLeft,
                    right: 0,
                    top: 0,
                    bottom: 0,
                    child: _Reveal(
                      expansion: widget.expansion,
                      width: _railExpanded - 2 * _tileLeft - textLeft,
                      child: Padding(
                        padding: const EdgeInsets.only(right: 16),
                        child: Row(
                          children: [
                            const Expanded(
                              child: Text(
                                'New crawl',
                                maxLines: 1,
                                style: TextStyle(
                                  color: Colors.white,
                                  fontSize: 14,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ),
                            Text(
                              shortcut,
                              style: TextStyle(
                                color: Colors.white.withValues(alpha: 0.72),
                                fontSize: 12.5,
                                fontWeight: FontWeight.w500,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );

    return Tooltip(
      message: 'New crawl  ($shortcut)',
      waitDuration: const Duration(milliseconds: 350),
      child: Semantics(
        button: true,
        label: 'New crawl',
        child: ExcludeSemantics(child: tile),
      ),
    );
  }
}

const _none = LinearGradient(colors: [Color(0x00000000), Color(0x00000000)]);

/// One row of the rail: a tile, and its label when the rail is open.
///
/// At rest the tile stands out of the canvas. The screen you are on gets a
/// tab of the frame's own surface, reaching out of the frame to wrap its row,
/// with the tile raised on that and its glyph lit in the accent — so the
/// destination reads as part of the screen it opens rather than as a button
/// beside it. Keeping the accent for that one tile is what lets it draw the
/// eye.
class _RailItem extends StatefulWidget {
  const _RailItem({
    this.glyph,
    this.icon,
    required this.label,
    required this.width,
    required this.expansion,
    required this.onTap,
    this.tooltip,
    this.selected = false,
    this.enabled = true,
    this.alwaysLit = false,
    this.compact = false,
    this.live,
    this.badge,
  }) : assert(glyph != null || icon != null);

  final Glyph? glyph;

  /// A plain line icon, for controls that are not destinations.
  final IconData? icon;

  final String label;

  /// Defaults to the label, and is then only shown while the label is hidden.
  final String? tooltip;

  final double width;
  final double expansion;
  final VoidCallback onTap;
  final bool selected;
  final bool enabled;

  /// Lights the glyph's filled part even at rest, for the Pro tile.
  final bool alwaysLit;

  /// A smaller tile in a shorter row.
  final bool compact;

  /// Rings the tile with the crawl's progress.
  final LiveCrawl? live;

  /// Beside the label: a live crawl's status.
  final Widget? badge;

  @override
  State<_RailItem> createState() => _RailItemState();
}

class _RailItemState extends State<_RailItem> {
  bool _hovered = false;
  bool _pressed = false;

  @override
  Widget build(BuildContext context) {
    return TweenAnimationBuilder<double>(
      tween: Tween(end: widget.selected ? 1 : 0),
      duration: const Duration(milliseconds: 260),
      curve: Curves.easeOutCubic,
      builder: (context, tab, _) => _build(context, tab),
    );
  }

  Widget _build(BuildContext context, double tab) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final surface = theme.colorScheme.surface;
    // A tile is cut from whatever it stands on: the canvas, or the tab.
    final base = Color.lerp(theme.scaffoldBackgroundColor, surface, tab)!;
    final enabled = widget.enabled;
    final hovered = _hovered && enabled;
    final sunk = _pressed && enabled && !widget.selected;
    final size = widget.compact ? 36.0 : _tileSize;
    final radius = widget.compact ? 11.0 : _tileRadius;
    final rowHeight = widget.compact ? 52.0 : _rowHeight;
    final corner = BorderRadius.circular(radius);

    final subdued = theme.colorScheme.onSurfaceVariant;
    final stroke = !enabled
        ? subdued.withValues(alpha: 0.3)
        : widget.selected
        ? const Color(0xFFFF7A1A)
        : hovered
        ? theme.colorScheme.onSurface
        : subdued;
    final fill = widget.selected || widget.alwaysLit
        ? const LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [Color(0xFFFFA33D), Color(0xFFFF4D00)],
          )
        : LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [
              stroke.withValues(alpha: stroke.a * 0.38),
              stroke.withValues(alpha: stroke.a * 0.14),
            ],
          );

    Widget tile = AnimatedContainer(
      duration: const Duration(milliseconds: 160),
      curve: Curves.easeOut,
      width: size,
      height: size,
      padding: const EdgeInsets.all(1),
      decoration: BoxDecoration(
        borderRadius: corner,
        gradient: sunk
            ? Soft.hollowRim(context, base: base)
            : Soft.rim(context, base: base),
        boxShadow: sunk
            ? Soft.pressed(context, depth: 0.5)
            : Soft.raised(
                context,
                depth: !enabled
                    ? 0.35
                    : hovered || widget.selected
                    ? 0.8
                    : widget.compact
                    ? 0.45
                    : 0.6,
              ),
      ),
      child: DecoratedBox(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(radius - 1),
          gradient: sunk
              ? Soft.hollow(context, base: base)
              : Soft.sheen(
                  context,
                  base: hovered
                      ? Color.lerp(base, Colors.white, dark ? 0.04 : 0.4)
                      : base,
                ),
        ),
        child: Center(
          child: widget.glyph != null
              ? GlyphIcon(widget.glyph!, stroke: stroke, fill: fill)
              : Icon(widget.icon, size: size * 0.44, color: stroke),
        ),
      ),
    );
    if (widget.live != null) {
      tile = _LiveRing(crawl: widget.live!, ground: base, child: tile);
    }

    final label = Row(
      children: [
        Flexible(
          child: Text(
            widget.label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodyMedium?.copyWith(
              fontWeight: widget.selected ? FontWeight.w600 : FontWeight.w500,
              color: !enabled
                  ? subdued.withValues(alpha: 0.45)
                  : widget.selected || hovered
                  ? theme.colorScheme.onSurface
                  : subdued,
            ),
          ),
        ),
        if (widget.badge != null) ...[const SizedBox(width: 8), widget.badge!],
      ],
    );

    final row = SizedBox(
      width: widget.width,
      height: rowHeight,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          if (tab > 0)
            Positioned.fill(
              child: IgnorePointer(
                child: CustomPaint(
                  painter: _TabPainter(
                    progress: tab,
                    colour: surface,
                    rim: Soft.rim(context, base: surface).colors,
                    shadows: Soft.raised(context, depth: 0.55),
                  ),
                ),
              ),
            ),
          Positioned(
            left: _tileLeft + (_tileSize - size) / 2,
            top: (rowHeight - size) / 2,
            child: tile,
          ),
          Positioned(
            left: _labelLeft,
            right: 0,
            top: 0,
            bottom: 0,
            child: _Reveal(
              expansion: widget.expansion,
              width: _railExpanded - _labelLeft - _tileLeft,
              child: label,
            ),
          ),
        ],
      ),
    );

    final tooltip = widget.tooltip ?? widget.label;
    // Once the label is on screen a tooltip repeating it is noise; one that
    // says something more (why a tile is disabled, how a crawl is doing)
    // still earns its place.
    final showTooltip = widget.expansion < 0.5 || tooltip != widget.label;

    final interactive = Semantics(
      button: true,
      enabled: enabled,
      selected: widget.selected,
      label: tooltip,
      child: ExcludeSemantics(
        child: MouseRegion(
          cursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
          onEnter: (_) => setState(() => _hovered = true),
          onExit: (_) => setState(() => _hovered = _pressed = false),
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapDown: enabled ? (_) => setState(() => _pressed = true) : null,
            onTapCancel: () => setState(() => _pressed = false),
            onTapUp: (_) => setState(() => _pressed = false),
            onTap: enabled ? widget.onTap : null,
            child: row,
          ),
        ),
      ),
    );

    if (!showTooltip) return interactive;
    return Tooltip(
      message: tooltip,
      waitDuration: const Duration(milliseconds: 350),
      child: interactive,
    );
  }
}

/// The selected row's tab: a piece of the frame's surface reaching out to
/// wrap the row, joined to the frame by two concave curves so the two read as
/// one moulded shape.
///
/// Painted in the row's own coordinates. The curves reach [_fillet] above and
/// below the row, and the right end runs [_tabOverlap] under the frame's edge
/// to cover its rim there. [progress] grows the tab out of the frame.
class _TabPainter extends CustomPainter {
  _TabPainter({
    required this.progress,
    required this.colour,
    required this.rim,
    required this.shadows,
  });

  final double progress;
  final Color colour;
  final List<Color> rim;
  final List<BoxShadow> shadows;

  @override
  void paint(Canvas canvas, Size size) {
    final edge = size.width;
    final height = size.height;
    final left = edge - (edge - _tabInset) * progress;
    final radius = math.min(_tabRadius, math.min(edge - left, height) / 2);
    final c = _fillet * progress;

    // The outline, from the frame's edge above the row round to its edge
    // below.
    final outline = Path()..moveTo(edge, -c);
    if (c > 0) {
      outline.arcTo(
        Rect.fromCircle(center: Offset(edge - c, -c), radius: c),
        0,
        math.pi / 2,
        false,
      );
    }
    outline
      ..lineTo(left + radius, 0)
      ..arcTo(
        Rect.fromLTWH(left, 0, radius * 2, radius * 2),
        -math.pi / 2,
        -math.pi / 2,
        false,
      )
      ..lineTo(left, height - radius)
      ..arcTo(
        Rect.fromLTWH(left, height - radius * 2, radius * 2, radius * 2),
        math.pi,
        -math.pi / 2,
        false,
      )
      ..lineTo(edge - c, height);
    if (c > 0) {
      outline.arcTo(
        Rect.fromCircle(center: Offset(edge - c, height + c), radius: c),
        -math.pi / 2,
        math.pi / 2,
        false,
      );
    }

    final shape = Path.from(outline)
      ..lineTo(edge + _tabOverlap, height + c)
      ..lineTo(edge + _tabOverlap, -c)
      ..close();

    // Shadows and rim stop at the frame's edge: past it the tab is the frame.
    canvas.save();
    canvas.clipRect(Rect.fromLTRB(-100, -100, edge, height + 100));
    for (final shadow in shadows) {
      canvas.drawPath(shape.shift(shadow.offset), shadow.toPaint());
    }
    canvas.restore();

    canvas.drawPath(shape, Paint()..color = colour);

    canvas.save();
    canvas.clipRect(Rect.fromLTRB(-100, -100, edge, height + 100));
    final bounds = Rect.fromLTRB(left, -c, edge, height + c);
    canvas.drawPath(
      outline,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [rim.first, rim[1], rim.last],
          stops: const [0.15, 0.4, 0.85],
        ).createShader(bounds),
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(_TabPainter old) =>
      old.progress != progress ||
      old.colour != colour ||
      !listEquals(old.rim, rim) ||
      !listEquals(old.shadows, shadows);
}

/// A progress ring around the Crawl tile while a crawl runs, so progress is
/// in sight from every screen. Spins when the total is not yet known.
class _LiveRing extends StatefulWidget {
  const _LiveRing({
    required this.crawl,
    required this.ground,
    required this.child,
  });

  final LiveCrawl crawl;

  /// What the tile stands on, which the status dot is cut out of.
  final Color ground;
  final Widget child;

  @override
  State<_LiveRing> createState() => _LiveRingState();
}

class _LiveRingState extends State<_LiveRing>
    with SingleTickerProviderStateMixin {
  late final AnimationController _spin = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1400),
  );

  bool get _indeterminate =>
      widget.crawl.fraction == null && widget.crawl.phase != CrawlPhase.paused;

  @override
  void initState() {
    super.initState();
    _sync();
  }

  @override
  void didUpdateWidget(_LiveRing old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    if (_indeterminate) {
      if (!_spin.isAnimating) _spin.repeat();
    } else {
      _spin.stop();
    }
  }

  @override
  void dispose() {
    _spin.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final crawl = widget.crawl;
    final paused = crawl.phase == CrawlPhase.paused;
    final colour = paused
        ? CrawlStatTheme.palette(context).warning
        : CrawlStatTheme.palette(context).success;

    return Stack(
      clipBehavior: Clip.none,
      alignment: Alignment.center,
      children: [
        Positioned.fill(
          left: -5,
          top: -5,
          right: -5,
          bottom: -5,
          child: IgnorePointer(
            child: AnimatedBuilder(
              animation: _spin,
              builder: (context, _) => CustomPaint(
                painter: _RingPainter(
                  fraction: crawl.fraction ?? (paused ? 0 : 0.28),
                  rotation: _indeterminate ? _spin.value : 0,
                  track: Theme.of(context).dividerColor,
                ),
              ),
            ),
          ),
        ),
        widget.child,
        Positioned(
          top: -2,
          right: -2,
          child: IgnorePointer(
            child: Container(
              width: 11,
              height: 11,
              decoration: BoxDecoration(
                color: colour,
                shape: BoxShape.circle,
                border: Border.all(color: widget.ground, width: 2),
                boxShadow: [
                  BoxShadow(
                    color: colour.withValues(alpha: 0.6),
                    blurRadius: 8,
                  ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _RingPainter extends CustomPainter {
  _RingPainter({
    required this.fraction,
    required this.rotation,
    required this.track,
  });

  final double fraction;
  final double rotation;
  final Color track;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final shape = RRect.fromRectAndRadius(
      rect.deflate(1.25),
      const Radius.circular(_tileRadius + 5),
    );
    final path = Path()..addRRect(shape);
    final base = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.5
      ..color = track;
    canvas.drawPath(path, base);

    final metric = path.computeMetrics().first;
    final length = metric.length;
    // The rounded rectangle's path starts on its left edge; shift the start to
    // the top centre so the ring fills clockwise from twelve o'clock.
    final origin = length * (0.375 + rotation);
    final arc = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.5
      ..strokeCap = StrokeCap.round
      ..shader = SweepGradient(
        colors: Soft.accentRamp,
        transform: GradientRotation(-math.pi / 2 + rotation * 2 * math.pi),
      ).createShader(rect);
    final span = length * fraction.clamp(0, 1);
    if (span <= 0) return;
    final start = origin % length;
    final end = start + span;
    canvas.drawPath(metric.extractPath(start, math.min(end, length)), arc);
    if (end > length) {
      canvas.drawPath(metric.extractPath(0, end - length), arc);
    }
  }

  @override
  bool shouldRepaint(_RingPainter old) =>
      old.fraction != fraction ||
      old.rotation != rotation ||
      old.track != track;
}

/// The heading at the top of a screen: a large title with the screen's
/// actions on the right, and an engraved rule beneath that the content hangs
/// from.
class ScreenHeader extends StatelessWidget {
  const ScreenHeader({
    super.key,
    required this.title,
    this.subtitle,
    this.leading,
    this.actions = const [],
  });

  final String title;
  final String? subtitle;

  /// Sits after the title, on its baseline: a status pill, a count.
  final Widget? leading;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final heading = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Row(
          children: [
            Flexible(
              child: Text(
                title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.titleLarge,
              ),
            ),
            if (leading != null) ...[const SizedBox(width: 12), leading!],
          ],
        ),
        if (subtitle != null) ...[
          const SizedBox(height: 3),
          Text(
            subtitle!,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall?.copyWith(fontSize: 12.5),
          ),
        ],
      ],
    );

    return Padding(
      padding: const EdgeInsets.fromLTRB(
        CrawlStatTheme.gutter,
        24,
        CrawlStatTheme.gutter,
        20,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // A minimum height, so the rule beneath sits at the same place on
          // every screen whether or not it has a subtitle or buttons.
          ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 46),
            child: LayoutBuilder(
              builder: (context, constraints) => Row(
                children: [
                  Expanded(child: heading),
                  // Their own width, up to a cap: an equal share of the row
                  // would park them mid-screen whenever the title is short.
                  // Wrapped, because the Crawl screen can show three at once.
                  if (actions.isNotEmpty) ...[
                    const SizedBox(width: 16),
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxWidth: constraints.maxWidth * 0.6,
                      ),
                      child: Wrap(
                        alignment: WrapAlignment.end,
                        // Centred, not the default top-alignment: the round
                        // icon buttons and the pill buttons are different
                        // heights, and top-aligning them left the circles
                        // sitting visibly high on every screen header.
                        crossAxisAlignment: WrapCrossAlignment.center,
                        spacing: 8,
                        runSpacing: 8,
                        children: actions,
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
          const SizedBox(height: 18),
          const _Engraving(),
        ],
      ),
    );
  }
}

/// Shown on Results and Issues when no crawl is open.
class NoProjectState extends StatelessWidget {
  const NoProjectState({super.key});

  @override
  Widget build(BuildContext context) {
    return EmptyState(
      icon: AppIcon.folderOpen,
      title: 'No crawl open',
      message: 'Open a recent crawl from Projects, or start a new one.',
      action: FilledButton.icon(
        onPressed: () => showNewCrawlDialog(context),
        icon: const Icon(AppIcon.add, size: 15),
        label: const Text('New crawl'),
      ),
    );
  }
}
