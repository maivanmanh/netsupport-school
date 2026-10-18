import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/settings.dart';

ButtonStyleButton saveButton(WidgetTester tester) =>
    tester.widget<ButtonStyleButton>(
      find.ancestor(
        of: find.byWidgetPredicate(
          (w) =>
              w is Text &&
              (w.data == 'Lưu & kết nối lại' || w.data == 'Lưu & áp dụng ngay'),
        ),
        matching: find.byWidgetPredicate((w) => w is ButtonStyleButton),
      ),
    );

void main() {
  economyTests();
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
        'Cài đặt máy học viên',
        'Truyền tệp',
        'Chẩn đoán',
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
      // Nothing was edited yet: Save must not be available.
      expect(saveButton(tester).onPressed, isNull);
      await tester.enterText(find.byType(TextField).first, 'bad-ip');
      await tester.pump();
      expect(saveButton(tester).onPressed, isNotNull);
      await tester.tap(find.text('Lưu & kết nối lại'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Nhập địa chỉ IPv4'), findsOneWidget);
      expect(find.byType(TeacherSettingsDialog), findsOneWidget);
      agent.dispose();
    },
  );

  testWidgets('Save follows the difference from the opening values', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1000, 700);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final agent = Agent([])..state = {'control_port': 47800};
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
    expect(saveButton(tester).onPressed, isNull);
    await tester.enterText(find.byType(TextField).first, '10.0.0.5');
    await tester.pump();
    expect(saveButton(tester).onPressed, isNotNull);
    // Typing the original value back cancels the change.
    await tester.enterText(find.byType(TextField).first, '');
    await tester.pump();
    expect(saveButton(tester).onPressed, isNull);
    agent.dispose();
  });

  testWidgets('Student sharing is one switch, off by default, on port 8080', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1000, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final agent = ShareAgent()..state = {'control_port': 47800};
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
    await tester.tap(find.text('Cài đặt máy học viên'));
    await tester.pumpAndSettle();
    // No port field, no progress bar, no extra buttons: one switch, off.
    expect(find.text('Cổng tải xuống HTTP'), findsNothing);
    expect(find.byType(LinearProgressIndicator), findsNothing);
    expect(find.textContaining('Chọn ZIP'), findsNothing);
    final tile = find.widgetWithText(
      SwitchListTile,
      'Chia sẻ ứng dụng Student đi kèm',
    );
    expect(tester.widget<SwitchListTile>(tile).value, isFalse);
    await tester.tap(tile);
    await tester.pump();
    expect(agent.shareCalls, [
      {
        'action': 'student_download_start',
        'input': {'port': 8080},
      },
    ]);
    // The Teacher reports the share as ready: the address and counts appear.
    agent.state = {
      'control_port': 47800,
      'student_download': {
        'phase': 'sharing',
        'url': 'http://192.168.50.27:8080/',
        'script_url': 'http://192.168.50.27:8080/install.bat',
        'script_path': '/tmp/Install-Classroom-Student.bat',
        'arch': 'x86_64',
        'size': 15000000,
        'downloads': 2,
      },
    };
    agent.notifyListeners();
    await tester.pump();
    expect(tester.widget<SwitchListTile>(tile).value, isTrue);
    expect(find.text('http://192.168.50.27:8080/'), findsOneWidget);
    expect(find.textContaining('2 lượt tải'), findsOneWidget);
    // The one-step installer is offered next to the ZIP link.
    expect(find.text('http://192.168.50.27:8080/install.bat'), findsOneWidget);
    expect(find.text('Cài đặt một bước (tệp .bat)'), findsOneWidget);
    expect(find.text('Sao chép liên kết'), findsOneWidget);
    expect(find.byKey(const ValueKey('script-save')), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsNothing);
    await tester.tap(tile);
    await tester.pump();
    expect(agent.shareCalls.last['action'], 'student_download_stop');
    agent.dispose();
  });
}

void economyTests() {
  testWidgets(
    'economy mode: selector, Wi-Fi switch, cap per Student, saved apart from video',
    (tester) async {
      tester.view.physicalSize = const Size(1000, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final agent = ShareAgent()
        ..state = {
          'control_port': 47800,
          'students': [
            for (var i = 0; i < 40; i++) {'id': 's$i', 'online': true},
          ],
          'performance': {
            'requested': 'normal',
            'active': 'economy',
            'reason': 'wifi',
            'width': 1280,
            'height': 720,
            'max_fps': 20,
            'bitrate_kbps': 3000,
          },
        };
      TeacherSettings? result;
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () async =>
                    result = await showDialog<TeacherSettings>(
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
      await tester.tap(find.text('Phát màn hình'));
      await tester.pumpAndSettle();
      expect(
        find.text('Chế độ tiết kiệm (mạng yếu / máy yếu)'),
        findsOneWidget,
      );
      // What is in force (here: Wi-Fi made it economy) is stated.
      expect(
        find.textContaining(
          'Đang áp dụng: Tiết kiệm (do đang dùng Wi-Fi) · 1280 × 720 · 20 fps · 3.0 Mbps',
        ),
        findsOneWidget,
      );
      // The Wi-Fi switch belongs to normal mode only.
      expect(
        find.text('Tự chuyển sang Tiết kiệm khi máy giảng viên dùng Wi-Fi'),
        findsOneWidget,
      );
      await tester.tap(find.text('Tiết kiệm'));
      await tester.pump();
      expect(
        find.text('Tự chuyển sang Tiết kiệm khi máy giảng viên dùng Wi-Fi'),
        findsNothing,
      );
      expect(find.textContaining('1280 × 720, 20 fps'), findsOneWidget);
      await tester.enterText(
        find.widgetWithText(
          TextField,
          'Giới hạn băng thông gửi cho cả lớp (Mbps)',
        ),
        '30',
      );
      await tester.pump();
      expect(
        find.text(
          'Với 40 máy đang online: khoảng 562 kbps mỗi máy, hình tối đa 640 × 360.',
        ),
        findsOneWidget,
      );
      await tester.tap(find.text('Lưu & kết nối lại'));
      await tester.pumpAndSettle();
      expect(result, isNotNull);
      expect(result!.overlay['performance'], {
        'mode': 'economy',
        'auto_wifi': false,
        'bandwidth_cap_mbps': 30,
      });
      // The picture settings the Teacher chose are not rewritten by the mode.
      expect((result!.overlay['video'] as Map)['width'], 1920);
      expect((result!.overlay['video'] as Map)['max_fps'], 30);
      agent.dispose();
    },
  );
  testWidgets('an invalid cap is refused on the page that has it', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1000, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final agent = ShareAgent()..state = {'control_port': 47800};
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
    await tester.tap(find.text('Phát màn hình'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(
        TextField,
        'Giới hạn băng thông gửi cho cả lớp (Mbps)',
      ),
      '1',
    );
    await tester.pump();
    await tester.tap(find.text('Lưu & kết nối lại'));
    await tester.pumpAndSettle();
    expect(
      find.textContaining('Giới hạn băng thông phải từ 2 đến 10000'),
      findsOneWidget,
    );
    agent.dispose();
  });
}

class ShareAgent extends Agent {
  ShareAgent() : super([]);
  final shareCalls = <Map<String, dynamic>>[];
  @override
  bool get connected => true;
  @override
  Future<void> command(
    String action, [
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
  ]) async {
    shareCalls.add({'action': action, if (input != null) 'input': input});
  }
}
