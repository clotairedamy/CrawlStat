import 'dart:ui';

import 'package:flutter/material.dart';

import 'theme.dart';

/// The glass layer.
///
/// Glassmorphism only reads as glass if there is something behind it to
/// refract. A blurred panel over a flat fill just looks like a slightly wrong
/// grey. So this file provides two halves that belong together:
///
/// * [GlassBackdrop] — the ambient colour behind everything, a few soft blooms
///   of accent light on the canvas.
/// * [GlassPanel] and friends — surfaces that blur whatever is behind them.
///
/// It is deliberately additive: it reads [CrawlStatTheme]'s tokens and adds no
/// new ones of its own, so changing the accent or the radius there changes the
/// glass too.
///
/// ## Where glass belongs, and where it does not
///
/// On chrome — panels, dialogs, the rail, headers, cards. **Not** behind a data
/// grid. A hundred thousand rows over a live blur is both unreadable and a
/// repaint per frame of a full-screen `BackdropFilter`, which is one of the
/// most expensive things Flutter can be asked to do. The grid keeps an opaque
/// surface, and the glass frames it.
class Glass {
  const Glass._();

  /// Blur radius behind a panel. Enough to abstract what is behind without
  /// turning it into mud.
  static const double blur = 24;

  /// A lighter blur for small chrome, where a heavy one reads as smudged.
  static const double blurLight = 14;

  /// How opaque the frosted fill is. Dark needs more than light: a translucent
  /// white over a dark canvas greys out far faster than the reverse.
  static double fillOpacity(Brightness brightness) =>
      brightness == Brightness.dark ? 0.52 : 0.72;

  /// The hairline highlight along a panel's top edge.
  ///
  /// This is the single detail that sells glass: real glass catches light on
  /// its top bevel. Without it a blurred panel looks like a blurred rectangle.
  static Color topHighlight(Brightness brightness) => brightness == Brightness.dark
      ? Colors.white.withValues(alpha: 0.14)
      : Colors.white.withValues(alpha: 0.85);

  static Color edge(BuildContext context) {
    final theme = Theme.of(context);
    return theme.brightness == Brightness.dark
        ? Colors.white.withValues(alpha: 0.08)
        : Colors.black.withValues(alpha: 0.06);
  }

  /// A glossy fill for primary actions: a bright top half falling to the
  /// accent, the way a lit convex surface behaves.
  static LinearGradient gloss(Color base) {
    return LinearGradient(
      begin: Alignment.topCenter,
      end: Alignment.bottomCenter,
      colors: [
        Color.lerp(base, Colors.white, 0.26)!,
        base,
        Color.lerp(base, Colors.black, 0.10)!,
      ],
      stops: const [0, 0.55, 1],
    );
  }

  /// A very soft shadow. Glass sits *above* the canvas, so it needs some
  /// separation — but a heavy drop shadow reads as plastic.
  static List<BoxShadow> lift(BuildContext context, {double strength = 1}) {
    final dark = Theme.of(context).brightness == Brightness.dark;
    return [
      BoxShadow(
        color: Colors.black.withValues(alpha: (dark ? 0.34 : 0.07) * strength),
        blurRadius: 28 * strength,
        offset: Offset(0, 10 * strength),
      ),
    ];
  }
}

/// The ambient light behind the whole app.
///
/// Three slow blooms of colour on the canvas. They are what the glass panels
/// pick up and bend, and they are why the window looks lit rather than filled.
/// Static rather than animated: this sits behind every frame of a long crawl,
/// and an animated gradient would repaint the window forever for decoration.
class GlassBackdrop extends StatelessWidget {
  const GlassBackdrop({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final canvas = theme.colorScheme.surfaceContainerLowest;

    return DecoratedBox(
      decoration: BoxDecoration(color: canvas),
      child: Stack(
        children: [
          // Accent bloom, top left — behind the rail, so the chrome glows.
          _Bloom(
            alignment: const Alignment(-0.85, -0.9),
            color: CrawlStatTheme.accent.withValues(alpha: dark ? 0.26 : 0.16),
            size: 620,
          ),
          // A cooler counterweight, bottom right.
          _Bloom(
            alignment: const Alignment(1.1, 0.95),
            color: (dark ? const Color(0xFF22B573) : const Color(0xFF6366F1))
                .withValues(alpha: dark ? 0.16 : 0.10),
            size: 720,
          ),
          // A faint warm centre, to stop the middle going dead.
          _Bloom(
            alignment: const Alignment(0.35, -0.35),
            color: const Color(0xFFF2A12E).withValues(alpha: dark ? 0.08 : 0.06),
            size: 520,
          ),
          child,
        ],
      ),
    );
  }
}

class _Bloom extends StatelessWidget {
  const _Bloom({required this.alignment, required this.color, required this.size});

  final Alignment alignment;
  final Color color;
  final double size;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: alignment,
      child: IgnorePointer(
        child: Container(
          width: size,
          height: size,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            gradient: RadialGradient(colors: [color, color.withValues(alpha: 0)]),
          ),
        ),
      ),
    );
  }
}

/// A frosted surface.
///
/// Blurs whatever is behind it, fills translucently, and catches a highlight
/// along its top edge.
class GlassPanel extends StatelessWidget {
  const GlassPanel({
    super.key,
    required this.child,
    this.padding,
    this.radius,
    this.blur,
    this.opacity,
    this.lifted = true,
    this.border = true,
    this.tint,
  });

  final Widget child;
  final EdgeInsetsGeometry? padding;
  final double? radius;
  final double? blur;

  /// Overrides the frosted fill's opacity. Raise it where text sits directly
  /// on the panel and legibility matters more than the effect.
  final double? opacity;

  final bool lifted;
  final bool border;

  /// Tints the frost, for a panel that should read as a severity.
  final Color? tint;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final corner = BorderRadius.circular(radius ?? CrawlStatTheme.radius);
    final base = tint ?? theme.colorScheme.surface;
    final fill = base.withValues(alpha: opacity ?? Glass.fillOpacity(theme.brightness));

    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: corner,
        boxShadow: lifted ? Glass.lift(context) : null,
      ),
      child: ClipRRect(
        borderRadius: corner,
        child: BackdropFilter(
          filter: ImageFilter.blur(
            sigmaX: blur ?? Glass.blur,
            sigmaY: blur ?? Glass.blur,
          ),
          child: Container(
            decoration: BoxDecoration(
              color: fill,
              borderRadius: corner,
              border: border ? Border.all(color: Glass.edge(context)) : null,
              // The lit top bevel: a bright hairline fading out within a few
              // pixels, which is what reads as a glass edge rather than a
              // border.
              gradient: LinearGradient(
                begin: Alignment.topCenter,
                end: Alignment.bottomCenter,
                colors: [
                  Glass.topHighlight(theme.brightness),
                  fill.withValues(alpha: 0),
                ],
                stops: const [0, 0.035],
              ),
            ),
            child: Padding(padding: padding ?? EdgeInsets.zero, child: child),
          ),
        ),
      ),
    );
  }
}

/// A glass panel with a title bar, replacing a plain `Card` for grouped
/// content.
class GlassSection extends StatelessWidget {
  const GlassSection({
    super.key,
    required this.child,
    this.title,
    this.trailing,
    this.padding = const EdgeInsets.all(16),
  });

  final Widget child;
  final String? title;
  final Widget? trailing;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return GlassPanel(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (title != null)
            Container(
              padding: const EdgeInsets.fromLTRB(16, 12, 10, 12),
              decoration: BoxDecoration(
                border: Border(bottom: BorderSide(color: Glass.edge(context))),
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

/// A primary action with a lit, convex surface.
class GlossyButton extends StatefulWidget {
  const GlossyButton({
    super.key,
    required this.onPressed,
    required this.child,
    this.icon,
    this.color,
    this.padding = const EdgeInsets.symmetric(horizontal: 18, vertical: 11),
  });

  final VoidCallback? onPressed;
  final Widget child;
  final IconData? icon;
  final Color? color;
  final EdgeInsets padding;

  @override
  State<GlossyButton> createState() => _GlossyButtonState();
}

class _GlossyButtonState extends State<GlossyButton> {
  bool _hovered = false;
  bool _pressed = false;

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onPressed != null;
    final base = widget.color ?? CrawlStatTheme.accent;
    final corner = BorderRadius.circular(CrawlStatTheme.controlRadius);

    return MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        onTapDown: enabled ? (_) => setState(() => _pressed = true) : null,
        onTapUp: enabled ? (_) => setState(() => _pressed = false) : null,
        onTapCancel: enabled ? () => setState(() => _pressed = false) : null,
        onTap: widget.onPressed,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 110),
          curve: Curves.easeOut,
          padding: widget.padding,
          decoration: BoxDecoration(
            borderRadius: corner,
            gradient: Glass.gloss(
              enabled
                  ? (_hovered ? Color.lerp(base, Colors.white, 0.08)! : base)
                  : base.withValues(alpha: 0.4),
            ),
            boxShadow: enabled && !_pressed
                ? [
                    BoxShadow(
                      color: base.withValues(alpha: _hovered ? 0.42 : 0.3),
                      blurRadius: _hovered ? 18 : 12,
                      offset: const Offset(0, 4),
                    ),
                  ]
                : null,
            border: Border.all(color: Colors.white.withValues(alpha: 0.18)),
          ),
          // A press sinks the button a pixel rather than dimming it.
          transform: Matrix4.translationValues(0, _pressed ? 1 : 0, 0),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (widget.icon != null) ...[
                Icon(widget.icon, size: 15, color: Colors.white),
                const SizedBox(width: 7),
              ],
              DefaultTextStyle.merge(
                style: const TextStyle(
                  color: Colors.white,
                  fontWeight: FontWeight.w600,
                  fontSize: 13,
                ),
                child: widget.child,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A thin frosted strip, for toolbars and headers that sit over content.
class GlassBar extends StatelessWidget {
  const GlassBar({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
    this.bottomBorder = true,
  });

  final Widget child;
  final EdgeInsets padding;
  final bool bottomBorder;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ClipRect(
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: Glass.blurLight, sigmaY: Glass.blurLight),
        child: Container(
          padding: padding,
          decoration: BoxDecoration(
            color: theme.colorScheme.surface
                .withValues(alpha: theme.brightness == Brightness.dark ? 0.62 : 0.76),
            border: bottomBorder
                ? Border(bottom: BorderSide(color: Glass.edge(context)))
                : null,
          ),
          child: child,
        ),
      ),
    );
  }
}
