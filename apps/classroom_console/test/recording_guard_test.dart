import 'package:classroom_console/recording.dart';
import 'package:flutter_test/flutter_test.dart';

const gib = 1024 * 1024 * 1024;

void main() {
  final now = DateTime(2026, 10, 16, 10, 0);
  int at(Duration d) => now.add(d).millisecondsSinceEpoch ~/ 1000;

  test('every automatic stop reason has a teacher-facing explanation', () {
    for (final reason in const [
      'time_limit',
      'teacher_disk_low',
      'student_disk_low',
      'spool_full',
      'teacher_unreachable',
    ]) {
      expect(recordingStopNotice(reason), isNotNull, reason: reason);
    }
    expect(recordingStopNotice(null), isNull);
    expect(recordingStopNotice(''), isNull);
    expect(recordingStopNotice('manual'), isNull);
  });

  test(
    'the stop time is shown, and becomes a warning in the last 10 minutes',
    () {
      final far = recordingSessionNotices({
        'deadline': at(const Duration(hours: 2)),
        'targets': const [],
      }, now: now);
      expect(far.single.warn, isFalse);
      expect(far.single.text, contains('12:00'));

      final near = recordingSessionNotices({
        'deadline': at(const Duration(minutes: 5)),
        'targets': const [],
      }, now: now);
      expect(near.single.warn, isTrue);
      expect(near.single.text, contains('còn 5 phút'));

      // A session already being stopped no longer promises a stop time.
      expect(
        recordingSessionNotices({
          'deadline': at(const Duration(hours: 2)),
          'stop': true,
          'targets': const [],
        }, now: now),
        isEmpty,
      );
    },
  );

  test('low disks warn on the Teacher and on the recording Student', () {
    final notices = recordingSessionNotices({
      'teacher_free_bytes': 3 * gib,
      'targets': [
        {
          'name': 'PC-07',
          'report': {
            'phase': 'recording',
            'disk_warning': true,
            'free_bytes': 1.5 * gib,
          },
        },
        {
          'name': 'PC-08',
          'report': {
            'phase': 'recording',
            'disk_warning': false,
            'free_bytes': 40 * gib,
          },
        },
      ],
    }, now: now);
    expect(notices.where((n) => n.warn), hasLength(2));
    expect(notices.any((n) => n.text.contains('3.0 GB')), isTrue);
    expect(notices.any((n) => n.text.contains('PC-07')), isTrue);
    expect(notices.any((n) => n.text.contains('PC-08')), isFalse);

    expect(
      recordingSessionNotices({
        'teacher_free_bytes': 100 * gib,
        'targets': const [],
      }, now: now),
      isEmpty,
    );
  });
}
