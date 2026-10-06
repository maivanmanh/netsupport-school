import 'dart:async';
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
  Timer? timer;
  int generation = 0;

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
    if (agent.connected) {
      refresh();
    }
    timer = Timer.periodic(const Duration(seconds: 6), (_) {
      if (autoRefresh && !loading && !syncing && online) refresh();
    });
  }

  void onAgent() {
    if (!mounted) return;
    setState(() {});
  }

  @override
  void dispose() {
    timer?.cancel();
    agent.removeListener(onAgent);
    super.dispose();
  }

  Future<void> enableMonitoring() =>
      agent.request('activity_monitoring_set', input: {'enabled': true});

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
        report = Map<String, dynamic>.from(cached);
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
            report = Map<String, dynamic>.from(body);
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
                          'Downloads${r == null ? '' : ' (${(r['downloads'] as List? ?? const []).length})'}',
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
                    : r['monitoring_enabled'] != true
                    ? _MonitoringOff(onEnable: enableMonitoring)
                    : TabBarView(
                        children: [
                          _WindowsTab(
                            apps: (r['apps'] as List? ?? const []).cast<Map>(),
                          ),
                          _TabsTab(
                            browsers: (r['browsers'] as List? ?? const [])
                                .cast<Map>(),
                            tabs: (r['tabs'] as List? ?? const []).cast<Map>(),
                          ),
                          _DownloadsTab(
                            downloads: (r['downloads'] as List? ?? const [])
                                .cast<Map>(),
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

class _Header extends StatelessWidget {
  const _Header({
    required this.name,
    required this.loading,
    required this.fetchedAt,
    required this.autoRefresh,
    required this.onAutoRefresh,
    required this.onRefresh,
  });
  final String name;
  final bool loading, autoRefresh;
  final DateTime? fetchedAt;
  final ValueChanged<bool> onAutoRefresh;
  final VoidCallback? onRefresh;
  @override
  Widget build(BuildContext context) => Material(
    color: Colors.white,
    child: Padding(
      padding: const EdgeInsets.fromLTRB(16, 10, 10, 10),
      child: Row(
        children: [
          const Icon(Icons.visibility_outlined, color: _teal, size: 20),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              'Activity · $name',
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
              overflow: TextOverflow.ellipsis,
            ),
          ),
          if (fetchedAt != null)
            Padding(
              padding: const EdgeInsets.only(right: 10),
              child: Text(
                'Updated ${formatClock(fetchedAt!.millisecondsSinceEpoch)}',
                style: const TextStyle(fontSize: 11.5, color: Colors.blueGrey),
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
  const _MonitoringOff({required this.onEnable});
  final VoidCallback onEnable;
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
            onPressed: onEnable,
            icon: const Icon(Icons.visibility, size: 18),
            label: const Text('Turn on for the class'),
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

/// A section header above a card wrap/list.
class _SectionLabel extends StatelessWidget {
  const _SectionLabel(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 16, 16, 6),
    child: Text(
      text,
      style: const TextStyle(
        fontSize: 12.5,
        fontWeight: FontWeight.w700,
        color: Colors.blueGrey,
        letterSpacing: 0.3,
      ),
    ),
  );
}

class _Empty extends StatelessWidget {
  const _Empty(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
    child: Text(
      text,
      style: const TextStyle(fontSize: 12.5, color: Colors.blueGrey),
    ),
  );
}

class _Badge extends StatelessWidget {
  const _Badge(this.text, {required this.color});
  final String text;
  final Color color;
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
    decoration: BoxDecoration(
      color: color.withValues(alpha: 0.12),
      borderRadius: BorderRadius.circular(5),
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

const _open = Color(0xff1f7a4d);
const _closed = Color(0xff8a97a3);

// ------------------------------------------------------------ Open windows

class _WindowsTab extends StatelessWidget {
  const _WindowsTab({required this.apps});
  final List<Map> apps;
  @override
  Widget build(BuildContext context) {
    final open = apps.where((a) => a['open'] == true).toList();
    final closed = apps.where((a) => a['open'] != true).toList();
    if (apps.isEmpty) {
      return const Center(
        child: Text(
          'No application windows observed yet.',
          style: TextStyle(color: Colors.blueGrey),
        ),
      );
    }
    return ListView(
      padding: const EdgeInsets.only(bottom: 16),
      children: [
        _SectionLabel('OPEN NOW (${open.length})'),
        if (open.isEmpty)
          const _Empty('Nothing currently open.')
        else
          _cardWrap(open.map((a) => _AppCard(a: a)).toList()),
        _SectionLabel('CLOSED (${closed.length})'),
        if (closed.isEmpty)
          const _Empty('Nothing closed yet this session.')
        else
          _cardWrap(closed.map((a) => _AppCard(a: a)).toList()),
        const _Note(
          'History is recorded only while this window has been open to watch — there may be gaps.',
        ),
      ],
    );
  }
}

Widget _cardWrap(List<Widget> cards) => Padding(
  padding: const EdgeInsets.symmetric(horizontal: 12),
  child: Wrap(spacing: 10, runSpacing: 10, children: cards),
);

class _AppCard extends StatelessWidget {
  const _AppCard({required this.a});
  final Map a;
  @override
  Widget build(BuildContext context) {
    final open = a['open'] == true;
    final look = lookAppUp('${a['app']}');
    final reopened = (a['reopened'] as num?)?.toInt() ?? 0;
    return Container(
      width: 270,
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: const Color(0xffe3e8ec)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          AppBadge(app: '${a['app']}'),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  look.name,
                  style: const TextStyle(
                    fontWeight: FontWeight.w600,
                    fontSize: 12.5,
                  ),
                ),
                Text(
                  '${a['title']}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 11.5,
                    color: Colors.blueGrey,
                  ),
                ),
                const SizedBox(height: 6),
                Wrap(
                  spacing: 5,
                  runSpacing: 4,
                  children: [
                    _Badge(
                      open
                          ? '● open ${formatDuration((a['total_open_ms'] as num?)?.toInt() ?? 0)}'
                          : 'closed ${formatClock((a['closed_ms'] as num?)?.toInt() ?? 0)}',
                      color: open ? _open : _closed,
                    ),
                    if (!open)
                      _Badge(
                        'total ${formatDuration((a['total_open_ms'] as num?)?.toInt() ?? 0)}',
                        color: _closed,
                      ),
                    if (reopened > 0) _Badge('×${reopened + 1}', color: _teal),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Note extends StatelessWidget {
  const _Note(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 10, 16, 4),
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

// ------------------------------------------------------------ Browser tabs

class _TabsTab extends StatelessWidget {
  const _TabsTab({required this.browsers, required this.tabs});
  final List<Map> browsers;
  final List<Map> tabs;
  @override
  Widget build(BuildContext context) {
    final closedTabs = tabs.where((t) => t['open'] != true).toList();
    return ListView(
      padding: const EdgeInsets.only(bottom: 16),
      children: [
        _SectionLabel('OPEN NOW'),
        if (browsers.isEmpty)
          const _Empty('No browser window is open.')
        else
          ...browsers.map((b) => _BrowserWindowCard(b: b)),
        _SectionLabel('HISTORY, INCLUDING CLOSED (${closedTabs.length})'),
        if (closedTabs.isEmpty)
          const _Empty('No closed tabs recorded yet this session.')
        else
          _cardWrap(closedTabs.map((t) => _TabCard(t: t)).toList()),
        const _Note(
          'A tab is identified by its title; navigating to a new page in the same tab is seen as a new one. '
          'A background tab\'s address is not available — only the active tab\'s is.',
        ),
      ],
    );
  }
}

class _BrowserWindowCard extends StatelessWidget {
  const _BrowserWindowCard({required this.b});
  final Map b;
  @override
  Widget build(BuildContext context) {
    final tabs = (b['tabs'] as List).cast<Map>();
    final unavailable = b['unavailable'] == true;
    return Container(
      margin: const EdgeInsets.fromLTRB(16, 0, 16, 10),
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: const Color(0xffe3e8ec)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 8, 10, 4),
            child: Row(
              children: [
                AppBadge(app: '${b['app']}', size: 22),
                const SizedBox(width: 8),
                Text(
                  '${lookAppUp('${b['app']}').name} — ${tabs.length} tab${tabs.length == 1 ? '' : 's'}',
                  style: const TextStyle(
                    fontWeight: FontWeight.w600,
                    fontSize: 12.5,
                  ),
                ),
                if (unavailable) ...[
                  const SizedBox(width: 6),
                  const Tooltip(
                    message:
                        'Tab list unavailable right now; showing the window title',
                    child: Icon(
                      Icons.info_outline,
                      size: 14,
                      color: Colors.orange,
                    ),
                  ),
                ],
              ],
            ),
          ),
          const Divider(height: 1),
          for (final t in tabs)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              child: Row(
                children: [
                  SiteIcon(favicon: t['favicon_png_base64'] as String?),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '${t['title']}'.isEmpty
                              ? '(untitled tab)'
                              : '${t['title']}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 12.5,
                            fontWeight: t['active'] == true
                                ? FontWeight.w700
                                : FontWeight.w400,
                          ),
                        ),
                        if (t['url'] != null && '${t['url']}'.isNotEmpty)
                          Text(
                            '${t['url']}',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontSize: 11,
                              color: Colors.blueGrey,
                            ),
                          ),
                      ],
                    ),
                  ),
                  if (t['active'] == true)
                    const Padding(
                      padding: EdgeInsets.only(left: 6),
                      child: Icon(Icons.circle, size: 8, color: _open),
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class _TabCard extends StatelessWidget {
  const _TabCard({required this.t});
  final Map t;
  @override
  Widget build(BuildContext context) {
    final open = t['open'] == true;
    final reopened = (t['reopened'] as num?)?.toInt() ?? 0;
    return Container(
      width: 290,
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: const Color(0xffe3e8ec)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SiteIcon(favicon: t['favicon_png_base64'] as String?, size: 20),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '${t['title']}'.isEmpty ? '(untitled tab)' : '${t['title']}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontWeight: FontWeight.w600,
                    fontSize: 12.5,
                  ),
                ),
                if (t['url'] != null && '${t['url']}'.isNotEmpty)
                  Text(
                    '${t['url']}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontSize: 11,
                      color: Colors.blueGrey,
                    ),
                  ),
                const SizedBox(height: 6),
                Wrap(
                  spacing: 5,
                  runSpacing: 4,
                  children: [
                    _Badge(
                      open
                          ? '● open ${formatDuration((t['total_open_ms'] as num?)?.toInt() ?? 0)}'
                          : 'closed ${formatClock((t['closed_ms'] as num?)?.toInt() ?? 0)}',
                      color: open ? _open : _closed,
                    ),
                    if (!open)
                      _Badge(
                        'total ${formatDuration((t['total_open_ms'] as num?)?.toInt() ?? 0)}',
                        color: _closed,
                      ),
                    if (reopened > 0) _Badge('×${reopened + 1}', color: _teal),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

// -------------------------------------------------------------- Downloads

class _DownloadsTab extends StatelessWidget {
  const _DownloadsTab({required this.downloads});
  final List<Map> downloads;
  @override
  Widget build(BuildContext context) {
    if (downloads.isEmpty) {
      return const Center(
        child: Text(
          'No recent browser downloads found.',
          style: TextStyle(color: Colors.blueGrey),
        ),
      );
    }
    return ListView.separated(
      padding: const EdgeInsets.symmetric(vertical: 8),
      itemCount: downloads.length,
      separatorBuilder: (_, _) =>
          const Divider(height: 1, indent: 16, endIndent: 16),
      itemBuilder: (_, i) {
        final d = downloads[i];
        final (icon, color) = fileTypeLook('${d['name']}');
        final url = '${d['url'] ?? ''}';
        final source = url.isEmpty ? '' : (Uri.tryParse(url)?.host ?? url);
        return Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
          child: Row(
            children: [
              Container(
                width: 34,
                height: 34,
                decoration: BoxDecoration(
                  color: color.withValues(alpha: 0.12),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Icon(icon, size: 18, color: color),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '${d['name']}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 12.5,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    Text(
                      [
                        if (source.isNotEmpty) 'from $source',
                        '${d['browser']}',
                        formatClock(((d['unix_ms'] as num?) ?? 0).toInt()),
                      ].where((s) => s.isNotEmpty).join(' · '),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 11,
                        color: Colors.blueGrey,
                      ),
                    ),
                  ],
                ),
              ),
              Text(
                formatBytes((d['bytes'] as num?) ?? 0),
                style: const TextStyle(fontSize: 11, color: Colors.blueGrey),
              ),
            ],
          ),
        );
      },
    );
  }
}
