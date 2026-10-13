import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/observe.dart';

class StartupAgent extends Agent {
  StartupAgent() : super([]);
  int opens = 0;
  @override
  bool get connected => true;
  @override
  void observeInput(String id, Map<String, dynamic> input) {}
  @override
  Future<void> commandChecked(
    String action,
    Iterable<String> selected,
    Map<String, dynamic> input,
  ) async {
    if (action == 'observe_open') {
      opens++;
      expect(input['restart'], false);
    }
  }
}

void main() {
  testWidgets(
    'slow first frame does not restart publisher or resize preview window',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      var frames = 0;
      var resizes = 0;
      messenger.setMockMethodCallHandler(
        const MethodChannel('window_manager'),
        (call) async {
          if (call.method == 'setBounds') resizes++;
          return null;
        },
      );
      messenger.setMockMethodCallHandler(
        const MethodChannel('classroom/observe'),
        (call) async => call.method == 'attach'
            ? 4
            : call.method == 'stats'
            ? {
                'width': 1280,
                'height': 1024,
                'connected': true,
                'sequence': frames,
                'source_sequence': frames,
                'native_frames': frames,
              }
            : null,
      );
      final agent = StartupAgent()
        ..state = {
          'observe': {
            'id': 'pc',
            'phase': 'live',
            'mode': 'observe',
            'surface': '/test/startup',
          },
          'students': [
            {'id': 'pc', 'online': true},
          ],
        };
      await tester.pumpWidget(
        MaterialApp(
          home: ObserveWindow(
            agent: agent,
            studentId: 'pc',
            name: 'PC',
            initialSourceSize: const Size(506, 404),
          ),
        ),
      );
      agent.notifyListeners();
      await tester.pump(const Duration(milliseconds: 150));
      // Reproduce the measured cold-start delay (> the old 3-second retry gate).
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 3300)),
      );
      await tester.pump(const Duration(milliseconds: 500));
      expect(agent.opens, 1);
      expect(find.text('Đang kết nối tới học viên…'), findsOneWidget);
      frames = 1;
      final dynamic window = tester.state(find.byType(ObserveWindow));
      await window.poll();
      await tester.pump();
      expect(window.live, true);
      expect(
        resizes,
        0,
        reason: 'rounded thumbnail aspect must retain initial bounds',
      );
      expect(agent.opens, 1);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
      messenger.setMockMethodCallHandler(
        const MethodChannel('window_manager'),
        null,
      );
      messenger.setMockMethodCallHandler(
        const MethodChannel('classroom/observe'),
        null,
      );
    },
  );
}
