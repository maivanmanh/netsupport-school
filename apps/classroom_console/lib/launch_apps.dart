import 'icon_tints.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';

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
    return parts.isEmpty ? '(trống)' : parts.last;
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
  _ => 'Mọi hệ điều hành',
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
    final confirmed = await confirmDialog(
      context,
      title: 'Xóa mục này?',
      message: '“${item.title}” sẽ bị xóa khỏi danh sách chương trình đã lưu.',
      confirmLabel: 'Xóa',
      icon: Icons.delete_outline,
      destructive: true,
    );
    if (!confirmed) return;
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
            'Không có máy nào đang chạy ${_osLabel(item.os)}; chưa khởi chạy gì.',
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
        'Sẵn sàng — ${widget.audience}',
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
        ? 'Đang khởi chạy… $done/${targets.length}'
        : failed.isEmpty
        ? 'Đã khởi chạy trên $done máy'
        : 'Đã chạy $done máy · lỗi ${failed.length} máy';
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
    return AppDialog(
      icon: Icons.rocket_launch_outlined,
      title: 'Chạy ứng dụng trên ${widget.audience}',
      subtitle: 'Chọn một chương trình đã lưu rồi bấm “Chạy”.',
      width: 680,
      height: 400,
      scrollable: false,
      footerLeading: AnimatedBuilder(
        animation: agent,
        builder: (_, _) => status(),
      ),
      actions: [
        AppDialog.primary(
          onPressed: item == null || busy ? null : launch,
          icon: Icons.rocket_launch_outlined,
          busy: busy,
          label: busy ? 'Đang chạy…' : 'Chạy',
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              AppDialog.secondary(
                onPressed: () => edit(),
                icon: Icons.add,
                label: 'Thêm',
              ),
              const SizedBox(width: 8),
              AppDialog.secondary(
                onPressed: item == null ? null : () => edit(item),
                icon: Icons.edit_outlined,
                label: 'Sửa',
              ),
              const SizedBox(width: 8),
              AppDialog.secondary(
                onPressed: item == null ? null : remove,
                icon: Icons.delete_outline,
                label: 'Xóa',
              ),
            ],
          ),
          const SizedBox(height: 12),
          Expanded(
            child: DialogPanel(
              child: items.isEmpty
                  ? const DialogEmpty(
                      icon: Icons.apps_outlined,
                      title: 'Chưa có chương trình nào',
                      message: 'Bấm “Thêm”, ví dụ Chrome kèm một địa chỉ web.',
                    )
                  : ListView.separated(
                      itemCount: items.length,
                      separatorBuilder: (_, _) => const Divider(height: 1),
                      itemBuilder: (_, i) {
                        final it = items[i];
                        return ListTile(
                          dense: true,
                          selected: it.id == selected,
                          selectedTileColor: const Color(0xffe3f1f3),
                          leading: const TIcon(Icons.apps, size: 20),
                          title: Text(
                            it.title,
                            style: const TextStyle(fontWeight: FontWeight.w600),
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
                              color: AppDialog.muted,
                            ),
                          ),
                          onTap: () => setState(() => selected = it.id),
                          onLongPress: () => edit(it),
                        );
                      },
                    ),
            ),
          ),
        ],
      ),
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

  /// A path is required, and an existing entry must actually differ.
  bool get canSave {
    if (path.text.trim().isEmpty) return false;
    final item = widget.item;
    return item == null ||
        note.text.trim() != item.note ||
        path.text.trim() != item.path ||
        args.text.trim() != item.args ||
        os != item.os;
  }

  void save() {
    if (path.text.trim().isEmpty) {
      setState(
        () => error = 'Hãy nhập đường dẫn chương trình trên máy học viên.',
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
        isDense: true,
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    return AppDialog(
      icon: widget.item == null
          ? Icons.add_circle_outline
          : Icons.edit_outlined,
      title: widget.item == null ? 'Thêm chương trình' : 'Sửa chương trình',
      subtitle: 'Chương trình sẽ được chạy trên máy học viên.',
      width: 560,
      actions: [
        AppDialog.cancel(context),
        ListenableBuilder(
          listenable: Listenable.merge([note, path, args]),
          builder: (context, _) => AppDialog.primary(
            label: 'Lưu',
            icon: Icons.check,
            onPressed: canSave ? save : null,
          ),
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          DialogSection(
            title: 'Mẫu có sẵn',
            child: Wrap(
              spacing: 6,
              runSpacing: 6,
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
          ),
          field(note, 'Ghi chú', 'Hiện trong danh sách, ví dụ: Mở web của lớp'),
          field(
            path,
            'Đường dẫn chương trình',
            r'C:\Program Files\Google\Chrome\Application\chrome.exe',
          ),
          field(
            args,
            'Tham số (không bắt buộc)',
            'https://example.org  hoặc  --kiosk "https://example.org"',
          ),
          DropdownButtonFormField<String>(
            initialValue: os,
            decoration: const InputDecoration(
              labelText: 'Chạy trên',
              isDense: true,
            ),
            items: const [
              DropdownMenuItem(value: 'any', child: Text('Mọi hệ điều hành')),
              DropdownMenuItem(value: 'windows', child: Text('Chỉ Windows')),
              DropdownMenuItem(value: 'macos', child: Text('Chỉ macOS')),
            ],
            onChanged: (v) => setState(() => os = v!),
          ),
          const SizedBox(height: 10),
          const Text(
            r'Tên ngắn như notepad.exe hoặc chrome sẽ do Windows tự tìm. Biến kiểu %USERPROFILE% dùng được. Trên macOS hãy nhập đường dẫn .app.',
            style: TextStyle(fontSize: 12, color: AppDialog.muted, height: 1.4),
          ),
          if (error != null) ...[
            const SizedBox(height: 12),
            InlineNotice(error!),
          ],
        ],
      ),
    );
  }
}
