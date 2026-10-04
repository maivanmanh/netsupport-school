import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/observe.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/observe_stats.dart';
import 'package:window_manager/window_manager.dart';

class ObserveAgent extends Agent {
  ObserveAgent() : super([]);
  final inputs = <Map<String, dynamic>>[];
  @override
  bool get connected => true;
  @override
  Future<void> command(
    String action, [
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
  ]) async {}
  @override
  void observeInput(String id, Map<String, dynamic> input) {
    inputs.add(input);
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test('Session stats reject stale, missing and invalid measurements', () {
    final now = DateTime.utc(2026, 10, 3);
    ObserveStatsData data({
      bool live = true,
      int age = 0,
      dynamic delay = 85.4,
    }) => ObserveStatsData(
      mode: 'control',
      live: live,
      native: {'width': 1600, 'height': 900},
      metrics: {
        'sampled_unix_us': now.microsecondsSinceEpoch - age,
        'receive_mbps': 2.5,
        'capture_to_surface_ms_p50': delay,
        'clock_uncertainty_ms': 1.5,
      },
      fps: 24,
      now: now,
    );
    final fresh = data();
    expect(fresh.resolution, '1600 × 900');
    expect(fresh.bitrate, 2.5);
    expect(fresh.delay, 85.4);
    expect(fresh.uncertainty, 1.5);
    expect(data(age: 3000001).bitrate, isNull);
    expect(data(age: -1).delay, isNull);
    expect(data(delay: double.nan).delay, isNull);
    expect(data(delay: -1).delay, isNull);
    expect(data(delay: null).delay, isNull);
    expect(data(live: false).delay, isNull);
    expect(data(live: false).resolution, '—');
  });
  test('Width preserves aspect and Fill stretches both dimensions', () {
    const viewport = Size(1200, 800), source = Size(1080, 1920);
    final width = observeImageSize(viewport, source, stretch: false);
    expect(
      observeImageSize(viewport, Size.zero, stretch: false),
      const Size(1200, 675),
    );
    expect(width.width, 1200);
    expect(width.height, closeTo(2133.3333, 0.001));
    expect(observeImageSize(viewport, source, stretch: true), viewport);
    expect(
      observePosition(Offset(width.width / 2, width.height / 2), width),
      const Offset(0.5, 0.5),
    );
    expect(
      observePosition(const Offset(-10, 9999), viewport),
      const Offset(0, 1),
    );
    final cover = observePosition(
      const Offset(0, 300),
      const Size(800, 600),
      source: const Size(1920, 1080),
      fit: BoxFit.cover,
    );
    expect(cover.dx, closeTo(0.125, 0.0001));
    expect(cover.dy, closeTo(0.5, 0.0001));
  });
  testWidgets(
    'Independent fit window stays read-only until acknowledged Control',
    (t) async {
      final titles = <String>[];
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(const MethodChannel('window_manager'), (
            c,
          ) async {
            if (c.method == 'setTitle') titles.add(c.arguments['title']);
            return null;
          });
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(
            const MethodChannel('classroom/observe'),
            (c) async => c.method == 'attach'
                ? 4
                : c.method == 'stats'
                ? {
                    'width': 1920,
                    'height': 1080,
                    'sequence': 2,
                    'native_frames': 2,
                  }
                : null,
          );
      final a = ObserveAgent()
        ..state = {
          'observe': {
            'id': 'pc',
            'phase': 'live',
            'mode': 'observe',
            'surface': '/private/tmp/test-frame',
            'metrics': {
              'sampled_unix_us': DateTime.now().microsecondsSinceEpoch,
              'receive_mbps': 3.25,
              'capture_to_surface_ms_p50': 76.5,
              'clock_uncertainty_ms': 2.5,
            },
          },
        };
      await t.pumpWidget(
        MaterialApp(
          home: ObserveWindow(agent: a, studentId: 'pc', name: 'PC 01'),
        ),
      );
      a.notifyListeners();
      await t.pump();
      expect(titles.last, 'Observe · PC 01');
      await t.pump(const Duration(milliseconds: 100));
      await t.pump();
      expect(
        find.text('Connecting to student…'),
        findsNothing,
        reason:
            'An available first frame must not wait for the one-second stats tick',
      );
      await t.pump(const Duration(milliseconds: 900));
      expect(find.byType(Dialog), findsNothing);
      expect(find.text('Fill'), findsOneWidget);
      expect(find.text('Observe · Read only'), findsOneWidget);
      expect(find.byType(Texture), findsOneWidget);
      expect(find.byType(ObserveStatsPanel), findsNothing);
      // Widget setup may take real seconds on Windows/VMs. Keep the mocked
      // live report fresh at the point where its display is being tested.
      (a.state['observe'] as Map)['metrics']['sampled_unix_us'] =
          DateTime.now().microsecondsSinceEpoch;
      a.notifyListeners();
      await t.tap(find.byKey(const ValueKey('observe-stats-toggle')));
      await t.pump();
      expect(find.byType(ObserveStatsPanel), findsOneWidget);
      expect(find.text('1920 × 1080'), findsNWidgets(2));
      expect(find.text('3.25 Mbps'), findsOneWidget);
      expect(find.text('77 ms'), findsOneWidget);
      expect(
        t
            .widget<IgnorePointer>(
              find
                  .descendant(
                    of: find.byType(ObserveStatsPanel),
                    matching: find.byType(IgnorePointer),
                  )
                  .first,
            )
            .ignoring,
        isTrue,
      );
      await t.tap(find.byKey(const ValueKey('observe-stats-toggle')));
      await t.pump();
      expect(find.byType(ObserveStatsPanel), findsNothing);
      expect(
        t.getSize(find.byKey(const ValueKey('observe-image'))),
        const Size(800, 600),
      );
      await t.tap(find.text('Fill'));
      await t.pump();
      expect(
        t.getSize(find.byKey(const ValueKey('observe-image'))),
        const Size(800, 600),
      );
      await t.tap(find.text('Control'));
      await t.pump();
      expect(a.inputs.any((v) => v['action'] == 'control'), true);
      expect(find.text('Changing input mode…'), findsOneWidget);
      expect(titles.last, 'Observe · PC 01');
      a.state['observe']['mode'] = 'control';
      a.notifyListeners();
      await t.pump();
      expect(titles.last, 'Control · PC 01');
      expect(
        find.text('Shared control · Student input stays enabled'),
        findsOneWidget,
      );
      final image = find.byKey(const ValueKey('observe-image'));
      expect(
        find.byWidgetPredicate(
          (w) => w is SegmentedButton<String> && w.selected.contains('fill'),
        ),
        findsOneWidget,
      );
      final region = find
          .descendant(of: image, matching: find.byType(MouseRegion))
          .first;
      expect(t.widget<MouseRegion>(region).cursor, SystemMouseCursors.basic);
      final mouse = await t.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(400, 300));
      await mouse.moveTo(const Offset(600, 450));
      await t.pump();
      final movement = a.inputs.lastWhere((v) => v['action'] == 'move');
      expect(movement['x'], 0.75);
      expect(movement['y'], 0.75);
      await mouse.down(const Offset(600, 450));
      await mouse.up();
      final clicks = a.inputs.where((v) => v['action'] == 'button_at');
      expect(clicks.any((v) => v['down'] == true), isTrue);
      expect(clicks.any((v) => v['down'] == false), isTrue);
      expect(
        clicks.map((v) => v['key']).toList(),
        ['left', 'left'],
        reason: 'A left click must not synthesize right/middle button releases',
      );
      for (final click in clicks) {
        expect(click['x'], 0.75);
        expect(click['y'], 0.75);
      }
      await mouse.removePointer();
      a.inputs.clear();
      final right = await t.createGesture(
        pointer: 2,
        kind: PointerDeviceKind.mouse,
        buttons: kSecondaryMouseButton,
      );
      await right.addPointer(location: const Offset(600, 450));
      await right.down(const Offset(600, 450));
      await right.up();
      expect(
        a.inputs
            .where((v) => v['action'] == 'button_at')
            .map((v) => (v['key'], v['down']))
            .toList(),
        [('right', true), ('right', false)],
      );
      await right.removePointer();
      a.inputs.clear();
      final chord = TestPointer(3, PointerDeviceKind.mouse, 3);
      await t.sendEventToBinding(
        chord.addPointer(location: const Offset(600, 450)),
      );
      await t.sendEventToBinding(chord.down(const Offset(600, 450)));
      await t.sendEventToBinding(
        chord.move(
          const Offset(600, 450),
          buttons: kPrimaryMouseButton | kSecondaryMouseButton,
        ),
      );
      await t.sendEventToBinding(
        chord.move(const Offset(600, 450), buttons: kSecondaryMouseButton),
      );
      await t.sendEventToBinding(chord.up());
      expect(
        a.inputs
            .where((v) => v['action'] == 'button_at')
            .map((v) => (v['key'], v['down']))
            .toList(),
        [('left', true), ('right', true), ('left', false), ('right', false)],
      );
      await t.sendEventToBinding(chord.removePointer());
      a.inputs.clear();
      final listener = t.state(find.byType(ObserveWindow)) as WindowListener;
      await mouse.addPointer(location: const Offset(600, 450));
      await mouse.down(const Offset(600, 450));
      await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      listener.onWindowBlur();
      await t.pump();
      expect(a.inputs.any((v) => v['action'] == 'observe'), isFalse);
      expect(a.state['observe']['mode'], 'control');
      expect(
        a.inputs.any(
          (v) =>
              v['action'] == 'key' && v['key'] == 'Shift' && v['down'] == false,
        ),
        isTrue,
      );
      expect(
        a.inputs.lastWhere((v) => v['action'] == 'button_at')['down'],
        isFalse,
      );
      a.inputs.clear();
      await mouse.moveTo(const Offset(620, 460));
      await t.pump(const Duration(seconds: 1));
      expect(a.inputs.any((v) => v['action'] == 'move'), isFalse);
      expect(a.inputs.any((v) => v['action'] == 'keepalive'), isTrue);
      await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await mouse.up();
      listener.onWindowFocus();
      await t.pump();
      a.inputs.clear();
      await mouse.moveTo(const Offset(600, 450));
      await t.sendKeyDownEvent(LogicalKeyboardKey.keyA);
      await t.sendKeyUpEvent(LogicalKeyboardKey.keyA);
      expect(a.inputs.any((v) => v['action'] == 'move'), isTrue);
      expect(
        a.inputs.any((v) => v['action'] == 'key' && v['key'] == 'a'),
        isTrue,
      );
      expect(
        a.inputs.any(
          (v) => v['action'] == 'control' || v['action'] == 'observe',
        ),
        isFalse,
      );
      await mouse.removePointer();
      await mouse.addPointer(location: const Offset(600, 450));
      await t.pump(const Duration(seconds: 3));
      await t.pump(const Duration(milliseconds: 200));
      expect(
        t.widget<AnimatedOpacity>(find.byType(AnimatedOpacity).first).opacity,
        0,
      );
      await mouse.moveTo(const Offset(400, 30));
      await t.pump(const Duration(milliseconds: 200));
      expect(
        t.widget<AnimatedOpacity>(find.byType(AnimatedOpacity).first).opacity,
        0,
        reason: 'Hidden toolbar must not reveal or intercept input at 30 px',
      );
      await mouse.moveTo(const Offset(400, 570));
      await t.pump(const Duration(milliseconds: 200));
      expect(
        t.widget<AnimatedOpacity>(find.byType(AnimatedOpacity).first).opacity,
        0,
        reason: 'Hidden footer must not reveal 30 px from the bottom',
      );
      expect(
        t.getSize(find.byKey(const ValueKey('observe-top-edge'))).height,
        5,
      );
      expect(
        t.getSize(find.byKey(const ValueKey('observe-bottom-edge'))).height,
        5,
      );
      await mouse.moveTo(const Offset(400, 2));
      await t.pump();
      expect(
        t.widget<AnimatedOpacity>(find.byType(AnimatedOpacity).first).opacity,
        1,
      );
      await mouse.removePointer();
      a.state['observe']['mode'] = 'observe';
      a.notifyListeners();
      await t.pump();
      expect(titles.last, 'Observe · PC 01');
      a.state['observe']['mode'] = 'exclusive';
      a.notifyListeners();
      await t.pump();
      expect(titles.last, 'Control · PC 01');
      a.state['observe']['phase'] = 'ended';
      a.notifyListeners();
      await t.pump();
      expect(titles.last, 'Observe · PC 01');
      await t.pumpWidget(const SizedBox());
      a.dispose();
    },
  );
  testWidgets('Stretch portrait image maps pointer through its crop', (
    t,
  ) async {
    var source = const Size(1080, 1920);
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('window_manager'),
          (c) async => null,
        );
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('classroom/observe'),
          (c) async => c.method == 'attach'
              ? 4
              : c.method == 'stats'
              ? {
                  'width': source.width,
                  'height': source.height,
                  'sequence': 2,
                  'native_frames': 2,
                }
              : null,
        );
    final a = ObserveAgent()
      ..state = {
        'observe': {
          'id': 'pc',
          'phase': 'live',
          'mode': 'observe',
          'surface': '/private/tmp/test-frame',
        },
      };
    await t.pumpWidget(
      MaterialApp(
        home: ObserveWindow(agent: a, studentId: 'pc', name: 'PC'),
      ),
    );
    a.notifyListeners();
    await t.pump();
    await t.pump(const Duration(seconds: 1));
    await t.pump();
    await t.tap(find.text('Stretch'));
    await t.pump();
    await t.tap(find.text('Control'));
    a.state['observe']['mode'] = 'control';
    a.notifyListeners();
    await t.pump();
    final mouse = await t.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(400, 300));
    await mouse.moveTo(const Offset(400, 300));
    await t.pump();
    final movement = a.inputs.lastWhere((v) => v['action'] == 'move');
    expect(movement['x'], closeTo(0.5, 0.0001));
    expect(movement['y'], closeTo(0.5, 0.0001));
    // The wheel belongs to the remote machine in Control, not to the viewport.
    await t.sendEventToBinding(
      const PointerScrollEvent(
        position: Offset(400, 300),
        scrollDelta: Offset(0, 48),
      ),
    );
    await t.pump();
    expect(a.inputs.lastWhere((v) => v['action'] == 'scroll')['y'], 2);
    a.inputs.clear();
    for (var i = 0; i < 8; i++) {
      await t.sendEventToBinding(
        const PointerScrollEvent(
          position: Offset(400, 300),
          scrollDelta: Offset(0, 3),
        ),
      );
    }
    expect(a.inputs.where((v) => v['action'] == 'scroll').map((v) => v['y']), [
      1,
    ], reason: 'Small wheel deltas must accumulate rather than disappear');
    a.inputs.clear();
    await t.sendEventToBinding(
      const PointerPanZoomStartEvent(pointer: 8, position: Offset(400, 300)),
    );
    for (var i = 1; i <= 3; i++) {
      await t.sendEventToBinding(
        PointerPanZoomUpdateEvent(
          pointer: 8,
          position: const Offset(400, 300),
          pan: Offset(0, -8.0 * i),
          panDelta: const Offset(0, -8),
        ),
      );
    }
    await t.sendEventToBinding(
      const PointerPanZoomEndEvent(pointer: 8, position: Offset(400, 300)),
    );
    expect(a.inputs.where((v) => v['action'] == 'scroll').map((v) => v['y']), [
      1,
    ], reason: 'Trackpad content pan must become remote wheel input');
    // A new frame shape changes the crop immediately after native stats update.
    source = const Size(1920, 1080);
    await t.pump(const Duration(seconds: 1));
    await t.pump();
    await mouse.moveTo(const Offset(100, 300));
    await t.pump();
    final resized = a.inputs.lastWhere((v) => v['action'] == 'move');
    expect(resized['x'], closeTo(0.21875, 0.0001));
    expect(resized['y'], closeTo(0.5, 0.0001));
    a.state['observe']['mode'] = 'observe';
    a.notifyListeners();
    await t.pump();
    a.inputs.clear();
    await t.sendEventToBinding(
      const PointerScrollEvent(
        position: Offset(400, 300),
        scrollDelta: Offset(0, 48),
      ),
    );
    expect(a.inputs.where((v) => v['action'] == 'scroll'), isEmpty);
    await mouse.removePointer();
    await t.pumpWidget(const SizedBox());
    a.dispose();
  });
  testWidgets('Stretch paints texture while initial native stats are zero', (
    t,
  ) async {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('window_manager'),
          (c) async => null,
        );
    var polls = 0;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(const MethodChannel('classroom/observe'), (
          c,
        ) async {
          if (c.method == 'attach') return 4;
          if (c.method == 'stats') {
            polls++;
            return polls == 1
                ? {'width': 0, 'height': 0, 'sequence': 0, 'native_frames': 0}
                : {
                    'width': 1920,
                    'height': 1080,
                    'sequence': 3,
                    'native_frames': 3,
                  };
          }
          return null;
        });
    final a = ObserveAgent()
      ..state = {
        'observe': {
          'id': 'pc',
          'phase': 'live',
          'mode': 'observe',
          'surface': '/private/tmp/test-frame',
        },
      };
    await t.pumpWidget(
      MaterialApp(
        home: ObserveWindow(agent: a, studentId: 'pc', name: 'PC'),
      ),
    );
    a.notifyListeners();
    await t.pump();
    await t.pump(const Duration(seconds: 1));
    await t.pump();
    expect(t.takeException(), isNull);
    expect(find.byType(Texture), findsOneWidget);
    expect(
      t.getSize(find.byKey(const ValueKey('observe-image'))),
      const Size(800, 600),
    );
    await t.pump(const Duration(seconds: 1));
    await t.pump();
    expect(find.text('Connecting to student…'), findsNothing);
    expect(find.byType(Texture), findsOneWidget);
    expect(t.takeException(), isNull);
    await t.pumpWidget(const SizedBox());
    a.dispose();
  });
  testWidgets('Control heartbeat and focus survive stalled native stats', (
    t,
  ) async {
    var actualFocus = true, blockStats = false;
    final stalled = Completer<Map<String, Object>>();
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(
      const MethodChannel('window_manager'),
      (c) async => c.method == 'isFocused' ? actualFocus : null,
    );
    messenger.setMockMethodCallHandler(
      const MethodChannel('classroom/observe'),
      (c) async {
        if (c.method == 'attach') return 4;
        if (c.method == 'stats') {
          if (blockStats) return stalled.future;
          return {
            'width': 1440,
            'height': 900,
            'sequence': 5,
            'native_frames': 5,
          };
        }
        return null;
      },
    );
    final a = ObserveAgent()
      ..state = {
        'observe': {
          'id': 'pc',
          'phase': 'live',
          'mode': 'control',
          'surface': '/private/tmp/test-frame',
        },
      };
    await t.pumpWidget(
      MaterialApp(
        home: ObserveWindow(agent: a, studentId: 'pc', name: 'PC'),
      ),
    );
    a.notifyListeners();
    await t.pump();
    await t.pump(const Duration(milliseconds: 100));
    await t.pump();
    blockStats = true;
    final listener = t.state(find.byType(ObserveWindow)) as WindowListener;
    listener.onWindowBlur();
    a.inputs.clear();
    await t.pump(const Duration(seconds: 1));
    await t.pump();
    await t.pump(const Duration(seconds: 4));
    await t.pump();
    expect(
      a.inputs.where((v) => v['action'] == 'keepalive').length,
      greaterThanOrEqualTo(3),
    );
    final mouse = await t.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(400, 300));
    await mouse.moveTo(const Offset(450, 300));
    expect(
      a.inputs.any((v) => v['action'] == 'move'),
      isTrue,
      reason: 'Actual key window repairs stale blur state',
    );
    actualFocus = false;
    await t.pump(const Duration(milliseconds: 500));
    await t.pump();
    a.inputs.clear();
    await mouse.moveTo(const Offset(500, 300));
    expect(
      a.inputs.any((v) => v['action'] == 'move'),
      isFalse,
      reason: 'Never forward input from a genuinely unfocused window',
    );
    expect(a.state['observe']['mode'], 'control');
    stalled.complete({
      'width': 1440,
      'height': 900,
      'sequence': 6,
      'native_frames': 6,
    });
    await t.pump();
    await mouse.removePointer();
    await t.pumpWidget(const SizedBox());
    a.dispose();
  });
}
