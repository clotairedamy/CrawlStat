import 'dart:math' as math;

import 'package:flutter/widgets.dart';

/// The navigation's own icons, painted rather than taken from a font.
///
/// Line icons are right for a dense table, but in the rail they were the
/// least engaging thing on screen: six thin outlines in six identical grey
/// tiles. These are duotone — an outline plus one filled part that carries
/// the accent — so the place you are on lights up in colour, and the others
/// still have some weight to them at rest.
///
/// Every glyph is drawn on a 24-unit grid, the same as Lucide, so they sit
/// at the same optical size as the line icons used everywhere else.
enum Glyph {
  projects,
  crawl,
  results,
  issues,
  plan,
  visualise,
  readerFlow,
  accessibility,
  compare,
  settings,
  add,
  pro,
}

class GlyphIcon extends StatelessWidget {
  const GlyphIcon(
    this.glyph, {
    super.key,
    required this.stroke,
    required this.fill,
    this.size = 22,
    this.strokeWidth = 1.9,
  });

  final Glyph glyph;
  final Color stroke;

  /// The second tone. A gradient lights the filled part from above, which is
  /// what makes the selected glyph read as glowing rather than coloured in.
  final Gradient fill;

  final double size;
  final double strokeWidth;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: CustomPaint(
        painter: _GlyphPainter(glyph, stroke, fill, strokeWidth),
      ),
    );
  }
}

class _GlyphPainter extends CustomPainter {
  _GlyphPainter(this.glyph, this.stroke, this.fill, this.strokeWidth);

  final Glyph glyph;
  final Color stroke;
  final Gradient fill;
  final double strokeWidth;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.scale(size.width / 24);
    final grid = Offset.zero & const Size(24, 24);
    final line = Paint()
      ..color = stroke
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final tone = Paint()..shader = fill.createShader(grid);
    final ink = Paint()..color = stroke;

    switch (glyph) {
      case Glyph.projects:
        // Four tiles, one of them lit: a board of crawls.
        final lit = _rrect(3.5, 3.5, 10.5, 10.5, 2.4);
        canvas.drawRRect(lit, tone);
        canvas.drawRRect(lit, line);
        canvas.drawRRect(_rrect(13.5, 3.5, 20.5, 10.5, 2.4), line);
        canvas.drawRRect(_rrect(3.5, 13.5, 10.5, 20.5, 2.4), line);
        canvas.drawCircle(const Offset(17, 17), 3.6, line);

      case Glyph.crawl:
        // A radar: the sweep is the lit part, with a blip caught in it.
        const centre = Offset(12, 12);
        final dial = Rect.fromCircle(center: centre, radius: 9);
        const start = -math.pi / 2;
        const sweep = 1.35;
        final wedge = Path()
          ..moveTo(centre.dx, centre.dy)
          ..arcTo(dial, start, sweep, false)
          ..close();
        canvas.drawPath(wedge, tone);
        canvas.drawCircle(centre, 9, line);
        canvas.drawCircle(centre, 4.8, line..strokeWidth = strokeWidth * 0.8);
        line.strokeWidth = strokeWidth;
        canvas.drawLine(centre, centre + _polar(9, start + sweep), line);
        canvas.drawCircle(centre, 1.7, ink);
        canvas.drawCircle(centre + _polar(6.6, start + 0.55), 1.35, ink);

      case Glyph.results:
        // A table whose header row is the lit part.
        final frame = _rrect(3.5, 4, 20.5, 20, 3);
        canvas.save();
        canvas.clipRRect(frame);
        canvas.drawRect(const Rect.fromLTRB(3.5, 4, 20.5, 9.5), tone);
        canvas.restore();
        canvas.drawRRect(frame, line);
        canvas.drawLine(const Offset(3.5, 9.5), const Offset(20.5, 9.5), line);
        canvas.drawLine(
          const Offset(3.5, 14.8),
          const Offset(20.5, 14.8),
          line,
        );
        canvas.drawLine(const Offset(9.6, 9.5), const Offset(9.6, 20), line);

      case Glyph.issues:
        // A clipboard of checks: the audit.
        canvas.drawRRect(_rrect(5, 4.5, 19, 21, 2.6), line);
        final clip = _rrect(8.5, 2.5, 15.5, 6.6, 1.6);
        canvas.drawRRect(clip, tone);
        canvas.drawRRect(clip, line);
        for (final y in const [11.8, 16.8]) {
          canvas.drawPath(
            Path()
              ..moveTo(8.1, y)
              ..lineTo(9.5, y + 1.4)
              ..lineTo(11.8, y - 0.9),
            line,
          );
          canvas.drawLine(Offset(13.8, y + 0.2), Offset(16, y + 0.2), line);
        }

      case Glyph.plan:
        // A ranked list: three bars, longest first, with the top one lit.
        // The plan is the issues sorted by what they are worth, so the glyph
        // is the issues glyph's content put in order.
        final top = _rrect(4.5, 5, 19.5, 8.6, 1.8);
        canvas.drawRRect(top, tone);
        canvas.drawRRect(top, line);
        canvas.drawRRect(_rrect(4.5, 10.8, 16, 14.4, 1.8), line);
        canvas.drawRRect(_rrect(4.5, 16.6, 12, 20.2, 1.8), line);

      case Glyph.visualise:
        // A hub and three pages: the site's shape.
        const hub = Offset(12, 12.6);
        const hubRadius = 3.4;
        const leaves = [Offset(5.6, 5.6), Offset(18.4, 5.6), Offset(12, 20.4)];
        const leafRadius = 2.3;
        for (final leaf in leaves) {
          final direction = (leaf - hub) / (leaf - hub).distance;
          canvas.drawLine(
            hub + direction * (hubRadius + 0.9),
            leaf - direction * (leafRadius + 0.9),
            line,
          );
          canvas.drawCircle(leaf, leafRadius, line);
        }
        canvas.drawCircle(hub, hubRadius, tone);
        canvas.drawCircle(hub, hubRadius, line);

      case Glyph.readerFlow:
        // A pointer mid-click, the arrow lit: someone using the page.
        final pointer = Path()
          ..moveTo(9.4, 9.4)
          ..lineTo(20.4, 13.9)
          ..lineTo(15.9, 15.5)
          ..lineTo(14.3, 20.2)
          ..close();
        canvas.drawPath(pointer, tone);
        canvas.drawPath(pointer, line);
        canvas.drawPath(
          Path()
            ..moveTo(7.7, 2.9)
            ..lineTo(8.3, 5.4)
            ..moveTo(2.9, 7.7)
            ..lineTo(5.4, 8.3)
            ..moveTo(13.4, 4.4)
            ..lineTo(11.7, 6.1)
            ..moveTo(4.4, 13.4)
            ..lineTo(6.1, 11.7),
          line,
        );

      case Glyph.accessibility:
        // The universal access figure in a ring, its head lit.
        const ring = Offset(12, 12);
        canvas.drawCircle(ring, 9.4, line);
        canvas.drawCircle(const Offset(12, 7), 2, tone);
        canvas.drawCircle(const Offset(12, 7), 2, line);
        canvas.drawPath(
          Path()
            ..moveTo(7, 10)
            ..quadraticBezierTo(12, 11.6, 17, 10)
            ..moveTo(12, 11)
            ..lineTo(12, 14.2)
            ..lineTo(9.8, 18.2)
            ..moveTo(12, 14.2)
            ..lineTo(14.2, 18.2),
          line,
        );

      case Glyph.compare:
        // Before and after: a tall bar, a shorter lit one, and the arc from
        // one to the other.
        canvas.drawRRect(_rrect(4, 10, 9.8, 20.5, 2.2), line);
        final after = _rrect(14.2, 14, 20, 20.5, 2.2);
        canvas.drawRRect(after, tone);
        canvas.drawRRect(after, line);
        canvas.drawPath(
          Path()
            ..moveTo(6.9, 7)
            ..cubicTo(7.8, 3.4, 17.1, 2.8, 17.1, 10)
            ..moveTo(14.8, 7.9)
            ..lineTo(17.1, 10.2)
            ..lineTo(19.4, 7.9),
          line,
        );

      case Glyph.settings:
        // A gear with a lit hub.
        const centre = Offset(12, 12);
        final gear = Path();
        const teeth = 8;
        for (var k = 0; k < teeth; k++) {
          final angle = k * 2 * math.pi / teeth;
          const root = 0.25;
          const tip = 0.15;
          final points = [
            centre + _polar(7.2, angle - root),
            centre + _polar(9.6, angle - tip),
            centre + _polar(9.6, angle + tip),
            centre + _polar(7.2, angle + root),
          ];
          if (k == 0) {
            gear.moveTo(points.first.dx, points.first.dy);
          } else {
            gear.lineTo(points.first.dx, points.first.dy);
          }
          for (final point in points.skip(1)) {
            gear.lineTo(point.dx, point.dy);
          }
        }
        gear.close();
        canvas.drawPath(gear, line);
        canvas.drawCircle(centre, 3.3, tone);
        canvas.drawCircle(centre, 3.3, line);

      case Glyph.add:
        line.strokeWidth = strokeWidth * 1.25;
        canvas.drawLine(const Offset(12, 5.5), const Offset(12, 18.5), line);
        canvas.drawLine(const Offset(5.5, 12), const Offset(18.5, 12), line);

      case Glyph.pro:
        // A large sparkle and a small one.
        final large = _sparkle(const Offset(10.5, 13), 8);
        canvas.drawPath(large, tone);
        canvas.drawPath(large, line);
        canvas.drawPath(_sparkle(const Offset(18.6, 5.6), 3.2), ink);
    }
  }

  static RRect _rrect(double l, double t, double r, double b, double radius) =>
      RRect.fromLTRBR(l, t, r, b, Radius.circular(radius));

  static Offset _polar(double radius, double angle) =>
      Offset(math.cos(angle) * radius, math.sin(angle) * radius);

  /// A four-pointed star with concave sides.
  static Path _sparkle(Offset centre, double radius) {
    final waist = radius * 0.22;
    return Path()
      ..moveTo(centre.dx, centre.dy - radius)
      ..quadraticBezierTo(
        centre.dx + waist,
        centre.dy - waist,
        centre.dx + radius,
        centre.dy,
      )
      ..quadraticBezierTo(
        centre.dx + waist,
        centre.dy + waist,
        centre.dx,
        centre.dy + radius,
      )
      ..quadraticBezierTo(
        centre.dx - waist,
        centre.dy + waist,
        centre.dx - radius,
        centre.dy,
      )
      ..quadraticBezierTo(
        centre.dx - waist,
        centre.dy - waist,
        centre.dx,
        centre.dy - radius,
      )
      ..close();
  }

  @override
  bool shouldRepaint(_GlyphPainter old) =>
      old.glyph != glyph ||
      old.stroke != stroke ||
      old.fill != fill ||
      old.strokeWidth != strokeWidth;
}
