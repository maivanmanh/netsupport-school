import 'dart:io';

import 'package:classroom_console/agent.dart';
import 'package:classroom_console/observe.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

class ClassControlAgent extends Agent {
  ClassControlAgent() : super([]);
  @override
  bool get connected => true;
  @override
  Future<void> command(
    String action, [
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
  ]) async {}
  final inputs = <Map<String, dynamic>>[];
  @override
  void observeInput(String id, Map<String, dynamic> input) {
    inputs.add(input);
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  testWidgets('class presentation keeps acknowledged shared Control selected', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1600, 1000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    final nativeCalls = <MethodCall>[];
    messenger.setMockMethodCallHandler(
      const MethodChannel('window_manager'),
      (_) async => null,
    );
    messenger.setMockMethodCallHandler(
      const MethodChannel('classroom/observe'),
      (call) async {
        nativeCalls.add(call);
        if (call.method == 'attach') return 4;
        if (call.method == 'stats') {
          return {
            'width': 1920,
            'height': 1080,
            'sequence': 2,
            'native_frames': 2,
          };
        }
        return null;
      },
    );
    final agent = ClassControlAgent()
      ..state = {
        'observe': {
          'id': 'source',
          'phase': 'live',
          'mode': 'control',
          'class_control': true,
          'surface': '/fixture/frame',
        },
      };
    await tester.pumpWidget(
      MaterialApp(
        home: ObserveWindow(
          agent: agent,
          studentId: 'source',
          name: 'Student A',
        ),
      ),
    );
    agent.notifyListeners();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump();
    final selector = tester.widget<SegmentedButton<String>>(
      find.byType(SegmentedButton<String>).first,
    );
    expect(selector.selected, {'control'});
    expect(selector.onSelectionChanged, isNull);
    expect(
      nativeCalls.where(
        (c) => c.method == 'captureSystemKeys' && c.arguments == true,
      ),
      isNotEmpty,
    );
    Future<void> systemKey(String label, bool down) async {
      final data = const StandardMethodCodec().encodeMethodCall(
        MethodCall('systemKey', {'key': label, 'down': down}),
      );
      await messenger.handlePlatformMessage('classroom/observe', data, (_) {});
    }

    await systemKey('Meta', true);
    await systemKey('c', true);
    await systemKey('c', false);
    await systemKey('Meta', false);
    expect(
      agent.inputs
          .where((x) => x['action'] == 'key')
          .map((x) => [x['key'], x['down']]),
      [
        ['Meta', true],
        ['c', true],
        ['c', false],
        ['Meta', false],
      ],
    );
    await tester.pumpWidget(const SizedBox());
    agent.dispose();
    messenger.setMockMethodCallHandler(
      const MethodChannel('classroom/observe'),
      null,
    );
    messenger.setMockMethodCallHandler(
      const MethodChannel('window_manager'),
      null,
    );
  });

  testWidgets('Accessibility warning has an in-place request button', (
    tester,
  ) async {
    if (!Platform.isMacOS) return;
    await tester.binding.setSurfaceSize(const Size(1600, 1000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    var captureAttempts = 0;
    var permissionRequests = 0;
    messenger.setMockMethodCallHandler(
      const MethodChannel('window_manager'),
      (_) async => null,
    );
    messenger.setMockMethodCallHandler(
      const MethodChannel('classroom/observe'),
      (call) async {
        if (call.method == 'attach') return 4;
        if (call.method == 'stats') {
          return {
            'width': 1920,
            'height': 1080,
            'sequence': 2,
            'native_frames': 2,
            'connected': true,
          };
        }
        if (call.method == 'captureSystemKeys' && call.arguments == true) {
          captureAttempts++;
          if (captureAttempts == 1) {
            throw PlatformException(code: 'keyboard_permission');
          }
        }
        return null;
      },
    );
    messenger.setMockMethodCallHandler(
      const MethodChannel('classroom/permissions'),
      (call) async {
        if (call.method == 'requestAccessibility') {
          permissionRequests++;
          return true;
        }
        return null;
      },
    );
    final agent = ClassControlAgent()
      ..state = {
        'observe': {
          'id': 'source',
          'phase': 'live',
          'mode': 'control',
          'surface': '/fixture/frame',
        },
        'students': [
          {'id': 'source', 'online': true},
        ],
      };
    await tester.pumpWidget(
      MaterialApp(
        home: ObserveWindow(agent: agent, studentId: 'source', name: 'Mac'),
      ),
    );
    agent.notifyListeners();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(
      find.text(
        'Hãy cấp quyền Accessibility cho Teacher để chuyển phím tắt macOS',
      ),
      findsOneWidget,
    );
    final request = find.byKey(
      const ValueKey('request-accessibility-permission'),
    );
    expect(request, findsOneWidget);
    expect(find.text('Yêu cầu quyền'), findsOneWidget);
    await tester.tap(request);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(permissionRequests, 1);
    expect(captureAttempts, 2);
    expect(
      find.text(
        'Hãy cấp quyền Accessibility cho Teacher để chuyển phím tắt macOS',
      ),
      findsNothing,
    );

    await tester.pumpWidget(const SizedBox());
    agent.dispose();
    messenger.setMockMethodCallHandler(
      const MethodChannel('classroom/observe'),
      null,
    );
    messenger.setMockMethodCallHandler(
      const MethodChannel('classroom/permissions'),
      null,
    );
    messenger.setMockMethodCallHandler(
      const MethodChannel('window_manager'),
      null,
    );
  });
}
