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
    'selected Start, session Stop and closing dialog have distinct effects',
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
              onPressed: () => showRecordings(context, agent, ['a', 'b']),
              child: const Text('Open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Ghi 2 máy đã chọn'));
      await tester.pumpAndSettle();
      expect(agent.calls.first, {
        'action': 'recording_start',
        'selected': ['a', 'b'],
        'input': null,
      });
      await tester.tap(find.text('Dừng ghi'));
      await tester.pumpAndSettle();
      expect(agent.calls.last['action'], 'recording_stop');
      expect(agent.calls.last['input'], {'id': 's1'});
      await tester.tap(find.text('Đóng'));
      await tester.pumpAndSettle();
      expect(agent.calls.length, 2);
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
        'tools': {
          'recordings': [session, old],
        },
      };
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
      expect(find.text('Đang ghi / chờ tải (0)'), findsOneWidget);
      await tester.tap(find.text('Lịch sử (2)'));
      await tester.pumpAndSettle();
      expect(find.text('PC Alpha'), findsOneWidget);
      expect(find.text('Đã hoàn tất'), findsOneWidget);
      expect(find.text('Capture failed'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.binding.setSurfaceSize(const Size(740, 520));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.tap(find.text('Đóng'));
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
    await tester.tap(find.byTooltip('Select computers for presentation'));
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
    expect(find.text('Ghi mới: PC B'), findsOneWidget);
    await tester.tap(find.text('Đóng'));
    await tester.pumpAndSettle();
    expect(agent.calls.where((c) => c['action'] == 'recording_stop'), isEmpty);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
    agent.dispose();
  });
}
