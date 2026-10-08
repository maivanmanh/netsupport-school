import 'dart:io';
import 'package:flutter/material.dart';
import 'agent.dart';

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
          'waiting_for_stop' => 'Collection will begin after you stop sharing.',
          'collecting' =>
            'Collecting ${status['completed'] ?? 0}/${status['total'] ?? 0} computers…',
          'complete' =>
            'Collected ${status['collected']}/${status['total']} Students. ${status['missing']} unavailable.\n${status['path']}',
          'error' => 'Collection failed: ${status['error']}',
          'cancelled' => 'Collection cancelled.',
          _ => 'Preparing collection…',
        };
        return AlertDialog(
          title: const Text('Classroom diagnostics'),
          content: SizedBox(
            width: 520,
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (phase == 'collecting') const LinearProgressIndicator(),
                  const SizedBox(height: 12),
                  SelectableText(message),
                  for (final row in rows.where(
                    (r) => r['status'] != 'collected',
                  ))
                    Text("${row['name']} (${row['ip']}): ${row['reason']}"),
                  const SizedBox(height: 12),
                  const Text(
                    'Includes Teacher and Student runtime logs and build information. Offline computers are listed as unavailable. Screenshots and message/file contents are excluded.',
                  ),
                ],
              ),
            ),
          ),
          actions: [
            if (phase == 'complete' && status['path'] is String)
              TextButton(
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
                          content: Text('Could not open archive: $error'),
                        ),
                      );
                    }
                  }
                },
                child: const Text('Show ZIP'),
              ),
            if (phase == 'collecting' || phase == 'waiting_for_stop')
              TextButton(
                onPressed: () => agent.command('cancel_diagnostics'),
                child: const Text('Cancel collection'),
              ),
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Close'),
            ),
          ],
        );
      },
    ),
  );
}
