import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import '../theme/soft.dart';
import '../theme/theme.dart';

/// A node-link diagram of a crawled site.
///
/// This is the picture an SEO person actually wants: not a list of URLs but
/// the *shape* of the site — which pages are hubs, how far from the home page
/// a visitor has to travel, and which corners hang off a single link and would
/// vanish if it broke.
///
/// ```text
///            ○ ← a leaf: one inlink, far from home
///          ╱
///   ●────●──── ○      ● big, central: a hub
///    ╲  ╱ ╲
///     ●     ○         rings = crawl depth from the start URL
/// ```
///
/// ## How it is laid out
///
/// A force-directed simulation: every node pushes every other node away, every
/// link pulls its two ends together, and each node is held near the ring that
/// matches its crawl depth. Run to a rest state, that produces clusters that
/// correspond to real sections of a site, which is the whole point — the
/// picture has to be readable, not merely correct.
///
/// The depth ring is what makes it an *SEO* diagram rather than a generic
/// graph: distance from the centre is clicks from the home page.
///
/// The simulation is cooled by a falling temperature and then stops. A graph
/// that jiggles forever is a laptop fan at full speed for no benefit, and a
/// diagram you cannot point at.

/// A page in the diagram.
class GraphNodeData {
  const GraphNodeData({
    required this.id,
    required this.url,
    required this.label,
    required this.depth,
    required this.inlinks,
    this.status,
  });

  final int id;
  final String url;
  final String label;
  final int depth;
  final int inlinks;
  final int? status;
}

/// A link between two pages in the diagram.
class GraphEdgeData {
  const GraphEdgeData({required this.source, required this.target});

  final int source;
  final int target;
}

/// A node's position and velocity during the simulation.
class GraphParticle {
  GraphParticle({required this.x, required this.y});

  double x;
  double y;
  double vx = 0;
  double vy = 0;
}

/// How big a node is drawn, from how many pages link to it.
///
/// Square-rooted, not linear: a page with a hundred inlinks is a more
/// important hub than one with ten, but it is not ten times the hub, and
/// drawing it ten times the area makes everything else invisible.
double nodeRadius(int inlinks, {double base = 4.5, double scale = 2.4}) {
  return base + math.sqrt(inlinks.toDouble()) * scale;
}

/// The ring a node of this depth sits on, as a fraction of the radius.
///
/// Depth 0 is the centre. The gaps shrink as depth grows, so a site twelve
/// levels deep still fits without the first few rings being squashed together.
double depthRing(int depth, int maxDepth) {
  if (depth <= 0) return 0;
  if (maxDepth <= 0) return 1;
  // Square root again: equal area per ring rather than equal width, so deep
  // levels, which hold most of the pages, get the room they need.
  return math.sqrt(depth / maxDepth);
}

/// The colour of a node, by its HTTP status.
///
/// The diagram's one job beyond shape is to show you where the site is broken,
/// so a 4xx page is red wherever it sits.
Color nodeColour(int? status, Color fallback) {
  if (status == null) return CrawlStatTheme.notice;
  if (status >= 500) return const Color(0xFFB91C4B);
  if (status >= 400) return CrawlStatTheme.error;
  if (status >= 300) return CrawlStatTheme.warning;
  return fallback;
}

/// Run the force simulation one step.
///
/// Pure, and takes its state as arguments, so the physics can be tested
/// without a widget tree — which matters, because a layout bug here looks
/// exactly like a rendering bug and is far harder to find by eye.
void relaxGraph(
  List<GraphParticle> particles,
  List<(int, int)> edges,
  List<double> ringFor, {
  required double temperature,
  double repulsion = 2400,
  double springLength = 46,
  double springStrength = 0.016,
  double ringStrength = 0.055,
  double damping = 0.82,
}) {
  final count = particles.length;
  if (count == 0) return;

  // Repulsion, every pair. Quadratic, which is fine at the few hundred nodes
  // this diagram is capped to and far simpler than a Barnes-Hut tree.
  for (var i = 0; i < count; i++) {
    final a = particles[i];
    for (var j = i + 1; j < count; j++) {
      final b = particles[j];
      var dx = a.x - b.x;
      var dy = a.y - b.y;
      var distanceSquared = dx * dx + dy * dy;
      if (distanceSquared < 0.01) {
        // Two nodes exactly on top of each other have no direction to push
        // in, so give them one rather than dividing by zero.
        dx = (i.isEven ? 1 : -1) * 0.1;
        dy = (j.isEven ? 1 : -1) * 0.1;
        distanceSquared = dx * dx + dy * dy;
      }
      final distance = math.sqrt(distanceSquared);
      final force = repulsion / distanceSquared;
      final fx = dx / distance * force;
      final fy = dy / distance * force;
      a.vx += fx;
      a.vy += fy;
      b.vx -= fx;
      b.vy -= fy;
    }
  }

  // Springs along the links.
  for (final (source, target) in edges) {
    if (source < 0 || source >= count || target < 0 || target >= count) {
      continue;
    }
    final a = particles[source];
    final b = particles[target];
    final dx = b.x - a.x;
    final dy = b.y - a.y;
    final distance = math.sqrt(dx * dx + dy * dy).clamp(0.01, double.infinity);
    final force = (distance - springLength) * springStrength;
    final fx = dx / distance * force;
    final fy = dy / distance * force;
    a.vx += fx;
    a.vy += fy;
    b.vx -= fx;
    b.vy -= fy;
  }

  // The depth ring: pull each node towards its own radius from the centre.
  for (var i = 0; i < count; i++) {
    final particle = particles[i];
    final target = ringFor[i];
    final distance = math.sqrt(
      particle.x * particle.x + particle.y * particle.y,
    );
    if (distance < 0.01) {
      // The home page belongs at the centre, so leave it there.
      if (target < 0.01) continue;
      particle.vx += target * ringStrength;
      continue;
    }
    final pull = (target - distance) * ringStrength;
    particle.vx += particle.x / distance * pull;
    particle.vy += particle.y / distance * pull;
  }

  for (final particle in particles) {
    particle.vx *= damping;
    particle.vy *= damping;
    // Temperature caps how far anything can move in one step, so the layout
    // settles instead of exploding on the first few frames when every node is
    // piled on top of every other.
    final speed = math.sqrt(
      particle.vx * particle.vx + particle.vy * particle.vy,
    );
    if (speed > temperature) {
      particle.vx = particle.vx / speed * temperature;
      particle.vy = particle.vy / speed * temperature;
    }
    particle.x += particle.vx;
    particle.y += particle.vy;
  }
}

/// The diagram.
class SiteGraphView extends StatefulWidget {
  const SiteGraphView({
    super.key,
    required this.nodes,
    required this.edges,
    this.onSelected,
    this.selectedId,
  });

  final List<GraphNodeData> nodes;
  final List<GraphEdgeData> edges;
  final ValueChanged<GraphNodeData>? onSelected;
  final int? selectedId;

  @override
  State<SiteGraphView> createState() => _SiteGraphViewState();
}

class _SiteGraphViewState extends State<SiteGraphView>
    with SingleTickerProviderStateMixin {
  late List<GraphParticle> _particles;
  late List<double> _rings;
  late List<(int, int)> _edgeIndices;
  late Map<int, int> _indexOf;
  late int _maxDepth;

  Ticker? _ticker;
  double _temperature = 24;
  int _step = 0;

  /// Pan and zoom, applied on top of the simulation's own coordinates.
  Offset _pan = Offset.zero;
  double _zoom = 1;
  int? _hovered;
  Offset? _pointer;

  /// How many steps the layout is given before it is declared settled.
  static const int _settleSteps = 420;

  @override
  void initState() {
    super.initState();
    _build();
    _start();
  }

  @override
  void didUpdateWidget(SiteGraphView old) {
    super.didUpdateWidget(old);
    // Only re-lay-out when the graph itself changed — a selection or a hover
    // must never shuffle the diagram under the pointer.
    if (old.nodes.length != widget.nodes.length ||
        old.edges.length != widget.edges.length) {
      _build();
      _start();
    }
  }

  void _build() {
    _indexOf = {
      for (final (index, node) in widget.nodes.indexed) node.id: index,
    };
    _maxDepth = widget.nodes.fold<int>(0, (a, n) => math.max(a, n.depth));

    // Seeded deterministically: the same crawl lays out the same way every
    // time it is opened, so the picture a user learned yesterday is the
    // picture they see today.
    final random = math.Random(1709);
    _particles = [
      for (final node in widget.nodes)
        () {
          final ring = depthRing(node.depth, _maxDepth) * 240;
          final angle = random.nextDouble() * math.pi * 2;
          return GraphParticle(
            x: math.cos(angle) * ring + random.nextDouble() * 6 - 3,
            y: math.sin(angle) * ring + random.nextDouble() * 6 - 3,
          );
        }(),
    ];
    _rings = [
      for (final node in widget.nodes) depthRing(node.depth, _maxDepth) * 240,
    ];
    _edgeIndices = [
      for (final edge in widget.edges)
        if (_indexOf[edge.source] != null && _indexOf[edge.target] != null)
          (_indexOf[edge.source]!, _indexOf[edge.target]!),
    ];
  }

  void _start() {
    _ticker?.dispose();
    _step = 0;
    _temperature = 24;
    if (_particles.isEmpty) return;
    _ticker = createTicker((_) {
      if (_step >= _settleSteps) {
        _ticker?.stop();
        return;
      }
      // Several steps per frame early on, so the layout resolves in about a
      // second instead of seven.
      final iterations = _step < 120 ? 4 : 1;
      for (var i = 0; i < iterations; i++) {
        relaxGraph(_particles, _edgeIndices, _rings, temperature: _temperature);
        _temperature = math.max(0.35, _temperature * 0.985);
        _step++;
      }
      setState(() {});
    })..start();
  }

  @override
  void dispose() {
    _ticker?.dispose();
    super.dispose();
  }

  /// Fit the laid-out graph into the view, then apply the user's pan and zoom.
  (Offset, double) _transform(Size size) {
    if (_particles.isEmpty) return (size.center(Offset.zero), _zoom);
    var minX = double.infinity, maxX = -double.infinity;
    var minY = double.infinity, maxY = -double.infinity;
    for (final particle in _particles) {
      minX = math.min(minX, particle.x);
      maxX = math.max(maxX, particle.x);
      minY = math.min(minY, particle.y);
      maxY = math.max(maxY, particle.y);
    }
    final width = math.max(1.0, maxX - minX);
    final height = math.max(1.0, maxY - minY);
    const margin = 56.0;
    final fit = math.min(
      (size.width - margin) / width,
      (size.height - margin) / height,
    );
    final scale = fit.clamp(0.1, 3.0) * _zoom;
    final centre = Offset((minX + maxX) / 2, (minY + maxY) / 2);
    final origin = size.center(Offset.zero) - centre * scale + _pan;
    return (origin, scale);
  }

  int? _nodeAt(Offset local, Size size) {
    final (origin, scale) = _transform(size);
    for (var i = _particles.length - 1; i >= 0; i--) {
      final particle = _particles[i];
      final at = Offset(particle.x * scale, particle.y * scale) + origin;
      final radius = math.max(
        8.0,
        nodeRadius(widget.nodes[i].inlinks) * math.max(scale, 0.6),
      );
      if ((at - local).distanceSquared <= radius * radius) return i;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (widget.nodes.isEmpty) {
      return Center(
        child: Text(
          'Nothing linked yet — this crawl found no internal links between '
          'its HTML pages.',
          textAlign: TextAlign.center,
          style: theme.textTheme.bodySmall,
        ),
      );
    }

    return LayoutBuilder(
      builder: (context, constraints) {
        final size = Size(constraints.maxWidth, constraints.maxHeight);
        return MouseRegion(
          cursor: _hovered == null
              ? SystemMouseCursors.grab
              : SystemMouseCursors.click,
          onHover: (event) {
            final found = _nodeAt(event.localPosition, size);
            if (found != _hovered || _pointer != event.localPosition) {
              setState(() {
                _hovered = found;
                _pointer = event.localPosition;
              });
            }
          },
          onExit: (_) => setState(() {
            _hovered = null;
            _pointer = null;
          }),
          child: Listener(
            // Scroll to zoom about the pointer, the way every map does.
            onPointerSignal: (signal) {
              if (signal is! PointerScrollEvent) return;
              setState(() {
                final before = _zoom;
                _zoom = (_zoom * (1 - signal.scrollDelta.dy / 420)).clamp(
                  0.35,
                  6.0,
                );
                final focus = signal.localPosition - size.center(Offset.zero);
                _pan = (_pan - focus) * (_zoom / before) + focus;
              });
            },
            child: GestureDetector(
              onPanUpdate: (details) => setState(() => _pan += details.delta),
              onTapUp: (details) {
                final found = _nodeAt(details.localPosition, size);
                if (found != null) {
                  widget.onSelected?.call(widget.nodes[found]);
                }
              },
              onDoubleTap: () => setState(() {
                _pan = Offset.zero;
                _zoom = 1;
              }),
              child: Stack(
                children: [
                  Positioned.fill(
                    child: CustomPaint(
                      painter: _GraphPainter(
                        nodes: widget.nodes,
                        particles: _particles,
                        edges: _edgeIndices,
                        maxDepth: _maxDepth,
                        transform: _transform,
                        hovered: _hovered,
                        selected: widget.selectedId == null
                            ? null
                            : _indexOf[widget.selectedId!],
                        baseColour: theme.colorScheme.onSurfaceVariant,
                        accent: CrawlStatTheme.accent,
                        edgeColour: theme.colorScheme.onSurfaceVariant
                            .withValues(alpha: 0.13),
                      ),
                    ),
                  ),
                  if (_hovered != null && _pointer != null)
                    _Tooltip(
                      node: widget.nodes[_hovered!],
                      at: _pointer!,
                      bounds: size,
                    ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

class _GraphPainter extends CustomPainter {
  _GraphPainter({
    required this.nodes,
    required this.particles,
    required this.edges,
    required this.maxDepth,
    required this.transform,
    required this.hovered,
    required this.selected,
    required this.baseColour,
    required this.accent,
    required this.edgeColour,
  });

  final List<GraphNodeData> nodes;
  final List<GraphParticle> particles;
  final List<(int, int)> edges;
  final int maxDepth;
  final (Offset, double) Function(Size) transform;
  final int? hovered;
  final int? selected;
  final Color baseColour;
  final Color accent;
  final Color edgeColour;

  @override
  void paint(Canvas canvas, Size size) {
    final (origin, scale) = transform(size);
    Offset at(int index) =>
        Offset(particles[index].x * scale, particles[index].y * scale) + origin;

    // The depth rings, drawn first so everything else sits on top of them.
    // The whole layout is organised around these; leaving them invisible
    // turns an SEO diagram back into a generic blob of dots.
    final ringPaint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = baseColour.withValues(alpha: 0.20);
    for (var depth = 1; depth <= maxDepth; depth++) {
      final radius = depthRing(depth, maxDepth) * 240 * scale;
      if (radius < 12 || radius > size.longestSide * 1.6) continue;
      canvas.drawCircle(origin, radius, ringPaint);
      if (scale > 0.45) {
        final label = TextPainter(
          text: TextSpan(
            text: '$depth',
            style: TextStyle(
              color: baseColour.withValues(alpha: 0.5),
              fontSize: 9,
              fontWeight: FontWeight.w600,
            ),
          ),
          textDirection: TextDirection.ltr,
        )..layout();
        // On the ring itself, up and to the left, where nodes are sparsest.
        label.paint(
          canvas,
          origin +
              Offset(-radius * 0.7071, -radius * 0.7071) -
              Offset(label.width / 2, label.height / 2),
        );
      }
    }

    final focus = hovered ?? selected;
    // When a node is in focus, its own links are lit and the rest fade back.
    // Without this a few hundred edges are a grey fog and the diagram answers
    // no question at all.
    final connected = <int>{};
    if (focus != null) {
      for (final (source, target) in edges) {
        if (source == focus) connected.add(target);
        if (target == focus) connected.add(source);
      }
      connected.add(focus);
    }

    final dim = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = focus == null
          ? edgeColour
          : edgeColour.withValues(alpha: edgeColour.a * 0.35);
    final lit = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.6
      ..color = accent.withValues(alpha: 0.75);

    for (final (source, target) in edges) {
      final touches = focus != null && (source == focus || target == focus);
      canvas.drawLine(at(source), at(target), touches ? lit : dim);
    }

    for (var i = 0; i < nodes.length; i++) {
      final node = nodes[i];
      final centre = at(i);
      final radius = nodeRadius(node.inlinks) * math.max(scale, 0.6);
      final faded = focus != null && !connected.contains(i);
      final colour = nodeColour(node.status, baseColour);
      final isFocus = i == focus;

      if (isFocus) {
        canvas.drawCircle(
          centre,
          radius + 7,
          Paint()
            ..color = accent.withValues(alpha: 0.35)
            ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 7),
        );
      }

      // Each node is a little soft disc: a lit top edge and a body, the same
      // construction as every other surface in the app.
      canvas.drawCircle(
        centre.translate(0, 1),
        radius,
        Paint()..color = Colors.black.withValues(alpha: faded ? 0.25 : 0.55),
      );
      canvas.drawCircle(
        centre,
        radius,
        Paint()
          ..shader = LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: Soft.rampFrom(colour),
          ).createShader(Rect.fromCircle(center: centre, radius: radius))
          ..color = colour.withValues(alpha: faded ? 0.35 : 1),
      );
      if (faded) {
        canvas.drawCircle(
          centre,
          radius,
          Paint()..color = const Color(0xFF131315).withValues(alpha: 0.6),
        );
      }
      if (isFocus) {
        canvas.drawCircle(
          centre,
          radius + 2.5,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1.6
            ..color = accent,
        );
      }

      // Labels only on the pages big enough to matter, and only when zoomed
      // in far enough to read them. A diagram captioned everywhere is a wall
      // of text with some circles behind it.
      final labelWorthIt = radius >= 9 || isFocus;
      if (labelWorthIt && scale > 0.5 && !faded) {
        final painter = TextPainter(
          text: TextSpan(
            text: node.label,
            style: TextStyle(
              color: isFocus ? accent : baseColour,
              fontSize: 10,
              fontWeight: isFocus ? FontWeight.w700 : FontWeight.w500,
            ),
          ),
          textDirection: TextDirection.ltr,
          maxLines: 1,
          ellipsis: '…',
        )..layout(maxWidth: 104);
        painter.paint(canvas, centre + Offset(-painter.width / 2, radius + 4));
      }
    }
  }

  @override
  bool shouldRepaint(_GraphPainter old) => true;
}

/// The hover card: which page this is, and why it is the size it is.
class _Tooltip extends StatelessWidget {
  const _Tooltip({required this.node, required this.at, required this.bounds});

  final GraphNodeData node;
  final Offset at;
  final Size bounds;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    const width = 268.0;
    // Flipped to the other side of the pointer near an edge, so the card is
    // never half off the panel.
    final left = (at.dx + 16 + width > bounds.width)
        ? math.max(8.0, at.dx - width - 16)
        : at.dx + 16;
    final top = math.min(
      math.max(8.0, at.dy + 14),
      math.max(8.0, bounds.height - 112),
    );

    return Positioned(
      left: left,
      top: top,
      child: IgnorePointer(
        child: SoftPanel(
          depth: 1.5,
          padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
          child: SizedBox(
            width: width - 28,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  node.url,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: CrawlStatTheme.mono(context, size: 11.5),
                ),
                const SizedBox(height: 8),
                Wrap(
                  spacing: 14,
                  runSpacing: 4,
                  children: [
                    _Fact(label: 'Depth', value: '${node.depth}'),
                    _Fact(label: 'Inlinks', value: '${node.inlinks}'),
                    _Fact(
                      label: 'Status',
                      value: node.status?.toString() ?? '—',
                      color: nodeColour(
                        node.status,
                        theme.colorScheme.onSurface,
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Fact extends StatelessWidget {
  const _Fact({required this.label, required this.value, this.color});

  final String label;
  final String value;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text('$label ', style: theme.textTheme.bodySmall),
        Text(
          value,
          style: CrawlStatTheme.mono(
            context,
            size: 12,
            color: color ?? theme.colorScheme.onSurface,
          ).copyWith(fontWeight: FontWeight.w600),
        ),
      ],
    );
  }
}
