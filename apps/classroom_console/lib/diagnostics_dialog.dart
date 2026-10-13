import 'dart:io';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';

Future<void> collectDiagnostics(
  BuildContext context,
  Agent agent,
  List<String> ids,
) async {
  try {
    await agent.commandChecked('collect_diagnostics', ids, const {});
  } catch (error) {
    if (context.mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('$error')));
    }
    return;
  }
  if (!context.mounted) return;
  await showDialog<void>(
    context: context,
    builder: (_) => AnimatedBuilder(
      animation: agent,
      builder: (context, _) {
        final status = agent.state['diagnostics'] as Map? ?? const {};
        final phase = status['phase'];
        final rows = status['students'] as List? ?? const [];
        final message = switch (phase) {
          'waiting_for_stop' =>
            'Việc thu thập sẽ bắt đầu sau khi bạn dừng chiếu.',
          'collecting' =>
            'Đang thu thập ${status['completed'] ?? 0}/${status['total'] ?? 0} máy…',
          'complete' =>
            'Đã thu ${status['collected']}/${status['total']} học viên. ${status['missing']} máy không khả dụng.\n${status['path']}',
          'error' => 'Thu thập thất bại: ${status['error']}',
          'cancelled' => 'Đã hủy thu thập.',
          _ => 'Đang chuẩn bị thu thập…',
        };
        final done = phase == 'complete';
        final failed = phase == 'error';
        return AppDialog(
          icon: done
              ? Icons.check_circle_outline
              : failed
              ? Icons.error_outline
              : Icons.bug_report_outlined,
          tone: done
              ? AppTone.success
              : failed
              ? AppTone.danger
              : AppTone.brand,
          title: 'Chẩn đoán lớp học',
          subtitle:
              'Gồm nhật ký chạy và thông tin bản build của Teacher và Student.',
          width: 560,
          footerLeading: phase == 'collecting' || phase == 'waiting_for_stop'
              ? Align(
                  alignment: Alignment.centerLeft,
                  child: TextButton(
                    onPressed: () => agent.command('cancel_diagnostics'),
                    child: const Text('Hủy thu thập'),
                  ),
                )
              : null,
          actions: [
            if (done && status['path'] is String)
              AppDialog.secondary(
                icon: Icons.folder_zip_outlined,
                label: 'Hiện tệp ZIP',
                onPressed: () async {
                  try {
                    final path = status['path'] as String;
                    await Process.start(
                      Platform.isWindows ? 'explorer.exe' : 'open',
                      Platform.isWindows ? ['/select,$path'] : ['-R', path],
                    );
                  } catch (error) {
                    if (context.mounted) {
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(
                          content: Text('Không mở được tệp lưu trữ: $error'),
                        ),
                      );
                    }
                  }
                },
              ),
            AppDialog.primary(
              onPressed: () => Navigator.pop(context),
              icon: Icons.check,
              label: 'Đóng',
            ),
          ],
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (phase == 'collecting') ...[
                const LinearProgressIndicator(),
                const SizedBox(height: 12),
              ],
              if (failed)
                InlineNotice(message)
              else if (done)
                InlineNotice(message, tone: AppTone.success)
              else
                SelectableText(message),
              for (final row in rows.where((r) => r['status'] != 'collected'))
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Text(
                    "${row['name']} (${row['ip']}): ${row['reason']}",
                    style: const TextStyle(fontSize: 12.5),
                  ),
                ),
              const SizedBox(height: 14),
              const Text(
                'Máy offline được liệt kê là không khả dụng. Không gồm ảnh chụp màn hình và nội dung tin nhắn/tệp.',
                style: TextStyle(
                  fontSize: 12,
                  color: AppDialog.muted,
                  height: 1.4,
                ),
              ),
            ],
          ),
        );
      },
    ),
  );
}
