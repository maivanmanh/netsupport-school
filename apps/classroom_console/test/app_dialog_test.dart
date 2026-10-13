import 'package:classroom_console/app_dialog.dart';
import 'package:classroom_console/main.dart';
import 'package:classroom_console/window_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> open(
  WidgetTester tester,
  Size size,
  Widget Function(BuildContext) dialog,
) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      theme: classroomTheme(),
      home: Builder(
        builder: (context) => TextButton(
          onPressed: () => showDialog<void>(context: context, builder: dialog),
          child: const Text('open'),
        ),
      ),
    ),
  );
  await tester.tap(find.text('open'));
  // A busy button spins forever, so settle by time rather than by quiescence.
  await tester.pump(const Duration(milliseconds: 400));
}

void main() {
  testWidgets(
    'confirmDialog returns the choice and uses the destructive tone',
    (tester) async {
      bool? result;
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(900, 700);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await tester.pumpWidget(
        MaterialApp(
          theme: classroomTheme(),
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () async => result = await confirmDialog(
                context,
                title: 'Tắt máy?',
                message: 'Học viên nhận cảnh báo 15 giây.',
                confirmLabel: 'Tắt máy',
                destructive: true,
              ),
              child: const Text('go'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('go'));
      await tester.pumpAndSettle();
      expect(find.text('Tắt máy?'), findsOneWidget);
      await tester.tap(find.text('Hủy'));
      await tester.pumpAndSettle();
      expect(result, false);
      await tester.tap(find.text('go'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Tắt máy'));
      await tester.pumpAndSettle();
      expect(result, true);
      // The close button is a cancel, never a confirmation.
      await tester.tap(find.text('go'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Đóng'));
      await tester.pumpAndSettle();
      expect(result, false);
    },
  );

  testWidgets('a tall body scrolls inside a short window without overflow', (
    tester,
  ) async {
    await open(
      tester,
      const Size(740, 520),
      (context) => AppDialog(
        icon: Icons.settings,
        title: 'Tiêu đề rất dài để kiểm tra việc cắt chữ khi cửa sổ hẹp lại',
        subtitle: 'Mô tả ngắn',
        width: 800,
        actions: [
          AppDialog.cancel(context),
          AppDialog.primary(label: 'Lưu', onPressed: () {}),
        ],
        child: Column(children: [for (var i = 0; i < 40; i++) Text('Dòng $i')]),
      ),
    );
    expect(tester.takeException(), isNull);
    expect(find.text('Lưu'), findsOneWidget);
    expect(find.text('Hủy'), findsOneWidget);
  });

  testWidgets('primary action shows progress and ignores taps while busy', (
    tester,
  ) async {
    var taps = 0;
    await open(
      tester,
      const Size(900, 700),
      (context) => AppDialog(
        title: 'Đang làm',
        actions: [
          AppDialog.primary(label: 'Gửi', busy: true, onPressed: () => taps++),
        ],
        child: const Text('nội dung'),
      ),
    );
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    await tester.tap(find.text('Gửi'), warnIfMissed: false);
    expect(taps, 0);
  });

  testWidgets('WindowShell footer spans the full window width', (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1000, 700);
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      MaterialApp(
        theme: classroomTheme(),
        home: const WindowShell(
          icon: Icons.apps,
          title: 'Cửa sổ',
          subtitle: 'Mô tả',
          footer: Text('chân trang'),
          child: Center(child: Text('nội dung')),
        ),
      ),
    );
    final footer = find.ancestor(
      of: find.text('chân trang'),
      matching: find.byType(Container),
    );
    expect(tester.getSize(footer.first).width, 1000);
    expect(find.text('Cửa sổ'), findsOneWidget);
  });
}
