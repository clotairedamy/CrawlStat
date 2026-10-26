import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'theme.dart';

/// Neumorphism, also called soft UI.
///
/// The look in the reference: nothing has a border. Shape comes entirely from
/// two shadows — a light one up and to the left, a dark one down and to the
/// right — on a surface the *same colour* as what is behind it. That reads as
/// a shape extruded from the background, or pressed into it.
///
/// ```text
///   raised                      inset (a "well")
///   ╭─────────╮ ← light          ╭─────────╮ ← dark
///   │ surface │                  │ surface │
///   ╰─────────╯ ← dark           ╰─────────╯ ← light
/// ```
///
/// Flip the two shadows and a button becomes a hole. That is the whole trick.
///
/// ## The honest caveat
///
/// Soft UI has a real accessibility problem, and this app now ships an
/// accessibility auditor, so it would be poor form to pretend otherwise: with
/// no borders and a surface matching its background, the *edges* of controls
/// are implied by shadow alone. At low contrast, or for anyone with reduced
/// contrast sensitivity, a button can be genuinely invisible.
///
/// So this implementation bends the style where it has to:
///
/// * Text and icons keep full contrast against the surface — the softness is
///   in the container, never in the content.
/// * Interactive things get a visible accent on hover and focus, so a keyboard
///   user can always see where they are.
/// * Anything carrying meaning by colour — severities, status codes — keeps
///   its colour at full strength rather than being muted into the surface.
class Soft {
  const Soft._();

  /// Surface colours, matched to the reference: a neutral charcoal, not a
  /// blue-grey and not black.
  ///
  /// Neumorphism needs a mid-tone to have room for a lighter *and* a darker
  /// shadow. Pure black leaves nowhere to go downward, which is why the
  /// reference sits around #2E2E2E and why every soft-UI design you have ever
  /// liked does the same.
  ///
  /// Canvas is a shade *darker* than surface, so a raised panel reads as
  /// standing out of the page even before its shadows are drawn.
  static const Color darkSurface = Color(0xFF1E1E22);
  static const Color darkCanvas = Color(0xFF131315);
  static const Color lightSurface = Color(0xFFECECEF);
  static const Color lightCanvas = Color(0xFFE2E2E6);

  /// The two lights.
  ///
  /// On the dark theme the shade is nearly opaque black: against a near-black
  /// page a translucent shadow simply disappears, and the whole surface flattens
  /// back into the background.
  static Color highlight(Brightness brightness) => brightness == Brightness.dark
      ? Colors.white.withValues(alpha: 0.07)
      : Colors.white;

  static Color shade(Brightness brightness) => brightness == Brightness.dark
      ? Colors.black.withValues(alpha: 0.8)
      : const Color(0xFFA3AEC6).withValues(alpha: 0.6);

  /// Shadows for a surface standing proud of the background.
  ///
  /// `depth` scales the whole effect: 0.5 for something subtle like a list
  /// row, 1 for a card, 1.6 for a dialog that should float.
  static List<BoxShadow> raised(BuildContext context, {double depth = 1}) {
    final brightness = Theme.of(context).brightness;
    // Wide and diffuse. The reference's shadows spread almost half the height
    // of the card they belong to; a tight 14px blur reads as Material
    // elevation, which is the opposite of the moulded look.
    final distance = 8.0 * depth;
    final blur = 28.0 * depth;
    return [
      // Two darks rather than one: a tight contact shadow right under the
      // edge, which is what reads as thickness, and a wide soft fall that
      // seats the whole shape on the page. A single blur gives you one or the
      // other and the result looks printed on.
      BoxShadow(
        color: shade(brightness),
        offset: Offset(distance * 0.45, distance * 0.6),
        blurRadius: blur * 0.3,
      ),
      BoxShadow(
        color: shade(brightness).withValues(alpha: shade(brightness).a * 0.75),
        offset: Offset(distance, distance),
        blurRadius: blur,
      ),
      BoxShadow(
        color: highlight(brightness),
        offset: Offset(-distance * 0.65, -distance * 0.65),
        blurRadius: blur * 0.75,
      ),
    ];
  }

  /// The same two lights, swapped, so the surface reads as a recess.
  ///
  /// Flutter cannot draw an inner box shadow, so a well is painted as a
  /// gradient plus a thin inner stroke — see [SoftWell].
  static List<BoxShadow> pressed(BuildContext context, {double depth = 1}) {
    final brightness = Theme.of(context).brightness;
    final distance = 5.0 * depth;
    final blur = 14.0 * depth;
    return [
      BoxShadow(
        color: highlight(brightness),
        offset: Offset(distance, distance),
        blurRadius: blur,
      ),
      BoxShadow(
        color: shade(brightness),
        offset: Offset(-distance, -distance),
        blurRadius: blur,
      ),
    ];
  }

  /// The bevel around a raised surface: bright where the top edge catches the
  /// light, dark where the bottom edge turns away from it.
  ///
  /// Drawn as a gradient *behind* the face with a 1px inset, since Flutter has
  /// no gradient border. Every colour is opaque on purpose: a translucent rim
  /// lets the surface's own shadow show through it, which is what used to
  /// turn every card into a black slab on the dark theme.
  static LinearGradient rim(BuildContext context, {Color? base}) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final surface = base ?? theme.colorScheme.surface;
    return LinearGradient(
      begin: Alignment.topLeft,
      end: Alignment.bottomRight,
      colors: [
        Color.lerp(surface, Colors.white, dark ? 0.16 : 1)!,
        Color.lerp(surface, Colors.white, dark ? 0.04 : 0.6)!,
        Color.lerp(surface, Colors.black, dark ? 0.5 : 0.12)!,
      ],
      // Most of the brightness is spent in the first stretch: a hard catch
      // along the top and left, then a long fall into the shaded edge.
      stops: const [0, 0.3, 1],
    );
  }

  /// The face of a raised surface: an opaque fall-off from top to bottom.
  ///
  /// Lit from straight above. Every surface in the reference fades this way,
  /// and that vertical fall-off is most of what separates "moulded" from "a
  /// grey rectangle". Opaque, again, so no shadow shows through the face.
  static LinearGradient sheen(BuildContext context, {Color? base}) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final surface = base ?? theme.colorScheme.surface;
    return LinearGradient(
      begin: Alignment.topCenter,
      end: Alignment.bottomCenter,
      colors: [
        Color.lerp(surface, Colors.white, dark ? 0.035 : 0.55)!,
        Color.lerp(surface, Colors.black, dark ? 0.1 : 0.025)!,
      ],
    );
  }

  /// The face of a recessed surface: dark where the top lip shades it, lit
  /// along the bottom where the light reaches in.
  static LinearGradient hollow(BuildContext context, {Color? base}) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final surface = base ?? theme.colorScheme.surface;
    return LinearGradient(
      begin: Alignment.topLeft,
      end: Alignment.bottomRight,
      colors: [
        Color.lerp(surface, Colors.black, dark ? 0.34 : 0.09)!,
        Color.lerp(surface, Colors.black, dark ? 0.16 : 0.03)!,
        Color.lerp(surface, Colors.white, dark ? 0.03 : 0.5)!,
      ],
      stops: const [0, 0.45, 1],
    );
  }

  /// The edge of a recess: the bevel turned inside out, shaded along the top
  /// and lit along the bottom.
  static LinearGradient hollowRim(BuildContext context, {Color? base}) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final surface = base ?? theme.colorScheme.surface;
    return LinearGradient(
      begin: Alignment.topLeft,
      end: Alignment.bottomRight,
      colors: [
        Color.lerp(surface, Colors.black, dark ? 0.55 : 0.16)!,
        Color.lerp(surface, Colors.white, dark ? 0.1 : 1)!,
      ],
    );
  }

  /// The accent as a gradient, taken from the theme in force.
  ///
  /// There used to be Ember constants here, used directly for the primary
  /// button, the gauge and the progress bars — so a theme could change every
  /// severity on the screen and the biggest orange object on it would stay
  /// orange. Every accent fill now comes from the palette.
  static List<Color> accentFill(BuildContext context) {
    final accent = CrawlStatTheme.palette(context).accent;
    return [
      Color.lerp(accent, Colors.black, 0.18)!,
      Color.lerp(accent, Colors.white, 0.16)!,
    ];
  }

  /// A travelling ramp from the accent, for arcs and progress bars.
  static List<Color> accentTravel(BuildContext context) {
    final accent = CrawlStatTheme.palette(context).accent;
    return [
      Color.lerp(accent, Colors.black, 0.32)!,
      accent,
      Color.lerp(accent, Colors.white, 0.14)!,
      Color.lerp(accent, Colors.white, 0.34)!,
    ];
  }

  /// The halo beneath a filled accent control.
  static List<BoxShadow> accentHalo(BuildContext context, {double depth = 1}) {
    return [
      BoxShadow(
        color: CrawlStatTheme.palette(context).accent
            .withValues(alpha: 0.38 * depth),
        blurRadius: 24 * depth,
        offset: Offset(0, 8 * depth),
      ),
    ];
  }

  static LinearGradient accentGradient(
    BuildContext context, {
    AlignmentGeometry begin = Alignment.topLeft,
    AlignmentGeometry end = Alignment.bottomRight,
  }) {
    return LinearGradient(begin: begin, end: end, colors: accentFill(context));
  }

  /// A ramp from a single colour, for arcs that must keep a specific meaning
  /// (a severity, a status class) but still want the gradient treatment.
  static List<Color> rampFrom(Color base) => [
    Color.lerp(base, Colors.white, 0.42)!,
    base,
    Color.lerp(base, Colors.black, 0.18)!,
  ];
}

/// A lit gradient from one colour, for legend swatches and small fills.
LinearGradient glossOf(Color base) {
  return LinearGradient(
    begin: Alignment.topCenter,
    end: Alignment.bottomCenter,
    colors: Soft.rampFrom(base),
  );
}

/// A surface raised out of the background.
class SoftPanel extends StatefulWidget {
  const SoftPanel({
    super.key,
    required this.child,
    this.padding,
    this.radius,
    this.depth = 1,
    this.color,
    this.onTap,
    this.pressed = false,
    this.glow,
  });

  final Widget child;
  final EdgeInsetsGeometry? padding;
  final double? radius;
  final double depth;
  final Color? color;
  final VoidCallback? onTap;

  /// Draws the panel recessed rather than raised.
  ///
  /// This is how soft UI says "selected": there are no borders to thicken and
  /// no outline to add, so the surface is pressed into the page instead. It
  /// reads instantly and needs no colour, which keeps the one accent free for
  /// the primary action.
  final bool pressed;

  /// An optional halo, for a panel that must draw the eye across a screen.
  final Color? glow;

  @override
  State<SoftPanel> createState() => _SoftPanelState();
}

class _SoftPanelState extends State<SoftPanel> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final radius = widget.radius ?? CrawlStatTheme.radius;
    final corner = BorderRadius.circular(radius);
    final surface = widget.color ?? theme.colorScheme.surface;
    final dark = theme.brightness == Brightness.dark;
    final pressed = widget.pressed;
    final inner = BorderRadius.circular(radius - 1);

    // A tappable panel answers the pointer by standing further off the page.
    // Material's own hover is a square wash drawn behind the child, which on
    // a rounded panel shows as a grey box at the corners — this is the whole
    // reason the theme turns that off.
    final lift = widget.onTap != null && _hovered && !pressed;

    final panel = AnimatedContainer(
      duration: const Duration(milliseconds: 160),
      curve: Curves.easeOut,
      decoration: BoxDecoration(
        borderRadius: corner,
        // The bevel, drawn as the outer layer; the face sits 1px inside it.
        // A recess gets the bevel inside out, which is what reads as a hole.
        gradient: pressed
            ? Soft.hollowRim(context, base: surface)
            : Soft.rim(context, base: surface),
        boxShadow: [
          if (pressed)
            ...Soft.pressed(context, depth: widget.depth * 0.8)
          else
            ...Soft.raised(context, depth: widget.depth * (lift ? 1.3 : 1)),
          if (widget.glow != null)
            BoxShadow(
              color: widget.glow!.withValues(alpha: 0.3),
              blurRadius: 22,
              spreadRadius: -2,
            ),
        ],
      ),
      padding: const EdgeInsets.all(1),
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 160),
        curve: Curves.easeOut,
        padding: widget.padding,
        decoration: BoxDecoration(
          borderRadius: inner,
          gradient: pressed
              ? Soft.hollow(context, base: surface)
              : Soft.sheen(
                  context,
                  base: lift
                      ? Color.lerp(surface, Colors.white, dark ? 0.03 : 0.4)
                      : surface,
                ),
        ),
        child: widget.child,
      ),
    );

    if (widget.onTap == null) return panel;
    return _SoftTappable(
      radius: corner,
      onTap: widget.onTap!,
      onHover: (value) => setState(() => _hovered = value),
      child: panel,
    );
  }
}

/// A recess pressed into the background.
///
/// Flutter has no inner shadow, so the depth is faked with a gradient running
/// dark-to-light across the shape plus a hairline inner stroke. At the sizes
/// this is used — gauge tracks, input fields, the well behind a donut — it is
/// indistinguishable from the real thing.
class SoftWell extends StatelessWidget {
  const SoftWell({
    super.key,
    required this.child,
    this.padding,
    this.radius,
    this.shape = BoxShape.rectangle,
    this.depth = 1,
  });

  final Widget child;
  final EdgeInsetsGeometry? padding;
  final double? radius;
  final BoxShape shape;
  final double depth;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;
    final surface = theme.colorScheme.surface;
    final corner = shape == BoxShape.circle
        ? null
        : BorderRadius.circular(radius ?? CrawlStatTheme.radius);

    return Container(
      padding: padding,
      decoration: BoxDecoration(
        shape: shape,
        borderRadius: corner,
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            Color.lerp(surface, Colors.black, dark ? 0.30 : 0.10)!,
            Color.lerp(surface, Colors.white, dark ? 0.045 : 0.6)!,
          ],
        ),
        border: Border.all(
          color: dark
              ? Colors.black.withValues(alpha: 0.28)
              : const Color(0xFFC9CEDC).withValues(alpha: 0.5),
          width: 1.2 * depth,
        ),
      ),
      child: child,
    );
  }
}

/// A button that physically sinks when pressed: raised at rest, recessed while
/// held. That is the one interaction neumorphism does better than anything
/// else, and it costs nothing to get right.
class SoftButton extends StatefulWidget {
  const SoftButton({
    super.key,
    required this.onPressed,
    required this.child,
    this.icon,
    this.gradient,
    this.padding = const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
    this.radius,
  });

  final VoidCallback? onPressed;
  final Widget child;
  final IconData? icon;

  /// Fills the button with the accent ramp rather than the surface colour.
  final Gradient? gradient;

  final EdgeInsets padding;
  final double? radius;

  @override
  State<SoftButton> createState() => _SoftButtonState();
}

class _SoftButtonState extends State<SoftButton> {
  bool _pressed = false;
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final enabled = widget.onPressed != null;
    final corner = BorderRadius.circular(
      widget.radius ?? CrawlStatTheme.controlRadius,
    );
    final onGradient = widget.gradient != null;
    final onAccent = CrawlStatTheme.palette(context).onAccent;

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
          duration: const Duration(milliseconds: 120),
          curve: Curves.easeOut,
          padding: widget.padding,
          decoration: BoxDecoration(
            gradient: onGradient
                ? widget.gradient
                : (_pressed ? Soft.hollow(context) : Soft.sheen(context)),
            borderRadius: corner,
            // Pressed: the lights swap and the button becomes a hole.
            boxShadow: !enabled
                ? null
                : _pressed
                ? Soft.pressed(context, depth: 0.6)
                : [
                    ...Soft.raised(context, depth: _hovered ? 1.15 : 0.85),
                    if (onGradient)
                      ...Soft.accentHalo(context, depth: _hovered ? 1.2 : 1),
                  ],
          ),
          child: DefaultTextStyle.merge(
            style: TextStyle(
              color: onGradient
                  ? onAccent
                  : enabled
                  ? theme.colorScheme.onSurface
                  : theme.colorScheme.onSurfaceVariant,
              fontWeight: FontWeight.w600,
              fontSize: 13,
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (widget.icon != null) ...[
                  Icon(
                    widget.icon,
                    size: 15,
                    color: onGradient ? onAccent : theme.colorScheme.onSurface,
                  ),
                  const SizedBox(width: 8),
                ],
                widget.child,
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Shared tap behaviour: a panel sinks slightly when pressed.
class _SoftTappable extends StatefulWidget {
  const _SoftTappable({
    required this.child,
    required this.radius,
    required this.onTap,
    this.onHover,
  });

  final Widget child;
  final BorderRadius radius;
  final VoidCallback onTap;
  final ValueChanged<bool>? onHover;

  @override
  State<_SoftTappable> createState() => _SoftTappableState();
}

class _SoftTappableState extends State<_SoftTappable> {
  bool _pressed = false;

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => widget.onHover?.call(true),
      onExit: (_) => widget.onHover?.call(false),
      child: GestureDetector(
        onTapDown: (_) => setState(() => _pressed = true),
        onTapUp: (_) => setState(() => _pressed = false),
        onTapCancel: () => setState(() => _pressed = false),
        onTap: widget.onTap,
        child: AnimatedScale(
          duration: const Duration(milliseconds: 110),
          scale: _pressed ? 0.988 : 1,
          child: widget.child,
        ),
      ),
    );
  }
}

/// The reference's scrubber: a recessed track, a gradient fill and a bright
/// bead at the head.
///
/// Used for crawl progress. The bead is the part that matters — a plain filled
/// bar tells you how far along you are, but the bead tells you *where the edge
/// is*, which is what the eye tracks when the thing is moving.
class SoftProgressBar extends StatelessWidget {
  const SoftProgressBar({
    super.key,
    required this.value,
    this.height = 7,
    this.ramp,
    this.showBead = true,
    this.indeterminate = false,
  });

  /// 0..1. Clamped, so a miscounted total cannot overflow the track.
  final double value;
  final double height;
  final List<Color>? ramp;
  final bool showBead;

  /// For a crawl that has started but has no total yet — a spider crawl does
  /// not know how many URLs exist until it has found them.
  final bool indeterminate;

  @override
  Widget build(BuildContext context) {
    final colours = ramp ?? Soft.accentTravel(context);
    final fraction = value.isNaN ? 0.0 : value.clamp(0.0, 1.0);

    return LayoutBuilder(
      builder: (context, constraints) {
        final width = constraints.maxWidth;
        return SizedBox(
          height: height + 10,
          child: Stack(
            alignment: Alignment.centerLeft,
            children: [
              SoftWell(
                radius: height,
                depth: 0.7,
                child: SizedBox(width: width, height: height),
              ),
              if (indeterminate)
                _IndeterminateSweep(
                  width: width,
                  height: height,
                  colours: colours,
                )
              else ...[
                TweenAnimationBuilder<double>(
                  tween: Tween(begin: 0, end: fraction),
                  duration: const Duration(milliseconds: 420),
                  curve: Curves.easeOutCubic,
                  builder: (context, grown, _) => Stack(
                    alignment: Alignment.centerLeft,
                    children: [
                      Container(
                        width: math.max(height, width * grown),
                        height: height,
                        decoration: BoxDecoration(
                          borderRadius: BorderRadius.circular(height),
                          gradient: LinearGradient(colors: colours),
                        ),
                      ),
                      if (showBead && grown > 0)
                        Positioned(
                          left: (width * grown - height * 0.9).clamp(
                            0.0,
                            width - height * 1.8,
                          ),
                          child: _Bead(size: height + 5, colour: colours.last),
                        ),
                    ],
                  ),
                ),
              ],
            ],
          ),
        );
      },
    );
  }
}

/// The head of the progress fill: a lit dot with a halo.
class _Bead extends StatelessWidget {
  const _Bead({required this.size, required this.colour});

  final double size;
  final Color colour;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: Colors.white,
        boxShadow: [
          BoxShadow(
            color: colour.withValues(alpha: 0.75),
            blurRadius: 9,
            spreadRadius: 1,
          ),
        ],
      ),
    );
  }
}

/// A sweep for a crawl with no known total.
class _IndeterminateSweep extends StatefulWidget {
  const _IndeterminateSweep({
    required this.width,
    required this.height,
    required this.colours,
  });

  final double width;
  final double height;
  final List<Color> colours;

  @override
  State<_IndeterminateSweep> createState() => _IndeterminateSweepState();
}

class _IndeterminateSweepState extends State<_IndeterminateSweep>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1400),
  )..repeat();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final span = widget.width * 0.32;
    return AnimatedBuilder(
      animation: _controller,
      builder: (context, _) {
        final travel = Curves.easeInOut.transform(_controller.value);
        return Positioned(
          left: (widget.width + span) * travel - span,
          child: Container(
            width: span,
            height: widget.height,
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(widget.height),
              gradient: LinearGradient(
                colors: [
                  widget.colours.first.withValues(alpha: 0),
                  widget.colours.last,
                  widget.colours.first.withValues(alpha: 0),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

/// A pill filled with the accent gradient — the "Filter By" control in the
/// reference.
class SoftPill extends StatelessWidget {
  const SoftPill({super.key, required this.label, this.onTap, this.gradient});

  final String label;
  final VoidCallback? onTap;
  final Gradient? gradient;

  @override
  Widget build(BuildContext context) {
    return SoftButton(
      onPressed: onTap,
      radius: 100,
      gradient: gradient ?? Soft.accentGradient(context),
      padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 10),
      child: Text(label),
    );
  }
}

/// A circular icon button, raised out of the page.
///
/// The signature control of the reference: every action there — add, play,
/// skip, favourite — is the same soft circle, and using them consistently is
/// what makes a screen read as this style rather than as a dark dashboard with
/// rounded corners.
class SoftCircleButton extends StatefulWidget {
  const SoftCircleButton({
    super.key,
    required this.icon,
    required this.onPressed,
    this.size = 42,
    this.iconSize,
    this.filled = false,
    this.tooltip,
    this.iconColor,
  });

  final IconData icon;
  final VoidCallback? onPressed;
  final double size;
  final double? iconSize;

  /// Fills the circle with the accent instead of the surface colour. At most
  /// one filled circle per screen — it is the primary action.
  final bool filled;

  final String? tooltip;
  final Color? iconColor;

  @override
  State<SoftCircleButton> createState() => _SoftCircleButtonState();
}

class _SoftCircleButtonState extends State<SoftCircleButton> {
  bool _pressed = false;
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final enabled = widget.onPressed != null;
    final scale = widget.size / 42;

    Widget circle = AnimatedContainer(
      duration: const Duration(milliseconds: 120),
      curve: Curves.easeOut,
      width: widget.size,
      height: widget.size,
      padding: const EdgeInsets.all(1),
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        // Same construction as a panel: a lit rim with the face inside it.
        gradient: _pressed ? Soft.hollowRim(context) : Soft.rim(context),
        boxShadow: !enabled
            ? null
            : _pressed
            ? Soft.pressed(context, depth: 0.5 * scale)
            : [
                ...Soft.raised(context, depth: (_hovered ? 0.95 : 0.7) * scale),
                if (widget.filled) ...Soft.accentHalo(context, depth: scale),
              ],
      ),
      child: DecoratedBox(
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          gradient: widget.filled
              ? Soft.accentGradient(context)
              : (_pressed ? Soft.hollow(context) : Soft.sheen(context)),
        ),
        child: Center(
          child: Icon(
            widget.icon,
            size: widget.iconSize ?? widget.size * 0.42,
            color:
                widget.iconColor ??
                (widget.filled
                    ? CrawlStatTheme.palette(context).onAccent
                    : enabled
                    ? theme.colorScheme.onSurface.withValues(
                        alpha: _hovered ? 1 : 0.8,
                      )
                    : theme.colorScheme.onSurfaceVariant),
          ),
        ),
      ),
    );

    if (widget.tooltip != null) {
      circle = Tooltip(message: widget.tooltip!, child: circle);
    }

    return MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        onTapDown: enabled ? (_) => setState(() => _pressed = true) : null,
        onTapUp: enabled ? (_) => setState(() => _pressed = false) : null,
        onTapCancel: enabled ? () => setState(() => _pressed = false) : null,
        onTap: widget.onPressed,
        child: Semantics(
          button: true,
          enabled: enabled,
          label: widget.tooltip,
          child: circle,
        ),
      ),
    );
  }
}
