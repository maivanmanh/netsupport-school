import 'dart:async';

import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';

const _terminal = {
  'complete',
  'skipped',
  'replied',
  'dismissed',
  'cancelled',
  'error',
};

/// One Student reply, captured when it first appears in this console session.
class StudentReply {
  StudentReply({
    required this.jobId,
    required this.studentId,
    required this.name,
    required this.ip,
    required this.text,
    required this.message,
    required this.received,
  });
  final String jobId;
  final String studentId;
  final String name;
  final String ip;
  final String text;
  final String message;
  final DateTime received;
}

/// Turns the agent's job snapshots into one-shot console notifications.
///
/// The Rust agent keeps recent jobs in memory and outlives the console, so a
/// reopened console would otherwise re-announce old replies and failures.
/// Everything already finished in the first snapshot this console sees is
/// treated as history and never shown. Nothing here is persisted.
class ToolActivity extends ChangeNotifier {
  ToolActivity(this.agent) {
    agent.addListener(_ingest);
    _ingest();
  }
  final Agent agent;
  final _seen = <String>{};
  bool _baselined = false;

  /// Replies received during this session, newest first (bounded).
  final replies = <StudentReply>[];
  int unread = 0;

  /// Replies not yet acknowledged in the toast (newest first, bounded).
  final toast = <StudentReply>[];

  /// Finished file/message jobs to announce once, until dismissed.
  final finished = <String>[];

  /// Students that disconnected while activity monitoring was on, newest
  /// first, kept for the session (mirrors `replies`/`unread` below).
  final disconnects = <Map<String, dynamic>>[];
  int unreadDisconnects = 0;
  final _hidden = <String>{};
  final _timers = <String, Timer>{};

  static const maxReplies = 300;

  List<Map<String, dynamic>> get jobs =>
      ((agent.state['tools'] as Map? ?? const {})['jobs'] as List? ?? const [])
          .map((e) => Map<String, dynamic>.from(e as Map))
          .toList();

  Map<String, Map<String, dynamic>> get _students => {
    for (final s in agent.students) '${s['id']}': s,
  };

  String nameOf(String id) {
    final s = _students[id];
    final name = '${s?['name'] ?? ''}'.trim();
    return name.isEmpty ? (s?['ip'] as String? ?? id) : name;
  }

  String ipOf(String id) => '${_students[id]?['ip'] ?? ''}';

  static List<Map<String, dynamic>> targetsOf(Map<String, dynamic> job) =>
      (job['targets'] as List? ?? const [])
          .map((e) => Map<String, dynamic>.from(e as Map))
          .toList();

  static bool isActive(Map<String, dynamic> job) =>
      targetsOf(job).any((t) => !_terminal.contains(t['status']));

  void _ingest() {
    if (!agent.state.containsKey('tools')) return;
    final current = jobs;
    var changed = false;
    for (final job in current) {
      final id = '${job['id']}';
      final targets = targetsOf(job);
      for (final t in targets) {
        final status = '${t['status']}';
        if (!_terminal.contains(status)) continue;
        final key = '$id/${t['id']}';
        if (!_seen.add(key) || !_baselined) continue;
        if (status == 'replied' && '${t['reply'] ?? ''}'.isNotEmpty) {
          final reply = StudentReply(
            jobId: id,
            studentId: '${t['id']}',
            name: nameOf('${t['id']}'),
            ip: ipOf('${t['id']}'),
            text: '${t['reply']}',
            message: '${job['text'] ?? ''}',
            received: DateTime.now(),
          );
          replies.insert(0, reply);
          toast.insert(0, reply);
          if (replies.length > maxReplies) replies.removeLast();
          if (toast.length > maxReplies) toast.removeLast();
          unread++;
          changed = true;
        }
      }
      final announce =
          _baselined &&
          !isActive(job) &&
          !finished.contains(id) &&
          !_hidden.contains(id) &&
          // Launch and power results are shown inside their own dialogs/menus.
          job['kind'] != 'launch' &&
          job['kind'] != 'website' &&
          job['kind'] != 'power' &&
          job['kind'] != 'policy' &&
          ((job['kind'] == 'file' || job['kind'] == 'update') ||
              targets.any((t) => t['status'] == 'error'));
      if (announce) {
        finished.insert(0, id);
        changed = true;
        final failed = targets.any(
          (t) => t['status'] == 'error' || t['status'] == 'cancelled',
        );
        if (!failed) {
          _timers[id] = Timer(const Duration(seconds: 5), () => hide(id));
        }
      }
    }
    if (!_baselined) {
      _baselined = true;
      for (final job in current) {
        if (!isActive(job)) _hidden.add('${job['id']}');
      }
    }
    final live = current.map((j) => '${j['id']}').toSet();
    finished.removeWhere((id) => !live.contains(id));
    final alerts =
        ((agent.state['tools'] as Map? ?? const {})['new_alerts'] as List?) ??
        const [];
    for (final a in alerts) {
      disconnects.insert(0, Map<String, dynamic>.from(a as Map));
      unreadDisconnects++;
      changed = true;
    }
    if (changed) notifyListeners();
  }

  void markDisconnectsRead() {
    if (unreadDisconnects == 0) return;
    unreadDisconnects = 0;
    notifyListeners();
  }

  void clearDisconnects() {
    disconnects.clear();
    unreadDisconnects = 0;
    notifyListeners();
  }

  void hide(String jobId) {
    _timers.remove(jobId)?.cancel();
    _hidden.add(jobId);
    if (finished.remove(jobId)) notifyListeners();
  }

  void dismissToast() {
    toast.clear();
    notifyListeners();
  }

  void markRead() {
    unread = 0;
    toast.clear();
    notifyListeners();
  }

  void clearReplies() {
    replies.clear();
    toast.clear();
    unread = 0;
    notifyListeners();
  }

  @override
  void dispose() {
    agent.removeListener(_ingest);
    for (final t in _timers.values) {
      t.cancel();
    }
    super.dispose();
  }
}

const _card = Color(0xff172b38);
const _muted = Color(0xffc7d4dc);
const _accent = Color(0xff7dd3dc);
const _warn = Color(0xffffc1a8);

/// Stack of compact cards: file progress, one-shot results and a single
/// aggregated reply toast (never one popup per reply).
class ClassroomToolsOverlay extends StatelessWidget {
  const ClassroomToolsOverlay({
    super.key,
    required this.agent,
    required this.activity,
    this.preparingFile = false,
    this.onOpenReplies,
  });
  final Agent agent;
  final ToolActivity activity;
  final bool preparingFile;
  final VoidCallback? onOpenReplies;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: Listenable.merge([agent, activity]),
      builder: (context, _) {
        final jobs = activity.jobs;
        final byId = {for (final j in jobs) '${j['id']}': j};
        final active = jobs.reversed
            .where(
              (j) =>
                  (j['kind'] == 'file' || j['kind'] == 'update') &&
                  ToolActivity.isActive(j),
            )
            .toList();
        final cards = <Widget>[
          if (preparingFile) const _PreparingCard(),
          if (active.isNotEmpty)
            _FileProgressCard(
              job: active.first,
              more: active.length - 1,
              activity: activity,
              agent: agent,
            ),
          for (final id in activity.finished.take(2))
            if (byId[id] != null)
              _FinishedCard(job: byId[id]!, activity: activity, agent: agent),
          if (activity.toast.isNotEmpty)
            _ReplyToast(activity: activity, onOpen: onOpenReplies),
        ];
        if (cards.isEmpty) return const SizedBox.shrink();
        return Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            for (final c in cards)
              Padding(padding: const EdgeInsets.only(top: 8), child: c),
          ],
        );
      },
    );
  }
}

class _Card extends StatelessWidget {
  const _Card({required this.children});
  final List<Widget> children;
  @override
  Widget build(BuildContext context) => Material(
    elevation: 8,
    color: _card,
    borderRadius: BorderRadius.circular(12),
    child: SizedBox(
      width: 340,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 10, 6, 10),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: children,
        ),
      ),
    ),
  );
}

Widget _header(
  IconData icon,
  String title, {
  required String tooltip,
  required VoidCallback onClose,
  Color color = _accent,
}) => Row(
  children: [
    Icon(icon, color: color, size: 18),
    const SizedBox(width: 8),
    Expanded(
      child: Text(
        title,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(
          color: Colors.white,
          fontWeight: FontWeight.w600,
        ),
      ),
    ),
    IconButton(
      visualDensity: VisualDensity.compact,
      tooltip: tooltip,
      onPressed: onClose,
      icon: const Icon(Icons.close, size: 17, color: Colors.white70),
    ),
  ],
);

Widget _line(String text, {Color color = _muted, int maxLines = 1}) => Padding(
  padding: const EdgeInsets.only(top: 4, right: 8),
  child: Text(
    text,
    maxLines: maxLines,
    overflow: TextOverflow.ellipsis,
    style: TextStyle(color: color, fontSize: 11),
  ),
);

String _who(ToolActivity a, String id) {
  final ip = a.ipOf(id);
  final name = a.nameOf(id);
  return ip.isEmpty || ip == name ? name : '$name ($ip)';
}

class _PreparingCard extends StatelessWidget {
  const _PreparingCard();
  @override
  Widget build(BuildContext context) => const _Card(
    children: [
      Text(
        'Đang chuẩn bị tệp…',
        style: TextStyle(color: Colors.white, fontWeight: FontWeight.w600),
      ),
      SizedBox(height: 10),
      Padding(
        padding: EdgeInsets.only(right: 6),
        child: LinearProgressIndicator(
          minHeight: 5,
          borderRadius: BorderRadius.all(Radius.circular(8)),
        ),
      ),
    ],
  );
}

class _FileProgressCard extends StatelessWidget {
  const _FileProgressCard({
    required this.job,
    required this.more,
    required this.activity,
    required this.agent,
  });
  final Map<String, dynamic> job;
  final int more;
  final ToolActivity activity;
  final Agent agent;

  @override
  Widget build(BuildContext context) {
    final targets = ToolActivity.targetsOf(job);
    final total = (job['size'] as num? ?? 0).toInt();
    final preparing = targets.every((t) => t['status'] == 'preparing');
    final done = targets.fold<int>(
      0,
      (sum, t) => sum + (t['done'] as num? ?? 0).toInt(),
    );
    final maximum = total * targets.length;
    final finished = targets
        .where((t) => t['status'] == 'complete' || t['status'] == 'skipped')
        .length;
    final busy = targets
        .where((t) => !_terminal.contains(t['status']))
        .take(3)
        .toList();
    return _Card(
      children: [
        _header(
          Icons.upload_file,
          '${job['kind'] == 'update' ? 'Đang cập nhật' : 'Đang gửi'} ${job['name'] ?? 'tệp'}',
          tooltip: 'Hủy truyền tệp',
          onClose: () =>
              agent.command('tool_cancel', const [], {'id': job['id']}),
        ),
        Padding(
          padding: const EdgeInsets.only(right: 6, top: 4),
          child: LinearProgressIndicator(
            value: preparing || maximum == 0
                ? null
                : (done / maximum).clamp(0.0, 1.0),
            minHeight: 5,
            borderRadius: BorderRadius.circular(8),
            backgroundColor: Colors.white24,
            color: const Color(0xff55c2cc),
          ),
        ),
        _line(
          preparing
              ? 'Đang kiểm tra tệp…'
              : '${_size(done)} / ${_size(maximum)} · $finished/${targets.length} máy',
        ),
        for (final t in busy)
          _line('${_who(activity, '${t['id']}')} · ${_status(t)}'),
        if (targets.length - finished > busy.length)
          _line('+ ${targets.length - finished - busy.length} máy nữa'),
        if (more > 0) _line('$more tệp nữa đang chờ'),
      ],
    );
  }
}

class _FinishedCard extends StatelessWidget {
  const _FinishedCard({
    required this.job,
    required this.activity,
    required this.agent,
  });
  final Map<String, dynamic> job;
  final ToolActivity activity;
  final Agent agent;

  @override
  Widget build(BuildContext context) {
    final id = '${job['id']}';
    final targets = ToolActivity.targetsOf(job);
    final failed = targets
        .where((t) => t['status'] == 'error' || t['status'] == 'cancelled')
        .toList();
    final isFile = (job['kind'] == 'file' || job['kind'] == 'update');
    final skipped = targets.where((t) => t['status'] == 'skipped').length;
    final ok = targets.length - failed.length;
    final title = job['kind'] == 'update'
        ? (failed.isEmpty
              ? 'Học viên đã cập nhật và kết nối lại'
              : 'Cập nhật học viên thất bại')
        : isFile
        ? (failed.isEmpty
              ? 'Đã gửi ${job['name'] ?? 'tệp'}'
              : 'Không gửi được ${job['name'] ?? 'tệp'}')
        : job['kind'] == 'launch'
        ? 'Không chạy được ${job['name'] ?? 'chương trình'}'
        : 'Tin nhắn chưa được chuyển';
    return _Card(
      children: [
        _header(
          failed.isEmpty ? Icons.check_circle_outline : Icons.error_outline,
          title,
          color: failed.isEmpty ? _accent : _warn,
          tooltip: 'Ẩn',
          onClose: () => activity.hide(id),
        ),
        if (isFile)
          _line(
            '$ok/${targets.length} students'
            '${skipped > 0 ? ' · $skipped already had it' : ''}',
          ),
        for (final t in failed.take(3))
          _line(
            '${_who(activity, '${t['id']}')}: '
            '${'${t['error'] ?? ''}'.isEmpty ? t['status'] : t['error']}',
            color: _warn,
            maxLines: 2,
          ),
        if (failed.length > 3)
          _line('+ ${failed.length - 3} máy nữa', color: _warn),
        if (isFile && failed.isNotEmpty)
          Align(
            alignment: Alignment.centerRight,
            child: TextButton.icon(
              onPressed: () {
                activity.hide(id);
                agent.command('tool_retry', const [], {'id': id});
              },
              icon: const Icon(Icons.refresh, size: 15),
              label: Text('Thử lại ${failed.length} máy'),
              style: TextButton.styleFrom(
                foregroundColor: const Color(0xff91e1e7),
                visualDensity: VisualDensity.compact,
              ),
            ),
          ),
      ],
    );
  }
}

class _ReplyToast extends StatelessWidget {
  const _ReplyToast({required this.activity, this.onOpen});
  final ToolActivity activity;
  final VoidCallback? onOpen;

  @override
  Widget build(BuildContext context) {
    final items = activity.toast;
    return _Card(
      children: [
        _header(
          Icons.mark_chat_unread_outlined,
          items.length == 1
              ? 'Phản hồi từ ${items.first.name}'
              : '${items.length} phản hồi mới',
          tooltip: 'Ẩn',
          onClose: activity.dismissToast,
        ),
        for (final r in items.take(3))
          Padding(
            padding: const EdgeInsets.only(top: 6, right: 8),
            child: Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: r.ip.isEmpty || r.ip == r.name
                        ? '${r.name}: '
                        : '${r.name} (${r.ip}): ',
                    style: const TextStyle(
                      color: Colors.white,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  TextSpan(text: r.text),
                ],
              ),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(color: _muted, fontSize: 11),
            ),
          ),
        if (items.length > 3) _line('+ ${items.length - 3} more'),
        Align(
          alignment: Alignment.centerRight,
          child: TextButton(
            onPressed: onOpen,
            style: TextButton.styleFrom(
              foregroundColor: const Color(0xff91e1e7),
              visualDensity: VisualDensity.compact,
            ),
            child: const Text('Xem tất cả phản hồi'),
          ),
        ),
      ],
    );
  }
}

/// Session reply inbox grouped by the message being answered.
Future<void> showRepliesDialog(BuildContext context, ToolActivity activity) {
  activity.markRead();
  return showDialog<void>(
    context: context,
    builder: (context) => AnimatedBuilder(
      animation: activity,
      builder: (context, _) {
        final groups = <String, List<StudentReply>>{};
        for (final r in activity.replies) {
          groups.putIfAbsent(r.jobId, () => []).add(r);
        }
        // Keep the badge clear while the inbox is open.
        if (activity.unread > 0) {
          scheduleMicrotask(activity.markRead);
        }
        return AppDialog(
          icon: Icons.forum_outlined,
          title: 'Phản hồi từ học viên (${activity.replies.length})',
          subtitle: 'Trả lời cho các tin nhắn bạn đã gửi trong phiên này.',
          width: 580,
          height: 400,
          scrollable: false,
          bodyPadding: const EdgeInsets.fromLTRB(12, 4, 12, 4),
          footerLeading: Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              onPressed: activity.replies.isEmpty
                  ? null
                  : activity.clearReplies,
              icon: const Icon(Icons.delete_sweep_outlined, size: 18),
              label: const Text('Xóa tất cả'),
            ),
          ),
          actions: [
            AppDialog.primary(
              onPressed: () => Navigator.pop(context),
              icon: Icons.check,
              label: 'Đóng',
            ),
          ],
          child: activity.replies.isEmpty
              ? const DialogEmpty(
                  icon: Icons.mark_chat_unread_outlined,
                  title: 'Chưa có phản hồi nào',
                  message: 'Phản hồi của học viên sẽ hiện ở đây.',
                )
              : ListView(
                  children: [
                    for (final entry in groups.entries) ...[
                      Padding(
                        padding: const EdgeInsets.fromLTRB(12, 12, 12, 4),
                        child: Text(
                          entry.value.first.message.isEmpty
                              ? 'Tin nhắn · ${entry.value.length} phản hồi'
                              : '“${entry.value.first.message}” · '
                                    '${entry.value.length} phản hồi',
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontSize: 12,
                            fontWeight: FontWeight.w700,
                            color: AppDialog.muted,
                          ),
                        ),
                      ),
                      for (final r in entry.value)
                        ListTile(
                          dense: true,
                          leading: const Icon(Icons.computer, size: 20),
                          title: Text(
                            r.ip.isEmpty || r.ip == r.name
                                ? r.name
                                : '${r.name} · ${r.ip}',
                            style: const TextStyle(fontWeight: FontWeight.w600),
                          ),
                          subtitle: SelectableText(r.text),
                          trailing: Text(
                            _time(r.received),
                            style: const TextStyle(
                              fontSize: 11,
                              color: AppDialog.muted,
                            ),
                          ),
                        ),
                      const Divider(height: 8),
                    ],
                  ],
                ),
        );
      },
    ),
  );
}

/// Session log of Students that disconnected while activity monitoring was on.
Future<void> showDisconnectsDialog(
  BuildContext context,
  ToolActivity activity,
) {
  activity.markDisconnectsRead();
  return showDialog<void>(
    context: context,
    builder: (context) => AnimatedBuilder(
      animation: activity,
      builder: (context, _) {
        if (activity.unreadDisconnects > 0) {
          scheduleMicrotask(activity.markDisconnectsRead);
        }
        return AppDialog(
          icon: Icons.link_off,
          tone: AppTone.danger,
          title: 'Cảnh báo mất kết nối (${activity.disconnects.length})',
          subtitle: 'Các máy bị mất kết nối khi đang bật theo dõi hoạt động.',
          width: 520,
          height: 340,
          scrollable: false,
          bodyPadding: EdgeInsets.zero,
          footerLeading: Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              onPressed: activity.disconnects.isEmpty
                  ? null
                  : activity.clearDisconnects,
              icon: const Icon(Icons.delete_sweep_outlined, size: 18),
              label: const Text('Xóa tất cả'),
            ),
          ),
          actions: [
            AppDialog.primary(
              onPressed: () => Navigator.pop(context),
              icon: Icons.check,
              label: 'Đóng',
            ),
          ],
          child: activity.disconnects.isEmpty
              ? const DialogEmpty(
                  icon: Icons.wifi_tethering,
                  title: 'Không có máy nào mất kết nối',
                  message: 'Chỉ ghi nhận khi theo dõi hoạt động đang bật.',
                )
              : ListView.separated(
                  itemCount: activity.disconnects.length,
                  separatorBuilder: (_, _) => const Divider(height: 1),
                  itemBuilder: (_, i) {
                    final a = activity.disconnects[i];
                    final id = '${a['student']}';
                    final ms = (a['unix_ms'] as num?)?.toInt() ?? 0;
                    return ListTile(
                      dense: true,
                      leading: const Icon(
                        Icons.link_off,
                        size: 20,
                        color: Color(0xffb83b40),
                      ),
                      title: Text(
                        activity.nameOf(id),
                        style: const TextStyle(fontWeight: FontWeight.w600),
                      ),
                      subtitle: Text(activity.ipOf(id)),
                      trailing: Text(
                        ms == 0
                            ? ''
                            : _time(DateTime.fromMillisecondsSinceEpoch(ms)),
                        style: const TextStyle(
                          fontSize: 11,
                          color: AppDialog.muted,
                        ),
                      ),
                    );
                  },
                ),
        );
      },
    ),
  );
}

String _time(DateTime t) =>
    '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}';

String _status(Map<String, dynamic> t) => switch ('${t['status']}') {
  'preparing' => 'đang chuẩn bị',
  'queued' => 'đang chờ',
  'waiting' => 'đang đợi',
  'receiving' => 'đang nhận ${_size((t['done'] as num? ?? 0).toInt())}',
  final other => other,
};

String _size(int bytes) {
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(0)} KB';
  if (bytes < 1024 * 1024 * 1024) {
    return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
  }
  return '${(bytes / (1024 * 1024 * 1024)).toStringAsFixed(2)} GB';
}
