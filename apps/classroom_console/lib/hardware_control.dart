import 'icon_tints.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'window_shell.dart';

const hardwareGroups = <String, (String, String, IconData)>{
  'usb_storage': (
    'USB Storage',
    'Chặn đọc/ghi thiết bị lưu trữ ngoài; giữ chuột và bàn phím USB.',
    Icons.usb,
  ),
  'wifi': (
    'Wi-Fi',
    'Tắt adapter Wi-Fi. Từ chối nếu đang dùng để kết nối Teacher.',
    Icons.wifi,
  ),
  'bluetooth': (
    'Bluetooth',
    'Tắt radio Bluetooth; từ chối nếu có thiết bị nhập thiết yếu.',
    Icons.bluetooth,
  ),
  'usb_tethering': (
    'USB Tethering',
    'Chặn adapter điện thoại RNDIS / CDC / Apple đã nhận diện.',
    Icons.phonelink_ring,
  ),
  'camera_microphone': (
    'Camera / Microphone',
    'Tắt camera và endpoint thu âm; giữ thiết bị phát âm thanh.',
    Icons.videocam_outlined,
  ),
};

Map hardwareState(Agent agent) =>
    (agent.state['tools'] as Map? ?? const {})['hardware'] as Map? ?? const {};

Map hardwareResult(
  Map hardware,
  Map capabilities,
  String id,
  String group,
  bool online,
) {
  if (!online) {
    return {'status': 'pending', 'error': 'Offline • chờ kết nối lại'};
  }
  if ((capabilities[id] as Map? ?? const {})['hardware'] != 1) {
    return {
      'status': 'unsupported',
      'error': 'Cần Windows Student mới chạy Administrator và Guard service',
    };
  }
  final report =
      ((hardware['reports'] as Map? ?? const {})[id] as Map? ?? const {});
  return (report['groups'] as Map? ?? const {})[group] as Map? ??
      {'status': 'pending', 'error': 'Chờ Student xác nhận chính sách'};
}

class HardwareControlWindow extends StatefulWidget {
  const HardwareControlWindow({super.key, required this.agent});
  final Agent agent;
  @override
  State<HardwareControlWindow> createState() => _HardwareControlWindowState();
}

class _HardwareControlWindowState extends State<HardwareControlWindow> {
  bool busy = false;
  String? error;
  Future<void> apply(String group, bool blocked, {bool end = false}) async {
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final result = await widget.agent.request(
        end ? 'hardware_session_end' : 'hardware_policy',
        input: {'group': group, 'blocked': blocked},
      );
      if (result['ok'] != true) throw StateError(Agent.failure(result));
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  static const _statusText = {
    'applied': 'Đã áp dụng',
    'failed': 'Lỗi',
    'pending': 'Đang chờ',
    'unsupported': 'Không hỗ trợ',
  };
  static const _statusColor = {
    'applied': Color(0xff1f7a4d),
    'failed': Color(0xffb83b40),
    'pending': Color(0xffc77d12),
    'unsupported': Color(0xff64748b),
  };
  static const _blockedColor = Color(0xffb83b40);

  Widget _section(String title) => Padding(
    padding: const EdgeInsets.fromLTRB(4, 6, 0, 10),
    child: Text(
      title.toUpperCase(),
      style: const TextStyle(
        fontSize: 11.5,
        fontWeight: FontWeight.w700,
        letterSpacing: 0.8,
        color: AppDialog.muted,
      ),
    ),
  );

  Widget _stat(String value, String label, Color color) => Expanded(
    child: Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: AppDialog.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            value,
            style: TextStyle(
              fontSize: 24,
              fontWeight: FontWeight.w700,
              color: color,
              height: 1.1,
            ),
          ),
          const SizedBox(height: 4),
          Text(
            label,
            style: const TextStyle(fontSize: 12.5, color: AppDialog.muted),
          ),
        ],
      ),
    ),
  );

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.agent,
    builder: (context, _) {
      final agent = widget.agent;
      final hardware = hardwareState(agent);
      final policy = hardware['policy'] as Map? ?? const {};
      final caps =
          (agent.state['tools'] as Map? ?? const {})['capabilities'] as Map? ??
          const {};
      final rows = {for (final r in agent.students) '${r['id']}': r};
      final ids = {
        ...rows.keys,
        ...(hardware['reports'] as Map? ?? const {}).keys.cast<String>(),
      }.toList()..sort();
      final enabled = agent.connected && !busy;
      Map resultOf(String id, String group) => hardwareResult(
        hardware,
        caps,
        id,
        group,
        rows[id]?['online'] == true,
      );
      final blockedCount = hardwareGroups.keys
          .where((g) => policy[g] == true)
          .length;
      final failedCount = {
        for (final g in hardwareGroups.keys)
          for (final id in ids)
            if (policy[g] == true && resultOf(id, g)['status'] == 'failed')
              '$g|$id',
      }.length;
      return WindowShell(
        icon: Icons.usb_outlined,
        title: 'Hardware Control',
        subtitle: 'Toàn bộ phòng máy • ${ids.length} máy',
        actions: [
          StatusChip(
            blockedCount == 0
                ? 'Không chặn gì'
                : '$blockedCount nhóm đang chặn',
            blockedCount == 0 ? AppDialog.muted : _blockedColor,
          ),
        ],
        notice: !agent.connected
            ? const InlineNotice(
                'Mất kết nối coordinator; chưa thể thay đổi chính sách.',
              )
            : error != null
            ? InlineNotice(error!)
            : null,
        footer: busy ? const LinearProgressIndicator() : null,
        // The scroll view spans the whole window so its scrollbar sits on the
        // right edge; the content column is centred through padding instead.
        child: LayoutBuilder(
          builder: (context, box) {
            final side = ((box.maxWidth - 900) / 2).clamp(0.0, 1e9) + 24;
            return ListView(
              padding: EdgeInsets.fromLTRB(side, 20, side, 28),
              children: [
                Row(
                  children: [
                    _stat('${ids.length}', 'Máy được quản lý', AppDialog.ink),
                    const SizedBox(width: 12),
                    _stat(
                      '$blockedCount/${hardwareGroups.length}',
                      'Nhóm đang chặn',
                      blockedCount == 0 ? AppDialog.ink : _blockedColor,
                    ),
                    const SizedBox(width: 12),
                    _stat(
                      '$failedCount',
                      'Máy gặp lỗi',
                      failedCount == 0
                          ? const Color(0xff1f7a4d)
                          : _blockedColor,
                    ),
                  ],
                ),
                const SizedBox(height: 14),
                const InlineNotice(
                  'Mỗi thay đổi áp dụng cho tất cả máy đang quản lý và máy tham gia sau. Chính sách được giữ qua mất kết nối, Teacher thoát, Student restart và Windows restart. Chỉ bỏ chặn hoặc kết thúc phiên phần cứng rõ ràng mới khôi phục.',
                  tone: AppTone.info,
                ),
                if (hardware['policy'] == null) ...[
                  const SizedBox(height: 10),
                  const InlineNotice(
                    'Teacher chưa lưu chính sách. Trạng thái từng máy có thể là chính sách cũ còn giữ. Lần áp dụng đầu sẽ gửi cả 5 công tắc bên dưới (mặc định cho phép) tới toàn bộ phòng máy.',
                    tone: AppTone.warning,
                  ),
                ],
                const SizedBox(height: 18),
                _section('Chính sách thiết bị'),
                for (final entry in hardwareGroups.entries)
                  _groupCard(entry, policy, ids, rows, resultOf, enabled),
                const SizedBox(height: 6),
                _section('Phiên làm việc'),
                _sessionCard(policy, enabled),
                const SizedBox(height: 16),
                const Text(
                  'Windows 10/11 • Student cần Administrator và Classroom Guard service. Thiết bị mới được rà soát định kỳ; có khoảng trễ khi hot-plug. “Đã áp dụng” là kết quả kiểm tra cấu hình hệ điều hành, không phải chứng nhận đã thử phần cứng thực tế.',
                  style: TextStyle(
                    fontSize: 12,
                    height: 1.45,
                    color: AppDialog.muted,
                  ),
                ),
              ],
            );
          },
        ),
      );
    },
  );

  Widget _groupCard(
    MapEntry<String, (String, String, IconData)> entry,
    Map policy,
    List<String> ids,
    Map<String, dynamic> rows,
    Map Function(String, String) resultOf,
    bool enabled,
  ) {
    final blocked = policy[entry.key] == true;
    final accent = blocked ? _blockedColor : const Color(0xff156b79);
    final counts = {
      for (final status in _statusText.keys)
        status: ids
            .where((id) => resultOf(id, entry.key)['status'] == status)
            .length,
    };
    return Card(
      elevation: 0,
      margin: const EdgeInsets.only(bottom: 12),
      color: Colors.white,
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(
          color: blocked
              ? _blockedColor.withValues(alpha: 0.35)
              : AppDialog.border,
        ),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 16, 14, 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  width: 44,
                  height: 44,
                  decoration: BoxDecoration(
                    color: accent.withValues(alpha: 0.1),
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: TIcon(entry.value.$3, color: accent, size: 24),
                ),
                const SizedBox(width: 14),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        entry.value.$1,
                        style: const TextStyle(
                          fontSize: 15.5,
                          fontWeight: FontWeight.w700,
                          color: AppDialog.ink,
                        ),
                      ),
                      const SizedBox(height: 2),
                      Text(
                        entry.value.$2,
                        style: const TextStyle(
                          fontSize: 12.5,
                          height: 1.4,
                          color: AppDialog.muted,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 12),
                StatusChip(blocked ? 'Đang chặn' : 'Cho phép', accent),
                const SizedBox(width: 6),
                Switch(
                  value: blocked,
                  activeThumbColor: Colors.white,
                  activeTrackColor: _blockedColor,
                  onChanged: enabled ? (v) => apply(entry.key, v) : null,
                ),
              ],
            ),
            const Divider(height: 18, color: AppDialog.border),
            if (ids.isEmpty)
              const Padding(
                padding: EdgeInsets.only(bottom: 10, left: 2),
                child: Text(
                  'Chưa có máy nào',
                  style: TextStyle(fontSize: 12.5, color: AppDialog.muted),
                ),
              )
            else
              Theme(
                data: Theme.of(
                  context,
                ).copyWith(dividerColor: Colors.transparent),
                child: ExpansionTile(
                  tilePadding: EdgeInsets.zero,
                  childrenPadding: const EdgeInsets.only(bottom: 8),
                  dense: true,
                  title: Wrap(
                    spacing: 8,
                    runSpacing: 6,
                    children: [
                      for (final status in _statusText.keys)
                        if (counts[status]! > 0)
                          StatusChip(
                            '${_statusText[status]}: ${counts[status]}',
                            _statusColor[status]!,
                          ),
                    ],
                  ),
                  subtitle: const Text(
                    'Chi tiết từng máy',
                    style: TextStyle(fontSize: 12, color: AppDialog.muted),
                  ),
                  children: [
                    for (final id in ids)
                      _deviceRow(id, entry.key, rows, resultOf),
                  ],
                ),
              ),
          ],
        ),
      ),
    );
  }

  Widget _deviceRow(
    String id,
    String group,
    Map<String, dynamic> rows,
    Map Function(String, String) resultOf,
  ) {
    final r = resultOf(id, group);
    final status = '${r['status']}';
    final color = _statusColor[status] ?? AppDialog.muted;
    final detail = '${r['error'] ?? ''}';
    return Container(
      margin: const EdgeInsets.only(top: 6),
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: AppDialog.footerFill,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 1),
            child: TIcon(
              status == 'applied'
                  ? Icons.check_circle_outline
                  : status == 'failed'
                  ? Icons.error_outline
                  : Icons.info_outline,
              color: color,
              size: 18,
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '${rows[id]?['name'] ?? id}',
                  style: const TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: AppDialog.ink,
                  ),
                ),
                if (detail.isNotEmpty)
                  SelectableText(
                    detail,
                    style: const TextStyle(
                      fontSize: 12,
                      height: 1.4,
                      color: AppDialog.muted,
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(width: 10),
          if (r['blocked'] is bool)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: Text(
                r['blocked'] == true ? 'Chặn' : 'Cho phép',
                style: const TextStyle(fontSize: 12, color: AppDialog.muted),
              ),
            ),
          StatusChip(_statusText[status] ?? status, color),
        ],
      ),
    );
  }

  Widget _sessionCard(Map policy, bool enabled) => Card(
    elevation: 0,
    margin: EdgeInsets.zero,
    color: Colors.white,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(12),
      side: const BorderSide(color: AppDialog.border),
    ),
    child: Padding(
      padding: const EdgeInsets.fromLTRB(18, 14, 14, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 44,
                height: 44,
                decoration: BoxDecoration(
                  color: const Color(0xff156b79).withValues(alpha: 0.1),
                  borderRadius: BorderRadius.circular(10),
                ),
                child: const TIcon(
                  Icons.settings_backup_restore,
                  color: Color(0xff156b79),
                  size: 24,
                ),
              ),
              const SizedBox(width: 14),
              const Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Khôi phục khi kết thúc phiên phần cứng',
                      style: TextStyle(
                        fontSize: 15.5,
                        fontWeight: FontWeight.w700,
                        color: AppDialog.ink,
                      ),
                    ),
                    SizedBox(height: 2),
                    Text(
                      'Chỉ nút “Kết thúc phiên” dưới đây kích hoạt. Mất kết nối, dừng chiếu, đóng cửa sổ và Teacher thoát không bỏ chặn. Máy offline nhận lệnh khôi phục khi kết nối lại.',
                      style: TextStyle(
                        fontSize: 12.5,
                        height: 1.4,
                        color: AppDialog.muted,
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              Switch(
                value: policy['restore_on_session_end'] == true,
                onChanged: enabled
                    ? (v) => apply('restore_on_session_end', v)
                    : null,
              ),
            ],
          ),
          const SizedBox(height: 12),
          Align(
            alignment: Alignment.centerRight,
            child: AppDialog.secondary(
              onPressed: enabled && policy['restore_on_session_end'] == true
                  ? () => apply('', false, end: true)
                  : null,
              icon: Icons.settings_backup_restore,
              label: 'Kết thúc phiên • khôi phục tất cả',
            ),
          ),
        ],
      ),
    ),
  );
}
