import 'package:flutter/material.dart';

/// Session values only. Missing/stale timing is never presented as zero delay.
class ObserveStatsData {
  ObserveStatsData({
    required this.mode,
    required this.live,
    required Map<String, dynamic> native,
    required Map<String, dynamic> metrics,
    required this.fps,
    DateTime? now,
  }) {
    final width = native['width'] as num? ?? 0;
    final height = native['height'] as num? ?? 0;
    resolution = live && width > 0 && height > 0
        ? '${width.toInt()} × ${height.toInt()}'
        : '—';
    final sampled = metrics['sampled_unix_us'] as num?;
    final age = sampled == null
        ? null
        : (now ?? DateTime.now()).microsecondsSinceEpoch - sampled.toInt();
    final fresh = live && age != null && age >= 0 && age <= 3000000;
    bitrate = fresh ? valid(metrics['receive_mbps']) : null;
    delay = fresh ? valid(metrics['capture_to_surface_ms_p50']) : null;
    uncertainty = fresh ? valid(metrics['clock_uncertainty_ms']) : null;
    codec = live ? codecLabel(metrics) : null;
  }

  /// Negotiated session codec, e.g. "H.265 · 60 fps max". Fixed for the
  /// session, so it is shown even between metric samples.
  static String? codecLabel(Map<String, dynamic> metrics) {
    final name = metrics['codec'];
    if (name is! String || name.isEmpty) return null;
    final cap = metrics['fps_cap'];
    return cap is num && cap > 0 ? '$name · ${cap.toInt()} fps max' : name;
  }

  final String mode;
  final bool live;
  final double? fps;
  late final String resolution;
  late final double? bitrate, delay, uncertainty;
  late final String? codec;

  static double? valid(dynamic value) {
    if (value is! num || !value.isFinite || value < 0) return null;
    return value.toDouble();
  }
}

/// Pointer-transparent so metrics never steal clicks or scrolling from Control.
class ObserveStatsPanel extends StatelessWidget {
  const ObserveStatsPanel({super.key, required this.data});
  final ObserveStatsData data;

  @override
  Widget build(BuildContext context) {
    Widget row(String label, String value, String help) => Tooltip(
      message: help,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Row(
          children: [
            Expanded(
              child: Text(
                label,
                style: const TextStyle(color: Color(0xffbacbd7)),
              ),
            ),
            Flexible(
              child: Text(
                value,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.right,
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
            ),
          ],
        ),
      ),
    );
    final fps = data.live ? ObserveStatsData.valid(data.fps) : null;
    return IgnorePointer(
      child: Container(
        width: 236,
        padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 10),
        decoration: BoxDecoration(
          color: const Color(0xd9192936),
          borderRadius: BorderRadius.circular(12),
          border: Border.all(color: const Color(0x507b9aa9)),
          boxShadow: const [
            BoxShadow(
              color: Color(0x30000000),
              blurRadius: 12,
              offset: Offset(0, 3),
            ),
          ],
        ),
        child: DefaultTextStyle(
          style: DefaultTextStyle.of(context).style.copyWith(
            color: const Color(0xfff2f7fa),
            fontSize: 11,
            fontFeatures: const [FontFeature.tabularFigures()],
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  Icon(
                    Icons.speed,
                    size: 14,
                    color: data.live ? const Color(0xff6ed9b4) : Colors.white54,
                  ),
                  const SizedBox(width: 6),
                  const Text(
                    'SESSION STATS',
                    style: TextStyle(
                      fontSize: 10,
                      letterSpacing: 1,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ],
              ),
              const Divider(color: Color(0x307b9aa9), height: 16),
              row(
                'Mode',
                data.mode == 'observe'
                    ? 'Observe'
                    : data.mode == 'exclusive'
                    ? 'Control · Locked'
                    : 'Control',
                'Acknowledged session input mode',
              ),
              row(
                'Codec',
                data.codec ?? '—',
                'Video codec and frame-rate ceiling negotiated for this session from both machines\' hardware',
              ),
              row('Resolution', data.resolution, 'Decoded video resolution'),
              row(
                'FPS',
                fps == null ? '—' : fps.toStringAsFixed(1),
                'New frames copied to the preview texture per second; not monitor refresh rate',
              ),
              row(
                'Bitrate',
                data.bitrate == null
                    ? '—'
                    : '${data.bitrate!.toStringAsFixed(2)} Mbps',
                'Actual received media bitrate over the last interval',
              ),
              row(
                'Delay ≈',
                data.delay == null
                    ? '—'
                    : '${data.delay!.toStringAsFixed(0)} ms',
                'Estimated capture to receiver surface, median over the last interval. Excludes Flutter/compositor/display latency.',
              ),
              Align(
                alignment: Alignment.centerRight,
                child: Text(
                  data.delay != null && data.uncertainty != null
                      ? 'Capture → surface · ±${data.uncertainty!.toStringAsFixed(1)} ms'
                      : 'Waiting for timing samples',
                  style: const TextStyle(fontSize: 9, color: Color(0xff91a9b9)),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
