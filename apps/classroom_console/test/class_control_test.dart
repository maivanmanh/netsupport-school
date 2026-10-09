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
  @override
  void observeInput(String id, Map<String, dynamic> input) {}
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
}
