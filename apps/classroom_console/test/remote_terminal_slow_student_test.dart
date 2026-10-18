import 'dart:async';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/remote_terminal.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// A Student that never answers `terminal_open`, like a very slow or hung lab PC.
class HungStudentAgent extends Agent {
  HungStudentAgent() : super([]);
  final opened = <Completer<Map<String, dynamic>>>[];
  bool online = true;
  @override
  bool get connected => true;
  @override
  List<Map<String, dynamic>> get students => [
    {'id': 'pc', 'online': online},
  ];
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) {
    if (action != 'terminal_open') return Future.value({'ok': true});
    final completer = Completer<Map<String, dynamic>>();
    opened.add(completer);
    return completer.future;
  }
}

void main() {
  setUp(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('window_manager'),
          (_) async => null,
        );
  });

  var clock = DateTime(2026);
  Future<HungStudentAgent> open(WidgetTester tester) async {
    clock = DateTime(2026);
    final agent = HungStudentAgent();
    await tester.pumpWidget(
      MaterialApp(
        home: RemoteTerminalWindow(
          agent: agent,
          studentId: 'pc',
          name: 'Student',
          now: () => clock,
        ),
      ),
    );
    await tester.pump();
    return agent;
  }

  testWidgets('waiting status shows elapsed time as the Student stays silent', (
    tester,
  ) async {
    await open(tester);
    expect(find.text('Đang kết nối tới học viên…'), findsOneWidget);
    clock = clock.add(const Duration(seconds: 5));
    await tester.pump(const Duration(seconds: 1));
    expect(find.textContaining('phản hồi chậm'), findsOneWidget);
    clock = clock.add(const Duration(seconds: 7));
    await tester.pump(const Duration(seconds: 1));
    expect(find.textContaining('Vẫn đang chờ học viên'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('offline Student is named instead of a generic spinner', (
    tester,
  ) async {
    final agent = await open(tester);
    agent.online = false;
    await tester.pump(const Duration(seconds: 1));
    expect(find.textContaining('đang offline'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a failed start can be retried and opens a new session', (
    tester,
  ) async {
    final agent = await open(tester);
    agent.opened.single.complete({'ok': false, 'error': 'Student is offline'});
    await tester.pump();
    await tester.pump();
    expect(find.text('Máy học viên đang offline.'), findsOneWidget);
    await tester.tap(find.text('Thử lại'));
    await tester.pump();
    expect(agent.opened, hasLength(2));
    expect(find.text('Máy học viên đang offline.'), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });
}
