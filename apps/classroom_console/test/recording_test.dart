import 'package:classroom_console/live_activity.dart';
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:classroom_console/main.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/recording.dart';

class RecordingAgent extends Agent {
  RecordingAgent() : super(const []);
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
    calls.add({
      'action': action,
      'selected': selected.toList(),
      'input': input,
    });
    return {'ok': true};
  }
}

void main() {
  setUpAll(() => BlinkingRecordDot.animated = false);
  test(
    'offline and stale reports are never presented as fresh recording status',
    () {
      final target = {
        'online': false,
        'stale': true,
        'received': 2,
        'report': {
          'phase': 'recording',
          'elapsed_seconds': 620,
          'fps': 24.2,
          'width': 1280,
          'height': 720,
          'queued_chunks': 1,
          'upload_error': 'Disk full',
        },
      };
      expect(recordingStatus(target), contains('Offline, trạng thái gần nhất'));
      expect(recordingStatus(target), contains('10:20'));
      expect(recordingStatus(target), contains('24.2 FPS'));
      expect(recordingStatus(target), contains('Đã nhận 2 đoạn'));
      expect(recordingStatus(target), contains('Disk full'));
      target['online'] = true;
      expect(recordingStatus(target), contains('Chưa có trạng thái mới'));
    },
  );
  test(
    'Stop acknowledgement, offline capture and upload work remain in progress',
    () {
      expect(
        recordingSessionPending({
          'stop': true,
          'targets': [
            {
              'online': false,
              'report': {'phase': 'recording'},
            },
          ],
        }),
        isTrue,
      );
      expect(
        recordingSessionPending({
          'targets': [
            {
              'uploading': true,
              'report': {'phase': 'stopped'},
            },
          ],
        }),
        isTrue,
      );
      expect(
        recordingSessionPending({
          'targets': [
            {
              'received': 0,
              'report': {'phase': 'interrupted', 'completed_chunks': 1},
            },
          ],
        }),
        isTrue,
      );
      expect(
        recordingSessionPending({
          'targets': [
            {
              'received': 1,
              'report': {'phase': 'interrupted', 'completed_chunks': 1},
            },
          ],
        }),
        isFalse,
      );
    },
  );
  testWidgets(
    'the manager has no bulk Start; session Stop and closing have distinct effects',
    (tester) async {
      tester.view.physicalSize = const Size(1200, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final agent = RecordingAgent();
      agent.state = {
        'tools': {
          'recordings': [
            {
              'id': 's1',
              'folder': '/Desktop/Classroom Recordings/PC',
              'stop': false,
              'targets': [
                {
                  'id': 'a',
                  'name': 'PC',
                  'online': true,
                  'stale': false,
                  'received': 1,
                  'report': {'phase': 'recording', 'fps': 30},
                },
              ],
            },
          ],
        },
      };
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () => showRecordings(context, agent, const []),
              child: const Text('Open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      // Recording is started per computer (right-click), never in bulk from here.
      expect(find.textContaining('máy đã chọn'), findsNothing);
      expect(find.textContaining('nhấp chuột phải'), findsOneWidget);
      expect(agent.calls, isEmpty);
      // A session that is still capturing can be stopped but not deleted:
      // there is no delete button until it has moved to History.
      expect(find.byIcon(Icons.delete_outline), findsNothing);
      await tester.tap(find.text('Dừng ghi'));
      await tester.pumpAndSettle();
      expect(agent.calls.last['action'], 'recording_stop');
      expect(agent.calls.last['input'], {'id': 's1'});
      await tester.tap(find.byTooltip('Đóng'));
      await tester.pumpAndSettle();
      expect(agent.calls.length, 1);
      expect(tester.takeException(), isNull);
      agent.dispose();
    },
  );
  testWidgets(
    'terminal uploads stay pending until received, then move to history',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final agent = RecordingAgent();
      final report = <String, dynamic>{
        'phase': 'stopped',
        'completed_chunks': 2,
        'queued_chunks': 1,
      };
      final target = <String, dynamic>{
        'id': 'a',
        'name': 'PC Alpha',
        'online': false,
        'received': 1,
        'report': report,
      };
      final session = {
        'id': 'pending',
        'stop': true,
        'folder': '/Desktop/Alpha',
        'targets': [target],
      };
      final old = {
        'id': 'old',
        'folder': '/Desktop/Old',
        'targets': [
          {
            'name': 'PC History',
            'received': 1,
            'report': {
              'phase': 'error',
              'completed_chunks': 1,
              'error': 'Capture failed',
            },
          },
        ],
      };
      agent.state = {
        'students': [
          {'id': 'a', 'name': 'PC Alpha', 'ip': '10.0.55.11', 'online': false},
        ],
        'tools': {
          'recordings': [session, old],
        },
      };
      // The dialog is height-limited; use a window tall enough to build both
      // history cards (the narrow-window overflow check below shrinks it).
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1000, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () => showRecordings(context, agent, ['a']),
              child: const Text('Open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      expect(find.text('Chờ tải về'), findsOneWidget);
      // The card names the computer with its address (from the roster).
      expect(find.text('PC Alpha (10.0.55.11)'), findsOneWidget);
      expect(find.text('Offline • Trạng thái gần nhất'), findsOneWidget);
      expect(find.text('Dừng ghi'), findsNothing);
      expect(find.text('PC History'), findsNothing);
      await tester.tap(find.text('Mở thư mục'));
      await tester.pumpAndSettle();
      expect(agent.calls.last['input'], {'id': 'pending'});
      target['received'] = 2;
      report['queued_chunks'] = 0;
      agent.notifyListeners();
      await tester.pumpAndSettle();
      // While the dialog stays open the finished session does not jump to
      // History; it stays where the teacher was looking, now marked complete.
      expect(find.text('Đang ghi / chờ tải (1)'), findsOneWidget);
      expect(find.byTooltip('Đã hoàn tất'), findsOneWidget);
      expect(find.byIcon(Icons.delete_outline), findsNothing);
      // Reopening the dialog files it under History, as a status icon only.
      await tester.tap(find.byTooltip('Đóng'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      expect(find.text('Đang ghi / chờ tải (0)'), findsOneWidget);
      await tester.tap(find.text('Lịch sử (2)'));
      await tester.pumpAndSettle();
      expect(find.text('PC Alpha (10.0.55.11)'), findsOneWidget);
      expect(find.byTooltip('Đã hoàn tất'), findsOneWidget);
      expect(find.byTooltip('Kết thúc • Có lỗi khi ghi'), findsOneWidget);
      expect(find.text('Đã hoàn tất'), findsNothing);
      expect(find.text('Capture failed'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.binding.setSurfaceSize(const Size(740, 520));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.tap(find.byTooltip('Đóng'));
      await tester.pumpAndSettle();
      agent.dispose();
    },
  );

  testWidgets(
    'the first seconds are calm; only a long silence or a real fault is red',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final agent = RecordingAgent();
      final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      Map session(int created) => {
        'id': 's',
        'created': created,
        'deadline': created + 3600,
        'stop': false,
        'folder': '/Desktop/S',
        'targets': [
          {
            'id': 'a',
            'name': 'PC A',
            'online': true,
            'stale': true,
            'received': 0,
            'report': {'phase': 'pending'},
          },
        ],
      };
      agent.state = {
        'tools': {
          'recordings': [session(now - 3)],
        },
      };
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () => showRecordings(context, agent, const []),
              child: const Text('Open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      expect(find.textContaining('báo tiến độ đầu tiên'), findsOneWidget);
      expect(find.text('Chưa có trạng thái mới'), findsNothing);
      expect(find.textContaining('Chưa nhận được phản hồi'), findsNothing);
      // No delete button while capturing; the stop time is shown.
      expect(find.byIcon(Icons.delete_outline), findsNothing);
      expect(find.textContaining('Tự động dừng lúc'), findsOneWidget);

      // After a minute of silence it is reported as a problem.
      agent.state = {
        'tools': {
          'recordings': [session(now - 90)],
        },
      };
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.textContaining('Chưa nhận được phản hồi'), findsOneWidget);

      // A live recording shows its running time.
      agent.state = {
        'tools': {
          'recordings': [
            {
              ...session(now - 90),
              'targets': [
                {
                  'id': 'a',
                  'name': 'PC A',
                  'online': true,
                  'stale': false,
                  'received': 0,
                  'report': {
                    'phase': 'recording',
                    'elapsed_seconds': 83,
                    'fps': 24.0,
                    'width': 1280,
                    'height': 720,
                  },
                },
              ],
            },
          ],
        },
      };
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.textContaining('Chưa nhận được phản hồi'), findsNothing);
      expect(find.textContaining('1:23'), findsOneWidget);
      await tester.tap(find.byTooltip('Đóng'));
      await tester.pumpAndSettle();
      agent.dispose();
    },
  );

  testWidgets('right-click recording starts only the context computer', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1100, 850));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(
      const MethodChannel('window_manager'),
      (_) async => null,
    );
    messenger.setMockMethodCallHandler(
      const MethodChannel('classroom/permissions'),
      (call) async => call.method == 'status'
          ? {'screen': true, 'helperScreen': true, 'onboardingSeen': true}
          : true,
    );
    final agent = RecordingAgent()
      ..state = {
        'phase': 'idle',
        'students': [
          {'id': 'a', 'name': 'PC A', 'ip': '10.0.55.11', 'online': true},
          {'id': 'b', 'name': 'PC B', 'ip': '10.0.55.12', 'online': true},
        ],
      };
    await tester.pumpWidget(ClassroomApp(agent: agent));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Chọn nhiều máy để thao tác'));
    await tester.pumpAndSettle();
    await tester.tap(find.byType(Checkbox).first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('PC B'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Ghi màn hình'));
    await tester.pumpAndSettle();
    expect(
      agent.calls.where((c) => c['action'] == 'recording_start').toList(),
      [
        {
          'action': 'recording_start',
          'selected': ['b'],
          'input': null,
        },
      ],
    );
    expect(find.text('Đang bắt đầu ghi: PC B'), findsOneWidget);
    await tester.tap(find.byTooltip('Đóng'));
    await tester.pumpAndSettle();
    expect(agent.calls.where((c) => c['action'] == 'recording_stop'), isEmpty);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
    agent.dispose();
  });

  testWidgets(
    'a finished session can be deleted after a confirmation, in both tabs',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final agent = RecordingAgent();
      agent.state = {
        'tools': {
          'recordings': [
            {
              'id': 'waiting',
              'stop': true,
              'folder': '/Desktop/Waiting',
              'targets': [
                {
                  'id': 'a',
                  'name': 'PC Waiting',
                  'online': false,
                  'received': 1,
                  'report': {
                    'phase': 'stopped',
                    'completed_chunks': 2,
                    'queued_chunks': 1,
                  },
                },
              ],
            },
            {
              'id': 'done',
              'stop': true,
              'folder': '/Desktop/Done',
              'targets': [
                {
                  'id': 'b',
                  'name': 'PC Done',
                  'online': false,
                  'received': 3,
                  'report': {'phase': 'stopped', 'completed_chunks': 3},
                },
              ],
            },
          ],
        },
      };
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () => showRecordings(context, agent, const []),
              child: const Text('Open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      // A session that is still capturing or waiting for upload cannot be
      // deleted, so the first tab offers no delete button at all.
      expect(find.byIcon(Icons.delete_outline), findsNothing);
      // History tab.
      await tester.tap(find.text('Lịch sử (1)'));
      await tester.pumpAndSettle();
      // Cancelling the confirmation deletes nothing.
      await tester.tap(find.byIcon(Icons.delete_outline));
      await tester.pumpAndSettle();
      expect(find.textContaining('3 đoạn video MP4'), findsOneWidget);
      await tester.tap(find.text('Hủy'));
      await tester.pumpAndSettle();
      expect(agent.calls, isEmpty);
      await tester.tap(find.byIcon(Icons.delete_outline));
      await tester.pumpAndSettle();
      expect(find.textContaining('3 đoạn video MP4'), findsOneWidget);
      await tester.tap(find.text('Xóa').last);
      await tester.pumpAndSettle();
      expect(agent.calls.last['input'], {'id': 'done'});
      expect(tester.takeException(), isNull);
      agent.dispose();
    },
  );
}
