import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'agent.dart';
import 'app_dialog.dart';

const _permissions = MethodChannel('classroom/permissions');

Future<void> showMacPermissions(BuildContext context, Agent agent) =>
    showDialog<void>(
      context: context,
      builder: (_) => MacPermissionsDialog(agent: agent),
    );

const _onboardingSeenKey = 'mac_permissions_onboarding_seen';

/// Waits (bounded) for the console to finish reading its settings database.
Future<void> _whenReady(Agent agent) async {
  if (agent.connected) return;
  final ready = Completer<void>();
  void check() {
    if (agent.connected && !ready.isCompleted) ready.complete();
  }

  agent.addListener(check);
  try {
    await ready.future.timeout(const Duration(seconds: 20));
  } finally {
    agent.removeListener(check);
  }
}

/// Auto-onboarding is once per installation/profile, only with two explicitly
/// missing Recording grants. Unknown helper status never creates a startup modal.
/// The "seen" marker lives in the Teacher's settings database.
Future<void> maybeShowMacPermissions(BuildContext context, Agent agent) async {
  try {
    await _whenReady(agent);
    final status = await _permissions.invokeMapMethod<String, dynamic>(
      'status',
    );
    if (status == null) return;
    Future<void> remember() async {
      try {
        await agent.storeApply('ui', {_onboardingSeenKey: true});
      } catch (_) {
        // Worst case the checklist is offered once more next launch.
      }
    }

    var seen = false;
    try {
      seen = (await agent.storeList('ui'))[_onboardingSeenKey] == true;
    } catch (_) {}
    // Builds before the database kept this in macOS preferences; carry it over.
    if (!seen && status['onboardingSeen'] == true) {
      await remember();
      seen = true;
    }
    if (seen) return;
    final anyGranted =
        status['screen'] == true || status['helperScreen'] == true;
    final noneGranted =
        status['screen'] == false && status['helperScreen'] == false;
    if (!anyGranted && !noneGranted) return;
    await remember();
    if (context.mounted && noneGranted) {
      await showMacPermissions(context, agent);
    }
  } catch (_) {
    // Startup stays usable; capture errors and the explicit toolbar remain.
  }
}

/// Only public, process-local OS checks and explicit LAN probes are authoritative.
/// Missing plugin/helper support remains unknown rather than a granted checkmark.
class MacPermissionsDialog extends StatefulWidget {
  const MacPermissionsDialog({super.key, required this.agent});
  final Agent agent;
  @override
  State<MacPermissionsDialog> createState() => _MacPermissionsDialogState();
}

class _MacPermissionsDialogState extends State<MacPermissionsDialog>
    with WidgetsBindingObserver {
  Map<String, dynamic> status = {};
  Map<String, dynamic> network = {};
  Timer? timer, networkTimer;
  bool networkRequested = false;
  bool checking = false, busy = false, restartNeeded = false;
  String? error;
  final host = TextEditingController();
  final port = TextEditingController(text: '22');

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    widget.agent.addListener(agentChanged);
    final rows = widget.agent.state['students'] as List? ?? [];
    for (final row in rows) {
      if (row is Map && row['ip'] is String) {
        host.text = row['ip'] as String;
        break;
      }
    }
    unawaited(refresh());
    timer = Timer.periodic(const Duration(seconds: 2), (_) => refresh());
    networkTimer = Timer.periodic(const Duration(seconds: 10), (_) {
      if (networkRequested && !busy) {
        unawaited(
          action('probeNetwork', {
            'host': host.text.trim(),
            'port': int.tryParse(port.text) ?? 0,
          }),
        );
      }
    });
  }

  void agentChanged() {
    if (mounted) setState(() {});
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) unawaited(refresh());
  }

  Future<void> refresh() async {
    if (checking || busy) return;
    checking = true;
    try {
      final next = await _permissions.invokeMapMethod<String, dynamic>(
        'status',
      );
      if (!mounted || next == null) return;
      setState(() {
        if ((status['screen'] == false && next['screen'] == true) ||
            (status['helperScreen'] == false && next['helperScreen'] == true)) {
          restartNeeded = true;
        }
        status = next;
        error = next['helperError'] as String?;
      });
    } catch (e) {
      if (mounted) setState(() => error = 'Không kiểm tra được quyền: $e');
    } finally {
      checking = false;
    }
  }

  Future<void> action(String method, [dynamic arguments]) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final result = await _permissions.invokeMethod<dynamic>(
        method,
        arguments,
      );
      if (mounted && method == 'probeNetwork' && result is Map) {
        setState(() => network = Map<String, dynamic>.from(result));
      }
      if (mounted && result == false && method.startsWith('open')) {
        setState(
          () => error = 'Hãy tự mở System Settings → Privacy & Security.',
        );
      }
      if (mounted && result is Map && result['error'] is String) {
        setState(() => error = result['error'] as String);
      }
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
      await refresh();
    }
  }

  Widget permission(
    String title,
    String reason,
    bool? granted,
    List<Widget> buttons, {
    String? verifiedLabel,
  }) {
    final label = granted == true
        ? verifiedLabel ?? 'Đã cấp'
        : granted == false
        ? 'Chưa cấp'
        : 'Chưa xác minh';
    final color = granted == true
        ? const Color(0xff1f7a4d)
        : granted == false
        ? const Color(0xffb83b40)
        : const Color(0xffc77d12);
    return Container(
      margin: const EdgeInsets.only(bottom: 10),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        border: Border.all(color: AppDialog.border),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                granted == true ? Icons.check_circle : Icons.info_outline,
                color: color,
                size: 22,
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  title,
                  style: const TextStyle(
                    fontWeight: FontWeight.w700,
                    fontSize: 13.5,
                  ),
                ),
              ),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 3),
                decoration: BoxDecoration(
                  color: color.withValues(alpha: 0.12),
                  borderRadius: BorderRadius.circular(10),
                ),
                child: Text(
                  label,
                  style: TextStyle(
                    fontSize: 11.5,
                    fontWeight: FontWeight.w600,
                    color: color,
                  ),
                ),
              ),
            ],
          ),
          Padding(
            padding: const EdgeInsets.only(left: 32, top: 6),
            child: Text(
              reason,
              style: const TextStyle(
                fontSize: 12.5,
                height: 1.45,
                color: AppDialog.muted,
              ),
            ),
          ),
          if (buttons.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(left: 24, top: 4),
              child: Wrap(spacing: 8, children: buttons),
            ),
        ],
      ),
    );
  }

  Widget button(String title, String method) => TextButton(
    onPressed: busy ? null : () => action(method),
    child: Text(title),
  );

  Widget note(String text) => Padding(
    padding: const EdgeInsets.only(bottom: 10),
    child: Text(
      text,
      style: const TextStyle(
        fontSize: 12.5,
        height: 1.45,
        color: AppDialog.muted,
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    final consoleNetwork = network['console'] as Map?;
    final helperNetwork = network['helper'] as Map?;
    final networkOK =
        consoleNetwork?['state'] == 'connected' &&
        helperNetwork?['state'] == 'connected';
    final idle =
        widget.agent.connected && widget.agent.state['phase'] == 'idle';
    return AppDialog(
      icon: Icons.privacy_tip_outlined,
      title: 'Thiết lập quyền macOS',
      subtitle:
          'macOS sẽ hỏi bạn cho phép. Chọn Allow hoặc bật Classroom trong Settings. Trạng thái tự làm mới mỗi 2 giây khi cửa sổ này đang mở.',
      width: 700,
      footerLeading: Align(
        alignment: Alignment.centerLeft,
        child: TextButton.icon(
          onPressed: busy ? null : refresh,
          icon: const Icon(Icons.refresh, size: 18),
          label: const Text('Làm mới'),
        ),
      ),
      actions: [
        AppDialog.secondary(
          icon: Icons.restart_alt,
          label: 'Khởi động lại Teacher',
          onPressed: idle && !busy
              ? () async {
                  setState(() => busy = true);
                  try {
                    await widget.agent.commandChecked(
                      'restart',
                      const [],
                      const {},
                    );
                    await _permissions.invokeMethod<bool>(
                      'restartConsole',
                      widget.agent.args,
                    );
                    if (context.mounted) Navigator.pop(context);
                  } catch (e) {
                    if (mounted) {
                      setState(() {
                        busy = false;
                        error = 'Không khởi động lại được Teacher: $e';
                      });
                    }
                  }
                }
              : null,
        ),
        AppDialog.primary(
          onPressed: () => Navigator.pop(context),
          icon: Icons.arrow_forward,
          label: 'Tiếp tục',
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (restartNeeded) ...[
            const InlineNotice(
              'Phát hiện quyền Screen Recording mới. Hãy khởi động lại Teacher để áp dụng.',
              tone: AppTone.warning,
            ),
            const SizedBox(height: 12),
          ],
          permission(
            'Screen Recording · Console',
            'Cần để hiện màn hình giảng viên cho cả lớp. Classroom không cần âm thanh hệ thống.',
            status['screen'] as bool?,
            [button('Yêu cầu / Mở Settings', 'openScreenRecordingSettings')],
          ),
          permission(
            'Screen Recording · Agent',
            'Tiến trình media đi kèm là bên thực sự chụp màn hình. Quyền của nó được kiểm tra riêng.',
            status['helperScreen'] as bool?,
            [
              button('Yêu cầu quyền cho Agent', 'requestHelperScreen'),
              button('Mở Screen Recording', 'openScreenRecordingSettings'),
            ],
          ),
          permission(
            'Mạng nội bộ (Local Network)',
            'Cần để kết nối tới máy học viên. macOS không có API chung cho trạng thái quyền này. Màu xanh nghĩa là cả Console và Agent đã kết nối được tới điểm thử; chưa xác minh mọi cổng của lớp học.',
            networkOK ? true : null,
            [button('Mở Local Network', 'openLocalNetworkSettings')],
            verifiedLabel: 'Đã kết nối (lần thử gần nhất)',
          ),
          DialogSection(
            title: 'Thử kết nối mạng nội bộ',
            hint:
                'Thử một dịch vụ TCP đã biết trên máy học viên. Cổng 22 cần SSH. Sau lần thử đầu, mạng được kiểm tra lại mỗi 10 giây khi cửa sổ này mở. Thử thất bại có thể do dịch vụ tắt, tường lửa hoặc mạng.',
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: TextField(
                        controller: host,
                        onChanged: (_) => setState(() {
                          network = {};
                          networkRequested = false;
                        }),
                        enabled: !busy,
                        decoration: const InputDecoration(
                          labelText: 'Địa chỉ IPv4 trong mạng LAN',
                          isDense: true,
                        ),
                      ),
                    ),
                    const SizedBox(width: 12),
                    SizedBox(
                      width: 110,
                      child: TextField(
                        controller: port,
                        onChanged: (_) => setState(() {
                          network = {};
                          networkRequested = false;
                        }),
                        enabled: !busy,
                        keyboardType: TextInputType.number,
                        decoration: const InputDecoration(
                          labelText: 'Cổng TCP',
                          isDense: true,
                        ),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                TextButton.icon(
                  onPressed: busy
                      ? null
                      : () {
                          setState(() {
                            network = {};
                            networkRequested = true;
                          });
                          action('probeNetwork', {
                            'host': host.text.trim(),
                            'port': int.tryParse(port.text) ?? 0,
                          });
                        },
                  icon: const Icon(Icons.network_check),
                  label: Text(
                    busy ? 'Đang xử lý…' : 'Yêu cầu / Thử truy cập LAN',
                  ),
                ),
                if (consoleNetwork != null)
                  Text(
                    'Console: ${consoleNetwork['state']} ${consoleNetwork['detail'] ?? ''}',
                    style: const TextStyle(fontSize: 12.5),
                  ),
                if (helperNetwork != null)
                  Text(
                    'Agent: ${helperNetwork['state']} ${helperNetwork['detail'] ?? ''}',
                    style: const TextStyle(fontSize: 12.5),
                  ),
              ],
            ),
          ),
          const Divider(),
          const SizedBox(height: 6),
          note(
            'Accessibility: Teacher không cần quyền này để chiếu màn hình hay điều khiển học viên Windows. Máy học viên Mac cần quyền Accessibility riêng để nhận bàn phím/chuột từ xa.',
          ),
          note(
            'Tệp: Classroom không xin Full Disk Access. Hãy chọn tệp và thư mục qua hộp thoại hệ thống khi truyền hoặc lưu.',
          ),
          note(
            'Sau khi đổi quyền Screen Recording, hãy khởi động lại Teacher. “Tiếp tục” chỉ bỏ qua cửa sổ này, không vượt qua quyền của macOS.',
          ),
          if (error != null) ...[
            const SizedBox(height: 4),
            InlineNotice(error!),
          ],
          ExpansionTile(
            tilePadding: EdgeInsets.zero,
            title: const Text(
              'Ứng dụng cần cấp quyền thủ công',
              style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
            ),
            children: [
              SelectableText(
                'Console: ${status['appPath'] ?? 'Đang kiểm tra…'}\nAgent: ${status['helperPath'] ?? 'Đang kiểm tra…'}',
                style: const TextStyle(fontSize: 12.5),
              ),
              button('Hiện ứng dụng trong Finder', 'revealApps'),
              note(
                'Ứng dụng này không xóa mục cũ hay đặt lại quyết định về quyền riêng tư. Hãy kiểm tra đường dẫn đầy đủ trước khi sửa một mục trong System Settings.',
              ),
            ],
          ),
        ],
      ),
    );
  }

  @override
  void dispose() {
    timer?.cancel();
    networkTimer?.cancel();
    widget.agent.removeListener(agentChanged);
    WidgetsBinding.instance.removeObserver(this);
    host.dispose();
    port.dispose();
    super.dispose();
  }
}
