import 'dart:ui' show PointerDeviceKind;
import 'package:classroom_console/performance_toggle.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'settings_test.dart' show ShareAgent;

void main() {
  testWidgets(
    'three icon modes, default high, tooltips explain, tap switches',
    (tester) async {
      final agent = ShareAgent();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: PerformanceToggle(agent: agent)),
        ),
      );
      expect(find.byWidgetPredicate((w) => w is Icon), findsNWidgets(3));
      expect(find.byType(Text), findsNothing); // icons only

      var buttons = tester.widget<ToggleButtons>(find.byType(ToggleButtons));
      expect(buttons.isSelected, [true, false, false]);

      final hover = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await hover.addPointer(location: Offset.zero);
      await hover.moveTo(tester.getCenter(find.byIcon(Icons.balance)));
      await tester.pump(const Duration(seconds: 1));
      expect(find.textContaining('Tiết kiệm'), findsOneWidget);
      expect(
        find.textContaining('Dùng khi mạng/Wi-Fi chập chờn'),
        findsOneWidget,
      );
      await hover.moveTo(Offset.zero);

      await tester.tap(find.byIcon(Icons.battery_saver));
      expect(agent.shareCalls.last, {
        'action': 'set_performance',
        'input': {'mode': 'minimal'},
      });

      agent.state = {
        'performance': {'requested': 'normal', 'active': 'economy'},
      };
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: PerformanceToggle(agent: agent)),
        ),
      );
      buttons = tester.widget<ToggleButtons>(find.byType(ToggleButtons));
      expect(buttons.isSelected, [false, true, false]);
    },
  );
}
