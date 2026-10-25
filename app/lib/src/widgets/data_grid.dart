import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme/icons.dart';
import '../theme/theme.dart';

/// Width of the bar marking the keyboard cursor.
///
/// Reserved permanently rather than drawn as a border, because a border on the
/// focused row would shrink that row's content box and overflow the
/// fixed-width columns by exactly its own width.
const double _indicatorWidth = 2;

/// Horizontal padding inside every header and body cell.
const double _cellPadding = 12;

/// How wide the grab area on a header's right edge is. Far wider than the
/// 1px line it carries, because a 1px target is a pixel hunt with a trackpad.
const double _resizeHandleWidth = 8;

/// The narrowest a column can be dragged to: room for the cell padding and a
/// few characters, so a column cannot vanish under its own handle.
const double _minColumnWidth = 48;

/// One column of a [DataGrid].
class GridColumn<T> {
  const GridColumn({
    required this.id,
    required this.label,
    required this.width,
    required this.value,
    this.sortKey,
    this.align = TextAlign.left,
    this.mono = false,
    this.color,
    this.tooltip,
    this.defaultVisible = true,
    this.cell,
    this.grow = false,
  });

  /// Stable identifier, used by the column chooser and saved layouts.
  final String id;
  final String label;
  final double width;

  /// The text to show. Called for every visible cell, so keep it cheap.
  final String Function(T row) value;

  /// Column name the engine sorts by. `null` means not sortable — true for
  /// anything the database has no index for.
  final String? sortKey;

  final TextAlign align;

  /// Numbers, URLs and status codes are monospaced so they align down the
  /// column.
  final bool mono;

  /// Optional per-row colour, for status codes and severities.
  final Color? Function(T row)? color;

  final String? tooltip;
  final bool defaultVisible;

  /// Full control over the cell, for chips and icons.
  final Widget Function(BuildContext context, T row)? cell;

  /// Takes the spare width when the table is narrower than its viewport. When
  /// no column asks for it, the widest one does — usually the URL.
  final bool grow;

  GridColumn<T> _withWidth(double newWidth) => GridColumn<T>(
    id: id,
    label: label,
    width: newWidth,
    value: value,
    sortKey: sortKey,
    align: align,
    mono: mono,
    color: color,
    tooltip: tooltip,
    defaultVisible: defaultVisible,
    cell: cell,
    grow: grow,
  );
}

/// Widen one column so the table fills `available`, rather than leaving a
/// ragged gap to the right of the last column and a header that stops short.
///
/// `widths` are the widths the user has dragged columns to, by column id. They
/// replace the defaults, and a column the user has sized is left exactly where
/// they put it — so if that is the column that would have taken the spare
/// width, nothing does, and the table simply ends short of the viewport.
List<GridColumn<T>> fillWidth<T>(
  List<GridColumn<T>> columns,
  double available, {
  Map<String, double> widths = const {},
}) {
  final sized = [
    for (final column in columns)
      switch (widths[column.id]) {
        final width? => column._withWidth(width),
        null => column,
      },
  ];
  if (columns.isEmpty || !available.isFinite) return sized;
  final natural = sized.fold<double>(
    _indicatorWidth,
    (sum, column) => sum + column.width,
  );
  final spare = available - natural;
  if (spare <= 0) return sized;

  // Chosen from the default widths, not the dragged ones: widening the title
  // past the URL should not move the spare width onto the title.
  var target = columns.indexWhere((column) => column.grow);
  if (target < 0) {
    target = 0;
    for (var i = 1; i < columns.length; i++) {
      if (columns[i].width > columns[target].width) target = i;
    }
  }
  if (widths.containsKey(columns[target].id)) return sized;
  return [
    for (var i = 0; i < sized.length; i++)
      i == target ? sized[i]._withWidth(sized[i].width + spare) : sized[i],
  ];
}

/// A dense, virtualized table backed by server-side paging.
///
/// The grid never holds the whole result: it renders `itemCount` rows from
/// `total` and asks [onNeedRow] for each one as it scrolls into view. A row
/// whose page has not arrived yet renders as a placeholder, so scrolling never
/// blocks on the database.
///
/// Keyboard navigation is first-class — arrow keys move the selection, Enter
/// opens the detail panel, Home/End jump to the ends — because this is the
/// screen a user spends an hour in.
class DataGrid<T> extends StatefulWidget {
  const DataGrid({
    super.key,
    required this.columns,
    required this.total,
    required this.rowAt,
    required this.onNeedRow,
    this.visibleColumnIds,
    this.sortBy,
    this.descending = false,
    this.onSort,
    this.onSelect,
    this.onActivate,
    this.selectedKey,
    this.rowKey,
    this.emptyState,
    this.loading = false,
  });

  final List<GridColumn<T>> columns;

  /// How many rows match the query, from the database's own count.
  final int total;

  /// The row at an index, or `null` if it has not been fetched yet.
  final T? Function(int index) rowAt;

  /// Called when a row scrolls into view and is not loaded.
  final void Function(int index) onNeedRow;

  /// `null` shows every column with `defaultVisible`.
  final Set<String>? visibleColumnIds;

  final String? sortBy;
  final bool descending;
  final void Function(String sortKey)? onSort;

  /// Selection changed, by click or by keyboard.
  final void Function(T row)? onSelect;

  /// Enter, or a double click.
  final void Function(T row)? onActivate;

  /// Identifies the selected row, so selection survives a reload.
  final Object? selectedKey;
  final Object Function(T row)? rowKey;

  final Widget? emptyState;
  final bool loading;

  @override
  State<DataGrid<T>> createState() => _DataGridState<T>();
}

class _DataGridState<T> extends State<DataGrid<T>> {
  final ScrollController _vertical = ScrollController();
  final ScrollController _horizontal = ScrollController();
  final FocusNode _focus = FocusNode(debugLabel: 'DataGrid');

  /// The row the keyboard is on. Kept separately from the selection so arrow
  /// keys can move without loading a detail panel for every step.
  int _cursor = 0;

  /// Double-click detection, done by hand.
  ///
  /// `InkWell`'s `onDoubleTap` would make every *single* click wait out the
  /// double-tap timeout before selecting — around 300ms of lag on the one
  /// gesture a user performs most in this screen. Tracking it ourselves keeps
  /// selection instant and still opens the row on a second click.
  int? _lastTapRow;
  DateTime? _lastTapAt;

  /// Longer than the gap a deliberate double click leaves, shorter than the gap
  /// between two separate clicks.
  static const _doubleClickWindow = Duration(milliseconds: 350);

  void _handleTap(int index, T row) {
    final now = DateTime.now();
    final isDoubleClick =
        _lastTapRow == index &&
        _lastTapAt != null &&
        now.difference(_lastTapAt!) < _doubleClickWindow;

    _lastTapRow = index;
    _lastTapAt = now;

    setState(() => _cursor = index);
    _focus.requestFocus();
    widget.onSelect?.call(row);
    if (isDoubleClick) {
      widget.onActivate?.call(row);
    }
  }

  /// Widths the user has dragged columns to, by column id.
  ///
  /// Keyed by id rather than position so a width follows its column when the
  /// column chooser adds or removes the ones around it, and survives a switch
  /// between tabs that share columns.
  final Map<String, double> _widths = {};

  /// The column whose edge is being dragged, so its handle stays lit even when
  /// the pointer runs ahead of it.
  String? _resizing;

  /// Where the drag began. The new width is measured from these rather than
  /// accumulated from per-frame deltas, so dragging past the minimum and back
  /// leaves the edge under the pointer instead of drifting away from it.
  double _resizeStartWidth = 0;
  double _resizeStartX = 0;

  void _startResize(GridColumn<T> column, double shownWidth, double x) {
    setState(() {
      _resizing = column.id;
      // The width on screen, which for the filling column is more than its
      // default. Starting from anything else would make the edge jump.
      _resizeStartWidth = shownWidth;
      _resizeStartX = x;
      _widths[column.id] = shownWidth;
    });
  }

  void _updateResize(GridColumn<T> column, double x) {
    // Never narrower than the column was designed at, if that is already
    // below the usual minimum.
    final floor = column.width < _minColumnWidth
        ? column.width
        : _minColumnWidth;
    final next = _resizeStartWidth + x - _resizeStartX;
    setState(() => _widths[column.id] = next < floor ? floor : next);
  }

  void _endResize() => setState(() => _resizing = null);

  void _resetWidth(GridColumn<T> column) =>
      setState(() => _widths.remove(column.id));

  @override
  void dispose() {
    _vertical.dispose();
    _horizontal.dispose();
    _focus.dispose();
    super.dispose();
  }

  List<GridColumn<T>> get _visible {
    final allowed = widget.visibleColumnIds;
    if (allowed == null) {
      return widget.columns.where((column) => column.defaultVisible).toList();
    }
    return widget.columns
        .where((column) => allowed.contains(column.id))
        .toList();
  }

  void _moveCursor(int delta) {
    if (widget.total == 0) return;
    final next = (_cursor + delta).clamp(0, widget.total - 1);
    if (next == _cursor) return;
    setState(() => _cursor = next);
    _revealCursor();
    final row = widget.rowAt(next);
    if (row != null) {
      widget.onSelect?.call(row);
    } else {
      widget.onNeedRow(next);
    }
  }

  /// Scroll the cursor into view, keeping one row of context.
  void _revealCursor() {
    if (!_vertical.hasClients) return;
    const height = CrawlStatTheme.rowHeight;
    final top = _cursor * height;
    final viewport = _vertical.position.viewportDimension;
    final offset = _vertical.offset;

    if (top < offset) {
      _vertical.jumpTo(
        (top - height).clamp(0, _vertical.position.maxScrollExtent),
      );
    } else if (top + height > offset + viewport) {
      _vertical.jumpTo(
        (top + height * 2 - viewport).clamp(
          0,
          _vertical.position.maxScrollExtent,
        ),
      );
    }
  }

  void _activate() {
    final row = widget.rowAt(_cursor);
    if (row != null) {
      widget.onActivate?.call(row);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (widget.total == 0 && !widget.loading) {
      return widget.emptyState ?? const SizedBox.shrink();
    }
    return LayoutBuilder(
      builder: (context, constraints) => _grid(context, constraints.maxWidth),
    );
  }

  Widget _grid(BuildContext context, double available) {
    final theme = Theme.of(context);
    final columns = fillWidth(_visible, available, widths: _widths);
    final natural = columns.fold<double>(
      _indicatorWidth,
      (sum, column) => sum + column.width,
    );
    // Once the user has sized the filling column the columns can add up to
    // less than the viewport. The header and row stripes still run to the
    // edge, so the table does not look cut off.
    final width = available.isFinite && natural < available
        ? available
        : natural;

    return Shortcuts(
      shortcuts: const {
        SingleActivator(LogicalKeyboardKey.arrowDown): _MoveIntent(1),
        SingleActivator(LogicalKeyboardKey.arrowUp): _MoveIntent(-1),
        SingleActivator(LogicalKeyboardKey.pageDown): _MoveIntent(20),
        SingleActivator(LogicalKeyboardKey.pageUp): _MoveIntent(-20),
        SingleActivator(LogicalKeyboardKey.home): _MoveIntent(-1 << 30),
        SingleActivator(LogicalKeyboardKey.end): _MoveIntent(1 << 30),
        SingleActivator(LogicalKeyboardKey.enter): _ActivateRowIntent(),
      },
      child: Actions(
        actions: {
          _MoveIntent: CallbackAction<_MoveIntent>(
            onInvoke: (intent) {
              _moveCursor(intent.delta);
              return null;
            },
          ),
          _ActivateRowIntent: CallbackAction<_ActivateRowIntent>(
            onInvoke: (_) {
              _activate();
              return null;
            },
          ),
        },
        child: Focus(
          focusNode: _focus,
          child: Scrollbar(
            controller: _horizontal,
            scrollbarOrientation: ScrollbarOrientation.bottom,
            child: SingleChildScrollView(
              controller: _horizontal,
              scrollDirection: Axis.horizontal,
              // The vertical constraint passes straight through a horizontal
              // scroll view, so the header can stay put while the rows scroll.
              child: SizedBox(
                width: width,
                child: Column(
                  children: [
                    _header(context, columns),
                    Expanded(
                      child: Scrollbar(
                        controller: _vertical,
                        child: ListView.builder(
                          controller: _vertical,
                          itemCount: widget.total,
                          itemExtent: CrawlStatTheme.rowHeight,
                          // Fixed extents mean the list can jump straight to any
                          // offset without measuring the rows in between.
                          itemBuilder: (context, index) =>
                              _row(context, columns, index),
                        ),
                      ),
                    ),
                    if (widget.loading)
                      SizedBox(
                        height: 2,
                        child: LinearProgressIndicator(
                          minHeight: 2,
                          backgroundColor:
                              theme.colorScheme.surfaceContainerLow,
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _header(BuildContext context, List<GridColumn<T>> columns) {
    final theme = Theme.of(context);
    return Container(
      height: CrawlStatTheme.headerHeight,
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainer.withValues(alpha: 0.6),
        border: Border(bottom: BorderSide(color: theme.dividerColor)),
      ),
      child: Row(
        children: [
          // Keeps the header aligned with the rows' cursor bar.
          const SizedBox(width: _indicatorWidth),
          for (final column in columns)
            _HeaderCell(
              column: column,
              sorted: widget.sortBy != null && widget.sortBy == column.sortKey,
              descending: widget.descending,
              onTap: column.sortKey == null || widget.onSort == null
                  ? null
                  : () => widget.onSort!(column.sortKey!),
              resizing: _resizing == column.id,
              // `column` is the laid-out one, so its width is what is on
              // screen; the default comes from the unfilled original.
              onResizeStart: (x) =>
                  _startResize(_original(column), column.width, x),
              onResizeUpdate: (x) => _updateResize(_original(column), x),
              onResizeEnd: _endResize,
              onResetWidth: () => _resetWidth(_original(column)),
            ),
        ],
      ),
    );
  }

  /// The column as the screen declared it, before any filling or dragging.
  GridColumn<T> _original(GridColumn<T> column) =>
      widget.columns.firstWhere((each) => each.id == column.id);

  Widget _row(BuildContext context, List<GridColumn<T>> columns, int index) {
    final theme = Theme.of(context);
    final row = widget.rowAt(index);
    if (row == null) {
      // Ask for it, then show a placeholder until it arrives. Scheduled
      // post-frame so a fast scroll does not fire a request per frame for rows
      // it has already flown past.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) widget.onNeedRow(index);
      });
      return _PlaceholderRow(columns: columns);
    }

    final isCursor = index == _cursor;
    final isSelected =
        widget.selectedKey != null &&
        widget.rowKey != null &&
        widget.rowKey!(row) == widget.selectedKey;

    return _GridRow<T>(
      row: row,
      columns: columns,
      striped: index.isOdd,
      cursor: isCursor && _focus.hasFocus,
      selected: isSelected,
      onTap: () => _handleTap(index, row),
      theme: theme,
    );
  }
}

class _MoveIntent extends Intent {
  const _MoveIntent(this.delta);
  final int delta;
}

class _ActivateRowIntent extends Intent {
  const _ActivateRowIntent();
}

class _HeaderCell<T> extends StatelessWidget {
  const _HeaderCell({
    required this.column,
    required this.sorted,
    required this.descending,
    required this.resizing,
    required this.onResizeStart,
    required this.onResizeUpdate,
    required this.onResizeEnd,
    required this.onResetWidth,
    this.onTap,
  });

  final GridColumn<T> column;
  final bool sorted;
  final bool descending;
  final VoidCallback? onTap;

  /// This column's edge is being dragged.
  final bool resizing;

  /// Pointer positions are global x coordinates.
  final void Function(double x) onResizeStart;
  final void Function(double x) onResizeUpdate;
  final VoidCallback onResizeEnd;
  final VoidCallback onResetWidth;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final label = Text(
      column.label,
      overflow: TextOverflow.ellipsis,
      textAlign: column.align,
      style: theme.textTheme.labelMedium?.copyWith(
        color: sorted
            ? CrawlStatTheme.palette(context).accent
            : theme.colorScheme.onSurfaceVariant,
      ),
    );

    // The same padding and alignment as the cells below, so a heading sits
    // exactly over its column whether or not it is sortable.
    final content = Padding(
      padding: const EdgeInsets.symmetric(horizontal: _cellPadding),
      child: Row(
        mainAxisAlignment: column.align == TextAlign.right
            ? MainAxisAlignment.end
            : MainAxisAlignment.start,
        children: [
          Flexible(child: label),
          if (sorted) ...[
            const SizedBox(width: 3),
            Icon(
              descending ? AppIcon.sortDescending : AppIcon.sortAscending,
              size: 12,
              color: CrawlStatTheme.palette(context).accent,
            ),
          ],
        ],
      ),
    );

    return SizedBox(
      width: column.width,
      height: double.infinity,
      // The handle is a sibling laid over the cell's right edge, not a child
      // of its InkWell, so grabbing the edge never also sorts the column.
      child: Stack(
        children: [
          Positioned.fill(
            child: Tooltip(
              message:
                  column.tooltip ??
                  (onTap == null ? '' : 'Sort by ${column.label}'),
              child: onTap == null
                  ? content
                  : InkWell(onTap: onTap, child: content),
            ),
          ),
          Positioned(
            top: 0,
            right: 0,
            bottom: 0,
            width: _resizeHandleWidth,
            child: _ResizeHandle(
              label: column.label,
              active: resizing,
              onStart: onResizeStart,
              onUpdate: onResizeUpdate,
              onEnd: onResizeEnd,
              onReset: onResetWidth,
            ),
          ),
        ],
      ),
    );
  }
}

/// The grab area on a header's right edge: drag to resize the column,
/// double-click to put it back to its default width.
class _ResizeHandle extends StatefulWidget {
  const _ResizeHandle({
    required this.label,
    required this.active,
    required this.onStart,
    required this.onUpdate,
    required this.onEnd,
    required this.onReset,
  });

  final String label;
  final bool active;
  final void Function(double x) onStart;
  final void Function(double x) onUpdate;
  final VoidCallback onEnd;
  final VoidCallback onReset;

  @override
  State<_ResizeHandle> createState() => _ResizeHandleState();
}

class _ResizeHandleState extends State<_ResizeHandle> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final Color line;
    if (widget.active) {
      line = CrawlStatTheme.palette(context).accent;
    } else if (_hovered) {
      line = theme.colorScheme.onSurfaceVariant;
    } else {
      line = theme.dividerColor;
    }

    return Semantics(
      label: 'Resize ${widget.label} column',
      child: MouseRegion(
        cursor: SystemMouseCursors.resizeColumn,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        // Unlike the rows, waiting out the double-tap timeout costs nothing
        // here: a single click on an edge does nothing anyway, and a drag is
        // claimed as soon as the pointer moves.
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          // Measure from where the button went down, not from where the
          // pointer had got to once it cleared the drag slop — otherwise the
          // first few pixels of every drag are lost.
          dragStartBehavior: DragStartBehavior.down,
          onDoubleTap: widget.onReset,
          onHorizontalDragStart: (details) =>
              widget.onStart(details.globalPosition.dx),
          onHorizontalDragUpdate: (details) =>
              widget.onUpdate(details.globalPosition.dx),
          onHorizontalDragEnd: (_) => widget.onEnd(),
          onHorizontalDragCancel: widget.onEnd,
          child: Align(
            alignment: Alignment.centerRight,
            // Inset at rest so the header reads as one strip, full height
            // while it is in play so it is clear which edge is moving.
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 120),
              width: 1,
              margin: EdgeInsets.symmetric(
                vertical: widget.active || _hovered ? 0 : 8,
              ),
              color: line,
            ),
          ),
        ),
      ),
    );
  }
}

class _GridRow<T> extends StatelessWidget {
  const _GridRow({
    required this.row,
    required this.columns,
    required this.striped,
    required this.cursor,
    required this.selected,
    required this.onTap,
    required this.theme,
  });

  final T row;
  final List<GridColumn<T>> columns;
  final bool striped;
  final bool cursor;
  final bool selected;
  final VoidCallback onTap;
  final ThemeData theme;

  @override
  Widget build(BuildContext context) {
    Color? background;
    if (selected) {
      background = CrawlStatTheme.palette(context).accent
          .withValues(alpha: 0.14);
    } else if (striped) {
      background = theme.colorScheme.surfaceContainer.withValues(alpha: 0.35);
    }

    return InkWell(
      onTap: onTap,
      // Explicit: the theme's hover is transparent so rounded panels stay
      // clean, but a square table row is exactly the case that wants it.
      hoverColor: theme.colorScheme.surfaceContainerHigh,
      child: Container(
        decoration: BoxDecoration(
          color: background,
          border: Border(
            bottom: BorderSide(
              color: theme.dividerColor.withValues(alpha: 0.5),
            ),
          ),
        ),
        child: Row(
          children: [
            Container(
              width: _indicatorWidth,
              color: cursor
                  ? CrawlStatTheme.palette(context).accent
                  : Colors.transparent,
            ),
            for (final column in columns)
              SizedBox(
                width: column.width,
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: _cellPadding),
                  child: Align(
                    alignment: column.align == TextAlign.right
                        ? Alignment.centerRight
                        : Alignment.centerLeft,
                    child: column.cell != null
                        ? column.cell!(context, row)
                        : Text(
                            column.value(row),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            textAlign: column.align,
                            style: column.mono
                                ? CrawlStatTheme.mono(
                                    context,
                                    color: column.color?.call(row),
                                  )
                                : theme.textTheme.bodyMedium?.copyWith(
                                    color: column.color?.call(row),
                                  ),
                          ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// A row whose page has not arrived. Shaped like real content, so the table
/// does not jump when it does.
class _PlaceholderRow<T> extends StatelessWidget {
  const _PlaceholderRow({required this.columns});

  final List<GridColumn<T>> columns;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: [
        const SizedBox(width: _indicatorWidth),
        for (final column in columns)
          SizedBox(
            width: column.width,
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: _cellPadding,
                vertical: 12,
              ),
              child: Align(
                alignment: Alignment.centerLeft,
                child: FractionallySizedBox(
                  widthFactor: 0.6,
                  child: Container(
                    height: 8,
                    decoration: BoxDecoration(
                      color: theme.dividerColor.withValues(alpha: 0.5),
                      borderRadius: BorderRadius.circular(2),
                    ),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}
