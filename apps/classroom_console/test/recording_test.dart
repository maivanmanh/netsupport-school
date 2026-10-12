import 'package:flutter/material.dart';
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
      await tester.tap(find.text('Stop'));
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
}
