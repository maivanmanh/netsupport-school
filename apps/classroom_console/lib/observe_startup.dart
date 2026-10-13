import 'package:flutter/widgets.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'dart:math' as math;

/// A thumbnail carries aspect, not the original display's pixel resolution.
/// Fit that aspect in the work area before showing the loading window.
Rect observePreviewBounds(Rect work, Size source, Size frame) {
  final scale = math.min(
    math.max(1, work.width - frame.width) / source.width,
    math.max(1, work.height - frame.height) / source.height,
  );
  return Rect.fromCenter(
    center: work.center,
    width: source.width * scale + frame.width,
    height: source.height * scale + frame.height,
  );
}

/// Thumbnail even-pixel rounding must not resize the window on first video.
bool observeSameAspect(Size a, Size b) =>
    !a.isEmpty &&
    !b.isEmpty &&
    ((a.width / a.height) / (b.width / b.height) - 1).abs() < .01;

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
