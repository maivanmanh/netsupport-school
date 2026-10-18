import 'icon_tints.dart';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';

String formatBytes(num bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  var value = bytes.toDouble();
  var unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return '${value.toStringAsFixed(unit == 0 || value >= 100 ? 0 : 1)} ${units[unit]}';
}

bool jobTargetActive(Map<String, dynamic> target) =>
    target['status'] == 'queued' || target['status'] == 'transferring';

String _two(int v) => v.toString().padLeft(2, '0');

/// `Collected 2026-10-06 14.32` — local time, safe on Windows and macOS.
String collectionLabel([DateTime? now]) {
  final t = now ?? DateTime.now();
  return 'Collected ${t.year}-${_two(t.month)}-${_two(t.day)} ${_two(t.hour)}.${_two(t.minute)}';
}

const _typePresets = <String, String>{
  'Tài liệu': 'doc, docx, pdf, txt, rtf, odt, pages',
  'Bảng tính & trình chiếu': 'xls, xlsx, csv, ppt, pptx, key, numbers',
  'Hình ảnh': 'png, jpg, jpeg, gif, bmp, heic',
  'Mã nguồn': 'py, java, c, cpp, h, cs, js, html, css, sql, ipynb',
};

/// Asks what to collect, starts the job, then shows its progress.
Future<void> showCollectDialog(
  BuildContext context,
  Agent agent, {
  required List<String> studentIds,
  required String audience,
}) async {
  final started = await showDialog<Map<String, dynamic>>(
    context: context,
    builder: (_) =>
        _CollectDialog(agent: agent, ids: studentIds, audience: audience),
  );
  if (started == null || !context.mounted) return;
  await showCollectProgress(
    context,
    agent,
    jobId: started['job_id'] as String?,
  );
}

class _CollectDialog extends StatefulWidget {
  const _CollectDialog({
    required this.agent,
    required this.ids,
    required this.audience,
  });
  final Agent agent;
  final List<String> ids;
  final String audience;
  @override
  State<_CollectDialog> createState() => _CollectDialogState();
}

class _CollectDialogState extends State<_CollectDialog> {
  String place = '~desktop';
  final custom = TextEditingController();
  final types = TextEditingController();
  bool subfolders = true, flat = false, busy = false;
  int maxMb = 200;
  String? error;
  final label = collectionLabel();

  @override
  void dispose() {
    custom.dispose();
    types.dispose();
    super.dispose();
  }

  Future<void> start() async {
    final path = place == 'custom' ? custom.text.trim() : place;
    if (path.isEmpty) {
      setState(() => error = 'Hãy nhập đường dẫn thư mục trên máy học viên.');
      return;
    }
    setState(() {
      busy = true;
      error = null;
    });
    final result = await widget.agent.request(
      'fs_collect',
      selected: widget.ids,
      input: {
        'path': path,
        'exts': types.text,
        'recursive': subfolders,
        'flat': flat,
        'max_file_mb': maxMb,
        'label': label,
      },
      timeout: const Duration(seconds: 15),
    );
    if (!mounted) return;
    if (result['ok'] == true) {
      Navigator.pop(context, result);
    } else {
      setState(() {
        busy = false;
        error = Agent.failure(result);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return AppDialog(
      icon: Icons.download_for_offline_outlined,
      title: 'Thu tệp từ ${widget.audience}',
      subtitle:
          'Tệp được lưu trên Desktop trong thư mục “$label”, mỗi học viên một thư mục con.',
      width: 560,
      actions: [
        AppDialog.cancel(context, enabled: !busy),
        AppDialog.primary(
          label: busy ? 'Đang bắt đầu…' : 'Bắt đầu thu',
          icon: Icons.download_outlined,
          busy: busy,
          onPressed: start,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          DialogSection(
            title: 'Thư mục trên máy học viên',
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                DropdownButtonFormField<String>(
                  initialValue: place,
                  decoration: const InputDecoration(isDense: true),
                  items: const [
                    DropdownMenuItem(value: '~desktop', child: Text('Desktop')),
                    DropdownMenuItem(
                      value: '~documents',
                      child: Text('Documents (Tài liệu)'),
                    ),
                    DropdownMenuItem(
                      value: '~downloads',
                      child: Text('Downloads (Tải xuống)'),
                    ),
                    DropdownMenuItem(
                      value: 'custom',
                      child: Text('Thư mục khác…'),
                    ),
                  ],
                  onChanged: busy ? null : (v) => setState(() => place = v!),
                ),
                if (place == 'custom') ...[
                  const SizedBox(height: 8),
                  TextField(
                    controller: custom,
                    enabled: !busy,
                    decoration: const InputDecoration(
                      hintText:
                          r'C:\Users\Public\Work   hoặc   /Users/Shared/Work',
                      helperText: 'Đường dẫn đầy đủ như trên máy học viên',
                      isDense: true,
                    ),
                  ),
                ],
              ],
            ),
          ),
          DialogSection(
            title: 'Loại tệp',
            hint: 'Để trống để thu mọi tệp.',
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                TextField(
                  controller: types,
                  enabled: !busy,
                  decoration: const InputDecoration(
                    hintText: 'docx, pdf, py …',
                    isDense: true,
                  ),
                ),
                const SizedBox(height: 8),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    for (final preset in _typePresets.entries)
                      ActionChip(
                        visualDensity: VisualDensity.compact,
                        label: Text(
                          preset.key,
                          style: const TextStyle(fontSize: 12),
                        ),
                        onPressed: busy
                            ? null
                            : () => setState(() => types.text = preset.value),
                      ),
                  ],
                ),
              ],
            ),
          ),
          DialogSection(
            title: 'Tùy chọn',
            padBottom: 4,
            child: Column(
              children: [
                CheckboxListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  controlAffinity: ListTileControlAffinity.leading,
                  value: subfolders,
                  onChanged: busy
                      ? null
                      : (v) => setState(() => subfolders = v!),
                  title: const Text('Gồm cả thư mục con'),
                ),
                CheckboxListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  controlAffinity: ListTileControlAffinity.leading,
                  value: flat,
                  onChanged: busy ? null : (v) => setState(() => flat = v!),
                  title: const Text(
                    'Gom mọi tệp vào một thư mục cho mỗi học viên',
                  ),
                  subtitle: const Text(
                    'Tắt: giữ nguyên cấu trúc thư mục của từng học viên. Tên trùng được đánh số.',
                    style: TextStyle(fontSize: 11.5),
                  ),
                ),
                const SizedBox(height: 4),
                Row(
                  children: [
                    const Text('Bỏ qua tệp lớn hơn'),
                    const SizedBox(width: 10),
                    DropdownButton<int>(
                      value: maxMb,
                      isDense: true,
                      items: [
                        for (final mb in [10, 50, 200, 1000, 4000])
                          DropdownMenuItem(value: mb, child: Text('$mb MB')),
                      ],
                      onChanged: busy
                          ? null
                          : (v) => setState(() => maxMb = v!),
                    ),
                  ],
                ),
              ],
            ),
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

/// Live progress of the newest (or given) collection job. Closing the dialog
/// never stops the job; reopen it from the toolbar.
Future<void> showCollectProgress(
  BuildContext context,
  Agent agent, {
  String? jobId,
}) => showDialog<void>(
  context: context,
  builder: (_) => _CollectProgress(agent: agent, jobId: jobId),
);

class _CollectProgress extends StatelessWidget {
  const _CollectProgress({required this.agent, this.jobId});
  final Agent agent;
  final String? jobId;

  Map<String, dynamic>? job() {
    final jobs = agent.fileJobs.where((j) => j['kind'] == 'collect').toList();
    if (jobId != null) {
      for (final j in jobs) {
        if (j['id'] == jobId) return j;
      }
    }
    return jobs.isEmpty ? null : jobs.last;
  }

  String describe(Map<String, dynamic> t) {
    final files = '${t['files_done']}/${t['files_total']} tệp';
    switch (t['status']) {
      case 'queued':
        return 'Đang chờ máy học viên…';
      case 'transferring':
        final current = '${t['current']}';
        return current.isEmpty ? files : '$files · $current';
      case 'complete':
        final extra = <String>[
          if ((t['skipped'] as num) > 0) 'bỏ qua ${t['skipped']}',
          if ((t['failed'] as num) > 0) 'lỗi ${t['failed']}',
        ];
        final size = formatBytes(t['bytes_done'] as num);
        return (t['files_total'] as num) == 0
            ? 'Không có tệp phù hợp'
            : '${t['files_done']} tệp · $size${extra.isEmpty ? '' : ' · ${extra.join(', ')}'}';
      case 'cancelled':
        return 'Đã hủy';
      default:
        return '${t['error']}';
    }
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: agent,
      builder: (context, _) {
        final j = job();
        if (j == null) {
          return AppDialog(
            icon: Icons.download_for_offline_outlined,
            title: 'Thu tệp',
            width: 440,
            child: const DialogEmpty(
              icon: Icons.inbox_outlined,
              title: 'Chưa có đợt thu tệp nào',
              message: 'Chọn “Thu bài / tệp” để bắt đầu một đợt thu mới.',
            ),
          );
        }
        final targets =
            (j['targets'] as List)
                .map((e) => Map<String, dynamic>.from(e as Map))
                .toList()
              ..sort((a, b) => '${a['name']}'.compareTo('${b['name']}'));
        final active = targets.where(jobTargetActive).length;
        final done = targets.where((t) => t['status'] == 'complete').length;
        final files = targets.fold<int>(
          0,
          (sum, t) => sum + (t['files_done'] as num).toInt(),
        );
        final bytes = targets.fold<num>(
          0,
          (sum, t) => sum + (t['bytes_done'] as num),
        );
        return AppDialog(
          icon: active > 0
              ? Icons.downloading_outlined
              : Icons.check_circle_outline,
          tone: active > 0 ? AppTone.brand : AppTone.success,
          title: active > 0
              ? 'Đang thu tệp… ($done/${targets.length} học viên)'
              : 'Đã thu xong',
          subtitle: '$files tệp · ${formatBytes(bytes)} → ${j['dest']}',
          width: 580,
          height: 380,
          scrollable: false,
          bodyPadding: EdgeInsets.zero,
          footerLeading: active > 0
              ? AppDialog.cancel(
                  context,
                  label: 'Hủy đợt thu',
                  onPressed: () =>
                      agent.request('fs_cancel', input: {'id': j['id']}),
                )
              : null,
          actions: [
            AppDialog.secondary(
              onPressed: () =>
                  agent.request('fs_open', input: {'path': j['dest']}),
              icon: Icons.folder_open,
              label: 'Mở thư mục',
            ),
          ],
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: ListView.separated(
                  itemCount: targets.length,
                  separatorBuilder: (_, _) => const Divider(height: 1),
                  itemBuilder: (_, i) {
                    final t = targets[i];
                    final total = (t['bytes_total'] as num).toDouble();
                    final ratio = total > 0
                        ? ((t['bytes_done'] as num) / total).clamp(0.0, 1.0)
                        : (t['status'] == 'complete' ? 1.0 : null);
                    final failed = t['status'] == 'error';
                    return Padding(
                      padding: const EdgeInsets.symmetric(vertical: 6),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              TIcon(
                                failed
                                    ? Icons.error_outline
                                    : t['status'] == 'complete'
                                    ? Icons.check_circle_outline
                                    : Icons.computer,
                                size: 16,
                                color: failed
                                    ? const Color(0xffb83b40)
                                    : Colors.blueGrey,
                              ),
                              const SizedBox(width: 6),
                              Expanded(
                                child: Text(
                                  '${t['name']}',
                                  style: const TextStyle(
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(height: 3),
                          if (jobTargetActive(t))
                            LinearProgressIndicator(
                              value: t['status'] == 'queued' ? null : ratio,
                              minHeight: 4,
                            ),
                          Tooltip(
                            message: (t['notes'] as List).join('\n'),
                            child: Text(
                              describe(t),
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 11,
                                color: failed
                                    ? const Color(0xffb83b40)
                                    : Colors.blueGrey,
                              ),
                            ),
                          ),
                        ],
                      ),
                    );
                  },
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}
