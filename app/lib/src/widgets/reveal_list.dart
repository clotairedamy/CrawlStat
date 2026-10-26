import 'package:flutter/material.dart';

/// A list that shows everything, animates each item in as it arrives, and
/// never draws a scrollbar.
///
/// ## Why not one section per screen
///
/// The first version of this snapped one card into the middle of the viewport
/// at a time. It looked deliberate on a short card and absurd on a tall
/// screen — a small panel marooned in a field of empty charcoal, showing less
/// than the plain scrolling list it replaced.
///
/// Density is not the enemy. The reader came to compare findings, and taking
/// six of them off the screen to present one does not help. So this packs
/// content exactly as an ordinary list does; the animation is the part that
/// was worth keeping.
///
/// ## How the reveal works
///
/// `ListView.builder` only builds an item shortly before it is needed, so an
/// item that animates on first build animates as it comes into view — with no
/// scroll listener, no position arithmetic and no visibility detector. The
/// first screenful is staggered so the opening paint settles in sequence
/// rather than all at once.
class RevealList extends StatelessWidget {
  const RevealList({
    super.key,
    required this.children,
    this.padding = EdgeInsets.zero,
    this.spacing = 14,
  });

  final List<Widget> children;
  final EdgeInsets padding;
  final double spacing;

  @override
  Widget build(BuildContext context) {
    if (children.isEmpty) return const SizedBox.shrink();

    // No scrollbar here or anywhere else: the app's scroll behaviour removes
    // them all and fades the edges instead.
    return ListView.builder(
      padding: padding,
      itemCount: children.length,
      itemBuilder: (context, index) => Padding(
        padding: EdgeInsets.only(
          bottom: index == children.length - 1 ? 0 : spacing,
        ),
        child: _Reveal(
          // Only the opening screenful is staggered. Past that, an item is
          // being built because the reader scrolled to it, and a delay
          // would read as lag.
          delay: index < 6 ? Duration(milliseconds: 55 * index) : Duration.zero,
          child: children[index],
        ),
      ),
    );
  }
}

/// Fades and lifts its child into place, once.
class _Reveal extends StatefulWidget {
  const _Reveal({required this.child, required this.delay});

  final Widget child;
  final Duration delay;

  @override
  State<_Reveal> createState() => _RevealState();
}

class _RevealState extends State<_Reveal> with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 420),
  );
  late final Animation<double> _curve = CurvedAnimation(
    parent: _controller,
    curve: Curves.easeOutCubic,
  );

  @override
  void initState() {
    super.initState();
    if (widget.delay == Duration.zero) {
      _controller.forward();
    } else {
      Future.delayed(widget.delay, () {
        // An item can be scrolled away and disposed before its delay
        // elapses, and driving a disposed controller throws.
        if (mounted) _controller.forward();
      });
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _curve,
      builder: (context, child) => Opacity(
        opacity: _curve.value,
        child: Transform.translate(
          offset: Offset(0, 18 * (1 - _curve.value)),
          child: child,
        ),
      ),
      child: widget.child,
    );
  }
}
