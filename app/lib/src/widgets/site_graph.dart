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
/// graph: distance from the centre is clicks from the home page. The angle is
/// the site's sections: each top-level folder gets a wedge in proportion to
/// its size, so `/blog` and `/shop` sit in their own labelled slices instead
/// of being stirred together into one ball.
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
    this.linkScore = 0,
  });

  final int id;
  final String url;
  final String label;
  final int depth;
  final int inlinks;
  final int? status;

  /// Internal link score, 0–100. Zero when the crawl's audits have not run,
  /// in which case nodes are sized by inlinks instead.
  final double linkScore;
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

/// How big a node is drawn, from its weight relative to the heaviest node in
/// the diagram (0–1).
///
/// Relative, not absolute: on a site where the menu links every page from
/// every other, absolute inlink counts make every node large and the diagram
/// a solid blob. Measured against the strongest page, the home page and the
/// real hubs stand out and the long tail recedes to dots.
double relativeRadius(double weight) {
  return 2.6 + math.sqrt(weight.clamp(0.0, 1.0)) * 11;
}

/// The URL's path segments, without query or fragment.
List<String> pathSegments(String url) {
  final uri = Uri.tryParse(url);
  if (uri == null) return const [];
  return [
    for (final segment in uri.pathSegments)
      if (segment.isNotEmpty) segment,
  ];
}

/// The top-level section a page belongs to: `/blog` for `/blog/post`, and
/// `/` for the home page and single files at the root such as `/about.html`.
String sectionOf(String url) {
  final segments = pathSegments(url);
  if (segments.isEmpty) return '/';
  if (segments.length == 1 && segments.first.contains('.')) return '/';
  return '/${segments.first}';
}

/// Whether a page lives in the folder `path` (`/blog/archive`), at any depth.
bool inSection(String url, String path) {
  if (path == '/' || path.isEmpty) return true;
  final own = '/${pathSegments(url).join('/')}';
  return own == path || own.startsWith('$path/');
}

/// One section's slice of the diagram.
class GraphWedge {
  const GraphWedge({
    required this.label,
    required this.start,
    required this.sweep,
    required this.count,
  });

  /// `/blog`, or `null` for the small sections folded together.
  final String? label;

  /// Radians, clockwise from three o'clock.
  final double start;
  final double sweep;
  final int count;

  double get middle => start + sweep / 2;
}

/// Divide the circle between the site's top-level sections, in proportion to
/// how many nodes each has, and give every node an angle inside its own
/// section's slice.
///
/// Sections too small to read as a slice are folded together at the end.
/// Within a slice, nodes are ordered by URL, so subfolders sit side by side.
(List<GraphWedge>, List<double>) sectionWedges(List<GraphNodeData> nodes) {
  final angles = List<double>.filled(nodes.length, 0);
  if (nodes.isEmpty) return (const [], angles);

  final groups = <String, List<int>>{};
  for (final (index, node) in nodes.indexed) {
    groups.putIfAbsent(sectionOf(node.url), () => []).add(index);
  }
  final smallest = math.max(3, (nodes.length * 0.02).ceil());
  final named = <(String?, List<int>)>[];
  final other = <int>[];
  for (final MapEntry(key: label, value: members) in groups.entries) {
    if (members.length >= smallest) {
      named.add((label, members));
    } else {
      other.addAll(members);
    }
  }
  named.sort((a, b) {
    final bySize = b.$2.length.compareTo(a.$2.length);
    return bySize != 0 ? bySize : a.$1!.compareTo(b.$1!);
  });
  if (other.isNotEmpty) named.add((null, other));

  final wedges = <GraphWedge>[];
  var start = -math.pi / 2;
  for (final (label, members) in named) {
    final sweep = members.length / nodes.length * math.pi * 2;
    members.sort((a, b) => nodes[a].url.compareTo(nodes[b].url));
    for (final (k, index) in members.indexed) {
      angles[index] = start + (k + 0.5) / members.length * sweep;
    }
    wedges.add(
      GraphWedge(
        label: label,
        start: start,
        sweep: sweep,
        count: members.length,
      ),
    );
    start += sweep;
  }
  return (wedges, angles);
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
Color nodeColour(int? status, Color fallback, CrawlStatPalette palette) {
  if (status == null) return palette.notice;
  if (status >= 500) {
    // A deeper crimson only where severity is a hue. On the grey ramps a
    // darker 5xx would read as *less* urgent than a 4xx, so it shares the
    // error tone instead.
    return palette.name == 'ember' ? const Color(0xFFB91C4B) : palette.error;
  }
  if (status >= 400) return palette.error;
  if (status >= 300) return palette.warning;
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
  List<Offset>? anchors,
  double anchorStrength = 0.014,
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

  // The section wedge: a gentle pull towards the node's place in its
  // section's slice. Weak enough that links still decide the fine layout.
  if (anchors != null && anchors.length == count) {
    for (var i = 0; i < count; i++) {
      final particle = particles[i];
      particle.vx += (anchors[i].dx - particle.x) * anchorStrength;
      particle.vy += (anchors[i].dy - particle.y) * anchorStrength;
    }
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
    this.highlight,
  });

  final List<GraphNodeData> nodes;
  final List<GraphEdgeData> edges;
  final ValueChanged<GraphNodeData>? onSelected;
  final int? selectedId;

  /// A folder path (`/blog`) whose pages are drawn in the accent colour while
  /// the rest of the site recedes.
  final String? highlight;

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
  late List<double> _radii;
  late List<Offset> _anchors;
  late List<GraphWedge> _wedges;

  /// Node indices biggest first, for deciding which ones get a label.
  late List<int> _bySize;

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
    _highlightSet = null;
    _indexOf = {
      for (final (index, node) in widget.nodes.indexed) node.id: index,
    };
    _maxDepth = widget.nodes.fold<int>(0, (a, n) => math.max(a, n.depth));

    final scored = widget.nodes.any((node) => node.linkScore > 0);
    final heaviest = widget.nodes.fold<double>(
      0,
      (a, n) => math.max(a, scored ? n.linkScore : n.inlinks.toDouble()),
    );
    _radii = [
      for (final node in widget.nodes)
        relativeRadius(
          heaviest <= 0
              ? 0
              : (scored ? node.linkScore : node.inlinks.toDouble()) / heaviest,
        ),
    ];
    _bySize = List.generate(widget.nodes.length, (i) => i)
      ..sort((a, b) => _radii[b].compareTo(_radii[a]));

    _rings = [
      for (final node in widget.nodes) depthRing(node.depth, _maxDepth) * 240,
    ];
    final (wedges, angles) = sectionWedges(widget.nodes);
    _wedges = wedges;
    _anchors = [
      for (var i = 0; i < widget.nodes.length; i++)
        Offset(
          math.cos(angles[i]) * _rings[i],
          math.sin(angles[i]) * _rings[i],
        ),
    ];

    // Seeded deterministically: the same crawl lays out the same way every
    // time it is opened, so the picture a user learned yesterday is the
    // picture they see today. Each node starts in its section's wedge.
    final random = math.Random(1709);
    _particles = [
      for (var i = 0; i < widget.nodes.length; i++)
        GraphParticle(
          x: _anchors[i].dx + random.nextDouble() * 6 - 3,
          y: _anchors[i].dy + random.nextDouble() * 6 - 3,
        ),
    ];
    _edgeIndices = [
      for (final edge in widget.edges)
        if (_indexOf[edge.source] != null && _indexOf[edge.target] != null)
          (_indexOf[edge.source]!, _indexOf[edge.target]!),
    ];
  }

  String? _highlightFor;
  Set<int>? _highlightSet;

  /// The highlighted nodes, worked out once per highlight rather than once
  /// per frame.
  Set<int>? _highlighted() {
    final path = widget.highlight;
    if (path == null) return null;
    if (path != _highlightFor || _highlightSet == null) {
      _highlightFor = path;
      _highlightSet = {
        for (final (index, node) in widget.nodes.indexed)
          if (inSection(node.url, path)) index,
      };
    }
    return _highlightSet;
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
        relaxGraph(
          _particles,
          _edgeIndices,
          _rings,
          anchors: _anchors,
          temperature: _temperature,
        );
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
      final radius = math.max(8.0, _radii[i] * math.max(scale, 0.6));
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
                        radii: _radii,
                        bySize: _bySize,
                        wedges: _wedges,
                        highlight: widget.highlight,
                        highlighted: _highlighted(),
                        inkColour: theme.colorScheme.onSurface,
                        particles: _particles,
                        edges: _edgeIndices,
                        maxDepth: _maxDepth,
                        transform: _transform,
                        hovered: _hovered,
                        selected: widget.selectedId == null
                            ? null
                            : _indexOf[widget.selectedId!],
                        baseColour: theme.colorScheme.onSurfaceVariant,
                        palette: CrawlStatTheme.palette(context),
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
    required this.radii,
    required this.bySize,
    required this.wedges,
    required this.highlight,
    required this.highlighted,
    required this.inkColour,
    required this.particles,
    required this.edges,
    required this.maxDepth,
    required this.transform,
    required this.hovered,
    required this.selected,
    required this.baseColour,
    required this.palette,
    required this.edgeColour,
  });

  final List<GraphNodeData> nodes;
  final List<double> radii;
  final List<int> bySize;
  final List<GraphWedge> wedges;
  final String? highlight;

  /// Indices of the nodes in [highlight], or `null` when nothing is.
  final Set<int>? highlighted;
  final Color inkColour;
  final List<GraphParticle> particles;
  final List<(int, int)> edges;
  final int maxDepth;
  final (Offset, double) Function(Size) transform;
  final int? hovered;
  final int? selected;
  final Color baseColour;
  final CrawlStatPalette palette;
  Color get accent => palette.accent;
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

    final outer = 240 * scale;
    _paintWedges(canvas, size, origin, outer);

    final focus = hovered ?? selected;
    // When a node is in focus, its own links are lit and the rest fade back.
    // Without this a few hundred edges are a grey fog and the diagram answers
    // no question at all. A highlighted section does the same for a whole
    // folder at once.
    final connected = <int>{};
    if (focus != null) {
      for (final (source, target) in edges) {
        if (source == focus) connected.add(target);
        if (target == focus) connected.add(source);
      }
      connected.add(focus);
    }
    bool faded(int i) => focus != null
        ? !connected.contains(i)
        : highlighted != null && !highlighted!.contains(i);

    final quiet = focus != null || highlighted != null;
    final dim = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = quiet
          ? edgeColour.withValues(alpha: edgeColour.a * 0.35)
          : edgeColour;
    final lit = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.6
      ..color = accent.withValues(alpha: 0.75);
    final within = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = accent.withValues(alpha: 0.22);

    for (final (source, target) in edges) {
      final touches = focus != null && (source == focus || target == focus);
      final inside =
          focus == null &&
          highlighted != null &&
          highlighted!.contains(source) &&
          highlighted!.contains(target);
      canvas.drawLine(
        at(source),
        at(target),
        touches ? lit : (inside ? within : dim),
      );
    }

    for (var i = 0; i < nodes.length; i++) {
      final node = nodes[i];
      final centre = at(i);
      final radius = radii[i] * math.max(scale, 0.6);
      final isFaded = faded(i);
      final working = (node.status ?? 0) >= 200 && (node.status ?? 0) < 300;
      // Broken pages keep their status colour everywhere; working pages in
      // the highlighted section take the accent, the rest stay neutral.
      final colour = highlighted != null && highlighted!.contains(i) && working
          ? accent
          : nodeColour(node.status, baseColour, palette);
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
        Paint()..color = Colors.black.withValues(alpha: isFaded ? 0.25 : 0.55),
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
          ..color = colour.withValues(alpha: isFaded ? 0.35 : 1),
      );
      if (isFaded) {
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
    }

    // Labels last, on top of every node, and only where they fit: the focus
    // first, then the biggest pages, skipping any that would overlap one
    // already placed. A diagram captioned everywhere is a wall of text with
    // some circles behind it.
    if (scale <= 0.4) return;
    final candidates = <int>[
      ?focus,
      if (highlighted != null)
        ...bySize.where((i) => highlighted!.contains(i)).take(14),
      ...bySize.take(highlighted == null ? 22 : 4),
    ];
    final placed = <Rect>[];
    final seen = <int>{};
    for (final i in candidates) {
      if (!seen.add(i)) continue;
      if (i != focus && faded(i)) continue;
      final centre = at(i);
      final radius = radii[i] * math.max(scale, 0.6);
      final isFocus = i == focus;
      final painter = TextPainter(
        text: TextSpan(
          text: nodes[i].label,
          style: TextStyle(
            color: isFocus ? inkColour : baseColour,
            fontSize: 10.5,
            fontWeight: isFocus ? FontWeight.w700 : FontWeight.w500,
            shadows: const [Shadow(color: Color(0xCC000000), blurRadius: 4)],
          ),
        ),
        textDirection: TextDirection.ltr,
        maxLines: 1,
        ellipsis: '…',
      )..layout(maxWidth: 120);
      final topLeft = centre + Offset(-painter.width / 2, radius + 3);
      final box = (topLeft & painter.size).inflate(2);
      if (!isFocus && placed.any(box.overlaps)) continue;
      placed.add(box);
      painter.paint(canvas, topLeft);
    }
  }

  /// Faint spokes between the sections and each section's name outside the
  /// outermost ring.
  void _paintWedges(Canvas canvas, Size size, Offset origin, double outer) {
    if (wedges.length < 2 || outer < 40) return;
    final spoke = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = baseColour.withValues(alpha: 0.10);
    final inner = outer * 0.18;
    for (final wedge in wedges) {
      final direction = Offset(math.cos(wedge.start), math.sin(wedge.start));
      canvas.drawLine(
        origin + direction * inner,
        origin + direction * (outer + 10),
        spoke,
      );
    }

    for (final wedge in wedges) {
      // Too thin a slice for its name to sit beside it without colliding.
      if (wedge.sweep * outer < 36) continue;
      final lit =
          highlight != null &&
          wedge.label != null &&
          (highlight == wedge.label ||
              highlight!.startsWith('${wedge.label}/'));
      final painter = TextPainter(
        text: TextSpan(
          children: [
            TextSpan(
              text: wedge.label ?? 'other',
              style: TextStyle(
                color: lit ? inkColour : baseColour,
                fontSize: 11.5,
                fontWeight: FontWeight.w700,
              ),
            ),
            TextSpan(
              text: '  ${wedge.count}',
              style: TextStyle(
                color: baseColour.withValues(alpha: 0.7),
                fontSize: 10.5,
              ),
            ),
          ],
        ),
        textDirection: TextDirection.ltr,
        maxLines: 1,
      )..layout(maxWidth: 160);
      final direction = Offset(math.cos(wedge.middle), math.sin(wedge.middle));
      final anchor = origin + direction * (outer + 18);
      // Text grows away from the circle: leftwards on the left-hand side,
      // rightwards on the right, centred at the top and bottom.
      final dx = direction.dx < -0.3
          ? -painter.width
          : direction.dx > 0.3
          ? 0.0
          : -painter.width / 2;
      final dy = direction.dy < -0.3
          ? -painter.height
          : direction.dy > 0.3
          ? 0.0
          : -painter.height / 2;
      if (lit) {
        final box = (anchor + Offset(dx, dy)) & painter.size;
        canvas.drawRRect(
          RRect.fromRectAndRadius(box.inflate(5), const Radius.circular(8)),
          Paint()..color = accent.withValues(alpha: 0.22),
        );
      }
      painter.paint(canvas, anchor + Offset(dx, dy));
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
                    if (node.linkScore > 0)
                      _Fact(
                        label: 'Link score',
                        value: node.linkScore.toStringAsFixed(0),
                      ),
                    _Fact(
                      label: 'Status',
                      value: node.status?.toString() ?? '—',
                      color: nodeColour(
                        node.status,
                        theme.colorScheme.onSurface,
                        CrawlStatTheme.palette(context),
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
