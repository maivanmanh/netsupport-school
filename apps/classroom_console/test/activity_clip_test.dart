import 'package:classroom_console/activity_window.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('clearing drops older closed items and restarts open durations', () {
    final now = DateTime.now().millisecondsSinceEpoch;
    final cutoff = now - 60000;
    final report = <String, dynamic>{
      'monitoring_enabled': true,
      'apps': [
        {
          'app': 'old.exe',
          'open': false,
          'closed_ms': cutoff - 5,
          'opened_ms': 1,
          'total_open_ms': 9000,
        },
        {
          'app': 'recent.exe',
          'open': false,
          'closed_ms': cutoff + 10000,
          'opened_ms': 1,
          'total_open_ms': 900000,
        },
        {
          'app': 'live.exe',
          'open': true,
          'opened_ms': cutoff - 100000,
          'closed_ms': 0,
          'total_open_ms': 100000,
          'reopened': 2,
        },
      ],
      'tabs': <Map>[],
      'downloads': [
        {'name': 'a.zip', 'unix_ms': cutoff - 1},
        {'name': 'b.zip', 'unix_ms': cutoff + 1},
      ],
    };
    final out = clipActivityReport(report, cutoff);
    final apps = (out['apps'] as List).cast<Map>();
    expect(apps.map((a) => a['app']), ['recent.exe', 'live.exe']);
    expect(apps[0]['total_open_ms'], 10000);
    expect(apps[1]['reopened'], 0);
    expect((apps[1]['total_open_ms'] as int), inInclusiveRange(59000, 62000));
    expect((out['downloads'] as List).single['name'], 'b.zip');
    expect(clipActivityReport({'monitoring_enabled': false}, cutoff), {
      'monitoring_enabled': false,
    });
  });
}
