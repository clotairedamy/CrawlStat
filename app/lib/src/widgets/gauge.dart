import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../theme/soft.dart';
import '../theme/theme.dart';

/// A speedometer for a running crawl.
///
/// A crawl has a natural "speed" — URLs per second — and a natural
/// completion, and both change every second. A dial shows a changing rate far
/// better than a number does: the eye reads the *position* of the needle
/// without reading the digits, so you can tell at a glance whether a crawl is
/// moving or has stalled.
///
/// ```text
///         ╭ ticks ╮
///      ·············        outer ring: the scale
///    ·   ╭───────╮   ·
///   ·   │   850   │   ·     centre: the live rate
///   ·   │  URL/s  │   ·
///    ·   ╰───────╯   ·      inner arc: progress through the crawl
///      ·············
/// ```
///
/// The dial is drawn, not animated frame by frame: it moves only when a new
/// value arrives — about twice a second during a crawl — and interpolates
/// between the two. A gauge repainting at 60fps for four hours is a laptop fan
/// at full speed for no benefit.
class CrawlGauge extends StatelessWidget {
  const CrawlGauge({
    super.key,
    required this.value,
    required this.max,
    this.label = 'URL/s',
    this.size = 260,
    this.progress,
    this.secondaryValue,
    this.secondaryLabel,
    this.ramp,
    this.live = true,
  });

  /// What the needle reads. The scale grows if this exceeds [max].
  final double value;
  final double max;
  final String label;
  final double size;

  /// The inner arc, 0..1. The share of the crawl that is done.
  final double? progress;

  /// A second figure under the main reading, like the reference's "3.2 G".
  final String? secondaryValue;
  final String? secondaryLabel;

  /// Colours for the main arc. Defaults to the accent ramp.
  final List<Color>? ramp;

  /// A stopped crawl keeps its last reading but drops the glow, so a finished
  /// dial does not look like a running one.
  final bool live;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    // Never let the needle pin: a crawl that beats its own scale re-scales.
    final ceiling = math.max(max, value * 1.08);

    return SizedBox(
      width: size,
      height: size,
      child: Stack(
        alignment: Alignment.center,
        children: [
          // A real instrument is a bezel standing off the panel with the face
          // set down inside it. Two layers, not one: the outer ring catches
          // the light, the face below it is in shadow.
          Container(
            width: size,
            height: size,
            padding: const EdgeInsets.all(1),
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              gradient: Soft.rim(context),
              boxShadow: Soft.raised(context, depth: 1.25),
            ),
            child: Container(
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                gradient: Soft.sheen(context),
              ),
              padding: EdgeInsets.all(size * 0.055),
              child: SoftWell(
                shape: BoxShape.circle,
                depth: 1.1,
                child: const SizedBox.expand(),
              ),
            ),
          ),
          TweenAnimationBuilder<double>(
            tween: Tween(begin: 0, end: value.clamp(0, ceiling)),
            duration: const Duration(milliseconds: 520),
            curve: Curves.easeOutCubic,
            builder: (context, animatedValue, _) {
              return TweenAnimationBuilder<double>(
                tween: Tween(begin: 0, end: (progress ?? 0).clamp(0, 1)),
                duration: const Duration(milliseconds: 620),
                curve: Curves.easeOutCubic,
                builder: (context, animatedProgress, _) => CustomPaint(
                  size: Size.square(size),
                  painter: _GaugePainter(
                    value: animatedValue,
                    max: ceiling,
                    progress: progress == null ? null : animatedProgress,
                    ramp: ramp ?? Soft.accentTravel(context),
                    tickColor: theme.colorScheme.onSurfaceVariant.withValues(
                      alpha: 0.30,
                    ),
                    tickTextColor: theme.colorScheme.onSurfaceVariant,
                    trackColor: Colors.black.withValues(
                      alpha: theme.brightness == Brightness.dark ? 0.35 : 0.10,
                    ),
                    progressColor: theme.colorScheme.onSurfaceVariant,
                    glow: live,
                  ),
                ),
              );
            },
          ),
          // Inset so a long reading can never run over the ticks, which is
          // what the first version did.
          Padding(
            padding: EdgeInsets.symmetric(horizontal: size * 0.235),
            child: _Readout(
              value: value,
              label: label,
              secondaryValue: secondaryValue,
              secondaryLabel: secondaryLabel,
            ),
          ),
        ],
      ),
    );
  }
}

class _Readout extends StatelessWidget {
  const _Readout({
    required this.value,
    required this.label,
    this.secondaryValue,
    this.secondaryLabel,
  });

  final double value;
  final String label;
  final String? secondaryValue;
  final String? secondaryLabel;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        // The rate loses its decimal once it is big enough not to need one.
        FittedBox(
          fit: BoxFit.scaleDown,
          child: Text(
            value >= 100 ? value.toStringAsFixed(0) : value.toStringAsFixed(1),
            maxLines: 1,
            style: CrawlStatTheme.mono(context, size: 42).copyWith(
              fontWeight: FontWeight.w300,
              height: 1,
              letterSpacing: -1.5,
            ),
          ),
        ),
        const SizedBox(height: 2),
        Text(label, style: theme.textTheme.labelMedium),
        if (secondaryValue != null) ...[
          const SizedBox(height: 10),
          // Scaled down rather than clipped: the dial is a fixed circle and
          // the figures inside it are whatever the crawl happens to produce,
          // so the text has to yield, not the box.
          FittedBox(
            fit: BoxFit.scaleDown,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.baseline,
              textBaseline: TextBaseline.alphabetic,
              children: [
                Text(
                  secondaryValue!,
                  style: CrawlStatTheme.mono(
                    context,
                    size: 17,
                  ).copyWith(fontWeight: FontWeight.w600),
                ),
                if (secondaryLabel != null) ...[
                  const SizedBox(width: 5),
                  Text(secondaryLabel!, style: theme.textTheme.labelMedium),
                ],
              ],
            ),
          ),
        ],
      ],
    );
  }
}

class _GaugePainter extends CustomPainter {
  _GaugePainter({
    required this.value,
    required this.max,
    required this.progress,
    required this.ramp,
    required this.tickColor,
    required this.tickTextColor,
    required this.trackColor,
    required this.progressColor,
    required this.glow,
  });

  final double value;
  final double max;
  final double? progress;
  final List<Color> ramp;
  final Color tickColor;
  final Color tickTextColor;
  final Color trackColor;
  final Color progressColor;
  final bool glow;

  @override
  void paint(Canvas canvas, Size size) {
    final centre = size.center(Offset.zero);
    final outer = size.width / 2;

    _paintTicks(canvas, centre, outer);

    // The rate: one bright gradient arc, and the only saturated thing on the
    // dial.
    final arcRadius = outer - size.width * 0.155;
    _paintArc(
      canvas,
      centre: centre,
      radius: arcRadius,
      fraction: max <= 0 ? 0 : (value / max).clamp(0.0, 1.0),
      width: 6,
      colors: ramp,
      glowing: glow,
    );

    // Progress through the crawl, a thin ring further in.
    //
    // Deliberately colourless. The first version drew this in cyan and teal,
    // which fought the orange for attention and belonged to no palette in the
    // app — two bright arcs on one small dial read as a mess.
    if (progress != null) {
      _paintArc(
        canvas,
        centre: centre,
        radius: arcRadius - 12,
        fraction: progress!.clamp(0.0, 1.0),
        width: 3,
        colors: [
          progressColor.withValues(alpha: 0.55),
          progressColor.withValues(alpha: 0.85),
        ],
        glowing: false,
        bead: false,
      );
    }
  }

  /// The scale: a long tick every fifth, short ones between.
  ///
  /// Unlabelled on purpose. Numerals round the rim were the worst part of the
  /// first dial: at this size they were 9pt, they collided with the readout
  /// where the sweep passes the bottom, and the live figure in the centre
  /// already says the number exactly.
  void _paintTicks(Canvas canvas, Offset centre, double outer) {
    const divisions = 40;
    final paint = Paint()..strokeCap = StrokeCap.round;

    for (var index = 0; index <= divisions; index++) {
      final fraction = index / divisions;
      final angle = gaugeStart + fraction * gaugeSweep;
      final major = index % 5 == 0;

      final length = major ? outer * 0.058 : outer * 0.028;
      final reached = max > 0 && fraction <= (value / max);

      paint
        ..color = reached
            ? _rampAt(fraction, ramp).withValues(alpha: 0.85)
            : tickColor
        ..strokeWidth = major ? 2 : 1;

      // Inside the bezel, not on it: the ring is raised and the ticks are
      // printed on the recessed face below.
      final start = outer - outer * 0.105;
      final from = Offset(
        centre.dx + math.cos(angle) * start,
        centre.dy + math.sin(angle) * start,
      );
      final to = Offset(
        centre.dx + math.cos(angle) * (start - length),
        centre.dy + math.sin(angle) * (start - length),
      );
      canvas.drawLine(from, to, paint);
    }
  }

  void _paintArc(
    Canvas canvas, {
    required Offset centre,
    required double radius,
    required double fraction,
    required double width,
    required List<Color> colors,
    required bool glowing,
    bool bead = true,
  }) {
    final rect = Rect.fromCircle(center: centre, radius: radius);

    // The unfilled remainder, so the dial has a visible track.
    canvas.drawArc(
      rect,
      gaugeStart,
      gaugeSweep,
      false,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = width
        ..strokeCap = StrokeCap.round
        ..color = trackColor,
    );

    if (fraction <= 0) return;
    final sweep = gaugeSweep * fraction;

    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = width
      ..strokeCap = StrokeCap.round
      ..shader = SweepGradient(
        startAngle: gaugeStart,
        endAngle: gaugeStart + gaugeSweep,
        colors: colors,
        transform: GradientRotation(gaugeStart),
      ).createShader(rect);

    if (glowing) {
      canvas.drawArc(
        rect,
        gaugeStart,
        sweep,
        false,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = width + 8
          ..strokeCap = StrokeCap.round
          ..color = _rampAt(fraction, colors).withValues(alpha: 0.26)
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 7),
      );
    }

    canvas.drawArc(rect, gaugeStart, sweep, false, paint);

    if (!bead) return;

    // The bead at the tip, which is what makes the dial look alive.
    final tipAngle = gaugeStart + sweep;
    final tip = Offset(
      centre.dx + math.cos(tipAngle) * radius,
      centre.dy + math.sin(tipAngle) * radius,
    );
    final tipColour = _rampAt(fraction, colors);

    if (glowing) {
      canvas.drawCircle(
        tip,
        width + 5,
        Paint()
          ..color = tipColour.withValues(alpha: 0.5)
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 6),
      );
    }
    canvas.drawCircle(tip, width * 0.95, Paint()..color = Colors.white);
    canvas.drawCircle(tip, width * 0.62, Paint()..color = tipColour);
  }

  @override
  bool shouldRepaint(_GaugePainter old) =>
      old.value != value ||
      old.max != max ||
      old.progress != progress ||
      old.progressColor != progressColor ||
      old.glow != glow;
}

// ─── geometry ────────────────────────────────────────────────────────────

/// Where the dial begins, in radians: down and to the left, the way a car's
/// speedometer does. Measured from the positive x axis, clockwise.
const double gaugeStart = math.pi * 0.75;

/// How far it travels: three quarters of a turn, ending down and to the right.
/// A full circle would leave no gap to tell the start from the end.
const double gaugeSweep = math.pi * 1.5;

/// The angle a reading sits at.
double gaugeAngle(double value, double max) {
  if (max <= 0) return gaugeStart;
  return gaugeStart + (value / max).clamp(0.0, 1.0) * gaugeSweep;
}

/// Sample a colour ramp at `fraction`, interpolating between stops.
Color _rampAt(double fraction, List<Color> ramp) {
  if (ramp.isEmpty) return Colors.white;
  if (ramp.length == 1) return ramp.first;

  final position = fraction.clamp(0.0, 1.0) * (ramp.length - 1);
  final index = position.floor().clamp(0, ramp.length - 2);
  return Color.lerp(ramp[index], ramp[index + 1], position - index)!;
}

/// Exposed for testing: the ramp sampler above.
Color rampColourAt(double fraction, List<Color> ramp) =>
    _rampAt(fraction, ramp);

/// Round a live rate up to a sensible dial maximum, and keep it there.
///
/// A scale that moved with every reading would make the needle meaningless —
/// it would sit in the same place whatever the rate. So the dial only ever
/// grows, in steps, and never shrinks during a crawl.
double gaugeCeiling(double observedPeak, {double floor = 10}) {
  if (observedPeak <= floor) return floor;
  final magnitude = math
      .pow(10, (math.log(observedPeak) / math.ln10).floor())
      .toDouble();
  final normalised = observedPeak / magnitude;
  final step = switch (normalised) {
    <= 1 => 1.0,
    <= 2 => 2.0,
    <= 2.5 => 2.5,
    <= 5 => 5.0,
    _ => 10.0,
  };
  return step * magnitude;
}
