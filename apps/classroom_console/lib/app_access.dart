import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'agent.dart';

const _teal = Color(0xff156b79);
const _bg = Color(0xfff3f5f9);
const _danger = Color(0xffb83b40);
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
  String mode = _modeBlock;
  bool enabled = false;
  String baselineMode = _modeBlock;
  bool baselineEnabled = false;
  Set<String> baselineSelected = {};
  final search = TextEditingController();
  bool scanning = false, fetchingIcons = false, busy = false;
  int scanTotal = 0, scanDone = 0;
  String? error, notice;
  List<Map<String, String>> skipped = [];
  final newIds = <String>{};
  int generation = 0;
  bool loadedCache = false, firstEverScan = true, started = false;
  final jobs = <String>{};

  bool get changed =>
      enabled != baselineEnabled ||
      mode != baselineMode ||
      !setEquals(selected, baselineSelected);

  Map get caps =>
      (agent.state['tools'] as Map?)?['capabilities'] as Map? ?? const {};
  List<String> get eligible => [
    for (final id in widget.studentIds)
      if (agent.students.any((s) => s['id'] == id && s['online'] == true) &&
          (caps[id] as Map?)?['apps'] == 1)
        id,
  ];

  File? get cacheFile {
    final dir = agent.dataDirectory;
    return dir == null ? null : File('${dir.path}/app-inventory.json');
  }

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
    if (!mounted || response['ok'] != true || changed) return;
    final rule = (response['result'] as Map?)?['rule'];
    if (rule is! Map) return;
    final scope = rule['scope'];
    if (scope is List && !widget.studentIds.every(scope.contains)) return;
    final policy = rule['value'];
    if (policy is! Map) return;
    setState(() {
      enabled = policy['enabled'] == true;
      mode = policy['mode'] == _modeAllowOnly ? _modeAllowOnly : _modeBlock;
      selected = {for (final id in (policy['ids'] as List? ?? const [])) '$id'};
      baselineEnabled = enabled;
      baselineMode = mode;
      baselineSelected = Set.of(selected);
      pinned = Set.of(selected);
    });
  }

  void applyPreferences() {
    final policy = agent.preferences['apps_policy'];
    if (policy is Map) {
      enabled = policy['enabled'] == true;
      mode = policy['mode'] == _modeAllowOnly ? _modeAllowOnly : _modeBlock;
      selected = {for (final id in (policy['ids'] as List? ?? const [])) '$id'};
    }
    baselineEnabled = enabled;
    baselineMode = mode;
    baselineSelected = Set.of(selected);
    pinned = Set.of(selected);
  }

  // ---------------------------------------------------------------- cache

  /// The merged inventory from earlier sessions is shown at once; a rescan
  /// only adds what is new.
  Future<void> loadCache() async {
    try {
      final file = cacheFile;
      if (file == null || !await file.exists()) return;
      final data = jsonDecode(await file.readAsString());
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
      final file = cacheFile;
      if (file == null) return;
      await file.writeAsString(
        jsonEncode({
          'saved_ms': DateTime.now().millisecondsSinceEpoch,
          'apps': [for (final e in inventory.values) e.toJson()],
        }),
      );
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
    while (mounted && gen == generation && DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(const Duration(milliseconds: 600));
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
            ? 'No connected computer supports application control (Windows, updated Student, run as administrator).'
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
    return 'No completion confirmation received. Check the computers and retry.';
  }

  Future<void> apply({bool turnOff = false}) async {
    if (busy) return;
    final enable = !turnOff && enabled;
    if (!turnOff && enabled && selected.isEmpty) {
      setState(
        () => error = mode == _modeAllowOnly
            ? 'Tick at least one application to allow, or turn restrictions off.'
            : 'Tick at least one application to block, or turn restrictions off.',
      );
      return;
    }
    final names = [
      for (final id in selected)
        if (inventory[id] != null) inventory[id]!.name,
    ]..sort();
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(
          turnOff ? 'Remove application restrictions?' : 'Apply restrictions?',
        ),
        content: Text(
          turnOff
              ? 'Every application block on the selected computers is lifted.'
              : mode == _modeAllowOnly
              ? 'On the selected computers only ${names.length} ticked application${names.length == 1 ? '' : 's'} can start; every other installed application is blocked and closed now. Ask students to save their work first.'
              : '${names.length} ticked application${names.length == 1 ? '' : 's'} can no longer start on the selected computers and any copy running is closed now. Ask students to save their work first.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(turnOff ? 'Remove' : 'Apply'),
          ),
        ],
      ),
    );
    if (!mounted || ok != true) return;
    setState(() {
      busy = true;
      error = null;
      skipped = [];
    });
    final result = await agent.request(
      'tool_app_policy',
      selected: widget.studentIds,
      input: {
        'class_wide': widget.classWide,
        'policy': {'enabled': enable, 'mode': mode, 'ids': selected.toList()},
      },
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
      await agent.savePreferences({
        'apps_policy': {
          'enabled': enable,
          'mode': mode,
          'ids': selected.toList(),
        },
      });
    } catch (e) {
      if (mounted) {
        setState(() => error = 'Could not save application selection: $e');
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
        enabled = enable;
        baselineEnabled = enable;
        baselineMode = mode;
        baselineSelected = Set.of(selected);
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
    final list = visible;
    final blocked = widget.studentIds
        .where(appsBlockedStudents(agent).contains)
        .length;
    final canApply = eligible.isNotEmpty && !busy;
    return Scaffold(
      backgroundColor: _bg,
      body: SafeArea(
        child: Column(
          children: [
            _header(blocked),
            if (scanning || fetchingIcons)
              LinearProgressIndicator(
                value: scanning && scanTotal > 0 ? scanDone / scanTotal : null,
                minHeight: 2,
              ),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
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
                          hintText: 'Search applications',
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
                  ),
                  const SizedBox(width: 12),
                  Text(
                    '${selected.length} ticked · ${inventory.length} applications',
                    style: const TextStyle(
                      fontSize: 12,
                      color: Colors.blueGrey,
                    ),
                  ),
                ],
              ),
            ),
            if (notice != null)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                child: Text(notice!, style: const TextStyle(color: _danger)),
              ),
            Expanded(
              child: inventory.isEmpty && scanning
                  ? const Center(child: CircularProgressIndicator())
                  : Container(
                      margin: const EdgeInsets.symmetric(horizontal: 16),
                      clipBehavior: Clip.antiAlias,
                      decoration: BoxDecoration(
                        color: Colors.white,
                        borderRadius: BorderRadius.circular(12),
                        border: Border.all(color: _line),
                      ),
                      child: list.isEmpty
                          ? const Center(
                              child: Text(
                                'No applications to show.',
                                style: TextStyle(color: Colors.blueGrey),
                              ),
                            )
                          : ListView.separated(
                              itemCount: list.length,
                              separatorBuilder: (_, _) =>
                                  const Divider(height: 1, color: _line),
                              itemBuilder: (_, i) => _row(list[i]),
                            ),
                    ),
            ),
            _footer(canApply),
          ],
        ),
      ),
    );
  }

  Widget _header(int blocked) => Material(
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
            child: const Icon(Icons.apps_outlined, color: _teal, size: 20),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'Application control',
                  style: TextStyle(fontSize: 16, fontWeight: FontWeight.w700),
                ),
                Text(
                  scanning
                      ? 'Scanning $scanDone of $scanTotal computers…'
                      : '${widget.audience} · restrictions active on $blocked',
                  style: const TextStyle(
                    fontSize: 11.5,
                    color: Colors.blueGrey,
                  ),
                ),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Scan the computers again',
            visualDensity: VisualDensity.compact,
            onPressed: scanning ? null : scan,
            icon: scanning
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.refresh, size: 19),
          ),
        ],
      ),
    ),
  );

  Widget _row(AppEntry e) {
    final checked = selected.contains(e.id);
    final controllable = e.executables > 0;
    final isNew = newIds.contains(e.id);
    // What the current selection would block: the ticked ones when blocking,
    // everything else controllable when only the ticked ones are allowed.
    final blockedByDraft =
        controllable && (mode == _modeBlock ? checked : !checked);
    final detail = [
      if (e.publisher.isNotEmpty) e.publisher,
      if (e.version.isNotEmpty) 'v${e.version}',
      'on ${e.students.length} computer${e.students.length == 1 ? '' : 's'}',
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
            if (isNew) _pill('NEW', const Color(0xff1f7a4d)),
            if (!controllable)
              Tooltip(
                message:
                    'No program file was found for this application, so it cannot be blocked.',
                child: _pill('not controllable', Colors.blueGrey),
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
    child: const Icon(Icons.apps, size: 18, color: Colors.blueGrey),
  );

  Widget _footer(bool canApply) => Material(
    color: Colors.white,
    elevation: 6,
    child: Padding(
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (error != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Text(error!, style: const TextStyle(color: _danger)),
            ),
          if (skipped.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Text(
                'Not applied to: ${skipped.map((s) => s['name']!.isEmpty ? s['ip'] : s['name']).join(', ')} '
                '(need the updated Student running as administrator on Windows).',
                style: const TextStyle(fontSize: 12, color: Colors.orange),
              ),
            ),
          Wrap(
            spacing: 12,
            runSpacing: 8,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              SegmentedButton<String>(
                showSelectedIcon: false,
                segments: const [
                  ButtonSegment(
                    value: _modeBlock,
                    icon: Icon(Icons.block, size: 16),
                    label: Text('Block ticked'),
                  ),
                  ButtonSegment(
                    value: _modeAllowOnly,
                    icon: Icon(Icons.verified_outlined, size: 16),
                    label: Text('Allow only ticked'),
                  ),
                ],
                selected: {mode},
                onSelectionChanged: (v) => setState(() {
                  mode = v.first;
                  enabled = true;
                }),
              ),
              const SizedBox(width: 12),
              SizedBox(
                width: 180,
                child: Text(
                  mode == _modeBlock
                      ? 'Ticked applications cannot be opened.'
                      : 'Only ticked applications can be opened; all other installed applications are blocked.',
                  style: const TextStyle(fontSize: 12, color: Colors.blueGrey),
                ),
              ),
              if (baselineEnabled)
                Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: OutlinedButton(
                    onPressed: busy ? null : () => apply(turnOff: true),
                    child: const Text('Remove restrictions'),
                  ),
                ),
              FilledButton.icon(
                onPressed: canApply && (changed || !baselineEnabled)
                    ? () {
                        enabled = true;
                        apply();
                      }
                    : null,
                icon: busy
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(
                          strokeWidth: 2,
                          color: Colors.white,
                        ),
                      )
                    : const Icon(Icons.lock_outline, size: 18),
                label: Text(busy ? 'Applying…' : 'Apply'),
              ),
            ],
          ),
        ],
      ),
    ),
  );
}
