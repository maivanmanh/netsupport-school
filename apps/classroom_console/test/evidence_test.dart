import 'dart:convert';
import 'dart:io';

import 'package:classroom_console/agent.dart';
import 'package:classroom_console/evidence_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

// A 1x1 PNG: enough for Image.file to decode.
final pixel = base64Decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
);

class EvidenceAgent extends Agent {
  EvidenceAgent(List<Map<String, dynamic>> items) : super([]) {
    state = {
      'students': [
        {'id': 'a', 'name': 'PC-01', 'ip': '10.0.0.1', 'online': true},
        {'id': 'b', 'name': 'PC-02', 'ip': '10.0.0.2', 'online': true},
      ],
      'tools': {
        'evidence': {'items': items, 'total': items.length},
      },
    };
  }
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
    calls.add({'action': action, 'input': input});
    if (action == 'evidence_delete') {
      final ids = List<String>.from(input!['ids'] as List);
      final evidence = (state['tools'] as Map)['evidence'] as Map;
      evidence['items'] = [
        for (final item in evidence['items'] as List)
          if (!ids.contains((item as Map)['id'])) item,
      ];
      notifyListeners();
    }
    return {'ok': true};
  }
}

Map<String, dynamic> shot(
  String id,
  String student,
  String label,
  String path, {
  int minute = 0,
}) => {
  'id': id,
  'student': student,
  'label': label,
  'address': '$label/c/1',
  'title': 'Bài tập',
  'reason': 'conversation',
  'taken_ms': DateTime(2026, 10, 16, 10, minute).millisecondsSinceEpoch,
  'bytes': 1000,
  'path': path,
};

void main() {
  late Directory dir;
  late String image;
  setUp(() {
    dir = Directory.systemTemp.createTempSync('evidence-ui');
    image = '${dir.path}/a.jpg';
    File(image).writeAsBytesSync(pixel);
  });
  tearDown(() => dir.deleteSync(recursive: true));

  test('items are read from the snapshot and counted per computer', () {
    final tools = {
      'evidence': {
        'items': [
          shot('1', 'a', 'chatgpt.com', image),
          shot('2', 'a', 'claude.ai', image),
          shot('3', 'b', 'chatgpt.com', image),
          {'broken': true},
        ],
      },
    };
    expect(evidenceItems(tools), hasLength(3));
    expect(evidenceCountFor(tools, 'a'), 2);
    expect(evidenceCountFor(tools, 'b'), 1);
    expect(evidenceCountFor(tools, 'zzz'), 0);
    expect(evidenceItems(const {}), isEmpty);
    expect(evidenceClock(DateTime(2026, 10, 16, 9, 5)), '09:05');
    expect(evidenceReason('conversation'), 'đang trò chuyện');
  });

  Future<void> open(
    WidgetTester tester,
    EvidenceAgent agent, {
    String? student,
  }) async {
    tester.view.physicalSize = const Size(1200, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () => showEvidence(context, agent, studentId: student),
            child: const Text('Open'),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
  }

  testWidgets(
    'evidence is grouped by computer and the empty state explains itself',
    (tester) async {
      final agent = EvidenceAgent([
        shot('1', 'a', 'chatgpt.com', image, minute: 5),
        shot('2', 'a', 'claude.ai', image, minute: 20),
        shot('3', 'b', 'chatgpt.com', image, minute: 30),
      ]);
      await open(tester, agent);
      expect(find.text('PC-01 · 2 ảnh'), findsOneWidget);
      expect(find.text('PC-02 · 1 ảnh'), findsOneWidget);
      expect(find.textContaining('10:30 · đang trò chuyện'), findsOneWidget);
      expect(find.textContaining('tự xóa khi qua ngày'), findsOneWidget);
      agent.dispose();

      final empty = EvidenceAgent([]);
      await tester.pumpWidget(const SizedBox());
      await open(tester, empty);
      expect(find.text('Chưa có ảnh minh chứng hôm nay'), findsOneWidget);
      empty.dispose();
    },
  );

  testWidgets('a computer-specific view shows only its pictures', (
    tester,
  ) async {
    final agent = EvidenceAgent([
      shot('1', 'a', 'chatgpt.com', image),
      shot('3', 'b', 'claude.ai', image),
    ]);
    await open(tester, agent, student: 'b');
    expect(find.textContaining('Minh chứng · PC-02'), findsOneWidget);
    expect(find.text('claude.ai'), findsOneWidget);
    expect(find.text('chatgpt.com'), findsNothing);
    agent.dispose();
  });

  testWidgets(
    'opening a picture shows its details; deleting it asks the Teacher agent',
    (tester) async {
      final agent = EvidenceAgent([shot('1', 'a', 'chatgpt.com', image)]);
      await open(tester, agent);
      await tester.tap(find.byKey(const ValueKey('evidence-1')));
      await tester.pumpAndSettle();
      expect(find.textContaining('“Bài tập”'), findsOneWidget);
      expect(find.text('Lưu vào Desktop'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('evidence-remove')));
      await tester.pumpAndSettle();
      expect(agent.calls.last, {
        'action': 'evidence_delete',
        'input': {
          'ids': ['1'],
        },
      });
      expect(find.text('Chưa có ảnh minh chứng hôm nay'), findsOneWidget);
      agent.dispose();
    },
  );

  testWidgets('delete all asks first and then removes everything shown', (
    tester,
  ) async {
    final agent = EvidenceAgent([
      shot('1', 'a', 'chatgpt.com', image),
      shot('2', 'b', 'claude.ai', image),
    ]);
    await open(tester, agent);
    await tester.tap(find.byKey(const ValueKey('evidence-delete-all')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Hủy'));
    await tester.pumpAndSettle();
    expect(agent.calls, isEmpty);
    await tester.tap(find.byKey(const ValueKey('evidence-delete-all')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Xóa').last);
    await tester.pumpAndSettle();
    expect(agent.calls.last['input']['ids'], ['1', '2']);
    agent.dispose();
  });

  testWidgets('a picture whose file is gone does not break the list', (
    tester,
  ) async {
    final agent = EvidenceAgent([
      shot('1', 'a', 'chatgpt.com', '${dir.path}/missing.jpg'),
    ]);
    await open(tester, agent);
    // File reads finish outside the test clock.
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 200)),
    );
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.broken_image_outlined), findsOneWidget);
    expect(tester.takeException(), isNull);
    agent.dispose();
  });
}
