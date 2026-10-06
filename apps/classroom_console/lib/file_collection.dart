import 'package:flutter/material.dart';
import 'agent.dart';

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
  'Documents': 'doc, docx, pdf, txt, rtf, odt, pages',
  'Spreadsheets & slides': 'xls, xlsx, csv, ppt, pptx, key, numbers',
  'Images': 'png, jpg, jpeg, gif, bmp, heic',
  'Code': 'py, java, c, cpp, h, cs, js, html, css, sql, ipynb',
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
      setState(() => error = 'Enter the folder path on the student computers.');
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
    return AlertDialog(
      title: Text('Collect files from ${widget.audience}'),
      content: SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              DropdownButtonFormField<String>(
                initialValue: place,
                decoration: const InputDecoration(
                  labelText: 'Folder on the student computer',
                  border: OutlineInputBorder(),
                  isDense: true,
                ),
                items: const [
                  DropdownMenuItem(value: '~desktop', child: Text('Desktop')),
                  DropdownMenuItem(
                    value: '~documents',
                    child: Text('Documents'),
                  ),
                  DropdownMenuItem(
                    value: '~downloads',
                    child: Text('Downloads'),
                  ),
                  DropdownMenuItem(
                    value: 'custom',
                    child: Text('Other folder…'),
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
                    hintText: r'C:\Users\Public\Work   or   /Users/Shared/Work',
                    helperText:
                        'Full path as it appears on the student computer',
                    border: OutlineInputBorder(),
                    isDense: true,
                  ),
                ),
              ],
              const SizedBox(height: 12),
              TextField(
                controller: types,
                enabled: !busy,
                decoration: const InputDecoration(
                  labelText: 'File types (optional)',
                  hintText: 'docx, pdf, py — leave empty for every file',
                  border: OutlineInputBorder(),
                  isDense: true,
                ),
              ),
              const SizedBox(height: 6),
              Wrap(
                spacing: 6,
                runSpacing: 0,
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
              CheckboxListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                controlAffinity: ListTileControlAffinity.leading,
                value: subfolders,
                onChanged: busy ? null : (v) => setState(() => subfolders = v!),
                title: const Text('Include subfolders'),
              ),
              CheckboxListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                controlAffinity: ListTileControlAffinity.leading,
                value: flat,
                onChanged: busy ? null : (v) => setState(() => flat = v!),
                title: const Text('Put all files in one folder per student'),
                subtitle: const Text(
                  'Off: keep each student\'s own folder structure. Duplicate names are numbered.',
                  style: TextStyle(fontSize: 11),
                ),
              ),
              Row(
                children: [
                  const Text('Skip files larger than'),
                  const SizedBox(width: 10),
                  DropdownButton<int>(
                    value: maxMb,
                    isDense: true,
                    items: [
                      for (final mb in [10, 50, 200, 1000, 4000])
                        DropdownMenuItem(value: mb, child: Text('$mb MB')),
                    ],
                    onChanged: busy ? null : (v) => setState(() => maxMb = v!),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              Text(
                'Saved on this Desktop in “$label”, one folder per student.',
                style: const TextStyle(fontSize: 12, color: Colors.blueGrey),
              ),
              if (error != null) ...[
                const SizedBox(height: 8),
                Text(error!, style: const TextStyle(color: Color(0xffb83b40))),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: busy ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton.icon(
          onPressed: busy ? null : start,
          icon: const Icon(Icons.download_outlined, size: 18),
          label: Text(busy ? 'Starting…' : 'Collect'),
        ),
      ],
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
    final files = '${t['files_done']}/${t['files_total']} files';
    switch (t['status']) {
      case 'queued':
        return 'Waiting for the student…';
      case 'transferring':
        final current = '${t['current']}';
        return current.isEmpty ? files : '$files · $current';
      case 'complete':
        final extra = <String>[
          if ((t['skipped'] as num) > 0) '${t['skipped']} skipped',
          if ((t['failed'] as num) > 0) '${t['failed']} failed',
        ];
        final size = formatBytes(t['bytes_done'] as num);
        return (t['files_total'] as num) == 0
            ? 'No matching files'
            : '${t['files_done']} files · $size${extra.isEmpty ? '' : ' · ${extra.join(', ')}'}';
      case 'cancelled':
        return 'Cancelled';
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
          return AlertDialog(
            title: const Text('File collection'),
            content: const Text('No collection has been started.'),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: const Text('Close'),
              ),
            ],
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
        return AlertDialog(
          title: Text(
            active > 0
                ? 'Collecting files… ($done/${targets.length} students)'
                : 'Collection finished',
          ),
          content: SizedBox(
            width: 520,
            height: 380,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '$files files · ${formatBytes(bytes)} → ${j['dest']}',
                  style: const TextStyle(fontSize: 12, color: Colors.blueGrey),
                ),
                const SizedBox(height: 8),
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
                                Icon(
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
          ),
          actions: [
            if (active > 0)
              TextButton(
                onPressed: () =>
                    agent.request('fs_cancel', input: {'id': j['id']}),
                child: const Text('Cancel collection'),
              ),
            TextButton.icon(
              onPressed: () =>
                  agent.request('fs_open', input: {'path': j['dest']}),
              icon: const Icon(Icons.folder_open, size: 18),
              label: const Text('Open folder'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context),
              child: Text(active > 0 ? 'Hide' : 'Close'),
            ),
          ],
        );
      },
    );
  }
}
