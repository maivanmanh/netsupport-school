import 'package:flutter/widgets.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'dart:math' as math;

/// Outer logical window bounds for a source displayed without upscaling.
Rect observeNativeBounds(
  Rect work,
  Size source,
  double pixelRatio,
  Size frame,
) {
  final native = source / (pixelRatio > 0 ? pixelRatio : 1);
  final scale = math.min(
    1.0,
    math.min(
      math.max(1, work.width - frame.width) / native.width,
      math.max(1, work.height - frame.height) / native.height,
    ),
  );
  final size = Size(
    native.width * scale + frame.width,
    native.height * scale + frame.height,
  );
  return Rect.fromCenter(
    center: work.center,
    width: size.width,
    height: size.height,
  );
}

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
