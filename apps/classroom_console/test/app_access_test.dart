import 'package:classroom_console/agent.dart';
import 'package:classroom_console/app_access.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_store.dart';

class AppsAgent extends Agent with FakeStoreAgent {
  AppsAgent(this.store) : super([]);
  @override
  final FakeStore store;
  @override
  bool get connected => true;
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async => action == 'app_policy_state'
      ? {
          'ok': true,
          'result': {
            'rule': {
              'scope': null,
              'value': {
                'enabled': true,
                'mode': 'block',
                'ids': ['aaaaaaaaaaaa', 'bbbbbbbbbbbb'],
              },
            },
          },
        }
      : {'ok': false};
}

void main() {
  testWidgets(
    'reopening restores both applied checkboxes with stale preferences',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1024, 768));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final store = FakeStore()
        ..apply('inventory', {
          'cache': {
            'apps': [
              {
                'id': 'aaaaaaaaaaaa',
                'name': 'Calculator',
                'executables': 1,
                'students': ['pc'],
              },
              {
                'id': 'bbbbbbbbbbbb',
                'name': 'Paint',
                'executables': 1,
                'students': ['pc'],
              },
            ],
          },
        });
      for (var opening = 0; opening < 2; opening++) {
        final agent = AppsAgent(store);
        await tester.pumpWidget(
          MaterialApp(
            home: AppsWindow(
              key: ValueKey(opening),
              agent: agent,
              studentIds: const ['pc'],
              audience: 'class',
              classWide: true,
            ),
          ),
        );
        for (var pending = 0; pending < 4; pending++) {
          await tester.pump();
          await tester.runAsync(() async {
            await Future<void>.delayed(const Duration(milliseconds: 100));
          });
        }
        await tester.pump();
        await tester.runAsync(() async {
          await (tester.state(find.byType(AppsWindow)) as dynamic).loadCache();
        });
        await tester.pump();
        expect(find.text('Calculator'), findsOneWidget);
        expect(find.text('Paint'), findsOneWidget);
        expect(
          tester
              .widgetList<Checkbox>(find.byType(Checkbox))
              .map((c) => c.value),
          [true, true],
        );
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox());
        agent.dispose();
      }
    },
  );
}
