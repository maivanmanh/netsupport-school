import 'package:flutter/material.dart';
import 'agent.dart';

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
      return Scaffold(
        backgroundColor: const Color(0xfff3f5f9),
        appBar: AppBar(
          title: const Text('Hardware Control'),
          automaticallyImplyLeading: false,
          actions: [
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: Center(
                child: Text('Toàn bộ phòng máy • ${ids.length} máy'),
              ),
            ),
          ],
        ),
        body: ListView(
          padding: const EdgeInsets.all(24),
          children: [
            const Text(
              'Chính sách thiết bị',
              style: TextStyle(fontSize: 26, fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 8),
            const Text(
              'Mỗi thay đổi áp dụng cho tất cả máy đang quản lý và máy tham gia sau. Chính sách được giữ qua mất kết nối, Teacher thoát, Student restart và Windows restart. Chỉ bỏ chặn hoặc kết thúc phiên phần cứng rõ ràng mới khôi phục.',
            ),
            const SizedBox(height: 8),
            const Text(
              'Windows 10/11 • Student cần Administrator và Classroom Guard service. Thiết bị mới được rà soát định kỳ; có khoảng trễ khi hot-plug. Applied là kết quả kiểm tra cấu hình OS, không phải chứng nhận đã test phần cứng thực tế.',
              style: TextStyle(color: Color(0xff64748b)),
            ),
            if (!agent.connected)
              const Padding(
                padding: EdgeInsets.only(top: 12),
                child: Text(
                  'Mất kết nối coordinator; chưa thể thay đổi chính sách.',
                  style: TextStyle(color: Colors.red),
                ),
              ),
            if (error != null)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: SelectableText(
                  error!,
                  style: const TextStyle(color: Colors.red),
                ),
              ),
            const SizedBox(height: 20),
            if (hardware['policy'] == null)
              const Padding(
                padding: EdgeInsets.only(bottom: 16),
                child: Text(
                  'Teacher chưa lưu policy. Trạng thái từng máy có thể là policy cũ còn giữ. Lần Apply đầu sẽ gửi cả 5 công tắc bên dưới (mặc định Unblock) cho toàn bộ phòng máy.',
                ),
              ),
            for (final entry in hardwareGroups.entries) ...[
              Card(
                elevation: 0,
                child: Padding(
                  padding: const EdgeInsets.all(18),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Icon(
                            entry.value.$3,
                            color: const Color(0xff156b79),
                            size: 28,
                          ),
                          const SizedBox(width: 14),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  entry.value.$1,
                                  style: const TextStyle(
                                    fontSize: 18,
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                                Text(entry.value.$2),
                              ],
                            ),
                          ),
                          Text(
                            policy[entry.key] == true ? 'Block' : 'Unblock',
                            style: const TextStyle(fontWeight: FontWeight.w600),
                          ),
                          Switch(
                            value: policy[entry.key] == true,
                            onChanged: enabled
                                ? (v) => apply(entry.key, v)
                                : null,
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Wrap(
                        spacing: 10,
                        runSpacing: 6,
                        children: [
                          for (final status in [
                            'applied',
                            'failed',
                            'pending',
                            'unsupported',
                          ])
                            Chip(
                              label: Text(
                                '${status[0].toUpperCase()}${status.substring(1)}: ${ids.where((id) => hardwareResult(hardware, caps, id, entry.key, rows[id]?['online'] == true)['status'] == status).length}',
                              ),
                            ),
                        ],
                      ),
                      if (ids.isNotEmpty)
                        ExpansionTile(
                          tilePadding: EdgeInsets.zero,
                          title: const Text('Trạng thái từng máy'),
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
                                    leading: Icon(
                                      status == 'applied'
                                          ? Icons.check_circle_outline
                                          : status == 'failed'
                                          ? Icons.error_outline
                                          : Icons.info_outline,
                                      color: status == 'applied'
                                          ? Colors.green
                                          : status == 'failed'
                                          ? Colors.red
                                          : Colors.orange,
                                    ),
                                    title: Text(
                                      '${rows[id]?['name'] ?? id} • $status${r['blocked'] is bool ? ' • ${r['blocked'] == true ? 'Block' : 'Unblock'}' : ''}',
                                    ),
                                    subtitle: r['error'] == ''
                                        ? null
                                        : SelectableText('${r['error'] ?? ''}'),
                                  );
                                },
                              ),
                          ],
                        ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 10),
            ],
            Card(
              elevation: 0,
              child: Column(
                children: [
                  SwitchListTile(
                    title: const Text('Khôi phục khi kết thúc phiên phần cứng'),
                    subtitle: const Text(
                      'Chỉ nút “Kết thúc phiên” dưới đây kích hoạt. Mất kết nối, dừng chiếu, đóng cửa sổ và Teacher thoát không bỏ chặn. Máy offline nhận lệnh khôi phục khi kết nối lại.',
                    ),
                    value: policy['restore_on_session_end'] == true,
                    onChanged: enabled
                        ? (v) => apply('restore_on_session_end', v)
                        : null,
                  ),
                  Padding(
                    padding: const EdgeInsets.all(16),
                    child: Align(
                      alignment: Alignment.centerRight,
                      child: OutlinedButton.icon(
                        onPressed:
                            enabled && policy['restore_on_session_end'] == true
                            ? () => apply('', false, end: true)
                            : null,
                        icon: const Icon(Icons.settings_backup_restore),
                        label: const Text('Kết thúc phiên • khôi phục tất cả'),
                      ),
                    ),
                  ),
                ],
              ),
            ),
            if (busy) const LinearProgressIndicator(),
          ],
        ),
      );
    },
  );
}
