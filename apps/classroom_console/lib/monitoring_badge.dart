import 'icon_tints.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';

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
      return AppDialog(
        icon: Icons.badge_outlined,
        tone: AppTone.danger,
        title: 'Huy hiệu “Đang giám sát”',
        subtitle: '$acknowledged/${widget.ids.length} máy đã xác nhận',
        width: 540,
        actions: [
          AppDialog.primary(
            onPressed: !agent.connected ? null : () => apply(targetEnabled),
            icon: targetEnabled ? Icons.badge_outlined : Icons.badge,
            busy: busy,
            label: badgeActionLabel(agent, widget.ids),
          ),
        ],
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text(
              'Hiển thị huy hiệu nhắc nhở trên màn hình học viên. Huy hiệu không có nghĩa là đang ghi hình hay ghi phím.',
              style: TextStyle(fontSize: 13, height: 1.45),
            ),
            const SizedBox(height: 8),
            const Text(
              'Mất kết nối không tắt huy hiệu. Khi đang chiếu màn hình, huy hiệu tạm ẩn rồi tự hiện lại; chỉ lệnh tắt từ giảng viên mới gỡ hẳn.',
              style: TextStyle(
                fontSize: 12.5,
                height: 1.45,
                color: AppDialog.muted,
              ),
            ),
            if (error != null) ...[
              const SizedBox(height: 12),
              InlineNotice(error!),
            ],
            const SizedBox(height: 14),
            DialogPanel(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 280),
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
                      leading: TIcon(
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
            ),
          ],
        ),
      );
    },
  );
}
