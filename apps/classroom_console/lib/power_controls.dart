import 'dart:async';
import 'package:flutter/material.dart';
import 'agent.dart';

/// One entry of the class-wide "Power & screen" menu.
class PowerAction {
  const PowerAction(this.op, this.label, this.icon, this.verb);
  final String op, label, verb;
  final IconData icon;
}

const lockAction = PowerAction(
  'lock',
  'Lock screens',
  Icons.lock_outline,
  'Locked',
);
const unlockAction = PowerAction(
  'unlock',
  'Unlock screens',
  Icons.lock_open_outlined,
  'Unlocked',
);
const logoutAction = PowerAction(
  'logout',
  'Log out users',
  Icons.logout,
  'Logging out',
);
const rebootAction = PowerAction(
  'reboot',
  'Restart computers',
  Icons.restart_alt,
  'Restarting',
);
const shutdownAction = PowerAction(
  'shutdown',
  'Shut down computers',
  Icons.power_settings_new,
  'Shutting down',
);
const clipboardBlockAction = PowerAction(
  'clipboard_block',
  'Block clipboard (exam mode)',
  Icons.content_paste_off,
  'Clipboard blocked',
);
const clipboardAllowAction = PowerAction(
  'clipboard_allow',
  'Allow clipboard',
  Icons.content_paste,
  'Clipboard allowed',
);

/// Menu entries, grouped: screen lock first, then the disruptive actions.
List<PopupMenuEntry<String>> powerMenuItems({
  required bool anyLocked,
  required bool anyClipboardBlocked,
}) => [
  const PopupMenuItem(
    value: 'lock',
    child: ListTile(
      dense: true,
      leading: Icon(Icons.lock_outline),
      title: Text('Lock screens…'),
    ),
  ),
  PopupMenuItem(
    value: 'unlock',
    child: ListTile(
      dense: true,
      leading: const Icon(Icons.lock_open_outlined),
      title: const Text('Unlock screens'),
      subtitle: anyLocked ? null : const Text('None locked right now'),
    ),
  ),
  PopupMenuItem(
    value: anyClipboardBlocked ? 'clipboard_allow' : 'clipboard_block',
    child: ListTile(
      dense: true,
      leading: Icon(
        anyClipboardBlocked ? Icons.content_paste : Icons.content_paste_off,
      ),
      title: Text(
        anyClipboardBlocked ? 'Allow clipboard' : 'Block clipboard (exam mode)',
      ),
      subtitle: Text(
        anyClipboardBlocked
            ? 'Clipboard is blocked right now'
            : 'Clears anything copied, on every app — stops answer sharing',
      ),
    ),
  ),
  const PopupMenuDivider(),
  const PopupMenuItem(
    value: 'logout',
    child: ListTile(
      dense: true,
      leading: Icon(Icons.logout),
      title: Text('Log out users…'),
    ),
  ),
  const PopupMenuItem(
    value: 'reboot',
    child: ListTile(
      dense: true,
      leading: Icon(Icons.restart_alt),
      title: Text('Restart…'),
    ),
  ),
  const PopupMenuItem(
    value: 'shutdown',
    child: ListTile(
      dense: true,
      leading: Icon(Icons.power_settings_new, color: Color(0xffb83b40)),
      title: Text('Shut down…'),
    ),
  ),
];

PowerAction actionFor(String op) => switch (op) {
  'lock' => lockAction,
  'unlock' => unlockAction,
  'logout' => logoutAction,
  'reboot' => rebootAction,
  'clipboard_block' => clipboardBlockAction,
  'clipboard_allow' => clipboardAllowAction,
  _ => shutdownAction,
};

/// Confirms (disruptive actions) or asks for a lock message, then sends the
/// command and reports a one-line result.
Future<void> runPower(
  BuildContext context,
  Agent agent, {
  required String op,
  required List<String> ids,
  required String audience,
  bool classWide = true,
}) async {
  if (ids.isEmpty) return;
  final action = actionFor(op);
  var message = '';
  if (op == 'lock') {
    final controller = TextEditingController(
      text: 'Please look at the teacher.',
    );
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Lock $audience'),
        content: SizedBox(
          width: 400,
          child: TextField(
            controller: controller,
            autofocus: true,
            maxLength: 200,
            maxLines: 2,
            decoration: const InputDecoration(
              labelText: 'Message shown on the locked screen',
              border: OutlineInputBorder(),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton.icon(
            onPressed: () => Navigator.pop(context, true),
            icon: const Icon(Icons.lock_outline, size: 18),
            label: const Text('Lock'),
          ),
        ],
      ),
    );
    message = controller.text.trim();
    controller.dispose();
    if (ok != true) return;
  } else if (op != 'unlock' &&
      op != 'clipboard_block' &&
      op != 'clipboard_allow') {
    final detail = switch (op) {
      'logout' =>
        'Students are signed out immediately. Unsaved work may be lost.',
      'reboot' =>
        'Students get a 15 second warning, then the computers restart. Unsaved work may be lost.',
      _ =>
        'Students get a 15 second warning, then the computers shut down. They must be switched on by hand afterwards.',
    };
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        icon: Icon(action.icon, color: const Color(0xffb83b40)),
        title: Text('${action.label.split(' ').first} $audience?'),
        content: Text(detail),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: const Color(0xffb83b40),
            ),
            onPressed: () => Navigator.pop(context, true),
            child: Text(
              op == 'logout' ? 'Log out' : action.label.split(' ').first,
            ),
          ),
        ],
      ),
    );
    if (ok != true) return;
  }
  if (!context.mounted) return;
  final messenger = ScaffoldMessenger.maybeOf(context);
  final result = await agent.request(
    'tool_power',
    selected: ids,
    input: {'op': op, 'message': message, 'class_wide': classWide},
  );
  if (result['ok'] != true) {
    messenger?.showSnackBar(
      SnackBar(content: Text(agent.error ?? Agent.failure(result))),
    );
    return;
  }
  final jobId = result['job_id'];
  for (var i = 0; i < 30; i++) {
    await Future<void>.delayed(const Duration(milliseconds: 300));
    final jobs = ((agent.state['tools'] as Map? ?? const {})['jobs'] as List?)
        ?.whereType<Map>();
    final job = jobs?.where((j) => j['id'] == jobId).firstOrNull;
    if (job == null) continue;
    final targets = (job['targets'] as List).whereType<Map>().toList();
    if (targets.any(
      (t) => t['status'] == 'queued' || t['status'] == 'delivered',
    )) {
      continue;
    }
    final failed = targets.where((t) => t['status'] == 'error').toList();
    final names = {
      for (final s in agent.students) '${s['id']}': '${s['name']}',
    };
    messenger?.showSnackBar(
      SnackBar(
        content: Text(
          failed.isEmpty
              ? '${action.verb} ${targets.length} computer${targets.length == 1 ? '' : 's'}'
              : '${action.verb}: ${targets.length - failed.length} done · failed ${failed.map((t) => names['${t['id']}'] ?? '').take(3).join(', ')}: ${failed.first['error']}',
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
        ),
        duration: Duration(seconds: failed.isEmpty ? 2 : 6),
      ),
    );
    return;
  }
}
