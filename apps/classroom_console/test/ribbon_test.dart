import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/main.dart';
import 'package:classroom_console/agent.dart';

class LiveAgent extends Agent {
  LiveAgent() : super([]);
  @override
  bool get connected => true;
}

Map<String, dynamic> student(int i, {bool online = true}) => {
  'id': '$i',
  'name': 'Máy ${i + 1}',
  'ip': '10.0.55.${i + 11}',
  'online': online,
  'monitoring': 'ready',
};

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  Future<LiveAgent> pump(WidgetTester tester, Size size) async {
    tester.view.physicalSize = size;
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final agent = LiveAgent()
      ..state = {
        'phase': 'idle',
        'students': [for (var i = 0; i < 6; i++) student(i, online: i < 5)],
      };
    await tester.pumpWidget(ClassroomApp(agent: agent));
    await tester.pumpAndSettle();
    await tester.pump(const Duration(seconds: 1));
    return agent;
  }

  testWidgets('ribbon tabs show labelled tools and switch groups', (
    tester,
  ) async {
    await pump(tester, const Size(1280, 800));
    expect(find.text('Kiểm soát lớp'), findsOneWidget);
    expect(find.text('Internet'), findsOneWidget);
    expect(find.text('Gửi tệp'), findsNothing);
    await tester.tap(find.text('Tin nhắn & tệp'));
    await tester.pumpAndSettle();
    expect(find.text('Gửi tệp'), findsOneWidget);
    expect(find.text('Internet'), findsNothing);
    await tester.tap(find.text('Hệ thống'));
    await tester.pumpAndSettle();
    expect(find.text('Chẩn đoán'), findsOneWidget);
  });

  testWidgets('Chọn máy shows selection count on the finish button', (
    tester,
  ) async {
    await pump(tester, const Size(1280, 800));
    expect(find.textContaining('Cả lớp'), findsNothing);
    await tester.tap(find.text('Chọn máy'));
    await tester.pumpAndSettle();
    expect(find.text('Xong chọn'), findsOneWidget);
    await tester.tap(find.text('Chọn tất cả máy online'));
    await tester.pumpAndSettle();
    expect(find.text('Xong chọn (5)'), findsOneWidget);
    await tester.tap(find.text('Bỏ chọn'));
    await tester.pumpAndSettle();
    expect(find.text('Xong chọn'), findsOneWidget);
    await tester.tap(find.text('Xong chọn'));
    await tester.pumpAndSettle();
    expect(find.text('Chọn máy'), findsOneWidget);
  });

  testWidgets('share is an icon-only button with a tooltip', (tester) async {
    await pump(tester, const Size(1280, 800));
    expect(find.text('Chiếu màn hình giảng viên'), findsNothing);
    expect(find.byTooltip('Chiếu màn hình giảng viên'), findsOneWidget);
    expect(find.byIcon(Icons.cast), findsOneWidget);
  });

  testWidgets('narrow window keeps every tool reachable without overflow', (
    tester,
  ) async {
    await pump(tester, const Size(900, 600));
    expect(find.text('Internet'), findsOneWidget);
    expect(find.byTooltip('Mở rộng thanh công cụ'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('per-Student menu button replaces inline icons', (tester) async {
    await pump(tester, const Size(1280, 800));
    expect(find.byIcon(Icons.more_vert), findsNWidgets(6));
    expect(find.byIcon(Icons.folder_copy_outlined), findsNothing);
    await tester.tap(find.byIcon(Icons.more_vert).first);
    await tester.pumpAndSettle();
    expect(find.text('Quan sát màn hình'), findsOneWidget);
  });

  testWidgets('tool order follows the teacher workflow', (tester) async {
    await pump(tester, const Size(1400, 800));
    double x(String label) => tester.getTopLeft(find.text(label)).dx;
    final control = [
      'Mở website',
      'Chạy ứng dụng',
      'Internet',
      'Chặn ứng dụng',
      'VS Code',
      'Khóa / Tắt máy',
    ].map(x).toList();
    expect(control, [...control]..sort());
    await tester.tap(find.text('Tin nhắn & tệp'));
    await tester.pumpAndSettle();
    final files = [
      'Thu bài / tệp',
      'Gửi tệp',
      'Gửi tin nhắn',
      'Phản hồi',
    ].map(x).toList();
    expect(files, [...files]..sort());
  });

  testWidgets('offline computers use a darker surface than online ones', (
    tester,
  ) async {
    await pump(tester, const Size(1280, 800));
    Color? cardColor(String name) => tester
        .widget<Card>(
          find.ancestor(of: find.text(name), matching: find.byType(Card)).first,
        )
        .color;
    expect(cardColor('Máy 1'), isNull);
    expect(cardColor('Máy 6'), const Color(0xffd3dae1));
  });
}
