import 'package:flutter/material.dart';
import 'agent.dart';

Map badgeState(Agent agent, String id) =>
    (((agent.state['tools'] as Map?)?['monitoring_badges'] as Map?)?[id]
        as Map?) ??
    const {};
String badgeLabel(Agent agent, String id) {
  final b = badgeState(agent, id);
  final on = b['enabled'] == true;
  return switch (b['status']) {
    'pending' => on ? 'Đang bật…' : 'Đang tắt…',
    'applied' => on ? 'Đang giám sát • đã xác nhận' : 'Đã tắt • đã xác nhận',
    'offline' => '${on ? 'Đang bật' : 'Đang tắt'} • chờ máy kết nối lại',
    'unsupported' => 'Student cần cập nhật để hiển thị badge',
    'error' || 'timeout' =>
      '${on ? 'Bật' : 'Tắt'} chưa được xác nhận: ${b['error'] ?? ''}',
    _ => on ? 'Đang bật • chưa xác nhận' : 'Đã tắt',
  };
}

bool badgeAllEnabled(Agent agent, List<String> ids) =>
    ids.isNotEmpty &&
    ids.every((id) => badgeState(agent, id)['enabled'] == true);
bool badgeAnyEnabled(Agent agent, List<String> ids) =>
    ids.any((id) => badgeState(agent, id)['enabled'] == true);
String badgeActionLabel(Agent agent, List<String> ids) {
  if (badgeAllEnabled(agent, ids)) return 'Tắt giám sát';
  if (badgeAnyEnabled(agent, ids)) return 'Bật cho tất cả';
  return 'Bật giám sát';
}

Future<void> showMonitoringBadge(
  BuildContext context,
  Agent agent,
  List<String> ids,
) {
  return showDialog<void>(
    context: context,
    builder: (_) => _BadgeDialog(agent: agent, ids: ids),
  );
}

class _BadgeDialog extends StatefulWidget {
  const _BadgeDialog({required this.agent, required this.ids});
  final Agent agent;
  final List<String> ids;
  @override
  State<_BadgeDialog> createState() => _BadgeDialogState();
}

class _BadgeDialogState extends State<_BadgeDialog> {
  bool busy = false;
  String? error;
  Future<void> apply(bool enabled) async {
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final result = await widget.agent.request(
        'monitoring_badge',
        selected: widget.ids,
        input: {'enabled': enabled},
      );
      if (result['ok'] != true) throw StateError(Agent.failure(result));
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.agent,
    builder: (context, _) {
      final agent = widget.agent;
      final allEnabled = badgeAllEnabled(agent, widget.ids);
      final targetEnabled = !allEnabled;
      final acknowledged = widget.ids.where((id) {
        final b = badgeState(agent, id);
        return b['status'] == 'applied' && b['actual'] == b['enabled'];
      }).length;
      return AlertDialog(
        title: Row(
          children: [
            const Icon(Icons.badge_outlined, color: Color(0xffc91f28)),
            const SizedBox(width: 10),
            const Expanded(child: Text('Đang giám sát')),
            IconButton(
              tooltip: 'Đóng',
              onPressed: () => Navigator.pop(context),
              icon: const Icon(Icons.close),
            ),
          ],
        ),
        content: SizedBox(
          width: 480,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'Hiển thị badge nhắc nhở trên màn hình Student. Badge không có nghĩa là đang ghi hình hoặc ghi phím.',
              ),
              const SizedBox(height: 8),
              const Text(
                'Mất kết nối không tắt badge. Khi đang chiếu màn hình, badge tạm ẩn rồi tự hiện lại; chỉ lệnh tắt từ Teacher mới gỡ hẳn.',
                style: TextStyle(fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 12),
              Text('$acknowledged/${widget.ids.length} máy đã xác nhận'),
              if (error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    error!,
                    style: const TextStyle(color: Colors.red),
                  ),
                ),
              const SizedBox(height: 8),
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 300),
                child: ListView(
                  shrinkWrap: true,
                  children: widget.ids.map((id) {
                    final rows = agent.students.where(
                      (r) => '${r['id']}' == id,
                    );
                    final row = rows.isEmpty ? <String, dynamic>{} : rows.first;
                    final b = badgeState(agent, id);
                    return ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      leading: Icon(
                        b['status'] == 'applied'
                            ? Icons.check_circle
                            : b['status'] == 'pending'
                            ? Icons.hourglass_top
                            : Icons.info_outline,
                        color: b['status'] == 'applied'
                            ? Colors.green
                            : Colors.orange,
                      ),
                      title: Text('${row['name'] ?? id} • ${row['ip'] ?? ''}'),
                      subtitle: Text(badgeLabel(agent, id)),
                    );
                  }).toList(),
                ),
              ),
            ],
          ),
        ),
        actions: [
          if (busy)
            const SizedBox(
              width: 18,
              height: 18,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
          FilledButton.icon(
            onPressed: busy || !agent.connected
                ? null
                : () => apply(targetEnabled),
            icon: Icon(targetEnabled ? Icons.badge_outlined : Icons.badge),
            label: Text(badgeActionLabel(agent, widget.ids)),
          ),
        ],
      );
    },
  );
}
