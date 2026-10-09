import 'package:classroom_console/agent.dart';
import 'package:classroom_console/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

class SwitchAgent extends Agent {
  SwitchAgent() : super([]);
  final commands = <(String, List<String>)>[];
  @override
  bool get connected => true;
  @override
  Future<void> command(
    String action, [
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
  ]) async {
    commands.add((action, selected.toList()));
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'active class can switch Students and return to Teacher; Stop cancels transition',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      final windowCalls = <String>[];
      messenger.setMockMethodCallHandler(
        const MethodChannel('window_manager'),
        (call) async {
          windowCalls.add(call.method);
          return null;
        },
      );
      messenger.setMockMethodCallHandler(
        const MethodChannel('classroom/permissions'),
        (call) async => call.method == 'status'
            ? {'screen': true, 'helperScreen': true, 'onboardingSeen': true}
            : true,
      );
      final agent = SwitchAgent()
        ..state = {
          'phase': 'sharing',
          'source_id': null,
          'students': [
            {
              'id': 'A',
              'name': 'Student A',
              'ip': '10.0.55.11',
              'online': true,
            },
            {
              'id': 'B',
              'name': 'Student B',
              'ip': '10.0.55.12',
              'online': true,
            },
          ],
        };
      await tester.pumpWidget(ClassroomApp(agent: agent));
      await tester.pumpAndSettle();
      expect(find.text('Set up macOS permissions'), findsNothing);
      await tester.tap(find.byTooltip('Select computers for presentation'));
      await tester.pumpAndSettle();
      await tester.tap(find.byType(Checkbox).first);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Control & present Student'));
      await tester.pumpAndSettle();
      expect(agent.commands.last.$1, 'switch_source');
      expect(agent.commands.last.$2, ['A']);
      agent.state.addAll({'phase': 'switching', 'next_source': 'A'});
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.text('Switching source…'), findsOneWidget);
      await tester.tap(find.text('Stop sharing'));
      expect(agent.commands.last.$1, 'stop');
      agent.state.addAll({'phase': 'sharing', 'source_id': 'A'});
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.text('Presenting: Student A'), findsOneWidget);
      expect(windowCalls.where((c) => c == 'minimize'), isEmpty);
      expect(
        tester
            .widget<OutlinedButton>(
              find.widgetWithText(OutlinedButton, 'Control & present Student'),
            )
            .onPressed,
        isNull,
      );
      await tester.tap(find.byType(Checkbox).first);
      await tester.tap(find.byType(Checkbox).at(1));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Control & present Student'));
      expect(agent.commands.last.$1, 'switch_source');
      expect(agent.commands.last.$2, ['B']);
      await tester.tap(find.text('Back to Teacher'));
      expect(agent.commands.last.$1, 'switch_source');
      expect(agent.commands.last.$2, isEmpty);
      await tester.binding.setSurfaceSize(const Size(740, 520));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
      messenger.setMockMethodCallHandler(
        const MethodChannel('window_manager'),
        null,
      );
      messenger.setMockMethodCallHandler(
        const MethodChannel('classroom/permissions'),
        null,
      );
    },
  );
}
