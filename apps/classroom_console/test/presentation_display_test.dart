import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/settings.dart';

void main() {
  for (final savedMode in [null, false, true]) {
    testWidgets(
      'presentation display loads $savedMode and saves opposite mode',
      (tester) async {
        tester.view.physicalSize = const Size(1200, 1000);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final agent = Agent([])
          ..state = {
            'control_port': 47800,
            'settings': {
              'network': {'bind_ip': '10.0.68.200'},
              'video': {
                if (savedMode != null)
                  'presentation_preserve_aspect': savedMode,
              },
            },
          };
        addTearDown(agent.dispose);
        TeacherSettings? result;
        await tester.pumpWidget(
          MaterialApp(
            home: Builder(
              builder: (context) => TextButton(
                onPressed: () async {
                  result = await showDialog<TeacherSettings>(
                    context: context,
                    builder: (_) => TeacherSettingsDialog(agent: agent),
                  );
                },
                child: const Text('Settings'),
              ),
            ),
          ),
        );
        await tester.tap(find.text('Settings'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Phát màn hình'));
        await tester.pumpAndSettle();
        final field = find.byWidgetPredicate(
          (widget) => widget is DropdownButtonFormField<bool>,
        );
        expect(field, findsOneWidget);
        final state = tester.state<FormFieldState<bool>>(field);
        expect(state.value, savedMode ?? false);
        await tester.tap(field);
        await tester.pumpAndSettle();
        final opposite = !(savedMode ?? false);
        await tester.tap(
          find
              .text(
                opposite
                    ? 'Giữ tỉ lệ nguồn (có thể có viền đen)'
                    : 'Kéo giãn phủ kín màn hình (mặc định)',
              )
              .last,
        );
        await tester.pumpAndSettle();
        await tester.tap(find.text('Lưu & kết nối lại'));
        await tester.pumpAndSettle();
        expect(result, isNotNull);
        expect(
          result!.overlay['video']['presentation_preserve_aspect'],
          opposite,
        );
        expect(tester.takeException(), isNull);
      },
    );
  }
}
