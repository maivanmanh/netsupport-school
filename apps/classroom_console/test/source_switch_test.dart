import 'package:classroom_console/agent.dart';
import 'package:classroom_console/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';

class SwitchAgent extends Agent {
  SwitchAgent() : super([]);
  final commands = <(String, List<String>)>[];
  String? rejection;
  @override
  Future<void> commandChecked(
    String action,
    Iterable<String> selected,
    Map<String, dynamic>? input,
  ) async {
    await command(action, selected, input);
    if (rejection != null) throw StateError(rejection!);
  }

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
          'phase': 'idle',
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
      agent.state['phase'] = 'sharing';
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.byTooltip('Dừng chiếu'), findsOneWidget);
      expect(
        find.byTooltip('Điều khiển & chiếu màn hình học viên'),
        findsNothing,
      );
      agent.state['phase'] = 'idle';
      agent.notifyListeners();
      await tester.pumpAndSettle();
      windowCalls.clear();
      // Context-menu target works without enabling checkbox selection.
      await tester.tap(find.text('Student B'), buttons: kSecondaryMouseButton);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Điều khiển & chiếu màn hình học viên'));
      await tester.pumpAndSettle();
      expect(agent.commands.last.$1, 'switch_source');
      expect(agent.commands.last.$2, ['B']);
      await tester.tap(find.byTooltip('Chọn nhiều máy để thao tác'));
      await tester.pumpAndSettle();
      await tester.tap(find.byType(Checkbox).first);
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Điều khiển & chiếu màn hình học viên'));
      await tester.pumpAndSettle();
      expect(agent.commands.last.$1, 'switch_source');
      expect(agent.commands.last.$2, ['A']);
      agent.rejection =
          'Close Observe/Control windows before presenting a Student';
      await tester.tap(find.byTooltip('Điều khiển & chiếu màn hình học viên'));
      await tester.pump();
      // Even an immediate authoritative state update cannot erase the feedback.
      agent.error = null;
      agent.notifyListeners();
      await tester.pump();
      expect(
        find.textContaining('Không chiếu được màn hình học viên:'),
        findsOneWidget,
      );
      agent.rejection = null;
      agent.state.addAll({'phase': 'switching', 'next_source': 'A'});
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.text('Đang đổi nguồn chiếu…'), findsOneWidget);
      await tester.tap(find.byTooltip('Dừng chiếu'));
      expect(agent.commands.last.$1, 'stop');
      agent.state.addAll({'phase': 'sharing', 'source_id': 'A'});
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.text('Đang chiếu: Student A'), findsOneWidget);
      expect(windowCalls.where((c) => c == 'minimize'), isEmpty);
      expect(
        tester
            .widget<IconButton>(
              find.byWidgetPredicate(
                (w) =>
                    w is IconButton &&
                    w.tooltip == 'Điều khiển & chiếu màn hình học viên',
              ),
            )
            .onPressed,
        isNull,
      );
      await tester.tap(find.byType(Checkbox).first);
      await tester.tap(find.byType(Checkbox).at(1));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Điều khiển & chiếu màn hình học viên'));
      expect(agent.commands.last.$1, 'switch_source');
      expect(agent.commands.last.$2, ['B']);
      await tester.tap(find.byTooltip('Quay lại màn hình giảng viên'));
      expect(agent.commands.last.$1, 'switch_source');
      expect(agent.commands.last.$2, isEmpty);
      (agent.state['students'] as List)[1]['online'] = false;
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(
        find.byTooltip('Máy học viên đã chọn đang offline'),
        findsOneWidget,
      );
      expect(
        tester
            .widget<IconButton>(
              find.byWidgetPredicate(
                (w) =>
                    w is IconButton &&
                    w.tooltip == 'Máy học viên đã chọn đang offline',
              ),
            )
            .onPressed,
        isNull,
      );
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
