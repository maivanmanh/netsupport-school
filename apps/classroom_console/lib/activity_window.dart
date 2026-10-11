import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/services.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
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

  File? get savedFile {
    final dir = agent.dataDirectory;
    if (dir == null) return null;
    final safe = widget.studentId.replaceAll(RegExp(r'[^A-Za-z0-9._-]'), '_');
    return File('${dir.path}/activity-cache/$safe.json');
  }

  /// Shows the last report from a previous session straight away (before the
  /// Teacher connection or the Student has answered); the refresh that
  /// follows then flags whatever is new compared with it.
  Future<void> loadSaved() async {
    for (var i = 0; i < 40 && agent.dataDirectory == null && mounted; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    try {
      final file = savedFile;
      if (file == null) return;
      await purgeOldCaches(file.parent);
      if (!await file.exists()) return;
      final saved = jsonDecode(await file.readAsString());
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
  Future<void> purgeOldCaches(Directory dir) async {
    if (!await dir.exists()) return;
    final midnight = _midnightMs();
    await for (final entry in dir.list()) {
      if (entry is! File) continue;
      try {
        if ((await entry.lastModified()).millisecondsSinceEpoch < midnight) {
          await entry.delete();
        }
      } catch (_) {}
    }
  }

  /// Nothing before this instant is shown: the last Clear, or the start of
  /// today if that is later.
  int get cutoffMs {
    final midnight = _midnightMs();
    return clearedMs > midnight ? clearedMs : midnight;
  }

  Future<void> clearHistory() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Clear activity history?'),
        content: Text(
          'Closed windows, closed tabs and downloads recorded so far for ${widget.studentName} '
          'are removed from this view, and durations start counting again from now. '
          'Things that are open right now stay listed.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Clear'),
          ),
        ],
      ),
    );
    if (ok != true || !mounted) return;
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
      final file = savedFile;
      if (file == null || data['monitoring_enabled'] != true) return;
      await file.parent.create(recursive: true);
      await file.writeAsString(
        jsonEncode({
          'saved_ms': DateTime.now().millisecondsSinceEpoch,
          'cleared_ms': clearedMs,
          'report': data,
        }),
      );
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
    while (DateTime.now().isBefore(deadline)) {
      if (!mounted || mine != generation) return;
      await Future<void>.delayed(const Duration(milliseconds: 250));
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
      if (report == null) error = 'The computer did not answer in time.';
    });
  }

  @override
  Widget build(BuildContext context) {
    final r = report;
    return Scaffold(
      backgroundColor: _bg,
      body: SafeArea(
        child: DefaultTabController(
          length: 3,
          child: Column(
            children: [
              if (!agent.connected || !online)
                MaterialBanner(
                  content: Text(
                    agent.connected
                        ? '${widget.studentName} is offline. The last known activity is shown below.'
                        : 'Reconnecting to the Teacher…',
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
                      icon: const Icon(Icons.window_outlined, size: 19),
                      text:
                          'Open windows${r == null ? '' : ' (${(r['apps'] as List? ?? const []).length})'}',
                    ),
                    Tab(
                      icon: const Icon(Icons.tab_outlined, size: 19),
                      text:
                          'Browser tabs${r == null ? '' : ' (${(r['tabs'] as List? ?? const []).length})'}',
                    ),
                    Tab(
                      icon: const Icon(Icons.download_outlined, size: 19),
                      text:
                          'Downloads today${r == null ? '' : ' (${_downloads(r).length})'}',
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
                        error:
                            'Activity monitoring is only available on Windows.',
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
  };
}

@visibleForTesting
Map<String, dynamic> clipActivityReport(Map<String, dynamic> r, int cutoff) =>
    _clip(r, cutoff);

String _appKey(Map a) => 'app|${a['app']}|${a['open'] == true}';
String _tabKey(Map t, {required bool open}) => 'tab|$open|${t['title']}';
String _downloadKey(Map d) => 'dl|${d['name']}|${d['unix_ms']}';

/// Identity of every item in [r], used to spot what a newer report adds.
Set<String> _keysOf(Map<String, dynamic> r) => {
  for (final a in _apps(r)) _appKey(a),
  for (final b in (r['browsers'] as List? ?? const []).cast<Map>())
    for (final t in (b['tabs'] as List).cast<Map>()) _tabKey(t, open: true),
  for (final t in (r['tabs'] as List? ?? const []).cast<Map>())
    if (t['open'] != true) _tabKey(t, open: false),
  for (final d in _downloads(r)) _downloadKey(d),
};

List<Map> _apps(Map<String, dynamic> r) =>
    (r['apps'] as List? ?? const []).cast<Map>();

/// Process name → the Student's real icon for that program.
Map<String, String> _iconsByApp(Map<String, dynamic> r) => {
  for (final a in _apps(r))
    if ('${a['icon_png_base64'] ?? ''}'.isNotEmpty)
      '${a['app']}'.toLowerCase(): '${a['icon_png_base64']}',
};

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
            const Icon(Icons.check_circle, color: Colors.white, size: 18),
            const SizedBox(width: 8),
            Expanded(child: Text('Copied $what')),
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
            child: const Icon(
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
                      ? 'Activity monitor'
                      : 'Activity monitor · updated ${formatClock(fetchedAt!.millisecondsSinceEpoch)}',
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
                ? 'Auto-refreshing every 6s'
                : 'Auto-refresh paused',
            child: IconButton(
              visualDensity: VisualDensity.compact,
              icon: Icon(
                autoRefresh ? Icons.sync : Icons.sync_disabled,
                size: 19,
                color: autoRefresh ? _teal : Colors.blueGrey,
              ),
              onPressed: () => onAutoRefresh(!autoRefresh),
            ),
          ),
          IconButton(
            visualDensity: VisualDensity.compact,
            tooltip: 'Clear history and start counting again',
            icon: const Icon(Icons.delete_sweep_outlined, size: 20),
            onPressed: onClear,
          ),
          IconButton(
            visualDensity: VisualDensity.compact,
            tooltip: 'Refresh now',
            icon: loading
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.refresh, size: 19),
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
          const Icon(
            Icons.visibility_off_outlined,
            size: 40,
            color: Colors.blueGrey,
          ),
          const SizedBox(height: 12),
          const Text(
            'Activity monitoring is off for the class.',
            style: TextStyle(fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 4),
          const Text(
            'Turn it on so Students collect open windows, browser tabs and\ndownloads in the background — no effect on performance while it stays off.',
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
                : const Icon(Icons.visibility, size: 18),
            label: Text(
              enabling ? 'Collecting from Students…' : 'Turn on for the class',
            ),
          ),
          if (enabling)
            const Padding(
              padding: EdgeInsets.only(top: 10),
              child: Text(
                'Monitoring is on — waiting for the first report.',
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
                    empty ?? 'Nothing here.',
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
  });
  final Widget leading;
  final String title;
  final String? subtitle;
  final Widget? trailing;
  final String copyText, copyLabel;
  final bool boldTitle, fresh;
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
        message: 'Click to copy',
        waitDuration: const Duration(milliseconds: 700),
        child: InkWell(
          onTap: () => _copy(context, widget.copyText, widget.copyLabel),
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
                  child: const Icon(Icons.copy_rounded, size: 15, color: _teal),
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
            ? 'open $total'
            : 'closed ${formatClock((a['closed_ms'] as num?)?.toInt() ?? 0)} · $total',
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
          prefixIcon: const Icon(Icons.search, size: 18),
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
          'No application windows observed yet.',
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
          hint: 'Filter windows',
          onChanged: (v) => setState(() => query = v),
        ),
        _Section(
          title: 'Open now',
          count: open.length,
          empty: 'Nothing currently open.',
          children: [for (final a in open) _appRow(a)],
        ),
        _Section(
          title: 'Closed',
          count: closed.length,
          empty: 'Nothing closed yet this session.',
          children: [for (final a in closed) _appRow(a)],
        ),
        const _Note(
          'History is recorded only while monitoring is on — there may be gaps. Click any row to copy it.',
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
      copyLabel: 'window title',
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
          hint: 'Filter tabs by title or address',
          onChanged: (v) => setState(() => query = v),
        ),
        if (widget.browsers.isEmpty)
          const _Section(
            title: 'Open now',
            count: 0,
            empty: 'No browser window is open.',
            children: [],
          )
        else
          for (final b in widget.browsers) _browserSection(b),
        _Section(
          title: 'History · closed tabs',
          count: history.length,
          empty: 'No closed tabs recorded yet this session.',
          children: [
            for (final t in history) _tabRow(t, status: t, open: false),
          ],
        ),
        const _Note(
          'Tab addresses come from the active tab and the browser history, so a background tab '
          'that was never visited may show no address. Click any row to copy its title and address.',
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
      empty: 'No tabs match the filter.',
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
      subtitle: url.isEmpty ? 'Address not available' : url,
      trailing: status != null
          ? _statusBadges(status)
          : active
          ? const _Badge('active', color: _open)
          : null,
      copyText: url.isEmpty ? title : '$title\n$url',
      copyLabel: url.isEmpty ? 'tab title' : 'tab title and address',
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
          'No browser downloads today.',
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
          hint: 'Filter downloads',
          onChanged: (v) => setState(() => query = v),
        ),
        _Section(
          title: 'Downloaded today',
          count: shown.length,
          empty: 'No downloads match the filter.',
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
                    child: Icon(icon, size: 18, color: color),
                  ),
                  title: '${d['name']}',
                  fresh: widget.isFresh(_downloadKey(d)),
                  subtitle: [
                    if (source.isNotEmpty) 'from $source',
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
                  copyLabel: 'download name and source',
                );
              }(),
          ],
        ),
      ],
    );
  }
}
