import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

/// How every scrollable in the app scrolls: no scrollbars anywhere, and the
/// content fading out at whichever edge has more of it beyond.
///
/// A scrollbar is a gauge of how much is left. On a page that streams its
/// content in as you reach it — the results table, the project cards — that
/// gauge is either wrong or jumpy, and either way it is chrome on a soft
/// interface that otherwise has none. The fade says the one thing worth
/// saying, "there is more this way", and says it in the content itself.
class CrawlStatScrollBehavior extends MaterialScrollBehavior {
  const CrawlStatScrollBehavior();

  @override
  Widget buildScrollbar(
    BuildContext context,
    Widget child,
    ScrollableDetails details,
  ) {
    // A page view's neighbours are whole pages, not more of the same list:
    // fading its edges would just look like a rendering fault.
    if (details.controller is PageController) return child;
    return EdgeFade(axis: axisDirectionToAxis(details.direction), child: child);
  }
}

/// Fades a scrollable's content out towards each edge that has more beyond.
///
/// The fade grows with the distance left to scroll, up to [extent], so it
/// melts away as the reader reaches the end rather than switching off.
class EdgeFade extends StatefulWidget {
  const EdgeFade({
    super.key,
    required this.axis,
    required this.child,
    this.extent = 36,
  });

  final Axis axis;
  final Widget child;
  final double extent;

  @override
  State<EdgeFade> createState() => _EdgeFadeState();
}

class _EdgeFadeState extends State<EdgeFade> {
  double _leading = 0;
  double _trailing = 0;

  bool _onMetrics(ScrollMetrics metrics, int depth) {
    // Only this scrollable's own metrics: a nested list's notifications
    // bubble through here too.
    if (depth != 0 || metrics.axis != widget.axis) return false;
    final leading = math.min(metrics.extentBefore, widget.extent);
    final trailing = math.min(metrics.extentAfter, widget.extent);
    if ((leading - _leading).abs() > 0.5 ||
        (trailing - _trailing).abs() > 0.5 ||
        (leading == 0) != (_leading == 0) ||
        (trailing == 0) != (_trailing == 0)) {
      setState(() {
        _leading = leading;
        _trailing = trailing;
      });
    }
    return false;
  }

  @override
  Widget build(BuildContext context) {
    return NotificationListener<ScrollMetricsNotification>(
      onNotification: (n) => _onMetrics(n.metrics, n.depth),
      child: NotificationListener<ScrollNotification>(
        onNotification: (n) => _onMetrics(n.metrics, n.depth),
        child: _EdgeFadeMask(
          axis: widget.axis,
          leading: _leading,
          trailing: _trailing,
          child: widget.child,
        ),
      ),
    );
  }
}

/// The mask itself, as a render object rather than a `ShaderMask`.
///
/// A `ShaderMask` composites into its own layer on every frame, fade or not.
/// Swapping it in and out would rebuild the scrollable beneath and lose its
/// position. This keeps one widget in the tree and only pushes the layer
/// while there is something to fade.
class _EdgeFadeMask extends SingleChildRenderObjectWidget {
  const _EdgeFadeMask({
    required this.axis,
    required this.leading,
    required this.trailing,
    super.child,
  });

  final Axis axis;
  final double leading;
  final double trailing;

  @override
  _RenderEdgeFadeMask createRenderObject(BuildContext context) =>
      _RenderEdgeFadeMask(axis, leading, trailing);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderEdgeFadeMask renderObject,
  ) {
    renderObject
      ..axis = axis
      ..leading = leading
      ..trailing = trailing;
  }
}

class _RenderEdgeFadeMask extends RenderProxyBox {
  _RenderEdgeFadeMask(this._axis, this._leading, this._trailing);

  Axis _axis;
  set axis(Axis value) {
    if (value == _axis) return;
    _axis = value;
    markNeedsPaint();
  }

  double _leading;
  set leading(double value) {
    if (value == _leading) return;
    final was = _active;
    _leading = value;
    _changed(was);
  }

  double _trailing;
  set trailing(double value) {
    if (value == _trailing) return;
    final was = _active;
    _trailing = value;
    _changed(was);
  }

  bool get _active => _leading > 0 || _trailing > 0;

  void _changed(bool wasActive) {
    if (wasActive != _active) markNeedsCompositingBitsUpdate();
    markNeedsPaint();
  }

  @override
  bool get alwaysNeedsCompositing => child != null && _active;

  @override
  ShaderMaskLayer? get layer => super.layer as ShaderMaskLayer?;

  @override
  void paint(PaintingContext context, Offset offset) {
    final child = this.child;
    if (child == null) return;
    final length = _axis == Axis.vertical ? size.height : size.width;
    if (!_active || length <= 0) {
      layer = null;
      context.paintChild(child, offset);
      return;
    }
    final start = (_leading / length).clamp(0.0, 0.5);
    final end = 1 - (_trailing / length).clamp(0.0, 0.5);
    final gradient = LinearGradient(
      begin: _axis == Axis.vertical
          ? Alignment.topCenter
          : Alignment.centerLeft,
      end: _axis == Axis.vertical
          ? Alignment.bottomCenter
          : Alignment.centerRight,
      colors: [
        if (_leading > 0) const Color(0x00000000),
        const Color(0xFF000000),
        const Color(0xFF000000),
        if (_trailing > 0) const Color(0x00000000),
      ],
      stops: [if (_leading > 0) 0, start, end, if (_trailing > 0) 1],
    );
    layer ??= ShaderMaskLayer();
    layer!
      ..shader = gradient.createShader(Offset.zero & size)
      ..maskRect = offset & size
      ..blendMode = BlendMode.dstIn;
    context.pushLayer(layer!, super.paint, offset);
  }
}
