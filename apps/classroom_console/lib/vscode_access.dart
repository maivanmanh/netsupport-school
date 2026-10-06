import 'dart:async';
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
  return {if (list is List) for (final v in list) '$v'};
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
  List<String> fetchErrors = [];
  final search = TextEditingController();
  bool busy = false;
  bool applied = false;
  String? error;
  List<Map<String, String>> skipped = [];

  @override
  void initState() {
    super.initState();
    final enabledPref = agent.preferences['vscode_enabled'];
    if (enabledPref is bool) enabled = enabledPref;
    load();
  }

  @override
  void dispose() {
    search.dispose();
    super.dispose();
  }

  Future<Map<String, dynamic>?> fetchOne(String studentId) async {
    final started = await agent.request(
      'vscode_list',
      input: {'student': studentId},
    );
    final id = started['job_id'];
    if (started['ok'] != true || id == null) {
      fetchErrors.add(
        Agent.failure(
          started,
        ).replaceFirst('Operation failed', 'A computer could not be reached'),
      );
      return null;
    }
    final deadline = DateTime.now().add(const Duration(seconds: 15));
    while (DateTime.now().isBefore(deadline)) {
      if (!mounted) return null;
      await Future<void>.delayed(const Duration(milliseconds: 250));
      final answer = await agent.request('fs_result', input: {'id': id});
      if (answer['ok'] != true) {
        fetchErrors.add(Agent.failure(answer));
        return null;
      }
      final body = answer['result'];
      if (body is Map && body['pending'] != true) {
        return Map<String, dynamic>.from(body);
      }
    }
    fetchErrors.add('A computer did not answer in time.');
    return null;
  }

  Future<void> load() async {
    setState(() {
      loading = true;
      error = null;
      fetchErrors = [];
    });
    final results = await Future.wait(widget.studentIds.map(fetchOne));
    if (!mounted) return;
    final byId = <String, _Extension>{};
    final notes = <String>{};
    for (final report in results) {
      if (report == null) continue;
      for (final raw in (report['extensions'] as List? ?? const [])) {
        if (raw is! Map) continue;
        final ext = _Extension.fromJson(raw);
        if (_copilotIds.contains(ext.id)) continue;
        byId.putIfAbsent(ext.id, () => ext);
      }
      for (final note in (report['notes'] as List? ?? const [])) {
        notes.add('$note');
      }
    }
    final list = byId.values.toList()
      ..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    final persisted = agent.preferences['vscode_allowed'];
    final previous = {
      if (persisted is List) for (final v in persisted) '$v',
    };
    setState(() {
      loading = false;
      extensions = list;
      selected = previous.intersection(list.map((e) => e.id).toSet());
      fetchErrors = [...fetchErrors, ...notes];
    });
  }

  void save() {
    agent.savePreferences({
      'vscode_allowed': selected.toList(),
      'vscode_enabled': enabled,
    });
  }

  Future<void> apply() async {
    if (busy) return;
    if (enabled && selected.isEmpty) {
      setState(
        () => error =
            'Allow at least one extension, or turn the allowlist off.',
      );
      return;
    }
    save();
    setState(() {
      busy = true;
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
    setState(() {
      busy = false;
      applied = true;
      skipped = [
        if (skippedList is List)
          for (final s in skippedList)
            if (s is Map)
              {'name': '${s['name'] ?? ''}', 'ip': '${s['ip'] ?? ''}'},
      ];
    });
    if (skipped.isEmpty) {
      await Future<void>.delayed(const Duration(milliseconds: 900));
      if (mounted) await agent.finishApplication();
    }
  }

  @override
  Widget build(BuildContext context) {
    final blocked = vscodeBlockedStudents(agent).length;
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
                    const Icon(Icons.extension_outlined, color: _teal, size: 20),
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
                      style: const TextStyle(fontSize: 12, color: Colors.blueGrey),
                    ),
                  ],
                ),
              ),
            ),
            const Divider(height: 1),
            Expanded(
              child: AbsorbPointer(
                absorbing: busy || applied,
                child: Opacity(
                  opacity: busy || applied ? 0.6 : 1,
                  child: loading
                      ? const Center(child: CircularProgressIndicator())
                      : Padding(
                          padding: const EdgeInsets.fromLTRB(20, 16, 20, 0),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              _CopilotRow(),
                              const SizedBox(height: 12),
                              Row(
                                children: [
                                  Switch(
                                    value: enabled,
                                    onChanged: (v) => setState(() => enabled = v),
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
                                    ? const Center(
                                        child: Text(
                                          'No user-installed extensions were found on the selected computers.',
                                          style: TextStyle(color: Colors.blueGrey),
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
                                            dense: true,
                                            value: checked,
                                            onChanged: (v) => setState(() {
                                              if (v == true) {
                                                selected.add(e.id);
                                              } else {
                                                selected.remove(e.id);
                                              }
                                            }),
                                            title: Text(
                                              e.name,
                                              style: const TextStyle(
                                                fontSize: 13,
                                                fontWeight: FontWeight.w600,
                                              ),
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
                        Icon(Icons.check_circle, size: 17, color: Color(0xff1f7a4d)),
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
                    Text(error!, style: const TextStyle(fontSize: 12.5, color: _danger)),
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
                      TextButton(
                        onPressed: busy
                            ? null
                            : () {
                                save();
                                agent.finishApplication();
                              },
                        child: Text(applied ? 'Close' : 'Cancel'),
                      ),
                      const SizedBox(width: 8),
                      FilledButton.icon(
                        onPressed: busy || applied || loading ? null : apply,
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

/// GitHub Copilot is never a normal checkbox: VS Code's own
/// `AllowedExtensions` policy denies anything not explicitly listed, so
/// Copilot is always left out of the allowlist this window sends —
/// regardless of what the Teacher checks below.
class _CopilotRow extends StatelessWidget {
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
        const Icon(Icons.smart_toy_outlined, size: 18, color: Colors.blueGrey),
        const SizedBox(width: 10),
        const Expanded(
          child: Text(
            'GitHub Copilot / Copilot Chat',
            style: TextStyle(fontSize: 13, fontWeight: FontWeight.w700),
          ),
        ),
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
          decoration: BoxDecoration(
            color: _danger.withValues(alpha: 0.12),
            borderRadius: BorderRadius.circular(20),
          ),
          child: const Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.lock_outline, size: 13, color: _danger),
              SizedBox(width: 4),
              Text(
                'Always blocked',
                style: TextStyle(
                  fontSize: 11.5,
                  fontWeight: FontWeight.w700,
                  color: _danger,
                ),
              ),
            ],
          ),
        ),
      ],
    ),
  );
}
