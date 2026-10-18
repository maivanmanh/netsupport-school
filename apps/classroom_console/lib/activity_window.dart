import 'icon_tints.dart';
import 'dart:async';
import 'package:flutter/services.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'app_icons.dart';
import 'file_collection.dart' show formatBytes;

const _teal = Color(0xff156b79);
const _bg = Color(0xfff3f5f9);

class ActivityWindow extends StatefulWidget {
  const ActivityWindow({
    super.key,
    required this.agent,
    required this.studentId,
    required this.studentName,
  });
  final Agent agent;
  final String studentId, studentName;
  @override
  State<ActivityWindow> createState() => _ActivityWindowState();
}

class _ActivityWindowState extends State<ActivityWindow> {
  Map<String, dynamic>? report;
  String? error;
  bool loading = false; // no data at all yet — full-screen spinner
  bool syncing = false; // have (possibly stale) data — small indicator only
  bool autoRefresh = true;
  DateTime? fetchedAt;
  Timer? timer, freshTimer;
  int generation = 0;
  bool requestedOnce = false;

  /// When the Teacher last pressed Clear; history older than this (or than
  /// today's midnight, whichever is later) is not shown.
  int clearedMs = 0;

  /// Keys of every item in the previously shown report, so what appears in
  /// the next one can be flagged as new for a while.
  Set<String>? shownKeys;
  final Map<String, DateTime> freshAt = {};
  static const _freshFor = Duration(seconds: 25);

  Agent get agent => widget.agent;
  bool get online => agent.students.any(
    (s) => s['id'] == widget.studentId && s['online'] == true,
  );
  bool get monitoringOn =>
      (agent.state['tools'] as Map? ?? const {})['activity_monitoring'] == true;

  @override
  void initState() {
    super.initState();
    agent.addListener(onAgent);
    loadSaved();
    startRefreshOnce();
    timer = Timer.periodic(const Duration(seconds: 6), (_) {
      if (autoRefresh && !loading && !syncing && online) refresh();
    });
  }

  void onAgent() {
    if (!mounted) return;
    startRefreshOnce();
    setState(() {});
  }

  /// Refresh the moment the Teacher connection is up, not on the next timer
  /// tick.
  void startRefreshOnce() {
    if (requestedOnce || !agent.connected) return;
    requestedOnce = true;
    refresh();
  }

  /// Last report per Student, kept in the settings database for one day.
  Future<void> loadSaved() async {
    try {
      final all = await agent.storeList('activity');
      await purgeOldCaches(all);
      final saved = all[widget.studentId];
      if (!mounted || report != null || saved is! Map) return;
      final data = saved['report'];
      if (data is! Map) return;
      clearedMs = ((saved['cleared_ms'] as num?) ?? 0).toInt();
      setState(() {
        report = _clip(Map<String, dynamic>.from(data), cutoffMs);
        fetchedAt = DateTime.fromMillisecondsSinceEpoch(
          ((saved['saved_ms'] as num?) ?? 0).toInt(),
        );
        shownKeys = _keysOf(report!);
        loading = false;
      });
    } catch (_) {}
  }

  /// Saved activity is only kept for the day it was recorded.
  Future<void> purgeOldCaches(Map<String, dynamic> all) async {
    final midnight = _midnightMs();
    final expired = <String, dynamic>{
      for (final entry in all.entries)
        if (entry.value is! Map ||
            ((entry.value as Map)['saved_ms'] as num? ?? 0) < midnight)
          entry.key: null,
    };
    if (expired.isEmpty) return;
    all.removeWhere((key, _) => expired.containsKey(key));
    await agent.storeApply('activity', expired);
  }

  /// Nothing before this instant is shown: the last Clear, or the start of
  /// today if that is later.
  int get cutoffMs {
    final midnight = _midnightMs();
    return clearedMs > midnight ? clearedMs : midnight;
  }

  Future<void> clearHistory() async {
    final ok = await confirmDialog(
      context,
      title: 'Xóa lịch sử hoạt động?',
      icon: Icons.delete_sweep_outlined,
      confirmLabel: 'Xóa',
      destructive: true,
      message:
          'Cửa sổ và tab đã đóng, tệp đã tải và nội dung đã sao chép của ${widget.studentName} sẽ bị xóa khỏi màn hình này, và thời lượng bắt đầu tính lại từ bây giờ. Những gì đang mở vẫn được giữ trong danh sách.',
    );
    if (!ok || !mounted) return;
    setState(() {
      clearedMs = DateTime.now().millisecondsSinceEpoch;
      freshAt.clear();
      final current = report;
      if (current != null) {
        report = _clip(current, cutoffMs);
        shownKeys = _keysOf(report!);
        save(report!);
      }
    });
  }

  Future<void> save(Map<String, dynamic> data) async {
    try {
      if (data['monitoring_enabled'] != true) return;
      await agent.storeApply('activity', {
        widget.studentId: {
          'saved_ms': DateTime.now().millisecondsSinceEpoch,
          'cleared_ms': clearedMs,
          'report': data,
        },
      });
    } catch (_) {}
  }

  /// Adopts [data] as the shown report and flags what was not in the
  /// previous one.
  void adopt(Map<String, dynamic> data) {
    data = _clip(data, cutoffMs);
    final keys = _keysOf(data);
    final before = shownKeys;
    // The first report with content is the baseline: everything in it would
    // be "new", which says nothing. Only later additions are flagged.
    if (before != null && before.isNotEmpty) {
      final now = DateTime.now();
      for (final k in keys.difference(before)) {
        freshAt[k] = now;
      }
      if (freshAt.isNotEmpty) {
        freshTimer?.cancel();
        freshTimer = Timer(_freshFor + const Duration(seconds: 1), () {
          if (mounted) setState(() {});
        });
      }
    }
    shownKeys = keys;
    report = data;
    save(data);
  }

  bool isFresh(String key) {
    final at = freshAt[key];
    return at != null && DateTime.now().difference(at) < _freshFor;
  }

  @override
  void dispose() {
    timer?.cancel();
    freshTimer?.cancel();
    agent.removeListener(onAgent);
    super.dispose();
  }

  bool enabling = false;

  /// Switches monitoring on for the class and keeps the button busy until the
  /// first data actually arrives (Students need a few seconds to collect it).
  Future<void> enableMonitoring() async {
    if (enabling) return;
    setState(() {
      enabling = true;
      error = null;
    });
    final answer = await agent.request(
      'activity_monitoring_set',
      input: {'enabled': true},
    );
    if (!mounted) return;
    if (answer['ok'] != true) {
      setState(() {
        enabling = false;
        error = Agent.failure(answer);
      });
      return;
    }
    for (var attempt = 0; attempt < 12 && mounted; attempt++) {
      if (!syncing && !loading) await refresh();
      final data = report;
      if (data != null &&
          data['monitoring_enabled'] == true &&
          (data['apps'] as List? ?? const []).isNotEmpty) {
        break;
      }
      await Future<void>.delayed(const Duration(seconds: 2));
    }
    if (mounted) setState(() => enabling = false);
  }

  /// Shows a cached report instantly if the Teacher already has one, then
  /// (if the Student's background collector queued a fresher one) waits for
  /// that in the background without blocking the view that's already shown.
  Future<void> refresh() async {
    if (!online) return;
    final mine = ++generation;
    setState(() {
      loading = report == null;
      syncing = report != null;
      error = null;
    });
    final started = await agent.request(
      'activity_request',
      input: {'student': widget.studentId},
    );
    if (!mounted || mine != generation) return;
    if (started['ok'] != true) {
      setState(() {
        loading = false;
        syncing = false;
        error = Agent.failure(started);
      });
      return;
    }
    final cached = (started['result'] as Map?)?['cached'];
    if (cached is Map) {
      setState(() {
        loading = false;
        adopt(Map<String, dynamic>.from(cached));
        fetchedAt ??= DateTime.now();
      });
    }
    final id = started['job_id'];
    if (id == null) {
      // No background refresh was queued (e.g. monitoring is off): the
      // cached/sentinel value above is the whole answer.
      setState(() => syncing = false);
      return;
    }
    final deadline = DateTime.now().add(const Duration(seconds: 20));
    var attempt = 0;
    while (DateTime.now().isBefore(deadline)) {
      if (!mounted || mine != generation) return;
      await Future<void>.delayed(
        resultPollDelay(attempt++, const Duration(milliseconds: 250)),
      );
      final answer = await agent.request('fs_result', input: {'id': id});
      if (!mounted || mine != generation) return;
      if (answer['ok'] != true) {
        setState(() {
          loading = false;
          syncing = false;
          error = report == null ? Agent.failure(answer) : null;
        });
        return;
      }
      final body = answer['result'];
      if (body is Map && body['pending'] != true) {
        setState(() {
          loading = false;
          syncing = false;
          fetchedAt = DateTime.now();
          if (body['error'] != null) {
            if (report == null) error = '${body['error']}';
          } else {
            error = null;
            adopt(Map<String, dynamic>.from(body));
          }
        });
        return;
      }
    }
    if (!mounted || mine != generation) return;
    setState(() {
      loading = false;
      syncing = false;
      if (report == null) error = 'Máy không phản hồi kịp thời.';
    });
  }

  @override
  Widget build(BuildContext context) {
    final r = report;
    return Scaffold(
      backgroundColor: _bg,
      body: SafeArea(
        child: DefaultTabController(
          length: 4,
          child: Column(
            children: [
              if (!agent.connected || !online)
                MaterialBanner(
                  content: Text(
                    agent.connected
                        ? '${widget.studentName} đang offline. Hoạt động gần nhất được hiển thị bên dưới.'
                        : 'Đang kết nối lại với giảng viên…',
                  ),
                  actions: const [SizedBox.shrink()],
                ),
              _Header(
                name: widget.studentName,
                loading: loading || syncing,
                fetchedAt: fetchedAt,
                autoRefresh: autoRefresh,
                onAutoRefresh: (v) => setState(() => autoRefresh = v),
                onRefresh: online ? refresh : null,
                onClear: r == null ? null : clearHistory,
              ),
              Material(
                color: Colors.white,
                child: TabBar(
                  labelColor: _teal,
                  unselectedLabelColor: Colors.blueGrey,
                  indicatorColor: _teal,
                  tabs: [
                    Tab(
                      icon: const TIcon(Icons.window_outlined, size: 19),
                      text:
                          'Cửa sổ đang mở${r == null ? '' : ' (${(r['apps'] as List? ?? const []).length})'}',
                    ),
                    Tab(
                      icon: const TIcon(Icons.tab_outlined, size: 19),
                      text:
                          'Tab trình duyệt${r == null ? '' : ' (${(r['tabs'] as List? ?? const []).length})'}',
                    ),
                    Tab(
                      icon: const TIcon(Icons.download_outlined, size: 19),
                      text:
                          'Tải xuống hôm nay${r == null ? '' : ' (${_downloads(r).length})'}',
                    ),
                    Tab(
                      icon: const TIcon(Icons.content_paste_outlined, size: 19),
                      text:
                          'Clipboard hôm nay${r == null ? '' : ' (${_clipboard(r).length})'}',
                    ),
                  ],
                ),
              ),
              Expanded(
                child: r == null
                    ? _Placeholder(loading: loading, error: error)
                    : r['supported'] != true
                    ? const _Placeholder(
                        loading: false,
                        error: 'Theo dõi hoạt động chỉ có trên Windows.',
                      )
                    : r['monitoring_enabled'] != true ||
                          (enabling && _apps(r).isEmpty)
                    ? _MonitoringOff(
                        onEnable: enableMonitoring,
                        enabling: enabling,
                      )
                    : TabBarView(
                        children: [
                          _WindowsTab(apps: _apps(r), isFresh: isFresh),
                          _TabsTab(
                            browsers: (r['browsers'] as List? ?? const [])
                                .cast<Map>(),
                            tabs: (r['tabs'] as List? ?? const []).cast<Map>(),
                            icons: _iconsByApp(r),
                            isFresh: isFresh,
                          ),
                          _DownloadsTab(
                            downloads: _downloads(r),
                            isFresh: isFresh,
                          ),
                          _ClipboardTab(
                            entries: _clipboard(r),
                            isFresh: isFresh,
                            studentName: widget.studentName,
                          ),
                        ],
                      ),
              ),
              if (error != null && r != null)
                Container(
                  width: double.infinity,
                  color: Colors.orange.shade50,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 6,
                  ),
                  child: Text(error!, style: const TextStyle(fontSize: 12)),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

int _midnightMs() {
  final now = DateTime.now();
  return DateTime(now.year, now.month, now.day).millisecondsSinceEpoch;
}

/// [r] without anything that ended before [cutoff]; open items keep their
/// place but count their time from the cutoff.
Map<String, dynamic> _clip(Map<String, dynamic> r, int cutoff) {
  final now = DateTime.now().millisecondsSinceEpoch;
  List<Map> clipTracked(Object? list) {
    final out = <Map>[];
    for (final raw in (list as List? ?? const []).cast<Map>()) {
      final item = Map<String, dynamic>.from(raw);
      final open = item['open'] == true;
      final opened = ((item['opened_ms'] as num?) ?? 0).toInt();
      final closed = ((item['closed_ms'] as num?) ?? 0).toInt();
      if (!open && closed <= cutoff) continue;
      final total = ((item['total_open_ms'] as num?) ?? 0).toInt();
      if (open) {
        if (opened <= cutoff) item['reopened'] = 0;
        item['total_open_ms'] = now - (opened > cutoff ? opened : cutoff);
      } else {
        final within = closed - cutoff;
        item['total_open_ms'] = total < within ? total : within;
      }
      out.add(item);
    }
    return out;
  }

  if (r['monitoring_enabled'] != true) return r;
  return {
    ...r,
    'apps': clipTracked(r['apps']),
    'tabs': clipTracked(r['tabs']),
    'downloads': (r['downloads'] as List? ?? const []).cast<Map>().where((d) {
      return ((d['unix_ms'] as num?) ?? 0).toInt() > cutoff;
    }).toList(),
    'clipboard': (r['clipboard'] as List? ?? const []).cast<Map>().where((c) {
      return ((c['unix_ms'] as num?) ?? 0).toInt() > cutoff;
    }).toList(),
  };
}

@visibleForTesting
Map<String, dynamic> clipActivityReport(Map<String, dynamic> r, int cutoff) =>
    _clip(r, cutoff);

String _appKey(Map a) => 'app|${a['app']}|${a['open'] == true}';
String _tabKey(Map t, {required bool open}) => 'tab|$open|${t['title']}';
String _downloadKey(Map d) => 'dl|${d['name']}|${d['unix_ms']}';
String _clipboardKey(Map c) => 'cb|${c['unix_ms']}|${c['app']}';

/// Identity of every item in [r], used to spot what a newer report adds.
Set<String> _keysOf(Map<String, dynamic> r) => {
  for (final a in _apps(r)) _appKey(a),
  for (final b in (r['browsers'] as List? ?? const []).cast<Map>())
    for (final t in (b['tabs'] as List).cast<Map>()) _tabKey(t, open: true),
  for (final t in (r['tabs'] as List? ?? const []).cast<Map>())
    if (t['open'] != true) _tabKey(t, open: false),
  for (final d in _downloads(r)) _downloadKey(d),
  for (final c in _clipboard(r)) _clipboardKey(c),
};

List<Map> _apps(Map<String, dynamic> r) =>
    (r['apps'] as List? ?? const []).cast<Map>();

/// Process name → the Student's real icon for that program.
Map<String, String> _iconsByApp(Map<String, dynamic> r) => {
  for (final a in _apps(r))
    if ('${a['icon_png_base64'] ?? ''}'.isNotEmpty)
      '${a['app']}'.toLowerCase(): '${a['icon_png_base64']}',
};

/// Clipboard copies seen while monitoring was on today.
List<Map> _clipboard(Map<String, dynamic> r) {
  final now = DateTime.now();
  final midnight = DateTime(
    now.year,
    now.month,
    now.day,
  ).millisecondsSinceEpoch;
  return (r['clipboard'] as List? ?? const []).cast<Map>().where((c) {
    final ms = ((c['unix_ms'] as num?) ?? 0).toInt();
    return ms >= midnight;
  }).toList();
}

/// Downloads started today (Teacher's local day); older Students that still
/// send earlier history are filtered here as well.
List<Map> _downloads(Map<String, dynamic> r) {
  final now = DateTime.now();
  final midnight = DateTime(
    now.year,
    now.month,
    now.day,
  ).millisecondsSinceEpoch;
  return (r['downloads'] as List? ?? const []).cast<Map>().where((d) {
    final ms = ((d['unix_ms'] as num?) ?? 0).toInt();
    return ms >= midnight;
  }).toList();
}

/// Copies [text] to the Teacher's clipboard and confirms with a toast.
Future<void> _copy(BuildContext context, String text, String what) async {
  await Clipboard.setData(ClipboardData(text: text));
  if (!context.mounted) return;
  final messenger = ScaffoldMessenger.of(context);
  messenger
    ..clearSnackBars()
    ..showSnackBar(
      SnackBar(
        duration: const Duration(milliseconds: 1600),
        behavior: SnackBarBehavior.floating,
        width: 320,
        content: Row(
          children: [
            const TIcon(Icons.check_circle, color: Colors.white, size: 18),
            const SizedBox(width: 8),
            Expanded(child: Text('Đã sao chép $what')),
          ],
        ),
      ),
    );
}

// ------------------------------------------------------------------ Header

class _Header extends StatelessWidget {
  const _Header({
    required this.name,
    required this.loading,
    required this.fetchedAt,
    required this.autoRefresh,
    required this.onAutoRefresh,
    required this.onRefresh,
    required this.onClear,
  });
  final VoidCallback? onClear;
  final String name;
  final bool loading, autoRefresh;
  final DateTime? fetchedAt;
  final ValueChanged<bool> onAutoRefresh;
  final VoidCallback? onRefresh;
  @override
  Widget build(BuildContext context) => Material(
    color: Colors.white,
    child: Padding(
      padding: const EdgeInsets.fromLTRB(18, 12, 10, 10),
      child: Row(
        children: [
          Container(
            width: 34,
            height: 34,
            decoration: BoxDecoration(
              color: _teal.withValues(alpha: 0.1),
              borderRadius: BorderRadius.circular(10),
            ),
            child: const TIcon(
              Icons.monitor_heart_outlined,
              color: _teal,
              size: 20,
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  name,
                  style: const TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.w700,
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
                Text(
                  fetchedAt == null
                      ? 'Theo dõi hoạt động'
                      : 'Theo dõi hoạt động · cập nhật ${formatClock(fetchedAt!.millisecondsSinceEpoch)}',
                  style: const TextStyle(
                    fontSize: 11.5,
                    color: Colors.blueGrey,
                  ),
                ),
              ],
            ),
          ),
          Tooltip(
            message: autoRefresh
                ? 'Tự làm mới mỗi 6 giây'
                : 'Đã tạm dừng tự làm mới',
            child: IconButton(
              visualDensity: VisualDensity.compact,
              icon: TIcon(
                autoRefresh ? Icons.sync : Icons.sync_disabled,
                size: 19,
                color: autoRefresh ? _teal : Colors.blueGrey,
              ),
              onPressed: () => onAutoRefresh(!autoRefresh),
            ),
          ),
          IconButton(
            visualDensity: VisualDensity.compact,
            tooltip: 'Xóa lịch sử và tính lại từ đầu',
            icon: const TIcon(Icons.delete_sweep_outlined, size: 20),
            onPressed: onClear,
          ),
          IconButton(
            visualDensity: VisualDensity.compact,
            tooltip: 'Làm mới ngay',
            icon: loading
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const TIcon(Icons.refresh, size: 19),
            onPressed: loading ? null : onRefresh,
          ),
        ],
      ),
    ),
  );
}

class _MonitoringOff extends StatelessWidget {
  const _MonitoringOff({required this.onEnable, required this.enabling});
  final VoidCallback onEnable;
  final bool enabling;
  @override
  Widget build(BuildContext context) => Center(
    child: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const TIcon(
            Icons.visibility_off_outlined,
            size: 40,
            color: Colors.blueGrey,
          ),
          const SizedBox(height: 12),
          const Text(
            'Theo dõi hoạt động đang tắt cho cả lớp.',
            style: TextStyle(fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 4),
          const Text(
            'Hãy bật để học viên thu thập cửa sổ đang mở, tab trình duyệt và\ntệp tải xuống ở chế độ nền — không ảnh hưởng hiệu năng khi vẫn tắt.',
            textAlign: TextAlign.center,
            style: TextStyle(fontSize: 12, color: Colors.blueGrey),
          ),
          const SizedBox(height: 16),
          FilledButton.icon(
            onPressed: enabling ? null : onEnable,
            icon: enabling
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(
                      strokeWidth: 2,
                      color: Colors.white,
                    ),
                  )
                : const TIcon(Icons.visibility, size: 18),
            label: Text(
              enabling ? 'Đang thu thập từ học viên…' : 'Bật cho cả lớp',
            ),
          ),
          if (enabling)
            const Padding(
              padding: EdgeInsets.only(top: 10),
              child: Text(
                'Đã bật theo dõi — đang chờ báo cáo đầu tiên.',
                style: TextStyle(fontSize: 12, color: Colors.blueGrey),
              ),
            ),
        ],
      ),
    ),
  );
}

class _Placeholder extends StatelessWidget {
  const _Placeholder({required this.loading, required this.error});
  final bool loading;
  final String? error;
  @override
  Widget build(BuildContext context) {
    if (error != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Text(
            error!,
            textAlign: TextAlign.center,
            style: const TextStyle(color: Color(0xffb83b40)),
          ),
        ),
      );
    }
    return const Center(child: CircularProgressIndicator());
  }
}

// ------------------------------------------------------- Shared list pieces

const _open = Color(0xff1f7a4d);
const _closed = Color(0xff8a97a3);
const _line = Color(0xffe6eaee);

class _Badge extends StatelessWidget {
  const _Badge(this.text, {required this.color});
  final String text;
  final Color color;
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
    decoration: BoxDecoration(
      color: color.withValues(alpha: 0.12),
      borderRadius: BorderRadius.circular(20),
    ),
    child: Text(
      text,
      style: TextStyle(
        fontSize: 10.5,
        color: color,
        fontWeight: FontWeight.w600,
      ),
    ),
  );
}

/// A titled card holding a list of rows separated by hairlines.
class _Section extends StatelessWidget {
  const _Section({
    required this.title,
    required this.count,
    required this.children,
    this.empty,
    this.leading,
  });
  final String title;
  final int count;
  final List<Widget> children;
  final String? empty;
  final Widget? leading;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 14, 16, 0),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 0, 4, 8),
          child: Row(
            children: [
              if (leading != null) ...[leading!, const SizedBox(width: 8)],
              Text(
                title.toUpperCase(),
                style: const TextStyle(
                  fontSize: 11.5,
                  fontWeight: FontWeight.w700,
                  color: Colors.blueGrey,
                  letterSpacing: 0.6,
                ),
              ),
              const SizedBox(width: 8),
              _Badge('$count', color: _teal),
            ],
          ),
        ),
        Container(
          clipBehavior: Clip.antiAlias,
          decoration: BoxDecoration(
            color: Colors.white,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: _line),
          ),
          child: children.isEmpty
              ? Padding(
                  padding: const EdgeInsets.all(16),
                  child: Text(
                    empty ?? 'Chưa có gì.',
                    style: const TextStyle(
                      fontSize: 12.5,
                      color: Colors.blueGrey,
                    ),
                  ),
                )
              : Column(
                  children: [
                    for (var i = 0; i < children.length; i++) ...[
                      if (i > 0) const Divider(height: 1, color: _line),
                      children[i],
                    ],
                  ],
                ),
        ),
      ],
    ),
  );
}

/// One clickable list row: [leading] · title/subtitle · [trailing]. Click
/// copies [copyText]; a copy glyph appears on hover.
class _Row extends StatefulWidget {
  const _Row({
    required this.leading,
    required this.title,
    required this.copyText,
    required this.copyLabel,
    this.subtitle,
    this.trailing,
    this.boldTitle = false,
    this.fresh = false,
    this.onTap,
  });
  final Widget leading;
  final String title;
  final String? subtitle;
  final Widget? trailing;
  final String copyText, copyLabel;
  final bool boldTitle, fresh;
  final VoidCallback? onTap;
  @override
  State<_Row> createState() => _RowState();
}

class _RowState extends State<_Row> {
  bool hover = false;
  @override
  Widget build(BuildContext context) {
    final sub = widget.subtitle;
    return MouseRegion(
      onEnter: (_) => setState(() => hover = true),
      onExit: (_) => setState(() => hover = false),
      cursor: SystemMouseCursors.click,
      child: Tooltip(
        message: 'Bấm để sao chép',
        waitDuration: const Duration(milliseconds: 700),
        child: InkWell(
          onTap:
              widget.onTap ??
              () => _copy(context, widget.copyText, widget.copyLabel),
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 600),
            color: widget.fresh
                ? const Color(0xffe3f4ee)
                : hover
                ? _teal.withValues(alpha: 0.05)
                : Colors.transparent,
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
            child: Row(
              children: [
                SizedBox(
                  width: 30,
                  height: 30,
                  child: Center(child: widget.leading),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        widget.title.isEmpty ? '(untitled)' : widget.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 13,
                          fontWeight: widget.boldTitle
                              ? FontWeight.w700
                              : FontWeight.w600,
                        ),
                      ),
                      if (sub != null && sub.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 1),
                          child: Text(
                            sub,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontSize: 11.5,
                              color: Colors.blueGrey,
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
                if (widget.fresh) ...[
                  const _Badge('NEW', color: _open),
                  const SizedBox(width: 8),
                ],
                if (widget.trailing != null) ...[
                  const SizedBox(width: 10),
                  widget.trailing!,
                ],
                const SizedBox(width: 8),
                AnimatedOpacity(
                  duration: const Duration(milliseconds: 120),
                  opacity: hover ? 1 : 0,
                  child: const TIcon(
                    Icons.copy_rounded,
                    size: 15,
                    color: _teal,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Note extends StatelessWidget {
  const _Note(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(20, 14, 20, 18),
    child: Text(
      text,
      style: const TextStyle(
        fontSize: 11,
        color: Colors.blueGrey,
        fontStyle: FontStyle.italic,
      ),
    ),
  );
}

Widget _statusBadges(Map a) {
  final open = a['open'] == true;
  final reopened = (a['reopened'] as num?)?.toInt() ?? 0;
  final total = formatDuration((a['total_open_ms'] as num?)?.toInt() ?? 0);
  return Wrap(
    spacing: 5,
    runSpacing: 4,
    crossAxisAlignment: WrapCrossAlignment.center,
    children: [
      if (reopened > 0) _Badge('×${reopened + 1}', color: _teal),
      _Badge(
        open
            ? 'mở $total'
            : 'đóng ${formatClock((a['closed_ms'] as num?)?.toInt() ?? 0)} · $total',
        color: open ? _open : _closed,
      ),
    ],
  );
}

/// Filter box shared by the three lists.
class _Search extends StatelessWidget {
  const _Search({required this.hint, required this.onChanged});
  final String hint;
  final ValueChanged<String> onChanged;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
    child: SizedBox(
      height: 38,
      child: TextField(
        onChanged: onChanged,
        style: const TextStyle(fontSize: 13),
        decoration: InputDecoration(
          hintText: hint,
          prefixIcon: const TIcon(Icons.search, size: 18),
          filled: true,
          fillColor: Colors.white,
          contentPadding: EdgeInsets.zero,
          border: OutlineInputBorder(
            borderRadius: BorderRadius.circular(10),
            borderSide: const BorderSide(color: _line),
          ),
          enabledBorder: OutlineInputBorder(
            borderRadius: BorderRadius.circular(10),
            borderSide: const BorderSide(color: _line),
          ),
        ),
      ),
    ),
  );
}

bool _matches(String query, Iterable<Object?> fields) {
  final q = query.trim().toLowerCase();
  if (q.isEmpty) return true;
  return fields.any((f) => '$f'.toLowerCase().contains(q));
}

// ------------------------------------------------------------ Open windows

class _WindowsTab extends StatefulWidget {
  const _WindowsTab({required this.apps, required this.isFresh});
  final List<Map> apps;
  final bool Function(String key) isFresh;
  @override
  State<_WindowsTab> createState() => _WindowsTabState();
}

class _WindowsTabState extends State<_WindowsTab> {
  String query = '';
  @override
  Widget build(BuildContext context) {
    if (widget.apps.isEmpty) {
      return const Center(
        child: Text(
          'Chưa ghi nhận cửa sổ ứng dụng nào.',
          style: TextStyle(color: Colors.blueGrey),
        ),
      );
    }
    final shown = widget.apps
        .where(
          (a) => _matches(query, [
            lookAppUp('${a['app']}').name,
            a['app'],
            a['title'],
          ]),
        )
        .toList();
    final open = shown.where((a) => a['open'] == true).toList();
    final closed = shown.where((a) => a['open'] != true).toList();
    return ListView(
      children: [
        _Search(
          hint: 'Lọc cửa sổ',
          onChanged: (v) => setState(() => query = v),
        ),
        _Section(
          title: 'Đang mở',
          count: open.length,
          empty: 'Hiện không có gì đang mở.',
          children: [for (final a in open) _appRow(a)],
        ),
        _Section(
          title: 'Đã đóng',
          count: closed.length,
          empty: 'Chưa có gì đóng trong phiên này.',
          children: [for (final a in closed) _appRow(a)],
        ),
        const _Note(
          'Lịch sử chỉ được ghi khi đang bật theo dõi — có thể có khoảng trống. Bấm một dòng để sao chép.',
        ),
      ],
    );
  }

  Widget _appRow(Map a) {
    final name = lookAppUp('${a['app']}').name;
    return _Row(
      leading: AppBadge(
        app: '${a['app']}',
        size: 28,
        icon: a['icon_png_base64'] as String?,
      ),
      title: name,
      subtitle: '${a['title']}',
      trailing: _statusBadges(a),
      fresh: widget.isFresh(_appKey(a)),
      copyText: '$name\n${a['title']}',
      copyLabel: 'tiêu đề cửa sổ',
    );
  }
}

// ------------------------------------------------------------ Browser tabs

class _TabsTab extends StatefulWidget {
  const _TabsTab({
    required this.browsers,
    required this.tabs,
    required this.icons,
    required this.isFresh,
  });
  final bool Function(String key) isFresh;
  final List<Map> browsers, tabs;
  final Map<String, String> icons;
  @override
  State<_TabsTab> createState() => _TabsTabState();
}

class _TabsTabState extends State<_TabsTab> {
  String query = '';
  @override
  Widget build(BuildContext context) {
    final history = widget.tabs
        .where((t) => t['open'] != true)
        .where((t) => _matches(query, [t['title'], t['url']]))
        .toList();
    return ListView(
      children: [
        _Search(
          hint: 'Lọc tab theo tiêu đề hoặc địa chỉ',
          onChanged: (v) => setState(() => query = v),
        ),
        if (widget.browsers.isEmpty)
          const _Section(
            title: 'Đang mở',
            count: 0,
            empty: 'Không có cửa sổ trình duyệt nào đang mở.',
            children: [],
          )
        else
          for (final b in widget.browsers) _browserSection(b),
        _Section(
          title: 'Lịch sử · tab đã đóng',
          count: history.length,
          empty: 'Chưa ghi nhận tab nào đã đóng trong phiên này.',
          children: [
            for (final t in history) _tabRow(t, status: t, open: false),
          ],
        ),
        const _Note(
          'Địa chỉ tab lấy từ tab đang hiển thị và lịch sử trình duyệt, nên tab nền chưa từng xem '
          'có thể không có địa chỉ. Bấm một dòng để sao chép tiêu đề và địa chỉ.',
        ),
      ],
    );
  }

  Widget _browserSection(Map b) {
    final app = '${b['app']}';
    final tabs = (b['tabs'] as List).cast<Map>().where(
      (t) => _matches(query, [t['title'], t['url']]),
    );
    return _Section(
      title: '${lookAppUp(app).name} · open now',
      count: tabs.length,
      leading: AppBadge(
        app: app,
        size: 20,
        icon: widget.icons[app.toLowerCase()],
      ),
      empty: 'Không có tab nào khớp bộ lọc.',
      children: [for (final t in tabs) _tabRow(t, active: t['active'] == true)],
    );
  }

  Widget _tabRow(Map t, {bool active = false, Map? status, bool open = true}) {
    final title = '${t['title']}';
    final url = '${t['url'] ?? ''}';
    return _Row(
      leading: SiteIcon(
        favicon: t['favicon_png_base64'] as String?,
        url: t['url'] as String?,
        size: 22,
      ),
      title: title,
      boldTitle: active,
      fresh: widget.isFresh(_tabKey(t, open: open)),
      subtitle: url.isEmpty ? 'Không có địa chỉ' : url,
      trailing: status != null
          ? _statusBadges(status)
          : active
          ? const _Badge('active', color: _open)
          : null,
      copyText: url.isEmpty ? title : '$title\n$url',
      copyLabel: url.isEmpty ? 'tiêu đề tab' : 'tiêu đề và địa chỉ tab',
    );
  }
}

// -------------------------------------------------------------- Downloads

class _DownloadsTab extends StatefulWidget {
  const _DownloadsTab({required this.downloads, required this.isFresh});
  final List<Map> downloads;
  final bool Function(String key) isFresh;
  @override
  State<_DownloadsTab> createState() => _DownloadsTabState();
}

class _DownloadsTabState extends State<_DownloadsTab> {
  String query = '';
  @override
  Widget build(BuildContext context) {
    if (widget.downloads.isEmpty) {
      return const Center(
        child: Text(
          'Hôm nay chưa có tệp tải xuống.',
          style: TextStyle(color: Colors.blueGrey),
        ),
      );
    }
    final shown = widget.downloads
        .where((d) => _matches(query, [d['name'], d['url'], d['browser']]))
        .toList();
    return ListView(
      children: [
        _Search(
          hint: 'Lọc tệp tải xuống',
          onChanged: (v) => setState(() => query = v),
        ),
        _Section(
          title: 'Đã tải hôm nay',
          count: shown.length,
          empty: 'Không có tệp tải xuống nào khớp bộ lọc.',
          children: [
            for (final d in shown)
              () {
                final (icon, color) = fileTypeLook('${d['name']}');
                final url = '${d['url'] ?? ''}';
                final source = url.isEmpty
                    ? ''
                    : (Uri.tryParse(url)?.host ?? url);
                final time = formatClock(((d['unix_ms'] as num?) ?? 0).toInt());
                return _Row(
                  leading: Container(
                    width: 30,
                    height: 30,
                    decoration: BoxDecoration(
                      color: color.withValues(alpha: 0.12),
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: TIcon(icon, size: 18, color: color),
                  ),
                  title: '${d['name']}',
                  fresh: widget.isFresh(_downloadKey(d)),
                  subtitle: [
                    if (source.isNotEmpty) 'từ $source',
                    '${d['browser']}',
                    time,
                  ].where((s) => s.isNotEmpty).join(' · '),
                  trailing: Text(
                    formatBytes((d['bytes'] as num?) ?? 0),
                    style: const TextStyle(
                      fontSize: 11.5,
                      color: Colors.blueGrey,
                    ),
                  ),
                  copyText: url.isEmpty ? '${d['name']}' : '${d['name']}\n$url',
                  copyLabel: 'tên tệp và nguồn tải',
                );
              }(),
          ],
        ),
      ],
    );
  }
}

// ------------------------------------------------------------- Clipboard

/// Common code punctuation/keywords, scored against length and line count:
/// cheap enough to run on every row, and only wrong in ambiguous cases where
/// monospace is a harmless default anyway.
bool looksLikeCode(String text) {
  final trimmed = text.trim();
  if (trimmed.isEmpty) return false;
  const keywords = [
    'function',
    'def ',
    'class ',
    'return',
    'import ',
    'public ',
    'private ',
    'static ',
    'const ',
    'let ',
    'var ',
    'void ',
    'int ',
    'String',
    'println',
    'console.',
    '#include',
    'namespace',
    'if(',
    'if (',
    'for(',
    'for (',
    'while(',
    '=>',
    '</',
    '<div',
    'SELECT ',
    'FROM ',
  ];
  final lower = trimmed.toLowerCase();
  final keywordHits = keywords
      .where((k) => lower.contains(k.toLowerCase()))
      .length;
  final symbols = RegExp(r'[{}();=<>\[\]]').allMatches(trimmed).length;
  final lines = trimmed.split('\n');
  final indented = lines
      .where((l) => l.startsWith('  ') || l.startsWith('\t'))
      .length;
  final symbolDensity = symbols / trimmed.length;
  return keywordHits >= 2 ||
      symbolDensity > 0.04 ||
      (lines.length > 2 && indented > lines.length / 3);
}

const _codeKeyword = Color(0xff9553e9);
const _codeString = Color(0xff1f7a4d);
const _codeComment = Color(0xff8a97a3);
const _codeNumber = Color(0xffb56b1c);

final _codeKeywords = {
  'if',
  'else',
  'for',
  'while',
  'do',
  'switch',
  'case',
  'break',
  'continue',
  'return',
  'function',
  'def',
  'class',
  'struct',
  'enum',
  'interface',
  'public',
  'private',
  'protected',
  'static',
  'final',
  'const',
  'let',
  'var',
  'void',
  'int',
  'float',
  'double',
  'bool',
  'boolean',
  'string',
  'char',
  'new',
  'this',
  'self',
  'super',
  'import',
  'from',
  'export',
  'package',
  'namespace',
  'using',
  'include',
  'true',
  'false',
  'null',
  'none',
  'nil',
  'try',
  'catch',
  'finally',
  'throw',
  'throws',
  'async',
  'await',
  'fn',
  'impl',
  'pub',
  'mut',
  'select',
  'insert',
  'update',
  'delete',
  'where',
};

/// A naive, language-agnostic tokenizer good enough to tell a reviewer
/// "this is code" at a glance: strings, comments, numbers and common
/// keywords get a colour, everything else stays plain.
List<TextSpan> highlightCode(String text) {
  final spans = <TextSpan>[];
  final pattern = RegExp(
    r'(//[^\n]*)' // line comment
    r"|(#[^\n]*)" // shell/python comment
    r'|(/\*[\s\S]*?\*/)' // block comment
    r'''|("(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`(?:[^`\\]|\\.)*`)''' // strings
    r'|(\b\d+\.?\d*\b)' // numbers
    r'|([A-Za-z_][A-Za-z0-9_]*)', // identifiers/keywords
  );
  var last = 0;
  for (final m in pattern.allMatches(text)) {
    if (m.start > last) {
      spans.add(TextSpan(text: text.substring(last, m.start)));
    }
    final chunk = m.group(0)!;
    Color? color;
    if (m.group(1) != null || m.group(2) != null || m.group(3) != null) {
      color = _codeComment;
    } else if (m.group(4) != null) {
      color = _codeString;
    } else if (m.group(5) != null) {
      color = _codeNumber;
    } else if (m.group(6) != null &&
        _codeKeywords.contains(chunk.toLowerCase())) {
      color = _codeKeyword;
    }
    spans.add(
      TextSpan(
        text: chunk,
        style: color == null
            ? null
            : TextStyle(color: color, fontWeight: FontWeight.w600),
      ),
    );
    last = m.end;
  }
  if (last < text.length) spans.add(TextSpan(text: text.substring(last)));
  return spans;
}

class _ClipboardTab extends StatefulWidget {
  const _ClipboardTab({
    required this.entries,
    required this.isFresh,
    required this.studentName,
  });
  final List<Map> entries;
  final bool Function(String key) isFresh;
  final String studentName;
  @override
  State<_ClipboardTab> createState() => _ClipboardTabState();
}

class _ClipboardTabState extends State<_ClipboardTab> {
  String query = '';
  @override
  Widget build(BuildContext context) {
    if (widget.entries.isEmpty) {
      return const Center(
        child: Text(
          'Hôm nay chưa có nội dung nào được sao chép khi đang theo dõi.',
          style: TextStyle(color: Colors.blueGrey),
        ),
      );
    }
    final shown = widget.entries
        .where((c) => _matches(query, [c['text'], c['app']]))
        .toList();
    return ListView(
      children: [
        _Search(
          hint: 'Lọc nội dung đã sao chép',
          onChanged: (v) => setState(() => query = v),
        ),
        _Section(
          title: 'Đã sao chép hôm nay',
          count: shown.length,
          empty: 'Không có nội dung nào khớp bộ lọc.',
          children: [for (final c in shown) _row(c)],
        ),
        const _Note(
          'Chỉ ghi khi đang bật theo dõi hoạt động, và chỉ là văn bản thuần được chủ động sao chép — '
          'không bao giờ ghi phím. Bấm một dòng để đọc toàn bộ nội dung.',
        ),
      ],
    );
  }

  Widget _row(Map c) {
    final text = '${c['text']}';
    final firstLine = text
        .split('\n')
        .map((l) => l.trim())
        .firstWhere((l) => l.isNotEmpty, orElse: () => text);
    final code = looksLikeCode(text);
    final lineCount = text.split('\n').length;
    final duplicates = ((c['duplicates'] as num?) ?? 0).toInt();
    return _Row(
      leading: TIcon(
        code ? Icons.code : Icons.content_paste,
        size: 18,
        color: code ? _codeKeyword : Colors.blueGrey,
      ),
      title: firstLine,
      fresh: widget.isFresh(_clipboardKey(c)),
      trailing: duplicates > 0
          ? _Badge('×${duplicates + 1}', color: _teal)
          : null,
      subtitle: [
        if ('${c['app']}'.isNotEmpty) lookAppUp('${c['app']}').name,
        if (lineCount > 1) '$lineCount lines',
        if (c['truncated'] == true) 'truncated',
        formatClock(((c['unix_ms'] as num?) ?? 0).toInt()),
      ].join(' · '),
      copyText: text,
      copyLabel: 'nội dung',
      onTap: () => _showDetail(c),
    );
  }

  void _showDetail(Map c) {
    final text = '${c['text']}';
    showDialog<void>(
      context: context,
      builder: (context) => _ClipboardDetail(
        text: text,
        app: '${c['app']}',
        unixMs: ((c['unix_ms'] as num?) ?? 0).toInt(),
        truncated: c['truncated'] == true,
        duplicates: ((c['duplicates'] as num?) ?? 0).toInt(),
        studentName: widget.studentName,
      ),
    );
  }
}

class _ClipboardDetail extends StatelessWidget {
  const _ClipboardDetail({
    required this.text,
    required this.app,
    required this.unixMs,
    required this.truncated,
    required this.duplicates,
    required this.studentName,
  });
  final String text, app, studentName;
  final int duplicates;
  final int unixMs;
  final bool truncated;
  @override
  Widget build(BuildContext context) {
    final code = looksLikeCode(text);
    return AppDialog(
      icon: code ? Icons.code : Icons.content_paste,
      title: 'Nội dung $studentName đã sao chép',
      subtitle: [
        if (app.isNotEmpty) 'từ ${lookAppUp(app).name}',
        formatClock(unixMs),
        if (truncated) 'đã cắt ở giới hạn ghi nhận',
        if (duplicates > 0) 'sao chép ${duplicates + 1} lần',
      ].join(' · '),
      width: 720,
      height: 380,
      scrollable: false,
      actions: [
        AppDialog.primary(
          onPressed: () => _copy(context, text, 'nội dung'),
          icon: Icons.copy_rounded,
          label: 'Sao chép',
        ),
      ],
      child: Container(
        width: double.infinity,
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: const Color(0xfff6f7f9),
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: _line),
        ),
        child: SingleChildScrollView(
          child: SelectableText.rich(
            TextSpan(
              style: TextStyle(
                fontSize: 12.5,
                height: 1.45,
                fontFamily: code ? 'monospace' : null,
                color: Colors.black87,
              ),
              children: code ? highlightCode(text) : [TextSpan(text: text)],
            ),
          ),
        ),
      ),
    );
  }
}
