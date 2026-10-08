import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'agent.dart';

const _permissions = MethodChannel('classroom/permissions');

Future<void> showMacPermissions(BuildContext context, Agent agent) =>
    showDialog<void>(
      context: context,
      builder: (_) => MacPermissionsDialog(agent: agent),
    );

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
      if (mounted) setState(() => error = 'Could not check permissions: $e');
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
          () => error = 'Open System Settings → Privacy & Security manually.',
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
        ? verifiedLabel ?? 'Granted'
        : granted == false
        ? 'Not granted'
        : 'Not verified';
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                granted == true ? Icons.check_circle : Icons.info_outline,
                color: granted == true
                    ? Colors.green.shade700
                    : Colors.orange.shade800,
                size: 22,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  title,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
              ),
              Text(label),
            ],
          ),
          Padding(
            padding: const EdgeInsets.only(left: 30, top: 5),
            child: Text(reason),
          ),
          if (buttons.isNotEmpty) Wrap(spacing: 8, children: buttons),
        ],
      ),
    );
  }

  Widget button(String title, String method) => TextButton(
    onPressed: busy ? null : () => action(method),
    child: Text(title),
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
    return AlertDialog(
      title: const Text('Set up macOS permissions'),
      content: SizedBox(
        width: 640,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'macOS asks you to approve access. Choose Allow or enable Classroom in Settings. Checks refresh every 2 seconds while this window is open.',
              ),
              permission(
                'Screen Recording · Console',
                'Needed to show the Teacher screen to the class. Classroom does not need system audio.',
                status['screen'] as bool?,
                [
                  button(
                    'Request / Open Settings',
                    'openScreenRecordingSettings',
                  ),
                ],
              ),
              permission(
                'Screen Recording · Agent',
                'The bundled media helper captures the screen. Its permission is checked separately.',
                status['helperScreen'] as bool?,
                [
                  button('Request Agent access', 'requestHelperScreen'),
                  button(
                    'Open Screen Recording',
                    'openScreenRecordingSettings',
                  ),
                ],
              ),
              const Divider(),
              permission(
                'Local Network',
                'Needed to connect to classroom computers. macOS has no general permission-status API. Green here means both Console and Agent connected to the test endpoint; it does not verify every classroom port.',
                networkOK ? true : null,
                [button('Open Local Network', 'openLocalNetworkSettings')],
                verifiedLabel: 'Connected (last test)',
              ),
              const Text(
                'Test a known TCP service on a classroom computer. Port 22 requires SSH. After the first test, LAN checks repeat every 10 seconds while this window is open. A failed test may mean an offline service, firewall or network issue.',
              ),
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
                        labelText: 'LAN IPv4 address',
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  SizedBox(
                    width: 90,
                    child: TextField(
                      controller: port,
                      onChanged: (_) => setState(() {
                        network = {};
                        networkRequested = false;
                      }),
                      enabled: !busy,
                      keyboardType: TextInputType.number,
                      decoration: const InputDecoration(labelText: 'TCP port'),
                    ),
                  ),
                ],
              ),
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
                label: Text(busy ? 'Working…' : 'Request / Test LAN access'),
              ),
              if (consoleNetwork != null)
                Text(
                  'Console: ${consoleNetwork['state']} ${consoleNetwork['detail'] ?? ''}',
                ),
              if (helperNetwork != null)
                Text(
                  'Agent: ${helperNetwork['state']} ${helperNetwork['detail'] ?? ''}',
                ),
              const Divider(),
              const Text(
                'Accessibility: not required for this Teacher to share its screen or control a Windows Student. A Mac Student needs its own Accessibility permission to receive remote keyboard/mouse input.',
              ),
              const SizedBox(height: 10),
              const Text(
                'Files: no Full Disk Access is requested. Select files and folders using the system picker when transferring or saving them.',
              ),
              const SizedBox(height: 10),
              const Text(
                'After changing Screen Recording, restart Teacher. Continuing skips this setup window; it does not bypass macOS permissions.',
              ),
              if (restartNeeded)
                const Text(
                  'New Screen Recording grant detected. Restart Teacher to apply it.',
                  style: TextStyle(fontWeight: FontWeight.w600),
                ),
              if (error != null)
                Text(
                  error!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ExpansionTile(
                title: const Text('Exact apps for manual setup'),
                children: [
                  SelectableText(
                    'Console: ${status['appPath'] ?? 'Checking…'}\nAgent: ${status['helperPath'] ?? 'Checking…'}',
                  ),
                  button('Show apps in Finder', 'revealApps'),
                  const Text(
                    'This app does not remove old entries or reset privacy decisions. Check the full path before changing an entry in System Settings.',
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: busy ? null : refresh,
          child: const Text('Refresh'),
        ),
        TextButton(
          onPressed: idle && !busy
              ? () async {
                  setState(() => busy = true);
                  try {
                    await widget.agent.commandChecked(
                      'restart',
                      const [],
                      const {},
                    );
                    await _permissions.invokeMethod<bool>('restartConsole');
                    if (context.mounted) Navigator.pop(context);
                  } catch (e) {
                    if (mounted) {
                      setState(() {
                        busy = false;
                        error = 'Could not restart Teacher: $e';
                      });
                    }
                  }
                }
              : null,
          child: const Text('Restart Teacher'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Continue'),
        ),
      ],
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
