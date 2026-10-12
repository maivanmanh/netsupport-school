import 'package:flutter/material.dart';
import 'agent.dart';

String recordingStatus(Map target) {
  final report = target['report'] as Map? ?? const {};
  final phase = switch (report['phase']) {
    'recording' => 'Đang ghi',
    'starting' => 'Đang khởi động',
    'stopped' => 'Đã dừng ghi',
    'error' => 'Lỗi ghi',
    'interrupted' => 'Ghi bị gián đoạn',
    _ => 'Chờ Student xác nhận',
  };
  final seconds = (report['elapsed_seconds'] as num? ?? 0).toInt();
  final duration =
      '${seconds ~/ 60}:${(seconds % 60).toString().padLeft(2, '0')}';
  final fps = (report['fps'] as num? ?? 0).toStringAsFixed(1);
  final received = target['received'] ?? 0;
  final queued = report['queued_chunks'] ?? 0;
  final connection = target['online'] != true
      ? ' • Offline, trạng thái gần nhất'
      : target['stale'] == true
      ? ' • Chưa có trạng thái mới'
      : '';
  final transfer = target['uploading'] == true
      ? ' • Đang tải ${((target['upload_bytes'] as num? ?? 0) / 1048576).toStringAsFixed(1)} MB'
      : '';
  return '$phase$connection • $duration • $fps FPS\n'
      '${report['width'] ?? '–'}×${report['height'] ?? '–'} • Đã nhận $received đoạn • Chờ tải $queued$transfer'
      '${report['error'] == null ? '' : '\n${report['error']}'}'
      '${report['upload_error'] == null || report['upload_error'] == '' ? '' : '\nTải lại tự động: ${report['upload_error']}'}';
}

Future<void> showRecordings(
  BuildContext context,
  Agent agent,
  List<String> ids,
) => showDialog<void>(
  context: context,
  builder: (_) => _RecordingDialog(agent: agent, ids: ids),
);

class _RecordingDialog extends StatefulWidget {
  const _RecordingDialog({required this.agent, required this.ids});
  final Agent agent;
  final List<String> ids;
  @override
  State<_RecordingDialog> createState() => _RecordingDialogState();
}

class _RecordingDialogState extends State<_RecordingDialog> {
  bool busy = false;
  String? error;
  Future<void> run(String action, {String? id}) async {
    setState(() {
      busy = true;
      error = null;
    });
    final result = await widget.agent.request(
      action,
      selected: widget.ids,
      input: id == null ? null : {'id': id},
      timeout: const Duration(seconds: 15),
    );
    if (mounted) {
      setState(() {
        busy = false;
        if (result['ok'] != true) error = Agent.failure(result);
      });
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.agent,
    builder: (context, _) {
      final sessions =
          (widget.agent.state['tools'] as Map?)?['recordings'] as List? ??
          const [];
      final targetNames = widget.agent.students
          .where((row) => widget.ids.contains('${row['id']}'))
          .map((row) => '${row['name'] ?? row['id']}')
          .join(', ');
      return AlertDialog(
        title: const Text('Screen recording'),
        content: SizedBox(
          width: 680,
          height: 460,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'Tối đa 720p, giữ tỉ lệ • Tối đa 30 FPS theo khả năng máy • Không âm thanh',
              ),
              const SizedBox(height: 6),
              const Text(
                'Mỗi đoạn 10 phút tự tải về Desktop theo thư mục máy. Stop tải cả đoạn cuối. Đóng cửa sổ này vẫn tiếp tục ghi.',
              ),
              const SizedBox(height: 6),
              Text(
                'Máy được chọn: ${targetNames.isEmpty ? 'Chưa chọn máy online' : targetNames}',
              ),
              if (error != null)
                Text(error!, style: const TextStyle(color: Colors.red)),
              const Divider(),
              Expanded(
                child: sessions.isEmpty
                    ? const Center(child: Text('Chưa có phiên ghi hình'))
                    : ListView(
                        children: sessions.map((item) {
                          final session = item as Map;
                          final targets =
                              session['targets'] as List? ?? const [];
                          final active =
                              session['stop'] != true &&
                              targets.any(
                                (t) => ![
                                  'stopped',
                                  'error',
                                  'interrupted',
                                ].contains((t['report'] as Map?)?['phase']),
                              );
                          return Card(
                            child: Padding(
                              padding: const EdgeInsets.all(10),
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Row(
                                    children: [
                                      Expanded(
                                        child: Text(
                                          '${session['folder']}',
                                          maxLines: 2,
                                          overflow: TextOverflow.ellipsis,
                                        ),
                                      ),
                                      IconButton(
                                        tooltip: 'Mở thư mục',
                                        onPressed: busy
                                            ? null
                                            : () => run(
                                                'recording_open',
                                                id: '${session['id']}',
                                              ),
                                        icon: const Icon(Icons.folder_open),
                                      ),
                                      if (active)
                                        TextButton.icon(
                                          onPressed: busy
                                              ? null
                                              : () => run(
                                                  'recording_stop',
                                                  id: '${session['id']}',
                                                ),
                                          icon: const Icon(Icons.stop),
                                          label: const Text('Stop'),
                                        ),
                                    ],
                                  ),
                                  if (session['stop'] == true)
                                    const Text(
                                      'Đã yêu cầu dừng • Các đoạn còn lại tiếp tục tải',
                                    ),
                                  ...targets.map(
                                    (t) => ListTile(
                                      dense: true,
                                      contentPadding: EdgeInsets.zero,
                                      title: Text('${t['name']}'),
                                      subtitle: Text(recordingStatus(t as Map)),
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          );
                        }).toList(),
                      ),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Đóng'),
          ),
          FilledButton.icon(
            onPressed: busy || !widget.agent.connected || widget.ids.isEmpty
                ? null
                : () => run('recording_start'),
            icon: const Icon(Icons.fiber_manual_record),
            label: Text('Ghi ${widget.ids.length} máy đã chọn'),
          ),
        ],
      );
    },
  );
}
