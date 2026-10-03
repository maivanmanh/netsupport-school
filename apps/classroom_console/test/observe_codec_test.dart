import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/observe_stats.dart';

void main() {
  test('codec label shows codec and negotiated fps ceiling', () {
    expect(
      ObserveStatsData.codecLabel({'codec': 'H.265', 'fps_cap': 60}),
      'H.265 · 60 fps max',
    );
    expect(ObserveStatsData.codecLabel({'codec': 'H.264'}), 'H.264');
    expect(ObserveStatsData.codecLabel({}), isNull);
  });

  testWidgets('session stats panel has a codec row', (tester) async {
    final data = ObserveStatsData(
      mode: 'control',
      live: true,
      native: {'width': 1920, 'height': 1080},
      metrics: {
        'codec': 'H.265',
        'fps_cap': 60,
        'sampled_unix_us': DateTime.now().microsecondsSinceEpoch,
        'receive_mbps': 5.2,
      },
      fps: 58.6,
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: ObserveStatsPanel(data: data)),
      ),
    );
    expect(find.text('Codec'), findsOneWidget);
    expect(find.text('H.265 · 60 fps max'), findsOneWidget);
  });
}
