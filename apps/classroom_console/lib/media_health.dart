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
      'Receiving image · ${(media?['render_fps'] as num? ?? 0).toStringAsFixed(1)} FPS',
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
}.contains((row['media'] as Map?)?['state']);
