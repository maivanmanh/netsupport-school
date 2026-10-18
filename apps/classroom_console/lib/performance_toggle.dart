import 'icon_tints.dart';
import 'package:flutter/material.dart';

import 'agent.dart';

/// Tooltip text for each mode: what it does and when to use it.
const performanceModes = <(String, IconData, String, String)>[
  (
    'normal',
    Icons.speed,
    'Hiệu năng cao',
    'Chất lượng đầy đủ theo cài đặt (mặc định). Dùng khi mạng dây tốt và máy học viên đủ mạnh.',
  ),
  (
    'economy',
    Icons.balance,
    'Tiết kiệm',
    'Tối đa 1280 × 720, 20 fps, khoảng 3 Mbps; ảnh thu nhỏ thưa và nhẹ hơn. Dùng khi mạng/Wi-Fi chập chờn hoặc có vài máy học viên giật.',
  ),
  (
    'minimal',
    Icons.battery_saver,
    'Rất tiết kiệm',
    'Tối đa 960 × 540, 12 fps, khoảng 1,2 Mbps. Dùng khi mạng rất yếu hoặc máy học viên rất cũ; chữ nhỏ có thể khó đọc.',
  ),
];

/// Three icon buttons at the bottom of the main window; switching is immediate.
class PerformanceToggle extends StatelessWidget {
  const PerformanceToggle({super.key, required this.agent});
  final Agent agent;

  String get current {
    final p = agent.state['performance'] as Map?;
    // What is in force (automatic Wi-Fi saving counts), else what was chosen.
    return (p?['active'] ?? p?['requested'] ?? 'normal') as String;
  }

  @override
  Widget build(BuildContext context) {
    final mode = current;
    return ToggleButtons(
      borderRadius: BorderRadius.circular(6),
      constraints: const BoxConstraints(minWidth: 28, minHeight: 24),
      isSelected: [for (final m in performanceModes) m.$1 == mode],
      onPressed: agent.connected
          ? (i) => agent.command('set_performance', const [], {
              'mode': performanceModes[i].$1,
            })
          : null,
      children: [
        for (final m in performanceModes)
          Tooltip(
            key: ValueKey('performance-${m.$1}'),
            richMessage: TextSpan(
              children: [
                TextSpan(
                  text: '${m.$3}\n',
                  style: const TextStyle(fontWeight: FontWeight.w700),
                ),
                TextSpan(text: m.$4),
              ],
            ),
            constraints: const BoxConstraints(maxWidth: 280),
            waitDuration: const Duration(milliseconds: 300),
            child: TIcon(m.$2, size: 15),
          ),
      ],
    );
  }
}
