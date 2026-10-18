import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/media_health.dart';

void main() {
  test('connection alone never becomes receiving image', () {
    expect(mediaHealthLabel({'online': true}), '');
    expect(
      mediaHealthLabel({
        'media': {'state': 'waiting_for_report'},
      }),
      contains('Waiting'),
    );
    expect(
      mediaNeedsAttention({
        'media': {'state': 'recovering'},
      }),
      isTrue,
    );
    expect(
      mediaNeedsAttention({
        'media': {'state': 'receiving'},
      }),
      isFalse,
    );
    expect(
      mediaHealthLabel({
        'media': {'state': 'receiving', 'render_fps': 27.25},
      }),
      contains('27.3 FPS'),
    );
    expect(
      mediaHealthLabel({
        'media': {'state': 'presenting_source'},
      }),
      'Presenting to class',
    );
  });

  test('receive speed is shown in bytes per second, not bits', () {
    // 24 Mbit/s is 3 MB/s (megabytes), not "24 MB/s".
    expect(formatByteRate(3 * 1024 * 1024), '3.0 MB/s');
    expect(formatByteRate(24e6 / 8), '2.9 MB/s');
    expect(formatByteRate(12.5 * 1024 * 1024), '13 MB/s');
    expect(formatByteRate(512 * 1024), '512 KB/s');
    expect(formatByteRate(100), '0 KB/s');
    expect(formatByteRate(-1), '');
    final row = {
      'media': {'state': 'receiving', 'render_fps': 23.6, 'receive_mbps': 24.0},
    };
    expect(mediaReceiveRate(row), '2.9 MB/s');
    expect(mediaHealthLabel(row), contains('23.6 FPS · 2.9 MB/s'));
    expect(mediaNumbersLabel(row), '24 FPS · 2.9 MB/s');
    // Not receiving, or an older Student without the figure: nothing invented.
    expect(
      mediaReceiveRate({
        'media': {'state': 'recovering', 'receive_mbps': 24.0},
      }),
      '',
    );
    expect(
      mediaNumbersLabel({
        'media': {'state': 'receiving', 'render_fps': 20},
      }),
      '20 FPS',
    );
    expect(mediaNumbersLabel({'online': true}), '');
  });
}
