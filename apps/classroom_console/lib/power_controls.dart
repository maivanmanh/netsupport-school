import 'dart:async';
import 'package:flutter/material.dart';
import 'agent.dart';
import 'app_dialog.dart';

/// One entry of the class-wide "Power & screen" menu.
class PowerAction {
  const PowerAction(this.op, this.label, this.icon, this.verb);
  final String op, label, verb;
  final IconData icon;
}

const lockAction = PowerAction(
  'lock',
  'Khóa màn hình',
  Icons.lock_outline,
  'Đã khóa',
);
const unlockAction = PowerAction(
  'unlock',
  'Mở khóa màn hình',
  Icons.lock_open_outlined,
  'Đã mở khóa',
);
const logoutAction = PowerAction(
  'logout',
  'Đăng xuất người dùng',
  Icons.logout,
  'Đang đăng xuất',
);
const rebootAction = PowerAction(
  'reboot',
  'Khởi động lại máy',
  Icons.restart_alt,
  'Đang khởi động lại',
);
const shutdownAction = PowerAction(
  'shutdown',
  'Tắt máy',
  Icons.power_settings_new,
  'Đang tắt máy',
);
const clipboardBlockAction = PowerAction(
  'clipboard_block',
  'Chặn clipboard (chế độ thi)',
  Icons.content_paste_off,
  'Đã chặn clipboard',
);
const clipboardAllowAction = PowerAction(
  'clipboard_allow',
  'Cho phép clipboard',
  Icons.content_paste,
  'Đã cho phép clipboard',
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
      title: Text('Khóa màn hình…'),
    ),
  ),
  PopupMenuItem(
    value: 'unlock',
    child: ListTile(
      dense: true,
      leading: const Icon(Icons.lock_open_outlined),
      title: const Text('Mở khóa màn hình'),
      subtitle: anyLocked ? null : const Text('Hiện không máy nào bị khóa'),
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
        anyClipboardBlocked
            ? 'Cho phép clipboard'
            : 'Chặn clipboard (chế độ thi)',
      ),
      subtitle: Text(
        anyClipboardBlocked
            ? 'Clipboard đang bị chặn'
            : 'Xóa nội dung đã sao chép trên mọi ứng dụng — ngăn chép bài',
      ),
    ),
  ),
  const PopupMenuDivider(),
  const PopupMenuItem(
    value: 'logout',
    child: ListTile(
      dense: true,
      leading: Icon(Icons.logout),
      title: Text('Đăng xuất người dùng…'),
    ),
  ),
  const PopupMenuItem(
    value: 'reboot',
    child: ListTile(
      dense: true,
      leading: Icon(Icons.restart_alt),
      title: Text('Khởi động lại…'),
    ),
  ),
  const PopupMenuItem(
    value: 'shutdown',
    child: ListTile(
      dense: true,
      leading: Icon(Icons.power_settings_new, color: Color(0xffb83b40)),
      title: Text('Tắt máy…'),
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
    final controller = TextEditingController(text: 'Hãy chú ý lên giảng viên.');
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AppDialog(
        icon: Icons.lock_outline,
        tone: AppTone.warning,
        title: 'Khóa $audience',
        subtitle: 'Màn hình bị khóa cho tới khi bạn mở khóa.',
        width: 480,
        onClose: () => Navigator.pop(context, false),
        actions: [
          AppDialog.cancel(
            context,
            onPressed: () => Navigator.pop(context, false),
          ),
          AppDialog.primary(
            onPressed: () => Navigator.pop(context, true),
            icon: Icons.lock_outline,
            label: 'Khóa',
          ),
        ],
        child: TextField(
          controller: controller,
          autofocus: true,
          maxLength: 200,
          maxLines: 2,
          decoration: const InputDecoration(
            labelText: 'Thông báo hiện trên màn hình bị khóa',
          ),
        ),
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
        'Học viên bị đăng xuất ngay lập tức. Công việc chưa lưu có thể bị mất.',
      'reboot' =>
        'Học viên nhận cảnh báo 15 giây rồi máy khởi động lại. Công việc chưa lưu có thể bị mất.',
      _ =>
        'Học viên nhận cảnh báo 15 giây rồi máy tắt. Sau đó phải bật máy bằng tay.',
    };
    final name = switch (op) {
      'logout' => 'Đăng xuất',
      'reboot' => 'Khởi động lại',
      _ => 'Tắt máy',
    };
    final ok = await confirmDialog(
      context,
      title: '$name $audience?',
      message: detail,
      confirmLabel: name,
      icon: action.icon,
      destructive: true,
    );
    if (!ok) return;
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
              ? '${action.verb} ${targets.length} máy'
              : '${action.verb}: ${targets.length - failed.length} máy xong · lỗi ${failed.map((t) => names['${t['id']}'] ?? '').take(3).join(', ')}: ${failed.first['error']}',
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
        ),
        duration: Duration(seconds: failed.isEmpty ? 2 : 6),
      ),
    );
    return;
  }
}
