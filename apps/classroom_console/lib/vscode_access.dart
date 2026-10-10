import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'agent.dart';

const _teal = Color(0xff156b79);
const _bg = Color(0xfff3f5f9);
const _danger = Color(0xffb83b40);
const _copilotIds = {
  'github.copilot',
  'github.copilot-chat',
  'github.copilot-nightly',
  'github.copilot-chat-nightly',
};

/// Ids of Students currently enforcing a VS Code extension allowlist.
Set<String> vscodeBlockedStudents(Agent agent) {
  final list = (agent.state['tools'] as Map? ?? const {})['vscode_blocked'];
  return {
    if (list is List)
      for (final v in list) '$v',
  };
}

class _Extension {
  _Extension(this.id, this.name, this.description, this.publisher);
  final String id, name, description, publisher;
  factory _Extension.fromJson(Map j) => _Extension(
    '${j['id'] ?? ''}',
    '${j['name'] ?? j['id'] ?? ''}',
    '${j['description'] ?? ''}',
    '${j['publisher'] ?? ''}',
  );
}

class VsCodeWindow extends StatefulWidget {
  const VsCodeWindow({
    super.key,
    required this.agent,
    required this.studentIds,
    required this.audience,
  });
  final Agent agent;
  final List<String> studentIds;
  final String audience;
  @override
  State<VsCodeWindow> createState() => _VsCodeWindowState();
}

class _VsCodeWindowState extends State<VsCodeWindow> {
  Agent get agent => widget.agent;
  bool loading = true;
  List<_Extension> extensions = [];
  Set<String> selected = {};
  bool enabled = false;
  bool baselineEnabled = false;
  Set<String> baselineSelected = {};
  bool get changed =>
      enabled != baselineEnabled || !setEquals(selected, baselineSelected);
  List<String> fetchErrors = [];
  final search = TextEditingController();
  bool busy = false;
  bool applied = false;
  String? error;
  List<Map<String, String>> skipped = [];
  bool copilot = false;
  bool copilotBusy = false;
  String? copilotError;
  int scanGeneration = 0;
  int scanTotal = 0, scanDone = 0;
  bool firstScan = true;
  final newIds = <String>{};
  final scanJobs = <String>{};

  List<String> get onlineIds => [
    for (final s in agent.students)
      if (s['online'] == true) '${s['id']}',
  ];
  List<String> get copilotEligible {
    final caps =
        (agent.state['tools'] as Map?)?['capabilities'] as Map? ?? const {};
    return onlineIds
        .where((id) => (caps[id] as Map?)?['copilot_network'] == 1)
        .toList();
  }

  Set<String> get copilotApplied {
    final ids = (agent.state['tools'] as Map?)?['copilot_blocked'];
    return {
      if (ids is List)
        for (final id in ids) '$id',
    };
  }

  @override
  void initState() {
    super.initState();
    agent.addListener(refreshStatus);
    // `agent.preferences` is only populated once Agent.start() finishes its
    // own disk read, which happens *after* this widget's first build (see
    // main()'s sequencing) — reading it here unconditionally always sees
    // the empty default, every single time, which is why the toggle used
    // to forget its last value on every reopen. Apply it (and fetch) only
    // once the agent is actually ready, same connected-gating as Files/
    // Activity windows already use.
    if (agent.connected) {
      applyPreferences();
      load();
    } else {
      agent.addListener(waitForAgent);
    }
  }

  void refreshStatus() {
    if (mounted) setState(() {});
  }

  void applyPreferences() {
    final enabledPref = agent.preferences['vscode_enabled'];
    if (enabledPref is bool) enabled = enabledPref;
    copilot = agent.preferences['copilot_network_block'] == true;
    final previous = agent.preferences['vscode_allowed'];
    if (previous is List) selected = previous.map((v) => '$v').toSet();
    baselineEnabled = enabled;
    baselineSelected = Set.of(selected);
  }

  void waitForAgent() {
    if (!agent.connected) return;
    agent.removeListener(waitForAgent);
    applyPreferences();
    load();
  }

  @override
  void dispose() {
    stopScan(notify: false);
    agent.removeListener(refreshStatus);
    agent.removeListener(waitForAgent);
    search.dispose();
    super.dispose();
  }

  Future<Map<String, dynamic>?> fetchOne(
    String studentId,
    int generation,
  ) async {
    final started = await agent.request(
      'vscode_list',
      input: {'student': studentId},
    );
    final id = started['job_id'];
    if (!mounted || generation != scanGeneration) {
      if (id != null) unawaited(agent.request('fs_cancel', input: {'id': id}));
      return null;
    }
    if (started['ok'] != true || id == null) {
      return null;
    }
    scanJobs.add('$id');
    while (mounted && generation == scanGeneration) {
      await Future<void>.delayed(const Duration(milliseconds: 700));
      if (!mounted || generation != scanGeneration) return null;
      final answer = await agent.request('fs_result', input: {'id': id});
      if (!mounted || generation != scanGeneration) return null;
      if (answer['ok'] != true) {
        scanJobs.remove('$id');
        unawaited(agent.request('fs_cancel', input: {'id': id}));
        return null;
      }
      final body = answer['result'];
      if (body is Map && body['pending'] != true) {
        scanJobs.remove('$id');
        if (body['error'] != null) return null;
        return Map<String, dynamic>.from(body);
      }
    }
    return null;
  }

  void stopScan({bool notify = true}) {
    scanGeneration++;
    for (final id in scanJobs) {
      unawaited(agent.request('fs_cancel', input: {'id': id}));
    }
    scanJobs.clear();
    if (notify && mounted) {
      setState(() {
        loading = false;
      });
    }
  }

  Future<void> load() async {
    if (loading && scanTotal > 0) return;
    final generation = ++scanGeneration;
    final ids = onlineIds; // Snapshot: later arrivals belong to the next scan.
    final baseline = extensions.map((e) => e.id).toSet();
    final initial = firstScan;
    firstScan = false;
    setState(() {
      loading = true;
      error = null;
      fetchErrors = [];
      scanTotal = ids.length;
      scanDone = 0;
      newIds.clear();
    });
    await Future.wait(
      ids.map((id) async {
        Map<String, dynamic>? report;
        while (mounted && generation == scanGeneration && report == null) {
          report = await fetchOne(id, generation);
          if (report == null) {
            await Future<void>.delayed(const Duration(seconds: 3));
          }
        }
        if (!mounted || generation != scanGeneration || report == null) return;
        final additions = <_Extension>[];
        for (final raw in (report['extensions'] as List? ?? const [])) {
          if (raw is! Map) continue;
          final ext = _Extension.fromJson(raw);
          if (_copilotIds.contains(ext.id)) continue;
          if (!extensions.any((e) => e.id == ext.id) &&
              !additions.any((e) => e.id == ext.id)) {
            additions.add(ext);
          }
        }
        setState(() {
          scanDone++;
          fetchErrors.addAll([
            for (final note in report!['notes'] as List? ?? const []) '$note',
          ]);
          if (initial) {
            extensions.addAll(additions);
          } else {
            newIds.addAll(
              additions.where((e) => !baseline.contains(e.id)).map((e) => e.id),
            );
            extensions.insertAll(0, additions);
          }
        });
      }),
    );
    if (!mounted || generation != scanGeneration) return;
    setState(() {
      loading = false;
      if (initial) {
        extensions.sort((a, b) {
          final rank =
              (selected.contains(b.id) ? 1 : 0) -
              (selected.contains(a.id) ? 1 : 0);
          return rank != 0
              ? rank
              : a.name.toLowerCase().compareTo(b.name.toLowerCase());
        });
      }
    });
  }

  Future<String?> waitForJob(dynamic id) async {
    final deadline = DateTime.now().add(const Duration(seconds: 60));
    while (mounted && DateTime.now().isBefore(deadline)) {
      final jobs = (agent.state['tools'] as Map?)?['jobs'];
      if (jobs is List) {
        for (final job in jobs.whereType<Map>()) {
          if (job['id'] != id) continue;
          final targets = (job['targets'] as List? ?? [])
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

  Future<void> changeCopilot(bool value) async {
    if (copilotBusy) return;
    setState(() {
      copilot = value;
      copilotBusy = true;
      copilotError = null;
    });
    agent.savePreferences({'copilot_network_block': value});
    final ids = copilotEligible;
    String? failure;
    if (ids.isEmpty) {
      failure =
          'No connected computer supports this. Windows administrator + updated Student required.';
    } else {
      final response = await agent.request(
        'tool_copilot_policy',
        selected: ids,
        input: {'enabled': value},
      );
      failure = response['ok'] == true
          ? await waitForJob(response['job_id'])
          : Agent.failure(response);
    }
    if (!mounted) return;
    setState(() {
      copilotBusy = false;
      copilotError = failure;
    });
  }

  void save() {
    agent.savePreferences({
      'vscode_allowed': selected.toList(),
      'vscode_enabled': enabled,
    });
  }

  Future<void> apply() async {
    if (busy || !changed) return;
    if (enabled && selected.isEmpty) {
      setState(
        () =>
            error = 'Allow at least one extension, or turn the allowlist off.',
      );
      return;
    }
    final confirmedByTeacher = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Apply VS Code extension policy?'),
        content: const Text(
          'Applying restrictions will close and reopen VS Code on the selected Windows computers. Ask students to save their work first. The Copilot network checkbox is applied separately.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Apply'),
          ),
        ],
      ),
    );
    if (!mounted || confirmedByTeacher != true) return;
    setState(() {
      busy = true;
      applied = false;
      skipped = [];
      error = null;
    });
    final result = await agent.request(
      'tool_vscode_policy',
      selected: widget.studentIds,
      input: {
        'policy': {'enabled': enabled, 'allowed': selected.toList()},
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
    final skippedList = (result['result'] as Map?)?['skipped'];
    final jobId = result['job_id'];
    final deadline = DateTime.now().add(const Duration(seconds: 30));
    String? confirmationError;
    bool confirmed = false;
    while (mounted && DateTime.now().isBefore(deadline)) {
      final jobs = (agent.state['tools'] as Map?)?['jobs'];
      if (jobs is List) {
        for (final job in jobs.whereType<Map>()) {
          if (job['id'] != jobId) continue;
          final targets = (job['targets'] as List? ?? const [])
              .whereType<Map>()
              .toList();
          if (targets.isNotEmpty &&
              targets.every(
                (t) => t['status'] == 'complete' || t['status'] == 'error',
              )) {
            final failures = targets.where((t) => t['status'] == 'error');
            confirmationError = failures.isEmpty
                ? null
                : failures.map((t) => '${t['error']}').join('\n');
            confirmed = failures.isEmpty;
            break;
          }
        }
      }
      if (confirmed || confirmationError != null) break;
      await Future<void>.delayed(const Duration(milliseconds: 200));
    }
    if (!mounted) return;
    setState(() {
      busy = false;
      applied = confirmed;
      if (confirmed) {
        baselineEnabled = enabled;
        baselineSelected = Set.of(selected);
        save();
      }
      error =
          confirmationError ??
          (confirmed
              ? null
              : 'No completion confirmation received. Check the selected computers and retry.');
      skipped = [
        if (skippedList is List)
          for (final s in skippedList)
            if (s is Map)
              {'name': '${s['name'] ?? ''}', 'ip': '${s['ip'] ?? ''}'},
      ];
    });
  }

  @override
  Widget build(BuildContext context) {
    final blockedIds = vscodeBlockedStudents(agent);
    final blocked = widget.studentIds.where(blockedIds.contains).length;
    final eligible = copilotEligible;
    final copilotCount = eligible.where(copilotApplied.contains).length;
    final filter = search.text.trim().toLowerCase();
    final shown = filter.isEmpty
        ? extensions
        : extensions
              .where(
                (e) =>
                    e.name.toLowerCase().contains(filter) ||
                    e.publisher.toLowerCase().contains(filter) ||
                    e.id.contains(filter),
              )
              .toList();
    return Scaffold(
      backgroundColor: _bg,
      body: SafeArea(
        child: Column(
          children: [
            Material(
              color: Colors.white,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 10, 16, 10),
                child: Row(
                  children: [
                    const Icon(
                      Icons.extension_outlined,
                      color: _teal,
                      size: 20,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        'VS Code extensions · ${widget.audience}',
                        style: const TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.w600,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    Text(
                      blocked == 0
                          ? 'No computer has an allowlist enforced'
                          : '$blocked computer${blocked == 1 ? '' : 's'} currently enforcing an allowlist',
                      style: const TextStyle(
                        fontSize: 12,
                        color: Colors.blueGrey,
                      ),
                    ),
                  ],
                ),
              ),
            ),
            const Divider(height: 1),
            Expanded(
              child: AbsorbPointer(
                absorbing: busy,
                child: Opacity(
                  opacity: busy ? 0.6 : 1,
                  child: Padding(
                    padding: const EdgeInsets.fromLTRB(20, 16, 20, 0),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        _CopilotRow(
                          value: copilot,
                          onChanged: copilotBusy
                              ? null
                              : (v) => changeCopilot(v ?? false),
                          busy: copilotBusy,
                          applied: copilotCount,
                          eligible: eligible.length,
                          unsupported: onlineIds.length - eligible.length,
                        ),
                        if (copilotError != null)
                          Row(
                            children: [
                              Expanded(
                                child: Text(
                                  copilotError!,
                                  style: const TextStyle(
                                    color: _danger,
                                    fontSize: 12,
                                  ),
                                ),
                              ),
                              TextButton(
                                onPressed: copilotBusy
                                    ? null
                                    : () => changeCopilot(copilot),
                                child: const Text('Retry'),
                              ),
                            ],
                          ),
                        const SizedBox(height: 12),
                        Row(
                          children: [
                            Switch(
                              value: enabled,
                              onChanged: busy
                                  ? null
                                  : (v) => setState(() {
                                      enabled = v;
                                      applied = false;
                                    }),
                            ),
                            const SizedBox(width: 4),
                            const Text(
                              'Enforce an allowlist: only checked extensions below may run',
                              style: TextStyle(
                                fontSize: 13,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const Spacer(),
                            if (extensions.isNotEmpty)
                              Text(
                                '${selected.length}/${extensions.length} allowed',
                                style: const TextStyle(
                                  fontSize: 12,
                                  color: Colors.blueGrey,
                                ),
                              ),
                          ],
                        ),
                        const SizedBox(height: 10),
                        Row(
                          children: [
                            if (loading) ...[
                              const SizedBox(
                                width: 14,
                                height: 14,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                ),
                              ),
                              const SizedBox(width: 8),
                              Text('Scanning $scanDone/$scanTotal computers…'),
                            ] else
                              Text('Extensions · ${extensions.length} unique'),
                            const Spacer(),
                            if (loading)
                              TextButton.icon(
                                onPressed: stopScan,
                                icon: const Icon(
                                  Icons.stop_circle_outlined,
                                  size: 18,
                                ),
                                label: const Text('Stop'),
                              )
                            else
                              TextButton.icon(
                                onPressed: load,
                                icon: const Icon(Icons.refresh, size: 18),
                                label: const Text('Rescan all connected'),
                              ),
                          ],
                        ),
                        TextField(
                          controller: search,
                          onChanged: (_) => setState(() {}),
                          decoration: const InputDecoration(
                            isDense: true,
                            prefixIcon: Icon(Icons.search, size: 18),
                            hintText: 'Search installed extensions…',
                            border: OutlineInputBorder(),
                          ),
                        ),
                        if (fetchErrors.isNotEmpty) ...[
                          const SizedBox(height: 8),
                          Text(
                            fetchErrors.toSet().join(' · '),
                            style: const TextStyle(
                              fontSize: 11.5,
                              color: Colors.blueGrey,
                            ),
                          ),
                        ],
                        const SizedBox(height: 8),
                        Expanded(
                          child: extensions.isEmpty
                              ? Center(
                                  child: Text(
                                    !agent.connected
                                        ? 'Connecting to the Teacher agent…'
                                        : 'No user-installed extensions were found on the selected computers.',
                                    style: const TextStyle(
                                      color: Colors.blueGrey,
                                    ),
                                  ),
                                )
                              : ListView.separated(
                                  itemCount: shown.length,
                                  separatorBuilder: (_, _) =>
                                      const Divider(height: 1),
                                  itemBuilder: (context, i) {
                                    final e = shown[i];
                                    final checked = selected.contains(e.id);
                                    return CheckboxListTile(
                                      tileColor: newIds.contains(e.id)
                                          ? const Color(0xffe0f3ed)
                                          : null,
                                      dense: true,
                                      value: checked,
                                      onChanged: busy
                                          ? null
                                          : (v) => setState(() {
                                              applied = false;
                                              if (v == true) {
                                                selected.add(e.id);
                                              } else {
                                                selected.remove(e.id);
                                              }
                                            }),
                                      title: Row(
                                        children: [
                                          Expanded(
                                            child: Text(
                                              e.name,
                                              style: const TextStyle(
                                                fontSize: 13,
                                                fontWeight: FontWeight.w600,
                                              ),
                                            ),
                                          ),
                                          if (newIds.contains(e.id))
                                            const Text(
                                              'NEW',
                                              style: TextStyle(
                                                fontSize: 10,
                                                fontWeight: FontWeight.w800,
                                                color: Color(0xff167452),
                                              ),
                                            ),
                                        ],
                                      ),
                                      subtitle: Text(
                                        e.description.isEmpty
                                            ? e.id
                                            : '${e.description}  ·  ${e.id}',
                                        style: const TextStyle(fontSize: 11.5),
                                        maxLines: 2,
                                        overflow: TextOverflow.ellipsis,
                                      ),
                                      controlAffinity:
                                          ListTileControlAffinity.leading,
                                    );
                                  },
                                ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
            const Divider(height: 1),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 12, 20, 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (applied)
                    const Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          Icons.check_circle,
                          size: 17,
                          color: Color(0xff1f7a4d),
                        ),
                        SizedBox(width: 6),
                        Text(
                          'Applied',
                          style: TextStyle(
                            color: Color(0xff1f7a4d),
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ],
                    ),
                  if (error != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      error!,
                      style: const TextStyle(fontSize: 12.5, color: _danger),
                    ),
                  ],
                  if (skipped.isNotEmpty) ...[
                    const SizedBox(height: 10),
                    Container(
                      padding: const EdgeInsets.all(10),
                      decoration: BoxDecoration(
                        color: const Color(0xfffdf1e0),
                        borderRadius: BorderRadius.circular(8),
                        border: Border.all(color: const Color(0xffe8c57a)),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              const Icon(
                                Icons.warning_amber_rounded,
                                size: 16,
                                color: Color(0xffb86b22),
                              ),
                              const SizedBox(width: 6),
                              Expanded(
                                child: Text(
                                  'Applied to the rest of the class, but skipped '
                                  '${skipped.length} computer${skipped.length == 1 ? '' : 's'} '
                                  'that cannot enforce this (needs the updated build running as administrator):',
                                  style: const TextStyle(
                                    fontSize: 12,
                                    fontWeight: FontWeight.w600,
                                    color: Color(0xff8a4e14),
                                  ),
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(height: 6),
                          Wrap(
                            spacing: 10,
                            runSpacing: 4,
                            children: [
                              for (final s in skipped)
                                Text(
                                  '${s['name']} (${s['ip']})',
                                  style: const TextStyle(
                                    fontSize: 12,
                                    fontFamily: 'Menlo',
                                    color: Color(0xff8a4e14),
                                  ),
                                ),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ],
                  const SizedBox(height: 12),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      FilledButton.icon(
                        onPressed: busy || copilotBusy || loading || !changed
                            ? null
                            : apply,
                        icon: busy
                            ? const SizedBox(
                                width: 16,
                                height: 16,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                  color: Colors.white,
                                ),
                              )
                            : const Icon(Icons.check, size: 18),
                        label: Text(busy ? 'Applying…' : 'Apply'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _CopilotRow extends StatelessWidget {
  const _CopilotRow({
    required this.value,
    required this.onChanged,
    required this.busy,
    required this.applied,
    required this.eligible,
    required this.unsupported,
  });
  final bool value, busy;
  final int applied, eligible, unsupported;
  final ValueChanged<bool?>? onChanged;
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(10),
    decoration: BoxDecoration(
      color: const Color(0xffeef1f5),
      borderRadius: BorderRadius.circular(8),
      border: Border.all(color: const Color(0xffd7dde4)),
    ),
    child: Row(
      children: [
        Checkbox(value: value, onChanged: onChanged),
        const Icon(Icons.smart_toy_outlined, size: 18, color: Colors.blueGrey),
        const SizedBox(width: 10),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'Block GitHub Copilot / Copilot Chat',
                style: TextStyle(fontSize: 13, fontWeight: FontWeight.w700),
              ),
              const Text(
                'Block GitHub Copilot connections · applies immediately to all supported online computers, independently of the extension allowlist.',
                style: TextStyle(fontSize: 11, color: Colors.blueGrey),
              ),
              if (unsupported > 0)
                Text(
                  '$unsupported connected computer(s) unsupported or need an update',
                  style: const TextStyle(fontSize: 11, color: Colors.blueGrey),
                ),
            ],
          ),
        ),
        const SizedBox(width: 8),
        if (busy)
          const SizedBox(
            width: 14,
            height: 14,
            child: CircularProgressIndicator(strokeWidth: 2),
          )
        else
          Text(
            '$applied/$eligible PC blocked',
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w700,
              color: applied > 0 ? _danger : Colors.blueGrey,
            ),
          ),
      ],
    ),
  );
}
