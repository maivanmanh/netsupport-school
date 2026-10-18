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
      const statusText = {
        'applied': 'Đã áp dụng',
        'failed': 'Lỗi',
        'pending': 'Đang chờ',
        'unsupported': 'Không hỗ trợ',
      };
      const statusColor = {
        'applied': Color(0xff1f7a4d),
        'failed': Color(0xffb83b40),
        'pending': Color(0xffc77d12),
        'unsupported': Color(0xff64748b),
      };
      return WindowShell(
        icon: Icons.usb_outlined,
        title: 'Hardware Control',
        subtitle: 'Toàn bộ phòng máy • ${ids.length} máy',
        notice: !agent.connected
            ? const InlineNotice(
                'Mất kết nối coordinator; chưa thể thay đổi chính sách.',
              )
            : error != null
            ? InlineNotice(error!)
            : null,
        footer: busy ? const LinearProgressIndicator() : null,
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 900),
            child: ListView(
              padding: const EdgeInsets.all(24),
              children: [
                const InlineNotice(
                  'Mỗi thay đổi áp dụng cho tất cả máy đang quản lý và máy tham gia sau. Chính sách được giữ qua mất kết nối, Teacher thoát, Student restart và Windows restart. Chỉ bỏ chặn hoặc kết thúc phiên phần cứng rõ ràng mới khôi phục.',
                  tone: AppTone.info,
                ),
                const SizedBox(height: 10),
                const Text(
                  'Windows 10/11 • Student cần Administrator và Classroom Guard service. Thiết bị mới được rà soát định kỳ; có khoảng trễ khi hot-plug. “Đã áp dụng” là kết quả kiểm tra cấu hình hệ điều hành, không phải chứng nhận đã thử phần cứng thực tế.',
                  style: TextStyle(
                    fontSize: 12.5,
                    height: 1.45,
                    color: AppDialog.muted,
                  ),
                ),
                const SizedBox(height: 18),
                if (hardware['policy'] == null) ...[
                  const InlineNotice(
                    'Teacher chưa lưu chính sách. Trạng thái từng máy có thể là chính sách cũ còn giữ. Lần áp dụng đầu sẽ gửi cả 5 công tắc bên dưới (mặc định cho phép) tới toàn bộ phòng máy.',
                    tone: AppTone.warning,
                  ),
                  const SizedBox(height: 14),
                ],
                for (final entry in hardwareGroups.entries) ...[
                  Card(
                    elevation: 0,
                    color: Colors.white,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(12),
                      side: const BorderSide(color: AppDialog.border),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.all(18),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Container(
                                width: 44,
                                height: 44,
                                decoration: BoxDecoration(
                                  color: const Color(
                                    0xff156b79,
                                  ).withValues(alpha: 0.1),
                                  borderRadius: BorderRadius.circular(10),
                                ),
                                child: TIcon(
                                  entry.value.$3,
                                  color: const Color(0xff156b79),
                                  size: 24,
                                ),
                              ),
                              const SizedBox(width: 14),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      entry.value.$1,
                                      style: const TextStyle(
                                        fontSize: 16,
                                        fontWeight: FontWeight.w700,
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
                              Text(
                                policy[entry.key] == true ? 'Chặn' : 'Cho phép',
                                style: TextStyle(
                                  fontWeight: FontWeight.w600,
                                  color: policy[entry.key] == true
                                      ? const Color(0xffb83b40)
                                      : AppDialog.muted,
                                ),
                              ),
                              Switch(
                                value: policy[entry.key] == true,
                                onChanged: enabled
                                    ? (v) => apply(entry.key, v)
                                    : null,
                              ),
                            ],
                          ),
                          const SizedBox(height: 12),
                          Wrap(
                            spacing: 8,
                            runSpacing: 6,
                            children: [
                              for (final status in statusText.keys)
                                StatusChip(
                                  '${statusText[status]}: ${ids.where((id) => hardwareResult(hardware, caps, id, entry.key, rows[id]?['online'] == true)['status'] == status).length}',
                                  statusColor[status]!,
                                ),
                            ],
                          ),
                          if (ids.isNotEmpty)
                            ExpansionTile(
                              tilePadding: EdgeInsets.zero,
                              title: const Text(
                                'Trạng thái từng máy',
                                style: TextStyle(
                                  fontSize: 13,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              children: [
                                for (final id in ids)
                                  Builder(
                                    builder: (_) {
                                      final r = hardwareResult(
                                        hardware,
                                        caps,
                                        id,
                                        entry.key,
                                        rows[id]?['online'] == true,
                                      );
                                      final status = '${r['status']}';
                                      return ListTile(
                                        dense: true,
                                        contentPadding: EdgeInsets.zero,
                                        leading: TIcon(
                                          status == 'applied'
                                              ? Icons.check_circle_outline
                                              : status == 'failed'
                                              ? Icons.error_outline
                                              : Icons.info_outline,
                                          color:
                                              statusColor[status] ??
                                              AppDialog.muted,
                                        ),
                                        title: Text(
                                          '${rows[id]?['name'] ?? id} • ${statusText[status] ?? status}${r['blocked'] is bool ? ' • ${r['blocked'] == true ? 'Chặn' : 'Cho phép'}' : ''}',
                                        ),
                                        subtitle: r['error'] == ''
                                            ? null
                                            : SelectableText(
                                                '${r['error'] ?? ''}',
                                              ),
                                      );
                                    },
                                  ),
                              ],
                            ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 12),
                ],
                Card(
                  elevation: 0,
                  color: Colors.white,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(12),
                    side: const BorderSide(color: AppDialog.border),
                  ),
                  child: Column(
                    children: [
                      SwitchListTile(
                        title: const Text(
                          'Khôi phục khi kết thúc phiên phần cứng',
                        ),
                        subtitle: const Text(
                          'Chỉ nút “Kết thúc phiên” dưới đây kích hoạt. Mất kết nối, dừng chiếu, đóng cửa sổ và Teacher thoát không bỏ chặn. Máy offline nhận lệnh khôi phục khi kết nối lại.',
                        ),
                        value: policy['restore_on_session_end'] == true,
                        onChanged: enabled
                            ? (v) => apply('restore_on_session_end', v)
                            : null,
                      ),
                      Padding(
                        padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                        child: Align(
                          alignment: Alignment.centerRight,
                          child: AppDialog.secondary(
                            onPressed:
                                enabled &&
                                    policy['restore_on_session_end'] == true
                                ? () => apply('', false, end: true)
                                : null,
                            icon: Icons.settings_backup_restore,
                            label: 'Kết thúc phiên • khôi phục tất cả',
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      );
    },
  );
}
