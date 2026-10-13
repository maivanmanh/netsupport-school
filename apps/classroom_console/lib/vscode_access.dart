import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'window_shell.dart';

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
    this.classWide = true,
  });

  /// Whether this covers the whole class (so computers that join later get
  /// the rule too) rather than a hand-picked group.
  final bool classWide;
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
    return 'Chưa nhận được xác nhận hoàn tất. Hãy kiểm tra các máy rồi thử lại.';
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
          'Chưa có máy nào đang kết nối hỗ trợ việc này. Cần Windows chạy quyền quản trị và Student bản mới.';
    } else {
      final response = await agent.request(
        'tool_copilot_policy',
        selected: ids,
        input: {'enabled': value, 'class_wide': widget.classWide},
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
        () => error =
            'Hãy cho phép ít nhất một tiện ích, hoặc tắt danh sách cho phép.',
      );
      return;
    }
    final confirmedByTeacher = await confirmDialog(
      context,
      title: 'Áp dụng chính sách tiện ích VS Code?',
      icon: Icons.extension_outlined,
      confirmLabel: 'Áp dụng',
      destructive: true,
      message:
          'Áp dụng hạn chế sẽ đóng rồi mở lại VS Code trên các máy Windows đã chọn. Hãy nhắc học viên lưu bài trước. Ô chặn mạng Copilot được áp dụng riêng.',
    );
    if (!mounted || !confirmedByTeacher) return;
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
        'class_wide': widget.classWide,
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
              : 'Chưa nhận được xác nhận hoàn tất. Hãy kiểm tra các máy đã chọn rồi thử lại.');
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
    return WindowShell(
      icon: Icons.extension_outlined,
      title: 'Tiện ích VS Code · ${widget.audience}',
      subtitle: blocked == 0
          ? 'Chưa có máy nào đang áp dụng danh sách cho phép'
          : '$blocked máy đang áp dụng danh sách cho phép',
      footer: _footerBar(),
      child: AbsorbPointer(
        absorbing: busy,
        child: Opacity(
          opacity: busy ? 0.6 : 1,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(24, 16, 24, 8),
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
                if (copilotError != null) ...[
                  const SizedBox(height: 8),
                  Row(
                    children: [
                      Expanded(child: InlineNotice(copilotError!)),
                      const SizedBox(width: 8),
                      TextButton(
                        onPressed: copilotBusy
                            ? null
                            : () => changeCopilot(copilot),
                        child: const Text('Thử lại'),
                      ),
                    ],
                  ),
                ],
                const SizedBox(height: 14),
                WindowCard(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 6,
                  ),
                  child: Row(
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
                      const SizedBox(width: 6),
                      const Expanded(
                        child: Text(
                          'Bật danh sách cho phép: chỉ các tiện ích được đánh dấu bên dưới mới chạy được',
                          style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      if (extensions.isNotEmpty)
                        StatusChip(
                          '${selected.length}/${extensions.length} được phép',
                          const Color(0xff156b79),
                        ),
                    ],
                  ),
                ),
                const SizedBox(height: 14),
                Row(
                  children: [
                    if (loading) ...[
                      const SizedBox(
                        width: 14,
                        height: 14,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                      const SizedBox(width: 8),
                      Text('Đang quét $scanDone/$scanTotal máy…'),
                    ] else
                      Text(
                        'Tiện ích · ${extensions.length} loại',
                        style: const TextStyle(fontWeight: FontWeight.w700),
                      ),
                    const Spacer(),
                    if (loading)
                      TextButton.icon(
                        onPressed: stopScan,
                        icon: const Icon(Icons.stop_circle_outlined, size: 18),
                        label: const Text('Dừng'),
                      )
                    else
                      TextButton.icon(
                        onPressed: load,
                        icon: const Icon(Icons.refresh, size: 18),
                        label: const Text('Quét lại mọi máy đang kết nối'),
                      ),
                  ],
                ),
                const SizedBox(height: 6),
                TextField(
                  controller: search,
                  onChanged: (_) => setState(() {}),
                  decoration: const InputDecoration(
                    isDense: true,
                    prefixIcon: Icon(Icons.search, size: 18),
                    hintText: 'Tìm tiện ích đã cài…',
                    filled: true,
                    fillColor: Colors.white,
                  ),
                ),
                if (fetchErrors.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Text(
                    fetchErrors.toSet().join(' · '),
                    style: const TextStyle(
                      fontSize: 11.5,
                      color: AppDialog.muted,
                    ),
                  ),
                ],
                const SizedBox(height: 10),
                Expanded(
                  child: Container(
                    clipBehavior: Clip.antiAlias,
                    decoration: BoxDecoration(
                      color: Colors.white,
                      borderRadius: BorderRadius.circular(12),
                      border: Border.all(color: AppDialog.border),
                    ),
                    child: extensions.isEmpty
                        ? DialogEmpty(
                            icon: Icons.extension_off_outlined,
                            title: !agent.connected
                                ? 'Đang kết nối tới agent giảng viên…'
                                : 'Không tìm thấy tiện ích nào do người dùng cài trên các máy đã chọn',
                          )
                        : ListView.separated(
                            itemCount: shown.length,
                            separatorBuilder: (_, _) =>
                                const Divider(height: 1),
                            itemBuilder: (context, i) {
                              final e = shown[i];
                              final checked = selected.contains(e.id);
                              return CheckboxListTile(
                                // Not on the allowlist while it is on = blocked.
                                tileColor: newIds.contains(e.id)
                                    ? const Color(0xffe0f3ed)
                                    : enabled && !checked
                                    ? const Color(0xffffeaea)
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
                                      const StatusChip(
                                        'MỚI',
                                        Color(0xff167452),
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
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _footerBar() => Column(
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
            'Đã áp dụng cho phần còn lại của lớp, nhưng bỏ qua ${skipped.length} máy không thể thực thi (cần bản mới chạy bằng quyền quản trị): '
            '${skipped.map((s) => '${s['name']} (${s['ip']})').join(', ')}',
            tone: AppTone.warning,
          ),
        ),
      Row(
        children: [
          if (applied)
            const Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.check_circle, size: 17, color: Color(0xff1f7a4d)),
                SizedBox(width: 6),
                Text(
                  'Đã áp dụng',
                  style: TextStyle(
                    color: Color(0xff1f7a4d),
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ],
            ),
          const Spacer(),
          AppDialog.primary(
            onPressed: busy || copilotBusy || loading || !changed
                ? null
                : apply,
            icon: Icons.check,
            busy: busy,
            label: busy ? 'Đang áp dụng…' : 'Áp dụng',
          ),
        ],
      ),
    ],
  );
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
                'Chặn GitHub Copilot / Copilot Chat',
                style: TextStyle(fontSize: 13, fontWeight: FontWeight.w700),
              ),
              const Text(
                'Chặn kết nối tới GitHub Copilot · áp dụng ngay cho mọi máy online được hỗ trợ, độc lập với danh sách tiện ích được phép.',
                style: TextStyle(fontSize: 11, color: Colors.blueGrey),
              ),
              if (unsupported > 0)
                Text(
                  '$unsupported máy đang kết nối không được hỗ trợ hoặc cần cập nhật',
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
            '$applied/$eligible máy đã chặn',
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
