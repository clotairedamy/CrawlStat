import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme/theme.dart';

/// A screen built as full-width sections rather than one long scroll.
///
/// Each section takes the whole stage, snaps into place, and animates itself
/// in when it arrives. There is no scrollbar: progress is shown by the marks
/// down the right-hand edge, which are also how you jump about.
///
/// ## Why not just a scroll view
///
/// A list of cards on a tall screen is a wall. The reader has to decide what
/// to look at, and the answer is usually "the top of it". One section at a
/// time makes that decision for them, which is the whole point of presenting
/// findings rather than dumping them.
///
/// ## What this must never do
///
/// **Hide content.** A section whose content is taller than the stage scrolls
/// inside itself, and the stage only moves on once that inner scroll has hit
/// its end. Snapping that strands half a paragraph off-screen would be worse
/// than the scrollbar it replaced — which is why the data table keeps its
/// scrollbar and is not built out of these.
class SectionStage extends StatefulWidget {
  const SectionStage({
    super.key,
    required this.sections,
    this.labels = const [],
  });

  /// One widget per section, in order.
  final List<Widget> sections;

  /// Optional names, shown beside the progress marks on hover.
  final List<String> labels;

  @override
  State<SectionStage> createState() => _SectionStageState();
}

class _SectionStageState extends State<SectionStage> {
  late final PageController _controller = PageController();
  final FocusNode _focus = FocusNode();
  int _current = 0;

  /// Wheel notches accumulate: a trackpad sends a stream of tiny deltas, and
  /// advancing on each one would fly through the whole screen on one flick.
  double _wheel = 0;
  static const double _wheelThreshold = 160;

  @override
  void dispose() {
    _controller.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _goTo(int index) {
    final target = index.clamp(0, widget.sections.length - 1);
    if (target == _current) return;
    _wheel = 0;
    _controller.animateToPage(
      target,
      duration: const Duration(milliseconds: 520),
      curve: Curves.easeOutCubic,
    );
  }

  void _onWheel(PointerScrollEvent event) {
    _wheel += event.scrollDelta.dy;
    if (_wheel.abs() < _wheelThreshold) return;
    _goTo(_current + (_wheel > 0 ? 1 : -1));
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    switch (event.logicalKey) {
      case LogicalKeyboardKey.arrowDown:
      case LogicalKeyboardKey.pageDown:
      case LogicalKeyboardKey.space:
        _goTo(_current + 1);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowUp:
      case LogicalKeyboardKey.pageUp:
        _goTo(_current - 1);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.home:
        _goTo(0);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.end:
        _goTo(widget.sections.length - 1);
        return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    if (widget.sections.isEmpty) return const SizedBox.shrink();

    return Focus(
      focusNode: _focus,
      autofocus: true,
      onKeyEvent: _onKey,
      child: Listener(
        onPointerSignal: (signal) {
          if (signal is PointerScrollEvent) _onWheel(signal);
        },
        child: Stack(
          children: [
            Positioned.fill(
              child: PageView.builder(
                controller: _controller,
                scrollDirection: Axis.vertical,
                itemCount: widget.sections.length,
                onPageChanged: (index) => setState(() {
                  _current = index;
                  _wheel = 0;
                }),
                itemBuilder: (context, index) => _Section(
                  active: index == _current,
                  child: widget.sections[index],
                ),
              ),
            ),
            Positioned(
              right: 10,
              top: 0,
              bottom: 0,
              child: Center(
                child: _Progress(
                  count: widget.sections.length,
                  current: _current,
                  labels: widget.labels,
                  onTap: _goTo,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// One section: scrolls inside itself when it has to, and fades up when it
/// becomes the one you are looking at.
class _Section extends StatefulWidget {
  const _Section({required this.active, required this.child});

  final bool active;
  final Widget child;

  @override
  State<_Section> createState() => _SectionState();
}

class _SectionState extends State<_Section>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 620),
    // The first section is already on screen when the stage is built, so it
    // starts shown rather than animating in from nothing.
    value: widget.active ? 1 : 0,
  );

  @override
  void didUpdateWidget(_Section old) {
    super.didUpdateWidget(old);
    if (widget.active && !old.active) {
      _controller.forward(from: 0);
    } else if (!widget.active && old.active) {
      _controller.value = 0;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final curve = CurvedAnimation(
      parent: _controller,
      curve: Curves.easeOutCubic,
    );
    return LayoutBuilder(
      builder: (context, constraints) => SingleChildScrollView(
        // The escape hatch: a section taller than the stage stays reachable.
        physics: const ClampingScrollPhysics(),
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: constraints.maxHeight),
          child: Center(
            child: AnimatedBuilder(
              animation: curve,
              builder: (context, child) => Opacity(
                opacity: curve.value,
                child: Transform.translate(
                  offset: Offset(0, 26 * (1 - curve.value)),
                  child: child,
                ),
              ),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(0, 8, 26, 8),
                child: widget.child,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// The marks down the right edge, in place of a scrollbar.
class _Progress extends StatelessWidget {
  const _Progress({
    required this.count,
    required this.current,
    required this.labels,
    required this.onTap,
  });

  final int count;
  final int current;
  final List<String> labels;
  final ValueChanged<int> onTap;

  @override
  Widget build(BuildContext context) {
    final palette = CrawlStatTheme.palette(context);
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var index = 0; index < count; index++)
          Tooltip(
            message: index < labels.length
                ? labels[index]
                : 'Section ${index + 1}',
            preferBelow: false,
            child: MouseRegion(
              cursor: SystemMouseCursors.click,
              child: GestureDetector(
                onTap: () => onTap(index),
                child: AnimatedContainer(
                  duration: const Duration(milliseconds: 280),
                  curve: Curves.easeOut,
                  margin: const EdgeInsets.symmetric(vertical: 5),
                  width: 4,
                  // The current section's mark is a bar, the rest are dots:
                  // position is readable at a glance without counting.
                  height: index == current ? 26 : 4,
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(3),
                    color: index == current
                        ? palette.accent
                        : muted.withValues(alpha: 0.35),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

/// How many wheel notches it takes to move on.
///
/// Exposed so the behaviour can be checked without a gesture: a trackpad
/// sends a stream of small deltas and a mouse sends few large ones, and the
/// stage has to feel the same under both.
bool shouldAdvance(double accumulated, {double threshold = 160}) =>
    accumulated.abs() >= threshold;

/// Where a wheel gesture of this size should land, from where it started.
int advanceTarget(int current, double accumulated, int count) {
  if (!shouldAdvance(accumulated)) return current;
  final next = current + (accumulated > 0 ? 1 : -1);
  return next.clamp(0, math.max(0, count - 1));
}
