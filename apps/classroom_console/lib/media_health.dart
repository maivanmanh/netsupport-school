String mediaHealthLabel(Map<String, dynamic> row) {
  final media = row['media'] as Map?;
  return switch (media?['state']) {
    'presenting_source' => 'Presenting to class',
    'waiting_for_report' => 'Waiting for receiver report',
    'waiting_for_image' => 'Waiting for first image',
    'recovering' => 'Recovering image',
    'decode_stalled' => 'Image decoding stalled',
    'render_stalled' => 'Image display stalled',
    'report_stale' => 'Receiver report outdated',
    'unknown' => 'Receiver health unavailable — update Student',
    'receiving_headless' => 'Receiving without a display',
    'receiving' =>
      'Receiving image · ${(media?['render_fps'] as num? ?? 0).toStringAsFixed(1)} FPS${mediaReceiveRate(row).isEmpty ? '' : ' · ${mediaReceiveRate(row)}'}',
    _ => '',
  };
}

bool mediaNeedsAttention(Map<String, dynamic> row) => const {
  'recovering',
  'decode_stalled',
  'render_stalled',
  'report_stale',
  'unknown',
  'waiting_for_report',
  'waiting_for_image',
  'receiving_headless',
}.contains((row['media'] as Map?)?['state']);

/// Bytes per second as the teacher reads it: `3.2 MB/s`, `512 KB/s`.
/// (Bytes, not bits: the Student reports megabits per second.)
String formatByteRate(double bytesPerSecond) {
  if (bytesPerSecond.isNaN || bytesPerSecond < 0) return '';
  const kb = 1024.0;
  const mb = kb * 1024;
  if (bytesPerSecond >= mb) {
    final value = bytesPerSecond / mb;
    return '${value.toStringAsFixed(value >= 10 ? 0 : 1)} MB/s';
  }
  return '${(bytesPerSecond / kb).round()} KB/s';
}

/// What a Student is receiving right now, or '' when it is not receiving the
/// presentation (or an older Student does not report it).
String mediaReceiveRate(Map<String, dynamic> row) {
  final media = row['media'] as Map?;
  if (media?['state'] != 'receiving' &&
      media?['state'] != 'receiving_headless') {
    return '';
  }
  final mbps = media?['receive_mbps'];
  return mbps is num ? formatByteRate(mbps * 1000000 / 8) : '';
}

/// Numbers only, for the small badge on a grid tile: `24 FPS · 3.2 MB/s`.
String mediaNumbersLabel(Map<String, dynamic> row) {
  final media = row['media'] as Map?;
  if (media?['state'] != 'receiving') return '';
  final fps = (media?['render_fps'] as num? ?? 0).round();
  final rate = mediaReceiveRate(row);
  return rate.isEmpty ? '$fps FPS' : '$fps FPS · $rate';
}
