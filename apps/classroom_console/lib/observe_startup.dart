import 'package:flutter/widgets.dart';
import 'package:screen_retriever/screen_retriever.dart';

/// Logical work-area coordinates already exclude the menu bar and Dock/taskbar.
/// Use the Teacher window's center to choose the monitor, not source resolution.
Rect observeWorkArea(List<Display> displays, Offset? teacherCenter) {
  final areas = displays
      .map((d) {
        final position = d.visiblePosition ?? Offset.zero;
        final size = d.visibleSize ?? d.size;
        return position & size;
      })
      .where((r) => r.width > 0 && r.height > 0)
      .toList();
  if (areas.isEmpty) throw StateError('No display work area available');
  return areas.firstWhere(
    (r) => teacherCenter != null && r.contains(teacherCenter),
    orElse: () => areas.first,
  );
}
