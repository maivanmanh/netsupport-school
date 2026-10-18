import 'dart:io';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';
import 'app_icons.dart' show SiteIcon;
import 'icon_tints.dart';

/// One screenshot the Teacher holds as evidence that a Student used an AI site.
class EvidenceItem {
  const EvidenceItem({
    required this.id,
    required this.student,
    required this.label,
    required this.address,
    required this.title,
    required this.reason,
    required this.takenMs,
    required this.path,
    required this.bytes,
  });

  final String id, student, label, address, title, reason, path;
  final int takenMs, bytes;

  DateTime get taken => DateTime.fromMillisecondsSinceEpoch(takenMs);

  static EvidenceItem? parse(Object? raw) {
    if (raw is! Map || raw['id'] == null || raw['path'] == null) return null;
    return EvidenceItem(
      id: '${raw['id']}',
      student: '${raw['student'] ?? ''}',
      label: '${raw['label'] ?? ''}',
      address: '${raw['address'] ?? ''}',
      title: '${raw['title'] ?? ''}',
      reason: '${raw['reason'] ?? ''}',
      takenMs: (raw['taken_ms'] as num?)?.toInt() ?? 0,
      path: '${raw['path']}',
      bytes: (raw['bytes'] as num?)?.toInt() ?? 0,
    );
  }
}

/// Today's evidence from the Teacher's snapshot, newest first.
List<EvidenceItem> evidenceItems(Map tools) {
  final evidence = tools['evidence'];
  if (evidence is! Map) return const [];
  return [
    for (final raw in (evidence['items'] as List? ?? const []))
      ?EvidenceItem.parse(raw),
  ];
}

int evidenceCountFor(Map tools, String studentId) =>
    evidenceItems(tools).where((e) => e.student == studentId).length;

String evidenceClock(DateTime t) {
  String two(int n) => n.toString().padLeft(2, '0');
  return '${two(t.hour)}:${two(t.minute)}';
}

String evidenceReason(String reason) => switch (reason) {
  'conversation' => 'đang trò chuyện',
  'in_use' => 'đang sử dụng',
  'continued' => 'tiếp tục sử dụng',
  _ => 'đang sử dụng',
};

/// Name the Teacher knows a computer by.
String evidenceWho(Agent agent, String studentId) {
  for (final row in agent.students) {
    if ('${row['id']}' == studentId) {
      final name = '${row['name'] ?? ''}'.trim();
      return name.isEmpty ? '${row['ip']}' : name;
    }
  }
  return studentId;
}

Future<void> showEvidence(
  BuildContext context,
  Agent agent, {
  String? studentId,
}) => showDialog<void>(
  context: context,
  builder: (_) => EvidenceDialog(agent: agent, studentId: studentId),
);

class EvidenceDialog extends StatefulWidget {
  const EvidenceDialog({super.key, required this.agent, this.studentId});
  final Agent agent;

  /// Show only this computer; null = the whole class.
  final String? studentId;
  @override
  State<EvidenceDialog> createState() => _EvidenceDialogState();
}

class _EvidenceDialogState extends State<EvidenceDialog> {
  String? error;

  Agent get agent => widget.agent;

  Future<void> delete(List<String> ids) async {
    final result = await agent.request('evidence_delete', input: {'ids': ids});
    if (mounted && result['ok'] != true) {
      setState(() => error = Agent.failure(result));
    }
  }

  Future<void> deleteAll(List<EvidenceItem> shown) async {
    final ok = await confirmDialog(
      context,
      title: 'Xóa ${shown.length} ảnh minh chứng?',
      icon: Icons.delete_outline,
      destructive: true,
      confirmLabel: 'Xóa',
      message:
          'Các ảnh sẽ bị xóa khỏi máy giảng viên. Không thể hoàn tác. (Ảnh vốn đã tự xóa khi qua ngày.)',
    );
    if (ok && mounted) await delete([for (final e in shown) e.id]);
  }

  Future<void> open(EvidenceItem item) async {
    await showDialog<void>(
      context: context,
      builder: (_) => _EvidenceDetail(
        agent: agent,
        item: item,
        onDelete: () => delete([item.id]),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: agent,
      builder: (context, _) {
        final all = evidenceItems((agent.state['tools'] as Map?) ?? const {});
        final shown = [
          for (final e in all)
            if (widget.studentId == null || e.student == widget.studentId) e,
        ];
        final byStudent = <String, List<EvidenceItem>>{};
        for (final e in shown) {
          (byStudent[e.student] ??= []).add(e);
        }
        return AppDialog(
          icon: Icons.photo_camera_outlined,
          title: widget.studentId == null
              ? 'Minh chứng dùng website AI'
              : 'Minh chứng · ${evidenceWho(agent, widget.studentId!)}',
          subtitle:
              'Ảnh màn hình được tự chụp khi học viên đang dùng website AI (lúc chưa chặn). Chỉ giữ trong ngày hôm nay, tự xóa khi qua ngày.',
          width: 820,
          height: 620,
          scrollable: false,
          bodyPadding: const EdgeInsets.fromLTRB(24, 8, 24, 0),
          footerLeading: TextButton.icon(
            key: const ValueKey('evidence-delete-all'),
            onPressed: shown.isEmpty ? null : () => deleteAll(shown),
            icon: const TIcon(Icons.delete_sweep_outlined, size: 18),
            label: const Text('Xóa tất cả'),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (error != null) InlineNotice(error!),
              Expanded(
                child: shown.isEmpty
                    ? const DialogEmpty(
                        icon: Icons.photo_camera_outlined,
                        title: 'Chưa có ảnh minh chứng hôm nay',
                        message:
                            'Ảnh chỉ được chụp khi "Theo dõi hoạt động" đang bật, website AI chưa bị chặn và học viên đang dùng trang đó.',
                      )
                    : ListView(
                        children: [
                          for (final entry in byStudent.entries) ...[
                            Padding(
                              padding: const EdgeInsets.fromLTRB(0, 12, 0, 6),
                              child: Text(
                                '${evidenceWho(agent, entry.key)} · ${entry.value.length} ảnh',
                                style: const TextStyle(
                                  fontWeight: FontWeight.w700,
                                  fontSize: 14,
                                ),
                              ),
                            ),
                            Wrap(
                              spacing: 10,
                              runSpacing: 10,
                              children: [
                                for (final item in entry.value)
                                  _Thumb(item: item, onTap: () => open(item)),
                              ],
                            ),
                          ],
                        ],
                      ),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _Thumb extends StatelessWidget {
  const _Thumb({required this.item, required this.onTap});
  final EvidenceItem item;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 236,
      child: InkWell(
        key: ValueKey('evidence-${item.id}'),
        onTap: onTap,
        borderRadius: BorderRadius.circular(10),
        child: Container(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(10),
            border: Border.all(color: AppDialog.border),
            color: Colors.white,
          ),
          clipBehavior: Clip.antiAlias,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              AspectRatio(
                aspectRatio: 16 / 9,
                child: Image.file(
                  File(item.path),
                  fit: BoxFit.cover,
                  cacheWidth: 480,
                  errorBuilder: (_, _, _) => const ColoredBox(
                    color: Color(0xffe8edf1),
                    child: Center(
                      child: TIcon(Icons.broken_image_outlined, size: 32),
                    ),
                  ),
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(10, 8, 10, 8),
                child: Row(
                  children: [
                    SiteIcon(
                      url: 'https://${item.label.split('/').first}',
                      size: 18,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            item.label,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontWeight: FontWeight.w600,
                              fontSize: 12.5,
                            ),
                          ),
                          Text(
                            '${evidenceClock(item.taken)} · ${evidenceReason(item.reason)}',
                            style: const TextStyle(
                              fontSize: 11.5,
                              color: AppDialog.muted,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _EvidenceDetail extends StatelessWidget {
  const _EvidenceDetail({
    required this.agent,
    required this.item,
    required this.onDelete,
  });
  final Agent agent;
  final EvidenceItem item;
  final Future<void> Function() onDelete;

  /// A copy on the Desktop, because the original disappears at midnight.
  Future<String> keep() async {
    final home =
        Platform.environment['USERPROFILE'] ??
        Platform.environment['HOME'] ??
        '';
    final sep = Platform.pathSeparator;
    final folder = Directory('$home${sep}Desktop${sep}Minh chung AI');
    await folder.create(recursive: true);
    String two(int n) => n.toString().padLeft(2, '0');
    final t = item.taken;
    final name =
        '${evidenceWho(agent, item.student)}-${item.label.replaceAll('/', '_')}-${t.year}${two(t.month)}${two(t.day)}-${two(t.hour)}${two(t.minute)}${two(t.second)}.jpg'
            .replaceAll(RegExp(r'[\\/:*?"<>|]'), '_');
    final target = '${folder.path}$sep$name';
    await File(item.path).copy(target);
    return target;
  }

  @override
  Widget build(BuildContext context) {
    return AppDialog(
      icon: Icons.photo_camera_outlined,
      title: '${evidenceWho(agent, item.student)} · ${item.label}',
      subtitle:
          '${evidenceClock(item.taken)} · ${evidenceReason(item.reason)}${item.title.isEmpty ? '' : ' · “${item.title}”'}',
      width: 900,
      height: 640,
      scrollable: false,
      bodyPadding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      footerLeading: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextButton.icon(
            key: const ValueKey('evidence-keep'),
            onPressed: () async {
              final messenger = ScaffoldMessenger.maybeOf(context);
              try {
                final saved = await keep();
                messenger?.showSnackBar(
                  SnackBar(content: Text('Đã lưu một bản vào: $saved')),
                );
              } catch (e) {
                messenger?.showSnackBar(
                  SnackBar(content: Text('Không lưu được: $e')),
                );
              }
            },
            icon: const TIcon(Icons.save_alt, size: 18),
            label: const Text('Lưu vào Desktop'),
          ),
          TextButton.icon(
            key: const ValueKey('evidence-remove'),
            onPressed: () async {
              Navigator.pop(context);
              await onDelete();
            },
            icon: const TIcon(
              Icons.delete_outline,
              size: 18,
              color: Color(0xffb83b40),
            ),
            label: const Text('Xóa ảnh này'),
          ),
        ],
      ),
      child: InteractiveViewer(
        maxScale: 4,
        child: Center(
          child: Image.file(
            File(item.path),
            errorBuilder: (_, _, _) =>
                const Text('Không mở được ảnh (đã bị xóa?)'),
          ),
        ),
      ),
    );
  }
}
