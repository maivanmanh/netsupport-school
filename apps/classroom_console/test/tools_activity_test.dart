import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/classroom_tools_ui.dart';

class FakeAgent extends Agent {
  FakeAgent() : super([]);
  @override
  bool get connected => true;
  void push(List<Map<String, dynamic>> jobs) {
    ingestState({
      'teacher_ip': '192.168.50.206',
      'control_port': 47800,
      'students': [
        {
          'id': 'mac',
          'name': 'MacStudio',
          'ip': '192.168.50.2',
          'online': true,
        },
        {'id': 'win', 'name': 'WIN11', 'ip': '192.168.50.94', 'online': true},
      ],
      'tools': {
        'ready': ['mac', 'win'],
        'jobs': jobs,
      },
    });
    notifyListeners();
  }
}

Map<String, dynamic> message(String id, Map<String, String> replies) => {
  'id': id,
  'kind': 'message',
  'name': '',
  'size': 0,
  'text': 'Question $id',
  'targets': [
    for (final e in replies.entries)
      {
        'id': e.key,
        'status': e.value.isEmpty ? 'delivered' : 'replied',
        'done': 0,
        'reply': e.value,
        'error': '',
      },
  ],
};

Map<String, dynamic> file(String id, Map<String, String> status) => {
  'id': id,
  'kind': 'file',
  'name': 'notes.pdf',
  'size': 100,
  'text': '',
  'targets': [
    for (final e in status.entries)
      {
        'id': e.key,
        'status': e.value,
        'done': 0,
        'reply': '',
        'error': e.value == 'error' ? 'disk full' : '',
      },
  ],
};

void main() {
  disconnectTests();
  test('history from a running agent is never re-announced', () {
    final agent = FakeAgent()
      ..push([
        message('old', {'mac': 'earlier reply'}),
        file('oldfile', {'win': 'error'}),
      ]);
    final activity = ToolActivity(agent);
    expect(activity.replies, isEmpty);
    expect(activity.toast, isEmpty);
    expect(activity.finished, isEmpty);
    expect(activity.unread, 0);
    // Further snapshots of the same finished jobs stay silent.
    agent.push([
      message('old', {'mac': 'earlier reply'}),
      file('oldfile', {'win': 'error'}),
    ]);
    expect(activity.toast, isEmpty);
    expect(activity.finished, isEmpty);
    activity.dispose();
  });

  test('many replies aggregate into one toast with machine identity', () {
    final agent = FakeAgent()..push([]);
    final activity = ToolActivity(agent);
    agent.push([
      message('q1', {'mac': '', 'win': ''}),
    ]);
    expect(activity.toast, isEmpty);
    agent.push([
      message('q1', {'mac': 'yes', 'win': ''}),
    ]);
    agent.push([
      message('q1', {'mac': 'yes', 'win': 'no'}),
    ]);
    // Repeated snapshots do not duplicate replies.
    agent.push([
      message('q1', {'mac': 'yes', 'win': 'no'}),
    ]);
    expect(activity.unread, 2);
    expect(activity.toast.map((r) => '${r.name} ${r.ip} ${r.text}'), [
      'WIN11 192.168.50.94 no',
      'MacStudio 192.168.50.2 yes',
    ]);
    expect(activity.replies.first.message, 'Question q1');
    activity.dismissToast();
    expect(activity.toast, isEmpty);
    expect(activity.replies.length, 2);
    activity.markRead();
    expect(activity.unread, 0);
    activity.dispose();
  });

  test('a finished file is announced once; failures stay until dismissed', () {
    final agent = FakeAgent()..push([]);
    final activity = ToolActivity(agent);
    agent.push([
      file('f1', {'mac': 'receiving', 'win': 'receiving'}),
    ]);
    expect(activity.finished, isEmpty);
    agent.push([
      file('f1', {'mac': 'skipped', 'win': 'error'}),
    ]);
    expect(activity.finished, ['f1']);
    activity.hide('f1');
    agent.push([
      file('f1', {'mac': 'skipped', 'win': 'error'}),
    ]);
    expect(activity.finished, isEmpty);
    activity.dispose();
  });

  testWidgets('reply toast names the student machine and opens the inbox', (
    tester,
  ) async {
    final agent = FakeAgent()..push([]);
    final activity = ToolActivity(agent);
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => Scaffold(
            body: Align(
              alignment: Alignment.bottomRight,
              child: ClassroomToolsOverlay(
                agent: agent,
                activity: activity,
                onOpenReplies: () => showRepliesDialog(context, activity),
              ),
            ),
          ),
        ),
      ),
    );
    agent.push([
      message('q1', {'mac': 'done', 'win': 'need help'}),
    ]);
    await tester.pump();
    expect(find.text('2 phản hồi mới'), findsOneWidget);
    expect(find.textContaining('WIN11 (192.168.50.94)'), findsOneWidget);
    await tester.tap(find.text('Xem tất cả phản hồi'));
    await tester.pumpAndSettle();
    expect(find.text('Phản hồi từ học viên (2)'), findsOneWidget);
    expect(find.text('MacStudio · 192.168.50.2'), findsOneWidget);
    expect(activity.unread, 0);
    activity.dispose();
  });

  Map<String, dynamic> policyJob(String id) => {
    'id': id,
    'kind': 'app_policy',
    'name': 'app_block',
    'size': 0,
    'text': '',
    'targets': [
      {
        'id': 'win',
        'status': 'error',
        'done': 0,
        'reply': '',
        'error': r'System folders cannot be blocked: C:\Windows\System32',
      },
    ],
  };

  Future<(FakeAgent, ToolActivity)> mount(WidgetTester tester) async {
    final agent = FakeAgent()..push([]);
    final activity = ToolActivity(agent);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Align(
            alignment: Alignment.bottomRight,
            child: ClassroomToolsOverlay(agent: agent, activity: activity),
          ),
        ),
      ),
    );
    return (agent, activity);
  }

  testWidgets('identical failures become one named card that X dismisses', (
    tester,
  ) async {
    final (agent, activity) = await mount(tester);
    agent.push([policyJob('a'), policyJob('b'), policyJob('c')]);
    await tester.pump();
    // One card for three identical failures, named for what failed.
    expect(
      find.textContaining('Không áp dụng được chặn ứng dụng'),
      findsOneWidget,
    );
    expect(find.textContaining('×3'), findsOneWidget);
    expect(
      find.textContaining('Không thể chặn thư mục hệ thống Windows'),
      findsOneWidget,
    );
    await tester.tap(find.byIcon(Icons.close));
    await tester.pump();
    // Nothing identical takes its place.
    expect(find.byIcon(Icons.close), findsNothing);
    // And it stays gone as the state keeps arriving.
    agent.push([policyJob('a'), policyJob('b'), policyJob('c')]);
    await tester.pump();
    expect(find.byIcon(Icons.close), findsNothing);
    activity.dispose();
  });

  testWidgets('a failure that needs no action hides itself after 10 seconds', (
    tester,
  ) async {
    final (agent, activity) = await mount(tester);
    agent.push([policyJob('a')]);
    await tester.pump();
    expect(find.byIcon(Icons.close), findsOneWidget);
    await tester.pump(const Duration(seconds: 9));
    expect(find.byIcon(Icons.close), findsOneWidget);
    await tester.pump(const Duration(seconds: 2));
    expect(find.byIcon(Icons.close), findsNothing);
    activity.dispose();
  });

  testWidgets(
    'a failed file transfer stays until dismissed (it can be retried)',
    (tester) async {
      final (agent, activity) = await mount(tester);
      agent.push([
        file('f', {'win': 'error'}),
      ]);
      await tester.pump();
      await tester.pump(const Duration(seconds: 30));
      expect(find.textContaining('Thử lại'), findsOneWidget);
      activity.dispose();
    },
  );

  testWidgets('a single reply names the computer once, in the title', (
    tester,
  ) async {
    final agent = FakeAgent()..push([]);
    final activity = ToolActivity(agent);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Align(
            alignment: Alignment.bottomRight,
            child: ClassroomToolsOverlay(agent: agent, activity: activity),
          ),
        ),
      ),
    );
    agent.push([
      message('q1', {'mac': 'okie teacheeer'}),
    ]);
    await tester.pump();
    expect(find.text('Phản hồi từ MacStudio (192.168.50.2)'), findsOneWidget);
    expect(find.text('okie teacheeer'), findsOneWidget);
    expect(find.textContaining('MacStudio (192.168.50.2):'), findsNothing);
    activity.dispose();
  });
}

class AlertAgent extends Agent {
  AlertAgent() : super([]);
  @override
  bool get connected => true;
  void push({List<String> ready = const [], List<Map>? alerts}) {
    ingestState({
      'teacher_ip': '192.168.50.206',
      'control_port': 47800,
      'students': [
        {'id': 'win', 'name': 'WIN11', 'ip': '192.168.50.94', 'online': true},
      ],
      'tools': {
        'ready': ready,
        'jobs': const [],
        'activity_monitoring': true,
        'new_alerts': ?alerts,
      },
    });
    notifyListeners();
  }
}

void disconnectTests() {
  test('a Student that comes back updates its own entry, not a second one', () {
    final agent = AlertAgent();
    final activity = ToolActivity(agent);
    agent.push(ready: ['win']);
    // The Student drops: one entry, still disconnected.
    agent.push(
      alerts: [
        {'id': 'win-1', 'student': 'win', 'unix_ms': 1000},
      ],
    );
    expect(activity.disconnects, hasLength(1));
    expect(activity.disconnects.single['reconnected_ms'], isNull);
    // It comes back (the tools connection is ready again).
    agent.push(ready: ['win']);
    expect(activity.disconnects, hasLength(1));
    expect(activity.disconnects.single['reconnected_ms'], isNotNull);
    // A later, separate drop is a new incident.
    agent.push(
      alerts: [
        {'id': 'win-2', 'student': 'win', 'unix_ms': 9000},
      ],
    );
    expect(activity.disconnects, hasLength(2));
    expect(activity.disconnects.first['reconnected_ms'], isNull);
    activity.dispose();
    agent.dispose();
  });

  test('time away reads naturally', () {
    expect(disconnectGap(400), 'dưới 1 giây');
    expect(disconnectGap(5000), '5 giây');
    expect(disconnectGap(125000), '2 phút 5 giây');
    expect(disconnectGap(120000), '2 phút');
    expect(disconnectGap(3720000), '1 giờ 2 phút');
  });

  testWidgets('the dialog tells lost from lost-and-back at a glance', (
    tester,
  ) async {
    final agent = AlertAgent();
    final activity = ToolActivity(agent);
    agent.push(ready: ['win']);
    agent.push(
      alerts: [
        {
          'id': 'win-1',
          'student': 'win',
          'unix_ms': DateTime(2026, 10, 16, 10, 0, 0).millisecondsSinceEpoch,
        },
      ],
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () => showDisconnectsDialog(context, activity),
            child: const Text('Open'),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    expect(find.text('Đang mất kết nối'), findsOneWidget);
    expect(find.textContaining('Chưa kết nối lại'), findsOneWidget);
    expect(find.text('Đã kết nối lại'), findsNothing);
    expect(find.byTooltip('Đang mất kết nối'), findsOneWidget);

    agent.push(ready: ['win']);
    await tester.pumpAndSettle();
    expect(find.text('Đã kết nối lại'), findsOneWidget);
    expect(find.text('Đang mất kết nối'), findsNothing);
    expect(find.textContaining('Kết nối lại lúc'), findsOneWidget);
    expect(find.textContaining('Mất kết nối lúc 10:00'), findsOneWidget);
    expect(find.byTooltip('Đã mất kết nối và đã kết nối lại'), findsOneWidget);
    expect(find.byType(ListTile), findsOneWidget);
    activity.dispose();
    agent.dispose();
  });
}
