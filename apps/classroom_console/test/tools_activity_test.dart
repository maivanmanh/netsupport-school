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
    expect(find.text('2 new replies'), findsOneWidget);
    expect(find.textContaining('WIN11 (192.168.50.94)'), findsOneWidget);
    await tester.tap(find.text('View all replies'));
    await tester.pumpAndSettle();
    expect(find.text('Student replies (2)'), findsOneWidget);
    expect(find.text('MacStudio · 192.168.50.2'), findsOneWidget);
    expect(activity.unread, 0);
    activity.dispose();
  });
}
