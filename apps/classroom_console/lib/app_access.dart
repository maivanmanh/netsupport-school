import 'icon_tints.dart';
import 'dart:async';
import 'dart:convert';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'window_shell.dart';

const _line = Color(0xffe6eaee);
const _modeBlock = 'block';
const _modeAllowOnly = 'allow_only';

/// Ids of Students currently enforcing an application block.
Set<String> appsBlockedStudents(Agent agent) {
  final list = (agent.state['tools'] as Map? ?? const {})['apps_blocked'];
  return {
    if (list is List)
      for (final v in list) '$v',
  };
}

/// One program, merged across every computer that reported it.
class AppEntry {
  AppEntry(this.id, this.name);
  final String id;
  String name, publisher = '', version = '';
  int executables = 0;
  String? icon;
  Set<String> students = {};
  factory AppEntry.fromJson(Map j) {
    final e = AppEntry('${j['id']}', '${j['name'] ?? ''}')
      ..publisher = '${j['publisher'] ?? ''}'
      ..version = '${j['version'] ?? ''}'
      ..executables = ((j['executables'] as num?) ?? 0).toInt()
      ..icon = (j['icon'] as String?)
      ..students = {for (final s in (j['students'] as List? ?? const [])) '$s'};
    return e;
  }
  Map<String, dynamic> toJson() => {
    'id': id,
    'name': name,
    'publisher': publisher,
    'version': version,
    'executables': executables,
    'icon': icon,
    'students': students.toList(),
  };
}

class AppsWindow extends StatefulWidget {
  const AppsWindow({
    super.key,
    required this.agent,
    required this.studentIds,
    required this.audience,
    required this.classWide,
  });
  final Agent agent;
  final List<String> studentIds;
  final String audience;

  /// Whether this covers the whole class (so computers that join later get
  /// the rule too) rather than a hand-picked group.
  final bool classWide;
  @override
  State<AppsWindow> createState() => _AppsWindowState();
}

class _AppsWindowState extends State<AppsWindow> {
  Agent get agent => widget.agent;
  final Map<String, AppEntry> inventory = {};
  Set<String> selected = {};
  // Which rows sort to the top: the selection as of the last load/apply, so
  // ticking a box does not make the row jump away from the cursor.
  Set<String> pinned = {};
  bool baselineEnabled = false;
  Set<String> baselineSelected = {};

  /// Path rules, edited here and as last applied. They are a separate rule
  /// from the program list: each tab applies on its own and sends the other
  /// tab's *applied* state unchanged.
  List<String> paths = [];
  List<String> baselinePaths = [];
  int tab = 0;
  int? pathSelected;

  /// A rule from the removed "only allow the ticked programs" mode is still in
  /// force on the Students; it is replaced the next time the Teacher applies.
  bool legacyAllowOnly = false;
  final search = TextEditingController();
  bool scanning = false, fetchingIcons = false, busy = false;
  int scanTotal = 0, scanDone = 0;
  String? error, notice;
  List<Map<String, String>> skipped = [];
  final newIds = <String>{};
  int generation = 0;
  bool loadedCache = false, firstEverScan = true, started = false;
  final jobs = <String>{};

  bool get changed => !setEquals(selected, baselineSelected);
  bool get pathsChanged => !listEquals(paths, baselinePaths);

  Map get caps =>
      (agent.state['tools'] as Map?)?['capabilities'] as Map? ?? const {};
  List<String> get eligible => [
    for (final id in widget.studentIds)
      if (agent.students.any((s) => s['id'] == id && s['online'] == true) &&
          (caps[id] as Map?)?['apps'] == 1)
        id,
  ];

  @override
  void initState() {
    super.initState();
    agent.addListener(onAgent);
    onAgent();
  }

  @override
  void dispose() {
    generation++;
    for (final id in jobs) {
      unawaited(agent.request('fs_cancel', input: {'id': id}));
    }
    agent.removeListener(onAgent);
    search.dispose();
    super.dispose();
  }

  void onAgent() {
    if (!mounted) return;
    if (agent.connected && !started) {
      started = true;
      applyPreferences();
      loadAppliedPolicy().then((_) => loadCache()).then((_) {
        if (mounted) scan();
      });
    }
    setState(() {});
  }

  Future<void> loadAppliedPolicy() async {
    final response = await agent.request('app_policy_state');
    if (!mounted || response['ok'] != true || changed || pathsChanged) return;
    final rule = (response['result'] as Map?)?['rule'];
    if (rule is! Map) return;
    final scope = rule['scope'];
    if (scope is List && !widget.studentIds.every(scope.contains)) return;
    final policy = rule['value'];
    if (policy is! Map) return;
    setState(() => loadPolicy(policy));
  }

  void applyPreferences() {
    final policy = agent.preferences['apps_policy'];
    if (policy is Map) loadPolicy(policy);
  }

  /// Makes [policy] both the shown and the baseline state.
  void loadPolicy(Map policy) {
    final active = policy['enabled'] == true;
    legacyAllowOnly = active && policy['mode'] == _modeAllowOnly;
    selected = legacyAllowOnly
        ? {}
        : {for (final id in (policy['ids'] as List? ?? const [])) '$id'};
    paths = [
      for (final p in (policy['paths'] as List? ?? const []))
        if ('$p'.trim().isNotEmpty) '$p'.trim(),
    ];
    baselineEnabled = active;
    baselineSelected = Set.of(selected);
    baselinePaths = List.of(paths);
    pinned = Set.of(selected);
    if (legacyAllowOnly) {
      notice =
          'Các máy đang áp dụng chế độ cũ “chỉ cho phép ứng dụng đã chọn”. Hãy đánh dấu các ứng dụng cần chặn rồi bấm Áp dụng để thay thế, hoặc bấm Gỡ hạn chế.';
    }
  }

  // ---------------------------------------------------------------- cache

  /// The merged inventory from earlier sessions is shown at once; a rescan
  /// only adds what is new.
  Future<void> loadCache() async {
    try {
      final data = (await agent.storeList('inventory'))['cache'];
      if (data is! Map) return;
      for (final raw in (data['apps'] as List? ?? const [])) {
        if (raw is! Map) continue;
        final entry = AppEntry.fromJson(raw);
        inventory[entry.id] = entry;
      }
      firstEverScan = inventory.isEmpty;
      loadedCache = true;
      if (mounted) setState(() {});
    } catch (_) {}
  }

  Future<void> saveCache() async {
    try {
      await agent.storeApply('inventory', {
        'cache': {
          'saved_ms': DateTime.now().millisecondsSinceEpoch,
          'apps': [for (final e in inventory.values) e.toJson()],
        },
      });
    } catch (_) {}
  }

  // ----------------------------------------------------------------- scan

  Future<Map<String, dynamic>?> fetch(
    String student,
    List<String> iconIds,
    int gen,
  ) async {
    final started = await agent.request(
      'app_list',
      input: {'student': student, 'icons_for': iconIds},
    );
    final id = started['job_id'];
    if (!mounted || gen != generation) return null;
    if (started['ok'] != true || id == null) return null;
    jobs.add('$id');
    final deadline = DateTime.now().add(const Duration(seconds: 40));
    var attempt = 0;
    while (mounted && gen == generation && DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(
        resultPollDelay(attempt++, const Duration(milliseconds: 600)),
      );
      if (!mounted || gen != generation) break;
      final answer = await agent.request('fs_result', input: {'id': id});
      if (answer['ok'] != true) break;
      final body = answer['result'];
      if (body is Map && body['pending'] != true) {
        jobs.remove('$id');
        return body['error'] != null ? null : Map<String, dynamic>.from(body);
      }
    }
    jobs.remove('$id');
    unawaited(agent.request('fs_cancel', input: {'id': id}));
    return null;
  }

  /// Phase 1: every computer reports its list (no icons — small). Phase 2:
  /// each program whose icon the Teacher lacks is requested from exactly one
  /// computer that has it, spreading the work across computers.
  Future<void> scan() async {
    if (scanning) return;
    final ids = eligible;
    if (ids.isEmpty) {
      setState(
        () => notice = inventory.isEmpty
            ? 'Chưa có máy nào đang kết nối hỗ trợ kiểm soát ứng dụng (cần Windows, Student bản mới, chạy bằng quyền quản trị).'
            : null,
      );
      return;
    }
    final gen = ++generation;
    final before = inventory.keys.toSet();
    setState(() {
      scanning = true;
      error = null;
      notice = null;
      scanTotal = ids.length;
      scanDone = 0;
      newIds.clear();
    });
    final reported = <String, Set<String>>{}; // student -> app ids it has
    final fresh = <String, AppEntry>{};
    await Future.wait(
      ids.map((student) async {
        Map<String, dynamic>? report;
        for (var attempt = 0; attempt < 2 && report == null; attempt++) {
          report = await fetch(student, const [], gen);
        }
        if (!mounted || gen != generation) return;
        setState(() {
          scanDone++;
          if (report == null) return;
          final have = reported.putIfAbsent(student, () => {});
          for (final raw in (report['apps'] as List? ?? const [])) {
            if (raw is! Map) continue;
            final info = AppEntry.fromJson(raw);
            have.add(info.id);
            final entry = fresh.putIfAbsent(
              info.id,
              () => AppEntry(info.id, info.name)
                ..publisher = info.publisher
                ..version = info.version,
            );
            entry.executables = entry.executables > info.executables
                ? entry.executables
                : info.executables;
            entry.students.add(student);
          }
        });
      }),
    );
    if (!mounted || gen != generation) return;
    // Merge: membership of scanned computers is replaced, others kept.
    final scanned = reported.keys.toSet();
    setState(() {
      for (final entry in inventory.values) {
        entry.students.removeAll(scanned);
      }
      for (final f in fresh.values) {
        final existing = inventory[f.id];
        if (existing == null) {
          inventory[f.id] = f;
        } else {
          existing
            ..name = f.name
            ..publisher = f.publisher
            ..version = f.version
            ..executables = f.executables;
          existing.students.addAll(f.students);
        }
      }
      inventory.removeWhere((_, e) => e.students.isEmpty);
      if (!firstEverScan) {
        newIds.addAll(inventory.keys.where((id) => !before.contains(id)));
      }
      firstEverScan = false;
      scanning = false;
    });
    await saveCache();
    await fetchMissingIcons(reported, gen);
  }

  Future<void> fetchMissingIcons(
    Map<String, Set<String>> reported,
    int gen,
  ) async {
    final missing = [
      for (final e in inventory.values)
        if (e.icon == null) e.id,
    ];
    if (missing.isEmpty || reported.isEmpty) return;
    // One computer per icon, shared out evenly.
    final plan = <String, List<String>>{};
    var turn = 0;
    final students = reported.keys.toList();
    for (final id in missing) {
      final owners = [
        for (final s in students)
          if (reported[s]!.contains(id)) s,
      ];
      if (owners.isEmpty) continue;
      plan.putIfAbsent(owners[turn++ % owners.length], () => []).add(id);
    }
    if (!mounted || gen != generation) return;
    setState(() => fetchingIcons = true);
    await Future.wait(
      plan.entries.map((e) async {
        for (var i = 0; i < e.value.length; i += 60) {
          final chunk = e.value.sublist(i, (i + 60).clamp(0, e.value.length));
          final report = await fetch(e.key, chunk, gen);
          if (!mounted || gen != generation || report == null) return;
          setState(() {
            for (final raw in (report['apps'] as List? ?? const [])) {
              if (raw is! Map) continue;
              final icon = raw['icon_png_base64'];
              final entry = inventory['${raw['id']}'];
              if (entry != null && icon is String && icon.isNotEmpty) {
                entry.icon = icon;
              }
            }
          });
        }
      }),
    );
    if (!mounted) return;
    setState(() => fetchingIcons = false);
    await saveCache();
  }

  // ---------------------------------------------------------------- apply

  Future<String?> waitForJob(dynamic id) async {
    final deadline = DateTime.now().add(const Duration(seconds: 60));
    while (mounted && DateTime.now().isBefore(deadline)) {
      final jobs = (agent.state['tools'] as Map?)?['jobs'];
      if (jobs is List) {
        for (final job in jobs.whereType<Map>()) {
          if (job['id'] != id) continue;
          final targets = (job['targets'] as List? ?? const [])
              .whereType<Map>()
              .toList();
          if (targets.isNotEmpty &&
              targets.every(
                (t) => [
                  'complete',
                  'error',
                  'cancelled',
                  'disconnected',
                ].contains(t['status']),
              )) {
            final failures = targets.where((t) => t['status'] != 'complete');
            return failures.isEmpty
                ? null
                : failures
                      .map((t) => '${t['error'] ?? t['status']}')
                      .join('\n');
          }
        }
      }
      await Future<void>.delayed(const Duration(milliseconds: 200));
    }
    return 'Chưa nhận được xác nhận hoàn tất. Hãy kiểm tra các máy rồi thử lại.';
  }

  /// Applies the program list. The path rules go along exactly as last applied.
  Future<void> apply({bool turnOff = false}) async {
    if (busy) return;
    if (!turnOff && selected.isEmpty) {
      setState(
        () => error =
            'Hãy đánh dấu ít nhất một ứng dụng cần chặn, hoặc gỡ hạn chế.',
      );
      return;
    }
    final names = [
      for (final id in selected)
        if (inventory[id] != null) inventory[id]!.name,
    ]..sort();
    await submit(
      enable: !turnOff,
      ids: selected,
      pathList: baselinePaths,
      title: turnOff ? 'Gỡ hạn chế ứng dụng?' : 'Áp dụng hạn chế ứng dụng?',
      confirmLabel: turnOff ? 'Gỡ hạn chế' : 'Áp dụng',
      destructive: !turnOff,
      message: turnOff
          ? 'Mọi hạn chế theo danh sách ứng dụng trên các máy đã chọn sẽ được gỡ.'
          : '${names.length} ứng dụng được đánh dấu sẽ không chạy được trên các máy đã chọn và bản đang chạy bị đóng ngay. Hãy nhắc học viên lưu bài trước.',
    );
  }

  /// Applies the path rules. The program list goes along as last applied.
  Future<void> applyPaths() async {
    if (busy || !pathsChanged) return;
    final clearing = paths.isEmpty;
    await submit(
      enable: baselineEnabled,
      ids: baselineSelected,
      pathList: paths,
      title: clearing
          ? 'Gỡ chặn theo đường dẫn?'
          : 'Áp dụng chặn theo đường dẫn?',
      confirmLabel: clearing ? 'Gỡ chặn' : 'Áp dụng',
      destructive: !clearing,
      message: clearing
          ? 'Mọi quy tắc chặn theo đường dẫn trên các máy đã chọn sẽ được gỡ.'
          : '${paths.length} đường dẫn sẽ bị chặn khởi động trên các máy đã chọn; chương trình đang chạy trong đó bị đóng ngay. Hãy nhắc học viên lưu bài trước.',
    );
  }

  Future<void> submit({
    required bool enable,
    required Set<String> ids,
    required List<String> pathList,
    required String title,
    required String confirmLabel,
    required String message,
    required bool destructive,
  }) async {
    final ok = await confirmDialog(
      context,
      title: title,
      icon: Icons.block_outlined,
      destructive: destructive,
      confirmLabel: confirmLabel,
      message: message,
    );
    if (!mounted || !ok) return;
    setState(() {
      busy = true;
      error = null;
      skipped = [];
    });
    final policy = {
      'enabled': enable,
      'mode': _modeBlock,
      'ids': ids.toList(),
      'paths': pathList,
    };
    final result = await agent.request(
      'tool_app_policy',
      selected: widget.studentIds,
      input: {'class_wide': widget.classWide, 'policy': policy},
      timeout: const Duration(seconds: 15),
    );
    if (!mounted) return;
    if (result['ok'] != true) {
      setState(() {
        busy = false;
        error = agent.error ?? Agent.failure(result);
      });
      return;
    }
    // Save the accepted selection before waiting for remote completion. Closing
    // this separate window during that wait must not discard its checkboxes.
    try {
      await agent.savePreferences({'apps_policy': policy});
    } catch (e) {
      if (mounted) {
        setState(() => error = 'Không lưu được lựa chọn ứng dụng: $e');
      }
    }
    if (!mounted) return;
    final skippedList = (result['result'] as Map?)?['skipped'];
    final failure = await waitForJob(result['job_id']);
    if (!mounted) return;
    setState(() {
      busy = false;
      error = failure;
      if (failure == null) {
        legacyAllowOnly = false;
        baselineEnabled = enable;
        baselineSelected = Set.of(ids);
        baselinePaths = List.of(pathList);
        pinned = Set.of(selected);
      }
      skipped = [
        if (skippedList is List)
          for (final s in skippedList)
            if (s is Map)
              {'name': '${s['name'] ?? ''}', 'ip': '${s['ip'] ?? ''}'},
      ];
    });
  }

  // ------------------------------------------------------------------ UI

  List<AppEntry> get visible {
    final q = search.text.trim().toLowerCase();
    final list = inventory.values
        .where(
          (e) =>
              q.isEmpty ||
              e.name.toLowerCase().contains(q) ||
              e.publisher.toLowerCase().contains(q),
        )
        .toList();
    list.sort((a, b) {
      final rank =
          (pinned.contains(b.id) ? 1 : 0) - (pinned.contains(a.id) ? 1 : 0);
      return rank != 0
          ? rank
          : a.name.toLowerCase().compareTo(b.name.toLowerCase());
    });
    return list;
  }

  @override
  Widget build(BuildContext context) {
    final blocked = widget.studentIds
        .where(appsBlockedStudents(agent).contains)
        .length;
    final canApply = eligible.isNotEmpty && !busy;
    return WindowShell(
      icon: Icons.block_outlined,
      tone: AppTone.danger,
      title: 'Kiểm soát ứng dụng',
      subtitle: scanning
          ? 'Đang quét $scanDone/$scanTotal máy…'
          : '${widget.audience} · $blocked máy đang bị hạn chế',
      actions: [
        if (tab == 0)
          IconButton(
            tooltip: 'Quét lại các máy',
            onPressed: scanning ? null : scan,
            icon: scanning
                ? const SizedBox(
                    width: 18,
                    height: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const TIcon(Icons.refresh, size: 20),
          ),
      ],
      footer: tab == 0 ? _appsFooter(canApply) : _pathsFooter(canApply),
      child: Column(
        children: [
          if (scanning || fetchingIcons)
            LinearProgressIndicator(
              value: scanning && scanTotal > 0 ? scanDone / scanTotal : null,
              minHeight: 2,
            ),
          _tabBar(),
          Expanded(child: tab == 0 ? _appsTab() : _pathsTab()),
        ],
      ),
    );
  }

  Widget _tabBar() => Container(
    color: Colors.white,
    padding: const EdgeInsets.symmetric(horizontal: 18),
    decoration: null,
    child: Row(
      children: [
        _tab(
          0,
          Icons.apps_outlined,
          'Theo ứng dụng',
          baselineEnabled ? baselineSelected.length : 0,
        ),
        _tab(
          1,
          Icons.folder_open_outlined,
          'Theo đường dẫn',
          baselinePaths.length,
        ),
        const Spacer(),
      ],
    ),
  );

  /// [applied] counts what is in force, so the badge reads as "đang chặn".
  Widget _tab(int index, IconData icon, String label, int applied) {
    final on = tab == index;
    const brand = Color(0xff156b79);
    return InkWell(
      onTap: () => setState(() => tab = index),
      child: Container(
        padding: const EdgeInsets.fromLTRB(8, 11, 8, 9),
        margin: const EdgeInsets.only(right: 14),
        decoration: BoxDecoration(
          border: Border(
            bottom: BorderSide(
              color: on ? brand : Colors.transparent,
              width: 2.5,
            ),
          ),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            TIcon(icon, size: 17, color: on ? brand : Colors.blueGrey),
            const SizedBox(width: 7),
            Text(
              label,
              style: TextStyle(
                fontSize: 13.5,
                fontWeight: on ? FontWeight.w700 : FontWeight.w500,
                color: on ? brand : Colors.blueGrey.shade700,
              ),
            ),
            if (applied > 0) _pill('$applied', const Color(0xffb83b40)),
          ],
        ),
      ),
    );
  }

  // ------------------------------------------------------- tab 1: programs

  Widget _appsTab() {
    final list = visible;
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 14, 24, 10),
          child: Row(
            children: [
              Expanded(
                child: SizedBox(
                  height: 38,
                  child: TextField(
                    controller: search,
                    onChanged: (_) => setState(() {}),
                    style: const TextStyle(fontSize: 13),
                    decoration: InputDecoration(
                      hintText: 'Tìm ứng dụng',
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
              ),
              const SizedBox(width: 14),
              Text(
                'Đã chọn ${selected.length} · ${inventory.length} ứng dụng',
                style: const TextStyle(fontSize: 12.5, color: AppDialog.muted),
              ),
            ],
          ),
        ),
        if (notice != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 0, 24, 10),
            child: InlineNotice(notice!, tone: AppTone.warning),
          ),
        Expanded(
          child: inventory.isEmpty && scanning
              ? const Center(child: CircularProgressIndicator())
              : Container(
                  margin: const EdgeInsets.fromLTRB(24, 0, 24, 16),
                  clipBehavior: Clip.antiAlias,
                  decoration: BoxDecoration(
                    color: Colors.white,
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(color: _line),
                  ),
                  child: list.isEmpty
                      ? const DialogEmpty(
                          icon: Icons.apps_outlined,
                          title: 'Không có ứng dụng nào để hiển thị',
                        )
                      : ListView.separated(
                          itemCount: list.length,
                          separatorBuilder: (_, _) =>
                              const Divider(height: 1, color: _line),
                          itemBuilder: (_, i) => _row(list[i]),
                        ),
                ),
        ),
      ],
    );
  }

  // -------------------------------------------------------- tab 2: paths

  static final _driveRoot = RegExp(r'^[a-z]:[\\/]*$', caseSensitive: false);
  static final _systemDir = RegExp(
    r'^[a-z]:[\\/]+windows([\\/]|$)',
    caseSensitive: false,
  );

  /// Operating-system folders that may not be blocked as a whole (their
  /// subfolders may). The Student enforces its own, fuller list; this only
  /// tells the Teacher straight away, without listing anything.
  static final _guardedDir = RegExp(
    r'^(?:[a-z]:[\\/]+(?:users|program files|program files \(x86\)|programdata|recovery|boot|\$recycle\.bin|system volume information|perflogs|documents and settings)|%(?:programfiles|programfiles\(x86\)|programw6432|programdata|systemdrive|homedrive|userprofile|public)%)[\\/]*$',
    caseSensitive: false,
  );
  static final _windowsEnv = RegExp(
    r'^%(?:windir|systemroot)%(?:[\\/]|$)',
    caseSensitive: false,
  );

  /// Shape check mirrored on every Student; returns what to tell the Teacher.
  String? pathProblem(String raw, {String? except}) {
    final text = raw.trim();
    if (text.isEmpty) return 'Hãy nhập đường dẫn.';
    if (text.length > 300) return 'Đường dẫn quá dài (tối đa 300 ký tự).';
    if (RegExp(r'["<>|*?]|[\x00-\x1f]').hasMatch(text)) {
      return 'Đường dẫn có ký tự Windows không cho phép: " < > | * ?';
    }
    final full =
        RegExp(r'^[a-zA-Z]:[\\/]').hasMatch(text) ||
        text.startsWith(r'\\') ||
        text.startsWith('%');
    if (!full) {
      return r'Dùng đường dẫn đầy đủ, ví dụ C:\Games hoặc C:\Tools\app.exe.';
    }
    if (_driveRoot.hasMatch(text)) return 'Không thể chặn cả ổ đĩa.';
    if (_systemDir.hasMatch(text) ||
        _windowsEnv.hasMatch(text) ||
        _guardedDir.hasMatch(text)) {
      return 'Không thể chặn thư mục hệ thống Windows.';
    }
    if (text.split(RegExp(r'[\\/]')).contains('..')) {
      return 'Không dùng “..” trong đường dẫn.';
    }
    final key = _pathKey(text);
    final dup = paths.any((p) => p != except && _pathKey(p) == key);
    return dup ? 'Đường dẫn này đã có trong danh sách.' : null;
  }

  static String _pathKey(String p) => p
      .trim()
      .replaceAll('/', r'\')
      .replaceAll(RegExp(r'\\+$'), '')
      .toLowerCase();

  static bool _isFile(String p) => p.trim().toLowerCase().endsWith('.exe');

  /// New (several lines allowed) or Edit (one line) in Windows-environment style.
  Future<void> editPath({int? index}) async {
    final editing = index != null;
    final original = editing ? paths[index] : null;
    final c = TextEditingController(text: original ?? '');
    final result = await showDialog<List<String>>(
      context: context,
      builder: (context) => AppDialog(
        icon: editing ? Icons.edit_outlined : Icons.add_circle_outline,
        title: editing ? 'Sửa đường dẫn' : 'Thêm đường dẫn',
        subtitle: 'Đường dẫn trên máy học viên.',
        width: 560,
        actions: [
          AppDialog.cancel(context),
          ListenableBuilder(
            listenable: c,
            builder: (context, _) {
              final lines = _lines(c.text);
              final ok =
                  lines.isNotEmpty &&
                  lines.every(
                    (l) => pathProblem(l, except: original) == null,
                  ) &&
                  lines.map(_pathKey).toSet().length == lines.length &&
                  (!editing || c.text.trim() != original);
              return AppDialog.primary(
                onPressed: ok ? () => Navigator.pop(context, lines) : null,
                icon: Icons.check,
                label: editing ? 'Lưu' : 'Thêm',
              );
            },
          ),
        ],
        child: ListenableBuilder(
          listenable: c,
          builder: (context, _) {
            final lines = _lines(c.text);
            final problems = [
              for (final l in lines)
                if (pathProblem(l, except: original) != null)
                  pathProblem(l, except: original)!,
            ];
            return Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                TextField(
                  controller: c,
                  autofocus: true,
                  minLines: editing ? 1 : 3,
                  maxLines: editing ? 1 : 6,
                  style: const TextStyle(fontFamily: 'Consolas', fontSize: 13),
                  decoration: InputDecoration(
                    labelText: editing ? 'Đường dẫn' : 'Mỗi dòng một đường dẫn',
                    hintText: r'C:\Games\Launcher.exe   hoặc   C:\Games',
                    errorText: problems.isEmpty ? null : problems.first,
                    errorMaxLines: 2,
                  ),
                  onSubmitted: editing
                      ? (_) {
                          if (lines.length == 1 && problems.isEmpty) {
                            Navigator.pop(context, lines);
                          }
                        }
                      : null,
                ),
                const SizedBox(height: 10),
                const Text(
                  'Tệp .exe: chặn chương trình đó. Thư mục: chặn mọi .exe trong thư mục và các thư mục con. Có thể dùng biến như %USERPROFILE%\\Downloads.',
                  style: TextStyle(
                    fontSize: 12,
                    height: 1.4,
                    color: AppDialog.muted,
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
    disposeAfterClose(c);
    if (result == null || !mounted) return;
    setState(() {
      error = null;
      if (editing) {
        paths[index] = result.first;
        pathSelected = index;
      } else {
        paths.addAll(result);
        pathSelected = paths.length - 1;
      }
    });
  }

  static List<String> _lines(String text) => [
    for (final l in text.split(RegExp(r'[\r\n]+')))
      if (l.trim().isNotEmpty) l.trim(),
  ];

  void deletePath(int index) => setState(() {
    paths.removeAt(index);
    pathSelected = paths.isEmpty ? null : index.clamp(0, paths.length - 1);
    error = null;
  });

  Widget _pathsTab() {
    final sel = pathSelected;
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 14, 24, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Chặn khởi động theo vị trí của tệp, độc lập với danh sách ứng dụng. Tệp .exe cụ thể hoặc cả thư mục (gồm thư mục con).',
            style: TextStyle(
              fontSize: 12.5,
              height: 1.4,
              color: AppDialog.muted,
            ),
          ),
          const SizedBox(height: 10),
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: Container(
                    clipBehavior: Clip.antiAlias,
                    decoration: BoxDecoration(
                      color: Colors.white,
                      borderRadius: BorderRadius.circular(12),
                      border: Border.all(color: _line),
                    ),
                    child: paths.isEmpty
                        ? const DialogEmpty(
                            icon: Icons.folder_off_outlined,
                            title: 'Chưa có đường dẫn nào bị chặn',
                          )
                        : ListView.separated(
                            itemCount: paths.length,
                            separatorBuilder: (_, _) =>
                                const Divider(height: 1, color: _line),
                            itemBuilder: (_, i) => _pathRow(i),
                          ),
                  ),
                ),
                const SizedBox(width: 12),
                SizedBox(
                  width: 112,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      FilledButton.tonalIcon(
                        onPressed: busy ? null : () => editPath(),
                        icon: const TIcon(Icons.add, size: 17),
                        label: const Text('Mới…'),
                      ),
                      const SizedBox(height: 8),
                      OutlinedButton.icon(
                        onPressed: busy || sel == null
                            ? null
                            : () => editPath(index: sel),
                        icon: const TIcon(Icons.edit_outlined, size: 16),
                        label: const Text('Sửa…'),
                      ),
                      const SizedBox(height: 8),
                      OutlinedButton.icon(
                        onPressed: busy || sel == null
                            ? null
                            : () => deletePath(sel),
                        icon: const TIcon(Icons.delete_outline, size: 17),
                        label: const Text('Xóa'),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _pathRow(int i) {
    final p = paths[i];
    final on = pathSelected == i;
    final isNew = !baselinePaths.contains(p);
    // A raw pointer listener selects the instant the row is pressed; a tap or
    // double-tap recognizer would hold the selection back until the double-tap
    // window had passed.
    return Listener(
      onPointerDown: (_) => setState(() => pathSelected = i),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onDoubleTap: busy ? null : () => editPath(index: i),
        child: Container(
          color: on ? const Color(0xffe3eef2) : null,
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          child: Row(
            children: [
              TIcon(
                _isFile(p) ? Icons.apps_outlined : Icons.folder_outlined,
                size: 19,
                color: Colors.blueGrey,
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  p,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontFamily: 'Consolas', fontSize: 13),
                ),
              ),
              if (isNew) _pill('CHƯA ÁP DỤNG', const Color(0xffb86b22)),
              _pill(_isFile(p) ? 'tệp' : 'thư mục', Colors.blueGrey),
            ],
          ),
        ),
      ),
    );
  }

  // --------------------------------------------------------------- footers

  Widget _notices() => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    mainAxisSize: MainAxisSize.min,
    children: [
      if (error != null)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: InlineNotice(error!),
        ),
      if (skipped.isNotEmpty)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: InlineNotice(
            'Chưa áp dụng cho: ${skipped.map((s) => s['name']!.isEmpty ? s['ip'] : s['name']).join(', ')} '
            '(cần Student bản mới chạy bằng quyền quản trị trên Windows).',
            tone: AppTone.warning,
          ),
        ),
    ],
  );

  /// One line: what will happen on the left, the actions on the right.
  Widget _bar({
    required String summary,
    required bool pending,
    required List<Widget> buttons,
  }) => Row(
    children: [
      Expanded(
        child: Row(
          children: [
            Flexible(
              child: Text(
                summary,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  fontSize: 12.5,
                  height: 1.35,
                  color: AppDialog.muted,
                ),
              ),
            ),
            if (pending) _pill('CHƯA ÁP DỤNG', const Color(0xffb86b22)),
          ],
        ),
      ),
      const SizedBox(width: 14),
      for (var i = 0; i < buttons.length; i++) ...[
        if (i > 0) const SizedBox(width: 8),
        buttons[i],
      ],
    ],
  );

  Widget _appsFooter(bool canApply) {
    final count = selected.length;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _notices(),
        _bar(
          summary: count == 0
              ? 'Đánh dấu ứng dụng cần chặn trên các máy đã chọn.'
              : '$count ứng dụng được đánh dấu sẽ không thể mở.',
          pending: baselineEnabled && changed,
          buttons: [
            if (baselineEnabled)
              AppDialog.secondary(
                onPressed: busy ? null : () => apply(turnOff: true),
                label: 'Gỡ hạn chế',
              ),
            AppDialog.primary(
              // With no restriction in force, Apply needs something to restrict;
              // with one in force it needs an actual edit (reverting disables it).
              onPressed:
                  canApply && (baselineEnabled ? changed : selected.isNotEmpty)
                  ? apply
                  : null,
              icon: Icons.lock_outline,
              busy: busy,
              label: busy ? 'Đang áp dụng…' : 'Áp dụng',
            ),
          ],
        ),
      ],
    );
  }

  Widget _pathsFooter(bool canApply) => Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      _notices(),
      _bar(
        summary: paths.isEmpty
            ? 'Không chặn đường dẫn nào.'
            : '${paths.length} đường dẫn bị chặn khởi động.',
        pending: pathsChanged,
        buttons: [
          AppDialog.primary(
            onPressed: canApply && pathsChanged ? applyPaths : null,
            icon: Icons.lock_outline,
            busy: busy,
            label: busy ? 'Đang áp dụng…' : 'Áp dụng',
          ),
        ],
      ),
    ],
  );

  Widget _row(AppEntry e) {
    final checked = selected.contains(e.id);
    final controllable = e.executables > 0;
    final isNew = newIds.contains(e.id);
    final blockedByDraft = controllable && checked;
    final detail = [
      if (e.publisher.isNotEmpty) e.publisher,
      if (e.version.isNotEmpty) 'v${e.version}',
      'trên ${e.students.length} máy',
    ].join(' · ');
    return InkWell(
      onTap: controllable
          ? () => setState(() {
              checked ? selected.remove(e.id) : selected.add(e.id);
              error = null;
            })
          : null,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 500),
        color: isNew
            ? const Color(0xffe3f4ee)
            : blockedByDraft
            ? const Color(0xffffeaea)
            : Colors.transparent,
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
        child: Row(
          children: [
            Checkbox(
              value: checked,
              onChanged: controllable
                  ? (v) => setState(() {
                      v == true ? selected.add(e.id) : selected.remove(e.id);
                      error = null;
                    })
                  : null,
            ),
            SizedBox(width: 30, height: 30, child: _icon(e)),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    e.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: checked ? FontWeight.w700 : FontWeight.w600,
                      color: controllable ? null : Colors.blueGrey,
                    ),
                  ),
                  Text(
                    detail,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontSize: 11.5,
                      color: Colors.blueGrey,
                    ),
                  ),
                ],
              ),
            ),
            if (isNew) _pill('MỚI', const Color(0xff1f7a4d)),
            if (!controllable)
              Tooltip(
                message:
                    'Không tìm thấy tệp chương trình của ứng dụng này nên không thể chặn.',
                child: _pill('không điều khiển được', Colors.blueGrey),
              ),
          ],
        ),
      ),
    );
  }

  Widget _pill(String text, Color color) => Container(
    margin: const EdgeInsets.only(left: 6),
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

  Widget _icon(AppEntry e) {
    final b64 = e.icon;
    if (b64 != null && b64.isNotEmpty) {
      try {
        return Image.memory(
          base64Decode(b64),
          fit: BoxFit.contain,
          filterQuality: FilterQuality.medium,
          gaplessPlayback: true,
          errorBuilder: (_, _, _) => _genericIcon(),
        );
      } catch (_) {}
    }
    return _genericIcon();
  }

  Widget _genericIcon() => Container(
    decoration: BoxDecoration(
      color: Colors.blueGrey.withValues(alpha: 0.12),
      borderRadius: BorderRadius.circular(8),
    ),
    child: const TIcon(Icons.apps, size: 18, color: Colors.blueGrey),
  );
}
