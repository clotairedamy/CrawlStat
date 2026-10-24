import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../theme/soft.dart';
import '../theme/theme.dart';
import 'common.dart';

/// Charts, painted rather than pulled in from a package.
///
/// Four shapes cover everything a crawl has to say, and writing them by hand
/// keeps the app free of a charting dependency, keeps them on the same palette
/// as the rest of the UI, and makes them cheap — a crawl screen repainting at
/// 60fps cannot afford a general-purpose chart library.
///
/// The arithmetic lives in free functions at the bottom so it can be tested
/// without pumping a widget.

/// One slice, bar or series.
class ChartDatum {
  const ChartDatum({
    required this.label,
    required this.value,
    required this.color,
    this.hint,
  });

  final String label;
  final num value;
  final Color color;

  /// Extra line in the tooltip.
  final String? hint;
}

// ─── donut ───────────────────────────────────────────────────────────────

/// A ring with a total in the middle.
///
/// Used for response codes, where the question is "what proportion of this
/// crawl is broken?" — a part-to-whole question, which is the only thing a
/// donut is actually good at.
class DonutChart extends StatefulWidget {
  const DonutChart({
    super.key,
    required this.data,
    this.size = 150,
    this.thickness = 18,
    this.centreLabel,
    this.centreValue,
    this.onTap,
  });

  final List<ChartDatum> data;
  final double size;
  final double thickness;
  final String? centreLabel;
  final String? centreValue;
  final void Function(ChartDatum)? onTap;

  @override
  State<DonutChart> createState() => _DonutChartState();
}

class _DonutChartState extends State<DonutChart>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 650),
  )..forward();

  int? _hovered;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final total = widget.data.fold<num>(0, (sum, datum) => sum + datum.value);

    if (total == 0) {
      return _EmptyChart(size: widget.size, message: 'Nothing to chart yet');
    }

    final segments = donutSegments(
      widget.data.map((datum) => datum.value).toList(),
    );

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        MouseRegion(
          onExit: (_) => setState(() => _hovered = null),
          child: GestureDetector(
            onTapUp: widget.onTap == null
                ? null
                : (details) {
                    final index = _segmentAt(details.localPosition, segments);
                    if (index != null) widget.onTap!(widget.data[index]);
                  },
            child: MouseRegion(
              onHover: (event) => setState(
                () => _hovered = _segmentAt(event.localPosition, segments),
              ),
              child: SizedBox(
                width: widget.size,
                height: widget.size,
                child: AnimatedBuilder(
                  animation: _controller,
                  builder: (context, _) => CustomPaint(
                    painter: _DonutPainter(
                      data: widget.data,
                      segments: segments,
                      thickness: widget.thickness,
                      progress: Curves.easeOutCubic.transform(
                        _controller.value,
                      ),
                      hovered: _hovered,
                      trackColor: theme.dividerColor.withValues(alpha: 0.4),
                    ),
                    child: Center(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            widget.centreValue ?? formatCount(total),
                            style: CrawlStatTheme.mono(
                              context,
                              size: 22,
                            ).copyWith(fontWeight: FontWeight.w600),
                          ),
                          if (widget.centreLabel != null)
                            Text(
                              widget.centreLabel!.toUpperCase(),
                              style: theme.textTheme.labelMedium,
                            ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
        const SizedBox(width: 20),
        Flexible(
          child: _Legend(
            data: widget.data,
            total: total,
            highlighted: _hovered,
            onHover: (index) => setState(() => _hovered = index),
            onTap: widget.onTap,
          ),
        ),
      ],
    );
  }

  /// Which segment a point falls in, or `null` outside the ring.
  int? _segmentAt(Offset position, List<DonutSegment> segments) {
    final centre = Offset(widget.size / 2, widget.size / 2);
    final vector = position - centre;
    final distance = vector.distance;
    final outer = widget.size / 2;
    if (distance > outer || distance < outer - widget.thickness - 4) {
      return null;
    }

    // atan2 gives -pi..pi from the positive x axis; the ring starts at 12
    // o'clock, so rotate a quarter turn and wrap.
    var angle = math.atan2(vector.dy, vector.dx) + math.pi / 2;
    if (angle < 0) angle += 2 * math.pi;

    for (var index = 0; index < segments.length; index++) {
      final segment = segments[index];
      if (angle >= segment.start && angle < segment.start + segment.sweep) {
        return index;
      }
    }
    return null;
  }
}

class _DonutPainter extends CustomPainter {
  _DonutPainter({
    required this.data,
    required this.segments,
    required this.thickness,
    required this.progress,
    required this.hovered,
    required this.trackColor,
  });

  final List<ChartDatum> data;
  final List<DonutSegment> segments;
  final double thickness;
  final double progress;
  final int? hovered;
  final Color trackColor;

  @override
  void paint(Canvas canvas, Size size) {
    final centre = size.center(Offset.zero);
    final radius = size.width / 2 - thickness / 2;

    canvas.drawCircle(
      centre,
      radius,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = thickness
        ..color = trackColor,
    );

    for (var index = 0; index < segments.length; index++) {
      final segment = segments[index];
      final isHovered = hovered == index;
      final colour = data[index].color;

      // A hovered slice thickens outward and glows, rather than exploding out
      // of the ring — which breaks the part-to-whole read.
      final paint = Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = thickness + (isHovered ? 5 : 0)
        ..strokeCap = StrokeCap.butt
        ..shader = SweepGradient(
          startAngle: segment.start,
          endAngle: segment.start + segment.sweep,
          colors: [colour.withValues(alpha: 0.78), colour],
          transform: GradientRotation(-math.pi / 2),
        ).createShader(Rect.fromCircle(center: centre, radius: radius));

      if (isHovered) {
        canvas.drawArc(
          Rect.fromCircle(center: centre, radius: radius),
          segment.start - math.pi / 2,
          segment.sweep * progress,
          false,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = thickness + 12
            ..color = colour.withValues(alpha: 0.2)
            ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 6),
        );
      }

      canvas.drawArc(
        Rect.fromCircle(center: centre, radius: radius),
        segment.start - math.pi / 2,
        segment.sweep * progress,
        false,
        paint,
      );
    }
  }

  @override
  bool shouldRepaint(_DonutPainter old) =>
      old.progress != progress ||
      old.hovered != hovered ||
      old.segments != segments;
}

// ─── bars ────────────────────────────────────────────────────────────────

/// A vertical bar chart, for the crawl-depth distribution.
///
/// Depth is the one place a bar chart genuinely beats a table: the *shape* of
/// the distribution is the finding. A site whose pages pile up at depth six
/// has a navigation problem, and that is visible at a glance and invisible in
/// a list of numbers.
class BarChart extends StatefulWidget {
  const BarChart({
    super.key,
    required this.data,
    this.height = 160,
    this.barWidth = 30,
    this.onTap,
  });

  final List<ChartDatum> data;
  final double height;
  final double barWidth;
  final void Function(ChartDatum)? onTap;

  @override
  State<BarChart> createState() => _BarChartState();
}

class _BarChartState extends State<BarChart>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 600),
  )..forward();

  int? _hovered;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (widget.data.isEmpty) {
      return _EmptyChart(size: widget.height, message: 'No pages crawled yet');
    }

    final max = niceAxisMax(
      widget.data.fold<num>(0, (best, datum) => math.max(best, datum.value)),
    );

    return SizedBox(
      height: widget.height,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          // Axis labels, so the bars mean a number and not just "more".
          SizedBox(
            height: widget.height,
            child: Column(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Text(formatCount(max), style: theme.textTheme.labelMedium),
                Text(formatCount(max ~/ 2), style: theme.textTheme.labelMedium),
                // Leaves room for the x-axis labels under the bars.
                const SizedBox(height: 20, child: Text('0')),
              ],
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  for (var index = 0; index < widget.data.length; index++)
                    _Bar(
                      datum: widget.data[index],
                      max: max,
                      width: widget.barWidth,
                      height: widget.height - 20,
                      controller: _controller,
                      hovered: _hovered == index,
                      onHover: (hovering) =>
                          setState(() => _hovered = hovering ? index : null),
                      onTap: widget.onTap,
                    ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Bar extends StatelessWidget {
  const _Bar({
    required this.datum,
    required this.max,
    required this.width,
    required this.height,
    required this.controller,
    required this.hovered,
    required this.onHover,
    this.onTap,
  });

  final ChartDatum datum;
  final num max;
  final double width;
  final double height;
  final AnimationController controller;
  final bool hovered;
  final void Function(bool) onHover;
  final void Function(ChartDatum)? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final fraction = max == 0 ? 0.0 : datum.value / max;

    return Tooltip(
      message: datum.hint ?? '${datum.label}: ${formatCount(datum.value)}',
      child: MouseRegion(
        onEnter: (_) => onHover(true),
        onExit: (_) => onHover(false),
        cursor: onTap == null ? MouseCursor.defer : SystemMouseCursors.click,
        child: GestureDetector(
          onTap: onTap == null ? null : () => onTap!(datum),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 4),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                AnimatedBuilder(
                  animation: controller,
                  builder: (context, _) {
                    final grown =
                        Curves.easeOutCubic.transform(controller.value) *
                        fraction;
                    return Container(
                      width: width,
                      height: math.max(3, height * grown),
                      decoration: BoxDecoration(
                        // Lighter at the top: the bar reads as lit from above,
                        // matching the glass surfaces around it.
                        gradient: LinearGradient(
                          begin: Alignment.topCenter,
                          end: Alignment.bottomCenter,
                          colors: [
                            Color.lerp(
                              datum.color,
                              Colors.white,
                              hovered ? 0.4 : 0.22,
                            )!,
                            datum.color,
                          ],
                        ),
                        borderRadius: const BorderRadius.vertical(
                          top: Radius.circular(5),
                        ),
                        boxShadow: hovered
                            ? [
                                BoxShadow(
                                  color: datum.color.withValues(alpha: 0.5),
                                  blurRadius: 12,
                                  offset: const Offset(0, 2),
                                ),
                              ]
                            : null,
                      ),
                    );
                  },
                ),
                const SizedBox(height: 5),
                SizedBox(
                  height: 15,
                  child: Text(datum.label, style: theme.textTheme.labelMedium),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

// ─── sparkline ───────────────────────────────────────────────────────────

/// A small line with a gradient underneath, for crawl rate over time.
///
/// No axes and no labels: the point is the shape, and a sparkline that needs
/// decoding is just a small bad chart.
class Sparkline extends StatelessWidget {
  const Sparkline({
    super.key,
    required this.values,
    this.color,
    this.height = 42,
    this.width,
  });

  final List<double> values;
  final Color? color;
  final double height;
  final double? width;

  @override
  Widget build(BuildContext context) {
    final line = color ?? CrawlStatTheme.accent;
    return SizedBox(
      height: height,
      width: width,
      child: values.length < 2
          ? const SizedBox.shrink()
          : CustomPaint(
              painter: _SparklinePainter(values: values, color: line),
            ),
    );
  }
}

class _SparklinePainter extends CustomPainter {
  _SparklinePainter({required this.values, required this.color});

  final List<double> values;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final points = sparklinePoints(values, size.width, size.height);
    if (points.length < 2) return;

    final path = Path()..moveTo(points.first.dx, points.first.dy);
    for (final point in points.skip(1)) {
      path.lineTo(point.dx, point.dy);
    }

    // The fill under the line, fading to nothing at the baseline.
    final fill = Path.from(path)
      ..lineTo(points.last.dx, size.height)
      ..lineTo(points.first.dx, size.height)
      ..close();

    canvas.drawPath(
      fill,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [color.withValues(alpha: 0.34), color.withValues(alpha: 0)],
        ).createShader(Offset.zero & size),
    );

    canvas.drawPath(
      path,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round
        ..color = color,
    );

    // A dot on the newest value, so "now" is findable.
    canvas.drawCircle(points.last, 3, Paint()..color = color);
    canvas.drawCircle(
      points.last,
      6,
      Paint()
        ..color = color.withValues(alpha: 0.3)
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3),
    );
  }

  @override
  bool shouldRepaint(_SparklinePainter old) => old.values != values;
}

// ─── severity bar ────────────────────────────────────────────────────────

/// One stacked bar: errors, warnings and notices in proportion.
///
/// Reads at a glance from across a room, which is what the Issues header needs.
class SeverityBar extends StatelessWidget {
  const SeverityBar({
    super.key,
    required this.errors,
    required this.warnings,
    required this.notices,
    this.height = 10,
  });

  final int errors;
  final int warnings;
  final int notices;
  final double height;

  @override
  Widget build(BuildContext context) {
    final total = errors + warnings + notices;
    if (total == 0) {
      return Container(
        height: height,
        decoration: BoxDecoration(
          color: CrawlStatTheme.success.withValues(alpha: 0.25),
          borderRadius: BorderRadius.circular(height / 2),
        ),
      );
    }

    return ClipRRect(
      borderRadius: BorderRadius.circular(height / 2),
      child: SizedBox(
        height: height,
        child: Row(
          children: [
            for (final (count, colour) in [
              (errors, CrawlStatTheme.error),
              (warnings, CrawlStatTheme.warning),
              (notices, CrawlStatTheme.notice),
            ])
              if (count > 0)
                Expanded(
                  flex: count,
                  child: DecoratedBox(
                    decoration: BoxDecoration(gradient: glossOf(colour)),
                  ),
                ),
          ],
        ),
      ),
    );
  }
}

// ─── shared pieces ───────────────────────────────────────────────────────

class _Legend extends StatelessWidget {
  const _Legend({
    required this.data,
    required this.total,
    required this.highlighted,
    required this.onHover,
    this.onTap,
  });

  final List<ChartDatum> data;
  final num total;
  final int? highlighted;
  final void Function(int?) onHover;
  final void Function(ChartDatum)? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var index = 0; index < data.length; index++)
          MouseRegion(
            onEnter: (_) => onHover(index),
            onExit: (_) => onHover(null),
            cursor: onTap == null
                ? MouseCursor.defer
                : SystemMouseCursors.click,
            child: GestureDetector(
              onTap: onTap == null ? null : () => onTap!(data[index]),
              child: AnimatedContainer(
                duration: const Duration(milliseconds: 120),
                padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 4),
                margin: const EdgeInsets.only(bottom: 2),
                decoration: BoxDecoration(
                  color: highlighted == index
                      ? data[index].color.withValues(alpha: 0.14)
                      : null,
                  borderRadius: BorderRadius.circular(6),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      width: 9,
                      height: 9,
                      decoration: BoxDecoration(
                        gradient: glossOf(data[index].color),
                        borderRadius: BorderRadius.circular(3),
                      ),
                    ),
                    const SizedBox(width: 8),
                    // Flexible, so a narrow card shortens the label rather
                    // than overflowing the row.
                    Flexible(
                      child: SizedBox(
                        width: 86,
                        child: Text(
                          data[index].label,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall,
                        ),
                      ),
                    ),
                    Text(
                      formatCount(data[index].value),
                      style: CrawlStatTheme.mono(context, size: 11.5),
                    ),
                    const SizedBox(width: 6),
                    SizedBox(
                      width: 38,
                      child: Text(
                        percentLabel(data[index].value, total),
                        textAlign: TextAlign.right,
                        style: theme.textTheme.labelMedium,
                      ),
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

class _EmptyChart extends StatelessWidget {
  const _EmptyChart({required this.size, required this.message});

  final double size;
  final String message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      height: size,
      child: Center(child: Text(message, style: theme.textTheme.bodySmall)),
    );
  }
}

// ─── the arithmetic ──────────────────────────────────────────────────────

/// One slice of a donut, in radians clockwise from 12 o'clock.
class DonutSegment {
  const DonutSegment({
    required this.start,
    required this.sweep,
    required this.fraction,
  });

  final double start;
  final double sweep;
  final double fraction;

  @override
  bool operator ==(Object other) =>
      other is DonutSegment &&
      other.start == start &&
      other.sweep == sweep &&
      other.fraction == fraction;

  @override
  int get hashCode => Object.hash(start, sweep, fraction);
}

/// Lay values out around a ring.
///
/// Zero-valued entries get a zero sweep rather than being dropped, so a slice's
/// index always matches its datum's — which is what makes hover and tap
/// mapping correct.
List<DonutSegment> donutSegments(List<num> values) {
  final total = values.fold<num>(0, (sum, value) => sum + math.max(0, value));
  if (total <= 0) return const [];

  final segments = <DonutSegment>[];
  var cursor = 0.0;
  for (final value in values) {
    final fraction = math.max(0, value) / total;
    final sweep = fraction * 2 * math.pi;
    segments.add(
      DonutSegment(start: cursor, sweep: sweep, fraction: fraction.toDouble()),
    );
    cursor += sweep;
  }
  return segments;
}

/// Round an axis maximum up to something a person would choose.
///
/// 43 becomes 50, 1,340 becomes 1,500. An axis topping out at exactly the
/// largest bar makes that bar look like the ceiling rather than the peak.
num niceAxisMax(num largest) {
  if (largest <= 0) return 1;
  if (largest <= 5) return 5;

  final magnitude = math
      .pow(10, (math.log(largest) / math.ln10).floor())
      .toDouble();
  final normalised = largest / magnitude;
  final step = switch (normalised) {
    <= 1 => 1.0,
    <= 1.5 => 1.5,
    <= 2 => 2.0,
    <= 3 => 3.0,
    <= 5 => 5.0,
    _ => 10.0,
  };
  return (step * magnitude).round();
}

/// Map a series onto points inside a box, newest last.
///
/// A flat series sits on the middle line rather than collapsing to the floor,
/// which would read as zero.
List<Offset> sparklinePoints(List<double> values, double width, double height) {
  if (values.length < 2 || width <= 0 || height <= 0) return const [];

  final lowest = values.reduce(math.min);
  final highest = values.reduce(math.max);
  final span = highest - lowest;
  final step = width / (values.length - 1);

  return [
    for (var index = 0; index < values.length; index++)
      Offset(
        index * step,
        span == 0
            ? height / 2
            : height - ((values[index] - lowest) / span) * height,
      ),
  ];
}

/// `12%`, or `<1%` for a non-zero sliver that would otherwise round to nothing.
String percentLabel(num value, num total) {
  if (total <= 0 || value <= 0) return '0%';
  final percent = value / total * 100;
  if (percent < 1) return '<1%';
  return '${percent.round()}%';
}
