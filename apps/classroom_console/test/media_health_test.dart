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
}
