import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/student_file_drop.dart';

void main() {
  testWidgets('Highlight, cancel, confirm and recheck disconnected recipient', (
    tester,
  ) async {
    var online = true;
    final sent = <String>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: StudentFileDrop(
            name: 'Lab PC 02',
            ip: '10.0.55.12',
            canSend: () => online,
            onSend: (items) async {
              sent.addAll(items.map((item) => item.path));
            },
            child: const SizedBox(
              width: 300,
              height: 160,
              child: Text('Student'),
            ),
          ),
        ),
      ),
    );
    DropTarget target() => tester.widget<DropTarget>(find.byType(DropTarget));
    final hover = DropEventDetails(
      localPosition: Offset.zero,
      globalPosition: Offset.zero,
    );
    final drop = DropDoneDetails(
      files: [DropItemFile('/tmp/lesson.txt')],
      localPosition: Offset.zero,
      globalPosition: Offset.zero,
    );
    target().onDragEntered!(hover);
    await tester.pumpAndSettle();
    expect(find.text('Drop to send…'), findsOneWidget);
    target().onDragExited!(hover);
    await tester.pumpAndSettle();
    expect(find.text('Drop to send…'), findsNothing);
    target().onDragDone!(drop);
    await tester.pumpAndSettle();
    expect(find.text('Lab PC 02'), findsOneWidget);
    expect(find.text('10.0.55.12'), findsOneWidget);
    expect(find.text('lesson.txt'), findsOneWidget);
    expect(sent, isEmpty);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(sent, isEmpty);
    target().onDragDone!(drop);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(sent, ['/tmp/lesson.txt']);
    target().onDragDone!(drop);
    await tester.pumpAndSettle();
    online = false;
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(sent, hasLength(1));
    expect(find.textContaining('no longer connected'), findsOneWidget);
  });
}
