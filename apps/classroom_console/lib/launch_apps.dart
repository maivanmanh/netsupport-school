import 'package:flutter/material.dart';
import 'agent.dart';

/// One saved program the Teacher can start on every Student at once.
class LaunchItem {
  LaunchItem({
    required this.id,
    this.note = '',
    this.path = '',
    this.args = '',
    this.os = 'any',
  });
  final String id;
  String note, path, args;

  /// `any`, `windows` or `macos`: Students on other systems are skipped.
  String os;

  factory LaunchItem.fromJson(Map<dynamic, dynamic> json) => LaunchItem(
    id: '${json['id']}',
    note: '${json['note'] ?? ''}',
    path: '${json['path'] ?? ''}',
    args: '${json['args'] ?? ''}',
    os: '${json['os'] ?? 'any'}',
  );
  Map<String, dynamic> toJson() => {
    'id': id,
    'note': note,
    'path': path,
    'args': args,
    'os': os,
  };

  String get title {
    if (note.trim().isNotEmpty) return note.trim();
    final parts = path.trim().split(RegExp(r'[\\/]'))
      ..removeWhere((e) => e.isEmpty);
    return parts.isEmpty ? '(empty)' : parts.last;
  }
}

const _presets = <(String, String, String, String)>[
  (
    'Chrome',
    r'C:\Program Files\Google\Chrome\Application\chrome.exe',
    'https://www.google.com',
    'windows',
  ),
  (
    'Edge',
    r'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe',
    'https://www.bing.com',
    'windows',
  ),
  ('Notepad', 'notepad.exe', '', 'windows'),
  ('Calculator', 'calc.exe', '', 'windows'),
  (
    'Chrome',
    '/Applications/Google Chrome.app',
    'https://www.google.com',
    'macos',
  ),
  ('Safari', '/Applications/Safari.app', '', 'macos'),
];

String _osLabel(String os) => switch (os) {
  'windows' => 'Windows',
  'macos' => 'macOS',
  _ => 'Any system',
};

List<LaunchItem> _load(Agent agent) =>
    ((agent.preferences['launch_items'] as List?) ?? const [])
        .whereType<Map>()
        .map(LaunchItem.fromJson)
        .toList();

/// Manage saved programs and start one on every selected Student.
Future<void> showLaunchDialog(
  BuildContext context,
  Agent agent, {
  required List<String> studentIds,
  required String audience,
}) => showDialog<void>(
  context: context,
  builder: (_) =>
      _LaunchDialog(agent: agent, ids: studentIds, audience: audience),
);

class _LaunchDialog extends StatefulWidget {
  const _LaunchDialog({
    required this.agent,
    required this.ids,
    required this.audience,
  });
  final Agent agent;
  final List<String> ids;
  final String audience;
  @override
  State<_LaunchDialog> createState() => _LaunchDialogState();
}

class _LaunchDialogState extends State<_LaunchDialog> {
  late List<LaunchItem> items = _load(widget.agent);
  String? selected;
  String? jobId;
  String? message;
  bool busy = false;

  Agent get agent => widget.agent;
  LaunchItem? get current {
    for (final i in items) {
      if (i.id == selected) return i;
    }
    return null;
  }

  void persist() => agent.savePreferences({
    'launch_items': items.map((i) => i.toJson()).toList(),
  });

  String osOf(String id) {
    final caps = (agent.state['tools'] as Map? ?? const {})['capabilities'];
    return '${(caps as Map?)?[id]?['os'] ?? ''}';
  }

  Future<void> edit([LaunchItem? existing]) async {
    final result = await showDialog<LaunchItem>(
      context: context,
      builder: (_) => _EditDialog(item: existing),
    );
    if (result == null) return;
    setState(() {
      final index = items.indexWhere((i) => i.id == result.id);
      if (index >= 0) {
        items[index] = result;
      } else {
        items.add(result);
      }
      selected = result.id;
    });
    persist();
  }

  Future<void> remove() async {
    final item = current;
    if (item == null) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete this item?'),
        content: Text(item.title),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    setState(() {
      items.removeWhere((i) => i.id == item.id);
      selected = null;
    });
    persist();
  }

  Future<void> launch() async {
    final item = current;
    if (item == null || busy) return;
    final targets = widget.ids
        .where((id) => item.os == 'any' || osOf(id) == item.os)
        .toList();
    if (targets.isEmpty) {
      setState(
        () => message =
            'No selected student runs ${_osLabel(item.os)}; nothing was launched.',
      );
      return;
    }
    setState(() {
      busy = true;
      message = null;
    });
    final result = await agent.request(
      'tool_launch',
      selected: targets,
      input: {'path': item.path, 'args': item.args},
    );
    if (!mounted) return;
    setState(() {
      busy = false;
      if (result['ok'] == true) {
        jobId = result['job_id'] as String?;
      } else {
        jobId = null;
        message = agent.error ?? Agent.failure(result);
      }
    });
  }

  String nameOf(String id) {
    for (final s in agent.students) {
      if (s['id'] == id) {
        final name = '${s['name'] ?? ''}'.trim();
        return name.isEmpty ? '${s['ip']}' : name;
      }
    }
    return id;
  }

  /// Live per-launch result, from the Hub's job snapshot.
  Widget status() {
    if (message != null) {
      return Text(message!, style: const TextStyle(color: Color(0xffb83b40)));
    }
    final jobs = ((agent.state['tools'] as Map? ?? const {})['jobs'] as List?)
        ?.whereType<Map>();
    final job = jobs?.where((j) => j['id'] == jobId).firstOrNull;
    if (job == null) {
      return Text(
        'Ready — ${widget.audience}',
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(fontSize: 12, color: Colors.blueGrey),
      );
    }
    final targets = (job['targets'] as List)
        .map((e) => Map<String, dynamic>.from(e as Map))
        .toList();
    final done = targets.where((t) => t['status'] == 'complete').length;
    final failed = targets.where((t) => t['status'] == 'error').toList();
    final waiting = targets.length - done - failed.length;
    final summary = waiting > 0
        ? 'Launching… $done/${targets.length}'
        : failed.isEmpty
        ? 'Launched on $done'
        : 'Launched on $done · failed on ${failed.length}';
    // One compact line; the failure reasons are in the tooltip.
    final detail = failed
        .map((t) => '${nameOf('${t['id']}')}: ${t['error']}')
        .join('\n');
    return Tooltip(
      message: detail,
      child: Text(
        failed.isEmpty
            ? summary
            : '$summary — ${failed.take(2).map((t) => nameOf('${t['id']}')).join(', ')}${failed.length > 2 ? '…' : ''}: ${failed.first['error']}',
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(
          fontSize: 12,
          fontWeight: FontWeight.w600,
          color: failed.isEmpty
              ? const Color(0xff1f7a4d)
              : const Color(0xffb83b40),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final item = current;
    return AlertDialog(
      title: Text('Launch app on ${widget.audience}'),
      content: SizedBox(
        width: 640,
        height: 430,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                FilledButton.tonalIcon(
                  onPressed: () => edit(),
                  icon: const Icon(Icons.add, size: 17),
                  label: const Text('Add'),
                ),
                const SizedBox(width: 8),
                OutlinedButton.icon(
                  onPressed: item == null ? null : () => edit(item),
                  icon: const Icon(Icons.edit_outlined, size: 17),
                  label: const Text('Edit'),
                ),
                const SizedBox(width: 8),
                OutlinedButton.icon(
                  onPressed: item == null ? null : remove,
                  icon: const Icon(Icons.delete_outline, size: 17),
                  label: const Text('Delete'),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Expanded(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  border: Border.all(color: const Color(0xffdfe5eb)),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: items.isEmpty
                    ? const Center(
                        child: Padding(
                          padding: EdgeInsets.all(20),
                          child: Text(
                            'No saved programs yet. Press Add, e.g. Chrome with a web address.',
                            textAlign: TextAlign.center,
                            style: TextStyle(color: Colors.blueGrey),
                          ),
                        ),
                      )
                    : ListView.separated(
                        itemCount: items.length,
                        separatorBuilder: (_, _) => const Divider(height: 1),
                        itemBuilder: (_, i) {
                          final it = items[i];
                          return ListTile(
                            dense: true,
                            selected: it.id == selected,
                            selectedTileColor: const Color(0xffdcecef),
                            leading: const Icon(Icons.apps, size: 20),
                            title: Text(
                              it.title,
                              style: const TextStyle(
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            subtitle: Text(
                              '${it.path}${it.args.isEmpty ? '' : '  ${it.args}'}',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            trailing: Text(
                              _osLabel(it.os),
                              style: const TextStyle(
                                fontSize: 11,
                                color: Colors.blueGrey,
                              ),
                            ),
                            onTap: () => setState(() => selected = it.id),
                            onLongPress: () => edit(it),
                          );
                        },
                      ),
              ),
            ),
            const SizedBox(height: 10),
            AnimatedBuilder(animation: agent, builder: (_, _) => status()),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Close'),
        ),
        FilledButton.icon(
          onPressed: item == null || busy ? null : launch,
          icon: const Icon(Icons.rocket_launch_outlined, size: 18),
          label: Text(busy ? 'Launching…' : 'Launch'),
        ),
      ],
    );
  }
}

class _EditDialog extends StatefulWidget {
  const _EditDialog({this.item});
  final LaunchItem? item;
  @override
  State<_EditDialog> createState() => _EditDialogState();
}

class _EditDialogState extends State<_EditDialog> {
  late final note = TextEditingController(text: widget.item?.note);
  late final path = TextEditingController(text: widget.item?.path);
  late final args = TextEditingController(text: widget.item?.args);
  late String os = widget.item?.os ?? 'any';
  String? error;

  @override
  void dispose() {
    note.dispose();
    path.dispose();
    args.dispose();
    super.dispose();
  }

  void save() {
    if (path.text.trim().isEmpty) {
      setState(
        () => error = 'Enter the program path on the student computers.',
      );
      return;
    }
    Navigator.pop(
      context,
      LaunchItem(
        id:
            widget.item?.id ??
            DateTime.now().microsecondsSinceEpoch.toRadixString(36),
        note: note.text.trim(),
        path: path.text.trim(),
        args: args.text.trim(),
        os: os,
      ),
    );
  }

  Widget field(
    TextEditingController c,
    String label,
    String hint, {
    int lines = 1,
  }) => Padding(
    padding: const EdgeInsets.only(bottom: 10),
    child: TextField(
      controller: c,
      minLines: lines,
      maxLines: lines,
      decoration: InputDecoration(
        labelText: label,
        hintText: hint,
        border: const OutlineInputBorder(),
        isDense: true,
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(widget.item == null ? 'Add program' : 'Edit program'),
      content: SizedBox(
        width: 520,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Wrap(
                spacing: 6,
                children: [
                  for (final p in _presets)
                    ActionChip(
                      visualDensity: VisualDensity.compact,
                      label: Text(
                        '${p.$1} · ${_osLabel(p.$4)}',
                        style: const TextStyle(fontSize: 12),
                      ),
                      onPressed: () => setState(() {
                        if (note.text.isEmpty) note.text = p.$1;
                        path.text = p.$2;
                        args.text = p.$3;
                        os = p.$4;
                      }),
                    ),
                ],
              ),
              const SizedBox(height: 12),
              field(note, 'Note', 'Shown in the list, e.g. Open class website'),
              field(
                path,
                'Path to program',
                r'C:\Program Files\Google\Chrome\Application\chrome.exe',
              ),
              field(
                args,
                'Arguments (optional)',
                'https://example.org  or  --kiosk "https://example.org"',
              ),
              DropdownButtonFormField<String>(
                initialValue: os,
                decoration: const InputDecoration(
                  labelText: 'Runs on',
                  border: OutlineInputBorder(),
                  isDense: true,
                ),
                items: const [
                  DropdownMenuItem(value: 'any', child: Text('Any system')),
                  DropdownMenuItem(
                    value: 'windows',
                    child: Text('Windows only'),
                  ),
                  DropdownMenuItem(value: 'macos', child: Text('macOS only')),
                ],
                onChanged: (v) => setState(() => os = v!),
              ),
              const SizedBox(height: 6),
              const Text(
                r'Bare names such as notepad.exe or chrome are looked up by Windows. %USERPROFILE% style variables work. On macOS give the .app path.',
                style: TextStyle(fontSize: 11.5, color: Colors.blueGrey),
              ),
              if (error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    error!,
                    style: const TextStyle(color: Color(0xffb83b40)),
                  ),
                ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: save, child: const Text('Save')),
      ],
    );
  }
}
