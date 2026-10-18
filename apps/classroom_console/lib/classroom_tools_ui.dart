import 'icon_tints.dart';
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

  /// Console preferences (Settings → Thông báo); the numbers are the defaults.
  int _pref(String key, int fallback) {
    final v = agent.preferences[key];
    return v is num ? v.toInt() : fallback;
  }

  int get okSeconds => _pref('ui_toast_ok_seconds', 5);
  int get errorSeconds => _pref('ui_toast_error_seconds', 10);
  int get maxReplies => _pref('ui_max_replies', 300);

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
        // A failed transfer stays (it offers a retry); everything else is only
        // information and goes away by itself: 5 s when it worked, 10 s when
        // it did not, enough to read a short line.
        final retryable =
            failed && (job['kind'] == 'file' || job['kind'] == 'update');
        if (!retryable) {
          _timers[id] = Timer(
            Duration(seconds: failed ? errorSeconds : okSeconds),
            () => hide(id),
          );
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
    // A Student that comes back updates its own entry instead of adding a
    // second one: the entry then shows both when it dropped and when it returned.
    final connected = {
      for (final id
          in ((agent.state['tools'] as Map? ?? const {})['ready'] as List? ??
              const []))
        '$id',
    };
    final nowMs = DateTime.now().millisecondsSinceEpoch;
    for (final entry in disconnects) {
      if (entry['reconnected_ms'] == null &&
          connected.contains('${entry['student']}')) {
        entry['reconnected_ms'] = nowMs;
        changed = true;
      }
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

  void hideAll(Iterable<String> ids) {
    var any = false;
    for (final id in ids.toList()) {
      _timers.remove(id)?.cancel();
      _hidden.add(id);
      any = finished.remove(id) || any;
    }
    if (any) notifyListeners();
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

// Same light look as the rest of the Teacher console.
const _ink = Color(0xff1b2a36);
const _muted = Color(0xff5c6b78);
const _accent = Color(0xff156b79);
const _good = Color(0xff1f7a4d);
const _warn = Color(0xffb83b40);

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
          ..._finishedGroups(activity, byId, agent),
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

/// What operation a finished job was, in words a Teacher recognises.
String _finishedTitle(Map<String, dynamic> job, bool failed) {
  final name = '${job['name'] ?? ''}';
  switch ('${job['kind']}') {
    case 'update':
      return failed
          ? 'Cập nhật học viên thất bại'
          : 'Học viên đã cập nhật và kết nối lại';
    case 'file':
      return failed
          ? 'Không gửi được ${name.isEmpty ? 'tệp' : name}'
          : 'Đã gửi ${name.isEmpty ? 'tệp' : name}';
    case 'launch':
      return 'Không chạy được ${name.isEmpty ? 'chương trình' : name}';
    case 'app_policy':
      return 'Không áp dụng được chặn ứng dụng';
    case 'vscode_policy':
      return 'Không áp dụng được chính sách VS Code';
    case 'copilot_policy':
      return 'Không áp dụng được chặn Copilot';
    case 'message':
      return 'Tin nhắn chưa được chuyển';
    default:
      return 'Thao tác chưa hoàn tất';
  }
}

/// Student-side errors are English; say them the way the Teacher reads them.
String _friendlyError(String error) {
  const known = {
    'System folders cannot be blocked':
        'Không thể chặn thư mục hệ thống Windows',
    'The Classroom program folder cannot be blocked':
        'Không thể chặn thư mục của Classroom',
    'Student is offline': 'Máy học viên đang offline',
  };
  for (final entry in known.entries) {
    if (error.startsWith(entry.key)) {
      return entry.value + error.substring(entry.key.length);
    }
  }
  return error;
}

/// Finished jobs with the same outcome become one card ("×3"), and at most
/// three cards are shown, so one dismissal is never answered by another
/// identical card taking its place.
List<Widget> _finishedGroups(
  ToolActivity activity,
  Map<String, Map<String, dynamic>> byId,
  Agent agent,
) {
  final groups = <String, List<Map<String, dynamic>>>{};
  for (final id in activity.finished) {
    final job = byId[id];
    if (job == null) continue;
    final targets = ToolActivity.targetsOf(job);
    final failed = targets.any(
      (t) => t['status'] == 'error' || t['status'] == 'cancelled',
    );
    final firstError = targets
        .map((t) => '${t['error'] ?? ''}')
        .firstWhere((e) => e.isNotEmpty, orElse: () => '');
    final key = '${_finishedTitle(job, failed)}|$failed|$firstError';
    groups.putIfAbsent(key, () => []).add(job);
  }
  final shown = groups.values.take(3).toList();
  return [
    for (final jobs in shown)
      _FinishedCard(jobs: jobs, activity: activity, agent: agent),
    if (groups.length > 1)
      Align(
        alignment: Alignment.centerRight,
        child: TextButton(
          onPressed: () => activity.hideAll(activity.finished),
          style: TextButton.styleFrom(
            foregroundColor: _muted,
            visualDensity: VisualDensity.compact,
          ),
          child: Text('Ẩn tất cả (${groups.length})'),
        ),
      ),
  ];
}

class _Card extends StatelessWidget {
  const _Card({required this.children, this.accent = _accent});
  final List<Widget> children;

  /// Colour of the stripe on the left edge: what kind of message this is.
  final Color accent;
  @override
  Widget build(BuildContext context) => Material(
    elevation: 6,
    shadowColor: Colors.black45,
    color: Colors.white,
    clipBehavior: Clip.antiAlias,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(12),
      side: const BorderSide(color: Color(0xffdde3e8)),
    ),
    child: SizedBox(
      width: 360,
      child: IntrinsicHeight(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(width: 4, color: accent),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(12, 8, 6, 10),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: children,
                ),
              ),
            ),
          ],
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
    TIcon(icon, color: color, size: 19),
    const SizedBox(width: 8),
    Expanded(
      child: Text(
        title,
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(
          color: _ink,
          fontSize: 13.5,
          fontWeight: FontWeight.w700,
        ),
      ),
    ),
    // A generous target: this is clicked once and must register the first time.
    SizedBox(
      width: 36,
      height: 36,
      child: IconButton(
        padding: EdgeInsets.zero,
        tooltip: tooltip,
        onPressed: onClose,
        icon: const TIcon(Icons.close, size: 18, color: _muted),
      ),
    ),
  ],
);

Widget _line(String text, {Color color = _muted, int maxLines = 1}) => Padding(
  padding: const EdgeInsets.only(top: 4, right: 8),
  child: Text(
    text,
    maxLines: maxLines,
    overflow: TextOverflow.ellipsis,
    style: TextStyle(color: color, fontSize: 12, height: 1.3),
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
        style: TextStyle(color: _ink, fontWeight: FontWeight.w700),
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
            backgroundColor: const Color(0xffe3eaee),
            color: _accent,
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
    required this.jobs,
    required this.activity,
    required this.agent,
  });

  /// Jobs with the same outcome, newest first; the first speaks for all.
  final List<Map<String, dynamic>> jobs;
  final ToolActivity activity;
  final Agent agent;

  @override
  Widget build(BuildContext context) {
    final job = jobs.first;
    final id = '${job['id']}';
    final allTargets = [for (final j in jobs) ...ToolActivity.targetsOf(j)];
    final seen = <String>{};
    final failed = [
      for (final t in allTargets)
        if ((t['status'] == 'error' || t['status'] == 'cancelled') &&
            seen.add('${t['id']}|${t['error']}'))
          t,
    ];
    final isFile = (job['kind'] == 'file' || job['kind'] == 'update');
    final targets = ToolActivity.targetsOf(job);
    final skipped = targets.where((t) => t['status'] == 'skipped').length;
    final ok = targets.length - failed.length;
    final title = _finishedTitle(job, failed.isNotEmpty);
    return _Card(
      accent: failed.isEmpty ? _good : _warn,
      children: [
        _header(
          failed.isEmpty ? Icons.check_circle_outline : Icons.error_outline,
          jobs.length > 1 ? '$title  ×${jobs.length}' : title,
          color: failed.isEmpty ? _good : _warn,
          tooltip: 'Ẩn',
          onClose: () => activity.hideAll([for (final j in jobs) '${j['id']}']),
        ),
        if (isFile)
          _line(
            '$ok/${targets.length} máy'
            '${skipped > 0 ? ' · $skipped máy đã có sẵn' : ''}',
          ),
        for (final t in failed.take(3))
          _line(
            '${_who(activity, '${t['id']}')}: '
            '${_friendlyError('${t['error'] ?? ''}'.isEmpty ? '${t['status']}' : '${t['error']}')}',
            color: _warn,
            maxLines: 3,
          ),
        if (failed.length > 3)
          _line('+ ${failed.length - 3} máy nữa', color: _warn),
        if (isFile && failed.isNotEmpty && jobs.length == 1)
          Align(
            alignment: Alignment.centerRight,
            child: TextButton.icon(
              onPressed: () {
                activity.hide(id);
                agent.command('tool_retry', const [], {'id': id});
              },
              icon: const TIcon(Icons.refresh, size: 15),
              label: Text('Thử lại ${failed.length} máy'),
              style: TextButton.styleFrom(
                foregroundColor: _accent,
                visualDensity: VisualDensity.compact,
              ),
            ),
          ),
      ],
    );
  }
}

/// "name (ip)", or just the name when there is no separate address.
String _replier(StudentReply r) =>
    r.ip.isEmpty || r.ip == r.name ? r.name : '${r.name} (${r.ip})';

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
              ? 'Phản hồi từ ${_replier(items.first)}'
              : '${items.length} phản hồi mới',
          tooltip: 'Ẩn',
          onClose: activity.dismissToast,
        ),
        // One reply: the title already names the computer, so show only the text.
        if (items.length == 1)
          Padding(
            padding: const EdgeInsets.only(top: 6, right: 8),
            child: Text(
              items.first.text,
              maxLines: 4,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(color: _ink, fontSize: 13, height: 1.35),
            ),
          )
        else
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
                        color: _ink,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    TextSpan(text: r.text),
                  ],
                ),
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: _muted, fontSize: 12),
              ),
            ),
        if (items.length > 3) _line('+ ${items.length - 3} more'),
        Align(
          alignment: Alignment.centerRight,
          child: TextButton(
            onPressed: onOpen,
            style: TextButton.styleFrom(
              foregroundColor: _accent,
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
              icon: const TIcon(Icons.delete_sweep_outlined, size: 18),
              label: const Text('Xóa tất cả'),
            ),
          ),
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
                          leading: const TIcon(Icons.computer, size: 20),
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
          subtitle:
              'Các máy bị mất kết nối khi đang bật theo dõi hoạt động. Máy kết nối lại sẽ được cập nhật ngay trên dòng của nó.',
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
              icon: const TIcon(Icons.delete_sweep_outlined, size: 18),
              label: const Text('Xóa tất cả'),
            ),
          ),
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
                    final lostMs = (a['unix_ms'] as num?)?.toInt() ?? 0;
                    final backMs = (a['reconnected_ms'] as num?)?.toInt();
                    final back = backMs != null;
                    final color = back
                        ? const Color(0xff1f7a4d)
                        : const Color(0xffb83b40);
                    String at(int ms) =>
                        _time(DateTime.fromMillisecondsSinceEpoch(ms));
                    return ListTile(
                      dense: true,
                      isThreeLine: true,
                      leading: Tooltip(
                        message: back
                            ? 'Đã mất kết nối và đã kết nối lại'
                            : 'Đang mất kết nối',
                        child: TIcon(
                          back ? Icons.link : Icons.link_off,
                          size: 22,
                          color: color,
                        ),
                      ),
                      title: Row(
                        children: [
                          Flexible(
                            child: Text(
                              activity.nameOf(id),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ),
                          const SizedBox(width: 8),
                          Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 8,
                              vertical: 1,
                            ),
                            decoration: BoxDecoration(
                              color: color.withValues(alpha: 0.10),
                              borderRadius: BorderRadius.circular(10),
                            ),
                            child: Text(
                              back ? 'Đã kết nối lại' : 'Đang mất kết nối',
                              style: TextStyle(
                                fontSize: 11,
                                fontWeight: FontWeight.w600,
                                color: color,
                              ),
                            ),
                          ),
                        ],
                      ),
                      subtitle: Text(
                        [
                          activity.ipOf(id),
                          if (lostMs != 0) 'Mất kết nối lúc ${at(lostMs)}',
                          if (back)
                            'Kết nối lại lúc ${at(backMs)}'
                                '${lostMs == 0 ? '' : ' (sau ${disconnectGap(backMs - lostMs)})'}'
                          else
                            'Chưa kết nối lại',
                        ].join('\n'),
                      ),
                    );
                  },
                ),
        );
      },
    ),
  );
}

/// "5 giây", "2 phút 5 giây", "1 giờ 2 phút": how long a Student was away.
String disconnectGap(int milliseconds) {
  final seconds = (milliseconds / 1000).round();
  if (seconds < 1) return 'dưới 1 giây';
  if (seconds < 60) return '$seconds giây';
  final minutes = seconds ~/ 60;
  if (minutes < 60) {
    final rest = seconds % 60;
    return rest == 0 ? '$minutes phút' : '$minutes phút $rest giây';
  }
  final hours = minutes ~/ 60;
  final rest = minutes % 60;
  return rest == 0 ? '$hours giờ' : '$hours giờ $rest phút';
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
