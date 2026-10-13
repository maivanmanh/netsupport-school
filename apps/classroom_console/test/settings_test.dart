import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/settings.dart';

void main() {
  testWidgets(
    'role-specific settings navigate at small desktop size and validate connection',
    (tester) async {
      tester.view.physicalSize = const Size(740, 520);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final agent = Agent([])
        ..state = {'control_port': 47800, 'teacher_ip': '192.168.50.94'};
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () => showDialog<TeacherSettings>(
                  context: context,
                  builder: (_) => TeacherSettingsDialog(agent: agent),
                ),
                child: const Text('Settings'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Settings'));
      await tester.pumpAndSettle();
      expect(find.text('Cài đặt giảng viên'), findsOneWidget);
      expect(find.text('Student'), findsNothing);
      for (final section in [
        'Phát màn hình',
        'Quan sát',
        'Truyền tệp',
        'Chẩn đoán',
        'Cài đặt máy học viên',
        'Kết nối',
      ]) {
        await tester.scrollUntilVisible(
          find.descendant(
            of: find.byKey(const ValueKey('teacher-settings-sections')),
            matching: find.text(section),
          ),
          section == 'Kết nối' ? -80 : 80,
          scrollable: find.descendant(
            of: find.byKey(const ValueKey('teacher-settings-sections')),
            matching: find.byType(Scrollable),
          ),
        );
        await tester.ensureVisible(
          find.descendant(
            of: find.byKey(const ValueKey('teacher-settings-sections')),
            matching: find.text(section),
          ),
        );
        await tester.pumpAndSettle();
        await tester.tap(find.text(section));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
      }
      await tester.enterText(find.byType(TextField).first, 'bad-ip');
      await tester.tap(find.text('Lưu & kết nối lại'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Nhập địa chỉ IPv4'), findsOneWidget);
      expect(find.byType(TeacherSettingsDialog), findsOneWidget);
      agent.dispose();
    },
  );
}
