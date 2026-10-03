import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'package:classroom_console/observe_startup.dart';

void main() {
  test(
    'Observe fills the Teacher monitor work area in logical Retina units',
    () {
      const displays = [
        Display(
          id: 'main',
          size: Size(1512, 982),
          scaleFactor: 2,
          visiblePosition: Offset(0, 32),
          visibleSize: Size(1512, 900),
        ),
        Display(
          id: 'left',
          size: Size(1920, 1080),
          scaleFactor: 1,
          visiblePosition: Offset(-1920, 0),
          visibleSize: Size(1920, 1040),
        ),
      ];
      expect(
        observeWorkArea(displays, const Offset(700, 400)),
        const Rect.fromLTWH(0, 32, 1512, 900),
      );
      expect(
        observeWorkArea(displays, const Offset(-800, 500)),
        const Rect.fromLTWH(-1920, 0, 1920, 1040),
      );
    },
  );
}
