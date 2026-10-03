import 'package:flutter/material.dart';
import 'agent.dart';

/// Compact file transfer status and student reply card.
class ClassroomToolsOverlay extends StatefulWidget {
  const ClassroomToolsOverlay({
    super.key,
    required this.agent,
    this.preparingFile = false,
  });
  final Agent agent;
  final bool preparingFile;

  @override
  State<ClassroomToolsOverlay> createState() => _ClassroomToolsOverlayState();
}

class _ClassroomToolsOverlayState extends State<ClassroomToolsOverlay> {
  final hiddenReplies = <String>{};

  @override
  Widget build(BuildContext context) {
    if (widget.preparingFile) {
      return Material(
        elevation: 8,
        color: const Color(0xff172b38),
        borderRadius: BorderRadius.circular(12),
        child: SizedBox(
          width: 330,
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: const [
                Text(
                  'Preparing file…',
                  style: TextStyle(
                    color: Colors.white,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                SizedBox(height: 10),
                LinearProgressIndicator(
                  minHeight: 5,
                  borderRadius: BorderRadius.all(Radius.circular(8)),
                ),
              ],
            ),
          ),
        ),
      );
    }
    final tools = Map<String, dynamic>.from(
      widget.agent.state['tools'] as Map? ?? {},
    );
    final jobs = (tools['jobs'] as List? ?? [])
        .map((e) => Map<String, dynamic>.from(e as Map))
        .toList()
        .reversed
        .toList();
    final active = jobs.where((job) {
      final targets = (job['targets'] as List? ?? []).cast<Map>();
      return targets.any(
        (target) => !const {
          'complete',
          'replied',
          'dismissed',
          'cancelled',
          'error',
        }.contains(target['status']),
      );
    }).toList();
    final retryable = jobs
        .where(
          (job) =>
              job['kind'] == 'file' &&
              !hiddenReplies.contains('${job['id']}') &&
              (job['targets'] as List? ?? []).any(
                (t) =>
                    (t as Map)['status'] == 'error' ||
                    (t)['status'] == 'cancelled',
              ),
        )
        .toList();
    final replies = jobs
        .where(
          (job) =>
              job['kind'] == 'message' &&
              !hiddenReplies.contains('${job['id']}') &&
              (job['targets'] as List? ?? []).any(
                (target) => '${(target as Map)['reply'] ?? ''}'.isNotEmpty,
              ),
        )
        .toList();
    final failedMessages = jobs
        .where(
          (job) =>
              job['kind'] == 'message' &&
              !hiddenReplies.contains('${job['id']}') &&
              (job['targets'] as List? ?? []).any(
                (target) => (target as Map)['status'] == 'error',
              ),
        )
        .toList();
    if (active.isEmpty &&
        replies.isEmpty &&
        retryable.isEmpty &&
        failedMessages.isEmpty) {
      return const SizedBox.shrink();
    }

    final job = active.isNotEmpty
        ? active.first
        : retryable.isNotEmpty
        ? retryable.first
        : replies.isNotEmpty
        ? replies.first
        : failedMessages.first;
    final targets = (job['targets'] as List? ?? [])
        .map((e) => Map<String, dynamic>.from(e as Map))
        .toList();
    final total = (job['size'] as num? ?? 0).toInt();
    final done = targets.fold<int>(
      0,
      (sum, target) => sum + (target['done'] as num? ?? 0).toInt(),
    );
    final maximum = total * targets.length;
    final progress = maximum == 0 ? null : (done / maximum).clamp(0.0, 1.0);
    final isFile = job['kind'] == 'file';
    final names = {
      for (final student in widget.agent.students)
        '${student['id']}':
            '${student['name'] ?? student['ip'] ?? student['id']}',
    };
    return Material(
      elevation: 8,
      color: const Color(0xff172b38),
      borderRadius: BorderRadius.circular(12),
      child: SizedBox(
        width: 330,
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(
                    isFile
                        ? Icons.upload_file
                        : Icons.mark_chat_unread_outlined,
                    color: const Color(0xff7dd3dc),
                    size: 18,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      isFile
                          ? 'Sending ${job['name'] ?? 'file'}'
                          : 'Message to students',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        color: Colors.white,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  IconButton(
                    visualDensity: VisualDensity.compact,
                    tooltip: isFile
                        ? 'Cancel transfer'
                        : 'Dismiss message status',
                    onPressed: isFile
                        ? () {
                            if (active.contains(job)) {
                              widget.agent.command('tool_cancel', const [], {
                                'id': job['id'],
                              });
                            } else {
                              setState(() => hiddenReplies.add('${job['id']}'));
                            }
                          }
                        : () =>
                              setState(() => hiddenReplies.add('${job['id']}')),
                    icon: const Icon(
                      Icons.close,
                      size: 17,
                      color: Colors.white70,
                    ),
                  ),
                ],
              ),
              if (isFile) ...[
                const SizedBox(height: 5),
                LinearProgressIndicator(
                  value: progress,
                  minHeight: 5,
                  borderRadius: BorderRadius.circular(8),
                  backgroundColor: Colors.white24,
                  color: const Color(0xff55c2cc),
                ),
                const SizedBox(height: 7),
                Text(
                  '${_size(done)} / ${_size(maximum)} · '
                  '${targets.where((t) => t['status'] == 'complete').length}/${targets.length} students',
                  style: const TextStyle(
                    color: Color(0xffc7d4dc),
                    fontSize: 11,
                  ),
                ),
              ],
              if (isFile)
                for (final target in targets.take(3))
                  Padding(
                    padding: const EdgeInsets.only(top: 5),
                    child: Text(
                      '${names['${target['id']}'] ?? target['id']} · '
                      '${target['status']} · ${_size((target['done'] as num? ?? 0).toInt())}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        color: Color(0xffc7d4dc),
                        fontSize: 10,
                      ),
                    ),
                  ),
              if (isFile && targets.length > 3)
                Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: Text(
                    '+ ${targets.length - 3} more Students',
                    style: const TextStyle(
                      color: Color(0xffc7d4dc),
                      fontSize: 10,
                    ),
                  ),
                ),
              for (final target
                  in targets
                      .where(
                        (t) => t['reply'] != null && '${t['reply']}'.isNotEmpty,
                      )
                      .take(2))
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Text(
                    'Reply: ${target['reply']}',
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: Color(0xffc7d4dc),
                      fontSize: 11,
                    ),
                  ),
                ),
              for (final target
                  in targets
                      .where(
                        (t) =>
                            t['status'] == 'error' &&
                            '${t['error'] ?? ''}'.isNotEmpty,
                      )
                      .take(2))
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Text(
                    'Could not deliver to ${names['${target['id']}'] ?? target['id']}: ${target['error']}',
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: Color(0xffffc1a8),
                      fontSize: 11,
                    ),
                  ),
                ),
              if (isFile &&
                  targets.any(
                    (t) => t['status'] == 'error' || t['status'] == 'cancelled',
                  ))
                Align(
                  alignment: Alignment.centerRight,
                  child: TextButton.icon(
                    onPressed: () => widget.agent.command(
                      'tool_retry',
                      const [],
                      {'id': job['id']},
                    ),
                    icon: const Icon(Icons.refresh, size: 15),
                    label: const Text('Retry'),
                    style: TextButton.styleFrom(
                      foregroundColor: const Color(0xff91e1e7),
                      visualDensity: VisualDensity.compact,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }

  static String _size(int bytes) {
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(0)} KB';
    if (bytes < 1024 * 1024 * 1024) {
      return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
    }
    return '${(bytes / (1024 * 1024 * 1024)).toStringAsFixed(2)} GB';
  }
}
