import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';

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

// A Stop request is not completion: keep the session visible until Students
// confirm a terminal phase and all completed chunks have reached Teacher.
bool recordingSessionCapturing(Map session) =>
    (session['targets'] as List? ?? const []).any(
      (target) => !const [
        'stopped',
        'error',
        'interrupted',
      ].contains((target['report'] as Map?)?['phase']),
    );

bool recordingSessionPending(Map session) =>
    recordingSessionCapturing(session) ||
    (session['targets'] as List? ?? const []).any((target) {
      final report = target['report'] as Map? ?? const {};
      return target['uploading'] == true ||
          (report['queued_chunks'] as num? ?? 0) > 0 ||
          (report['completed_chunks'] as num? ?? 0) >
              (target['received'] as num? ?? 0);
    });

Future<void> showRecordings(
  BuildContext context,
  Agent agent,
  List<String> ids, {
  bool startImmediately = false,
}) => showDialog<void>(
  context: context,
  builder: (_) => _RecordingDialog(
    agent: agent,
    ids: ids,
    startImmediately: startImmediately,
  ),
);

class _RecordingDialog extends StatefulWidget {
  const _RecordingDialog({
    required this.agent,
    required this.ids,
    required this.startImmediately,
  });
  final Agent agent;
  final List<String> ids;
  final bool startImmediately;
  @override
  State<_RecordingDialog> createState() => _RecordingDialogState();
}

class _RecordingDialogState extends State<_RecordingDialog> {
  bool busy = false;
  String? error;

  @override
  void initState() {
    super.initState();
    if (widget.startImmediately) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) run('recording_start');
      });
    }
  }

  Future<void> run(String action, {String? id}) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final result = await widget.agent.request(
        action,
        selected: widget.ids,
        input: id == null ? null : {'id': id},
        timeout: const Duration(seconds: 15),
      );
      if (mounted && result['ok'] != true) {
        setState(() => error = Agent.failure(result));
      }
    } catch (failure) {
      if (mounted) setState(() => error = '$failure');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  String started(Map session) {
    final seconds = session['created'] as num?;
    if (seconds == null) return 'Phiên ghi';
    final time = DateTime.fromMillisecondsSinceEpoch(seconds.toInt() * 1000);
    String two(int n) => n.toString().padLeft(2, '0');
    return '${two(time.day)}/${two(time.month)}/${time.year} • ${two(time.hour)}:${two(time.minute)}';
  }

  Widget sessionCard(Map session) {
    final theme = Theme.of(context);
    final targets = session['targets'] as List? ?? const [];
    final capturing = recordingSessionCapturing(session);
    final pending = recordingSessionPending(session);
    final failed = targets.any(
      (t) => const [
        'error',
        'interrupted',
      ].contains((t['report'] as Map?)?['phase']),
    );
    final label = session['stop'] == true && capturing
        ? 'Đang dừng'
        : capturing
        ? 'Đang ghi'
        : pending
        ? 'Chờ tải về'
        : failed
        ? 'Đã kết thúc • Có lỗi'
        : 'Đã hoàn tất';
    final color = failed
        ? theme.colorScheme.error
        : capturing
        ? theme.colorScheme.primary
        : theme.colorScheme.onSurfaceVariant;
    final names = targets.map((t) => '${t['name'] ?? t['id']}').join(', ');
    final received = targets.fold<int>(
      0,
      (n, t) => n + (t['received'] as num? ?? 0).toInt(),
    );
    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      clipBehavior: Clip.antiAlias,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  capturing
                      ? Icons.radio_button_checked
                      : Icons.video_library_outlined,
                  color: color,
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        targets.length > 1
                            ? 'Phiên ghi • ${targets.length} máy'
                            : names.isEmpty
                            ? 'Phiên ghi'
                            : names,
                        style: theme.textTheme.titleMedium,
                      ),
                      const SizedBox(height: 4),
                      Text(started(session), style: theme.textTheme.bodySmall),
                    ],
                  ),
                ),
                const SizedBox(width: 8),
                Chip(
                  label: Text(label),
                  visualDensity: VisualDensity.compact,
                  labelStyle: TextStyle(color: color),
                ),
              ],
            ),
            const Divider(height: 24),
            ...targets.map(
              (target) => targetRow(target as Map, session['stop'] == true),
            ),
            const SizedBox(height: 8),
            Text(
              '$received đoạn MP4 đã lưu trên Teacher',
              style: theme.textTheme.labelLarge,
            ),
            const SizedBox(height: 4),
            SelectableText(
              '${session['folder'] ?? ''}',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 10),
            Wrap(
              spacing: 12,
              runSpacing: 8,
              children: [
                OutlinedButton.icon(
                  onPressed: busy
                      ? null
                      : () => run('recording_open', id: '${session['id']}'),
                  icon: const Icon(Icons.folder_open),
                  label: const Text('Mở thư mục'),
                ),
                if (capturing)
                  FilledButton.tonalIcon(
                    onPressed: busy || session['stop'] == true
                        ? null
                        : () => run('recording_stop', id: '${session['id']}'),
                    icon: const Icon(Icons.stop),
                    label: Text(
                      session['stop'] == true ? 'Đã yêu cầu dừng' : 'Dừng ghi',
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget targetRow(Map target, bool stopping) {
    final theme = Theme.of(context);
    final report = target['report'] as Map? ?? const {};
    final seconds = (report['elapsed_seconds'] as num? ?? 0).toInt();
    final received = target['received'] ?? 0;
    final queued = report['queued_chunks'] ?? 0;
    final terminal = const [
      'stopped',
      'error',
      'interrupted',
    ].contains(report['phase']);
    final phase = switch (report['phase']) {
      'recording' => stopping ? 'Đang dừng' : 'Đang ghi',
      'starting' => 'Đang khởi động',
      'stopped' => 'Đã dừng ghi',
      'error' => 'Lỗi ghi',
      'interrupted' => 'Ghi bị gián đoạn',
      _ => 'Chờ Student xác nhận',
    };
    final connection = target['online'] != true
        ? 'Offline • Trạng thái gần nhất'
        : target['stale'] == true
        ? 'Chưa có trạng thái mới'
        : null;
    final failure = [
      report['error'],
      report['upload_error'],
    ].where((v) => v != null && '$v'.isNotEmpty).join(' • ');
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '${target['name'] ?? target['id']} • $phase',
            style: theme.textTheme.titleSmall,
          ),
          const SizedBox(height: 6),
          Wrap(
            spacing: 16,
            runSpacing: 4,
            children: [
              Text(
                'Thời lượng ${seconds ~/ 60}:${(seconds % 60).toString().padLeft(2, '0')}',
              ),
              Text('${report['width'] ?? '–'}×${report['height'] ?? '–'}'),
              if (!terminal)
                Text('${(report['fps'] as num? ?? 0).toStringAsFixed(1)} FPS'),
            ],
          ),
          const SizedBox(height: 4),
          Text('Đã nhận $received đoạn • Chờ tải $queued'),
          if (target['uploading'] == true) ...[
            const SizedBox(height: 6),
            const LinearProgressIndicator(),
            Text(
              'Đang tải về • ${((target['upload_bytes'] as num? ?? 0) / 1048576).toStringAsFixed(1)} MB',
            ),
          ],
          if (connection != null &&
              (!terminal ||
                  recordingSessionPending({
                    'targets': [target],
                  })))
            Text(connection, style: TextStyle(color: theme.colorScheme.error)),
          if (failure.isNotEmpty)
            Text(failure, style: TextStyle(color: theme.colorScheme.error)),
        ],
      ),
    );
  }

  Widget sessionList(List<Map> sessions, bool history) => sessions.isEmpty
      ? Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                history
                    ? Icons.video_library_outlined
                    : Icons.videocam_outlined,
                size: 40,
              ),
              const SizedBox(height: 12),
              Text(
                history
                    ? 'Chưa có phiên ghi đã kết thúc'
                    : 'Không có phiên đang ghi hoặc chờ tải',
              ),
            ],
          ),
        )
      : ListView.builder(
          padding: const EdgeInsets.only(top: 12),
          itemCount: sessions.length,
          itemBuilder: (_, i) => sessionCard(sessions[i]),
        );

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.agent,
    builder: (context, _) {
      final sessions =
          ((widget.agent.state['tools'] as Map?)?['recordings'] as List? ??
                  const [])
              .cast<Map>();
      final pending = sessions.where(recordingSessionPending).toList();
      final history = sessions
          .where((s) => !recordingSessionPending(s))
          .toList();
      final targetNames = widget.agent.students
          .where((row) => widget.ids.contains('${row['id']}'))
          .map((row) => '${row['name'] ?? row['id']}')
          .join(', ');
      return DefaultTabController(
        length: 2,
        child: AppDialog(
          icon: Icons.video_camera_back_outlined,
          title: 'Ghi màn hình',
          subtitle:
              'Ghi mới: ${targetNames.isEmpty ? '${widget.ids.length} máy được chọn' : targetNames}',
          width: 780,
          height: 470,
          scrollable: false,
          bodyPadding: const EdgeInsets.fromLTRB(24, 12, 24, 0),
          footerLeading: const Text(
            '720p • Giữ tỉ lệ • Tối đa 30 FPS • MP4 mỗi 10 phút • Tự lưu về Desktop. Đóng cửa sổ này vẫn tiếp tục ghi.',
            style: TextStyle(
              fontSize: 12,
              color: AppDialog.muted,
              height: 1.35,
            ),
          ),
          actions: [
            AppDialog.cancel(context, label: 'Đóng'),
            AppDialog.primary(
              onPressed: !widget.agent.connected || widget.ids.isEmpty
                  ? null
                  : () => run('recording_start'),
              icon: Icons.fiber_manual_record,
              busy: busy,
              label: busy
                  ? 'Đang xử lý…'
                  : 'Ghi ${widget.ids.length} máy đã chọn',
            ),
          ],
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (error != null) ...[
                InlineNotice(error!),
                const SizedBox(height: 10),
              ],
              TabBar(
                tabs: [
                  Tab(text: 'Đang ghi / chờ tải (${pending.length})'),
                  Tab(text: 'Lịch sử (${history.length})'),
                ],
              ),
              Expanded(
                child: TabBarView(
                  children: [
                    sessionList(pending, false),
                    sessionList(history, true),
                  ],
                ),
              ),
            ],
          ),
        ),
      );
    },
  );
}
