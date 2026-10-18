import 'package:classroom_console/agent.dart';
import 'package:classroom_console/internet_access.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

class InternetAgent extends Agent {
  InternetAgent() : super([]);
  final saved = <Map<String, dynamic>>[];
  final requests = <String>[];
  @override
  bool get connected => true;
  @override
  Future<void> savePreferences(Map<String, dynamic> changes) async =>
      saved.add(changes);
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async {
    requests.add(action);
    return {'ok': true, 'result': {}};
  }
}

void main() {
  ButtonStyleButton apply(WidgetTester tester) =>
      tester.widget<ButtonStyleButton>(
        find.ancestor(
          of: find.text('Áp dụng'),
          matching: find.byWidgetPredicate((w) => w is ButtonStyleButton),
        ),
      );

  testWidgets('Apply needs a real change from the policy in force', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1100, 760));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final agent = InternetAgent()
      ..state = {
        'students': [
          {'id': 'pc', 'online': true},
        ],
        'tools': {'internet': <String, dynamic>{}},
      };
    await tester.pumpWidget(
      MaterialApp(
        home: InternetWindow(
          agent: agent,
          studentIds: const ['pc'],
          audience: 'class',
        ),
      ),
    );
    await tester.pump();
    // Nothing is restricted and the window opens on "Cho phép tất cả".
    expect(apply(tester).onPressed, isNull);
    await tester.tap(find.text('Chặn tất cả'));
    await tester.pump();
    expect(apply(tester).onPressed, isNotNull);
    // Going back to what is already in force cancels the pending change.
    await tester.tap(find.text('Cho phép tất cả'));
    await tester.pump();
    expect(apply(tester).onPressed, isNull);
    await tester.pumpWidget(const SizedBox());
    agent.dispose();
  });

  testWidgets('after applying, Apply stays off until the policy is edited', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1100, 760));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final agent = InternetAgent()
      ..state = {
        'students': [
          {'id': 'pc', 'online': true},
        ],
        'tools': {'internet': <String, dynamic>{}},
      };
    await tester.pumpWidget(
      MaterialApp(
        home: InternetWindow(
          agent: agent,
          studentIds: const ['pc'],
          audience: 'class',
        ),
      ),
    );
    await tester.pump();
    await tester.tap(find.text('Chặn tất cả'));
    await tester.pump();
    await tester.tap(find.text('Áp dụng'));
    await tester.pump();
    await tester.pump();
    expect(agent.requests, contains('tool_policy'));
    final stored =
        agent.saved
                .where((p) => p.containsKey('internet_applied'))
                .single['internet_applied']
            as Map;
    expect(stored['class'], '["block_all",[]]');
    final dynamic state = tester.state(find.byType(InternetWindow));
    expect(state.changed, isFalse);
    state.setState(() => state.mode = 'open');
    expect(state.changed, isTrue);
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 2));
    agent.dispose();
  });
}
