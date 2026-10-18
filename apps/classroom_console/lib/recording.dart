import 'icon_tints.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'live_activity.dart' show BlinkingRecordDot;

const _gib = 1024 * 1024 * 1024;

/// Plain-language reason a recording ended without the teacher pressing Stop.
String? recordingStopNotice(Object? reason) => switch (reason) {
  'time_limit' => 'Đã tự dừng vì đạt thời gian tối đa của phiên ghi.',
  'teacher_disk_low' =>
    'Đã tự dừng vì ổ đĩa giảng viên sắp đầy. Video đã ghi vẫn nằm trên máy học viên và sẽ tự tải về sau khi bạn giải phóng dung lượng.',
  'student_disk_low' =>
    'Máy học viên gần hết dung lượng ổ đĩa nên đã dừng ghi. Video đã ghi vẫn được tải về.',
  'spool_full' =>
    'Video chưa tải về trên máy học viên quá nhiều nên đã dừng ghi.',
  'teacher_unreachable' =>
    'Đã tự dừng vì máy học viên mất liên lạc với giảng viên quá 30 phút.',
  _ => null,
};

String _gb(num bytes) => (bytes / _gib).toStringAsFixed(1);

/// Standing information and warnings for a recording that is still capturing:
/// when it will stop by itself, and disks running low. `warn` marks lines the
/// teacher should act on.
List<({String text, bool warn})> recordingSessionNotices(
  Map session, {
  DateTime? now,
}) {
  final result = <({String text, bool warn})>[];
  final deadline = session['deadline'] as num?;
  if (deadline != null && session['stop'] != true) {
    final end = DateTime.fromMillisecondsSinceEpoch(deadline.toInt() * 1000);
    final left = end.difference(now ?? DateTime.now());
    String two(int n) => n.toString().padLeft(2, '0');
    final at = '${two(end.hour)}:${two(end.minute)}';
    if (left.inMinutes < 10) {
      result.add((
        text:
            'Sắp tự dừng: còn ${left.inMinutes < 1 ? 'dưới 1' : left.inMinutes} phút (lúc $at). Hãy bấm Dừng ghi hoặc bắt đầu phiên mới nếu cần ghi tiếp.',
        warn: true,
      ));
    } else {
      result.add((text: 'Tự động dừng lúc $at', warn: false));
    }
  }
  final free = session['teacher_free_bytes'] as num?;
  final warnBelow = session['teacher_warn_bytes'] as num? ?? 5 * _gib;
  final stopBelow = session['teacher_stop_bytes'] as num? ?? _gib;
  if (free != null && free < warnBelow) {
    result.add((
      text:
          'Ổ đĩa giảng viên chỉ còn ${_gb(free)} GB trống. Ghi sẽ tự dừng khi dưới ${_gb(stopBelow)} GB — hãy giải phóng dung lượng.',
      warn: true,
    ));
  }
  final studentStop =
      ((session['options'] as Map?)?['student_stop_bytes'] as num?) ??
      512 * 1024 * 1024;
  for (final target in (session['targets'] as List? ?? const [])) {
    final report = (target as Map)['report'] as Map? ?? const {};
    final machine = report['free_bytes'] as num?;
    if (report['phase'] == 'recording' &&
        report['disk_warning'] == true &&
        machine != null) {
      result.add((
        text:
            '${target['name'] ?? 'Máy học viên'}: ổ đĩa chỉ còn ${_gb(machine)} GB trống; sẽ dừng ghi khi dưới ${_gb(studentStop)} GB.',
        warn: true,
      ));
    }
  }
  return result;
}

/// One icon, colour and plain-language label for a whole session, used as its
/// status mark (the history tab shows only the icon; the label is its tooltip).
({IconData icon, Color color, String label, bool live}) recordingSessionStatus(
  Map session,
) {
  final targets = (session['targets'] as List? ?? const []).cast<Map>();
  final phases = [for (final t in targets) (t['report'] as Map?)?['phase']];
  final failed = phases.any((p) => p == 'error' || p == 'interrupted');
  final reason = '${session['stop_reason'] ?? ''}'.isNotEmpty
      ? '${session['stop_reason']}'
      : [
          for (final t in targets) (t['report'] as Map?)?['stop_reason'],
        ].firstWhere((r) => r != null && '$r'.isNotEmpty, orElse: () => '');
  if (recordingSessionCapturing(session)) {
    return session['stop'] == true
        ? (
            icon: Icons.stop_circle_outlined,
            color: const Color(0xffb26a00),
            label: 'Đang dừng',
            live: false,
          )
        : (
            icon: Icons.fiber_manual_record,
            color: const Color(0xffd32f2f),
            label: 'Đang ghi',
            live: true,
          );
  }
  if (recordingSessionPending(session)) {
    return (
      icon: Icons.cloud_upload_outlined,
      color: const Color(0xffb26a00),
      label: 'Chờ tải về',
      live: false,
    );
  }
  if (failed) {
    return (
      icon: Icons.error_outline,
      color: const Color(0xffc62828),
      label: phases.contains('interrupted')
          ? 'Ghi bị gián đoạn'
          : 'Kết thúc • Có lỗi khi ghi',
      live: false,
    );
  }
  if (reason != '') {
    final (icon, label) = switch (reason) {
      'time_limit' => (
        Icons.timer_off_outlined,
        'Tự dừng: hết thời gian tối đa',
      ),
      'teacher_disk_low' => (
        Icons.sd_storage_outlined,
        'Tự dừng: ổ đĩa giảng viên sắp đầy',
      ),
      'student_disk_low' => (
        Icons.sd_storage_outlined,
        'Tự dừng: ổ đĩa máy học viên sắp đầy',
      ),
      'spool_full' => (
        Icons.sd_storage_outlined,
        'Tự dừng: video chưa tải về quá nhiều',
      ),
      'teacher_unreachable' => (
        Icons.link_off,
        'Tự dừng: mất liên lạc với giảng viên',
      ),
      _ => (Icons.info_outline, 'Đã dừng tự động'),
    };
    return (
      icon: icon,
      color: const Color(0xffb26a00),
      label: label,
      live: false,
    );
  }
  return (
    icon: Icons.check_circle,
    color: const Color(0xff2e7d32),
    label: 'Đã hoàn tất',
    live: false,
  );
}

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
      '${recordingStopNotice(report['stop_reason']) == null || report['error'] != null ? '' : '\n${recordingStopNotice(report['stop_reason'])}'}'
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

Map? activeRecordingForStudent(Map tools, String studentId) {
  for (final session in tools['recordings'] as List? ?? const []) {
    for (final target in session['targets'] as List? ?? const []) {
      final phase = (target['report'] as Map?)?['phase'];
      if (target['id'] == studentId &&
          (phase == 'recording' || phase == 'starting')) {
        return session;
      }
    }
  }
  return null;
}

bool studentIsRecording(Map tools, String studentId) {
  for (final session in tools['recordings'] as List? ?? const []) {
    for (final target in session['targets'] as List? ?? const []) {
      if (target['id'] == studentId &&
          (target['report'] as Map?)?['phase'] == 'recording') {
        return true;
      }
    }
  }
  return false;
}

/// Seconds a Student has been recording, from its latest status, or null.
int? recordingElapsedSeconds(Map tools, String studentId) {
  for (final session in tools['recordings'] as List? ?? const []) {
    for (final target in session['targets'] as List? ?? const []) {
      final report = target['report'] as Map?;
      if (target['id'] == studentId && report?['phase'] == 'recording') {
        return (report?['elapsed_seconds'] as num?)?.toInt();
      }
    }
  }
  return null;
}

/// `MM:SS`, or `H:MM:SS` once past an hour.
String recordingClock(int seconds) {
  String two(int n) => n.toString().padLeft(2, '0');
  final h = seconds ~/ 3600;
  final m = (seconds % 3600) ~/ 60;
  final sec = seconds % 60;
  return h > 0 ? '$h:${two(m)}:${two(sec)}' : '${two(m)}:${two(sec)}';
}

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

  /// Sessions that were ever "recording or waiting" while this dialog was
  /// open. They stay in the first tab until the dialog is closed, so a
  /// recording the teacher stops does not vanish from under the cursor; it
  /// moves to History the next time the dialog is opened.
  final pinned = <String>{};

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

  /// Removes the recording from the list and its downloaded MP4 files from the
  /// Teacher. Not while a Student is still capturing.
  Future<void> deleteSession(Map session) async {
    final targets = (session['targets'] as List? ?? const []).cast<Map>();
    final files = targets.fold<int>(
      0,
      (n, t) => n + (t['received'] as num? ?? 0).toInt(),
    );
    final unfinished = recordingSessionPending(session);
    final ok = await confirmDialog(
      context,
      title: 'Xóa phiên ghi này?',
      icon: Icons.delete_outline,
      destructive: true,
      confirmLabel: 'Xóa',
      message: files > 0
          ? 'Phiên ghi sẽ bị xóa khỏi danh sách cùng $files đoạn video MP4 đã tải về máy giảng viên. Không thể hoàn tác.${unfinished ? ' Phần chưa tải về trên máy học viên cũng sẽ bị bỏ.' : ''}'
          : 'Phiên ghi sẽ bị xóa khỏi danh sách (chưa có video nào được tải về).${unfinished ? ' Phần chưa tải về trên máy học viên cũng sẽ bị bỏ.' : ''}',
    );
    if (!mounted || !ok) return;
    await run('recording_delete', id: '${session['id']}');
  }

  /// "hostname (ip)" for a recorded computer; the address comes from the roster
  /// (it is also remembered there while the computer is offline).
  String who(Map target) {
    final name = '${target['name'] ?? target['id'] ?? ''}';
    String ip = '';
    for (final row in widget.agent.students) {
      if (row['id'] == target['id']) {
        ip = '${row['ip'] ?? ''}';
        break;
      }
    }
    return ip.isEmpty || ip == name ? name : '$name ($ip)';
  }

  String started(Map session) {
    final seconds = session['created'] as num?;
    if (seconds == null) return 'Phiên ghi';
    final time = DateTime.fromMillisecondsSinceEpoch(seconds.toInt() * 1000);
    String two(int n) => n.toString().padLeft(2, '0');
    return '${two(time.day)}/${two(time.month)}/${time.year} • ${two(time.hour)}:${two(time.minute)}';
  }

  static String clock(num? raw) {
    final seconds = (raw ?? 0).toInt();
    return '${seconds ~/ 60}:${(seconds % 60).toString().padLeft(2, '0')}';
  }

  Widget badge(String text, Color color) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
    decoration: BoxDecoration(
      color: color.withValues(alpha: 0.10),
      borderRadius: BorderRadius.circular(10),
    ),
    child: Text(
      text,
      style: TextStyle(
        fontSize: 11.5,
        fontWeight: FontWeight.w600,
        color: color,
        height: 1.3,
      ),
    ),
  );

  Widget sessionCard(Map session, {bool history = false}) {
    final theme = Theme.of(context);
    final targets = (session['targets'] as List? ?? const []).cast<Map>();
    final capturing = recordingSessionCapturing(session);
    final pending = recordingSessionPending(session);
    final stopping = session['stop'] == true;
    final status = recordingSessionStatus(session);
    final label = status.label;
    final color = status.color;
    final showLabel = capturing || pending;
    final received = targets.fold<int>(
      0,
      (n, t) => n + (t['received'] as num? ?? 0).toInt(),
    );
    final single = targets.length == 1;
    final title = targets.length > 1
        ? 'Phiên ghi • ${targets.length} máy'
        : targets.isEmpty
        ? 'Phiên ghi'
        : who(targets.first);
    final subtitle = '${started(session)} • $received đoạn MP4 trên Teacher';
    const small = TextStyle(fontSize: 12, color: AppDialog.muted, height: 1.3);
    final folder = '${session['folder'] ?? ''}';
    return Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.fromLTRB(12, 10, 8, 8),
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: AppDialog.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              Tooltip(
                message: label,
                child: status.live
                    ? const BlinkingRecordDot(size: 16)
                    : TIcon(status.icon, size: 18, color: color),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 14,
                    fontWeight: FontWeight.w600,
                    color: AppDialog.ink,
                  ),
                ),
              ),
              if (showLabel) ...[const SizedBox(width: 8), badge(label, color)],
              const SizedBox(width: 4),
              Tooltip(
                message: folder,
                child: TextButton.icon(
                  style: TextButton.styleFrom(
                    visualDensity: VisualDensity.compact,
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    minimumSize: const Size(0, 30),
                    foregroundColor: AppDialog.muted,
                  ),
                  onPressed: busy
                      ? null
                      : () => run('recording_open', id: '${session['id']}'),
                  icon: const TIcon(Icons.folder_open, size: 16),
                  label: const Text('Mở thư mục'),
                ),
              ),
              if (capturing)
                TextButton.icon(
                  style: TextButton.styleFrom(
                    visualDensity: VisualDensity.compact,
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    minimumSize: const Size(0, 30),
                    foregroundColor: theme.colorScheme.error,
                  ),
                  onPressed: busy || stopping
                      ? null
                      : () => run('recording_stop', id: '${session['id']}'),
                  icon: const TIcon(Icons.stop, size: 16),
                  label: Text(stopping ? 'Đã yêu cầu dừng' : 'Dừng ghi'),
                ),
              if (history)
                Tooltip(
                  message: 'Xóa phiên ghi và các video đã tải về',
                  child: IconButton(
                    visualDensity: VisualDensity.compact,
                    iconSize: 19,
                    color: theme.colorScheme.error,
                    onPressed: busy ? null : () => deleteSession(session),
                    icon: const TIcon(Icons.delete_outline),
                  ),
                ),
            ],
          ),
          if (recordingStopNotice(session['stop_reason']) != null)
            Padding(
              padding: const EdgeInsets.only(left: 22, top: 3),
              child: Text(
                recordingStopNotice(session['stop_reason'])!,
                style: small.copyWith(color: const Color(0xffb26a00)),
              ),
            ),
          if (capturing)
            for (final notice in recordingSessionNotices(session))
              Padding(
                padding: const EdgeInsets.only(left: 22, top: 3),
                child: Text(
                  notice.text,
                  style: small.copyWith(
                    color: notice.warn
                        ? const Color(0xffb26a00)
                        : AppDialog.muted,
                    fontWeight: notice.warn ? FontWeight.w600 : null,
                  ),
                ),
              ),
          if (!single)
            Padding(
              padding: const EdgeInsets.only(left: 22, top: 2),
              child: Text(subtitle, style: small),
            ),
          if (targets.isNotEmpty) ...[
            SizedBox(height: single ? 2 : 6),
            Padding(
              padding: const EdgeInsets.only(left: 22, right: 4),
              child: Column(
                children: [
                  for (final target in targets)
                    targetRow(
                      target,
                      stopping,
                      created: session['created'] as num?,
                      showName: !single,
                      lead: single ? started(session) : null,
                    ),
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }

  Widget targetRow(
    Map target,
    bool stopping, {
    num? created,
    bool showName = true,
    String? lead,
  }) {
    final theme = Theme.of(context);
    final report = target['report'] as Map? ?? const {};
    final received = target['received'] ?? 0;
    final queued = report['queued_chunks'] ?? 0;
    final terminal = const [
      'stopped',
      'error',
      'interrupted',
    ].contains(report['phase']);
    // Before the Student's first progress report the recording is usually fine,
    // just not reported yet: say so calmly, and only complain after a while.
    final waiting = const [
      null,
      'pending',
      'starting',
    ].contains(report['phase']);
    final waitedSeconds = created == null
        ? 0
        : DateTime.now().millisecondsSinceEpoch ~/ 1000 - created.toInt();
    final waitedTooLong = waiting && waitedSeconds > 45;
    final phase = switch (report['phase']) {
      'recording' => stopping ? 'Đang dừng' : 'Đang ghi',
      'starting' => 'Đang khởi động ghi trên máy học viên…',
      'stopped' => 'Đã dừng ghi',
      'error' => 'Lỗi ghi',
      'interrupted' => 'Ghi bị gián đoạn',
      _ => 'Đang chờ máy học viên báo tiến độ đầu tiên…',
    };
    final connection = target['online'] != true
        ? 'Offline • Trạng thái gần nhất'
        : waitedTooLong
        ? 'Chưa nhận được phản hồi từ máy học viên sau $waitedSeconds giây — kiểm tra máy còn bật và còn kết nối.'
        : (target['stale'] == true && !waiting)
        ? 'Chưa có trạng thái mới'
        : null;
    final failure = [
      report['error'],
      report['upload_error'],
      if (report['error'] == null) recordingStopNotice(report['stop_reason']),
    ].where((v) => v != null && '$v'.isNotEmpty).join(' • ');
    final recording = report['phase'] == 'recording' && !stopping;
    final clockText = waiting ? '' : clock(report['elapsed_seconds'] as num?);
    final rest = [
      if (!waiting) '${report['width'] ?? '–'}×${report['height'] ?? '–'}',
      if (!terminal && !waiting)
        '${(report['fps'] as num? ?? 0).toStringAsFixed(1)} FPS',
      if (!waiting) 'Nhận $received • Chờ $queued',
    ].join('  ·  ');
    const metricStyle = TextStyle(
      fontSize: 12,
      color: AppDialog.muted,
      height: 1.3,
      fontFeatures: [FontFeature.tabularFigures()],
    );
    final metrics = TextSpan(
      style: metricStyle,
      children: [
        if (clockText.isNotEmpty)
          TextSpan(
            text: clockText,
            style: recording
                ? const TextStyle(
                    color: Color(0xffd32f2f),
                    fontWeight: FontWeight.w700,
                  )
                : null,
          ),
        if (clockText.isNotEmpty && rest.isNotEmpty)
          const TextSpan(text: '  ·  '),
        TextSpan(text: rest),
      ],
    );
    final showConnection =
        connection != null &&
        (!terminal ||
            recordingSessionPending({
              'targets': [target],
            }));
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              if (showName)
                Flexible(
                  flex: 0,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 250),
                    child: Text(
                      who(target),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 12.5,
                        fontWeight: FontWeight.w600,
                        color: AppDialog.ink,
                      ),
                    ),
                  ),
                ),
              if (showName) const SizedBox(width: 8),
              Flexible(
                child: Text(
                  showName
                      ? phase
                      : '${lead == null ? '' : '$lead  ·  '}$phase  ·',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 12,
                    color: terminal && report['phase'] != 'stopped'
                        ? theme.colorScheme.error
                        : AppDialog.muted,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text.rich(
                  metrics,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  textAlign: showName ? TextAlign.right : TextAlign.left,
                ),
              ),
            ],
          ),
          if (target['uploading'] == true)
            Padding(
              padding: const EdgeInsets.only(top: 3),
              child: Row(
                children: [
                  const Expanded(child: LinearProgressIndicator(minHeight: 3)),
                  const SizedBox(width: 8),
                  Text(
                    'Đang tải ${((target['upload_bytes'] as num? ?? 0) / 1048576).toStringAsFixed(1)} MB',
                    style: metricStyle,
                  ),
                ],
              ),
            ),
          if (showConnection)
            Text(
              connection,
              style: TextStyle(fontSize: 12, color: theme.colorScheme.error),
            ),
          if (failure.isNotEmpty)
            Text(
              failure,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 12, color: theme.colorScheme.error),
            ),
        ],
      ),
    );
  }

  Widget sessionList(List<Map> sessions, bool history) => sessions.isEmpty
      ? Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TIcon(
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
          padding: const EdgeInsets.only(top: 10, bottom: 8),
          itemCount: sessions.length,
          itemBuilder: (_, i) => sessionCard(sessions[i], history: history),
        );

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.agent,
    builder: (context, _) {
      final sessions =
          ((widget.agent.state['tools'] as Map?)?['recordings'] as List? ??
                  const [])
              .cast<Map>();
      for (final s in sessions) {
        if (recordingSessionPending(s)) pinned.add('${s['id']}');
      }
      final pending = sessions
          .where((s) => pinned.contains('${s['id']}'))
          .toList();
      final history = sessions
          .where((s) => !pinned.contains('${s['id']}'))
          .toList();
      final maxMinutes =
          (((widget.agent.state['settings'] as Map?)?['tools']
                          as Map?)?['recording_max_minutes']
                      as num? ??
                  60)
              .toInt();
      final targetNames = widget.agent.students
          .where((row) => widget.ids.contains('${row['id']}'))
          .map((row) => '${row['name'] ?? row['id']}')
          .join(', ');
      return DefaultTabController(
        length: 2,
        child: AppDialog(
          icon: Icons.video_camera_back_outlined,
          title: 'Ghi màn hình',
          subtitle: widget.ids.isEmpty
              ? 'Quản lý các phiên ghi. Muốn ghi một máy: nhấp chuột phải vào máy đó → Ghi màn hình.'
              : 'Đang bắt đầu ghi: $targetNames',
          width: 780,
          height: 500,
          scrollable: false,
          bodyPadding: const EdgeInsets.fromLTRB(24, 12, 24, 0),
          footerLeading: Text(
            '720p • Giữ tỉ lệ • Tối đa 30 FPS • MP4 mỗi 10 phút • Tự dừng sau tối đa $maxMinutes phút (chỉnh trong Cài đặt) • Tự lưu về Desktop. Đóng cửa sổ này vẫn tiếp tục ghi.',
            style: const TextStyle(
              fontSize: 12,
              color: AppDialog.muted,
              height: 1.35,
            ),
          ),
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
