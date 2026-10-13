import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/hardware_control.dart';

class _HardwareAgent extends Agent {
  _HardwareAgent() : super(const []);
  final calls = <Map<String, dynamic>>[];
  @override
  bool get connected => true;
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async {
    calls.add({
      'action': action,
      'selected': selected.toList(),
      'input': input,
    });
    return {'ok': true};
  }
}

void main() {
  testWidgets('a switch sends a class-wide edit without device selection', (
    tester,
  ) async {
    final agent = _HardwareAgent();
    await tester.pumpWidget(
      MaterialApp(home: HardwareControlWindow(agent: agent)),
    );
    await tester.scrollUntilVisible(find.text('Wi-Fi'), 240);
    final card = find.ancestor(
      of: find.text('Wi-Fi'),
      matching: find.byType(Card),
    );
    final toggle = find.descendant(of: card, matching: find.byType(Switch));
    await tester.ensureVisible(toggle);
    await tester.pumpAndSettle();
    await tester.tap(toggle);
    await tester.pumpAndSettle();
    expect(agent.calls.single, {
      'action': 'hardware_policy',
      'selected': <String>[],
      'input': {'group': 'wifi', 'blocked': true},
    });
    await tester.pumpWidget(const SizedBox.shrink());
    agent.dispose();
  });
  test('offline and unsupported never count as applied', () {
    final hardware = {
      'reports': {
        'pc': {
          'groups': {
            'wifi': {'status': 'applied', 'error': ''},
          },
        },
      },
    };
    expect(
      hardwareResult(
        hardware,
        {
          'pc': {'hardware': 1},
        },
        'pc',
        'wifi',
        false,
      )['status'],
      'pending',
    );
    expect(
      hardwareResult(hardware, {}, 'pc', 'wifi', true)['status'],
      'unsupported',
    );
    expect(
      hardwareResult(
        hardware,
        {
          'pc': {'hardware': 1},
        },
        'pc',
        'wifi',
        true,
      )['status'],
      'applied',
    );
    expect(
      hardwareResult(
        hardware,
        {
          'pc': {'hardware': 1},
        },
        'pc',
        'bluetooth',
        true,
      )['status'],
      'pending',
    );
  });
  testWidgets(
    'independent class window contains five switches and persistent lifecycle notice',
    (tester) async {
      final agent = Agent(const []);
      await tester.pumpWidget(
        MaterialApp(home: HardwareControlWindow(agent: agent)),
      );
      expect(find.text('Hardware Control'), findsOneWidget);
      expect(find.textContaining('Student restart'), findsOneWidget);
      expect(find.text('USB Storage'), findsOneWidget);
      await tester.scrollUntilVisible(find.text('Wi-Fi'), 240);
      expect(find.text('Wi-Fi'), findsOneWidget);
      await tester.scrollUntilVisible(find.text('Bluetooth'), 240);
      expect(find.text('Bluetooth'), findsOneWidget);
      await tester.scrollUntilVisible(find.text('USB Tethering'), 240);
      expect(find.text('USB Tethering'), findsOneWidget);
      await tester.scrollUntilVisible(find.text('Camera / Microphone'), 240);
      expect(find.text('Camera / Microphone'), findsOneWidget);
      expect(find.textContaining('chọn từng máy'), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      agent.dispose();
    },
  );
}
